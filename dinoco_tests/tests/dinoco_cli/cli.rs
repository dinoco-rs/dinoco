use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Command;

use dinoco_engine::{
    DinocoAdapter, DinocoAdapterHandler, DinocoClient, DinocoClientConfig, DinocoError, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};
use uuid::Uuid;

const INITIAL_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum UserRole {
    ADMIN
    MEMBER
}

model User {
    id Integer @id @default(autoincrement())
    email String @unique
    name String
    role UserRole @default(MEMBER)
}
"#;

#[test]
fn init_command_creates_schema_from_automated_answers() {
    let project = TestDir::new();

    let output = run_cli(
        project.path(),
        ["init"],
        &[
            ("DINOCO_CLI_INIT_DATABASE", "PostgreSQL"),
            ("DINOCO_CLI_INIT_CONNECTION_TYPE", "Environment variable"),
            ("DINOCO_CLI_INIT_CONNECTION_URL", "DATABASE_URL"),
            ("DINOCO_CLI_INIT_WITH_REPLICAS", "true"),
            ("DINOCO_CLI_INIT_REPLICAS_AMOUNT", "2"),
        ],
    );

    let schema_path = project.path().join("dinoco/schema.dinoco");
    let schema = fs::read_to_string(&schema_path).expect("schema should be created");

    assert!(output.contains("Dinoco environment was successfully created"), "unexpected init output:\n{output}");
    assert!(schema.contains("database = \"postgresql\""), "unexpected schema:\n{schema}");
    assert!(schema.contains("database_url = env(\"DATABASE_URL\")"), "unexpected schema:\n{schema}");
    assert!(schema.contains("env(\"DATABASE_URL_REPLICA_1\")"), "unexpected schema:\n{schema}");
    assert!(schema.contains("env(\"DATABASE_URL_REPLICA_2\")"), "unexpected schema:\n{schema}");
}

#[test]
fn init_command_supports_sqlite_from_automated_answers() {
    let project = TestDir::new();

    let output = run_cli(
        project.path(),
        ["init"],
        &[
            ("DINOCO_CLI_INIT_DATABASE", "Sqlite"),
            ("DINOCO_CLI_INIT_CONNECTION_TYPE", "Static string"),
            ("DINOCO_CLI_INIT_CONNECTION_URL", "file:./dinoco/database.sqlite"),
            ("DINOCO_CLI_INIT_WITH_REPLICAS", "false"),
        ],
    );

    let schema_path = project.path().join("dinoco/schema.dinoco");
    let schema = fs::read_to_string(&schema_path).expect("schema should be created");

    assert!(output.contains("Dinoco environment was successfully created"), "unexpected init output:\n{output}");
    assert!(schema.contains("database = \"sqlite\""), "unexpected schema:\n{schema}");
    assert!(schema.contains("database_url = \"file:./dinoco/database.sqlite\""), "unexpected schema:\n{schema}");
    assert!(!schema.contains("read_replicas"), "unexpected schema:\n{schema}");
}

