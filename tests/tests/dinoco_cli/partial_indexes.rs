//! Partial indexes: `@unique<...>`, `@index<...>`, `@@uniques<...>([...])` and
//! `@@indexes<...>([...])`, migrated by the CLI on every backend.
//!
//! - the database keeps the condition, reads it back, and generating again
//!   finds nothing to do;
//! - a partial unique index only rejects duplicates among the rows its
//!   condition matches (MySQL emulates it with `CASE WHEN` key parts);
//! - a changed or removed condition recreates the index;
//! - a SQLite table rebuild keeps rows that only collide outside the
//!   condition, and the test ambient creates partial indexes too.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_cli::db::{CliDatabase, DatabaseSchema};
use dinoco_engine::{
    DatabaseConstraintError, DatabaseError, DinocoAdapter, IndexPredicate, MigrationIndex, MigrationIndexKind,
    MySqlAdapter, PostgresAdapter, PredicateOperator, PredicateValue, SqliteAdapter,
};

const POSTGRES_ADMIN_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_ADMIN_URL: &str = "mysql://root:root@localhost:3306/mysql";
static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Backend {
    Sqlite,
    Postgres,
    Mysql,
}

impl Backend {
    fn provider(self) -> &'static str {
        match self {
            Backend::Sqlite => "sqlite",
            Backend::Postgres => "postgresql",
            Backend::Mysql => "mysql",
        }
    }
}

macro_rules! on_every_backend {
    ($($name:ident => $scenario:ident),* $(,)?) => {
        $(
            mod $name {
                use super::*;

                #[tokio::test]
                async fn sqlite() -> anyhow::Result<()> {
                    $scenario(Backend::Sqlite).await
                }

                #[tokio::test]
                async fn postgres() -> anyhow::Result<()> {
                    $scenario(Backend::Postgres).await
                }

                #[tokio::test]
                async fn mysql() -> anyhow::Result<()> {
                    $scenario(Backend::Mysql).await
                }
            }
        )*
    };
}

on_every_backend! {
    conditions_migrate_and_stay_stable => conditions_migrate_and_stay_stable,
    partial_uniques_only_constrain_matching_rows => partial_uniques_only_constrain_matching_rows,
    changed_conditions_recreate_the_index => changed_conditions_recreate_the_index,
    rebuilds_keep_rows_outside_the_condition => rebuilds_keep_rows_outside_the_condition,
}

const MODELS: &str = r#"
enum Plan {
    FREE
    PRO
}

model Account {
    id         String    @id
    email      String?   @unique<email != null>
    handle     String?   @index<handle != null && handle != "it's \"*/\" ok">
    tenant     String
    slug       String
    plan       Plan      @default(FREE)
    archived   Boolean   @default(false)
    deleted_at DateTime?
    score      Integer   @default(0)

    @@uniques<deleted_at == null && slug != "_">([tenant, slug])
    @@uniques<!archived>([handle])
    @@indexes<plan == PRO || score >= 10>([tenant, score])
}
"#;

fn field(name: &str) -> String {
    name.to_string()
}

fn compare(name: &str, operator: PredicateOperator, value: PredicateValue) -> IndexPredicate {
    IndexPredicate::Compare { field: field(name), operator, value }
}

fn expected_indexes() -> Vec<(&'static str, Vec<&'static str>, MigrationIndexKind, IndexPredicate)> {
    use PredicateOperator::*;
    vec![
        (
            "idx_account_handle",
            vec!["handle"],
            MigrationIndexKind::Standard,
            IndexPredicate::And(vec![
                compare("handle", Neq, PredicateValue::Null),
                compare("handle", Neq, PredicateValue::String("it's \"*/\" ok".to_string())),
            ]),
        ),
        (
            "idx_account_tenant_score",
            vec!["tenant", "score"],
            MigrationIndexKind::Standard,
            IndexPredicate::Or(vec![
                compare("plan", Eq, PredicateValue::String("PRO".to_string())),
                compare("score", Gte, PredicateValue::Number("10".to_string())),
            ]),
        ),
        ("uq_account_email", vec!["email"], MigrationIndexKind::Unique, compare("email", Neq, PredicateValue::Null)),
        (
            "uq_account_handle",
            vec!["handle"],
            MigrationIndexKind::Unique,
            IndexPredicate::Not(Box::new(IndexPredicate::Field(field("archived")))),
        ),
        (
            "uq_account_tenant_slug",
            vec!["tenant", "slug"],
            MigrationIndexKind::Unique,
            IndexPredicate::And(vec![
                compare("deleted_at", Eq, PredicateValue::Null),
                compare("slug", Neq, PredicateValue::String("_".to_string())),
            ]),
        ),
    ]
}

