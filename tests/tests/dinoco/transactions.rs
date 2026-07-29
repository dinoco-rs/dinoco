use dinoco::{
    Entity, Transaction, Transcation, count, delete, find_and_update, find_first, insert_into, transaction,
    transactions, update,
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
