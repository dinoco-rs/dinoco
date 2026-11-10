//! Keyset (cursor) pagination: `find_many(...).after(...)` and `.before(...)`
//! keep the rows past a cursor in the `order_by` order. Covers single and
//! multi-column orders with mixed directions and ties, row cursors (models
//! and projections), tuple and `Cursor` values, dates, filters and
//! `where_complex`, `.count()` (the total ignores the cursor), `skip`,
//! includes, `select`, `transform`, field and aggregate plucks (grouped too),
//! transactions, the end of the list, and the error paths.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::chrono::NaiveDate;
use dinoco::{Cursor, DinocoValue, Entity, EntityExtend, find_many, transaction};
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, DinocoSqlCompiler, MigrationColumnType, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://dinoco:dinoco@localhost:5432/dinoco";
const MYSQL_URL: &str = "mysql://dinoco:dinoco@localhost:3306/dinoco";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 2] = ["kp_author", "kp_post"];

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

on_every_backend!(
    single_column_pages,
    multi_column_mixed_directions,
    tuple_and_cursor_values,
    filters_count_and_skip,
    includes_select_and_transform,
    plucks,
    transactions,
    invalid_cursors,
);

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "kp_author")]
pub struct Author {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "author_id", references = "id")]
    posts: Vec<Post>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "kp_post")]
pub struct Post {
    #[dinoco(primary_key)]
    id: String,
    author_id: String,
    score: i64,
    published_on: NaiveDate,
    subtitle: Option<String>,

    #[dinoco(many_to_one, foreign_key = "author_id", references = "id")]
    author: Option<Box<Author>>,
}

#[derive(Debug, Clone, EntityExtend)]
#[extend(Post)]
pub struct PostKey {
    pub id: String,
    pub score: i64,
}

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, day).expect("valid date")
}

/// `(id, author, score, day, subtitle)`: scores tie, and so do days.
const POSTS: [(&str, &str, i64, u32, Option<&str>); 10] = [
    ("p01", "a1", 5, 1, Some("s1")),
    ("p02", "a1", 3, 2, None),
    ("p03", "a2", 5, 2, Some("s3")),
    ("p04", "a2", 1, 3, Some("s4")),
    ("p05", "a1", 3, 3, Some("s5")),
    ("p06", "a3", 5, 4, None),
    ("p07", "a3", 2, 4, Some("s7")),
    ("p08", "a1", 4, 5, Some("s8")),
    ("p09", "a2", 1, 5, Some("s9")),
    ("p10", "a3", 2, 6, Some("s10")),
];

async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    dinoco::insert_many::<Author>()
        .values(["a1", "a2", "a3"].map(|id| Author {
            id: id.to_string(),
            name: format!("{id} name"),
            posts: Vec::new(),
        }))
        .execute(client)
        .await?;
    dinoco::insert_many::<Post>()
        .values(POSTS.map(|(id, author, score, day, subtitle)| Post {
            id: id.to_string(),
            author_id: author.to_string(),
            score,
            published_on: date(day),
            subtitle: subtitle.map(str::to_string),
            author: None,
        }))
        .execute(client)
        .await?;
    Ok(())
}

fn ids<T>(rows: &[T], id: impl Fn(&T) -> &str) -> Vec<String> {
    rows.iter().map(|row| id(row).to_string()).collect()
}

fn post_ids(rows: &[Post]) -> Vec<String> {
    ids(rows, |post| &post.id)
}

/// Every post by score descending, then id ascending.
const BY_SCORE: [&str; 10] = ["p01", "p03", "p06", "p08", "p02", "p05", "p07", "p10", "p04", "p09"];