fn partial_indexes(schema: &DatabaseSchema) -> Vec<&MigrationIndex> {
    let table = schema.tables.iter().find(|table| table.name == "account").expect("account table");
    let mut indexes = table.indexes.iter().filter(|index| index.predicate.is_some()).collect::<Vec<_>>();
    indexes.sort_by(|left, right| left.name.cmp(&right.name));
    indexes
}

async fn conditions_migrate_and_stay_stable(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "stable", MODELS, |project, url| async move {
        let output = run_cli(&project, &url, &["migrate", "generate", "--yes"]);
        assert_success(&output);
        let text = combined_output(&output);
        assert!(text.contains("where email != null"), "{text}");

        let live = inspect(backend, &url).await?;
        let indexes = partial_indexes(&live);
        let found = indexes
            .iter()
            .map(|index| {
                (
                    index.name.as_str(),
                    index.columns.iter().map(String::as_str).collect::<Vec<_>>(),
                    index.kind,
                    index.predicate.clone().expect("partial"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(found, expected_indexes());

        // A conditional `@unique` is an index of its own, not a column constraint.
        let table = live.tables.iter().find(|table| table.name == "account").expect("account table");
        let email = table.columns.iter().find(|column| column.name == "email").expect("email");
        assert!(!email.unique);

        assert_stable(&project, &url, 1);
        Ok(())
    })
    .await
}

async fn partial_uniques_only_constrain_matching_rows(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "unique", MODELS, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));
        let db = connect(backend, &url).await?;
        let insert = |id: &str, email: &str, handle: &str, slug: &str, archived: bool, deleted_at: &str| {
            format!(
                "INSERT INTO account (id, email, handle, tenant, slug, plan, archived, deleted_at, score) \
                 VALUES ('{id}', {email}, {handle}, 'acme', '{slug}', 'FREE', {}, {deleted_at}, 0)",
                if archived { "TRUE" } else { "FALSE" }
            )
        };
        let unique_violation = |result: anyhow::Result<usize>| match result {
            Ok(_) => panic!("expected a unique violation"),
            Err(error) => {
                assert_eq!(DatabaseError::new(error).constraint(), Some(DatabaseConstraintError::UniqueViolation))
            }
        };

        db.execute(&insert("a1", "'ana@dinoco.rs'", "'ana'", "home", false, "NULL")).await?;

        // (tenant, slug) only collides while `deleted_at` is NULL and the slug isn't "_".
        unique_violation(db.execute(&insert("a2", "NULL", "NULL", "home", false, "NULL")).await);
        db.execute(&insert("a3", "NULL", "NULL", "home", false, "'2026-01-01 00:00:00'")).await?;
        db.execute(&insert("a4", "NULL", "NULL", "home", false, "'2026-02-01 00:00:00'")).await?;
        db.execute(&insert("a5", "NULL", "NULL", "_", false, "NULL")).await?;
        db.execute(&insert("a6", "NULL", "NULL", "_", false, "NULL")).await?;

        // `email != null`: NULLs never collide, the same address does.
        unique_violation(db.execute(&insert("a7", "'ana@dinoco.rs'", "NULL", "other", false, "NULL")).await);

        // `!archived`: archived rows may repeat a handle, active ones can't.
        unique_violation(db.execute(&insert("a8", "NULL", "'ana'", "eight", false, "NULL")).await);
        db.execute(&insert("a9", "NULL", "'ana'", "nine", true, "NULL")).await?;
        db.execute(&insert("a10", "NULL", "'ana'", "ten", true, "NULL")).await?;

        assert_eq!(db.count("SELECT COUNT(*) FROM account").await?, 7);
        Ok(())
    })
    .await
}

