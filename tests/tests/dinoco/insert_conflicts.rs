//! `insert_into`/`insert_many` with `.on_conflict_do_nothing(...)`: a row that
//! collides with an existing one on a unique index is skipped instead of
//! failing, and `.returning::<S>()`/`.pluck(...)` read back the row already
//! there. Covers single and composite targets, duplicates inside one batch,
//! payload order, projections, `transform`, database generated keys, conflicts
//! on other unique indexes, the error paths (nested payloads, `NULL` and
//! generated targets), transactions and hooks.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;
use std::sync::{Arc, Mutex};

use dinoco::chrono::NaiveDate;

use dinoco::{
    CreateError, Entity, EntityExtend, TransactionError, count, find_first, find_many, insert_into, insert_many,
    remove_test_methods, setup_test_methods, transaction,
};
use dinoco_engine::{
    Backend, CreateIndexMigration, DinocoAdapter, DinocoClient, DinocoSqlCompiler, MigrationColumnType,
    MigrationDefault, MigrationIndex, MigrationIndexKind, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use dinoco_tests::{column, create_table, default, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 4] = ["ic_member", "ic_session", "ic_tag", "ic_seat"];

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
    insert_into_skips_duplicates,
    insert_into_returns_the_existing_row,
    insert_many_returns_rows_in_payload_order,
    composite_targets,
    conflicts_on_other_indexes,
    invalid_operations,
    database_generated_keys,
    integer_and_date_keys,
    empty_batches,
    transactions,
    hooks,
);

#[derive(Debug, Entity)]
#[dinoco(table_name = "ic_member")]
pub struct Member {
    #[dinoco(primary_key)]
    id: String,
    email: String,
    tenant: String,
    handle: String,
    nickname: Option<String>,
    visits: i64,

    #[dinoco(one_to_many, foreign_key = "member_id", references = "id")]
    sessions: Vec<Session>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ic_session")]
pub struct Session {
    #[dinoco(primary_key)]
    id: String,
    member_id: String,

