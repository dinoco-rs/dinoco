//! `insert_into`/`insert_many` with `.on_conflict_do_update(...)`: a row that
//! collides with an existing one on a unique index updates it instead of
//! failing (an upsert). Covers `excluded()`, `set(...)` and arithmetic on the
//! existing row, tuples/arrays/`Vec`s of assignments, single and composite
//! targets, `@updated_at`, no assignments, `.returning::<S>()`/`.pluck(...)`
//! in payload order, duplicates inside one batch, the error paths, the SQL
//! each database gets, transactions and hooks.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;
use std::sync::{Arc, Mutex};

use dinoco::chrono::{DateTime, TimeZone, Utc};
use dinoco::{
    ConflictSet, CreateError, DinocoValue, Entity, EntityExtend, InsertQuery, TransactionError, UpdateSet, UpsertQuery,
    count, find_first, find_many, insert_into, insert_many, remove_test_methods, setup_test_methods, transaction,
};
use dinoco_engine::{
    Backend, CreateIndexMigration, DinocoAdapter, DinocoClient, DinocoSqlCompiler, MigrationColumnType,
    MigrationDefault, MigrationIndex, MigrationIndexKind, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use dinoco_tests::{column, create_table, default, drop_table, primary};

const POSTGRES_URL: &str = "postgres://dinoco:dinoco@localhost:5432/dinoco";
const MYSQL_URL: &str = "mysql://dinoco:dinoco@localhost:3306/dinoco";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 4] = ["us_page", "us_visit", "us_seat", "us_counter"];

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
    insert_into_updates_the_existing_row,
    returning_and_pluck,
    insert_many_mixes_inserts_and_updates,
    duplicates_inside_one_batch,
    composite_targets,
    updated_at_is_refreshed,
    no_assignments_keep_the_row,
    invalid_operations,
    transactions,
    hooks,
);

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "us_page")]
pub struct Page {
    #[dinoco(primary_key)]
    id: String,
    path: String,
    title: String,
    hits: i64,
    #[dinoco(updated_at)]
    updated_at: DateTime<Utc>,

    #[dinoco(one_to_many, foreign_key = "page_id", references = "id")]
    visits: Vec<Visit>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "us_visit")]
pub struct Visit {
    #[dinoco(primary_key)]
    id: String,
    page_id: String,

    #[dinoco(many_to_one, foreign_key = "page_id", references = "id")]
    page: Option<Box<Page>>,
}

#[derive(Debug, EntityExtend)]
#[extend(Page)]
pub struct PageHits {
    pub id: String,
    pub hits: i64,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "us_seat")]
pub struct Seat {
    #[dinoco(primary_key)]
    id: String,
    room: String,
    number: i64,
    holder: String,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "us_counter")]
pub struct Counter {
    #[dinoco(primary_key, auto_generate = autoincrement)]
    id: i64,
    code: String,
    total: i64,
}

fn long_ago() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).single().expect("valid instant")
}

fn page(id: &str, path: &str, title: &str) -> Page {
    Page {
        id: id.to_string(),
        path: path.to_string(),
        title: title.to_string(),
        hits: 1,
        updated_at: long_ago(),
        visits: Vec::new(),
    }
}

fn seat(id: &str, room: &str, number: i64, holder: &str) -> Seat {
    Seat { id: id.to_string(), room: room.to_string(), number, holder: holder.to_string() }
}

async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Page>().values([page("p1", "/home", "Home"), page("p2", "/blog", "Blog")]).execute(client).await?;
    insert_many::<Seat>().values([seat("s1", "red", 1, "ana"), seat("s2", "red", 2, "bia")]).execute(client).await?;
    insert_into::<Counter>().values(Counter { id: 0, code: "visits".to_string(), total: 10 }).execute(client).await?;
    Ok(())
}

async fn pages(client: &DinocoClient) -> anyhow::Result<Vec<(String, String, String, i64)>> {
    Ok(find_many::<Page>()
        .order_by(|x| x.id.asc())
        .execute(client)
        .await?
        .into_iter()
        .map(|page| (page.id, page.path, page.title, page.hits))
        .collect())
}

fn row(id: &str, path: &str, title: &str, hits: i64) -> (String, String, String, i64) {
    (id.to_string(), path.to_string(), title.to_string(), hits)
}

