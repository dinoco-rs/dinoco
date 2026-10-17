//! Generated index, constraint and pivot table names longer than the database
//! keeps: PostgreSQL truncates them to 63 bytes, MySQL rejects them over 64
//! characters, SQLite keeps them whole.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_cli::db::{CliDatabase, DatabaseSchema};
use dinoco_cli::sql::{
    desired_database_schema, fit_identifier, fit_schema_identifiers, generate_create_table_migrations,
    plan_database_migration, plan_schema_migration,
};
use dinoco_compiler::IdentifierLimit;
use dinoco_engine::{DinocoAdapter, MySqlAdapter, PostgresAdapter, SqliteAdapter};

const POSTGRES_ADMIN_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_ADMIN_URL: &str = "mysql://root:root@localhost:3306/mysql";
static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

const LONG_INDEX: &str = "idx_business_receivable_transaction_business_id_executed_at_unlock_at";
const POSTGRES_INDEX: &str = "idx_business_receivable_transaction_business_id_executed_at_unl";
const MYSQL_INDEX: &str = "idx_business_receivable_transaction_business_id_executed_at_unlo";
const LONG_PIVOT: &str = "_business_receivable_category_to_business_receivable_transaction";

#[derive(Clone, Copy, PartialEq)]
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

    fn limit(self) -> Option<IdentifierLimit> {
        match self {
            Backend::Sqlite => None,
            Backend::Postgres => Some(IdentifierLimit::POSTGRES),
            Backend::Mysql => Some(IdentifierLimit::MYSQL),
        }
    }
}

fn schema_source(backend: Backend) -> String {
    format!(
        r#"
config {{
    database = "{}"
    database_url = env("DATABASE_URL")
}}

model BusinessReceivableCategory {{
    id           Integer                         @id @default(autoincrement())
    transactions BusinessReceivableTransaction[]
}}

model BusinessReceivableTransaction {{
    id          Integer                         @id @default(autoincrement())
    business_id Integer
    executed_at Integer?
    unlock_at   Integer
    unlock_amount Integer
    categories  BusinessReceivableCategory[]
    settlements BusinessReceivableTransactionSettlement[]

    @@indexes([business_id, executed_at, unlock_at])
    @@indexes([business_id, executed_at, unlock_amount])
}}

model BusinessReceivableTransactionSettlement {{
    id                                  Integer                        @id @default(autoincrement())
    business_receivable_transaction_ref Integer
    business_receivable_transaction     BusinessReceivableTransaction? @relation(fields: [business_receivable_transaction_ref], references: [id])
    settled_at_for_the_business_receivable_transaction Integer @index(map: "idx_business_receivable_transaction_settlement_settled_at_for_the_business")
}}
"#,
        backend.provider()
    )
}

fn compile(backend: Backend) -> dinoco_compiler::Schema {
    dinoco_compiler::compile(&schema_source(backend)).expect("schema")
}

fn index_names(schema: &DatabaseSchema, table: &str) -> Vec<String> {
    let table = schema.tables.iter().find(|item| item.name == table).unwrap_or_else(|| panic!("table {table}"));
    table.indexes.iter().map(|index| index.name.clone()).collect()
}

fn foreign_key_names(schema: &DatabaseSchema, table: &str) -> Vec<String> {
    let table = schema.tables.iter().find(|item| item.name == table).unwrap_or_else(|| panic!("table {table}"));
    table.foreign_keys.iter().map(|key| key.name.clone()).collect()
}

fn assert_fits(name: &str, backend: Backend) {
    match backend.limit() {
        Some(IdentifierLimit::Bytes(max)) => assert!(name.len() <= max, "{name} has {} bytes", name.len()),
        Some(IdentifierLimit::Chars(max)) => {
            assert!(name.chars().count() <= max, "{name} has {} chars", name.chars().count())
        }
        None => {}
    }
}

