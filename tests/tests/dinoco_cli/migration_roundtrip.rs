//! A database migrated by the CLI matches the schema it was migrated to, so
//! generating again finds nothing to do, on every backend:
//!
//! - string, enum and number defaults read back from the database. PostgreSQL
//!   reports them with a cast (`'USER'::"Role"`, `'-3'::integer`) and MySQL 8
//!   without quotes (`USER`);
//! - tables and columns named after SQL keywords (`user`, `order`, `group`,
//!   `key`, `desc`, `end`, `values`, `to`), created, altered with rows in them
//!   (a table rebuild on SQLite) and replayed by `migrate run`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_cli::db::{CliDatabase, DatabaseSchema};
use dinoco_engine::{DinocoAdapter, MigrationDefault, MySqlAdapter, PostgresAdapter, SqliteAdapter};

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
    fresh_database_matches_the_schema => fresh_database_matches_the_schema,
    changed_defaults_migrate_and_stay_stable => changed_defaults_migrate_and_stay_stable,
    defaults_on_a_nullable_and_a_new_column => defaults_on_a_nullable_and_a_new_column,
    reserved_names_migrate_and_stay_stable => reserved_names_migrate_and_stay_stable,
    rows_that_do_not_fit_stop_before_anything_changes => rows_that_do_not_fit_stop_before_anything_changes,
}

#[tokio::test]
async fn postgres_snapshot_without_literal_defaults_reports_no_drift() -> anyhow::Result<()> {
    legacy_snapshot_reports_no_drift(Backend::Postgres).await
}

#[tokio::test]
async fn mysql_snapshot_without_literal_defaults_reports_no_drift() -> anyhow::Result<()> {
    legacy_snapshot_reports_no_drift(Backend::Mysql).await
}

const MODELS: &str = r#"
enum Role {
    USER
    ADMIN
}

model Account {
    id        Integer @id @default(autoincrement())
    role      Role    @default(USER)
    nickname  String  @default("anon")
    quoted    String  @default("it's (ok)")
    casted    String  @default("a::b")
    empty     String  @default("")
    negative  Integer @default(-3)
    positive  Integer @default(7)
    ratio     Float   @default(-0.5)
    score     Float   @default(1.5)
    whole     Float   @default(0)
    minus     Float   @default(-2)
    active    Boolean @default(true)
    joined_at DateTime @default(now())
}
"#;

fn schema_source(backend: Backend, models: &str) -> String {
    format!(
        "config {{\n    database = \"{}\"\n    database_url = env(\"DATABASE_URL\")\n}}\n{models}",
        backend.provider()
    )
}

async fn fresh_database_matches_the_schema(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "fresh", MODELS, |project, url| async move {
        let output = run_cli(&project, &url, &["migrate", "generate", "--yes"]);
        assert_success(&output);
        let text = combined_output(&output);
        assert!(!text.contains("drift"), "{text}");

        let live = inspect(backend, &url).await?;
        assert_eq!(default_of(&live, "role"), Some(MigrationDefault::String("USER".into())));
        assert_eq!(default_of(&live, "nickname"), Some(MigrationDefault::String("anon".into())));
        assert_eq!(default_of(&live, "quoted"), Some(MigrationDefault::String("it's (ok)".into())));
        assert_eq!(default_of(&live, "casted"), Some(MigrationDefault::String("a::b".into())));
        assert_eq!(default_of(&live, "empty"), Some(MigrationDefault::String(String::new())));
        assert_eq!(default_of(&live, "negative"), Some(MigrationDefault::Integer(-3)));
        assert_eq!(default_of(&live, "positive"), Some(MigrationDefault::Integer(7)));
        assert_eq!(default_of(&live, "ratio"), Some(MigrationDefault::Float(-0.5)));
        assert_eq!(default_of(&live, "score"), Some(MigrationDefault::Float(1.5)));
        // A whole number on a Float field reads back as an integer on SQLite
        // and as a float on PostgreSQL and MySQL; both are the same default.
        assert!(
            matches!(default_of(&live, "whole"), Some(MigrationDefault::Integer(0) | MigrationDefault::Float(0.0))),
            "{:?}",
            default_of(&live, "whole")
        );
        assert!(
            matches!(default_of(&live, "minus"), Some(MigrationDefault::Integer(-2) | MigrationDefault::Float(-2.0))),
            "{:?}",
            default_of(&live, "minus")
        );
        assert_eq!(default_of(&live, "active"), Some(MigrationDefault::Boolean(true)));
        assert_eq!(default_of(&live, "joined_at"), Some(MigrationDefault::CurrentTimestamp));

        assert_stable(&project, &url, 1);
        Ok(())
    })
    .await
}