    #[dinoco(many_to_one, foreign_key = "member_id", references = "id")]
    member: Option<Member>,
}

#[derive(Debug, EntityExtend)]
#[extend(Member)]
pub struct MemberContact {
    pub id: String,
    pub email: String,
    pub nickname: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ic_tag")]
pub struct Tag {
    #[dinoco(primary_key, auto_generate = autoincrement)]
    id: i64,
    code: String,
    label: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ic_seat")]
pub struct Seat {
    #[dinoco(primary_key)]
    id: String,
    number: i64,
    booked_on: NaiveDate,
}

fn member(id: &str, email: &str, tenant: &str, handle: &str) -> Member {
    Member {
        id: id.to_string(),
        email: email.to_string(),
        tenant: tenant.to_string(),
        handle: handle.to_string(),
        nickname: Some(format!("{id}-nick")),
        visits: 1,
        sessions: Vec::new(),
    }
}

fn tag(code: &str, label: &str) -> Tag {
    Tag { id: 0, code: code.to_string(), label: label.to_string() }
}

async fn ids(client: &DinocoClient) -> anyhow::Result<Vec<String>> {
    find_many::<Member>().order_by(|x| x.id.asc()).pluck(|x| x.id).execute(client).await
}

/// Seeds `m1` (`ana@dinoco.rs`, `acme/ana`) and `m2` (`bia@dinoco.rs`, `acme/bia`).
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Member>()
        .values([member("m1", "ana@dinoco.rs", "acme", "ana"), member("m2", "bia@dinoco.rs", "acme", "bia")])
        .execute(client)
        .await?;
    Ok(())
}

async fn insert_into_skips_duplicates(client: DinocoClient) -> anyhow::Result<()> {
    // Without the clause the duplicate fails, as before.
    let error = insert_into::<Member>()
        .values(member("m3", "ana@dinoco.rs", "acme", "ana-2"))
        .execute(&client)
        .await
        .expect_err("duplicate email");
    assert!(error.is_unique_violation(), "{error:?}");

    let mut duplicate = member("m3", "ana@dinoco.rs", "acme", "ana-2");
    duplicate.nickname = Some("replaced".to_string());
    insert_into::<Member>().values(&duplicate).on_conflict_do_nothing(|x| x.email).execute(&client).await?;
    assert_eq!(ids(&client).await?, ["m1", "m2"]);

    // The existing row is left as it was.
    let nickname = find_first::<Member>().where_(|x| x.id.eq("m1")).pluck(|x| x.nickname).execute(&client).await?;
    assert_eq!(nickname, Some(Some("m1-nick".to_string())));

    // A payload without a conflict is inserted; `on_conflict_do_nothing` may
    // come before `values`.
    insert_into::<Member>()
        .on_conflict_do_nothing(|x| x.email)
        .values(member("m3", "caio@dinoco.rs", "acme", "caio"))
        .execute(&client)
        .await?;
    assert_eq!(ids(&client).await?, ["m1", "m2", "m3"]);
    Ok(())
}

async fn insert_into_returns_the_existing_row(client: DinocoClient) -> anyhow::Result<()> {
    let existing = insert_into::<Member>()
        .values(member("m3", "ana@dinoco.rs", "acme", "ana-2"))
        .on_conflict_do_nothing(|x| x.email)
        .returning::<Member>()
        .execute(&client)
        .await?;
    assert_eq!((existing.id.as_str(), existing.handle.as_str()), ("m1", "ana"));
    assert_eq!(existing.nickname.as_deref(), Some("m1-nick"));

    let inserted = insert_into::<Member>()
        .values(member("m3", "caio@dinoco.rs", "acme", "caio"))
        .on_conflict_do_nothing(|x| x.email)
        .returning::<MemberContact>()
        .execute(&client)
        .await?;
    assert_eq!((inserted.id.as_str(), inserted.email.as_str()), ("m3", "caio@dinoco.rs"));

    let id = insert_into::<Member>()
        .values(member("m4", "bia@dinoco.rs", "acme", "bia-2"))
        .on_conflict_do_nothing(|x| x.email)
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(id, "m2");

    let visits = insert_into::<Member>()
        .values(member("m4", "duda@dinoco.rs", "acme", "duda"))
        .on_conflict_do_nothing(|x| x.email)
        .pluck(|x| x.visits)
        .execute(&client)
        .await?;
    assert_eq!(visits, 1);

    let handle = insert_into::<Member>()
        .values(member("m5", "ana@dinoco.rs", "acme", "ana-3"))
        .on_conflict_do_nothing(|x| x.email)
        .returning::<Member>()
        .transform(|member| member.handle)
        .execute(&client)
        .await?;
    assert_eq!(handle, "ana");

    assert_eq!(ids(&client).await?, ["m1", "m2", "m3", "m4"]);
    Ok(())
}

async fn insert_many_returns_rows_in_payload_order(client: DinocoClient) -> anyhow::Result<()> {
    let payloads = [
        member("m3", "caio@dinoco.rs", "acme", "caio"),
        member("m9", "ana@dinoco.rs", "acme", "ana-9"),
        member("m4", "duda@dinoco.rs", "acme", "duda"),
        // Collides with the first payload of the same batch.
        member("m10", "caio@dinoco.rs", "acme", "caio-10"),
    ];

    let rows = insert_many::<Member>()
        .values(&payloads)
        .on_conflict_do_nothing(|x| x.email)
        .returning::<Member>()
        .execute(&client)
        .await?;
    let returned = rows.iter().map(|row| (row.id.as_str(), row.email.as_str())).collect::<Vec<_>>();
    assert_eq!(
        returned,
        [("m3", "caio@dinoco.rs"), ("m1", "ana@dinoco.rs"), ("m4", "duda@dinoco.rs"), ("m3", "caio@dinoco.rs")]
    );
    assert_eq!(ids(&client).await?, ["m1", "m2", "m3", "m4"]);

    // Every row already exists now: all are read back, still in order.
    let ids_back = insert_many::<Member>()
        .values([member("x1", "duda@dinoco.rs", "acme", "x1"), member("x2", "bia@dinoco.rs", "acme", "x2")])
        .on_conflict_do_nothing(|x| x.email)
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(ids_back, ["m4", "m2"]);

    let contacts = insert_many::<Member>()
        .values([member("m5", "eva@dinoco.rs", "acme", "eva"), member("x3", "ana@dinoco.rs", "acme", "x3")])
        .on_conflict_do_nothing(|x| x.email)
        .returning::<MemberContact>()
        .transform(|contact| contact.id)
        .execute(&client)
        .await?;
    assert_eq!(contacts, ["m5", "m1"]);

    // Without returning, the new rows are inserted and the others skipped.
    insert_many::<Member>()
        .values([member("m6", "fabi@dinoco.rs", "acme", "fabi"), member("x4", "fabi@dinoco.rs", "acme", "x4")])
        .on_conflict_do_nothing(|x| x.email)
        .execute(&client)
        .await?;
    assert_eq!(ids(&client).await?, ["m1", "m2", "m3", "m4", "m5", "m6"]);
    Ok(())
}

async fn composite_targets(client: DinocoClient) -> anyhow::Result<()> {
    let existing = insert_into::<Member>()
        .values(member("m3", "other@dinoco.rs", "acme", "ana"))
        .on_conflict_do_nothing(|x| (x.tenant, x.handle))
        .returning::<Member>()
        .execute(&client)
        .await?;
    assert_eq!(existing.id, "m1");

    // The same handle in another tenant is no conflict.
    let rows = insert_many::<Member>()
        .values([
            member("m3", "ana@globex.rs", "globex", "ana"),
            member("x1", "x1@dinoco.rs", "acme", "bia"),
            member("m4", "bia@globex.rs", "globex", "bia"),
        ])
        .on_conflict_do_nothing(|x| (x.tenant, x.handle))
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["m3", "m2", "m4"]);