#[tokio::test]
async fn cli_commands_cover_full_sqlite_flow() {
    let apply_project = TestDir::new();
    let apply_database_path = apply_project.path().join("apply.sqlite");
    let apply_database_url = format!("file:{}", apply_database_path.display());

    write_schema(apply_project.path(), INITIAL_SCHEMA);

    let apply_output = run_cli(
        apply_project.path(),
        ["migrate", "generate", "--apply"],
        &[("DATABASE_URL", apply_database_url.as_str()), ("DINOCO_CLI_MIGRATION_NAME", "InitialUsers")],
    );

    assert!(apply_output.contains("Migration files generated successfully"));
    assert!(apply_output.contains("Rust models generated successfully"));

    let applied_migration = latest_migration_name(apply_project.path());
    let applied_migration_dir = apply_project.path().join("dinoco/migrations").join(&applied_migration);

    assert!(applied_migration.ends_with("_initial_users"));
    assert!(applied_migration_dir.join("migration.sql").exists());
    assert!(applied_migration_dir.join("schema.bin").exists());
    assert!(!applied_migration_dir.join("schema.dinoco").exists());
    assert!(apply_project.path().join("dinoco/models/user.rs").exists());

    let apply_client =
        DinocoClient::<SqliteAdapter>::new(apply_database_url.clone(), vec![], DinocoClientConfig::default())
            .await
            .expect("sqlite client should connect");
    let tables_after_apply = apply_client.primary().fetch_tables().await.expect("tables should load");

    assert!(tables_after_apply.iter().any(|table| table.name == "User"));
    assert!(tables_after_apply.iter().any(|table| table.name == "_dinoco_migrations"));

    let run_project = TestDir::new();
    let run_database_path = run_project.path().join("run.sqlite");
    let run_database_url = format!("file:{}", run_database_path.display());

    write_schema(run_project.path(), INITIAL_SCHEMA);

    let generate_output = run_cli(
        run_project.path(),
        ["migrate", "generate"],
        &[("DATABASE_URL", run_database_url.as_str()), ("DINOCO_CLI_MIGRATION_NAME", "InitialUsers")],
    );

    assert!(generate_output.contains("Migration files generated successfully"));
    assert!(generate_output.contains("Migration generated only"));
    assert!(!run_project.path().join("dinoco/models/user.rs").exists());

    let pending_migration = latest_migration_name(run_project.path());
    let generate_client =
        DinocoClient::<SqliteAdapter>::new(run_database_url.clone(), vec![], DinocoClientConfig::default())
            .await
            .expect("sqlite client should connect");
    let tables_after_generate = generate_client.primary().fetch_tables().await.expect("tables should load");

    assert!(!tables_after_generate.iter().any(|table| table.name == "_dinoco_migrations"));

    let run_output = run_cli(run_project.path(), ["migrate", "run"], &[("DATABASE_URL", run_database_url.as_str())]);

    assert!(
        run_output.contains("All pending migrations were applied successfully"),
        "unexpected migrate run output:\n{run_output}"
    );
    let generated_user_model = fs::read_to_string(run_project.path().join("dinoco/models/user.rs"))
        .expect("generated user model should exist");

    let run_client =
        DinocoClient::<SqliteAdapter>::new(run_database_url.clone(), vec![], DinocoClientConfig::default())
            .await
            .expect("sqlite client should connect");
    let tables_after_run = run_client.primary().fetch_tables().await.expect("tables should load");
    let user_table =
        tables_after_run.iter().find(|table| table.name == "User").expect("user table should exist after migrate run");

    assert!(generated_user_model.contains("pub struct User"));
    assert!(user_table.columns.iter().any(|column| column.name == "email"));

    fs::remove_dir_all(run_project.path().join("dinoco/models")).expect("generated models should be removable");

    let models_output =
        run_cli(run_project.path(), ["models", "generate"], &[("DATABASE_URL", run_database_url.as_str())]);

    assert!(
        models_output.contains("Rust models generated successfully from the latest migration stored in the database")
    );
    let regenerated_models_user = fs::read_to_string(run_project.path().join("dinoco/models/user.rs"))
        .expect("models generate should recreate user model");

    assert!(regenerated_models_user.contains("pub struct User"));

    fs::write(run_project.path().join("dinoco/schema.dinoco"), "broken schema").expect("schema should be replaced");

    let restore_first = run_cli(
        run_project.path(),
        ["schema", "restore", pending_migration.as_str()],
        &[("DATABASE_URL", run_database_url.as_str())],
    );
    let restored_first_schema =
        fs::read_to_string(run_project.path().join("dinoco/schema.dinoco")).expect("restored schema should exist");

    assert!(restore_first.contains("schema.dinoco was restored successfully"));
    assert!(restored_first_schema.contains("model User"));
    assert!(restored_first_schema.contains("enum UserRole"));
    assert!(restored_first_schema.contains("email"));

    let restore_latest =
        run_cli(run_project.path(), ["schema", "restore"], &[("DATABASE_URL", run_database_url.as_str())]);
    let restored_latest_schema = fs::read_to_string(run_project.path().join("dinoco/schema.dinoco"))
        .expect("latest restored schema should exist");

    assert!(restore_latest.contains("schema.dinoco was restored successfully"));
    assert!(restored_latest_schema.contains("model User"));
    assert!(restored_latest_schema.contains("UserRole"));

    let reset_output = run_cli(
        run_project.path(),
        ["database", "reset"],
        &[("DATABASE_URL", run_database_url.as_str()), ("DINOCO_CLI_DATABASE_RESET_CONFIRM", "true")],
    );

    assert!(reset_output.contains("Database reset completed successfully"));

    let tables_after_reset = run_client.primary().fetch_tables().await.expect("tables should load after reset");

    assert!(tables_after_reset.is_empty());
}