#[test]
fn postgres_truncates_generated_names_to_63_bytes_like_postgres_does() {
    let desired = desired_database_schema(&compile(Backend::Postgres));
    let indexes = index_names(&desired, "business_receivable_transaction");

    assert!(indexes.contains(&POSTGRES_INDEX.to_string()), "{indexes:?}");
    assert_eq!(POSTGRES_INDEX, &LONG_INDEX[..63]);
    for name in desired.tables.iter().flat_map(|table| {
        table.indexes.iter().map(|index| &index.name).chain(table.foreign_keys.iter().map(|key| &key.name))
    }) {
        assert_fits(name, Backend::Postgres);
    }
}

#[test]
fn mysql_truncates_generated_names_to_64_characters() {
    let desired = desired_database_schema(&compile(Backend::Mysql));
    let indexes = index_names(&desired, "business_receivable_transaction");

    assert!(indexes.contains(&MYSQL_INDEX.to_string()), "{indexes:?}");
    for name in desired.tables.iter().flat_map(|table| {
        table.indexes.iter().map(|index| &index.name).chain(table.foreign_keys.iter().map(|key| &key.name))
    }) {
        assert_fits(name, Backend::Mysql);
    }
}

#[test]
fn sqlite_keeps_generated_names_whole() {
    let desired = desired_database_schema(&compile(Backend::Sqlite));
    let indexes = index_names(&desired, "business_receivable_transaction");

    assert!(indexes.contains(&LONG_INDEX.to_string()), "{indexes:?}");
    assert!(desired.tables.iter().any(|table| table.name == LONG_PIVOT));
    assert!(
        foreign_key_names(&desired, "business_receivable_transaction_settlement")
            .contains(&"fk_business_receivable_transaction_settlement_business_receivable_transaction_ref".to_string())
    );
}

#[test]
fn names_equal_after_truncation_get_a_suffix_within_the_limit() {
    for backend in [Backend::Postgres, Backend::Mysql] {
        let desired = desired_database_schema(&compile(backend));
        let indexes = index_names(&desired, "business_receivable_transaction");
        let first = fit_identifier(LONG_INDEX.to_string(), backend.limit());
        let second = indexes
            .iter()
            .find(|name| name.ends_with("_2"))
            .unwrap_or_else(|| panic!("no suffixed index in {indexes:?}"));

        assert!(indexes.contains(&first));
        assert_fits(second, backend);
        assert_eq!(second.len(), first.len(), "the suffix replaces the end instead of growing the name");
        assert_eq!(indexes.iter().filter(|name| **name == first).count(), 1);
    }
}

#[test]
fn foreign_keys_explicit_maps_and_pivot_tables_are_truncated() {
    for backend in [Backend::Postgres, Backend::Mysql] {
        let desired = desired_database_schema(&compile(backend));
        let settlement = "business_receivable_transaction_settlement";

        let keys = foreign_key_names(&desired, settlement);
        assert_eq!(keys.len(), 1);
        assert_eq!(
            keys[0],
            fit_identifier(
                "fk_business_receivable_transaction_settlement_business_receivable_transaction_ref".to_string(),
                backend.limit()
            )
        );
        assert_fits(&keys[0], backend);

        let mapped = fit_identifier(
            "idx_business_receivable_transaction_settlement_settled_at_for_the_business".to_string(),
            backend.limit(),
        );
        assert!(index_names(&desired, settlement).contains(&mapped));

        let pivot = fit_identifier(LONG_PIVOT.to_string(), backend.limit());
        let table = desired.tables.iter().find(|table| table.name == pivot).expect("pivot table");
        for column in &table.columns {
            assert_fits(&column.name, backend);
        }
        for key in &table.foreign_keys {
            assert_fits(&key.name, backend);
        }
    }
}