async fn single_column_pages(client: DinocoClient) -> anyhow::Result<()> {
    // Forward: each page starts after the last row of the one before.
    let mut seen = Vec::new();
    let mut page = find_many::<Post>().order_by(|x| x.id.asc()).take(3).execute(&client).await?;
    while !page.is_empty() {
        seen.extend(post_ids(&page));
        let last = page.last().expect("a row").clone();
        page = find_many::<Post>().order_by(|x| x.id.asc()).after(&last).take(3).execute(&client).await?;
    }
    assert_eq!(seen, POSTS.map(|post| post.0));

    // Backward from the end: the rows right before the cursor, in order.
    let last = find_many::<Post>().order_by(|x| x.id.asc()).skip(9).execute(&client).await?.remove(0);
    let page = find_many::<Post>().order_by(|x| x.id.asc()).before(&last).take(3).execute(&client).await?;
    assert_eq!(post_ids(&page), ["p07", "p08", "p09"]);

    // Descending: `after` keeps smaller ids.
    let page = find_many::<Post>().order_by(|x| x.id.desc()).after(("p04",)).execute(&client).await?;
    assert_eq!(post_ids(&page), ["p03", "p02", "p01"]);
    let page = find_many::<Post>().order_by(|x| x.id.desc()).before(("p04",)).take(2).execute(&client).await?;
    assert_eq!(post_ids(&page), ["p06", "p05"]);

    // Past either end there is nothing left.
    assert!(find_many::<Post>().order_by(|x| x.id.asc()).after(("p10",)).execute(&client).await?.is_empty());
    assert!(find_many::<Post>().order_by(|x| x.id.asc()).before(("p01",)).execute(&client).await?.is_empty());
    // A cursor between rows (no row has it) works the same.
    let page = find_many::<Post>().order_by(|x| x.id.asc()).after(("p055",)).take(2).execute(&client).await?;
    assert_eq!(post_ids(&page), ["p06", "p07"]);
    Ok(())
}

async fn multi_column_mixed_directions(client: DinocoClient) -> anyhow::Result<()> {
    let ordered = || find_many::<Post>().order_by(|x| x.score.desc()).order_by(|x| x.id.asc());
    assert_eq!(post_ids(&ordered().execute(&client).await?), BY_SCORE);

    // Ties on the score are broken by the id, so no row is lost or repeated.
    let mut seen = Vec::new();
    let mut page = ordered().take(4).execute(&client).await?;
    while !page.is_empty() {
        seen.extend(post_ids(&page));
        let last = page.last().expect("a row").clone();
        page = ordered().after(&last).take(4).execute(&client).await?;
    }
    assert_eq!(seen, BY_SCORE);

    // Back from the first row of the second page to the first page.
    let second = ordered().skip(4).take(4).execute(&client).await?;
    assert_eq!(post_ids(&second), ["p02", "p05", "p07", "p10"]);
    let first = ordered().before(&second[0]).take(4).execute(&client).await?;
    assert_eq!(post_ids(&first), &BY_SCORE[..4]);
    // `before` without `take` keeps every earlier row, still in order.
    let earlier = ordered().before(&second[1]).execute(&client).await?;
    assert_eq!(post_ids(&earlier), &BY_SCORE[..5]);

    // Three columns, the middle one ascending.
    let page = find_many::<Post>()
        .order_by(|x| x.score.desc())
        .order_by(|x| x.published_on.asc())
        .order_by(|x| x.id.desc())
        .after((5_i64, date(2), "p03"))
        .execute(&client)
        .await?;
    assert_eq!(post_ids(&page), ["p06", "p08", "p02", "p05", "p07", "p10", "p04", "p09"]);
    Ok(())
}

async fn tuple_and_cursor_values(client: DinocoClient) -> anyhow::Result<()> {
    let ordered = || find_many::<Post>().order_by(|x| x.score.desc()).order_by(|x| x.id.asc());

    let tuple = ordered().after((3_i64, "p02")).take(3).execute(&client).await?;
    assert_eq!(post_ids(&tuple), ["p05", "p07", "p10"]);

    // A cursor a client sent back, as plain values.
    let cursor = Cursor::new([DinocoValue::from(3_i64), DinocoValue::from("p02")]);
    assert_eq!(cursor.values().len(), 2);
    let values = ordered().after(cursor.clone()).take(3).execute(&client).await?;
    assert_eq!(post_ids(&values), post_ids(&tuple));
    let previous = ordered().before(cursor).take(2).execute(&client).await?;
    assert_eq!(post_ids(&previous), ["p06", "p08"]);

    // Dates, with ties broken by the id.
    let by_day = find_many::<Post>()
        .order_by(|x| x.published_on.asc())
        .order_by(|x| x.id.asc())
        .after((date(4), "p06"))
        .execute(&client)
        .await?;
    assert_eq!(post_ids(&by_day), ["p07", "p08", "p09", "p10"]);
    Ok(())
}

