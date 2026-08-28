use dinoco::{
    AtomicUpdateError, DatabaseConstraintError, Entity, TransactionError, count, delete, delete_many, find_and_update,
    find_first, find_many, insert_into, insert_many, transaction, update, update_many,
};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_account")]
pub struct Account {
    id: String,
    email: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_account_session")]
pub struct AccountSession {
    id: String,
    account_id: String,
}

#[tokio::test]
async fn transaction_closure_commits_and_rolls_back_with_typed_errors() -> anyhow::Result<()> {
    let (client, path) = client("closure-api").await?;
    let committed = Account::new("account-committed".to_string(), "committed@dinoco.rs".to_string());

    transaction(&client, |tx| async move {
        insert_into::<Account>().value(&committed).execute(tx).await?;
        update::<Account>()
            .where_(|account| account.id.eq("account-committed"))
            .update(|account| account.email.set("updated@dinoco.rs"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await?;
    assert_eq!(
        find_first::<Account>()
            .where_(|account| account.id.eq("account-committed"))
            .execute(&client)
            .await?
            .expect("committed account")
            .email,
        "updated@dinoco.rs"
    );

    let rolled_back = Account::new("account-rolled-back".to_string(), "rollback@dinoco.rs".to_string());
    let result = transaction(&client, |tx| async move {
        insert_into::<Account>().value(&rolled_back).execute(tx).await?;
        find_and_update::<Account>()
            .where_(|account| account.id.eq("missing"))
            .update(|account| account.email.set("never@dinoco.rs"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected))));
    assert!(
        find_first::<Account>()
            .where_(|account| account.id.eq("account-rolled-back"))
            .execute(&client)
            .await?
            .is_none()
    );

    let first = Account::new("account-first".to_string(), "duplicate@dinoco.rs".to_string());
    let duplicate = Account::new("account-duplicate".to_string(), "duplicate@dinoco.rs".to_string());
    let result = transaction(&client, |tx| async move {
        insert_into::<Account>().value(&first).execute(tx).await?;
        insert_into::<Account>().value(&duplicate).execute(tx).await?;
        Ok(())
    })
    .await;
    assert!(matches!(
        result,
        Err(TransactionError::Create(dinoco::CreateError::Constraint {
            kind: DatabaseConstraintError::UniqueViolation,
            ..
        }))
    ));
    assert!(find_first::<Account>().where_(|account| account.id.eq("account-first")).execute(&client).await?.is_none());

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_closure_classifies_update_and_delete_errors_and_rolls_back() -> anyhow::Result<()> {
    let (client, path) = client("typed-mutations").await?;
    let first = Account::new("account-update-first".to_string(), "first@dinoco.rs".to_string());
    let second = Account::new("account-update-second".to_string(), "second@dinoco.rs".to_string());
    insert_many::<Account>().values([&first, &second]).execute(&client).await?;

    let update_session = AccountSession::new("session-before-update".to_string(), first.id.clone());
    let result = transaction(&client, |tx| async move {
        insert_into::<AccountSession>().value(&update_session).execute(tx).await?;
        update::<Account>()
            .where_(|account| account.id.eq("account-update-second"))
            .update(|account| account.email.set("first@dinoco.rs"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    assert!(matches!(
        result,
        Err(TransactionError::Update(dinoco::UpdateError::Constraint {
            kind: DatabaseConstraintError::UniqueViolation,
            ..
        }))
    ));
    assert!(
        find_first::<AccountSession>()
            .where_(|session| session.id.eq("session-before-update"))
            .execute(&client)
            .await?
            .is_none()
    );

    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite transaction test") };
    adapter
        .execute(
            "CREATE TRIGGER reject_transaction_account_delete BEFORE DELETE ON transaction_account BEGIN SELECT RAISE(ABORT, 'delete rejected'); END",
            &[],
        )
        .await?;

    let delete_session = AccountSession::new("session-before-delete".to_string(), first.id.clone());
    let result = transaction(&client, |tx| async move {
        insert_into::<AccountSession>().value(&delete_session).execute(tx).await?;
        delete::<Account>().where_(|account| account.id.eq("account-update-first")).execute(tx).await?;
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Delete(dinoco::DeleteError::Database(_)))));
    assert!(
        find_first::<AccountSession>()
            .where_(|session| session.id.eq("session-before-delete"))
            .execute(&client)
            .await?
            .is_none()
    );
    assert!(
        find_first::<Account>()
            .where_(|account| account.id.eq("account-update-first"))
            .execute(&client)
            .await?
            .is_some()
    );

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-transaction-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    let mut email = column("email", MigrationColumnType::String);
    email.unique = true;
    create_table(&adapter, "transaction_account", vec![primary(column("id", MigrationColumnType::String)), email])
        .await?;
    create_table(
        &adapter,
        "transaction_account_session",
        vec![primary(column("id", MigrationColumnType::String)), column("account_id", MigrationColumnType::String)],
    )
    .await?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

#[allow(dead_code)]
fn public_operation_futures_are_send(client: &DinocoClient) {
    fn assert_send<T: Send>(_: T) {}

    let account = Account::new("account-1".to_string(), "send@dinoco.rs".to_string());

    assert_send(find_first::<Account>().execute(client));
    assert_send(find_many::<Account>().execute(client));
    assert_send(count::<Account>().execute(client));
    assert_send(insert_into::<Account>().values(&account).execute(client));
    assert_send(insert_into::<Account>().values(&account).returning::<Account>().execute(client));
    assert_send(insert_many::<Account>().values([&account]).execute(client));
    assert_send(insert_many::<Account>().values([&account]).returning::<Account>().execute(client));
    assert_send(
        update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("updated@dinoco.rs"))
            .execute(client),
    );
    assert_send(
        update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("updated@dinoco.rs"))
            .returning::<Account>()
            .execute(client),
    );
    assert_send(update_many::<Account>().update(|item| item.email.set("updated@dinoco.rs")).execute(client));
    assert_send(
        update_many::<Account>()
            .update(|item| item.email.set("updated@dinoco.rs"))
            .returning::<Account>()
            .execute(client),
    );
    assert_send(
        find_and_update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("updated@dinoco.rs"))
            .execute(client),
    );
    assert_send(delete::<Account>().where_(|item| item.id.eq("account-1")).execute(client));
    assert_send(delete::<Account>().where_(|item| item.id.eq("account-1")).returning::<Account>().execute(client));
    assert_send(delete_many::<Account>().execute(client));
    assert_send(delete_many::<Account>().returning::<Account>().execute(client));
}