async fn changed_conditions_recreate_the_index(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "changed", MODELS, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));

        let changed = MODELS
            .replace("@unique<email != null>", "@unique<email != null && email != \"root@dinoco.rs\">")
            .replace("@@uniques<deleted_at == null && slug != \"_\">([tenant, slug])", "@@uniques([tenant, slug])")
            .replace("@@indexes<plan == PRO || score >= 10>", "@@indexes<score > 0>");
        write_schema(&project, backend, &changed);
        let output = run_cli(&project, &url, &["migrate", "generate", "--yes", "--allow-destructive"]);
        assert_success(&output);
        let text = combined_output(&output);
        assert!(text.contains("where email != null && email != \"root@dinoco.rs\""), "{text}");

        let live = inspect(backend, &url).await?;
        let table = live.tables.iter().find(|table| table.name == "account").expect("account table");
        let index =
            |name: &str| table.indexes.iter().find(|index| index.name == name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(
            index("uq_account_email").predicate,
            Some(IndexPredicate::And(vec![
                compare("email", PredicateOperator::Neq, PredicateValue::Null),
                compare("email", PredicateOperator::Neq, PredicateValue::String("root@dinoco.rs".to_string())),
            ]))
        );
        assert_eq!(index("uq_account_tenant_slug").predicate, None);
        assert_eq!(
            index("idx_account_tenant_score").predicate,
            Some(compare("score", PredicateOperator::Gt, PredicateValue::Number("0".to_string())))
        );

        assert_stable(&project, &url, 2);
        Ok(())
    })
    .await
}

async fn rebuilds_keep_rows_outside_the_condition(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "rebuild", MODELS, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));
        let db = connect(backend, &url).await?;
        for (id, deleted_at) in [("r1", "'2026-01-01 00:00:00'"), ("r2", "'2026-02-01 00:00:00'"), ("r3", "NULL")] {
            db.execute(&format!(
                "INSERT INTO account (id, tenant, slug, plan, archived, deleted_at, score) \
                 VALUES ('{id}', 'acme', 'home', 'FREE', FALSE, {deleted_at}, 0)"
            ))
            .await?;
        }

        // A new default rebuilds the table on SQLite: the rows that share
        // (tenant, slug) outside the condition don't stop it.
        let changed = MODELS.replace("score      Integer   @default(0)", "score      Integer   @default(5)");
        write_schema(&project, backend, &changed);
        let output = run_cli(&project, &url, &["migrate", "generate", "--yes", "--allow-destructive"]);
        assert_success(&output);

        if backend == Backend::Sqlite {
            // The duplicate guard of the rebuild filters by the condition.
            let up = fs::read_to_string(migration_dirs(&project)[1].join("up.sql"))?;
            assert!(up.contains("DROP TABLE \"account\""), "{up}");
            assert!(
                up.contains(
                    r#"AND ("account"."deleted_at" IS NULL AND "account"."slug" <> '_') GROUP BY "account"."tenant", "account"."slug""#
                ),
                "{up}"
            );
        }
        assert_eq!(db.count("SELECT COUNT(*) FROM account").await?, 3);
        let live = inspect(backend, &url).await?;
        assert_eq!(partial_indexes(&live).len(), expected_indexes().len());
        assert_stable(&project, &url, 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_test_ambient_creates_partial_indexes() -> anyhow::Result<()> {
    let directory = std::env::temp_dir().join(format!("dinoco-partial-ambient-{}-{}", std::process::id(), monotonic()));
    fs::create_dir_all(directory.join("dinoco"))?;
    let schema = directory.join("dinoco/schema.dinoco");
    fs::write(&schema, schema_source(Backend::Sqlite, MODELS))?;

    let client = dinoco::TestAmbient::new().schema(&schema).create().await?;
    let insert = |id: &str, deleted_at: &str| {
        format!(
            "INSERT INTO account (id, tenant, slug, plan, archived, deleted_at, score) \
             VALUES ('{id}', 'acme', 'home', 'FREE', 0, {deleted_at}, 0)"
        )
    };
    let execute = |sql: String| {
        let client = &client;
        async move {
            match &client.backend {
                dinoco_engine::Backend::Sqlite(adapter) => adapter.execute(&sql, &[]).await,
                _ => unreachable!("the test ambient is SQLite"),
            }
        }
    };

    execute(insert("t1", "'2026-01-01'")).await?;
    execute(insert("t2", "'2026-01-02'")).await?;
    execute(insert("t3", "NULL")).await?;
    let error = execute(insert("t4", "NULL")).await.expect_err("duplicate among the matching rows");
    assert_eq!(DatabaseError::new(error).constraint(), Some(DatabaseConstraintError::UniqueViolation));

    let _ = fs::remove_dir_all(directory);
    Ok(())
}

fn schema_source(backend: Backend, models: &str) -> String {
    format!(
        "config {{\n    database = \"{}\"\n    database_url = env(\"DATABASE_URL\")\n}}\n{models}",
        backend.provider()
    )
}

/// Generating again finds nothing to do and `migrate run` sees no drift.
fn assert_stable(project: &Path, url: &str, migrations: usize) {
    let stable = run_cli(project, url, &["migrate", "generate"]);
    assert_success(&stable);
    let text = combined_output(&stable);
    assert!(text.contains("No schema changes were found"), "{text}");
    assert_success(&run_cli(project, url, &["migrate", "run"]));
    assert_eq!(migration_dirs(project).len(), migrations);
}

async fn with_project<F, Fut>(backend: Backend, label: &str, models: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(PathBuf, String) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let (name, url, project) = temporary_database(backend, label).await?;
    write_schema(&project, backend, models);
    let outcome = scenario(project.clone(), url).await;
    let _ = fs::remove_dir_all(&project);
    outcome.and(drop_database(backend, &name).await)
}

async fn inspect(backend: Backend, url: &str) -> anyhow::Result<DatabaseSchema> {
    connect(backend, url).await?.inspect_schema().await
}

async fn temporary_database(backend: Backend, label: &str) -> anyhow::Result<(String, String, PathBuf)> {
    let suffix =
        format!("{label}_{}_{}_{}", std::process::id(), monotonic(), NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed));
    let database_name = format!("dinoco_partial_{suffix}");
    let project = std::env::temp_dir().join(format!("dinoco-partial-indexes-{suffix}"));
    fs::create_dir(&project)?;
    let url = match backend {
        Backend::Sqlite => project.join("dev.sqlite").to_string_lossy().to_string(),
        Backend::Postgres => {
            let admin = PostgresAdapter::direct(POSTGRES_ADMIN_URL).await?;
            admin.execute(&format!("CREATE DATABASE {database_name}"), &[]).await?;
            format!("postgres://postgres:postgres@localhost:5432/{database_name}")
        }
        Backend::Mysql => {
            let admin = MySqlAdapter::new(MYSQL_ADMIN_URL);
            admin.execute(&format!("CREATE DATABASE {database_name}"), &[]).await?;
            format!("mysql://root:root@localhost:3306/{database_name}")
        }
    };
    Ok((database_name, url, project))
}

