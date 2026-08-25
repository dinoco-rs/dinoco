use dinoco::{
    AtomicUpdateError, DatabaseConstraintError, Entity, Transaction, TransactionError, Transcation, count, delete,
    delete_many, find_and_update, find_first, find_many, insert_into, insert_many, transaction, transactions, update,
    update_many,
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

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_business")]
pub struct TransactionBusiness {
    #[dinoco(primary_key)]
    id: String,
    name: String,
    enabled: bool,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "business_id",
        join_child_field = "system_id"
    )]
    systems: Vec<TransactionSystem>,

    #[dinoco(
        many_to_many_key,
        join_table = "_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "business_id",
        join_child_field = "system_id"
    )]
    system_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_system")]
pub struct TransactionSystem {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "business_id"
    )]
    businesses: Vec<TransactionBusiness>,

    #[dinoco(
        many_to_many_key,
        join_table = "_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "business_id"
    )]
    business_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_auto_endpoint")]
pub struct TransactionAutoEndpoint {
    #[dinoco(primary_key, auto_generate = autoincrement)]
    id: i64,
    name: String,

    #[dinoco(
        many_to_many_key,
        join_table = "_transaction_auto_endpoint_to_system",
        parent_field = "id",
        join_parent_field = "endpoint_id",
        join_child_field = "system_id"
    )]
    system_id: Option<String>,
}

