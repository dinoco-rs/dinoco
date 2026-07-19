use dinoco::{Entity, insert_into};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, MigrationDefault, SqliteAdapter};
use dinoco_tests::{column, create_table, default, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "count_user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<Token>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "count_token")]
pub struct Token {
    #[dinoco(auto_generate = uuid)]
    id: String,
    #[dinoco(default = false)]
    is_expired: bool,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[tokio::test]
async fn relation_counts_only_appear_when_included() -> anyhow::Result<()> {
    let (client, path) = client("relation-counts").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(
        adapter,
        "count_user",
        vec![primary(column("id", MigrationColumnType::String)), column("email", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "count_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            default(column("is_expired", MigrationColumnType::Boolean), MigrationDefault::Boolean(false)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    let mut user = User::new("counts@dinoco.rs".to_string());
    user.tokens = vec![Token::new(), Token::new()];
    user.tokens[0].is_expired = true;
    insert_into::<User>().values(&user).execute(&client).await?;

    let base = dinoco::count::<User>().execute(&client).await?;
    assert_eq!(base.total, 1);
    assert!(base.tokens.is_none());

    let filtered = dinoco::count::<User>()
        .includes(|x| x.tokens().where_(|token| token.is_expired.eq(false)))
        .execute(&client)
        .await?;
    assert_eq!(filtered.total, 1);
    assert_eq!(filtered.tokens.expect("token count").total, 1);

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
