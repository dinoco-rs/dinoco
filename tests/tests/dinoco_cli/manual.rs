use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_cli::commands::manual::{format_timestamp, pascal_case, scaffold, snake_case};
use dinoco_engine::rusqlite::Connection;

const SCHEMA: &str = r#"config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    migration_engine = "manual"
}

model User {
    id   String @id
    name String
}
"#;

fn temp_project(name: &str) -> PathBuf {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let path = std::env::temp_dir().join(format!("dinoco-manual-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&path).expect("temp project");
    path
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root").to_path_buf()
}

/// Runs the CLI. Anything that builds the runner compiles Dinoco itself, so
/// build against this checkout and share one target directory between tests.
fn cli(project: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let target = repo_root().join("target/runner-tests");
    let mut command = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"));
    command
        .args(args)
        .current_dir(project)
        .env("DINOCO_RUNNER_CRATES_PATH", repo_root())
        .env("CARGO_TARGET_DIR", target)
        .env("DATABASE_URL", project.join("dev.sqlite"));
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("cli should run")
}

fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn failure(output: &Output) -> String {
    assert!(!output.status.success(), "expected a failure, got: {}", String::from_utf8_lossy(&output.stdout));
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn table_exists(project: &Path, table: &str) -> bool {
    let connection = Connection::open(project.join("dev.sqlite")).expect("sqlite");
    connection
        .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)", [table], |row| {
            row.get(0)
        })
        .expect("table existence query")
}

const REAL_MIGRATION: &str = r#"use ::dinoco::*;

#[dinoco(migration)]
pub struct CreateUsers;

impl DinocoMigration for CreateUsers {
    async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        manager
            .create_table(CreateTableMigration {
                table: "user".to_string(),
                if_not_exists: false,
                columns: vec![
                    MigrationColumn::new("id", MigrationColumnType::String).primary_key(),
                    MigrationColumn::new("name", MigrationColumnType::String),
                ],
                foreign_keys: Vec::new(),
            })
            .await
    }

    async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        manager.drop_table(DropTableMigration { table: "user".to_string(), if_exists: false }).await
    }
}
"#;

#[test]
fn timestamps_and_names_are_formatted_for_module_and_file_names() {
    assert_eq!(format_timestamp(0), "19700101000000");
    assert_eq!(format_timestamp(1_782_302_400), "20260624120000");
    assert_eq!(format_timestamp(951_782_399), "20000228235959");
    assert_eq!(format_timestamp(951_782_400), "20000229000000", "leap day");
    assert_eq!(format_timestamp(951_868_800), "20000301000000");

    assert_eq!(snake_case("Create Users"), "create_users");
    assert_eq!(snake_case("  addUserEmail!! "), "add_user_email");
    assert_eq!(snake_case("!!!"), "");
    assert_eq!(pascal_case("create_users"), "CreateUsers");
    assert_eq!(pascal_case("2fa_codes"), "Migration2faCodes", "struct names cannot start with a digit");
}

#[test]
fn scaffold_registers_migrations_in_order_and_keeps_the_markers() {
    let project = temp_project("scaffold");
    let mod_path = project.join("mod.rs");
    fs::write(&mod_path, dinoco_codegen::migrations_mod_template()).expect("mod.rs");

    let first = scaffold(&mod_path, &project, "20260101000000", "create_users").expect("first");
    let second = scaffold(&mod_path, &project, "20260102000000", "add_email").expect("second");

    assert_eq!(first.file_name().unwrap(), "20260101000000_create_users.rs");
    let migration = fs::read_to_string(&first).expect("migration file");
    assert!(migration.contains("#[dinoco(migration)]\npub struct CreateUsers;"));
    assert!(migration.contains("impl DinocoMigration for CreateUsers"));
    assert!(migration.contains("async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr>"));
    assert!(migration.contains("async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr>"));
    assert!(second.exists());

    let registry = fs::read_to_string(&mod_path).expect("mod.rs");
    let first_mod = registry.find("mod m20260101000000_create_users;").expect("first mod");
    let second_mod = registry.find("mod m20260102000000_add_email;").expect("second mod");
    assert!(first_mod < second_mod);
    assert!(registry.contains("#[path = \"20260101000000_create_users.rs\"]"));
    let first_entry = registry
        .find("::dinoco::MigrationEntry::new(\"20260101000000_create_users\", m20260101000000_create_users::CreateUsers),")
        .expect("first entry");
    let second_entry =
        registry.find("::dinoco::MigrationEntry::new(\"20260102000000_add_email\"").expect("second entry");
    assert!(first_entry < second_entry);
    assert!(registry.contains("// dinoco:migrations:mods:end") && registry.contains("// dinoco:migrations:list:end"));

    let error = scaffold(&mod_path, &project, "20260101000000", "create_users").expect_err("same file twice");
    assert!(error.to_string().contains("already exists"), "{error}");

    fs::write(&mod_path, "pub fn migrations() {}\n").expect("broken mod.rs");
    let error = scaffold(&mod_path, &project, "20260103000000", "later").expect_err("markers are required");
    assert!(error.to_string().contains("marker"), "{error}");
}

