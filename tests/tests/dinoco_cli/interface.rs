//! The command-line contract: English output, decisions taken from options
//! when no terminal is attached, exit codes, error wording and color rules.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::rusqlite::Connection;

const DECISION_REQUIRED: i32 = 2;

#[test]
fn migrate_generate_help_lists_every_decision_option() {
    let output = cli(&std::env::temp_dir(), &["migrate", "generate", "--help"], &[]);

    assert!(output.status.success(), "{}", combined(&output));
    let help = stdout(&output);
    for option in [
        "--workspace <NAME>",
        "--schema-from <NAME>",
        "--yes",
        "--allow-destructive",
        "--create-only",
        "--accept-renames",
        "--rename <MAPPING>",
        "--no-rename <MAPPING>",
        "--replace-enum-value <MAPPING>",
        "--repair-drift",
        "--adopt-baseline",
        "--non-interactive",
        "--no-color",
        "Examples:",
    ] {
        assert!(help.contains(option), "missing {option}: {help}");
    }
}

#[test]
fn init_without_a_terminal_requires_the_database_option() {
    let project = temp_project("init-non-interactive");

    let output = cli(&project, &["init"], &[]);

    assert_eq!(output.status.code(), Some(DECISION_REQUIRED), "{}", combined(&output));
    let stderr = stderr(&output);
    assert!(stderr.contains("error: A database must be selected to initialize the project."), "{stderr}");
    assert!(stderr.contains("help: Pass --database <postgresql|mysql|sqlite>."), "{stderr}");
    assert!(stdout(&output).is_empty(), "errors belong on standard error: {}", stdout(&output));
    assert!(!project.join("dinoco").exists(), "nothing is created without a decision");
}

#[test]
fn init_takes_its_answers_from_options() {
    let project = temp_project("init-options");

    let output = cli(&project, &["init", "--database", "sqlite"], &[]);

    assert!(output.status.success(), "{}", combined(&output));
    let schema = fs::read_to_string(project.join("dinoco/schema.dinoco")).expect("schema");
    assert!(schema.contains("\"sqlite\""), "{schema}");
    assert!(!schema.contains("connection"), "{schema}");
    assert!(stdout(&output).contains("SQLite"));

    let other = temp_project("init-invalid-connection");
    let invalid = cli(&other, &["init", "--database", "sqlite", "--connection", "direct"], &[]);
    assert_eq!(invalid.status.code(), Some(2), "{}", combined(&invalid));
    assert!(stderr(&invalid).contains("--connection only applies to PostgreSQL projects."));

    let unknown = cli(&other, &["init", "--database", "oracle"], &[]);
    assert_eq!(unknown.status.code(), Some(2), "{}", combined(&unknown));
    assert!(stderr(&unknown).contains("invalid value 'oracle'"), "{}", stderr(&unknown));
}

#[test]
fn a_workspace_is_never_chosen_for_the_user() {
    let project = temp_project("workspace-required");
    write_schema(&project, TWO_WORKSPACES_SCHEMA);

    let missing = cli(&project, &["models", "generate"], &[]);
    assert_eq!(missing.status.code(), Some(DECISION_REQUIRED), "{}", combined(&missing));
    let stderr_text = stderr(&missing);
    assert!(stderr_text.contains("error: A workspace must be selected."), "{stderr_text}");
    assert!(stderr_text.contains("dev, prod"), "{stderr_text}");
    assert!(stderr_text.contains("--workspace"), "{stderr_text}");
    assert!(!project.join("dinoco/models").exists());

    let unknown = cli(&project, &["models", "generate", "--workspace", "staging"], &[]);
    assert_eq!(unknown.status.code(), Some(2), "{}", combined(&unknown));
    assert!(stderr(&unknown).contains("Workspace `staging` is not defined in dinoco/schema.dinoco."));
    assert!(stderr(&unknown).contains("dev, prod"));

    let selected = cli(&project, &["models", "generate", "-w", "prod"], &[]);
    assert!(selected.status.success(), "{}", combined(&selected));
    assert!(stdout(&selected).contains("Workspace  prod"), "{}", stdout(&selected));
}