async fn filters_count_and_skip(client: DinocoClient) -> anyhow::Result<()> {
    let ordered = || find_many::<Post>().order_by(|x| x.score.desc()).order_by(|x| x.id.asc());

    // Filters and the cursor combine.
    let page = ordered().where_(|x| x.author_id.eq("a1")).after((5_i64, "p01")).execute(&client).await?;
    assert_eq!(post_ids(&page), ["p08", "p02", "p05"]);

    // `where_complex` replaces the `where_` filters, never the cursor.
    let page = ordered()
        .where_(|x| x.author_id.eq("a3"))
        .after((4_i64, "p08"))
        .where_complex(|x, c| c.or(x.author_id.eq("a2"), x.score.eq(3_i64)))
        .execute(&client)
        .await?;
    assert_eq!(post_ids(&page), ["p02", "p05", "p04", "p09"]);

    // `.count()` keeps reporting the whole filtered list.
    let (rows, count) = ordered().after((3_i64, "p05")).take(2).count().execute(&client).await?;
    assert_eq!(post_ids(&rows), ["p07", "p10"]);
    assert_eq!(count.total, 10);
    let (rows, count) =
        ordered().where_(|x| x.author_id.eq("a2")).after((1_i64, "p09")).count().execute(&client).await?;
    assert!(rows.is_empty());
    assert_eq!(count.total, 3);
    let (rows, count) = ordered().before((4_i64, "p08")).take(2).count().execute(&client).await?;
    assert_eq!(post_ids(&rows), ["p03", "p06"]);
    assert_eq!(count.total, 10);

    // `skip` skips rows past the cursor.
    let page = ordered().after((5_i64, "p03")).skip(2).take(2).execute(&client).await?;
    assert_eq!(post_ids(&page), ["p02", "p05"]);
    Ok(())
}

async fn includes_select_and_transform(client: DinocoClient) -> anyhow::Result<()> {
    let page = find_many::<Post>()
        .order_by(|x| x.id.asc())
        .includes(|x| x.author())
        .before(("p04",))
        .take(2)
        .execute(&client)
        .await?;
    assert_eq!(post_ids(&page), ["p02", "p03"]);
    assert_eq!(page[0].author.as_ref().map(|author| author.id.as_str()), Some("a1"));
    assert_eq!(page[1].author.as_ref().map(|author| author.id.as_str()), Some("a2"));

    // Authors after `a1`, each with its posts after `p05`.
    let authors = find_many::<Author>()
        .order_by(|x| x.id.asc())
        .after(("a1",))
        .includes(|x| x.posts().where_(|post| post.id.gt("p05")).order_by(|post| post.id.asc()))
        .execute(&client)
        .await?;
    assert_eq!(ids(&authors, |author| &author.id), ["a2", "a3"]);
    assert_eq!(post_ids(&authors[0].posts), ["p09"]);
    assert_eq!(post_ids(&authors[1].posts), ["p06", "p07", "p10"]);

    // A projection is a row cursor too.
    let keys = find_many::<Post>()
        .select::<PostKey>()
        .order_by(|x| x.score.desc())
        .order_by(|x| x.id.asc())
        .take(3)
        .execute(&client)
        .await?;
    let next = find_many::<Post>()
        .select::<PostKey>()
        .order_by(|x| x.score.desc())
        .order_by(|x| x.id.asc())
        .after(keys.last().expect("a row"))
        .take(3)
        .execute(&client)
        .await?;
    assert_eq!(ids(&next, |key| &key.id), ["p08", "p02", "p05"]);
    assert_eq!(next[0].score, 4);

    let mapped = find_many::<Post>()
        .order_by(|x| x.id.desc())
        .before(("p08",))
        .take(2)
        .transform(|post| post.id)
        .execute(&client)
        .await?;
    assert_eq!(mapped, ["p10", "p09"]);
    Ok(())
}