#[test]
fn generate_apply_cleans_up_failed_sqlite_migration() {
    let project = TestDir::new();
    let database_path = project.path().join("readonly.sqlite");

    fs::write(&database_path, "").expect("sqlite file should exist");
    write_schema(project.path(), INITIAL_SCHEMA);

    let database_url = format!("file:{}?mode=ro", database_path.display());
    let output = run_cli(
        project.path(),
        ["migrate", "generate", "--apply"],
        &[("DATABASE_URL", database_url.as_str()), ("DINOCO_CLI_MIGRATION_NAME", "InitialUsers")],
    );

    assert!(
        output.contains("Applying the migration to the primary database"),
        "unexpected migrate generate --apply output:\n{output}"
    );
    assert!(
        !project.path().join("dinoco/migrations").exists()
            || fs::read_dir(project.path().join("dinoco/migrations"))
                .expect("migrations dir should be readable")
                .next()
                .is_none()
    );
}

#[test]
fn rollback_command_reports_temporary_unavailability() {
    let project = TestDir::new();
    write_schema(project.path(), INITIAL_SCHEMA);

    let output = run_cli(project.path(), ["migrate", "rollback"], &[("DATABASE_URL", "file:unused.sqlite")]);

    assert!(output.contains("Rollback is temporarily unavailable"));
}

#[tokio::test]
async fn database_introspect_generates_schema_from_sqlite() {
    let project = TestDir::new();
    let database_path = project.path().join("introspect.sqlite");
    let database_url = format!("file:{}", database_path.display());
    let client = DinocoClient::<SqliteAdapter>::new(database_url.clone(), vec![], DinocoClientConfig::default())
        .await
        .expect("sqlite client should connect");

    client
        .primary()
        .execute_script(
            r#"
            CREATE TABLE User (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT NOT NULL UNIQUE,
                is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
                role TEXT NOT NULL CHECK (role IN ('ADMIN', 'MEMBER'))
            );

            INSERT INTO User (email, is_active, role) VALUES
                ('first@dinoco.dev', 1, 'ADMIN'),
                ('second@dinoco.dev', 0, 'MEMBER');
        "#,
        )
        .await
        .expect("schema should be created");

    let output = run_cli(project.path(), ["database", "introspect"], &[("DATABASE_URL", database_url.as_str())]);
    let schema = fs::read_to_string(project.path().join("dinoco/schema.dinoco")).expect("schema should be generated");

    assert!(output.contains("Database introspection completed"), "unexpected introspect output:\n{output}");
    assert!(schema.contains("database = \"sqlite\""), "unexpected schema:\n{schema}");
    assert!(schema.contains("database_url = env(\"DATABASE_URL\")"), "unexpected schema:\n{schema}");
    assert!(schema.contains("model User"), "unexpected schema:\n{schema}");
    assert!(
        schema.lines().any(|line| line.contains("id") && line.contains("Integer") && line.contains("@id")),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.lines().any(|line| line.contains("email") && line.contains("String") && line.contains("@unique")),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.lines().any(|line| line.contains("is_active") && line.contains("Boolean")),
        "unexpected schema:\n{schema}"
    );
    assert!(schema.contains("enum UserRole"), "unexpected schema:\n{schema}");
    assert!(
        schema.lines().any(|line| line.contains("role") && line.contains("UserRole")),
        "unexpected schema:\n{schema}"
    );
}

