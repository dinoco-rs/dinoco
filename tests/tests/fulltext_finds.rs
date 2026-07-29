use dinoco::{Entity, Transaction, find_and_update, find_first, find_many, insert_many, transactions};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "fulltext_find_author")]
pub struct Author {
    id: String,
    #[dinoco(fulltext)]
    name: String,

    #[dinoco(one_to_many, foreign_key = "author_id", references = "id")]
    articles: Vec<Article>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "fulltext_find_article")]
pub struct Article {
    id: String,
    #[dinoco(fulltext = "title,body")]
    title: String,
    #[dinoco(fulltext = "title,body")]
    body: String,
    author_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "author_id", references = "id")]
    author: Option<Author>,
}

#[tokio::test]
async fn fulltext_is_available_in_every_find_builder() -> anyhow::Result<()> {
    let (client, path) = client("all-find-builders").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };

    create_table(
        adapter,
        "fulltext_find_author",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "fulltext_find_article",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("title", MigrationColumnType::String),
            column("body", MigrationColumnType::String),
            nullable(column("author_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    insert_many::<Author>()
        .values(vec![
            Author::new("author-1".to_string(), "Matheus Dinoco".to_string()),
            Author::new("author-2".to_string(), "Another writer".to_string()),
        ])
        .execute(&client)
        .await?;
    let mut first_article =
        Article::new("article-1".to_string(), "Atomic Rust".to_string(), "Rust transactions with Dinoco".to_string());
    first_article.author_id = Some("author-1".to_string());
    let mut second_article =
        Article::new("article-2".to_string(), "Database note".to_string(), "An unrelated database note".to_string());
    second_article.author_id = Some("author-2".to_string());
    insert_many::<Article>().values(vec![first_article, second_article]).execute(&client).await?;

    let author = find_first::<Author>()
        .where_(|item| item.name.fulltext("Matheus"))
        .execute(&client)
        .await?
        .expect("find_first full-text match");
    assert_eq!(author.id, "author-1");

    let articles = find_many::<Article>().where_(|item| item.title.fulltext("Dinoco")).execute(&client).await?;
    assert_eq!(articles.len(), 1);
    assert_eq!(articles[0].id, "article-1");

    let articles = find_many::<Article>()
        .where_(|item| item.id.eq("ignored-before"))
        .where_complex(|item, m| m.and([item.body.fulltext("transactions"), m.not(item.body.fulltext("unrelated"))]))
        .where_(|item| item.id.eq("ignored-after"))
        .execute(&client)
        .await?;
    assert_eq!(articles.len(), 1);

    let updated = find_and_update::<Article>()
        .where_(|item| item.body.fulltext("transactions"))
        .update(|item| item.body.set("Dinoco atomic updates".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.id, "article-1");
    assert_eq!(updated.body, "Dinoco atomic updates");

    let author = find_first::<Author>()
        .where_(|item| item.id.eq("author-1"))
        .includes(|item| item.articles().where_(|article| article.body.fulltext("atomic")))
        .execute(&client)
        .await?
        .expect("author with filtered articles");
    assert_eq!(author.articles.len(), 1);
    assert_eq!(author.articles[0].id, "article-1");

    let article = find_first::<Article>()
        .where_(|item| item.id.eq("article-1"))
        .includes(|item| item.author().where_(|author| author.name.fulltext("Matheus")))
        .execute(&client)
        .await?
        .expect("article with filtered author");
    assert_eq!(article.author.expect("full-text belongs-to match").id, "author-1");

    let mut transaction = Transaction::new();
    transaction.push(find_first::<Article>().where_(|item| item.body.fulltext("atomic")));
    transaction.push(find_many::<Author>().where_(|item| item.name.fulltext("Dinoco")));
    let mut results = transactions(transaction).execute(&client).await?;
    assert_eq!(results.take::<Option<Article>>(0)?.expect("transaction find_first").id, "article-1");
    assert_eq!(results.take::<Vec<Author>>(1)?.len(), 1);

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-fulltext-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