#[tokio::test]
async fn transaction_batch_commits_in_order_and_returns_typed_results() -> anyhow::Result<()> {
    let (client, path) = client("commit").await?;
    let original = Account::new("account-1".to_string(), "first@dinoco.rs".to_string());
    insert_into::<Account>().values(&original).execute(&client).await?;

    let session = AccountSession::new("session-1".to_string(), "account-1".to_string());
    let mut batch: Transcation = Transaction::new();
    batch.push(
        find_first::<Account>()
            .where_(|account| account.id.eq("ignored-before"))
            .where_complex(|account, m| {
                m.and([account.id.eq("account-1"), m.not(account.email.eq("blocked@dinoco.rs"))])
            })
            .where_(|account| account.id.eq("ignored-after")),
    );
    batch.push(find_first::<AccountSession>().where_(|session| session.id.eq("session-1")));
    batch.push(insert_into::<AccountSession>().values(&session));
    batch.push(find_first::<AccountSession>().where_(|session| session.id.eq("session-1")));
    batch.push(count::<Account>());

    let mut results = transactions(batch).execute(&client).await?;
    assert_eq!(results.len(), 5);
    assert_eq!(results.take::<Option<Account>>(0)?.expect("original account").email, "first@dinoco.rs");
    assert!(results.take::<Option<AccountSession>>(1)?.is_none());
    results.take::<()>(2)?;
    assert_eq!(results.take::<Option<AccountSession>>(3)?.expect("inserted session").account_id, "account-1");
    assert_eq!(results.take::<AccountCount>(4)?.total, 1);

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_batch_rolls_back_every_write_after_an_error() -> anyhow::Result<()> {
    let (client, path) = client("rollback").await?;
    let first = Account::new("account-1".to_string(), "same@dinoco.rs".to_string());
    let duplicate = Account::new("account-2".to_string(), "same@dinoco.rs".to_string());
    let batch = transaction![insert_into::<Account>().values(&first), insert_into::<Account>().values(&duplicate),];

    let error = transactions(batch).execute(&client).await.expect_err("duplicate email must fail");
    assert!(error.to_string().to_lowercase().contains("unique"));
    assert!(find_first::<Account>().where_(|account| account.id.eq("account-1")).execute(&client).await?.is_none());

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
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

#[tokio::test]
async fn transaction_batch_supports_returning_updates_and_deletes() -> anyhow::Result<()> {
    let (client, path) = client("returning").await?;
    let account = Account::new("account-1".to_string(), "first@dinoco.rs".to_string());
    let batch = transaction![
        insert_into::<Account>().values(&account).returning::<Account>(),
        update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("updated@dinoco.rs"))
            .returning::<Account>(),
        find_and_update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("final@dinoco.rs")),
        delete::<Account>().where_(|item| item.id.eq("account-1")).returning::<Account>(),
        count::<Account>(),
    ];

    let mut results = transactions(batch).execute(&client).await?;
    assert_eq!(results.take::<Account>(0)?.email, "first@dinoco.rs");
    assert_eq!(results.take::<Vec<Account>>(1)?[0].email, "updated@dinoco.rs");
    assert_eq!(results.take::<Account>(2)?.email, "final@dinoco.rs");
    assert_eq!(results.take::<Vec<Account>>(3)?[0].email, "final@dinoco.rs");
    assert_eq!(results.take::<AccountCount>(4)?.total, 0);

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_batch_connects_and_disconnects_many_to_many_relations() -> anyhow::Result<()> {
    let (client, path) = client("many-to-many-commit").await?;
    let first = TransactionBusiness::new("business-1".to_string(), "First".to_string(), true);
    let second = TransactionBusiness::new("business-2".to_string(), "Second".to_string(), true);
    let erp = TransactionSystem::new("system-erp".to_string(), "ERP".to_string());
    let crm = TransactionSystem::new("system-crm".to_string(), "CRM".to_string());
    insert_many::<TransactionBusiness>().values([&first, &second]).execute(&client).await?;
    insert_many::<TransactionSystem>().values([&erp, &crm]).execute(&client).await?;

    let batch = transaction![
        update_many::<TransactionBusiness>()
            .where_(|item| item.id.batch(vec![first.id.clone(), second.id.clone()]))
            .update(|item| item.system_id.connect(&erp.id))
            .update(|item| item.enabled.set(false))
            .returning::<TransactionBusiness>(),
        find_and_update::<TransactionBusiness>()
            .where_(|item| item.id.eq(&first.id))
            .update(|item| item.system_id.connect(&crm.id))
            .update(|item| item.name.set("First updated".to_string())),
    ];
    let mut results = transactions(batch).execute(&client).await?;
    let updated = results.take::<Vec<TransactionBusiness>>(0)?;
    assert_eq!(updated.len(), 2);
    assert!(updated.iter().all(|business| !business.enabled));
    assert_eq!(results.take::<TransactionBusiness>(1)?.name, "First updated");

    let mut businesses = find_many::<TransactionBusiness>()
        .includes(|item| item.systems())
        .order_by(|item| item.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(businesses.remove(0).systems.len(), 2);
    assert_eq!(businesses.remove(0).systems.len(), 1);

    let batch = transaction![
        update::<TransactionBusiness>()
            .where_(|item| item.id.eq(&first.id))
            .update(|item| item.system_id.disconnect(&erp.id)),
        find_and_update::<TransactionBusiness>()
            .where_(|item| item.id.eq(&first.id))
            .update(|item| item.system_id.disconnect(&crm.id)),
    ];
    let mut results = transactions(batch).execute(&client).await?;
    results.take::<()>(0)?;
    assert_eq!(results.take::<TransactionBusiness>(1)?.id, first.id);

    let businesses = find_many::<TransactionBusiness>()
        .includes(|item| item.systems())
        .order_by(|item| item.id.asc())
        .execute(&client)
        .await?;
    assert!(businesses[0].systems.is_empty());
    assert_eq!(businesses[1].systems.len(), 1);

    let mut finance = TransactionSystem::new("system-finance".to_string(), "Finance".to_string());
    finance.business_id = Some(second.id.clone());
    let mut inserted_systems = vec![
        TransactionSystem::new("system-bi".to_string(), "BI".to_string()),
        TransactionSystem::new("system-support".to_string(), "Support".to_string()),
    ];
    for system in &mut inserted_systems {
        system.business_id = Some(first.id.clone());
    }
    let batch = transaction![
        insert_into::<TransactionSystem>().values(&finance).returning::<TransactionSystem>(),
        insert_many::<TransactionSystem>().values(&inserted_systems).returning::<TransactionSystem>(),
    ];
    let mut results = transactions(batch).execute(&client).await?;
    assert!(results.take::<TransactionSystem>(0)?.business_id.is_none());
    assert!(results.take::<Vec<TransactionSystem>>(1)?.iter().all(|system| system.business_id.is_none()));

    let businesses = find_many::<TransactionBusiness>()
        .includes(|item| item.systems())
        .order_by(|item| item.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(businesses[0].systems.len(), 2);
    assert_eq!(businesses[1].systems.len(), 2);

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_batch_rolls_back_many_to_many_connects() -> anyhow::Result<()> {
    let (client, path) = client("many-to-many-rollback").await?;
    let business = TransactionBusiness::new("business-1".to_string(), "Before".to_string(), true);
    let system = TransactionSystem::new("system-1".to_string(), "ERP".to_string());
    insert_into::<TransactionBusiness>().values(&business).execute(&client).await?;
    insert_into::<TransactionSystem>().values(&system).execute(&client).await?;

    let error = transactions(transaction![
        update::<TransactionBusiness>()
            .where_(|item| item.id.eq(&business.id))
            .update(|item| item.name.set("After".to_string())),
        update::<TransactionBusiness>()
            .where_(|item| item.id.eq(&business.id))
            .update(|item| item.system_id.connect(&system.id)),
        update::<TransactionBusiness>()
            .where_(|item| item.id.eq(&business.id))
            .update(|item| item.system_id.connect(&system.id)),
    ])
    .execute(&client)
    .await
    .expect_err("duplicate pivot row must roll back the complete transaction");
    assert!(error.to_string().to_lowercase().contains("unique"));

    let loaded = find_many::<TransactionBusiness>()
        .where_(|item| item.id.eq(&business.id))
        .includes(|item| item.systems())
        .execute(&client)
        .await?;
    assert_eq!(loaded[0].name, "Before");
    assert!(loaded[0].systems.is_empty());

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_insert_rejects_virtual_connection_with_autoincrement_parent_id() -> anyhow::Result<()> {
    let (client, path) = client("many-to-many-autoincrement").await?;
    let mut endpoint = TransactionAutoEndpoint::new("Generated later".to_string());
    endpoint.system_id = Some("system-1".to_string());

    let error = transactions(transaction![insert_into::<TransactionAutoEndpoint>().values(&endpoint)])
        .execute(&client)
        .await
        .expect_err("the pivot cannot be built before an autoincrement parent ID exists");
    assert!(error.to_string().contains("autoincrement"));

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
    create_table(
        &adapter,
        "transaction_business",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("name", MigrationColumnType::String),
            column("enabled", MigrationColumnType::Boolean),
        ],
    )
    .await?;
    create_table(
        &adapter,
        "transaction_system",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        &adapter,
        "_transaction_business_to_system",
        vec![
            primary(column("business_id", MigrationColumnType::String)),
            primary(column("system_id", MigrationColumnType::String)),
        ],
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

    let batch = transaction![find_first::<Account>()];
    assert_send(transactions(batch).execute(client));
}