#[test]
fn generated_models_use_the_truncated_pivot_table() {
    for backend in [Backend::Sqlite, Backend::Postgres, Backend::Mysql] {
        let schema = compile(backend);
        let model = schema.models().find(|model| model.name == "BusinessReceivableTransaction").expect("model");
        let rendered = dinoco_codegen::render_model_file(model, &schema);
        let pivot = fit_identifier(LONG_PIVOT.to_string(), backend.limit());
        let migrations = generate_create_table_migrations(&schema);

        assert!(rendered.contains(&format!("join_table = \"{pivot}\"")), "{rendered}");
        assert!(migrations.iter().any(|migration| migration.table == pivot));
    }
}

#[test]
fn fit_identifier_cuts_on_a_character_boundary() {
    let name = format!("{}é_suffix", "a".repeat(62));

    assert_eq!(fit_identifier(name.clone(), Some(IdentifierLimit::POSTGRES)), "a".repeat(62));
    assert_eq!(fit_identifier(name.clone(), Some(IdentifierLimit::MYSQL)), format!("{}é_", "a".repeat(62)));
    assert_eq!(fit_identifier(name.clone(), None), name);
    assert_eq!(fit_identifier(String::new(), Some(IdentifierLimit::POSTGRES)), "");
    assert_eq!(fit_identifier("short".to_string(), Some(IdentifierLimit::MYSQL)), "short");
}

#[test]
fn identifier_limit_follows_the_configured_database() {
    assert_eq!(compile(Backend::Postgres).identifier_limit(), Some(IdentifierLimit::POSTGRES));
    assert_eq!(compile(Backend::Mysql).identifier_limit(), Some(IdentifierLimit::MYSQL));
    assert_eq!(compile(Backend::Sqlite).identifier_limit(), None);
    assert_eq!(dinoco_compiler::compile("model A {\n id Integer @id\n}").expect("schema").identifier_limit(), None);
}

/// A snapshot written before 2.1.5 holds the full names, while PostgreSQL
/// stored them truncated. Fitting the snapshot removes the phantom drift.
/// (Names that collide once truncated never reached a 2.1.4 database:
/// PostgreSQL refused the second one.)
#[test]
fn legacy_postgres_snapshot_matches_the_truncated_database_after_fitting() {
    let source = |database: &str| {
        format!(
            r#"
config {{
    database = "{database}"
    database_url = env("DATABASE_URL")
}}

model BusinessReceivableTransaction {{
    id          Integer @id @default(autoincrement())
    business_id Integer
    executed_at Integer?
    unlock_at   Integer

    @@indexes([business_id, executed_at, unlock_at])
}}
"#
        )
    };
    let schema = dinoco_compiler::compile(&source("postgresql")).expect("schema");
    let live = desired_database_schema(&schema);
    let mut legacy = desired_database_schema(&dinoco_compiler::compile(&source("sqlite")).expect("schema"));
    assert!(index_names(&legacy, "business_receivable_transaction").contains(&LONG_INDEX.to_string()));

    let drift = plan_database_migration(&legacy, &live);
    assert!(!drift.steps.is_empty(), "the unfitted snapshot reports drift");

    fit_schema_identifiers(&mut legacy, Some(IdentifierLimit::POSTGRES));
    let drift = plan_database_migration(&legacy, &live);
    assert!(drift.steps.is_empty(), "{:#?}", drift.steps);
    assert!(plan_schema_migration(&schema, &live).steps.is_empty());

    let mut unchanged = legacy.clone();
    fit_schema_identifiers(&mut unchanged, None);
    assert_eq!(format!("{unchanged:?}"), format!("{legacy:?}"));
}

#[tokio::test]
async fn sqlite_migrations_keep_long_names_and_stay_stable() -> anyhow::Result<()> {
    exercise_migrations(Backend::Sqlite).await
}

#[tokio::test]
async fn postgres_migrations_create_truncated_names_and_stay_stable() -> anyhow::Result<()> {
    exercise_migrations(Backend::Postgres).await
}

#[tokio::test]
async fn mysql_migrations_create_truncated_names_and_stay_stable() -> anyhow::Result<()> {
    exercise_migrations(Backend::Mysql).await
}

