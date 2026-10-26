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
    assert!(stdout.contains("Created dinoco/schema.dinoco"));
    assert!(stdout.contains("Created dinoco/migrations/"));
    assert!(stdout.contains("Next steps"));
    assert!(stdout.contains("DATABASE_URL"));
    assert!(stdout.contains("https://dinoco.io"));
    assert!(schema.contains("database"));
    assert!(schema.contains("\"postgresql\""));
    assert!(schema.contains("connection"));
    assert!(schema.contains("\"pgbouncer\""));
    assert!(schema.contains("database_url"));
    assert!(schema.contains("env(\"DATABASE_URL\")"));
}

#[test]
fn init_run_again_warns_instead_of_overwriting_and_does_not_repeat_created_lines() {
    let project = temp_project("init-twice");
    let first = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .arg("init")
        .env("DINOCO_CLI_INIT_DATABASE", "sqlite")
        .current_dir(&project)
        .output()
        .expect("cli should run");
    assert!(first.status.success(), "stderr: {}", String::from_utf8_lossy(&first.stderr));

    fs::write(project.join("dinoco/schema.dinoco"), "# customized by the user\n").expect("simulate user edits");

    let second = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .arg("init")
        .env("DINOCO_CLI_INIT_DATABASE", "sqlite")
        .current_dir(&project)
        .output()
        .expect("cli should run");
    assert!(second.status.success(), "stderr: {}", String::from_utf8_lossy(&second.stderr));

    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(stdout.contains("already exists"));
    assert!(!stdout.contains("Created dinoco/schema.dinoco"));
    assert!(!stdout.contains("Created dinoco/migrations/"));

    let schema = fs::read_to_string(project.join("dinoco/schema.dinoco")).expect("schema");
    assert_eq!(schema, "# customized by the user\n", "existing schema must not be overwritten");
}

#[test]
fn models_generate_runs_the_complete_recursive_multi_file_pipeline() {
    let project = temp_project("multi-file-codegen");
    fs::create_dir_all(project.join("dinoco/domain")).expect("dinoco directories");
    fs::write(
        project.join("dinoco/schema.dinoco"),
        r#"config {
    imports = ["domain/business.dinoco"]
}
"#,
    )
    .expect("root schema");
    fs::write(
        project.join("dinoco/domain/business.dinoco"),
        r#"import { BusinessStatus } from "../enums.dinoco"

model Business {
    id     String @id
    status BusinessStatus
}
"#,
    )
    .expect("business schema");
    fs::write(project.join("dinoco/enums.dinoco"), "enum BusinessStatus { active inactive }\n").expect("enum schema");

    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["models", "generate"])
        .current_dir(&project)
        .output()
        .expect("models generation should run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let generated_mod = fs::read_to_string(project.join("dinoco/mod.rs")).expect("generated dinoco module");
    let generated_models = fs::read_to_string(project.join("dinoco/models/mod.rs")).expect("generated models module");
    let generated_business =
        fs::read_to_string(project.join("dinoco/models/business.rs")).expect("generated business model");
    assert!(generated_mod.starts_with("#![allow(unused)]"));
    assert!(generated_models.contains("pub enum BusinessStatus"));
    assert!(generated_business.contains("pub status: BusinessStatus"));
}

#[test]
fn migrate_generate_sqlite_creates_migration_and_models() {
    let project = temp_project("migrate");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA).expect("schema");

    let db_path = project.join("dev.sqlite");
    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate", "--yes"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .stdin(Stdio::null())
        .current_dir(&project)
        .output()
        .expect("cli should run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Migration Plan"), "{stdout}");
    assert!(stdout.contains("+ Create table `user`"), "{stdout}");
    assert!(stdout.contains("Detected 1 schema change: 1 addition."), "{stdout}");
    assert!(stdout.contains("Migration generated and applied"), "{stdout}");
    assert!(stdout.contains("Rust models generated"), "{stdout}");
    assert!(!stdout.contains('\u{1b}'), "redirected output must not contain ANSI escapes: {stdout}");
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

