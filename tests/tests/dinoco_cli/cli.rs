use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::{DinocoAdapter, DinocoSqlCompiler, DinocoValue, InsertQuery, SqliteAdapter, rusqlite::Connection};

#[test]
fn version_flags_print_the_cli_version() {
    for flag in ["--version", "-v"] {
        let output =
            Command::new(env!("CARGO_BIN_EXE_dinoco_cli")).arg(flag).output().expect("cli should print its version");

        assert!(output.status.success(), "{flag} stderr: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), format!("dinoco {}", env!("CARGO_PKG_VERSION")));
    }
}

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
    let mut child = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(&project)
        .spawn()
        .expect("cli should run");
    child.stdin.take().expect("migration confirmation stdin").write_all(b"Y\n").expect("confirm migration");
    let output = child.wait_with_output().expect("cli output");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Detected"));
    assert!(stdout.contains("Generate and apply this migration"));
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
fn migrate_generate_cancels_before_writing_when_not_confirmed() {
    let project = temp_project("migrate-cancelled");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA).expect("schema");

    let db_path = project.join("dev.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(&project)
        .spawn()
        .expect("cli should run");
    child.stdin.take().expect("migration confirmation stdin").write_all(b"n\n").expect("cancel migration");
    let output = child.wait_with_output().expect("cli output");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Detected"));
    assert!(stdout.contains("Migration generation cancelled"));
    assert!(!project.join("dinoco/models").exists());
    let migrations = fs::read_dir(project.join("dinoco/migrations")).expect("empty migrations directory");
    assert_eq!(migrations.count(), 0);
}

#[test]
fn workspace_commands_use_separate_migrations_and_replace_generated_models() {
    let project = temp_project("workspaces");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), WORKSPACE_SCHEMA).expect("workspace schema");

    let dev_db = project.join("dev.sqlite");
    let dev_replica_db = project.join("dev-replica.sqlite");
    let prod_db = project.join("prod.sqlite");
    let dev = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate", "--workspace", "dev"])
        .env("DEV_DATABASE_URL", &dev_db)
        .env("DEV_REPLICA_DATABASE_URL", &dev_replica_db)
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .current_dir(&project)
        .output()
        .expect("dev migration should run");
    assert!(dev.status.success(), "stderr: {}", String::from_utf8_lossy(&dev.stderr));
    assert!(project.join("dinoco/migrations/dev").is_dir());
    assert!(!project.join("dinoco/migrations/prod").exists());
    assert!(project.join("dinoco/models/user.rs").exists());
    assert!(dev_db.exists(), "the selected workspace primary must be used");
    assert!(!prod_db.exists(), "an unselected workspace database must not be opened");
    assert!(!dev_replica_db.exists(), "migration commands must not open read replicas");
    let dev_connection = Connection::open(&dev_db).expect("dev primary");
    assert!(table_exists(&dev_connection, "user"));
    drop(dev_connection);
    let generated = fs::read_to_string(project.join("dinoco/mod.rs")).expect("generated mod");
    assert!(generated.contains("DEV_DATABASE_URL"));
    assert!(generated.contains("DEV_REPLICA_DATABASE_URL"));
    assert!(!generated.contains("PROD_REPLICA_DATABASE_URL"));
    assert!(generated.contains("migrations/dev/"));
    assert!(generated.contains("pub async fn migrate("));

    fs::write(project.join("dinoco/schema.dinoco"), WORKSPACE_ACCOUNT_SCHEMA).expect("updated workspace schema");
    let prod_replica_db = project.join("prod-replica.sqlite");
    let prod = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate", "-w", "prod"])
        .env("PROD_DATABASE_URL", &prod_db)
        .env("PROD_REPLICA_DATABASE_URL", &prod_replica_db)
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .current_dir(&project)
        .output()
        .expect("prod migration should run");
    assert!(prod.status.success(), "stderr: {}", String::from_utf8_lossy(&prod.stderr));
    assert!(project.join("dinoco/migrations/prod").is_dir());
    assert!(project.join("dinoco/migrations/dev").is_dir());
    assert!(project.join("dinoco/models/account.rs").exists());
    assert!(prod_db.exists(), "the selected prod primary must be used");
    assert!(!prod_replica_db.exists(), "prod migrations must not open the prod replica");
    assert!(!project.join("dinoco/models/user.rs").exists(), "switching workspaces must remove stale generated models");
    let generated = fs::read_to_string(project.join("dinoco/mod.rs")).expect("generated mod");
    assert!(generated.contains("PROD_DATABASE_URL"));
    assert!(generated.contains("PROD_REPLICA_DATABASE_URL"));
    assert!(!generated.contains("DEV_REPLICA_DATABASE_URL"));
    assert!(generated.contains("migrations/prod/"));
    assert!(!generated.contains("migrations/dev/"));

    let models = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["models", "generate", "-w", "dev"])
        .current_dir(&project)
        .output()
        .expect("dev models should run");
    assert!(models.status.success(), "stderr: {}", String::from_utf8_lossy(&models.stderr));
    let generated = fs::read_to_string(project.join("dinoco/mod.rs")).expect("generated mod");
    assert!(generated.contains("DEV_DATABASE_URL"));
    assert!(generated.contains("DEV_REPLICA_DATABASE_URL"));
    assert!(!generated.contains("PROD_REPLICA_DATABASE_URL"));
    assert!(generated.contains("migrations/dev/"));
    assert!(!generated.contains("migrations/prod/"));

    let run = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "run", "--workspace", "dev"])
        .env("DEV_DATABASE_URL", &dev_db)
        .env("DEV_REPLICA_DATABASE_URL", &dev_replica_db)
        .current_dir(&project)
        .output()
        .expect("dev migrate run should run");
    assert!(run.status.success(), "stderr: {}", String::from_utf8_lossy(&run.stderr));
    assert!(!dev_replica_db.exists(), "migrate run must continue using only the workspace primary");
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
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
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
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
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

