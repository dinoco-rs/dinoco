use dinoco::{Entity, find_first, find_many, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, MigrationDefault, SqliteAdapter};
use dinoco_tests::{column, create_table, default, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "crud_account")]
pub struct Account {
    #[dinoco(auto_generate = uuid)]
    id: String,
    #[dinoco(fulltext)]
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

    let account =
        find_first::<Account>().where_(|x| x.email.fulltext("one@dinoco")).execute(&client).await?.expect("account");
    assert_eq!(account.email, "one@dinoco.rs");

    let account = find_first::<Account>()
        .where_(|x| x.email.eq("ignored-before@dinoco.rs"))
        .where_complex(|x, m| {
            m.or(
                m.and([x.email.eq("one@dinoco.rs"), x.locked.eq(true)]),
                m.and([x.email.eq("missing@dinoco.rs"), x.locked.eq(false)]),
            )
        })
        .where_(|x| x.email.eq("ignored-after@dinoco.rs"))
        .execute(&client)
        .await?
        .expect("complex account");
    assert_eq!(account.email, "one@dinoco.rs");

    let accounts = find_many::<Account>()
        .where_(|x| x.email.eq("ignored-before@dinoco.rs"))
        .where_complex(|x, m| m.or_many([x.email.eq("one@dinoco.rs"), x.email.eq("two@dinoco.rs")]))
        .where_(|x| x.email.eq("ignored-after@dinoco.rs"))
        .execute(&client)
        .await?;
    assert_eq!(accounts.len(), 2);

    let updated = dinoco::find_and_update::<Account>()
        .where_(|x| x.email.eq("ignored-before@dinoco.rs"))
        .where_complex(|x, m| m.and([x.email.eq("two@dinoco.rs"), m.not(x.locked.eq(true))]))
        .where_(|x| x.email.eq("ignored-after@dinoco.rs"))
        .update(|x| x.locked.set(true))
        .execute(&client)
        .await?;
    assert_eq!(updated.email, "two@dinoco.rs");
    assert!(updated.locked);

    dinoco::delete_many::<Account>().where_(|x| x.locked.eq(true)).execute(&client).await?;
    assert_eq!(dinoco::count::<Account>().execute(&client).await?.total, 0);

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
