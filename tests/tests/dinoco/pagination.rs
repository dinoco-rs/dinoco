//! `skip` without `take`: SQLite and MySQL only accept `OFFSET` after a
//! `LIMIT`, so Dinoco writes each one's "no limit" value there. Covers
//! `find_many`, `.pluck(...)`, `.includes(...)`, aggregates over a window,
//! grouped aggregates, skipping past the end, and transactions.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::{Entity, find_first, find_many, insert_many, transaction};
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, DinocoSqlCompiler, MigrationColumnType, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 2] = ["pg_author", "pg_book"];

macro_rules! on_every_backend {
    ($($scenario:ident),* $(,)?) => {
        $(
            mod $scenario {
                #[tokio::test]
                async fn sqlite() -> anyhow::Result<()> {
                    super::run_on_sqlite(stringify!($scenario), super::$scenario).await
                }

                #[tokio::test]
                async fn postgres() -> anyhow::Result<()> {
                    super::run_on_postgres(super::$scenario).await
                }

                #[tokio::test]
                async fn mysql() -> anyhow::Result<()> {
                    super::run_on_mysql(super::$scenario).await
                }
            }
        )*
    };
}

on_every_backend!(rows, plucks_and_includes, aggregates, transactions, several_order_columns);

#[derive(Debug, Entity)]
#[dinoco(table_name = "pg_author")]
pub struct Author {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "author_id", references = "id")]
    books: Vec<Book>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "pg_book")]
pub struct Book {
    #[dinoco(primary_key)]
    id: String,
    author_id: String,
    pages: i64,

    #[dinoco(many_to_one, foreign_key = "author_id", references = "id")]
    author: Option<Author>,
}

/// Authors a1..a5; a1 wrote b1 (100 pages) and b2 (200), a2 wrote b3 (300),
/// a3 wrote b4 (400) and b5 (500).
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Author>()
        .values((1..=5).map(|index| Author { id: format!("a{index}"), name: format!("Author {index}"), books: vec![] }))
        .execute(client)
        .await?;
    insert_many::<Book>()
        .values([
            book("b1", "a1", 100),
            book("b2", "a1", 200),
            book("b3", "a2", 300),
            book("b4", "a3", 400),
            book("b5", "a3", 500),
        ])
        .execute(client)
        .await?;
    Ok(())
}

async fn rows(client: DinocoClient) -> anyhow::Result<()> {
    let names = find_many::<Author>().order_by(|x| x.id.asc()).skip(3).execute(&client).await?;
    assert_eq!(names.iter().map(|author| author.id.as_str()).collect::<Vec<_>>(), ["a4", "a5"]);

    let all = find_many::<Author>().order_by(|x| x.id.asc()).skip(0).execute(&client).await?;
    assert_eq!(all.len(), 5);

    let past_the_end = find_many::<Author>().order_by(|x| x.id.asc()).skip(10).execute(&client).await?;
    assert!(past_the_end.is_empty());

    let filtered =
        find_many::<Book>().where_(|x| x.pages.gte(200)).order_by(|x| x.pages.desc()).skip(1).execute(&client).await?;
    assert_eq!(filtered.iter().map(|book| book.id.as_str()).collect::<Vec<_>>(), ["b4", "b3", "b2"]);
    Ok(())
}