async fn changed_defaults_migrate_and_stay_stable(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "changed", MODELS, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));

        let changed = MODELS
            .replace("@default(USER)", "@default(ADMIN)")
            .replace("@default(\"anon\")", "@default(\"guest\")")
            .replace("@default(-3)", "@default(-10)");
        write_schema(&project, backend, &changed);
        let output = run_cli(&project, &url, &["migrate", "generate", "--yes", "--allow-destructive"]);
        assert_success(&output);

        let live = inspect(backend, &url).await?;
        assert_eq!(default_of(&live, "role"), Some(MigrationDefault::String("ADMIN".into())));
        assert_eq!(default_of(&live, "nickname"), Some(MigrationDefault::String("guest".into())));
        assert_eq!(default_of(&live, "negative"), Some(MigrationDefault::Integer(-10)));
        assert_stable(&project, &url, 2);
        Ok(())
    })
    .await
}

async fn defaults_on_a_nullable_and_a_new_column(backend: Backend) -> anyhow::Result<()> {
    let models = "model Note {\n    id   Integer @id @default(autoincrement())\n    body String?\n}\n";
    with_project(backend, "added", models, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));
        let db = connect(backend, &url).await?;
        db.execute("INSERT INTO note (body) VALUES ('kept')").await?;
        drop(db);

        let added = "model Note {\n    id    Integer @id @default(autoincrement())\n    body  String? @default(\"none\")\n    title String  @default(\"untitled\")\n}\n";
        write_schema(&project, backend, added);
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes", "--allow-destructive"]));

        let live = inspect(backend, &url).await?;
        assert_eq!(default_of(&live, "body"), Some(MigrationDefault::String("none".into())));
        assert_eq!(default_of(&live, "title"), Some(MigrationDefault::String("untitled".into())));
        assert_stable(&project, &url, 2);
        Ok(())
    })
    .await
}

const RESERVED_MODELS: &str = r#"
enum Status {
    OPEN
    CLOSED
}

model User {
    id     Integer   @id @default(autoincrement())
    key    String    @unique
    desc   String?
    end    DateTime?
    range  Integer   @default(-1)
    status Status    @default(OPEN)
    orders Order[]
    groups Group[]

    @@indexes([range, end])
}

model Order {
    id      Integer  @id @default(autoincrement())
    values  String   @default("x")
    to      String?
    user_id Integer?
    user    User?    @relation(fields: [user_id], references: [id], onDelete: Cascade)
}

model Group {
    id    Integer @id @default(autoincrement())
    rank  Integer
    users User[]
}
"#;