#[test]
fn migrate_generate_sqlite_enforces_enum_values_with_check_constraint() {
    let project = temp_project("sqlite-enum");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_ENUM_SCHEMA).expect("schema");

    let db_path = project.join("dev.sqlite");
    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
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
    assert!(up.contains("role TEXT CHECK (role IN ('ADMIN', 'MEMBER'))"), "{up}");

    let connection = Connection::open(&db_path).expect("sqlite");
    connection.execute("INSERT INTO user (id, role) VALUES ('valid', 'ADMIN')", []).expect("valid enum value");
    assert!(connection.execute("INSERT INTO user (id, role) VALUES ('invalid', 'OWNER')", []).is_err());
    drop(connection);

    let stable = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .current_dir(&project)
        .output()
        .expect("stable cli run");
    assert!(stable.status.success(), "stderr: {}", String::from_utf8_lossy(&stable.stderr));
    assert!(String::from_utf8_lossy(&stable.stdout).contains("No schema changes were found"));
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
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
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

const WORKSPACE_SCHEMA: &str = r#"
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
            read_replicas = [env("DEV_REPLICA_DATABASE_URL")]
        }

        prod {
            database = "sqlite"
            database_url = env("PROD_DATABASE_URL")
            read_replicas = [env("PROD_REPLICA_DATABASE_URL")]
        }
    }
}

model User {
    id String @id @default(uuid())
}
"#;

const WORKSPACE_ACCOUNT_SCHEMA: &str = r#"
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
            read_replicas = [env("DEV_REPLICA_DATABASE_URL")]
        }

        prod {
            database = "sqlite"
            database_url = env("PROD_DATABASE_URL")
            read_replicas = [env("PROD_REPLICA_DATABASE_URL")]
        }
    }
}

model Account {
    id String @id @default(uuid())
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

const SQLITE_ENUM_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum Role {
    ADMIN
    MEMBER
}

model User {
    id   String @id
    role Role
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

fn table_exists(connection: &Connection, table: &str) -> bool {
    connection
        .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)", [table], |row| {
            row.get(0)
        })
        .expect("table existence query")
}