/// Without a terminal, `migrate generate` never waits for an answer: a plan
/// that was not approved with an option stops with exit code 2, before
/// anything is written.
#[test]
fn migrate_generate_requires_approval_without_a_terminal() {
    let project = temp_project("migrate-unapproved");
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
    // Answers on a pipe are not read: only options approve a plan.
    child.stdin.take().expect("stdin").write_all(b"y\n").expect("write stdin");
    let output = child.wait_with_output().expect("cli output");

    assert_eq!(output.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("Detected"), "{stdout}");
    assert!(stderr.contains("error: This migration needs approval."), "{stderr}");
    assert!(stderr.contains("--yes"), "{stderr}");
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
    assert_eq!(saved_schema(&project, "dev"), WORKSPACE_SCHEMA, "dev keeps the schema it was generated from");
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
    assert_eq!(saved_schema(&project, "prod"), WORKSPACE_ACCOUNT_SCHEMA);
    assert_eq!(saved_schema(&project, "dev"), WORKSPACE_SCHEMA, "switching to prod leaves dev's saved schema alone");
    let generated = fs::read_to_string(project.join("dinoco/mod.rs")).expect("generated mod");
    assert!(generated.contains("PROD_DATABASE_URL"));
    assert!(generated.contains("PROD_REPLICA_DATABASE_URL"));
    assert!(!generated.contains("DEV_REPLICA_DATABASE_URL"));

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
    assert_eq!(saved_schema(&project, "dev"), WORKSPACE_ACCOUNT_SCHEMA, "generating for dev saves the current schema");

    let run = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "run", "--workspace", "dev"])
        .env("DEV_DATABASE_URL", &dev_db)
        .env("DEV_REPLICA_DATABASE_URL", &dev_replica_db)
        .current_dir(&project)
        .output()
        .expect("dev migrate run should run");
    assert!(run.status.success(), "stderr: {}", String::from_utf8_lossy(&run.stderr));
    assert!(!dev_replica_db.exists(), "migrate run must continue using only the workspace primary");

    // The saved `schema/` folder sits next to dev's migrations without being
    // mistaken for one: generating again plans the Account table normally.
    let again = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate", "-w", "dev"])
        .env("DEV_DATABASE_URL", &dev_db)
        .env("DEV_REPLICA_DATABASE_URL", &dev_replica_db)
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .env("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")
        .current_dir(&project)
        .output()
        .expect("second dev migration should run");
    assert!(again.status.success(), "stderr: {}", String::from_utf8_lossy(&again.stderr));
    let dev_connection = Connection::open(&dev_db).expect("dev primary");
    assert!(table_exists(&dev_connection, "account"));
    let migrations = fs::read_dir(project.join("dinoco/migrations/dev"))
        .expect("dev migrations")
        .map(|entry| entry.expect("entry").file_name().into_string().expect("utf-8 name"))
        .filter(|name| name != "schema")
        .count();
    assert_eq!(migrations, 2, "one migration per generate, besides the saved schema");
}