async fn insert_into_updates_the_existing_row(client: DinocoClient) -> anyhow::Result<()> {
    // `/home` exists: its title takes the proposed one and its hits go up.
    insert_into::<Page>()
        .values(page("p9", "/home", "Home v2"))
        .on_conflict_do_update(|x| x.path, |x| (x.title.excluded(), x.hits.increment(1)))
        .execute(&client)
        .await?;
    // `/docs` doesn't: it is inserted as given.
    insert_into::<Page>()
        .values(page("p3", "/docs", "Docs"))
        .on_conflict_do_update(|x| x.path, |x| (x.title.excluded(), x.hits.increment(1)))
        .execute(&client)
        .await?;
    assert_eq!(
        pages(&client).await?,
        [row("p1", "/home", "Home v2", 2), row("p2", "/blog", "Blog", 1), row("p3", "/docs", "Docs", 1)]
    );

    // A value of its own, an array, a `Vec`, and every arithmetic operation.
    insert_into::<Page>()
        .values(page("p9", "/blog", "ignored"))
        .on_conflict_do_update(|x| x.path, |x| [x.title.set("Pinned"), x.hits.set(40_i64)])
        .execute(&client)
        .await?;
    for update in [
        |x: PageUpdate| vec![x.hits.multiply(3_i64)],
        |x: PageUpdate| vec![x.hits.divide(2_i64)],
        |x: PageUpdate| vec![x.hits.decrement(5_i64)],
    ] {
        insert_into::<Page>()
            .values(page("p9", "/blog", "ignored"))
            .on_conflict_do_update(|x| x.path, update)
            .execute(&client)
            .await?;
    }
    let blog = find_first::<Page>().where_(|x| x.path.eq("/blog")).execute(&client).await?.expect("blog");
    assert_eq!((blog.id.as_str(), blog.title.as_str(), blog.hits), ("p2", "Pinned", 40 * 3 / 2 - 5));
    assert_eq!(count::<Page>().execute(&client).await?.total, 3);
    Ok(())
}

async fn returning_and_pluck(client: DinocoClient) -> anyhow::Result<()> {
    // The row after the statement: the existing one, updated.
    let updated = insert_into::<Page>()
        .values(page("p9", "/home", "Home v2"))
        .on_conflict_do_update(|x| x.path, |x| (x.title.excluded(), x.hits.increment(2)))
        .returning::<Page>()
        .execute(&client)
        .await?;
    assert_eq!((updated.id.as_str(), updated.title.as_str(), updated.hits), ("p1", "Home v2", 3));

    let inserted = insert_into::<Page>()
        .values(page("p3", "/docs", "Docs"))
        .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
        .returning::<PageHits>()
        .execute(&client)
        .await?;
    assert_eq!((inserted.id.as_str(), inserted.hits), ("p3", 1));

    let id = insert_into::<Page>()
        .values(page("p9", "/blog", "Blog v2"))
        .on_conflict_do_update(|x| x.path, |x| x.hits.increment(1))
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(id, "p2");

    let title = insert_into::<Page>()
        .values(page("p9", "/blog", "Blog v3"))
        .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
        .returning::<Page>()
        .transform(|page| page.title)
        .execute(&client)
        .await?;
    assert_eq!(title, "Blog v3");

    // A database generated key comes back too.
    let counter = insert_into::<Counter>()
        .values(Counter { id: 0, code: "visits".to_string(), total: 1 })
        .on_conflict_do_update(|x| x.code, |x| x.total.increment(1))
        .returning::<Counter>()
        .execute(&client)
        .await?;
    assert_eq!(counter.total, 11);
    assert!(counter.id > 0);
    Ok(())
}

async fn insert_many_mixes_inserts_and_updates(client: DinocoClient) -> anyhow::Result<()> {
    let rows = insert_many::<Page>()
        .values([page("p3", "/docs", "Docs"), page("p9", "/home", "Home v2"), page("p4", "/about", "About")])
        .on_conflict_do_update(|x| x.path, |x| (x.title.excluded(), x.hits.increment(1)))
        .returning::<PageHits>()
        .execute(&client)
        .await?;
    // One row per payload, in payload order.
    let rows = rows.into_iter().map(|row| (row.id, row.hits)).collect::<Vec<_>>();
    assert_eq!(rows, [("p3".to_string(), 1), ("p1".to_string(), 2), ("p4".to_string(), 1)]);

    let ids = insert_many::<Page>()
        .values([page("p8", "/blog", "Blog v2"), page("p7", "/docs", "Docs v2")])
        .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(ids, ["p2", "p3"]);

    insert_many::<Page>()
        .values([page("p5", "/contact", "Contact"), page("p6", "/home", "Home v3")])
        .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
        .execute(&client)
        .await?;
    assert_eq!(
        pages(&client).await?,
        [
            row("p1", "/home", "Home v3", 2),
            row("p2", "/blog", "Blog v2", 1),
            row("p3", "/docs", "Docs v2", 1),
            row("p4", "/about", "About", 1),
            row("p5", "/contact", "Contact", 1),
        ]
    );

    // An empty batch runs nothing.
    let none = insert_many::<Page>()
        .values(Vec::<Page>::new())
        .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
        .returning::<Page>()
        .execute(&client)
        .await?;
    assert!(none.is_empty());
    Ok(())
}