#[tokio::test]
async fn database_introspect_generates_complex_schema_for_postgres() {
    let project = TestDir::new();
    let database_url = postgres_url();
    let prefix = format!("introspect_pg_{}", short_test_suffix());

    let adapter = match PostgresAdapter::connect(database_url.clone(), DinocoClientConfig::default()).await {
        Ok(adapter) => adapter,
        Err(err) if should_skip_external_adapter_test(&err) => {
            eprintln!("skipping postgres introspect test: {err}");
            return;
        }
        Err(err) => panic!("postgres adapter should connect: {err}"),
    };

    if !setup_postgres_complex_schema(&adapter, &prefix).await {
        return;
    }

    let output = run_cli(project.path(), ["database", "introspect"], &[("DATABASE_URL", database_url.as_str())]);
    let schema = fs::read_to_string(project.path().join("dinoco/schema.dinoco")).expect("schema should be generated");

    assert!(output.contains("Database introspection completed"), "unexpected introspect output:\n{output}");
    assert!(
        schema.contains(&format!("model {}", to_model_name(&format!("{prefix}_user")))),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.contains(&format!("model {}", to_model_name(&format!("{prefix}_post")))),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.contains(&format!("model {}", to_model_name(&format!("{prefix}_tag")))),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.lines().any(|line| line.contains("is_active") && line.contains("Boolean")),
        "unexpected schema:\n{schema}"
    );
    let compact_schema = schema.chars().filter(|ch| !ch.is_whitespace()).collect::<String>();

    assert!(schema.contains("@relation("), "unexpected schema:\n{schema}");
    assert!(schema.contains("references: [id]"), "unexpected schema:\n{schema}");
    assert!(
        compact_schema.contains("@@uniques([slug,locale])") || compact_schema.contains("@@uniques([locale,slug])"),
        "unexpected schema:\n{schema}"
    );
    assert!(
        compact_schema.contains("@@indexes([title,author_id])")
            || compact_schema.contains("@@indexes([author_id,title])"),
        "unexpected schema:\n{schema}"
    );
}

#[tokio::test]
async fn database_introspect_generates_complex_schema_for_mysql() {
    let project = TestDir::new();
    let database_url = mysql_url();
    let prefix = format!("introspect_my_{}", short_test_suffix());

    let adapter = match MySqlAdapter::connect(database_url.clone(), DinocoClientConfig::default()).await {
        Ok(adapter) => adapter,
        Err(err) if should_skip_external_adapter_test(&err) => {
            eprintln!("skipping mysql introspect test: {err}");
            return;
        }
        Err(err) => panic!("mysql adapter should connect: {err}"),
    };

    if !setup_mysql_complex_schema(&adapter, &prefix).await {
        return;
    }

    let output = run_cli(project.path(), ["database", "introspect"], &[("DATABASE_URL", database_url.as_str())]);
    let schema = fs::read_to_string(project.path().join("dinoco/schema.dinoco")).expect("schema should be generated");

    assert!(output.contains("Database introspection completed"), "unexpected introspect output:\n{output}");
    assert!(
        schema.contains(&format!("model {}", to_model_name(&format!("{prefix}_user")))),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.contains(&format!("model {}", to_model_name(&format!("{prefix}_post")))),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.contains(&format!("model {}", to_model_name(&format!("{prefix}_tag")))),
        "unexpected schema:\n{schema}"
    );
    assert!(
        schema.lines().any(|line| line.contains("is_active") && line.contains("Boolean")),
        "unexpected schema:\n{schema}"
    );
    let compact_schema = schema.chars().filter(|ch| !ch.is_whitespace()).collect::<String>();

    assert!(schema.contains("@relation("), "unexpected schema:\n{schema}");
    assert!(schema.contains("references: [id]"), "unexpected schema:\n{schema}");
    assert!(
        compact_schema.contains("@@uniques([slug,locale])") || compact_schema.contains("@@uniques([locale,slug])"),
        "unexpected schema:\n{schema}"
    );
    assert!(
        compact_schema.contains("@@indexes([title,author_id])")
            || compact_schema.contains("@@indexes([author_id,title])"),
        "unexpected schema:\n{schema}"
    );
}

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_dinoco_cli")
}

fn postgres_url() -> String {
    env::var("DINOCO_POSTGRES_DATABASE_URL")
        .or_else(|_| env::var("POSTGRES_DATABASE_URL"))
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://postgres:root@localhost:5432/dinoco".to_string())
}