async fn reserved_names_migrate_and_stay_stable(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "reserved", RESERVED_MODELS, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));
        assert_stable(&project, &url, 1);

        let (user, order, group) = match backend {
            Backend::Mysql => ("`user`", "`order`", "`group`"),
            _ => ("\"user\"", "\"order\"", "\"group\""),
        };
        let db = connect(backend, &url).await?;
        let key = if backend == Backend::Mysql { "`key`" } else { "\"key\"" };
        let desc = if backend == Backend::Mysql { "`desc`" } else { "\"desc\"" };
        db.execute(&format!("INSERT INTO {user} ({key}, {desc}) VALUES ('a', 'first')")).await?;
        db.execute(&format!("INSERT INTO {order} (user_id) SELECT MAX(id) FROM {user}")).await?;
        let rank = if backend == Backend::Mysql { "`rank`" } else { "\"rank\"" };
        db.execute(&format!("INSERT INTO {group} ({rank}) VALUES (1)")).await?;
        drop(db);

        let changed = RESERVED_MODELS
            .replace("    desc   String?\n", "    desc   String    @default(\"\")\n")
            .replace("    to      String?\n", "")
            .replace(
                "    values  String   @default(\"x\")\n",
                "    values  String   @default(\"y\")\n    rank    Float    @default(0.5)\n",
            )
            .replace("    end    DateTime?\n", "    finish DateTime?\n")
            .replace("@@indexes([range, end])", "@@indexes([range, finish])")
            .replace("    rank  Integer\n", "    rank  Integer\n    label String  @default(\"g\")\n");
        write_schema(&project, backend, &changed);
        let output = run_cli(
            &project,
            &url,
            &["migrate", "generate", "--yes", "--allow-destructive", "--rename", "user.end=finish"],
        );
        assert_success(&output);
        assert_stable(&project, &url, 2);

        let db = connect(backend, &url).await?;
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {user} WHERE {key} = 'a'")).await?, 1, "rows are kept");
        assert_eq!(db.count(&format!("SELECT COUNT(*) FROM {order} WHERE user_id IS NOT NULL")).await?, 1);
        drop(db);
        let live = inspect(backend, &url).await?;
        assert_eq!(default_of(&live, "label"), Some(MigrationDefault::String("g".into())));
        assert!(live.tables.iter().any(|table| table.columns.iter().any(|column| column.name == "finish")));

        // The same migrations replayed on an empty database.
        let (replay_name, replay_url, _replay_project) = temporary_database(backend, "reserved_replay").await?;
        let outcome = async {
            assert_success(&run_cli(&project, &replay_url, &["migrate", "run"]));
            let stable = run_cli(&project, &replay_url, &["migrate", "generate"]);
            assert_success(&stable);
            assert!(combined_output(&stable).contains("No schema changes were found"), "{}", combined_output(&stable));
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let _ = fs::remove_dir_all(&_replay_project);
        outcome.and(drop_database(backend, &replay_name).await)
    })
    .await
}

/// A column made required while rows hold NULL in it, or a required column
/// without a default added to a table with rows, would fail halfway (MySQL
/// commits each DDL statement), so the CLI stops before writing anything.
async fn rows_that_do_not_fit_stop_before_anything_changes(backend: Backend) -> anyhow::Result<()> {
    let models = "model Note {\n    id   Integer @id @default(autoincrement())\n    body String?\n}\n";
    with_project(backend, "rows", models, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));
        let db = connect(backend, &url).await?;
        db.execute("INSERT INTO note (body) VALUES (NULL)").await?;
        db.execute("INSERT INTO note (body) VALUES ('kept')").await?;
        drop(db);

        for changed in [
            "model Note {\n    id    Integer @id @default(autoincrement())\n    body  String\n    extra String?\n}\n",
            "model Note {\n    id    Integer @id @default(autoincrement())\n    body  String?\n    title String\n}\n",
        ] {
            write_schema(&project, backend, changed);
            let output = run_cli(&project, &url, &["migrate", "generate", "--yes", "--allow-destructive"]);
            assert!(!output.status.success(), "{}", combined_output(&output));
            let text = combined_output(&output);
            // SQLite refuses to rebuild a table into a required column it cannot fill on its own.
            let sqlite_rebuild = backend == Backend::Sqlite && text.contains("SQLite cannot populate");
            assert!(
                sqlite_rebuild
                    || (text.contains("The migration cannot be applied to the rows this database already holds.")
                        && text.contains("Nothing was written or changed.")),
                "{text}"
            );
            assert!(
                sqlite_rebuild
                    || text.contains("`note.body` becomes required, but 1 row holds NULL")
                    || text.contains("`note.title` is added as required without a default to a table with 2 rows"),
                "{text}"
            );
            assert_eq!(migration_dirs(&project).len(), 1, "no migration was written");

            let live = inspect(backend, &url).await?;
            let note = live.tables.iter().find(|table| table.name == "note").expect("note");
            assert!(note.columns.iter().any(|column| column.name == "body" && column.nullable), "{note:?}");
            assert!(!note.columns.iter().any(|column| column.name == "extra" || column.name == "title"), "{note:?}");
        }

        // Once the rows are filled in, the same change applies.
        let db = connect(backend, &url).await?;
        db.execute("UPDATE note SET body = 'filled' WHERE body IS NULL").await?;
        drop(db);
        write_schema(
            &project,
            backend,
            "model Note {\n    id    Integer @id @default(autoincrement())\n    body  String\n    extra String?\n}\n",
        );
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes", "--allow-destructive"]));
        assert_stable(&project, &url, 2);

        // --create-only never reads rows, since nothing is applied: `extra` is
        // NULL in every row.
        write_schema(
            &project,
            backend,
            "model Note {\n    id    Integer @id @default(autoincrement())\n    body  String\n    extra String\n}\n",
        );
        assert_success(&run_cli(
            &project,
            &url,
            &["migrate", "generate", "--yes", "--allow-destructive", "--create-only"],
        ));
        assert_eq!(migration_dirs(&project).len(), 3);
        Ok(())
    })
    .await
}