async fn duplicates_inside_one_batch(client: DinocoClient) -> anyhow::Result<()> {
    let result = insert_many::<Page>()
        .values([page("p3", "/docs", "Docs"), page("p4", "/docs", "Docs v2")])
        .on_conflict_do_update(|x| x.path, |x| (x.title.excluded(), x.hits.increment(1)))
        .execute(&client)
        .await;

    if matches!(client.backend, Backend::Postgres(_)) {
        // PostgreSQL updates a row once per statement, and inserts nothing.
        assert!(result.is_err());
        assert_eq!(count::<Page>().execute(&client).await?.total, 2);
    } else {
        // SQLite and MySQL apply the rows in order.
        result?;
        let docs = find_first::<Page>().where_(|x| x.path.eq("/docs")).execute(&client).await?.expect("docs");
        assert_eq!((docs.id.as_str(), docs.title.as_str(), docs.hits), ("p3", "Docs v2", 2));
    }
    Ok(())
}

async fn composite_targets(client: DinocoClient) -> anyhow::Result<()> {
    let seats = insert_many::<Seat>()
        .values([seat("x1", "red", 2, "caio"), seat("s3", "blue", 2, "duda")])
        .on_conflict_do_update(|x| (x.room, x.number), |x| x.holder.excluded())
        .returning::<Seat>()
        .execute(&client)
        .await?;
    let seats = seats.into_iter().map(|seat| (seat.id, seat.holder)).collect::<Vec<_>>();
    assert_eq!(seats, [("s2".to_string(), "caio".to_string()), ("s3".to_string(), "duda".to_string())]);

    let holders = find_many::<Seat>().order_by(|x| x.id.asc()).pluck(|x| x.holder).execute(&client).await?;
    assert_eq!(holders, ["ana", "caio", "duda"]);
    Ok(())
}

async fn updated_at_is_refreshed(client: DinocoClient) -> anyhow::Result<()> {
    insert_into::<Page>()
        .values(page("p9", "/home", "Home v2"))
        .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
        .execute(&client)
        .await?;
    let home = find_first::<Page>().where_(|x| x.id.eq("p1")).execute(&client).await?.expect("home");
    assert!(home.updated_at > long_ago() + dinoco::chrono::Duration::days(365), "{:?}", home.updated_at);

    // Assigned explicitly, it keeps that value.
    let pinned = Utc.with_ymd_and_hms(2021, 6, 1, 12, 0, 0).single().expect("valid instant");
    insert_into::<Page>()
        .values(page("p9", "/blog", "Blog"))
        .on_conflict_do_update(|x| x.path, |x| x.updated_at.set(pinned))
        .execute(&client)
        .await?;
    let blog = find_first::<Page>().where_(|x| x.id.eq("p2")).execute(&client).await?.expect("blog");
    assert_eq!(blog.updated_at, pinned);
    Ok(())
}

async fn no_assignments_keep_the_row(client: DinocoClient) -> anyhow::Result<()> {
    let existing = insert_into::<Page>()
        .values(page("p9", "/home", "Home v2"))
        .on_conflict_do_update(|x| x.path, |_| Vec::<UpdateSet>::new())
        .returning::<Page>()
        .execute(&client)
        .await?;
    assert_eq!((existing.id.as_str(), existing.title.as_str(), existing.hits), ("p1", "Home", 1));
    // Nothing changed, `@updated_at` included.
    assert_eq!(existing.updated_at, long_ago());
    Ok(())
}