#[test]
fn a_workspace_applies_the_schema_saved_for_another_workspace() {
    let project = temp_project("workspace-schema-from");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), WORKSPACE_SCHEMA).expect("workspace schema");
    let dev_db = project.join("dev.sqlite");
    let prod_db = project.join("prod.sqlite");
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
            .args(args)
            .env("DEV_DATABASE_URL", &dev_db)
            .env("PROD_DATABASE_URL", &prod_db)
            .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
            .current_dir(&project)
            .output()
            .expect("dinoco cli")
    };
    let stdout = |output: &std::process::Output| String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = |output: &std::process::Output| String::from_utf8_lossy(&output.stderr).into_owned();

    let dev = cli(&["migrate", "generate", "-w", "dev"]);
    assert!(dev.status.success(), "stderr: {}", stderr(&dev));
    assert!(stdout(&dev).contains("Schema      dinoco/schema.dinoco"), "{}", stdout(&dev));

    // Work in progress that dev has not been generated with yet.
    fs::write(project.join("dinoco/schema.dinoco"), WORKSPACE_ACCOUNT_SCHEMA).expect("updated schema");
    let prod = cli(&["migrate", "generate", "-w", "prod", "--schema-from", "dev"]);
    assert!(prod.status.success(), "stderr: {}", stderr(&prod));
    let text = stdout(&prod);
    assert!(text.contains("Schema      dinoco/migrations/dev/schema/ (saved for dev)"), "{text}");
    assert!(text.contains("Schema of `dev` saved to dinoco/migrations/prod/schema (1 file)"), "{text}");
    let prod_connection = Connection::open(&prod_db).expect("prod primary");
    assert!(table_exists(&prod_connection, "user"), "prod gets dev's schema");
    assert!(!table_exists(&prod_connection, "account"), "the work in progress stays out of prod");
    drop(prod_connection);
    assert_eq!(saved_schema(&project, "prod"), WORKSPACE_SCHEMA, "prod's copy is now dev's schema");
    assert_eq!(saved_schema(&project, "dev"), WORKSPACE_SCHEMA);
    assert_eq!(
        fs::read_to_string(project.join("dinoco/schema.dinoco")).expect("schema"),
        WORKSPACE_ACCOUNT_SCHEMA,
        "the schema you edit is never changed"
    );
    // The models come from dev's schema, the connection from prod's config.
    assert!(project.join("dinoco/models/user.rs").exists());
    assert!(!project.join("dinoco/models/account.rs").exists());
    let generated = fs::read_to_string(project.join("dinoco/mod.rs")).expect("generated mod");
    assert!(generated.contains("PROD_DATABASE_URL"), "{generated}");
    assert!(!generated.contains("DEV_DATABASE_URL"), "{generated}");

    // A workspace's own saved schema is what it already has.
    let again = cli(&["migrate", "generate", "-w", "prod", "--schema-from", "prod"]);
    assert!(again.status.success(), "stderr: {}", stderr(&again));
    assert!(stdout(&again).contains("No schema changes were found."), "{}", stdout(&again));

    let models = cli(&["models", "generate", "-w", "dev", "--schema-from", "prod"]);
    assert!(models.status.success(), "stderr: {}", stderr(&models));
    assert!(stdout(&models).contains("Schema of `prod` saved to dinoco/migrations/dev/schema (1 file)"));
    assert!(project.join("dinoco/models/user.rs").exists());

    // Without --schema-from and without a terminal, dinoco/schema.dinoco is used.
    let current = cli(&["migrate", "generate", "-w", "prod"]);
    assert!(current.status.success(), "stderr: {}", stderr(&current));
    let prod_connection = Connection::open(&prod_db).expect("prod primary");
    assert!(table_exists(&prod_connection, "account"));
    drop(prod_connection);
    assert_eq!(saved_schema(&project, "prod"), WORKSPACE_ACCOUNT_SCHEMA);

    let unknown = cli(&["migrate", "generate", "-w", "prod", "--schema-from", "staging"]);
    assert_eq!(unknown.status.code(), Some(2), "{}", stderr(&unknown));
    assert!(stderr(&unknown).contains("Workspace `staging` is not defined in dinoco/schema.dinoco."));
    assert!(stderr(&unknown).contains("--schema-from"), "{}", stderr(&unknown));

    fs::remove_dir_all(project.join("dinoco/migrations/dev/schema")).expect("remove dev copy");
    let missing = cli(&["models", "generate", "-w", "prod", "--schema-from", "dev"]);
    assert_eq!(missing.status.code(), Some(2), "{}", stderr(&missing));
    assert!(stderr(&missing).contains("Workspace `dev` has no saved schema yet."), "{}", stderr(&missing));
    assert!(stderr(&missing).contains("dinoco migrate generate --workspace dev"), "{}", stderr(&missing));
}

#[test]
fn schema_from_needs_workspaces() {
    let project = temp_project("schema-from-without-workspaces");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco directory");
    fs::write(project.join("dinoco/schema.dinoco"), "model User {\n    id String @id\n}\n").expect("schema");

    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["models", "generate", "--schema-from", "dev"])
        .current_dir(&project)
        .output()
        .expect("models generation should run");
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--schema-from was passed, but dinoco/schema.dinoco does not define workspaces.")
    );
    assert!(!project.join("dinoco/models").exists());
}