async fn plucks(client: DinocoClient) -> anyhow::Result<()> {
    let ordered = || find_many::<Post>().order_by(|x| x.score.desc()).order_by(|x| x.id.asc());

    let after = ordered().after((4_i64, "p08")).take(3).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(after, ["p02", "p05", "p07"]);
    let before = ordered().before((4_i64, "p08")).take(2).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(before, ["p03", "p06"]);

    // An aggregate reads only the rows past the cursor; with `take`, `before`
    // aggregates the rows right before it.
    let rest = ordered().after((3_i64, "p05")).pluck(|x| x.score.sum()).execute(&client).await?;
    assert_eq!(rest, 2 + 2 + 1 + 1);
    let window = ordered().before((3_i64, "p02")).take(2).pluck(|x| x.score.sum()).execute(&client).await?;
    assert_eq!(window, 5 + 4);

    // Grouped, the cursor picks the rows and the order applies to the groups.
    let groups = find_many::<Post>()
        .order_by(|x| x.author_id.asc())
        .order_by(|x| x.id.asc())
        .after(("a1", "p05"))
        .pluck(|x| x.score.sum())
        .group_by(|x| x.author_id)
        .execute(&client)
        .await;
    // `order_by(id)` names neither the key nor the aggregate.
    assert!(groups.is_err());
    let groups = find_many::<Post>()
        .order_by(|x| x.author_id.asc())
        .after(("a1",))
        .pluck(|x| x.score.sum())
        .group_by(|x| x.author_id)
        .execute(&client)
        .await?;
    assert_eq!(groups, [("a2".to_string(), 5 + 1 + 1), ("a3".to_string(), 5 + 2 + 2)]);
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let (page, total) = transaction(&client, |tx| async move {
        dinoco::delete_many::<Post>().where_(|x| x.id.eq("p03")).execute(tx).await?;
        let (rows, count) = find_many::<Post>()
            .order_by(|x| x.score.desc())
            .order_by(|x| x.id.asc())
            .after((5_i64, "p01"))
            .take(2)
            .count()
            .execute(tx)
            .await?;
        Ok((post_ids(&rows), count.total))
    })
    .await?;
    // The transaction sees its own delete.
    assert_eq!(page, ["p06", "p08"]);
    assert_eq!(total, 9);
    Ok(())
}

async fn invalid_cursors(client: DinocoClient) -> anyhow::Result<()> {
    let message = |result: anyhow::Result<Vec<Post>>| result.expect_err("an invalid cursor").to_string();

    // A cursor is a position in the `order_by` order.
    let error = message(find_many::<Post>().after(("p01",)).execute(&client).await);
    assert!(error.contains("needs an `order_by`"), "{error}");

    // One value per `order_by` column.
    let error = message(find_many::<Post>().order_by(|x| x.score.desc()).after((5_i64, "p01")).execute(&client).await);
    assert!(error.contains("has 2 value(s) but the query orders by 1"), "{error}");

    // `NULL` has no place in the order.
    let error = message(
        find_many::<Post>()
            .order_by(|x| x.subtitle.asc())
            .order_by(|x| x.id.asc())
            .after((DinocoValue::Null, "p02"))
            .execute(&client)
            .await,
    );
    assert!(error.contains("is NULL"), "{error}");
    let no_subtitle = find_many::<Post>().where_(|x| x.id.eq("p02")).execute(&client).await?.remove(0);
    let error = message(find_many::<Post>().order_by(|x| x.subtitle.asc()).after(&no_subtitle).execute(&client).await);
    assert!(error.contains("no value for `subtitle`"), "{error}");

    // A projection without an `order_by` column can't be a cursor for it.
    let key = PostKey { id: "p01".to_string(), score: 5 };
    let error = message(find_many::<Post>().order_by(|x| x.published_on.asc()).after(&key).execute(&client).await);
    assert!(error.contains("no value for `published_on`"), "{error}");

    // A row cursor reads the columns ordered by when `after` is called.
    let first = find_many::<Post>().where_(|x| x.id.eq("p01")).execute(&client).await?.remove(0);
    let error = message(find_many::<Post>().after(&first).order_by(|x| x.id.asc()).execute(&client).await);
    assert!(error.contains("call `order_by` before"), "{error}");

    // The error paths reach every builder that runs the cursor.
    let error = find_many::<Post>().after(("p01",)).pluck(|x| x.id).execute(&client).await;
    assert!(error.is_err());
    let error = find_many::<Post>().after(("p01",)).pluck(|x| x.score.sum()).execute(&client).await;
    assert!(error.is_err());
    let error = find_many::<Post>().after(("p01",)).count().execute(&client).await;
    assert!(error.is_err());
    let error =
        find_many::<Post>().after(("p01",)).pluck(|x| x.score.sum()).group_by(|x| x.author_id).execute(&client).await;
    assert!(error.is_err());
    Ok(())
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-kp-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
    create_table(adapter, "kp_author", vec![primary(string("id")), string("name")]).await?;
    create_table(
        adapter,
        "kp_post",
        vec![
            primary(string("id")),
            string("author_id"),
            column("score", MigrationColumnType::Integer),
            column("published_on", MigrationColumnType::Date),
            nullable(string("subtitle")),
        ],
    )
    .await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
