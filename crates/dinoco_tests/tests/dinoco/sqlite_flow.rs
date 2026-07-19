use dinoco::{Entity, find_first, find_many, insert_into, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, MigrationDefault, SqliteAdapter};
use dinoco_tests::{column, create_table, default, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "flow_user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,
    office: String,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<UserToken>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "flow_user_token")]
pub struct UserToken {
    #[dinoco(auto_generate = uuid)]
    id: String,
    #[dinoco(default = false)]
    is_expired: bool,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[tokio::test]
async fn sqlite_flow_covers_find_insert_update_delete_and_count() -> anyhow::Result<()> {
    let (client, path) = client("sqlite-flow").await?;
    create_user_tables(&client).await?;

    let mut user = User::new("a@dinoco.rs".to_string(), "admin".to_string());
    user.tokens = vec![UserToken::new(), UserToken::new()];
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>().includes(|x| x.tokens()).execute(&client).await?;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].tokens.len(), 2);

    dinoco::update::<User>()
        .where_(|x| x.email.eq("a@dinoco.rs"))
        .update(|x| x.email.set("b@dinoco.rs".to_string()))
        .execute(&client)
        .await?;

    let updated = find_first::<User>().where_(|x| x.email.eq("b@dinoco.rs")).execute(&client).await?.expect("updated");
    assert_eq!(updated.office, "admin");

    let updated = dinoco::find_and_update::<User>()
        .where_(|x| x.email.eq("b@dinoco.rs"))
        .update(|x| x.office.set("member".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.office, "member");

    let count = dinoco::count::<User>().includes(|x| x.tokens()).execute(&client).await?;
    assert_eq!(count.total, 1);
    assert_eq!(count.tokens, Some(2));

    insert_many::<User>()
        .values(vec![
            User::new("many-a@dinoco.rs".to_string(), "admin".to_string()),
            User::new("many-b@dinoco.rs".to_string(), "admin".to_string()),
        ])
        .execute(&client)
        .await?;

    dinoco::update_many::<User>()
        .where_(|x| x.office.eq("admin"))
        .update(|x| x.office.set("owner".to_string()))
        .execute(&client)
        .await?;
    let owners = find_many::<User>().where_(|x| x.office.eq("owner")).execute(&client).await?;
    assert_eq!(owners.len(), 2);

    dinoco::delete_many::<UserToken>().where_(|x| x.user_id.not_null()).execute(&client).await?;
    assert_eq!(dinoco::count::<UserToken>().execute(&client).await?.total, 0);

    dinoco::delete::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?;
    assert!(find_first::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?.is_none());

    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

async fn create_user_tables(client: &DinocoClient) -> anyhow::Result<()> {
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(
        adapter,
        "flow_user",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("email", MigrationColumnType::String),
            column("office", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "flow_user_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            default(column("is_expired", MigrationColumnType::Boolean), MigrationDefault::Boolean(false)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