#[test]
fn workspace_generation_saves_every_file_of_a_multi_file_schema() {
    let project = temp_project("workspace-multi-file-schema");
    fs::create_dir_all(project.join("dinoco/domain")).expect("dinoco directories");
    fs::create_dir_all(project.join("shared")).expect("shared directory");
    let root = r#"config {
    imports = ["domain/business.dinoco", "../shared/common.dinoco"]

    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "sqlite"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
"#;
    let business = r#"import { BusinessStatus } from "../enums.dinoco"

model Business {
    id     String @id
    status BusinessStatus
}
"#;
    let enums = "enum BusinessStatus { active inactive }\n";
    let common = "model Tag {\n    id String @id\n}\n";
    fs::write(project.join("dinoco/schema.dinoco"), root).expect("root schema");
    fs::write(project.join("dinoco/domain/business.dinoco"), business).expect("business schema");
    fs::write(project.join("dinoco/enums.dinoco"), enums).expect("enum schema");
    fs::write(project.join("shared/common.dinoco"), common).expect("shared schema");
    fs::write(project.join("dinoco/notes.dinoco"), "model Unused {\n    id String @id\n}\n").expect("unused schema");

    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["models", "generate", "-w", "dev"])
        .current_dir(&project)
        .output()
        .expect("models generation should run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Schema saved to dinoco/migrations/dev/schema (4 files)"));

    let saved = project.join("dinoco/migrations/dev/schema");
    assert_eq!(fs::read_to_string(saved.join("schema.dinoco")).expect("root copy"), root);
    assert_eq!(fs::read_to_string(saved.join("domain/business.dinoco")).expect("business copy"), business);
    assert_eq!(fs::read_to_string(saved.join("enums.dinoco")).expect("enum copy"), enums);
    assert_eq!(
        fs::read_to_string(saved.join("_external/_up/shared/common.dinoco")).expect("external copy"),
        common,
        "a file imported from outside dinoco/ is kept inside the copy"
    );
    assert!(!saved.join("notes.dinoco").exists(), "only files the schema actually loads are saved");
    assert!(!project.join("dinoco/migrations/prod").exists(), "other workspaces are not touched");

    // Files inside dinoco/ keep their relative paths, so the copy compiles on its own
    // once the external import is pointed at its saved location.
    let copied_root = fs::read_to_string(saved.join("schema.dinoco"))
        .expect("root copy")
        .replace("../shared/common.dinoco", "_external/_up/shared/common.dinoco");
    fs::write(saved.join("schema.dinoco"), copied_root).expect("rewrite copy");
    let copy = dinoco_compiler::compile_file(saved.join("schema.dinoco")).expect("the saved schema compiles");
    assert_eq!(copy.models().map(|model| model.name.as_str()).collect::<Vec<_>>().len(), 2);

    // Dropping an import removes its file from the next copy.
    fs::write(project.join("dinoco/schema.dinoco"), root.replace(", \"../shared/common.dinoco\"", "")).expect("root");
    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["models", "generate", "-w", "dev"])
        .current_dir(&project)
        .output()
        .expect("models generation should run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(!saved.join("_external").exists(), "files the schema no longer loads are removed from the copy");
    assert!(saved.join("domain/business.dinoco").exists());
    assert!(!project.join("dinoco/migrations/dev/.schema.saving").exists(), "no staging folder is left behind");
}

#[test]
fn generation_without_workspaces_saves_no_schema_copy() {
    let project = temp_project("no-workspace-schema-copy");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco directory");
    fs::write(project.join("dinoco/schema.dinoco"), "model User {\n    id String @id\n}\n").expect("schema");

    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["models", "generate"])
        .current_dir(&project)
        .output()
        .expect("models generation should run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(!project.join("dinoco/migrations/schema").exists());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Schema saved"));
}

fn saved_schema(project: &std::path::Path, workspace: &str) -> String {
    fs::read_to_string(project.join(format!("dinoco/migrations/{workspace}/schema/schema.dinoco")))
        .unwrap_or_else(|error| panic!("{workspace} should have a saved schema: {error}"))
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
        on_conflict_do_nothing: None,
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

/// Dinoco must refuse to guess an ambiguous rename rather than silently
/// dropping and recreating columns/tables it cannot prove are related.
#[test]
fn migrate_generate_refuses_an_ambiguous_column_rename() {
    let project = temp_project("ambiguous-rename");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA_WITH_AMBIGUOUS_RENAME_SOURCES)
        .expect("initial schema");

    let db_path = project.join("dev.sqlite");
    let initial = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .current_dir(&project)
        .output()
        .expect("initial migration");
    assert!(initial.status.success(), "stderr: {}", String::from_utf8_lossy(&initial.stderr));
    let migrations_before = fs::read_dir(project.join("dinoco/migrations")).expect("migrations dir").count();

    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA_WITH_AMBIGUOUS_RENAME_TARGET)
        .expect("ambiguous schema change");

    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", db_path.to_string_lossy().as_ref())
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .env("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")
        .current_dir(&project)
        .output()
        .expect("cli should run");

    assert!(!output.status.success(), "ambiguous rename must not exit successfully");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ambiguous"), "stderr: {stderr}");
    assert!(stderr.contains("first_old_name"), "stderr: {stderr}");
    assert!(stderr.contains("second_old_name"), "stderr: {stderr}");

    let migrations_after = fs::read_dir(project.join("dinoco/migrations")).expect("migrations dir").count();
    assert_eq!(migrations_before, migrations_after, "a rejected plan must not write a migration");

    let conn = Connection::open(&db_path).expect("sqlite");
    let columns = conn
        .prepare("PRAGMA table_info(item)")
        .expect("pragma")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names");
    assert!(columns.iter().any(|column| column == "first_old_name"), "rejected plan must not touch the live schema");
    assert!(columns.iter().any(|column| column == "second_old_name"), "rejected plan must not touch the live schema");
}

