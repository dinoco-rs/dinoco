use dinoco::{Entity, find_first, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, MigrationDefault, SqliteAdapter};
use dinoco_tests::{column, create_table, default, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "crud_account")]
pub struct Account {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,
    #[dinoco(default = false)]
    locked: bool,
}

#[tokio::test]
async fn sqlite_adapter_runs_crud_methods_with_new_api() -> anyhow::Result<()> {
    let (client, path) = client("adapters-crud").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(
        adapter,
        "crud_account",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("email", MigrationColumnType::String),
            default(column("locked", MigrationColumnType::Boolean), MigrationDefault::Boolean(false)),
        ],
    )
    .await?;

    insert_many::<Account>()
        .values(vec![Account::new("one@dinoco.rs".to_string()), Account::new("two@dinoco.rs".to_string())])
        .execute(&client)
        .await?;

    dinoco::update::<Account>()
        .where_(|x| x.email.eq("one@dinoco.rs"))
        .update(|x| x.locked.set(true))
        .execute(&client)
        .await?;

    let account =
        find_first::<Account>().where_(|x| x.email.eq("one@dinoco.rs")).execute(&client).await?.expect("account");
    assert!(account.locked);

    dinoco::delete_many::<Account>().where_(|x| x.locked.eq(true)).execute(&client).await?;
    assert_eq!(dinoco::count::<Account>().execute(&client).await?.total, 1);

    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