async fn plucks_and_includes(client: DinocoClient) -> anyhow::Result<()> {
    let ids = find_many::<Book>().order_by(|x| x.id.asc()).skip(2).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(ids, ["b3", "b4", "b5"]);

    let authors = find_many::<Author>()
        .order_by(|x| x.id.asc())
        .skip(2)
        .includes(|x| x.books().order_by(|book| book.id.asc()))
        .execute(&client)
        .await?;
    let shape = authors
        .iter()
        .map(|author| (author.id.as_str(), author.books.iter().map(|book| book.id.as_str()).collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    assert_eq!(shape, [("a3", vec!["b4", "b5"]), ("a4", vec![]), ("a5", vec![])]);

    // A skip inside the include pages each author's books.
    let authors = find_many::<Author>()
        .where_(|x| x.id.batch(["a1", "a3"]))
        .order_by(|x| x.id.asc())
        .includes(|x| x.books().order_by(|book| book.id.asc()).skip(1))
        .execute(&client)
        .await?;
    let shape = authors
        .iter()
        .map(|author| (author.id.as_str(), author.books.iter().map(|book| book.id.as_str()).collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    assert_eq!(shape, [("a1", vec!["b2"]), ("a3", vec!["b5"])]);
    Ok(())
}

async fn aggregates(client: DinocoClient) -> anyhow::Result<()> {
    // The window of rows aggregated: every book but the two shortest.
    let pages =
        find_many::<Book>().order_by(|x| x.pages.asc()).skip(2).pluck(|x| x.pages.sum()).execute(&client).await?;
    assert_eq!(pages, 1200);

    // A page of groups: every author but the first by key.
    let by_author =
        find_many::<Book>().skip(1).pluck(|x| x.pages.sum()).group_by(|x| x.author_id).execute(&client).await?;
    assert_eq!(by_author, [("a2".to_string(), 300), ("a3".to_string(), 900)]);
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let ids = transaction(&client, |tx| async move {
        Ok(find_many::<Author>().order_by(|x| x.id.desc()).skip(4).pluck(|x| x.id).execute(tx).await?)
    })
    .await?;
    assert_eq!(ids, ["a1"]);
    Ok(())
}

fn book(id: &str, author_id: &str, pages: i64) -> Book {
    Book { id: id.to_string(), author_id: author_id.to_string(), pages, author: None }
}

fn book_ids(books: &[Book]) -> Vec<&str> {
    books.iter().map(|book| book.id.as_str()).collect()
}

/// Each `order_by` adds a column after the previous ones, so ties on the first
/// are broken by the next, in every builder that orders rows.
async fn several_order_columns(client: DinocoClient) -> anyhow::Result<()> {
    // Ties on `pages`: b1 and b7 (100), b2 and b8 (200), b3 and b6 (300).
    insert_many::<Book>()
        .values([book("b6", "a2", 300), book("b7", "a2", 100), book("b8", "a1", 200)])
        .execute(&client)
        .await?;

    let books = find_many::<Book>().order_by(|x| x.pages.asc()).order_by(|x| x.id.desc()).execute(&client).await?;
    assert_eq!(book_ids(&books), ["b7", "b1", "b8", "b2", "b6", "b3", "b4", "b5"]);
    let page = find_many::<Book>()
        .order_by(|x| x.pages.asc())
        .order_by(|x| x.id.desc())
        .skip(1)
        .take(3)
        .execute(&client)
        .await?;
    assert_eq!(book_ids(&page), ["b1", "b8", "b2"]);
    let three = find_many::<Book>()
        .order_by(|x| x.author_id.desc())
        .order_by(|x| x.pages.desc())
        .order_by(|x| x.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(book_ids(&three), ["b5", "b4", "b3", "b6", "b7", "b2", "b8", "b1"]);

    let first = find_first::<Book>().order_by(|x| x.pages.asc()).order_by(|x| x.id.desc()).execute(&client).await?;
    assert_eq!(first.map(|book| book.id), Some("b7".to_string()));
    let ids = find_many::<Book>()
        .where_(|x| x.pages.lte(200))
        .order_by(|x| x.pages.desc())
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(ids, ["b2", "b8", "b1", "b7"]);

    // In an include, plain and paged (the paged one orders its window).
    let shape = |authors: &[Author]| {
        authors.iter().map(|author| (author.id.clone(), book_ids(&author.books).join(","))).collect::<Vec<_>>()
    };
    let books_of = |author: &str, books: &str| (author.to_string(), books.to_string());
    let authors = find_many::<Author>()
        .where_(|x| x.id.batch(["a1", "a2"]))
        .order_by(|x| x.id.asc())
        .includes(|x| x.books().order_by(|book| book.pages.asc()).order_by(|book| book.id.desc()))
        .execute(&client)
        .await?;
    assert_eq!(shape(&authors), [books_of("a1", "b1,b8,b2"), books_of("a2", "b7,b6,b3")]);
    let authors = find_many::<Author>()
        .where_(|x| x.id.batch(["a1", "a2"]))
        .order_by(|x| x.id.asc())
        .includes(|x| x.books().order_by(|book| book.pages.desc()).order_by(|book| book.id.desc()).take(2))
        .execute(&client)
        .await?;
    assert_eq!(shape(&authors), [books_of("a1", "b8,b2"), books_of("a2", "b6,b3")]);
    let authors = find_many::<Author>()
        .where_(|x| x.id.batch(["a1", "a2"]))
        .order_by(|x| x.id.asc())
        .includes(|x| x.books().order_by(|book| book.pages.asc()).order_by(|book| book.id.asc()).skip(1).take(1))
        .execute(&client)
        .await?;
    assert_eq!(shape(&authors), [books_of("a1", "b2"), books_of("a2", "b3")]);

    // The window an aggregate reads: the 300-page tie decides which book is in.
    let window_max = |id_descending: bool| {
        let client = &client;
        async move {
            find_many::<Book>()
                .order_by(|x| x.pages.desc())
                .order_by(|x| if id_descending { x.id.desc() } else { x.id.asc() })
                .take(3)
                .pluck(|x| x.id.max())
                .execute(client)
                .await
        }
    };
    assert_eq!(window_max(false).await?, Some("b5".to_string()));
    assert_eq!(window_max(true).await?, Some("b6".to_string()));

    // Groups: a1 and a2 tie on 3 books. A second `order_by` on the key picks
    // how that tie breaks (by default, key ascending).
    let count_by_author = |order: fn(dinoco::FindMany<Book>) -> dinoco::FindMany<Book>| {
        let client = &client;
        async move { order(find_many::<Book>()).pluck(|x| x.id.count()).group_by(|x| x.author_id).execute(client).await }
    };
    let pairs = |pairs: &[(&str, i64)]| pairs.iter().map(|(key, count)| (key.to_string(), *count)).collect::<Vec<_>>();
    assert_eq!(count_by_author(|q| q.order_by(|x| x.id.desc())).await?, pairs(&[("a1", 3), ("a2", 3), ("a3", 2)]));
    assert_eq!(
        count_by_author(|q| q.order_by(|x| x.id.desc()).order_by(|x| x.author_id.desc())).await?,
        pairs(&[("a2", 3), ("a1", 3), ("a3", 2)])
    );
    assert_eq!(
        count_by_author(|q| q.order_by(|x| x.id.desc()).order_by(|x| x.author_id.asc())).await?,
        pairs(&[("a1", 3), ("a2", 3), ("a3", 2)])
    );
    assert_eq!(
        count_by_author(|q| q.order_by(|x| x.author_id.desc()).order_by(|x| x.id.asc())).await?,
        pairs(&[("a3", 2), ("a2", 3), ("a1", 3)])
    );
    let error = count_by_author(|q| q.order_by(|x| x.id.desc()).order_by(|x| x.pages.asc())).await.unwrap_err();
    assert!(error.to_string().contains("not by `pages`"), "{error}");

    let in_transaction = transaction(&client, |tx| async move {
        Ok(find_many::<Book>()
            .order_by(|x| x.pages.asc())
            .order_by(|x| x.id.desc())
            .pluck(|x| x.id)
            .execute(tx)
            .await?)
    })
    .await?;
    assert_eq!(in_transaction, ["b7", "b1", "b8", "b2", "b6", "b3", "b4", "b5"]);

    Ok(())
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-pg-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    reset_schema(&adapter).await?;
    let result = run_seeded(DinocoClient::new(Backend::Sqlite(adapter)), scenario).await;
    let _ = std::fs::remove_file(path);
    result
}

async fn run_on_postgres<F, Fut>(scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_schema(&adapter).await?;
    run_seeded(DinocoClient::new(Backend::Postgres(adapter)), scenario).await
}

async fn run_on_mysql<F, Fut>(scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_schema(&adapter).await?;
    run_seeded(DinocoClient::new(Backend::Mysql(adapter)), scenario).await
}

async fn run_seeded<F, Fut>(client: DinocoClient, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    seed(&client).await?;
    scenario(client).await
}

async fn reset_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    for table in TABLES.iter().rev() {
        drop_table(adapter, table).await?;
    }
    let string = |name: &str| column(name, MigrationColumnType::String);
    create_table(adapter, "pg_author", vec![primary(string("id")), string("name")]).await?;
    create_table(
        adapter,
        "pg_book",
        vec![primary(string("id")), string("author_id"), column("pages", MigrationColumnType::Integer)],
    )
    .await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