#[tokio::test]
async fn postgres_snapshot_with_full_names_from_before_2_1_5_reports_no_drift() -> anyhow::Result<()> {
    let (name, url, project) = temporary_database(Backend::Postgres, "legacy_snapshot").await?;
    write_schema(&project, Backend::Postgres);

    let outcome = async {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));
        let db = connect(Backend::Postgres, &url).await?;
        // Rewrite the recorded snapshot the way 2.1.4 wrote it: with the full name.
        db.execute(&format!(
            "UPDATE dinoco_migration_schemas SET schema_json = REPLACE(schema_json, '\"{POSTGRES_INDEX}\"', '\"{LONG_INDEX}\"')"
        ))
        .await?;
        let snapshot = db.count(&format!(
            "SELECT COUNT(*) FROM dinoco_migration_schemas WHERE schema_json LIKE '%\"{LONG_INDEX}\"%'"
        ))
        .await?;
        assert_eq!(snapshot, 1, "the snapshot was rewritten");
        drop(db);

        for args in [&["migrate", "run"][..], &["migrate", "generate"][..]] {
            let output = run_cli(&project, &url, args);
            assert_success(&output);
            let text = combined_output(&output);
            assert!(!text.contains("Missing index"), "{text}");
            assert!(!text.contains("Unexpected index"), "{text}");
        }
        assert_eq!(migration_dirs(&project).len(), 1);
        Ok::<_, anyhow::Error>(())
    }
    .await;

    let _ = fs::remove_dir_all(&project);
    outcome.and(drop_database(Backend::Postgres, &name).await)
}

async fn exercise_migrations(backend: Backend) -> anyhow::Result<()> {
    let (name, url, project) = temporary_database(backend, "names").await?;
    write_schema(&project, backend);

    let outcome = async {
        assert_success(&run_cli(&project, &url, &["migrate", "generate", "--yes"]));

        let db = connect(backend, &url).await?;
        let live = db.inspect_schema().await?;
        let desired = desired_database_schema(&compile(backend));
        let expected = fit_identifier(LONG_INDEX.to_string(), backend.limit());
        assert!(index_names(&live, "business_receivable_transaction").contains(&expected));
        // SQLite reports synthesized foreign key names, so the CLI compares it
        // against a shadow database instead; the stable run below covers it.
        if backend != Backend::Sqlite {
            let drift = plan_database_migration(&desired, &live);
            assert!(drift.steps.is_empty(), "{:#?}", drift.steps);
        }

        // The pivot table the models query is the one the migration created.
        let pivot = fit_identifier(LONG_PIVOT.to_string(), backend.limit());
        assert!(live.tables.iter().any(|table| table.name == pivot), "missing pivot {pivot}");
        drop(db);

        let stable = run_cli(&project, &url, &["migrate", "generate"]);
        assert_success(&stable);
        let text = combined_output(&stable);
        assert!(text.contains("No schema changes were found"), "{text}");
        assert_eq!(migration_dirs(&project).len(), 1);

        let run = run_cli(&project, &url, &["migrate", "run"]);
        assert_success(&run);
        Ok::<_, anyhow::Error>(())
    }
    .await;

    let _ = fs::remove_dir_all(&project);
    outcome.and(drop_database(backend, &name).await)
}

async fn temporary_database(backend: Backend, label: &str) -> anyhow::Result<(String, String, PathBuf)> {
    let suffix =
        format!("{label}_{}_{}_{}", std::process::id(), monotonic(), NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed));
    let database_name = format!("dinoco_ident_{suffix}");
    let project = std::env::temp_dir().join(format!("dinoco-identifier-limits-{suffix}"));
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

fn write_schema(project: &Path, backend: Backend) {
    fs::create_dir_all(project.join("dinoco")).expect("dinoco directory");
    fs::write(project.join("dinoco/schema.dinoco"), schema_source(backend)).expect("schema");
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
