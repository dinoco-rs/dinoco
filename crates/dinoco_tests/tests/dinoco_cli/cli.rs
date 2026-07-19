use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::{
    DinocoAdapter, DinocoSqlCompiler, DinocoValue, InsertQuery, MigrationColumnType, SqliteAdapter,
    rusqlite::Connection,
};
use dinoco_tests::{column, create_table, primary};

#[test]
fn init_creates_english_colored_schema() {
    let project = temp_project("init");
    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .arg("init")
        .env("DINOCO_CLI_INIT_DATABASE", "postgresql")
        .env("DINOCO_CLI_INIT_POSTGRES_CONNECTION", "pgbouncer")
        .current_dir(&project)
        .output()
        .expect("cli should run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let schema = fs::read_to_string(project.join("dinoco/schema.dinoco")).expect("schema");

    assert!(stdout.contains("Dinoco project initialized"));
    assert!(stdout.contains("DATABASE_URL"));
    assert!(schema.contains("database"));
    assert!(schema.contains("\"postgresql\""));
    assert!(schema.contains("connection"));
    assert!(schema.contains("\"pgbouncer\""));
    assert!(schema.contains("database_url"));
    assert!(schema.contains("env(\"DATABASE_URL\")"));
}

#[test]
fn migrate_generate_sqlite_creates_migration_and_models() {
    let project = temp_project("migrate");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA).expect("schema");

    let db_path = project.join("dev.sqlite");
    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .current_dir(&project)
        .output()
        .expect("cli should run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Migration generated and applied"));
    assert!(stdout.contains("Rust models generated"));
    assert!(project.join("dinoco/mod.rs").exists());
    assert!(project.join("dinoco/models/mod.rs").exists());
    assert!(project.join("dinoco/models/user.rs").exists());

    let migration_dirs = fs::read_dir(project.join("dinoco/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("migration entry").path())
        .collect::<Vec<_>>();
    let migrations = migration_dirs.len();
    assert_eq!(migrations, 1);
    assert!(migration_dirs[0].join("up.sql").exists());
    assert!(migration_dirs[0].join("down.sql").exists());
}

#[tokio::test]
async fn migrate_generate_detects_and_applies_destructive_column_drop_when_confirmed() {
    let project = temp_project("drop-column");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA).expect("schema");

    let db_path = project.join("dev.sqlite");
    let adapter = SqliteAdapter::new(db_path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg).unwrap();
    adapter.execute(&adapter.compile_create_migrations_table(), &[]).await.expect("migrations table");
    create_table(
        &adapter,
        "user",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("email", MigrationColumnType::String),
            column("password", MigrationColumnType::String),
        ],
    )
    .await
    .expect("user table");

    let (insert_sql, insert_params) = adapter.compile_insert_query(InsertQuery {
        table: "user",
        fields: vec!["id", "email", "password"],
        rows: vec![vec![
            DinocoValue::String("1".to_string()),
            DinocoValue::String("a@dinoco.rs".to_string()),
            DinocoValue::String("secret".to_string()),
        ]],
        returning: None,
    });
    adapter.execute(&insert_sql, &insert_params).await.expect("seed user");
    drop(adapter);

    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .env("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")
        .current_dir(&project)
        .output()
        .expect("cli should run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Destructive change"));
    assert!(stdout.contains("Drop column `user.password`"));

    let conn = Connection::open(&db_path).expect("sqlite");
    let columns = conn
        .prepare("PRAGMA table_info(user)")
        .expect("pragma")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names");

    assert!(!columns.iter().any(|column| column == "password"));
}

const SQLITE_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id    String @id @default(uuid())
    email String
}
"#;

fn temp_project(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("dinoco-cli-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).expect("temp project");
    path
}