#[test]
fn the_only_workspace_is_used_and_shown() {
    let project = temp_project("single-workspace");
    write_schema(&project, ONE_WORKSPACE_SCHEMA);

    let output = cli(&project, &["models", "generate"], &[]);

    assert!(output.status.success(), "{}", combined(&output));
    assert!(stdout(&output).contains("Workspace  dev"), "{}", stdout(&output));
}

#[test]
fn create_only_writes_the_migration_and_migrate_run_applies_it() {
    let project = temp_project("create-only");
    write_schema(&project, USER_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let env = [("DATABASE_URL", path_str(&db_path))];

    let created = cli(&project, &["migrate", "generate", "--create-only", "--yes"], &env);

    assert!(created.status.success(), "{}", combined(&created));
    let output = stdout(&created);
    assert!(output.contains("Migration created:"), "{output}");
    assert!(output.contains("It was not applied to the database."), "{output}");
    assert!(output.contains("dinoco migrate run"), "{output}");
    assert_eq!(migration_dirs(&project).len(), 1);
    assert!(!project.join("dinoco/models").exists(), "--create-only does not regenerate the models");
    assert!(!table_exists(&db_path, "user"), "--create-only must not change the database");

    let run = cli(&project, &["migrate", "run"], &env);
    assert!(run.status.success(), "{}", combined(&run));
    assert!(stdout(&run).contains("Applied 1 migration."), "{}", stdout(&run));
    assert!(table_exists(&db_path, "user"));

    let again = cli(&project, &["migrate", "run"], &env);
    assert!(again.status.success(), "{}", combined(&again));
    assert!(stdout(&again).contains("The database is up to date."), "{}", stdout(&again));

    let stable = cli(&project, &["migrate", "generate"], &env);
    assert!(stable.status.success(), "{}", combined(&stable));
    assert!(stdout(&stable).contains("No schema changes were found."), "{}", stdout(&stable));
}

#[test]
fn destructive_plans_need_allow_destructive() {
    let project = temp_project("destructive");
    write_schema(&project, USER_WITH_PASSWORD_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let env = [("DATABASE_URL", path_str(&db_path))];
    assert!(cli(&project, &["migrate", "generate", "--yes"], &env).status.success());

    write_schema(&project, USER_SCHEMA);
    let refused = cli(&project, &["migrate", "generate", "--yes"], &env);

    assert_eq!(refused.status.code(), Some(DECISION_REQUIRED), "{}", combined(&refused));
    // SQLite drops a column by rebuilding its table.
    assert!(stdout(&refused).contains("~ Rebuild table `user`: Drop column `user.password`"), "{}", stdout(&refused));
    assert!(stdout(&refused).contains("Data Impact"), "{}", stdout(&refused));
    assert!(stderr(&refused).contains("This migration contains destructive changes and needs explicit approval."));
    assert!(stderr(&refused).contains("--allow-destructive"));
    assert!(columns(&db_path, "user").contains(&"password".to_string()), "a refused plan changes nothing");
    assert_eq!(migration_dirs(&project).len(), 1);

    let approved = cli(&project, &["migrate", "generate", "--allow-destructive"], &env);
    assert!(approved.status.success(), "{}", combined(&approved));
    assert!(!columns(&db_path, "user").contains(&"password".to_string()));
}

#[test]
fn inferred_renames_are_confirmed_or_rejected_explicitly() {
    let project = temp_project("renames");
    write_schema(&project, NICKNAME_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let env = [("DATABASE_URL", path_str(&db_path))];
    assert!(cli(&project, &["migrate", "generate", "--yes"], &env).status.success());
    execute(&db_path, "INSERT INTO user (id, nickname) VALUES ('u1', 'Preserved')");

    write_schema(&project, DISPLAY_NAME_SCHEMA);
    let undecided = cli(&project, &["migrate", "generate", "--allow-destructive"], &env);
    assert_eq!(undecided.status.code(), Some(DECISION_REQUIRED), "{}", combined(&undecided));
    let stderr_text = stderr(&undecided);
    assert!(stderr_text.contains("The migration plan contains renames that need confirmation."), "{stderr_text}");
    assert!(stderr_text.contains("--rename user.nickname=display_name"), "{stderr_text}");
    assert!(stderr_text.contains("--no-rename user.nickname=display_name"), "{stderr_text}");
    assert_eq!(migration_dirs(&project).len(), 1);

    let typo = cli(&project, &["migrate", "generate", "--yes", "--rename", "user.nick=display_name"], &env);
    assert_eq!(typo.status.code(), Some(2), "{}", combined(&typo));
    assert!(stderr(&typo).contains("does not match any rename"), "{}", stderr(&typo));

    let malformed = cli(&project, &["migrate", "generate", "--yes", "--rename", "nickname"], &env);
    assert_eq!(malformed.status.code(), Some(2), "{}", combined(&malformed));
    assert!(stderr(&malformed).contains("--rename expects TABLE.OLD=NEW"), "{}", stderr(&malformed));

    // A confirmed rename is not destructive, so --yes is enough.
    let renamed = cli(&project, &["migrate", "generate", "--yes", "--rename", "user.nickname=display_name"], &env);
    assert!(renamed.status.success(), "{}", combined(&renamed));
    assert!(stdout(&renamed).contains("~ Rename column `user.nickname` to `display_name`"), "{}", stdout(&renamed));
    assert_eq!(query_text(&db_path, "SELECT display_name FROM user WHERE id = 'u1'"), Some("Preserved".to_string()));

    write_schema(&project, NICKNAME_SCHEMA);
    let rejected = cli(
        &project,
        &["migrate", "generate", "--allow-destructive", "--no-rename", "user.display_name=nickname"],
        &env,
    );
    assert!(rejected.status.success(), "{}", combined(&rejected));
    let plan = stdout(&rejected);
    assert!(plan.contains("Drop column `user.display_name`") && plan.contains("Add column `user.nickname`"), "{plan}");
    assert_eq!(query_text(&db_path, "SELECT nickname FROM user WHERE id = 'u1'"), None);
}

#[test]
fn removed_enum_values_in_use_need_a_replacement() {
    let project = temp_project("enum-replacement");
    write_schema(&project, &task_schema("ACTIVE\n    PROCESSING\n    DONE"));
    let db_path = project.join("dev.sqlite");
    let env = [("DATABASE_URL", path_str(&db_path))];
    assert!(cli(&project, &["migrate", "generate", "--yes"], &env).status.success());
    execute(
        &db_path,
        "INSERT INTO task (id, status) VALUES ('t1', 'PROCESSING'), ('t2', 'PROCESSING'), ('t3', 'ACTIVE')",
    );

    write_schema(&project, &task_schema("ACTIVE\n    DONE"));
    let undecided = cli(&project, &["migrate", "generate", "--allow-destructive"], &env);
    assert_eq!(undecided.status.code(), Some(DECISION_REQUIRED), "{}", combined(&undecided));
    let stderr_text = stderr(&undecided);
    assert!(stderr_text.contains("Removed enum values are still used by existing rows."), "{stderr_text}");
    assert!(stderr_text.contains("Status.PROCESSING (2 rows; replace with ACTIVE, DONE)"), "{stderr_text}");
    assert!(stderr_text.contains("--replace-enum-value Status.PROCESSING=ACTIVE"), "{stderr_text}");

    let invalid = cli(
        &project,
        &["migrate", "generate", "--allow-destructive", "--replace-enum-value", "Status.PROCESSING=LATER"],
        &env,
    );
    assert_eq!(invalid.status.code(), Some(2), "{}", combined(&invalid));
    assert!(stderr(&invalid).contains("names an invalid replacement"), "{}", stderr(&invalid));
    assert_eq!(migration_dirs(&project).len(), 1);

    let replaced = cli(
        &project,
        &["migrate", "generate", "--allow-destructive", "--replace-enum-value", "Status.PROCESSING=DONE"],
        &env,
    );
    assert!(replaced.status.success(), "{}", combined(&replaced));
    assert!(
        stdout(&replaced).contains("~ Replace `PROCESSING` with `DONE` in `task.status` (2 rows)"),
        "{}",
        stdout(&replaced)
    );
    assert_eq!(query_text(&db_path, "SELECT COUNT(*) FROM task WHERE status = 'DONE'"), Some("2".to_string()));
    assert_eq!(query_text(&db_path, "SELECT COUNT(*) FROM task WHERE status = 'ACTIVE'"), Some("1".to_string()));
    let up = fs::read_to_string(migration_dirs(&project).last().unwrap().join("up.sql")).expect("up.sql");
    assert!(up.contains(r#"UPDATE "task" SET "status" = 'DONE' WHERE "status" = 'PROCESSING';"#), "{up}");
    let down = fs::read_to_string(migration_dirs(&project).last().unwrap().join("down.sql")).expect("down.sql");
    assert!(down.contains("were changed from `PROCESSING` to `DONE`"), "{down}");
}

#[test]
fn colors_follow_no_color_clicolor_force_and_the_option() {
    let forced = cli(&temp_project("color-forced"), &["init", "--database", "sqlite"], &[("CLICOLOR_FORCE", "1")]);
    assert!(forced.status.success(), "{}", combined(&forced));
    assert!(stdout(&forced).contains('\u{1b}'), "CLICOLOR_FORCE enables colors: {}", stdout(&forced));

    let no_color = cli(
        &temp_project("color-no-color"),
        &["init", "--database", "sqlite"],
        &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")],
    );
    assert!(no_color.status.success(), "{}", combined(&no_color));
    assert!(!stdout(&no_color).contains('\u{1b}'), "NO_COLOR wins: {}", stdout(&no_color));

    let option =
        cli(&temp_project("color-option"), &["--no-color", "init", "--database", "sqlite"], &[("CLICOLOR_FORCE", "1")]);
    assert!(option.status.success(), "{}", combined(&option));
    assert!(!stdout(&option).contains('\u{1b}'), "--no-color wins: {}", stdout(&option));

    let plain = cli(&temp_project("color-redirected"), &["init", "--database", "sqlite"], &[]);
    assert!(!stdout(&plain).contains('\u{1b}'), "redirected output has no colors: {}", stdout(&plain));
}

#[test]
fn errors_say_what_failed_and_what_to_do_next() {
    let empty = temp_project("missing-schema");
    let missing_schema = cli(&empty, &["migrate", "generate"], &[]);
    assert_eq!(missing_schema.status.code(), Some(1), "{}", combined(&missing_schema));
    assert!(stderr(&missing_schema).contains("error: dinoco/schema.dinoco was not found in the current directory."));
    assert!(stderr(&missing_schema).contains("help: Run `dinoco init`"));
    assert!(stdout(&missing_schema).is_empty());

    let project = temp_project("missing-env");
    write_schema(&project, USER_SCHEMA);
    let missing_env = cli(&project, &["migrate", "run"], &[]);
    assert_eq!(missing_env.status.code(), Some(1), "{}", combined(&missing_env));
    assert!(stderr(&missing_env).contains("error: The environment variable `DATABASE_URL` is not set."));
    assert!(stderr(&missing_env).contains("help: Set DATABASE_URL in your environment or in a .env file"));
}

#[test]
fn connection_errors_never_print_credentials() {
    let project = temp_project("connection-secrets");
    write_schema(&project, POSTGRES_SCHEMA);
    let url = "postgresql://app:s3cret@127.0.0.1:1/app_db?sslmode=disable&password=t0ken";

    let output = cli(&project, &["migrate", "generate", "--yes"], &[("DATABASE_URL", url)]);

    assert_eq!(output.status.code(), Some(1), "{}", combined(&output));
    let all = combined(&output);
    assert!(!all.contains("s3cret"), "{all}");
    assert!(!all.contains("t0ken"), "{all}");
    let stderr_text = stderr(&output);
    assert!(stderr_text.contains("error: Unable to connect to the database."), "{stderr_text}");
    assert!(stderr_text.contains("postgresql://***@127.0.0.1:1/app_db?sslmode=disable&password=***"), "{stderr_text}");
    assert!(stderr_text.contains("help: Check that"), "{stderr_text}");
}

fn cli(project: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"));
    command.args(args).current_dir(project).stdin(Stdio::null());
    for key in [
        "DATABASE_URL",
        "NO_COLOR",
        "CLICOLOR_FORCE",
        "DINOCO_CLI_CONFIRM_MIGRATION",
        "DINOCO_CLI_CONFIRM_DESTRUCTIVE",
        "DINOCO_CLI_CONFIRM_DRIFT",
        "DINOCO_CLI_INIT_DATABASE",
        "DINOCO_CLI_INIT_POSTGRES_CONNECTION",
    ] {
        command.env_remove(key);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("dinoco cli")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn combined(output: &Output) -> String {
    format!("{}\n{}", stdout(output), stderr(output))
}

fn temp_project(name: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("dinoco-cli-ux-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).expect("temp project");
    path
}

fn write_schema(project: &Path, schema: &str) {
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), schema).expect("schema");
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
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

fn table_exists(db_path: &Path, table: &str) -> bool {
    if !db_path.exists() {
        return false;
    }
    Connection::open(db_path)
        .expect("sqlite")
        .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)", [table], |row| {
            row.get(0)
        })
        .expect("table existence query")
}

fn columns(db_path: &Path, table: &str) -> Vec<String> {
    let connection = Connection::open(db_path).expect("sqlite");
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})")).expect("pragma");
    statement.query_map([], |row| row.get::<_, String>(1)).expect("columns").collect::<Result<_, _>>().expect("names")
}

fn execute(db_path: &Path, sql: &str) {
    Connection::open(db_path).expect("sqlite").execute_batch(sql).expect("statement");
}

fn query_text(db_path: &Path, sql: &str) -> Option<String> {
    Connection::open(db_path)
        .expect("sqlite")
        .query_row(sql, [], |row| row.get::<_, Option<dinoco_engine::rusqlite::types::Value>>(0))
        .expect("query")
        .map(|value| match value {
            dinoco_engine::rusqlite::types::Value::Integer(value) => value.to_string(),
            dinoco_engine::rusqlite::types::Value::Text(value) => value,
            other => format!("{other:?}"),
        })
}

fn task_schema(values: &str) -> String {
    format!(
        r#"
config {{
    database = "sqlite"
    database_url = env("DATABASE_URL")
}}

enum Status {{
    {values}
}}

model Task {{
    id     String @id
    status Status
}}
"#
    )
}

const USER_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id    String @id @default(uuid())
    email String
}
"#;

const USER_WITH_PASSWORD_SCHEMA: &str = r#"
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

const NICKNAME_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id       String  @id
    nickname String?
}
"#;

const DISPLAY_NAME_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id           String  @id
    display_name String?
}
"#;

const POSTGRES_SCHEMA: &str = r#"
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
}

model User {
    id String @id @default(uuid())
}
"#;

const TWO_WORKSPACES_SCHEMA: &str = r#"
config {
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

model User {
    id String @id @default(uuid())
}
"#;

const ONE_WORKSPACE_SCHEMA: &str = r#"
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }
    }
}

model User {
    id String @id @default(uuid())
}
"#;