fn mysql_url() -> String {
    env::var("DINOCO_MYSQL_DATABASE_URL")
        .or_else(|_| env::var("MYSQL_DATABASE_URL"))
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "mysql://root:root@localhost:3306/dinoco".to_string())
}

fn should_skip_external_adapter_test(error: &DinocoError) -> bool {
    match error {
        DinocoError::ConnectionError(_) => true,
        DinocoError::MySql(mysql_error) => mysql_error.to_string().contains("Operation not permitted"),
        DinocoError::Postgres(postgres_error) => postgres_error.to_string().contains("error connecting to server"),
        _ => false,
    }
}

fn to_model_name(table_name: &str) -> String {
    let mut result = String::new();

    for piece in
        table_name.chars().map(|ch| if ch.is_alphanumeric() { ch } else { ' ' }).collect::<String>().split_whitespace()
    {
        let mut chars = piece.chars();

        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());

            for ch in chars {
                result.push(ch.to_ascii_lowercase());
            }
        }
    }

    result
}

fn short_test_suffix() -> String {
    Uuid::now_v7().simple().to_string().chars().take(8).collect()
}

async fn setup_postgres_complex_schema(adapter: &PostgresAdapter, prefix: &str) -> bool {
    let user = format!("{prefix}_user");
    let post = format!("{prefix}_post");
    let tag = format!("{prefix}_tag");
    let join = format!("{prefix}_post_tag");
    let status_enum = format!("{prefix}_status_enum");

    let sql = format!(
        r#"
        DROP TABLE IF EXISTS "{join}" CASCADE;
        DROP TABLE IF EXISTS "{post}" CASCADE;
        DROP TABLE IF EXISTS "{tag}" CASCADE;
        DROP TABLE IF EXISTS "{user}" CASCADE;
        DROP TYPE IF EXISTS "{status_enum}";

        CREATE TYPE "{status_enum}" AS ENUM ('ACTIVE', 'INACTIVE');

        CREATE TABLE "{user}" (
            id BIGSERIAL PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            manager_id BIGINT UNIQUE,
            is_active SMALLINT NOT NULL,
            status "{status_enum}" NOT NULL
        );

        ALTER TABLE "{user}"
            ADD CONSTRAINT "{prefix}_user_manager_fk"
            FOREIGN KEY (manager_id) REFERENCES "{user}" (id);

        CREATE TABLE "{post}" (
            id BIGSERIAL PRIMARY KEY,
            author_id BIGINT NOT NULL,
            title TEXT NOT NULL,
            slug TEXT NOT NULL,
            locale TEXT NOT NULL,
            CONSTRAINT "{prefix}_post_slug_locale_uk" UNIQUE (slug, locale)
        );

        ALTER TABLE "{post}"
            ADD CONSTRAINT "{prefix}_post_author_fk"
            FOREIGN KEY (author_id) REFERENCES "{user}" (id);

        CREATE INDEX "{prefix}_post_title_author_idx" ON "{post}" (title, author_id);

        CREATE TABLE "{tag}" (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE
        );

        CREATE TABLE "{join}" (
            post_id BIGINT NOT NULL,
            tag_id BIGINT NOT NULL,
            PRIMARY KEY (post_id, tag_id)
        );

        ALTER TABLE "{join}"
            ADD CONSTRAINT "{prefix}_join_post_fk"
            FOREIGN KEY (post_id) REFERENCES "{post}" (id);
        ALTER TABLE "{join}"
            ADD CONSTRAINT "{prefix}_join_tag_fk"
            FOREIGN KEY (tag_id) REFERENCES "{tag}" (id);

        INSERT INTO "{user}" (email, manager_id, is_active, status) VALUES
            ('chief-{prefix}@dinoco.dev', NULL, 1, 'ACTIVE'),
            ('dev-{prefix}@dinoco.dev', 1, 0, 'INACTIVE');
    "#
    );

    if let Err(err) = adapter.execute_script(&sql).await {
        if should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres introspect schema setup: {err}");
            return false;
        }

        panic!("postgres schema setup should work: {err}");
    }

    true
}

