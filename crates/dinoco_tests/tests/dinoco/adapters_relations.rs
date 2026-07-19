use dinoco::{Entity, find_many, insert_into};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<Token>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_token")]
pub struct Token {
    #[dinoco(auto_generate = uuid)]
    id: String,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[tokio::test]
async fn insert_resolves_vec_relations_without_with_relation() -> anyhow::Result<()> {
    let (client, path) = client("adapters-relations").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(
        adapter,
        "relation_user",
        vec![primary(column("id", MigrationColumnType::String)), column("email", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "relation_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    let mut user = User::new("relations@dinoco.rs".to_string());
    user.tokens = vec![Token::new(), Token::new(), Token::new()];
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>().includes(|x| x.tokens()).execute(&client).await?;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].tokens.len(), 3);
    assert!(users[0].tokens.iter().all(|token| token.user_id.as_deref() == Some(users[0].id.as_str())));

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
