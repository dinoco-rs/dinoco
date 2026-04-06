use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::{DinocoAdapterHandler, DinocoClient, DinocoClientConfig, SqliteAdapter};

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

    assert!(output.contains("Your Dinoco environment was successfully created"));
    assert!(schema.contains("database = \"postgresql\""));
    assert!(schema.contains("database_url = env(\"DATABASE_URL\")"));
    assert!(schema.contains("env(\"DATABASE_URL_REPLICA_1\")"));
    assert!(schema.contains("env(\"DATABASE_URL_REPLICA_2\")"));
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
fn rollback_command_reports_temporary_unavailability() {
    let project = TestDir::new();
    write_schema(project.path(), INITIAL_SCHEMA);

    let output = run_cli(project.path(), ["migrate", "rollback"], &[("DATABASE_URL", "file:unused.sqlite")]);

    assert!(output.contains("Rollback is temporarily unavailable"));
}

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_dinoco_cli")
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
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("system time should be valid").as_nanos();

        path.push(format!("dinoco-cli-tests-{}-{nanos}", std::process::id()));

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