/// An unreachable database must fail with a message that explains what
/// happened, which database was targeted (credentials redacted), and how to
/// start fixing it - not a bare, unformatted error chain.
#[test]
fn migrate_generate_reports_a_friendly_error_when_the_database_is_unreachable() {
    let project = temp_project("unreachable-db");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), POSTGRES_SCHEMA).expect("schema");

    let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
        .args(["migrate", "generate"])
        .env("DATABASE_URL", "postgresql://app_user:s3cret@127.0.0.1:1/nonexistent")
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .current_dir(&project)
        .output()
        .expect("cli should run");

    assert!(!output.status.success(), "connecting to an unreachable database must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unable to connect to the database"), "stderr: {stderr}");
    assert!(stderr.contains("127.0.0.1:1/nonexistent"), "stderr: {stderr}");
    assert!(!stderr.contains("s3cret"), "credentials must never be printed: {stderr}");
    assert!(stderr.contains("Check that"), "stderr: {stderr}");
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
    user    User? @relation(fields: [user_id], references: [id])
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

const SQLITE_SCHEMA_WITH_AMBIGUOUS_RENAME_SOURCES: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Item {
    id               String  @id @default(uuid())
    first_old_name   String?
    second_old_name  String?
}
"#;

const POSTGRES_SCHEMA: &str = r#"
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
}

model User {
    id    String @id @default(uuid())
    email String
}
"#;