async fn invalid_operations(client: DinocoClient) -> anyhow::Result<()> {
    let invalid = |error: CreateError, needle: &str| match error {
        CreateError::InvalidOperation(message) => assert!(message.contains(needle), "{message}"),
        error => panic!("expected an invalid operation, got {error:?}"),
    };

    // The conflict columns find the row; they can't be assigned.
    let error = insert_into::<Page>()
        .values(page("p9", "/home", "Home v2"))
        .on_conflict_do_update(|x| x.path, |x| x.path.excluded())
        .execute(&client)
        .await;
    invalid(error.expect_err("conflict column"), "is a conflict column");

    // A column the insert doesn't write has no proposed value.
    let error = insert_into::<Counter>()
        .values(Counter { id: 0, code: "visits".to_string(), total: 1 })
        .on_conflict_do_update(|x| x.code, |x| x.id.excluded())
        .execute(&client)
        .await;
    invalid(error.expect_err("generated column"), "the insert does not write it");

    // Nested rows have no inserted parent to hang from.
    let mut with_visit = page("p9", "/home", "Home v2");
    with_visit.visits = vec![Visit { id: "v1".to_string(), page_id: String::new(), page: None }];
    let error = insert_into::<Page>()
        .values(&with_visit)
        .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
        .execute(&client)
        .await;
    invalid(error.expect_err("nested payload"), "on_conflict_do_update(...) on `us_page` does not insert nested");

    // A generated target can't be read back.
    let error = insert_into::<Counter>()
        .values(Counter { id: 0, code: "other".to_string(), total: 1 })
        .on_conflict_do_update(|x| x.id, |x| x.total.increment(1))
        .returning::<Counter>()
        .execute(&client)
        .await;
    invalid(error.expect_err("generated target"), "is not inserted");

    // Nothing was written by the failed calls.
    assert_eq!(pages(&client).await?, [row("p1", "/home", "Home", 1), row("p2", "/blog", "Blog", 1)]);
    assert_eq!(count::<Counter>().execute(&client).await?.total, 1);
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let hits = transaction(&client, |tx| async move {
        let updated = insert_into::<Page>()
            .values(page("p9", "/home", "Home v2"))
            .on_conflict_do_update(|x| x.path, |x| x.hits.increment(1))
            .returning::<Page>()
            .execute(tx)
            .await?;
        assert_eq!(updated.id, "p1");

        insert_many::<Page>()
            .values([page("p3", "/docs", "Docs"), page("p9", "/home", "ignored")])
            .on_conflict_do_update(|x| x.path, |x| x.hits.increment(1))
            .execute(tx)
            .await?;

        // The rows written inside the transaction are read back inside it.
        Ok(find_many::<Page>().order_by(|x| x.id.asc()).pluck(|x| x.hits).execute(tx).await?)
    })
    .await?;
    assert_eq!(hits, [3, 1, 1]);

    let rolled_back = transaction(&client, |tx| async move {
        insert_into::<Page>()
            .values(page("p9", "/blog", "Blog v2"))
            .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
            .execute(tx)
            .await?;
        Err::<(), _>(anyhow::anyhow!("roll back"))
    })
    .await;
    assert!(matches!(rolled_back, Err(TransactionError::Operation(_))));

    let invalid = transaction(&client, |tx| async move {
        insert_into::<Page>()
            .values(page("p9", "/blog", "Blog v2"))
            .on_conflict_do_update(|x| x.path, |x| x.path.set("/other"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    assert!(matches!(invalid, Err(TransactionError::Create(CreateError::InvalidOperation(_)))), "{invalid:?}");

    assert_eq!(
        pages(&client).await?,
        [row("p1", "/home", "Home", 3), row("p2", "/blog", "Blog", 1), row("p3", "/docs", "Docs", 1)]
    );
    Ok(())
}

async fn hooks(client: DinocoClient) -> anyhow::Result<()> {
    let inserts = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&inserts);
    setup_test_methods::<Page>(&client).on_insert(move |rows, query, _| {
        sink.lock().unwrap().push((rows.map(<[_]>::len), query.sql.clone(), query.params.len()));
    });

    insert_many::<Page>()
        .values([page("p3", "/docs", "Docs"), page("p9", "/home", "Home v2")])
        .on_conflict_do_update(|x| x.path, |x| (x.title.excluded(), x.hits.increment(1)))
        .execute(&client)
        .await?;
    let in_transaction = transaction(&client, |tx| async move {
        insert_into::<Page>()
            .values(page("p9", "/blog", "Blog v2"))
            .on_conflict_do_update(|x| x.path, |x| x.title.excluded())
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    remove_test_methods::<Page>(&client);
    in_transaction?;

    let inserts = inserts.lock().unwrap();
    assert_eq!(inserts.len(), 2);
    let clause = if matches!(client.backend, Backend::Mysql(_)) { "ON DUPLICATE KEY UPDATE" } else { "DO UPDATE SET" };
    assert!(inserts.iter().all(|(_, sql, _)| sql.contains(clause)), "{inserts:?}");
    // Every proposed row is reported, plus the bound increment and the `@updated_at` refresh binds nothing.
    assert_eq!(inserts[0].0, Some(2));
    assert_eq!(inserts[0].2, 2 * 5 + 1);
    assert_eq!(inserts[1].0, Some(1));
    Ok(())
}

/// The statement each database gets.
#[tokio::test]
async fn compiles_upserts() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer(POSTGRES_URL).await?;
    let mysql = MySqlAdapter::new(MYSQL_URL);
    let query = |sets: Vec<ConflictSet>| UpsertQuery {
        insert: InsertQuery {
            table: "us_page",
            fields: vec!["id", "path", "title"],
            rows: vec![vec![DinocoValue::from("p1"), DinocoValue::from("/home"), DinocoValue::from("Home")]],
            returning: None,
            on_conflict_do_nothing: None,
        },
        target: &["path"],
        sets,
    };
    let sets = || {
        let update = PageUpdate::default();
        vec![update.title.excluded(), ConflictSet::from(update.hits.increment(1_i64))]
    };

    let (sql, params) = sqlite.compile_upsert_query(query(sets()));
    assert_eq!(
        sql,
        "INSERT INTO us_page (id, path, title) VALUES (?, ?, ?) ON CONFLICT (path) DO UPDATE SET title = \
         excluded.title, hits = us_page.hits + ?"
    );
    assert_eq!(params.len(), 4);
    let (sql, _) = postgres.compile_upsert_query(query(sets()));
    assert_eq!(
        sql,
        "INSERT INTO us_page (id, path, title) VALUES ($1, $2, $3) ON CONFLICT (path) DO UPDATE SET title = \
         excluded.title, hits = us_page.hits + $4"
    );
    let (sql, _) = mysql.compile_upsert_query(query(sets()));
    assert_eq!(
        sql,
        "INSERT INTO us_page (id, path, title) VALUES (?, ?, ?) AS __dinoco_new ON DUPLICATE KEY UPDATE title = \
         __dinoco_new.title, hits = us_page.hits + ?"
    );

    // Without assignments it is an insert that skips conflicts.
    let (sql, _) = sqlite.compile_upsert_query(query(Vec::new()));
    assert!(sql.ends_with("ON CONFLICT (path) DO NOTHING"), "{sql}");
    let (sql, _) = mysql.compile_upsert_query(query(Vec::new()));
    assert!(sql.ends_with("ON DUPLICATE KEY UPDATE path = path"), "{sql}");
    Ok(())
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-us-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
    let unique = |mut column: dinoco_engine::MigrationColumn| {
        column.unique = true;
        column
    };
    create_table(
        adapter,
        "us_page",
        vec![
            primary(string("id")),
            unique(string("path")),
            string("title"),
            column("hits", MigrationColumnType::Integer),
            column("updated_at", MigrationColumnType::DateTime),
        ],
    )
    .await?;
    create_table(adapter, "us_visit", vec![primary(string("id")), string("page_id")]).await?;
    create_table(
        adapter,
        "us_seat",
        vec![primary(string("id")), string("room"), column("number", MigrationColumnType::Integer), string("holder")],
    )
    .await?;
    let index = adapter.compile_create_index_migration(CreateIndexMigration {
        table: "us_seat".to_string(),
        index: MigrationIndex {
            name: "us_seat_room_number_key".to_string(),
            columns: vec!["room".to_string(), "number".to_string()],
            automatic: false,
            kind: MigrationIndexKind::Unique,
            predicate: None,
        },
    });
    adapter.execute(&index, &[]).await?;
    create_table(
        adapter,
        "us_counter",
        vec![
            primary(default(column("id", MigrationColumnType::Integer), MigrationDefault::AutoIncrement)),
            unique(string("code")),
            column("total", MigrationColumnType::Integer),
        ],
    )
    .await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
