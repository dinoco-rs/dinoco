use dinoco::{Entity, find_first, find_many, insert_into, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, MigrationDefault, SqliteAdapter};
use dinoco_tests::{column, create_table, default, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,
    office: String,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<UserToken>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "user_token")]
pub struct UserToken {
    #[dinoco(auto_generate = uuid)]
    id: String,
    #[dinoco(default = false)]
    is_expired: bool,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "user_post")]
pub struct UserPost {
    user_id: String,
    post_id: String,
}

#[tokio::test]
async fn sqlite_crud_relations_and_count_work_end_to_end() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-test-{}.sqlite", std::process::id());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    create_table(
        &adapter,
        "user",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("email", MigrationColumnType::String),
            column("office", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        &adapter,
        "user_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            default(column("is_expired", MigrationColumnType::Boolean), MigrationDefault::Boolean(false)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(
        &adapter,
        "user_post",
        vec![column("user_id", MigrationColumnType::String), column("post_id", MigrationColumnType::String)],
    )
    .await?;

    let client = DinocoClient::new(Backend::Sqlite(adapter));

    let mut user = User::new("a@dinoco.rs".to_string(), "admin".to_string());
    user.tokens = vec![UserToken::new()];
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>().includes(|x| x.tokens()).execute(&client).await?;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].tokens.len(), 1);

    dinoco::update::<User>()
        .where_(|x| x.email.eq("a@dinoco.rs"))
        .update(|x| x.email.set("b@dinoco.rs".to_string()))
        .execute(&client)
        .await?;

    let updated =
        find_first::<User>().where_(|x| x.email.eq("b@dinoco.rs")).execute(&client).await?.expect("updated user");
    assert_eq!(updated.email, "b@dinoco.rs");

    let updated = dinoco::find_and_update::<User>()
        .where_(|x| x.email.eq("b@dinoco.rs"))
        .update(|x| x.office.set("member".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.office, "member");

    let count = dinoco::count::<User>().includes(|x| x.tokens()).execute(&client).await?;
    assert_eq!(count.total, 1);
    assert_eq!(count.tokens.expect("token count").total, 1);

    dinoco::delete_many::<UserToken>().where_(|x| x.user_id.not_null()).execute(&client).await?;
    let count = dinoco::count::<UserToken>().execute(&client).await?;
    assert_eq!(count.total, 0);

    let many_a = User::new("many-a@dinoco.rs".to_string(), "admin".to_string());
    let many_b = User::new("many-b@dinoco.rs".to_string(), "member".to_string());
    insert_many::<User>().values(vec![many_a, many_b]).execute(&client).await?;

    dinoco::update_many::<User>()
        .where_(|x| x.office.eq("admin"))
        .update(|x| x.office.set("owner".to_string()))
        .execute(&client)
        .await?;
    let owners = find_many::<User>().where_(|x| x.office.eq("owner")).execute(&client).await?;
    assert_eq!(owners.len(), 1);

    dinoco::update::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.connect("post-a"))
        .execute(&client)
        .await?;
    let pivot_count = adapter_count(&client, "SELECT COUNT(*) FROM user_post").await?;
    assert_eq!(pivot_count, 1);

    dinoco::update_many::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.disconnect("post-a"))
        .execute(&client)
        .await?;
    let pivot_count = adapter_count(&client, "SELECT COUNT(*) FROM user_post").await?;
    assert_eq!(pivot_count, 0);

    dinoco::delete::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?;
    let deleted =
        find_first::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).read_in_primary().execute(&client).await?;
    assert!(deleted.is_none());

    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn adapter_count(client: &DinocoClient, sql: &str) -> anyhow::Result<i64> {
    match &client.backend {
        Backend::Sqlite(adapter) => adapter.query_count(sql, &[]).await,
        _ => unreachable!("sqlite test"),
    }
}