#[test]
fn init_can_create_a_manual_project_and_generate_scaffolds_a_migration() {
    let project = temp_project("init");
    let output = cli(&project, &["init", "--migration-engine", "manual"], &[("DINOCO_CLI_INIT_DATABASE", "sqlite")]);
    success(&output);

    let schema = fs::read_to_string(project.join("dinoco/schema.dinoco")).expect("schema");
    assert!(schema.contains("migration_engine = \"manual\""), "{schema}");
    assert!(project.join("dinoco/migrations/mod.rs").exists());

    let stdout = success(&cli(&project, &["migrate", "generate", "Create Users"], &[]));
    assert!(stdout.contains("Manual migration created"), "{stdout}");
    assert!(stdout.contains("Rust models generated"), "{stdout}");

    let files = fs::read_dir(project.join("dinoco/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(files.iter().any(|name| name.ends_with("_create_users.rs")), "{files:?}");
    assert!(!files.iter().any(|name| name.ends_with(".sql")), "manual mode never writes SQL: {files:?}");

    let generated_mod = fs::read_to_string(project.join("dinoco/mod.rs")).expect("dinoco/mod.rs");
    assert!(generated_mod.contains("pub mod migrations;"), "{generated_mod}");
    let registry = fs::read_to_string(project.join("dinoco/migrations/mod.rs")).expect("registry");
    assert!(registry.contains("::dinoco::MigrationEntry::new("));
}

#[test]
fn rollback_and_status_require_the_manual_engine() {
    let project = temp_project("auto-only");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(
        project.join("dinoco/schema.dinoco"),
        "config {\n    database = \"sqlite\"\n    database_url = env(\"DATABASE_URL\")\n}\n",
    )
    .expect("schema");

    for command in ["rollback", "status"] {
        let stderr = failure(&cli(&project, &["migrate", command], &[]));
        assert!(stderr.contains("migration_engine = \"manual\""), "{stderr}");
    }
}

#[test]
fn manual_migrations_run_status_and_rollback_end_to_end() {
    let project = temp_project("e2e");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SCHEMA).expect("schema");

    success(&cli(&project, &["migrate", "generate", "create_users"], &[]));
    let migration = fs::read_dir(project.join("dinoco/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path())
        .find(|path| path.file_name().is_some_and(|name| name.to_string_lossy().ends_with("_create_users.rs")))
        .expect("scaffolded migration");
    fs::write(&migration, REAL_MIGRATION).expect("write migration body");

    let stdout = success(&cli(&project, &["migrate", "status"], &[]));
    assert!(stdout.contains("[pending]") && stdout.contains("_create_users"), "{stdout}");

    let stdout = success(&cli(&project, &["migrate", "run"], &[]));
    assert!(stdout.contains("Migration applied:"), "{stdout}");
    assert!(table_exists(&project, "user"));

    let stdout = success(&cli(&project, &["migrate", "run"], &[]));
    assert!(stdout.contains("No pending migrations."), "{stdout}");

    let stdout = success(&cli(&project, &["migrate", "status"], &[]));
    assert!(stdout.contains("[applied]"), "{stdout}");

    let stdout = success(&cli(&project, &["migrate", "rollback"], &[]));
    assert!(stdout.contains("Migration reverted:"), "{stdout}");
    assert!(!table_exists(&project, "user"));
}

#[test]
fn transform_rs_customizes_generated_models_and_enums() {
    let project = temp_project("transform");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(
        project.join("dinoco/schema.dinoco"),
        r#"config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum Role { admin member }

model User {
    id         String    @id
    role       Role
    deleted_at DateTime?
}
"#,
    )
    .expect("schema");
    fs::write(
        project.join("dinoco/transform.rs"),
        r##"use dinoco_codegen::prelude::*;

struct Custom;

impl DinocoTransformer for Custom {
    fn transform_enum(&self, item: &mut Enum) {
        item.add_derive("Hash");
    }

    fn transform_model(&self, model: &mut Model) {
        model.add_derive("Hash");
        model.add_import("std::fmt");
        model.add_impl_method(ImplMethod::new("is_active", "bool", quote! { self.deleted_at.is_none() }));
    }

    fn transform_field(&self, _model: &Model, field: &mut Field) {
        if field.name.ends_with("_at") {
            field.add_attribute("#[serde(skip)]");
        }
    }
}

pub fn transformer() -> impl DinocoTransformer {
    Custom
}
"##,
    )
    .expect("transform.rs");

    let stdout = success(&cli(&project, &["models", "generate"], &[]));
    assert!(stdout.contains("transform.rs applied"), "{stdout}");

    let user = fs::read_to_string(project.join("dinoco/models/user.rs")).expect("user.rs");
    assert!(user.contains(", Hash)]"), "{user}");
    assert!(user.contains("use std::fmt;"), "{user}");
    assert!(user.contains("#[serde(skip)]\n    pub deleted_at:"), "{user}");
    assert!(user.contains("pub fn is_active(&self) -> bool"), "{user}");
    let models = fs::read_to_string(project.join("dinoco/models/mod.rs")).expect("models/mod.rs");
    assert!(models.contains("::dinoco::DinocoEnum, Hash)]"), "{models}");

    // A broken transform surfaces the compiler error instead of silently
    // falling back to the untransformed models.
    fs::write(project.join("dinoco/transform.rs"), "pub fn transformer() -> () { not_rust }\n").expect("broken");
    let stderr = failure(&cli(&project, &["models", "generate"], &[]));
    assert!(stderr.contains("runner failed"), "{stderr}");
}