/// A snapshot that `migrate run` recorded before 2.1.6 has no string, enum
/// or negative number defaults, because they were not read back.
async fn legacy_snapshot_reports_no_drift(backend: Backend) -> anyhow::Result<()> {
    with_project(backend, "legacy", MODELS, |project, url| async move {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));

        let db = connect(backend, &url).await?;
        let recorded = db.migration_metadata().await?.schema_snapshots.expect("snapshots");
        let (name, snapshot) = recorded.into_iter().next().expect("one snapshot");
        let mut snapshot: serde_json::Value = serde_json::from_str(&snapshot)?;
        assert_eq!(snapshot.get(dinoco_cli::db::SNAPSHOT_LITERAL_DEFAULTS_KEY), Some(&serde_json::Value::Bool(true)));
        snapshot.as_object_mut().expect("object").remove(dinoco_cli::db::SNAPSHOT_LITERAL_DEFAULTS_KEY);
        let mut legacy: DatabaseSchema = serde_json::from_value(snapshot)?;
        for column in legacy.tables.iter_mut().flat_map(|table| &mut table.columns) {
            let unread = match &column.default {
                Some(MigrationDefault::String(_)) => true,
                Some(MigrationDefault::Integer(value)) => *value < 0,
                Some(MigrationDefault::Float(value)) => *value < 0.0,
                _ => false,
            };
            if unread {
                column.default = None;
            }
        }
        let legacy_json = serde_json::to_string(&legacy)?.replace('\'', "''");
        db.execute(&format!("UPDATE dinoco_migration_schemas SET schema_json = '{legacy_json}' WHERE name = '{name}'"))
            .await?;
        drop(db);

        for args in [&["migrate", "run"][..], &["migrate", "generate"][..]] {
            let output = run_cli(&project, &url, args);
            assert_success(&output);
            let text = combined_output(&output);
            assert!(!text.contains("drift"), "{text}");
        }
        assert_eq!(migration_dirs(&project).len(), 1);

        // The next migration records a snapshot that has them.
        write_schema(&project, backend, &MODELS.replace("    joined_at", "    extra     Integer?\n    joined_at"));
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));
        let db = connect(backend, &url).await?;
        let snapshots = db.migration_metadata().await?.schema_snapshots.expect("snapshots");
        let latest = snapshots.into_iter().max_by(|left, right| left.0.cmp(&right.0)).expect("latest");
        let latest: serde_json::Value = serde_json::from_str(&latest.1)?;
        assert!(latest.get(dinoco_cli::db::SNAPSHOT_LITERAL_DEFAULTS_KEY).is_some());
        let latest: DatabaseSchema = serde_json::from_value(latest)?;
        assert_eq!(default_of(&latest, "role"), Some(MigrationDefault::String("USER".into())));
        Ok(())
    })
    .await
}

fn default_of(schema: &DatabaseSchema, column: &str) -> Option<MigrationDefault> {
    schema
        .tables
        .iter()
        .flat_map(|table| &table.columns)
        .find(|item| item.name == column)
        .unwrap_or_else(|| panic!("column {column}"))
        .default
        .clone()
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
    let database_name = format!("dinoco_defaults_{suffix}");
    let project = std::env::temp_dir().join(format!("dinoco-literal-defaults-{suffix}"));
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
