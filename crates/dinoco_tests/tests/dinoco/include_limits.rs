use dinoco::{Entity, find_many, insert_into};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "limit_user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<Token>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "limit_token")]
pub struct Token {
    #[dinoco(auto_generate = uuid)]
    id: String,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[tokio::test]
async fn include_take_uses_per_parent_window_limit() -> anyhow::Result<()> {
    let (client, path) = client("include-limits").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(
        adapter,
        "limit_user",
        vec![primary(column("id", MigrationColumnType::String)), column("email", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "limit_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    for idx in 0..2 {
        let mut user = User::new(format!("limit-{idx}@dinoco.rs"));
        user.tokens = vec![Token::new(), Token::new(), Token::new()];
        insert_into::<User>().values(&user).execute(&client).await?;
    }

    let users = find_many::<User>().includes(|x| x.tokens().take(2)).execute(&client).await?;

    assert_eq!(users.len(), 2);
    assert!(users.iter().all(|user| user.tokens.len() == 2));

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