async fn drop_database(backend: Backend, name: &str) -> anyhow::Result<()> {
    match backend {
        Backend::Sqlite => {}
        Backend::Postgres => {
            let admin = PostgresAdapter::direct(POSTGRES_ADMIN_URL).await?;
            admin.execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"), &[]).await?;
        }
        Backend::Mysql => {
            MySqlAdapter::new(MYSQL_ADMIN_URL).execute(&format!("DROP DATABASE IF EXISTS {name}"), &[]).await?;
        }
    }
    Ok(())
}

async fn connect(backend: Backend, url: &str) -> anyhow::Result<CliDatabase> {
    Ok(match backend {
        Backend::Sqlite => CliDatabase::Sqlite(SqliteAdapter::new(url.to_string()).await.map_err(anyhow::Error::msg)?),
        Backend::Postgres => CliDatabase::Postgres(PostgresAdapter::direct(url).await?),
        Backend::Mysql => CliDatabase::Mysql(MySqlAdapter::new(url)),
    })
}

fn write_schema(project: &Path, backend: Backend, models: &str) {
    fs::create_dir_all(project.join("dinoco")).expect("dinoco directory");
    fs::write(project.join("dinoco/schema.dinoco"), schema_source(backend, models)).expect("schema");
}

fn run_cli(project: &Path, database_url: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(args)
        .env("DATABASE_URL", database_url)
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .current_dir(project)
        .output()
        .expect("dinoco cli")
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", combined_output(output));
}

fn combined_output(output: &Output) -> String {
    format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn migration_dirs(project: &Path) -> Vec<PathBuf> {
    let mut migrations = fs::read_dir(project.join("dinoco/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("migration entry").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    migrations.sort();
    migrations
}

fn monotonic() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos()
}