const SQLITE_SCHEMA_WITH_AMBIGUOUS_RENAME_TARGET: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Item {
    id        String  @id @default(uuid())
    new_name  String?
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

/// `dinoco seed` needs exactly one seed file, declared by the generated
/// `dinoco/mod.rs`; each missing piece stops it with exit code 2 and the step
/// that fixes it, before it touches the database or builds anything.
#[test]
fn seed_requires_one_seed_declared_in_dinoco_mod() {
    let project = temp_project("seed-errors");
    fs::create_dir_all(project.join("dinoco/seed")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA).expect("schema");
    let database = project.join("dev.sqlite");
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
            .args(args)
            .env("DATABASE_URL", database.to_string_lossy().as_ref())
            .stdin(Stdio::null())
            .current_dir(&project)
            .output()
            .expect("cli should run")
    };
    let seed = |expected: &str| {
        let output = cli(&["seed"]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
        assert!(stderr.contains(expected), "{stderr}");
        stderr.into_owned()
    };

    let stderr = seed("There is no seed to run.");
    assert!(stderr.contains("dinoco/seed.rs"), "{stderr}");

    const SEED: &str = "pub async fn seed(_: &dinoco::DinocoClient) -> dinoco::anyhow::Result<()> { Ok(()) }\n";
    fs::write(project.join("dinoco/seed.rs"), SEED).expect("seed");
    fs::write(project.join("dinoco/seed/mod.rs"), SEED).expect("seed directory");
    seed("The seed is defined twice.");

    fs::remove_file(project.join("dinoco/seed/mod.rs")).expect("remove seed directory");
    let stderr = seed("dinoco/mod.rs does not declare the seed yet.");
    assert!(stderr.contains("dinoco models generate"), "{stderr}");
    assert!(!database.exists(), "nothing connects to the database before the seed can run");

    // `models generate` declares it, for either layout.
    assert!(cli(&["models", "generate"]).status.success());
    let module = fs::read_to_string(project.join("dinoco/mod.rs")).expect("dinoco/mod.rs");
    assert!(module.contains("\npub mod seed;\n"), "{module}");
    assert!(
        module.contains("fn __dinoco_seed() -> ::dinoco::anyhow::Result<()> {"),
        "the test `dinoco seed` runs: {module}"
    );
    fs::remove_file(project.join("dinoco/seed.rs")).expect("remove seed");
    fs::write(project.join("dinoco/seed/mod.rs"), SEED).expect("seed directory");
    assert!(cli(&["models", "generate"]).status.success());
    let module = fs::read_to_string(project.join("dinoco/mod.rs")).expect("dinoco/mod.rs");
    assert!(module.contains("\npub mod seed;\n"), "{module}");
    fs::remove_dir_all(project.join("dinoco/seed")).expect("remove seed directory");
    assert!(cli(&["models", "generate"]).status.success());
    let module = fs::read_to_string(project.join("dinoco/mod.rs")).expect("dinoco/mod.rs");
    assert!(!module.contains("mod seed"), "a removed seed is no longer declared: {module}");
    assert!(!module.contains("__dinoco_seed"), "nor is its test: {module}");
}

/// The whole flow on a real Cargo project depending on this checkout: the seed
/// runs once inside the application's own crate, here its library, so it
/// reaches the crate's items under `crate::`; a plain `cargo test -- --ignored`
/// skips it, and running it again changes nothing.
#[test]
#[ignore = "builds a Cargo project and its tests (about a minute); run with --ignored"]
fn seed_runs_inside_the_application_crate() {
    let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("checkout").to_path_buf();
    // Shared by every run, so a rerun only rebuilds the project itself.
    let target = checkout.join("target/seed-e2e");
    let project = temp_project("seed-runner");
    fs::create_dir_all(project.join("src")).expect("src");
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(
        project.join("Cargo.toml"),
        format!(
            "[package]\nname = \"seed-runner\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n\n[dependencies]\ndinoco = {{ path = \"{}/crates/dinoco\" }}\ntokio = \"1\"\n",
            checkout.display()
        ),
    )
    .expect("manifest");
    fs::copy(checkout.join("Cargo.lock"), project.join("Cargo.lock")).expect("lockfile");
    fs::write(
        project.join("src/lib.rs"),
        "#[path = \"../dinoco/mod.rs\"]\nmod dinoco;\n\nmod helpers {\n    pub struct Emails;\n\n    impl Emails {\n        pub fn ana() -> String {\n            \"ana@dinoco.rs\".to_string()\n        }\n    }\n}\n\nuse helpers::*;\n",
    )
    .expect("lib");
    fs::write(project.join("src/main.rs"), "fn main() {}\n").expect("main");
    fs::write(project.join("dinoco/schema.dinoco"), SQLITE_SCHEMA).expect("schema");
    fs::write(
        project.join("dinoco/seed.rs"),
        r#"use std::io::Write;

use crate::{Emails, dinoco::models::User};

pub async fn seed(client: &dinoco::DinocoClient) -> dinoco::anyhow::Result<()> {
    dinoco::insert_many::<User>()
        .values([User { id: "u1".to_string(), email: Emails::ana() }])
        .on_conflict_do_nothing(|user| user.id)
        .execute(client)
        .await?;
    // `block_in_place` needs the multi-threaded runtime `#[tokio::main]` builds.
    tokio::task::block_in_place(|| std::fs::OpenOptions::new().create(true).append(true).open("seed-runs.log"))?
        .write_all(b"run\n")?;
    Ok(())
}
"#,
    )
    .expect("seed");
    let database = project.join("dev.sqlite");
    let cli = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"))
            .args(args)
            .env("DATABASE_URL", database.to_string_lossy().as_ref())
            .env("CARGO_TARGET_DIR", &target)
            .stdin(Stdio::null())
            .current_dir(&project)
            .output()
            .expect("cli should run");
        assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let cargo = |args: &[&str]| {
        Command::new(env!("CARGO"))
            .args(args)
            .env("CARGO_TARGET_DIR", &target)
            .current_dir(&project)
            .status()
            .expect("cargo")
            .success()
    };
    let users = || {
        let connection = Connection::open(&database).expect("database");
        connection.query_row("SELECT COUNT(*) FROM \"user\"", [], |row| row.get::<_, i64>(0)).expect("count")
    };
    let runs = || fs::read_to_string(project.join("seed-runs.log")).unwrap_or_default().lines().count();

    cli(&["migrate", "generate", "--yes"]);
    assert!(cargo(&["build", "--quiet"]), "the application compiles with the seed declared");
    assert!(cargo(&["test", "--quiet", "--", "--ignored"]), "the seed's test passes without running it");
    assert_eq!((users(), runs()), (0, 0), "a plain `cargo test -- --ignored` does not run the seed");

    let stdout = cli(&["seed"]);
    assert!(stdout.contains("Seed applied"), "{stdout}");
    assert!(stdout.contains("dinoco/seed.rs"), "{stdout}");
    assert_eq!((users(), runs()), (1, 1), "the seed runs once, in the library");

    cli(&["seed"]);
    assert_eq!((users(), runs()), (1, 2), "running the seed again changes nothing");

    fs::remove_dir_all(&project).expect("remove the project");
}

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