async fn setup_mysql_complex_schema(adapter: &MySqlAdapter, prefix: &str) -> bool {
    let user = format!("{prefix}_user");
    let post = format!("{prefix}_post");
    let tag = format!("{prefix}_tag");
    let join = format!("{prefix}_post_tag");

    let sql = format!(
        r#"
        DROP TABLE IF EXISTS `{join}`;
        DROP TABLE IF EXISTS `{post}`;
        DROP TABLE IF EXISTS `{tag}`;
        DROP TABLE IF EXISTS `{user}`;

        CREATE TABLE `{user}` (
            id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            email VARCHAR(255) NOT NULL UNIQUE,
            manager_id BIGINT NULL UNIQUE,
            is_active TINYINT(1) NOT NULL,
            status ENUM('ACTIVE', 'INACTIVE') NOT NULL,
            CONSTRAINT `{prefix}_user_manager_fk` FOREIGN KEY (manager_id) REFERENCES `{user}` (id)
        );

        CREATE TABLE `{post}` (
            id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            author_id BIGINT NOT NULL,
            title VARCHAR(255) NOT NULL,
            slug VARCHAR(255) NOT NULL,
            locale VARCHAR(32) NOT NULL,
            UNIQUE KEY `{prefix}_post_slug_locale_uk` (slug, locale),
            CONSTRAINT `{prefix}_post_author_fk` FOREIGN KEY (author_id) REFERENCES `{user}` (id),
            INDEX `{prefix}_post_title_author_idx` (title, author_id)
        );

        CREATE TABLE `{tag}` (
            id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(255) NOT NULL UNIQUE
        );

        CREATE TABLE `{join}` (
            post_id BIGINT NOT NULL,
            tag_id BIGINT NOT NULL,
            PRIMARY KEY (post_id, tag_id),
            CONSTRAINT `{prefix}_join_post_fk` FOREIGN KEY (post_id) REFERENCES `{post}` (id),
            CONSTRAINT `{prefix}_join_tag_fk` FOREIGN KEY (tag_id) REFERENCES `{tag}` (id)
        );

        INSERT INTO `{user}` (email, manager_id, is_active, status) VALUES
            ('chief-{prefix}@dinoco.dev', NULL, 1, 'ACTIVE'),
            ('dev-{prefix}@dinoco.dev', 1, 0, 'INACTIVE');
    "#
    );

    if let Err(err) = adapter.execute_script(&sql).await {
        if should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql introspect schema setup: {err}");
            return false;
        }

        panic!("mysql schema setup should work: {err}");
    }

    true
}

fn latest_migration_name(root: &Path) -> String {
    let migrations_dir = root.join("dinoco/migrations");
    let mut entries = fs::read_dir(migrations_dir)
        .expect("migrations dir should exist")
        .map(|entry| entry.expect("migration dir entry should load").path())
        .filter(|path| path.is_dir())
        .map(|path| file_name(&path))
        .collect::<Vec<_>>();

    entries.sort();

    entries.pop().expect("at least one migration should exist")
}

fn file_name(path: &Path) -> String {
    path.file_name().and_then(OsStr::to_str).expect("path should have a valid file name").to_string()
}

fn run_cli<I, S>(root: &Path, args: I, envs: &[(&str, &str)]) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(binary_path())
        .args(args)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .envs(envs.iter().copied())
        .output()
        .expect("cli command should run");

    assert!(output.status.success(), "stdout:\n{}\nstderr:\n{}", to_utf8(&output.stdout), to_utf8(&output.stderr));

    format!("{}\n{}", to_utf8(&output.stdout), to_utf8(&output.stderr))
}

fn to_utf8(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("output should be valid utf-8")
}

fn write_schema(root: &Path, schema: &str) {
    let dinoco_dir = root.join("dinoco");

    fs::create_dir_all(&dinoco_dir).expect("dinoco dir should be created");
    fs::write(dinoco_dir.join("schema.dinoco"), schema.trim_start()).expect("schema should be written");
}

struct TestDir {
    path: std::path::PathBuf,
}

impl TestDir {
    fn new() -> Self {
        let mut path = std::env::temp_dir();

        path.push(format!("dinoco-cli-tests-{}-{}", std::process::id(), Uuid::now_v7()));

        fs::create_dir_all(&path).expect("temp test dir should be created");

        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