    // Targets of three and four columns, sliced or interned, compile too.
    let target = |x: MemberWhere| (x.tenant, x.handle, x.email);
    let _ = insert_into::<Member>().values(member("y", "y@dinoco.rs", "y", "y")).on_conflict_do_nothing(target);
    let _ = insert_many::<Member>()
        .values([member("z", "z@dinoco.rs", "z", "z")])
        .on_conflict_do_nothing(|x| (x.visits, x.handle, x.tenant, x.id));

    assert_eq!(ids(&client).await?, ["m1", "m2", "m3", "m4"]);
    Ok(())
}

async fn conflicts_on_other_indexes(client: DinocoClient) -> anyhow::Result<()> {
    // `m1` is taken as a primary key, but the target is the email.
    let payload = || member("m1", "new@dinoco.rs", "acme", "new");
    let skipped = insert_into::<Member>().values(payload()).on_conflict_do_nothing(|x| x.email).execute(&client).await;
    let returned = insert_into::<Member>()
        .values(payload())
        .on_conflict_do_nothing(|x| x.email)
        .returning::<Member>()
        .execute(&client)
        .await;

    if matches!(client.backend, Backend::Mysql(_)) {
        // MySQL has no conflict target: any duplicate key is skipped, and the
        // row can't be found again by the email.
        skipped?;
        assert!(matches!(returned, Err(CreateError::NotReturned { table: "ic_member" })), "{returned:?}");
    } else {
        assert!(skipped.expect_err("duplicate id").is_unique_violation());
        let error = returned.expect_err("duplicate id");
        assert!(matches!(error, CreateError::UniqueViolation { ref columns, .. } if columns == &["id"]), "{error:?}");
    }

    assert_eq!(ids(&client).await?, ["m1", "m2"]);
    assert_eq!(count::<Member>().where_(|x| x.email.eq("new@dinoco.rs")).execute(&client).await?.total, 0);
    Ok(())
}

async fn invalid_operations(client: DinocoClient) -> anyhow::Result<()> {
    let invalid = |error: CreateError, needle: &str| match error {
        CreateError::InvalidOperation(message) => assert!(message.contains(needle), "{message}"),
        error => panic!("expected an invalid operation, got {error:?}"),
    };

    // Nested rows have no inserted parent to hang from when it is skipped.
    let mut with_session = member("m3", "caio@dinoco.rs", "acme", "caio");
    with_session.sessions = vec![Session { id: "s1".to_string(), member_id: String::new(), member: None }];
    let error =
        insert_into::<Member>().values(&with_session).on_conflict_do_nothing(|x| x.email).execute(&client).await;
    invalid(error.expect_err("nested payload"), "does not insert nested relations");
    let error = insert_many::<Member>()
        .values([member("m4", "duda@dinoco.rs", "acme", "duda"), with_session])
        .on_conflict_do_nothing(|x| x.email)
        .returning::<Member>()
        .execute(&client)
        .await;
    invalid(error.expect_err("nested payload"), "does not insert nested relations");

    // A `NULL` target never conflicts, so the row can't be read back.
    let mut anonymous = member("m3", "caio@dinoco.rs", "acme", "caio");
    anonymous.nickname = None;
    let error = insert_into::<Member>()
        .values(&anonymous)
        .on_conflict_do_nothing(|x| x.nickname)
        .pluck(|x| x.id)
        .execute(&client)
        .await;
    invalid(error.expect_err("NULL target"), "is NULL");

    // A column the database generates is not in the insert.
    let error = insert_into::<Tag>()
        .values(tag("rust", "Rust"))
        .on_conflict_do_nothing(|x| x.id)
        .returning::<Tag>()
        .execute(&client)
        .await;
    invalid(error.expect_err("generated target"), "is not inserted");

    // Nothing was inserted by the failed calls.
    assert_eq!(ids(&client).await?, ["m1", "m2"]);
    assert_eq!(count::<Tag>().execute(&client).await?.total, 0);
    Ok(())
}

async fn database_generated_keys(client: DinocoClient) -> anyhow::Result<()> {
    let rust = insert_into::<Tag>()
        .values(tag("rust", "Rust"))
        .on_conflict_do_nothing(|x| x.code)
        .returning::<Tag>()
        .execute(&client)
        .await?;
    assert!(rust.id > 0);
    assert_eq!(rust.label, "Rust");

    let again = insert_into::<Tag>()
        .values(tag("rust", "Rust again"))
        .on_conflict_do_nothing(|x| x.code)
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(again, rust.id);

    let tags = insert_many::<Tag>()
        .values([tag("orm", "ORM"), tag("rust", "Rust 3"), tag("sql", "SQL")])
        .on_conflict_do_nothing(|x| x.code)
        .returning::<Tag>()
        .execute(&client)
        .await?;
    let codes = tags.iter().map(|tag| (tag.code.as_str(), tag.label.as_str())).collect::<Vec<_>>();
    assert_eq!(codes, [("orm", "ORM"), ("rust", "Rust"), ("sql", "SQL")]);
    assert_eq!(tags[1].id, rust.id);
    assert_ne!(tags[0].id, tags[2].id);
    assert_eq!(count::<Tag>().execute(&client).await?.total, 3);
    Ok(())
}

async fn integer_and_date_keys(client: DinocoClient) -> anyhow::Result<()> {
    let day = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
    let seat = |id: &str, number: i64, booked_on| Seat { id: id.to_string(), number, booked_on };
    insert_into::<Seat>().values(seat("s1", 7, day(19))).execute(&client).await?;

    // Integer keys are read back by their value (cast to `bigint` on PostgreSQL).
    let ids = insert_many::<Seat>()
        .values([seat("s2", 8, day(19)), seat("x1", 7, day(20)), seat("s3", 9, day(19))])
        .on_conflict_do_nothing(|x| x.number)
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(ids, ["s2", "s1", "s3"]);

    // A unique index over a date and an integer.
    let seats = insert_many::<Seat>()
        .values([seat("x2", 7, day(19)), seat("x3", 9, day(19))])
        .on_conflict_do_nothing(|x| (x.booked_on, x.number))
        .returning::<Seat>()
        .execute(&client)
        .await?;
    assert_eq!(seats.iter().map(|seat| seat.id.as_str()).collect::<Vec<_>>(), ["s1", "s3"]);
    assert_eq!(seats[0].booked_on, day(19));

    assert_eq!(count::<Seat>().execute(&client).await?.total, 3);
    Ok(())
}

async fn empty_batches(client: DinocoClient) -> anyhow::Result<()> {
    let rows = insert_many::<Member>()
        .values(Vec::<Member>::new())
        .on_conflict_do_nothing(|x| x.email)
        .returning::<Member>()
        .execute(&client)
        .await?;
    assert!(rows.is_empty());

    let ids_back = insert_many::<Member>()
        .values(Vec::<Member>::new())
        .on_conflict_do_nothing(|x| x.email)
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert!(ids_back.is_empty());

    insert_many::<Member>().values(Vec::<Member>::new()).on_conflict_do_nothing(|x| x.email).execute(&client).await?;
    assert_eq!(ids(&client).await?, ["m1", "m2"]);
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let ids_in_tx = transaction(&client, |tx| async move {
        let existing = insert_into::<Member>()
            .values(member("m3", "ana@dinoco.rs", "acme", "ana-3"))
            .on_conflict_do_nothing(|x| x.email)
            .returning::<Member>()
            .execute(tx)
            .await?;
        assert_eq!(existing.id, "m1");

        // A skipped row raised no error, so the transaction is still usable
        // (PostgreSQL would have aborted it on a failed statement).
        let rows = insert_many::<Member>()
            .values([member("m3", "caio@dinoco.rs", "acme", "caio"), member("x1", "bia@dinoco.rs", "acme", "x1")])
            .on_conflict_do_nothing(|x| x.email)
            .pluck(|x| x.id)
            .execute(tx)
            .await?;
        assert_eq!(rows, ["m3", "m2"]);

        // The rows inserted inside the transaction are found again inside it.
        let again = insert_into::<Member>()
            .values(member("x2", "caio@dinoco.rs", "acme", "x2"))
            .on_conflict_do_nothing(|x| x.email)
            .pluck(|x| x.id)
            .execute(tx)
            .await?;
        assert_eq!(again, "m3");

        Ok(find_many::<Member>().order_by(|x| x.id.asc()).pluck(|x| x.id).execute(tx).await?)
    })
    .await?;
    assert_eq!(ids_in_tx, ["m1", "m2", "m3"]);
    assert_eq!(ids(&client).await?, ["m1", "m2", "m3"]);

    let rolled_back = transaction(&client, |tx| async move {
        insert_into::<Member>()
            .values(member("m4", "duda@dinoco.rs", "acme", "duda"))
            .on_conflict_do_nothing(|x| x.email)
            .execute(tx)
            .await?;
        Err::<(), _>(anyhow::anyhow!("roll back"))
    })
    .await;
    assert!(matches!(rolled_back, Err(TransactionError::Operation(_))));

    // Error paths come back as `TransactionError::Create`.
    let invalid = transaction(&client, |tx| async move {
        insert_into::<Tag>()
            .values(tag("rust", "Rust"))
            .on_conflict_do_nothing(|x| x.id)
            .pluck(|x| x.id)
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    assert!(matches!(invalid, Err(TransactionError::Create(CreateError::InvalidOperation(_)))), "{invalid:?}");

    assert_eq!(ids(&client).await?, ["m1", "m2", "m3"]);
    Ok(())
}

async fn hooks(client: DinocoClient) -> anyhow::Result<()> {
    let inserts = Arc::new(Mutex::new(Vec::new()));
    let finds = Arc::new(Mutex::new(Vec::new()));
    let (insert_sink, find_sink) = (Arc::clone(&inserts), Arc::clone(&finds));
    setup_test_methods::<Member>(&client)
        .on_insert(move |rows, query, _| {
            insert_sink.lock().unwrap().push((rows.map(<[_]>::len), query.sql.clone()));
        })
        .on_find(move |rows, query, _| {
            find_sink.lock().unwrap().push((rows, query.sql.clone()));
        });

    insert_into::<Member>()
        .values(member("m3", "ana@dinoco.rs", "acme", "x"))
        .on_conflict_do_nothing(|x| x.email)
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    insert_many::<Member>()
        .values([member("m3", "caio@dinoco.rs", "acme", "caio"), member("x1", "bia@dinoco.rs", "acme", "x1")])
        .on_conflict_do_nothing(|x| x.email)
        .returning::<Member>()
        .execute(&client)
        .await?;
    remove_test_methods::<Member>(&client);

    let inserts = inserts.lock().unwrap();
    let finds = finds.lock().unwrap();
    assert_eq!(inserts.len(), 2);
    let clause = if matches!(client.backend, Backend::Mysql(_)) { "ON DUPLICATE KEY UPDATE" } else { "ON CONFLICT" };
    assert!(inserts.iter().all(|(_, sql)| sql.contains(clause)), "{inserts:?}");
    assert_eq!(finds.len(), 2);
    assert_eq!(finds[0].0, Some(1));
    assert_eq!(finds[1].0, Some(2));
    assert!(finds[1].1.starts_with("WITH __dinoco_lookup_keys"), "{}", finds[1].1);
    Ok(())
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-ic-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
    let mut email = string("email");
    email.unique = true;
    create_table(
        adapter,
        "ic_member",
        vec![
            primary(string("id")),
            email,
            string("tenant"),
            string("handle"),
            nullable(string("nickname")),
            column("visits", MigrationColumnType::Integer),
        ],
    )
    .await?;
    let index = adapter.compile_create_index_migration(CreateIndexMigration {
        table: "ic_member".to_string(),
        index: MigrationIndex {
            name: "ic_member_tenant_handle_key".to_string(),
            columns: vec!["tenant".to_string(), "handle".to_string()],
            automatic: false,
            kind: MigrationIndexKind::Unique,
            predicate: None,
        },
    });
    adapter.execute(&index, &[]).await?;

    create_table(adapter, "ic_session", vec![primary(string("id")), string("member_id")]).await?;

    let mut number = column("number", MigrationColumnType::Integer);
    number.unique = true;
    create_table(
        adapter,
        "ic_seat",
        vec![primary(string("id")), number, column("booked_on", MigrationColumnType::Date)],
    )
    .await?;
    let index = adapter.compile_create_index_migration(CreateIndexMigration {
        table: "ic_seat".to_string(),
        index: MigrationIndex {
            name: "ic_seat_booked_on_number_key".to_string(),
            columns: vec!["booked_on".to_string(), "number".to_string()],
            automatic: false,
            kind: MigrationIndexKind::Unique,
            predicate: None,
        },
    });
    adapter.execute(&index, &[]).await?;

    let mut code = string("code");
    code.unique = true;
    create_table(
        adapter,
        "ic_tag",
        vec![
            primary(default(column("id", MigrationColumnType::Integer), MigrationDefault::AutoIncrement)),
            code,
            string("label"),
        ],
    )
    .await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
