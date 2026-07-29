use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::{DinocoAdapter, DinocoSqlCompiler, DinocoValue, InsertQuery, SqliteAdapter, rusqlite::Connection};

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

#[test]
fn migrate_generate_sqlite_creates_explicit_and_foreign_key_indexes() {
    let project = temp_project("indexes");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_INDEX_SCHEMA).expect("schema");

    let db_path = project.join("dev.sqlite");
    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .current_dir(&project)
        .output()
        .expect("cli should run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let migration_dir = fs::read_dir(project.join("dinoco/migrations"))
        .expect("migrations")
        .next()
        .expect("migration directory")
        .expect("migration entry")
        .path();
    let up = fs::read_to_string(migration_dir.join("up.sql")).expect("up migration");
    assert!(up.contains("CREATE INDEX idx_post_title ON post (title);"), "{up}");
    assert!(up.contains("CREATE INDEX idx_post_user_id ON post (user_id);"), "{up}");
    assert!(!up.contains("idx_post_summary_fulltext"), "SQLite must not create a native full-text index: {up}");
    assert!(!up.contains("CREATE INDEX idx_user_id "), "the user primary key already owns an index: {up}");
    assert!(!up.contains("CREATE INDEX idx_post_id "), "the post primary key already owns an index: {up}");
    let generated_post = fs::read_to_string(project.join("dinoco/models/post.rs")).expect("generated post model");
    assert!(generated_post.contains("#[dinoco(fulltext)]\n    pub summary: String"));

    let conn = Connection::open(&db_path).expect("sqlite");
    let indexes = conn
        .prepare("SELECT name FROM pragma_index_list('post') WHERE [unique] = 0 ORDER BY name")
        .expect("index introspection")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("indexes")
        .collect::<Result<Vec<_>, _>>()
        .expect("index names");
    assert_eq!(indexes, ["idx_post_title", "idx_post_user_id"]);
}

#[test]
fn migrate_generate_sqlite_applies_composite_model_attributes() {
    let project = temp_project("composite-indexes");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_COMPOSITE_SCHEMA).expect("schema");

    let db_path = project.join("dev.sqlite");
    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .current_dir(&project)
        .output()
        .expect("cli should run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let migration_dir = fs::read_dir(project.join("dinoco/migrations"))
        .expect("migrations")
        .next()
        .expect("migration directory")
        .expect("migration entry")
        .path();
    let up = fs::read_to_string(migration_dir.join("up.sql")).expect("up migration");
    assert!(up.contains("PRIMARY KEY (tenant_id, id)"), "{up}");
    assert!(
        up.contains("CREATE UNIQUE INDEX uq_search_articles_tenant_id_slug ON search_articles (tenant_id, slug);"),
        "{up}"
    );
    assert!(
        up.contains("CREATE INDEX idx_search_articles_tenant_id_category ON search_articles (tenant_id, category);"),
        "{up}"
    );
    assert!(!up.contains("title_body_fulltext"), "SQLite must use the query fallback: {up}");

    let generated = fs::read_to_string(project.join("dinoco/models/article.rs")).expect("generated article");
    assert!(generated.contains("#[dinoco(table_name = \"search_articles\")]"));
    assert!(generated.contains("#[dinoco(fulltext = \"title,body\")]"));
}

#[tokio::test]
async fn migrate_generate_detects_and_applies_destructive_column_drop_when_confirmed() {
    let project = temp_project("drop-column");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA_WITH_PASSWORD).expect("initial schema");

    let db_path = project.join("dev.sqlite");
    let initial = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .current_dir(&project)
        .output()
        .expect("initial migration");
    assert!(initial.status.success(), "stderr: {}", String::from_utf8_lossy(&initial.stderr));

    let adapter = SqliteAdapter::new(db_path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg).unwrap();
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
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA).expect("updated schema");

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

const SQLITE_INDEX_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id    String @id @default(uuid())
    posts Post[]
}

model Post {
    id      String @id @default(uuid())
    title   String @index
    summary String @fulltext
    user_id String
    user    User @relation(fields: [user_id], references: [id])
}
"#;

const SQLITE_SCHEMA_WITH_PASSWORD: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id       String @id @default(uuid())
    email    String
    password String
}
"#;

const SQLITE_COMPOSITE_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Article {
    tenant_id String
    id        String
    slug      String
    category  String
    title     String
    body      String?

    @@ids([tenant_id, id])
    @@uniques([tenant_id, slug])
    @@indexes([tenant_id, category])
    @@fulltexts([title, body])
    @@table_name("search_articles")
}
"#;

fn temp_project(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("dinoco-cli-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).expect("temp project");
    path
}
