use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_cli::db::{CliDatabase, DatabaseSchema, DatabaseTable};
use dinoco_cli::schema::{Database, PostgresConnection, RuntimeConfig};
use dinoco_cli::sql::{MigrationStep, plan_database_migration};
use dinoco_engine::{MigrationColumn, MigrationColumnType, MigrationDefault, rusqlite::Connection};

#[test]
fn generate_rejects_unilateral_relation_before_creating_migration_state() {
    let project = temp_project("unilateral-relation");
    write_project_schema(&project, UNILATERAL_RELATION_SCHEMA);
    let db_path = project.join("dev.sqlite");

    let generate = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert!(!generate.status.success(), "{}", combined_output(&generate));
    assert!(combined_output(&generate).to_ascii_lowercase().contains("relation"));
    assert!(!project.join("dinoco/migrations").exists());
    assert!(!db_path.exists());
}

#[test]
fn generate_repairs_missing_tables_without_recreating_populated_tables() {
    let project = temp_project("repair-missing-tables");
    write_project_schema(&project, THREE_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");

    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("INSERT INTO account (id, name) VALUES (1001, 'Preserved')", []).expect("seed account");
    let account_sql_before = table_sql(&conn, "account");
    drop(conn);

    let populated_but_stable = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&populated_but_stable);
    assert!(!combined_output(&populated_but_stable).contains("schema drift"));

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("DROP TABLE business", []).expect("drop business");
    conn.execute("DROP TABLE address", []).expect("drop address");
    drop(conn);

    let repair = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DRIFT", "true")]);
    assert_success(&repair);
    let output = combined_output(&repair);
    assert!(output.contains("Database schema drift detected"), "{output}");
    assert!(output.contains("Missing table `business`"), "{output}");
    assert!(output.contains("Missing table `address`"), "{output}");
    assert!(output.contains("no new migration was needed"), "{output}");

    let migrations = migration_dirs(&project);
    assert_eq!(migrations.len(), 1, "repairing drift must not create redundant schema history");

    let conn = Connection::open(&db_path).expect("sqlite");
    let account: (i64, String) = conn
        .query_row("SELECT COUNT(*), MAX(name) FROM account", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("account data");
    assert_eq!(account, (1, "Preserved".to_string()));
    assert!(table_exists(&conn, "business"));
    assert!(table_exists(&conn, "address"));
    assert_eq!(table_sql(&conn, "account"), account_sql_before);
    drop(conn);

    let stable = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&stable);
    assert!(combined_output(&stable).contains("No schema changes were found"));
    assert_eq!(migration_dirs(&project).len(), 1);
}

#[test]
fn drift_repair_is_not_embedded_in_an_unrelated_schema_migration() {
    let project = temp_project("repair-plus-schema-change");
    write_project_schema(&project, THREE_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("INSERT INTO account (id, name) VALUES (1001, 'Preserved')", []).expect("seed account");
    conn.execute("DROP TABLE business", []).expect("drop business");
    conn.execute("DROP TABLE address", []).expect("drop address");
    drop(conn);
    write_project_schema(&project, FOUR_TABLE_SCHEMA);

    let generate = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DRIFT", "true")]);
    assert_success(&generate);
    let migrations = migration_dirs(&project);
    assert_eq!(migrations.len(), 2);
    let up_sql = fs::read_to_string(migrations.last().unwrap().join("up.sql")).expect("up.sql");
    assert!(up_sql.contains("CREATE TABLE location"), "{up_sql}");
    assert!(!up_sql.contains("CREATE TABLE address"), "{up_sql}");
    assert!(!up_sql.contains("CREATE TABLE business"), "{up_sql}");
    assert!(!up_sql.contains("CREATE TABLE account"), "{up_sql}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let account_rows: i64 = conn.query_row("SELECT COUNT(*) FROM account", [], |row| row.get(0)).expect("account rows");
    assert_eq!(account_rows, 1);
    assert!(table_exists(&conn, "address"));
    assert!(table_exists(&conn, "business"));
    assert!(table_exists(&conn, "location"));
}

#[test]
fn baseline_adopts_a_populated_table_without_dropping_it_in_down_sql() {
    let project = temp_project("safe-baseline-adoption");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("CREATE TABLE account (id TEXT PRIMARY KEY NOT NULL)", []).expect("preexisting account");
    conn.execute("INSERT INTO account (id) VALUES ('preserved-account')", []).expect("seed account");
    drop(conn);

    let generate = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DRIFT", "true")]);
    assert_success(&generate);

    let migration = migration_dirs(&project).into_iter().next().expect("baseline migration");
    let down_sql = fs::read_to_string(migration.join("down.sql")).expect("down.sql");
    assert!(down_sql.contains("was only adopted"), "{down_sql}");
    assert!(!down_sql.to_ascii_lowercase().contains("drop table account"), "{down_sql}");

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute_batch(&down_sql).expect("conservative rollback");
    let account_id: String = conn.query_row("SELECT id FROM account", [], |row| row.get(0)).expect("preserved account");
    assert_eq!(account_id, "preserved-account");
    let history_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migrations", [], |row| row.get(0)).expect("history");
    assert_eq!(history_rows, 1, "a non-reversible adoption must retain its history record");
    let checksum_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migration_checksums", [], |row| row.get(0)).expect("checksums");
    assert_eq!(checksum_rows, 1);
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&run);
    assert!(combined_output(&run).contains("Skipping already applied migration"));
}

#[test]
fn reversible_down_is_atomic_and_can_be_applied_again() {
    let project = temp_project("reversible-down");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let generate = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&generate);

    let migration = migration_dirs(&project).into_iter().next().expect("migration");
    let down_sql = fs::read_to_string(migration.join("down.sql")).expect("down.sql");
    assert!(down_sql.starts_with("PRAGMA foreign_keys = ON;\n\nBEGIN IMMEDIATE;"), "{down_sql}");
    let drop_position = down_sql.find("DROP TABLE IF EXISTS account").expect("drop account");
    let metadata_position = down_sql.find("DELETE FROM dinoco_migration_checksums").expect("checksum delete");
    assert!(drop_position < metadata_position, "schema must roll back before metadata: {down_sql}");

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute_batch(&down_sql).expect("down migration");
    assert!(!table_exists(&conn, "account"));
    let history_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migrations", [], |row| row.get(0)).expect("history");
    assert_eq!(history_rows, 0);
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&run);
    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(table_exists(&conn, "account"));
    let history_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migrations", [], |row| row.get(0)).expect("history");
    assert_eq!(history_rows, 1);
}

#[test]
fn baseline_refuses_to_reconcile_an_untracked_database_implicitly() {
    let project = temp_project("unsafe-baseline-reconciliation");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("CREATE TABLE account (id TEXT PRIMARY KEY NOT NULL)", []).expect("preexisting account");
    conn.execute("CREATE TABLE legacy_data (id TEXT PRIMARY KEY NOT NULL)", []).expect("legacy table");
    conn.execute("INSERT INTO legacy_data (id) VALUES ('must-survive')", []).expect("legacy row");
    drop(conn);

    let generate = run_cli(
        &project,
        &db_path,
        &["migrate", "generate"],
        &[("DINOCO_CLI_CONFIRM_DRIFT", "true"), ("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")],
    );
    assert!(!generate.status.success(), "{}", combined_output(&generate));
    let output = combined_output(&generate);
    assert!(output.contains("will not modify an existing untracked database"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let legacy_id: String = conn.query_row("SELECT id FROM legacy_data", [], |row| row.get(0)).expect("legacy row");
    assert_eq!(legacy_id, "must-survive");
    assert!(!table_exists(&conn, "dinoco_migrations"));
    assert!(!table_exists(&conn, "dinoco_migration_checksums"));
    assert!(migration_dirs(&project).is_empty());
}

#[test]
fn modified_applied_migration_is_rejected_by_checksum() {
    let project = temp_project("migration-checksum");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let migration = migration_dirs(&project).into_iter().next().expect("migration");
    let up_path = migration.join("up.sql");
    let mut up_sql = fs::read_to_string(&up_path).expect("up.sql");
    assert!(up_sql.contains("CREATE TABLE IF NOT EXISTS dinoco_migrations"), "{up_sql}");
    assert!(up_sql.contains("CREATE TABLE IF NOT EXISTS dinoco_migration_checksums"), "{up_sql}");
    assert!(up_sql.contains("-- dinoco-checksum: "), "{up_sql}");
    assert!(up_sql.starts_with("PRAGMA foreign_keys = ON;\n\nBEGIN IMMEDIATE;"), "{up_sql}");

    let failed_direct_db = project.join("failed-direct-apply.sqlite");
    let failed_direct = Connection::open(&failed_direct_db).expect("failed direct sqlite");
    let invalid_up_sql = up_sql.replacen("COMMIT;", "THIS IS NOT VALID SQL;\n\nCOMMIT;", 1);
    failed_direct.execute_batch(&invalid_up_sql).expect_err("a broken direct up.sql must not commit partial schema");
    drop(failed_direct);
    let failed_direct = Connection::open(&failed_direct_db).expect("reopen failed direct sqlite");
    assert!(!table_exists(&failed_direct, "account"));
    assert!(!table_exists(&failed_direct, "dinoco_migrations"));
    assert!(!table_exists(&failed_direct, "dinoco_migration_checksums"));
    drop(failed_direct);

    let direct_db = project.join("direct-apply.sqlite");
    let direct = Connection::open(&direct_db).expect("direct sqlite");
    direct.execute_batch(&up_sql).expect("up.sql must remain self-contained");
    assert!(table_exists(&direct, "account"));
    let direct_history: i64 =
        direct.query_row("SELECT COUNT(*) FROM dinoco_migrations", [], |row| row.get(0)).expect("direct history");
    let direct_checksums: i64 = direct
        .query_row("SELECT COUNT(*) FROM dinoco_migration_checksums", [], |row| row.get(0))
        .expect("direct checksums");
    assert_eq!((direct_history, direct_checksums), (1, 1));
    drop(direct);

    up_sql.push_str("\n-- modified after apply\n");
    fs::write(&up_path, up_sql).expect("tampered up.sql");

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    let output = combined_output(&run);
    assert!(output.contains("checksum"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(table_exists(&conn, "account"));
    let checksum_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM dinoco_migration_checksums", [], |row| row.get(0))
        .expect("checksum count");
    assert_eq!(checksum_rows, 1);
}

#[test]
fn missing_checksum_table_for_generated_history_is_an_integrity_error() {
    let project = temp_project("missing-checksum-table");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("DROP TABLE dinoco_migration_checksums", []).expect("drop checksums");
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    let output = combined_output(&run);
    assert!(output.contains("checksum metadata is missing"), "{output}");
    assert!(output.contains("trusted backup"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(!table_exists(&conn, "dinoco_migration_checksums"));
    assert!(table_exists(&conn, "account"));
}

#[test]
fn legacy_migration_layout_and_history_are_adopted() {
    let project = temp_project("legacy-layout-adoption");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_legacy");
    fs::create_dir_all(&migration).expect("legacy migration directory");
    fs::write(migration.join("migration.sql"), "CREATE TABLE account (id TEXT PRIMARY KEY NOT NULL);")
        .expect("legacy migration.sql");
    fs::write(migration.join("schema.bin"), b"legacy snapshot is retained").expect("legacy schema.bin");

    let conn = Connection::open(&db_path).expect("legacy sqlite");
    conn.execute_batch(
        "CREATE TABLE account (id TEXT PRIMARY KEY NOT NULL);
         INSERT INTO account (id) VALUES ('must-survive');
         CREATE TABLE _dinoco_migrations (
             name TEXT PRIMARY KEY NOT NULL,
             applied_at TEXT NULL,
             rollback_at TEXT NULL
         );
         INSERT INTO _dinoco_migrations (name, applied_at, rollback_at)
         VALUES ('001_legacy', CURRENT_TIMESTAMP, NULL);",
    )
    .expect("legacy database state");
    drop(conn);

    let generate = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&generate);
    let output = combined_output(&generate);
    assert!(output.contains("Upgraded 1 legacy migration(s)"), "{output}");
    assert!(output.contains("No schema changes were found"), "{output}");
    assert_eq!(
        fs::read_to_string(migration.join("up.sql")).expect("upgraded up.sql"),
        "CREATE TABLE account (id TEXT PRIMARY KEY NOT NULL);"
    );
    assert!(migration.join("down.sql").is_file());
    assert!(migration.join("migration.sql").is_file());
    assert!(migration.join("schema.bin").is_file());

    let conn = Connection::open(&db_path).expect("upgraded sqlite");
    let adopted: i64 = conn
        .query_row("SELECT COUNT(*) FROM dinoco_migrations WHERE name = '001_legacy'", [], |row| row.get(0))
        .expect("adopted migration row");
    let checksum: i64 = conn
        .query_row("SELECT COUNT(*) FROM dinoco_migration_checksums WHERE name = '001_legacy'", [], |row| row.get(0))
        .expect("legacy checksum");
    assert_eq!((adopted, checksum), (1, 1));
    let preserved_id: String = conn.query_row("SELECT id FROM account", [], |row| row.get(0)).expect("account data");
    assert_eq!(preserved_id, "must-survive");
    assert!(table_exists(&conn, "_dinoco_migrations"), "legacy metadata must be retained");
}

#[test]
fn missing_checksum_table_for_new_custom_history_is_an_integrity_error() {
    let project = temp_project("missing-custom-checksum-table");
    write_project_schema(&project, CUSTOM_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_custom");
    fs::create_dir_all(&migration).expect("migration dir");
    let up_path = migration.join("up.sql");
    fs::write(&up_path, "CREATE TABLE custom_table (id TEXT PRIMARY KEY NOT NULL);").expect("up.sql");
    let initial = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("DROP TABLE dinoco_migration_checksums", []).expect("drop checksums");
    drop(conn);
    fs::write(
        &up_path,
        "CREATE TABLE custom_table (id TEXT PRIMARY KEY NOT NULL);\n-- modified after metadata deletion",
    )
    .expect("tampered up.sql");

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    let output = combined_output(&run);
    assert!(output.contains("this database requires"), "{output}");
    assert!(output.contains("trusted backup"), "{output}");
}

#[test]
fn orphaned_checksum_row_is_rejected_before_a_migration_can_run_again() {
    let project = temp_project("orphaned-checksum");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("DELETE FROM dinoco_migrations", []).expect("remove history only");
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    let output = combined_output(&run);
    assert!(output.contains("checksum rows exist without matching applied history"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(table_exists(&conn, "account"));
    let checksum_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migration_checksums", [], |row| row.get(0)).expect("checksums");
    assert_eq!(checksum_rows, 1);
}

#[test]
fn missing_checksum_row_is_an_integrity_error_not_a_legacy_migration() {
    let project = temp_project("missing-checksum-row");
    write_project_schema(&project, CUSTOM_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_custom");
    fs::create_dir_all(&migration).expect("migration dir");
    let up_path = migration.join("up.sql");
    fs::write(&up_path, "CREATE TABLE custom_table (id TEXT PRIMARY KEY NOT NULL);").expect("up.sql");

    let first = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&first);
    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("DELETE FROM dinoco_migration_checksums WHERE name = '001_custom'", [])
        .expect("remove checksum fixture");
    drop(conn);
    fs::write(
        &up_path,
        "CREATE TABLE custom_table (id TEXT PRIMARY KEY NOT NULL);\n-- changed after checksum deletion",
    )
    .expect("tampered up.sql");

    let second = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!second.status.success(), "{}", combined_output(&second));
    let output = combined_output(&second);
    assert!(output.contains("checksum metadata is incomplete"), "{output}");
    assert!(output.contains("001_custom"), "{output}");
}

#[test]
fn old_generated_sqlite_artifact_can_be_replayed_applied_and_checksummed() {
    let project = temp_project("old-generated-artifact");
    write_project_schema(&project, EMPTY_SCHEMA);
    let migration_name = "001_old_generated";
    let migration = project.join("dinoco/migrations").join(migration_name);
    fs::create_dir_all(&migration).expect("migration dir");
    let old_up_sql = format!(
        "CREATE TABLE IF NOT EXISTS dinoco_migrations (
             name TEXT PRIMARY KEY,
             applied_at TEXT DEFAULT CURRENT_TIMESTAMP
         );

         CREATE TABLE legacy_generated (id TEXT PRIMARY KEY NOT NULL);

         INSERT OR IGNORE INTO dinoco_migrations (name) VALUES ('{migration_name}');"
    );
    fs::write(migration.join("up.sql"), &old_up_sql).expect("legacy up.sql");

    let pending_db = project.join("pending.sqlite");
    let pending = run_cli(&project, &pending_db, &["migrate", "run"], &[]);
    assert_success(&pending);
    let conn = Connection::open(&pending_db).expect("pending sqlite");
    assert!(table_exists(&conn, "legacy_generated"));
    assert_migration_checksum_recorded(&conn, migration_name);
    drop(conn);

    let already_applied_db = project.join("already-applied.sqlite");
    let conn = Connection::open(&already_applied_db).expect("already-applied sqlite");
    conn.execute_batch(&old_up_sql).expect("apply legacy artifact with the old runner semantics");
    assert!(!table_exists(&conn, "dinoco_migration_checksums"));
    drop(conn);

    let replay = run_cli(&project, &already_applied_db, &["migrate", "run"], &[]);
    assert_success(&replay);
    let conn = Connection::open(&already_applied_db).expect("replayed sqlite");
    assert!(table_exists(&conn, "legacy_generated"));
    assert_migration_checksum_recorded(&conn, migration_name);
}

#[test]
fn legacy_history_replay_cannot_attach_an_external_sqlite_database() {
    let project = temp_project("safe-history-replay");
    write_project_schema(&project, EMPTY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let attached_path = project.join("must-not-be-created.sqlite");
    let migration = project.join("dinoco/migrations/001_unsafe");
    fs::create_dir_all(&migration).expect("migration dir");
    let attached_sql_path = attached_path.to_string_lossy().replace('\'', "''");
    fs::write(
        migration.join("up.sql"),
        format!("ATTACH DATABASE '{attached_sql_path}' AS external;\nCREATE TABLE external.side_effect (id TEXT);"),
    )
    .expect("up.sql");

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute(
        "CREATE TABLE dinoco_migrations (name TEXT PRIMARY KEY, applied_at TEXT DEFAULT CURRENT_TIMESTAMP)",
        [],
    )
    .expect("history table");
    conn.execute("INSERT INTO dinoco_migrations (name) VALUES ('001_unsafe')", []).expect("history row");
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    assert!(combined_output(&run).contains("failed to safely replay applied migration"), "{}", combined_output(&run));
    assert!(!attached_path.exists(), "history replay must not touch an external database");
}

#[test]
fn generate_rejects_pending_or_inconsistently_ordered_local_history() {
    let pending_project = temp_project("pending-generate");
    write_project_schema(&pending_project, ACCOUNT_ID_ONLY_SCHEMA);
    let pending_db = pending_project.join("dev.sqlite");
    let pending = pending_project.join("dinoco/migrations/001_pending");
    fs::create_dir_all(&pending).expect("pending dir");
    fs::write(pending.join("up.sql"), "CREATE TABLE account (id TEXT PRIMARY KEY NOT NULL);").expect("up.sql");

    let generate = run_cli(&pending_project, &pending_db, &["migrate", "generate"], &[]);
    assert!(!generate.status.success(), "{}", combined_output(&generate));
    assert!(combined_output(&generate).contains("local migrations are pending"));

    let order_project = temp_project("history-order");
    write_project_schema(&order_project, EMPTY_SCHEMA);
    let order_db = order_project.join("dev.sqlite");
    for name in ["001_pending", "002_applied"] {
        let migration = order_project.join("dinoco/migrations").join(name);
        fs::create_dir_all(&migration).expect("migration dir");
        fs::write(migration.join("up.sql"), "").expect("up.sql");
    }
    let conn = Connection::open(&order_db).expect("sqlite");
    conn.execute(
        "CREATE TABLE dinoco_migrations (name TEXT PRIMARY KEY, applied_at TEXT DEFAULT CURRENT_TIMESTAMP)",
        [],
    )
    .expect("history table");
    conn.execute("INSERT INTO dinoco_migrations (name) VALUES ('002_applied')", []).expect("history row");
    drop(conn);

    let run = run_cli(&order_project, &order_db, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    assert!(combined_output(&run).contains("appears after a pending migration"));
}

#[test]
fn generate_repairs_applied_history_drift_before_leaving_pending_migrations_for_run() {
    let project = temp_project("pending-with-drift");
    write_project_schema(&project, THREE_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let pending = project.join("dinoco/migrations/999_pending_location");
    fs::create_dir_all(&pending).expect("pending dir");
    fs::write(pending.join("up.sql"), "CREATE TABLE location (id INTEGER PRIMARY KEY NOT NULL);")
        .expect("pending up.sql");
    write_project_schema(&project, FOUR_TABLE_SCHEMA);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("DROP TABLE business", []).expect("drop business");
    drop(conn);

    let repair = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DRIFT", "true")]);
    assert_success(&repair);
    let output = combined_output(&repair);
    assert!(output.contains("schema drift repaired against applied history"), "{output}");
    assert!(output.contains("999_pending_location"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(table_exists(&conn, "business"));
    assert!(!table_exists(&conn, "location"));
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&run);
    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(table_exists(&conn, "business"));
    assert!(table_exists(&conn, "location"));
}

#[test]
fn applied_history_without_its_local_directory_is_rejected() {
    let project = temp_project("missing-history-dir");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);
    let migration = migration_dirs(&project).into_iter().next().expect("migration");
    fs::remove_dir_all(migration).expect("remove migration fixture");

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    assert!(combined_output(&run).contains("applied migration directories are missing"));
}

#[test]
fn run_refuses_applied_history_when_live_sqlite_tables_are_missing() {
    let project = temp_project("run-drift");
    write_project_schema(&project, THREE_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");

    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("INSERT INTO account (id, name) VALUES (1001, 'Preserved')", []).expect("seed account");
    conn.execute("DROP TABLE business", []).expect("drop business");
    conn.execute("DROP TABLE address", []).expect("drop address");
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    let output = combined_output(&run);
    assert!(output.contains("Database schema drift detected"), "{output}");
    assert!(output.contains("Missing table `business`"), "{output}");
    assert!(output.contains("Missing table `address`"), "{output}");
    assert!(output.contains("Refusing to run pending migrations"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let account_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM account", [], |row| row.get(0)).expect("account count");
    assert_eq!(account_rows, 1);
    assert!(!table_exists(&conn, "business"));
    assert!(!table_exists(&conn, "address"));
}

#[test]
fn sqlite_migration_failure_rolls_back_every_statement_and_history_record() {
    let project = temp_project("atomic-run");
    write_project_schema(&project, EMPTY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_broken");
    fs::create_dir_all(&migration).expect("migration dir");
    fs::write(
        migration.join("up.sql"),
        "CREATE TABLE partial_write (id TEXT PRIMARY KEY);\nINSERT INTO table_that_does_not_exist (id) VALUES ('x');",
    )
    .expect("up.sql");

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    assert!(combined_output(&run).contains("failed to safely replay pending migration"));

    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(!table_exists(&conn, "partial_write"));
    assert!(!table_exists(&conn, "dinoco_migrations"));
}

#[test]
fn run_records_custom_migration_only_after_its_sql_succeeds() {
    let project = temp_project("custom-history");
    write_project_schema(&project, CUSTOM_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_custom");
    fs::create_dir_all(&migration).expect("migration dir");
    fs::write(migration.join("up.sql"), "CREATE TABLE custom_table (id TEXT PRIMARY KEY NOT NULL);").expect("up.sql");

    let first = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&first);

    let conn = Connection::open(&db_path).expect("sqlite");
    let records: i64 = conn
        .query_row("SELECT COUNT(*) FROM dinoco_migrations WHERE name = '001_custom'", [], |row| row.get(0))
        .expect("migration record");
    assert_eq!(records, 1);
    drop(conn);

    let second = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&second);
    assert!(combined_output(&second).contains("Skipping already applied migration: 001_custom"));
}

#[test]
fn concurrent_runners_apply_a_custom_migration_exactly_once() {
    let project = temp_project("concurrent-run");
    write_project_schema(&project, EMPTY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_events");
    fs::create_dir_all(&migration).expect("migration dir");
    fs::write(
        migration.join("up.sql"),
        "CREATE TABLE events (id INTEGER PRIMARY KEY NOT NULL, value TEXT NOT NULL);
         INSERT INTO events (id, value) VALUES (1, 'once');",
    )
    .expect("up.sql");

    let mut first_command = cli_command(&project, &db_path, &["migrate", "run"], &[]);
    first_command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let first = first_command.spawn().expect("first runner");
    let mut second_command = cli_command(&project, &db_path, &["migrate", "run"], &[]);
    second_command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let second = second_command.spawn().expect("second runner");
    let first = first.wait_with_output().expect("first output");
    let second = second.wait_with_output().expect("second output");
    assert_success(&first);
    assert_success(&second);

    let conn = Connection::open(&db_path).expect("sqlite");
    let events: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0)).expect("events");
    let history: i64 = conn
        .query_row("SELECT COUNT(*) FROM dinoco_migrations WHERE name = '001_events'", [], |row| row.get(0))
        .expect("history");
    assert_eq!((events, history), (1, 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_conflicting_migrations_reject_the_loser_without_mixing_schemas() {
    let project = temp_project("concurrent-conflicting-checksums");
    let db_path = project.join("dev.sqlite");
    let config = RuntimeConfig {
        database: Database::Sqlite,
        postgres_connection: PostgresConnection::Direct,
        database_url: db_path.to_string_lossy().to_string(),
        min_connection: 2,
        max_connection: 10,
    };
    let first_db = CliDatabase::connect(&config).await.expect("first sqlite adapter");
    let second_db = CliDatabase::connect(&config).await.expect("second sqlite adapter");
    let first_checksum = "a".repeat(64);
    let second_checksum = "b".repeat(64);

    let first = first_db.apply_sqlite_migration(
        "001_conflict".to_string(),
        "CREATE TABLE first_schema (id TEXT PRIMARY KEY NOT NULL);".to_string(),
        first_checksum.clone(),
        false,
    );
    let second = second_db.apply_sqlite_migration(
        "001_conflict".to_string(),
        "CREATE TABLE second_schema (id TEXT PRIMARY KEY NOT NULL);".to_string(),
        second_checksum.clone(),
        false,
    );
    let (first, second) = tokio::join!(first, second);

    let applied = usize::from(matches!(&first, Ok(true))) + usize::from(matches!(&second, Ok(true)));
    let rejected = usize::from(first.is_err()) + usize::from(second.is_err());
    assert_eq!(applied, 1, "exactly one conflicting migration must apply: first={first:?}, second={second:?}");
    assert_eq!(rejected, 1, "the conflicting runner must fail: first={first:?}, second={second:?}");
    let error = first.as_ref().err().or_else(|| second.as_ref().err()).expect("conflicting runner error");
    assert!(error.to_string().to_ascii_lowercase().contains("checksum"), "{error:#}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let first_exists = table_exists(&conn, "first_schema");
    let second_exists = table_exists(&conn, "second_schema");
    assert_ne!(first_exists, second_exists, "conflicting schemas must never be mixed");
    let recorded: String = conn
        .query_row("SELECT checksum FROM dinoco_migration_checksums WHERE name = '001_conflict'", [], |row| row.get(0))
        .expect("recorded checksum");
    assert_eq!(recorded, if first_exists { first_checksum } else { second_checksum });
    let history: i64 = conn
        .query_row("SELECT COUNT(*) FROM dinoco_migrations WHERE name = '001_conflict'", [], |row| row.get(0))
        .expect("history row");
    assert_eq!(history, 1);
}

#[tokio::test]
async fn later_legacy_checksum_bootstrap_cannot_replace_the_original_hash() {
    let project = temp_project("legacy-checksum-bootstrap-conflict");
    let db_path = project.join("dev.sqlite");
    let conn = Connection::open(&db_path).expect("legacy sqlite");
    conn.execute_batch(
        "CREATE TABLE legacy_table (id TEXT PRIMARY KEY NOT NULL);
         CREATE TABLE dinoco_migrations (
             name TEXT PRIMARY KEY,
             applied_at TEXT DEFAULT CURRENT_TIMESTAMP
         );
         INSERT INTO dinoco_migrations (name) VALUES ('001_legacy');",
    )
    .expect("legacy database");
    drop(conn);

    let db = CliDatabase::connect(&RuntimeConfig {
        database: Database::Sqlite,
        postgres_connection: PostgresConnection::Direct,
        database_url: db_path.to_string_lossy().to_string(),
        min_connection: 2,
        max_connection: 10,
    })
    .await
    .expect("sqlite adapter");
    let original_checksum = "a".repeat(64);
    let conflicting_checksum = "b".repeat(64);
    let bootstrap = |checksum: &str| {
        vec![
            db.compile_create_migration_checksums_table(),
            db.compile_create_migration_checksum_guard(),
            db.compile_insert_migration_checksum("001_legacy", checksum),
        ]
    };

    db.execute_transaction(&bootstrap(&original_checksum)).await.expect("initial legacy checksum bootstrap");
    let error = db
        .execute_transaction(&bootstrap(&conflicting_checksum))
        .await
        .expect_err("a later bootstrap must not replace an established checksum");
    assert!(error.to_string().to_ascii_lowercase().contains("checksum"), "{error:#}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let recorded: String = conn
        .query_row("SELECT checksum FROM dinoco_migration_checksums WHERE name = '001_legacy'", [], |row| row.get(0))
        .expect("recorded legacy checksum");
    assert_eq!(recorded, original_checksum);
}

#[test]
fn safe_sqlite_pragma_in_custom_migration_is_validated_before_apply() {
    let project = temp_project("custom-safe-pragma");
    write_project_schema(&project, CUSTOM_TABLE_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_custom");
    fs::create_dir_all(&migration).expect("migration dir");
    fs::write(
        migration.join("up.sql"),
        "PRAGMA foreign_keys = ON;\nCREATE TABLE custom_table (id TEXT PRIMARY KEY NOT NULL);",
    )
    .expect("up.sql");

    let first = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&first);
    let second = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&second);
    assert!(combined_output(&second).contains("Skipping already applied migration"));
}

#[test]
fn custom_migration_preserves_crlf_bytes_inside_string_literals() {
    let project = temp_project("custom-crlf-literal");
    write_project_schema(&project, EMPTY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_crlf");
    fs::create_dir_all(&migration).expect("migration dir");
    let up_path = migration.join("up.sql");
    fs::write(
        &up_path,
        "CREATE TABLE messages (id INTEGER PRIMARY KEY, body TEXT NOT NULL);\r\n\
         INSERT INTO messages (id, body) VALUES (1, 'line one\r\nline two');\r\n",
    )
    .expect("up.sql");

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&run);
    let conn = Connection::open(&db_path).expect("sqlite");
    let body: String = conn.query_row("SELECT body FROM messages WHERE id = 1", [], |row| row.get(0)).expect("body");
    assert_eq!(body, "line one\r\nline two");
    drop(conn);

    fs::write(
        &up_path,
        "CREATE TABLE messages (id INTEGER PRIMARY KEY, body TEXT NOT NULL);\r\n\
         INSERT INTO messages (id, body) VALUES (1, 'line one\nline two');\r\n",
    )
    .expect("tampered up.sql");
    let tampered = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!tampered.status.success(), "{}", combined_output(&tampered));
    assert!(combined_output(&tampered).contains("checksum"));
}

#[test]
fn sqlite_migration_cannot_disable_foreign_key_enforcement() {
    let project = temp_project("unsafe-foreign-key-pragma");
    write_project_schema(&project, EMPTY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let migration = project.join("dinoco/migrations/001_unsafe_pragma");
    fs::create_dir_all(&migration).expect("migration dir");
    fs::write(
        migration.join("up.sql"),
        "PRAGMA foreign_keys = OFF;
         CREATE TABLE must_not_exist (id TEXT PRIMARY KEY NOT NULL);",
    )
    .expect("up.sql");

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(!table_exists(&conn, "must_not_exist"));
    assert!(!table_exists(&conn, "dinoco_migrations"));
}

#[test]
fn custom_migration_cannot_mutate_dinoco_history_metadata() {
    let project = temp_project("custom-metadata-tamper");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let tamper = project.join("dinoco/migrations/999_tamper");
    fs::create_dir_all(&tamper).expect("tamper migration");
    fs::write(
        tamper.join("up.sql"),
        "DELETE FROM dinoco_migration_checksums;
         DROP INDEX dinoco_migrations_checksum_required;
         CREATE TABLE leaked (id TEXT PRIMARY KEY);",
    )
    .expect("tamper up.sql");

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert!(!run.status.success(), "{}", combined_output(&run));
    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(!table_exists(&conn, "leaked"));
    let history: i64 = conn.query_row("SELECT COUNT(*) FROM dinoco_migrations", [], |row| row.get(0)).expect("history");
    let checksums: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migration_checksums", [], |row| row.get(0)).expect("checksums");
    let guard: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'dinoco_migrations_checksum_required'",
            [],
            |row| row.get(0),
        )
        .expect("guard");
    assert_eq!((history, checksums, guard), (1, 1, 1));
}

#[test]
fn checksum_placeholder_text_in_a_default_is_preserved_verbatim() {
    let project = temp_project("checksum-placeholder-default");
    write_project_schema(&project, CHECKSUM_PLACEHOLDER_SCHEMA);
    let db_path = project.join("dev.sqlite");

    let generate = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&generate);
    let migration = migration_dirs(&project).into_iter().next().expect("migration");
    let up_sql = fs::read_to_string(migration.join("up.sql")).expect("up.sql");
    assert_eq!(up_sql.matches("__DINOCO_INTERNAL_SHA256_PLACEHOLDER_7F43A9C2__").count(), 1, "{up_sql}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let default: String = conn
        .query_row("SELECT dflt_value FROM pragma_table_info('account') WHERE name = 'label'", [], |row| row.get(0))
        .expect("default");
    assert_eq!(default, "'__DINOCO_INTERNAL_SHA256_PLACEHOLDER_7F43A9C2__'");
    drop(conn);

    let run = run_cli(&project, &db_path, &["migrate", "run"], &[]);
    assert_success(&run);
}

#[test]
fn drift_repair_rolls_back_when_recreating_a_parent_would_leave_orphaned_rows() {
    let project = temp_project("foreign-key-drift");
    write_project_schema(&project, RELATED_TABLES_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.pragma_update(None, "foreign_keys", true).expect("enable foreign keys");
    conn.execute("INSERT INTO z_parent (id) VALUES ('parent-1')", []).expect("parent");
    conn.execute("INSERT INTO a_child (id, parent_id) VALUES ('child-1', 'parent-1')", []).expect("child");
    conn.pragma_update(None, "foreign_keys", false).expect("disable foreign keys for drift fixture");
    conn.execute("DROP TABLE z_parent", []).expect("drop parent outside migrations");
    drop(conn);

    let repair = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DRIFT", "true")]);
    assert!(!repair.status.success(), "{}", combined_output(&repair));
    let output = combined_output(&repair);
    assert!(output.contains("foreign key integrity check failed"), "{output}");
    assert!(output.contains("all changes were rolled back"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    assert!(!table_exists(&conn, "z_parent"));
    assert!(table_exists(&conn, "a_child"));
    let child_rows: i64 = conn.query_row("SELECT COUNT(*) FROM a_child", [], |row| row.get(0)).expect("child rows");
    assert_eq!(child_rows, 1);
}

#[test]
fn down_sql_drops_foreign_key_children_before_parents() {
    let project = temp_project("foreign-key-down-order");
    write_project_schema(&project, RELATED_TABLES_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let generate = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&generate);

    let migration = migration_dirs(&project).into_iter().next().expect("migration");
    let down_sql = fs::read_to_string(migration.join("down.sql")).expect("down.sql");
    let child_drop = down_sql.find("DROP TABLE IF EXISTS a_child").expect("child drop");
    let parent_drop = down_sql.find("DROP TABLE IF EXISTS z_parent").expect("parent drop");
    assert!(child_drop < parent_drop, "{down_sql}");

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.pragma_update(None, "foreign_keys", true).expect("enable foreign keys");
    conn.execute("INSERT INTO z_parent (id) VALUES ('parent-1')", []).expect("parent");
    conn.execute("INSERT INTO a_child (id, parent_id) VALUES ('child-1', 'parent-1')", []).expect("child");
    conn.execute_batch(&down_sql).expect("down migration with foreign keys enabled");
    assert!(!table_exists(&conn, "a_child"));
    assert!(!table_exists(&conn, "z_parent"));
}

#[test]
fn destructive_up_sql_defers_foreign_keys_until_all_related_tables_are_dropped() {
    let project = temp_project("foreign-key-up-drop");
    write_project_schema(&project, PARENT_FIRST_RELATED_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);
    let initial_migration = migration_dirs(&project).into_iter().next().expect("initial migration");
    let initial_up = fs::read_to_string(initial_migration.join("up.sql")).expect("initial up.sql");

    let direct_db = project.join("direct.sqlite");
    let direct = Connection::open(&direct_db).expect("direct sqlite");
    direct.execute_batch(&initial_up).expect("initial direct migration");
    direct.execute("INSERT INTO a_parent (id) VALUES ('parent-1')", []).expect("direct parent");
    direct.execute("INSERT INTO z_child (id, parent_id) VALUES ('child-1', 'parent-1')", []).expect("direct child");
    drop(direct);

    let live = Connection::open(&db_path).expect("live sqlite");
    live.execute("INSERT INTO a_parent (id) VALUES ('parent-1')", []).expect("live parent");
    live.execute("INSERT INTO z_child (id, parent_id) VALUES ('child-1', 'parent-1')", []).expect("live child");
    drop(live);
    write_project_schema(&project, EMPTY_SCHEMA);

    let destructive =
        run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")]);
    assert_success(&destructive);
    let migrations = migration_dirs(&project);
    let destructive_up = fs::read_to_string(migrations.last().unwrap().join("up.sql")).expect("destructive up.sql");
    assert!(destructive_up.contains("PRAGMA defer_foreign_keys = ON;"), "{destructive_up}");
    let parent_drop = destructive_up.find("DROP TABLE a_parent").expect("parent drop");
    let child_drop = destructive_up.find("DROP TABLE z_child").expect("child drop");
    assert!(child_drop < parent_drop, "foreign-key children must be dropped before their parents: {destructive_up}");

    let direct = Connection::open(&direct_db).expect("direct sqlite");
    direct.execute_batch(&destructive_up).expect("destructive direct migration");
    assert!(!table_exists(&direct, "a_parent"));
    assert!(!table_exists(&direct, "z_child"));
}

#[test]
fn generated_up_sql_bootstraps_legacy_checksums_when_applied_directly() {
    let project = temp_project("legacy-direct-bootstrap");
    write_project_schema(&project, LEGACY_AND_NEW_SCHEMA);
    let first_db = project.join("first.sqlite");
    let legacy_migration = project.join("dinoco/migrations/001_legacy");
    fs::create_dir_all(&legacy_migration).expect("legacy migration");
    fs::write(legacy_migration.join("up.sql"), "CREATE TABLE legacy_table (id TEXT PRIMARY KEY NOT NULL);")
        .expect("legacy up.sql");
    create_legacy_database(&first_db);

    let generate = run_cli(&project, &first_db, &["migrate", "generate"], &[]);
    assert_success(&generate);
    let migrations = migration_dirs(&project);
    assert_eq!(migrations.len(), 2);
    let new_up_sql = fs::read_to_string(migrations.last().unwrap().join("up.sql")).expect("new up.sql");
    assert!(new_up_sql.contains("WHERE EXISTS (SELECT 1 FROM dinoco_migrations"), "{new_up_sql}");

    let second_db = project.join("second.sqlite");
    create_legacy_database(&second_db);
    let conn = Connection::open(&second_db).expect("second sqlite");
    conn.execute_batch(&new_up_sql).expect("direct new migration");
    let checksum_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migration_checksums", [], |row| row.get(0)).expect("checksums");
    assert_eq!(checksum_rows, 2);
    let guard_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'dinoco_migrations_checksum_required'",
            [],
            |row| row.get(0),
        )
        .expect("checksum guard");
    assert_eq!(guard_rows, 1, "direct up.sql must upgrade legacy history integrity");
    drop(conn);

    let run = run_cli(&project, &second_db, &["migrate", "run"], &[]);
    assert_success(&run);
    let output = combined_output(&run);
    assert!(output.contains("Skipping already applied migration: 001_legacy"), "{output}");
    let conn = Connection::open(&second_db).expect("second sqlite");
    assert!(table_exists(&conn, "legacy_table"));
    assert!(table_exists(&conn, "new_table"));
    conn.execute("DROP TABLE dinoco_migration_checksums", []).expect("drop checksum metadata");
    drop(conn);

    let missing = run_cli(&project, &second_db, &["migrate", "run"], &[]);
    assert!(!missing.status.success(), "{}", combined_output(&missing));
    assert!(combined_output(&missing).contains("this database requires"));
}

#[test]
fn failed_generated_sqlite_migration_is_rolled_back_and_its_directory_is_removed() {
    let project = temp_project("failed-generate");
    write_project_schema(&project, ACCOUNT_ID_ONLY_SCHEMA);
    let db_path = project.join("dev.sqlite");
    let initial = run_cli(&project, &db_path, &["migrate", "generate"], &[]);
    assert_success(&initial);

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("INSERT INTO account (id) VALUES ('account-1')", []).expect("seed account");
    drop(conn);
    write_project_schema(&project, ACCOUNT_WITH_REQUIRED_NAME_SCHEMA);

    let failed = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")]);
    assert!(!failed.status.success(), "{}", combined_output(&failed));
    assert!(combined_output(&failed).contains("stopped before writing or recording"), "{}", combined_output(&failed));
    assert_eq!(migration_dirs(&project).len(), 1);

    let conn = Connection::open(&db_path).expect("sqlite");
    let columns = conn
        .prepare("PRAGMA table_info(account)")
        .expect("pragma")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names");
    assert_eq!(columns, vec!["id"]);
    let records: i64 =
        conn.query_row("SELECT COUNT(*) FROM dinoco_migrations", [], |row| row.get(0)).expect("history count");
    assert_eq!(records, 1);
}

#[test]
fn sqlite_rebuild_preserves_rows_across_type_default_enum_drop_and_unique_changes() {
    let project = temp_project("sqlite-generic-rebuild");
    write_project_schema(&project, SQLITE_REBUILD_INITIAL_SCHEMA);
    let db_path = project.join("dev.sqlite");
    assert_success(&run_cli(&project, &db_path, &["migrate", "generate"], &[]));

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute(
        "INSERT INTO item (code, status, obsolete, note, legacy_name) VALUES ('42', 'Active', true, 'kept', 'renamed')",
        [],
    )
    .expect("seed item");
    drop(conn);

    write_project_schema(&project, SQLITE_REBUILD_DESIRED_SCHEMA);
    let rebuilt = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")]);
    assert_success(&rebuilt);

    let migration = migration_dirs(&project).pop().expect("rebuild migration");
    let up_sql = fs::read_to_string(migration.join("up.sql")).expect("up.sql");
    assert!(up_sql.starts_with("PRAGMA foreign_keys = OFF;"), "{up_sql}");
    assert!(up_sql.contains("dinoco-sqlite-table-rebuild"), "{up_sql}");
    assert!(up_sql.contains("PRAGMA foreign_keys = ON;"), "{up_sql}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let row: (i64, String, String, String, String, Option<String>) = conn
        .query_row("SELECT code, typeof(code), status, added, note, display_name FROM item", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })
        .expect("rebuilt row");
    assert_eq!(
        row,
        (
            42,
            "integer".to_string(),
            "Active".to_string(),
            "created".to_string(),
            "kept".to_string(),
            Some("renamed".to_string())
        )
    );
    assert!(!table_sql(&conn, "item").contains("obsolete"));
    assert!(
        conn.execute("INSERT INTO item (code, status) VALUES (42, 'Active')", []).is_err(),
        "the rebuilt unique constraint must reject duplicates"
    );
    drop(conn);

    write_project_schema(&project, SQLITE_REBUILD_FINAL_SCHEMA);
    assert_success(&run_cli(
        &project,
        &db_path,
        &["migrate", "generate"],
        &[("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")],
    ));
    let conn = Connection::open(&db_path).expect("sqlite");
    let preserved: (String, Option<String>) = conn
        .query_row("SELECT status, note FROM item WHERE code = 42", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("row after removing enum and constraints");
    assert_eq!(preserved, ("Active".to_string(), Some("kept".to_string())));
    conn.execute("INSERT INTO item (code, status, added) VALUES (42, 'free-form', 'manual')", [])
        .expect("removed unique and enum constraints");
}

#[test]
fn sqlite_rebuild_rejects_a_removed_enum_value_and_rolls_everything_back() {
    let project = temp_project("sqlite-enum-rebuild-rollback");
    write_project_schema(&project, SQLITE_ENUM_INITIAL_SCHEMA);
    let db_path = project.join("dev.sqlite");
    assert_success(&run_cli(&project, &db_path, &["migrate", "generate"], &[]));

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("INSERT INTO item (status) VALUES ('Legacy')", []).expect("seed legacy enum value");
    let old_sql = table_sql(&conn, "item");
    drop(conn);

    write_project_schema(&project, SQLITE_ENUM_DESIRED_SCHEMA);
    let failed = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")]);
    assert!(!failed.status.success(), "{}", combined_output(&failed));
    let output = combined_output(&failed);
    assert!(output.contains("item.status"), "{output}");
    assert!(output.contains("removed values: Legacy"), "{output}");
    assert!(output.contains("allowed: Active"), "{output}");
    assert_eq!(migration_dirs(&project).len(), 1, "a failed rebuild must not publish migration history");

    let conn = Connection::open(&db_path).expect("sqlite");
    assert_eq!(table_sql(&conn, "item"), old_sql);
    let status: String = conn.query_row("SELECT status FROM item", [], |row| row.get(0)).expect("preserved row");
    assert_eq!(status, "Legacy");
    let history: i64 = conn.query_row("SELECT COUNT(*) FROM dinoco_migrations", [], |row| row.get(0)).unwrap();
    assert_eq!(history, 1);
}

#[test]
fn sqlite_rebuild_rejects_an_unsafe_type_conversion_without_changing_the_primary_key() {
    let project = temp_project("sqlite-type-rebuild-rollback");
    write_project_schema(&project, SQLITE_STRING_ID_SCHEMA);
    let db_path = project.join("dev.sqlite");
    assert_success(&run_cli(&project, &db_path, &["migrate", "generate"], &[]));
    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("INSERT INTO account (id) VALUES ('not-an-integer')", []).expect("seed string id");
    drop(conn);

    write_project_schema(&project, SQLITE_INTEGER_ID_SCHEMA);
    let failed = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")]);
    assert!(!failed.status.success(), "{}", combined_output(&failed));
    let output = combined_output(&failed);
    assert!(output.contains("account.id"), "{output}");
    assert!(output.contains("Text") && output.contains("Integer"), "{output}");

    let conn = Connection::open(&db_path).expect("sqlite");
    let id: String = conn.query_row("SELECT id FROM account", [], |row| row.get(0)).expect("preserved id");
    assert_eq!(id, "not-an-integer");
    assert!(table_sql(&conn, "account").contains("TEXT"));
    assert_eq!(migration_dirs(&project).len(), 1);
}

#[test]
fn sqlite_rebuild_preserves_cascading_and_self_relations() {
    let project = temp_project("sqlite-rebuild-relations");
    write_project_schema(&project, SQLITE_RELATIONS_INITIAL_SCHEMA);
    let db_path = project.join("dev.sqlite");
    assert_success(&run_cli(&project, &db_path, &["migrate", "generate"], &[]));

    let conn = Connection::open(&db_path).expect("sqlite");
    conn.execute("INSERT INTO parent (id, name) VALUES (1, 'parent')", []).expect("seed parent");
    conn.execute("INSERT INTO child (id, parent_id) VALUES (1, 1)", []).expect("seed child");
    conn.execute("INSERT INTO node (id, label, parent_id) VALUES (1, 'root', NULL)", []).expect("seed root");
    conn.execute("INSERT INTO node (id, label, parent_id) VALUES (2, 'child', 1)", []).expect("seed node child");
    drop(conn);

    write_project_schema(&project, SQLITE_RELATIONS_DESIRED_SCHEMA);
    let rebuilt = run_cli(&project, &db_path, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DESTRUCTIVE", "true")]);
    assert_success(&rebuilt);

    let conn = Connection::open(&db_path).expect("sqlite");
    let child_parent: i64 = conn.query_row("SELECT parent_id FROM child", [], |row| row.get(0)).unwrap();
    let node_parent: i64 = conn.query_row("SELECT parent_id FROM node WHERE id = 2", [], |row| row.get(0)).unwrap();
    assert_eq!((child_parent, node_parent), (1, 1));
    assert!(conn.query_row("PRAGMA foreign_key_check", [], |_| Ok(())).is_err(), "foreign_key_check must be empty");
    assert!(table_sql(&conn, "child").contains("ON DELETE SET NULL"));
    assert!(table_sql(&conn, "node").contains("ON DELETE CASCADE"));
    assert!(conn.execute("INSERT INTO parent (id, name) VALUES (2, 'parent')", []).is_err());
    assert!(conn.execute("INSERT INTO node (id, label) VALUES (3, 'root')", []).is_err());
}

#[tokio::test]
async fn sqlite_introspection_preserves_typed_defaults() {
    let project = temp_project("defaults");
    let db_path = project.join("defaults.sqlite");
    let db = CliDatabase::connect(&RuntimeConfig {
        database: Database::Sqlite,
        postgres_connection: PostgresConnection::Direct,
        database_url: db_path.to_string_lossy().to_string(),
        min_connection: 2,
        max_connection: 10,
    })
    .await
    .expect("sqlite");
    db.execute(
        "CREATE TABLE defaults (
            id TEXT PRIMARY KEY NOT NULL,
            label TEXT NOT NULL DEFAULT 'it''s ready',
            attempts INTEGER NOT NULL DEFAULT -42,
            ratio REAL NOT NULL DEFAULT 3.5,
            enabled BOOLEAN NOT NULL DEFAULT 1,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            payload BLOB
        )",
    )
    .await
    .expect("create defaults");

    let schema = db.inspect_schema().await.expect("inspect");
    let table = schema.tables.iter().find(|table| table.name == "defaults").expect("defaults table");
    assert_default(table, "label", Some(MigrationDefault::String("it's ready".to_string())));
    assert_default(table, "attempts", Some(MigrationDefault::Integer(-42)));
    assert_default(table, "ratio", Some(MigrationDefault::Float(3.5)));
    assert_default(table, "enabled", Some(MigrationDefault::Boolean(true)));
    assert_default(table, "created_at", Some(MigrationDefault::CurrentTimestamp));
    assert!(matches!(
        table.columns.iter().find(|column| column.name == "payload").map(|column| &column.ty),
        Some(MigrationColumnType::Json)
    ));
}

#[test]
fn empty_table_and_column_drops_are_still_destructive() {
    let id_without_default = MigrationColumn {
        name: "id".to_string(),
        ty: MigrationColumnType::Integer,
        primary_key: true,
        unique: false,
        nullable: false,
        default: None,
    };
    let id_with_autoincrement =
        MigrationColumn { default: Some(MigrationDefault::AutoIncrement), ..id_without_default.clone() };
    let obsolete = MigrationColumn {
        name: "obsolete".to_string(),
        ty: MigrationColumnType::String,
        primary_key: false,
        unique: false,
        nullable: true,
        default: None,
    };

    let drop_column = plan_database_migration(
        &DatabaseSchema { tables: vec![table("account", vec![id_without_default.clone()])], enums: Vec::new() },
        &DatabaseSchema { tables: vec![table("account", vec![id_without_default, obsolete])], enums: Vec::new() },
    );
    assert!(drop_column.steps.iter().any(
        |step| matches!(step, MigrationStep::DropColumn(item) if item.table == "account" && item.column == "obsolete")
    ));
    assert!(drop_column.warnings.iter().any(|warning| warning.destructive && warning.message.contains("0 row(s)")));

    let drop_table = plan_database_migration(
        &DatabaseSchema::default(),
        &DatabaseSchema { tables: vec![table("empty_table", vec![id_with_autoincrement])], enums: Vec::new() },
    );
    assert!(
        drop_table
            .steps
            .iter()
            .any(|step| matches!(step, MigrationStep::DropTable(item) if item.table == "empty_table"))
    );
    assert!(drop_table.warnings.iter().any(|warning| warning.destructive && warning.message.contains("0 row(s)")));
}

fn table(name: &str, columns: Vec<MigrationColumn>) -> DatabaseTable {
    DatabaseTable { name: name.to_string(), row_count: 0, columns, foreign_keys: Vec::new(), indexes: Vec::new() }
}

fn assert_default(table: &DatabaseTable, column: &str, expected: Option<MigrationDefault>) {
    let actual = table.columns.iter().find(|item| item.name == column).expect("column");
    assert_eq!(actual.default, expected, "default for {column}");
}

fn write_project_schema(project: &Path, schema: &str) {
    fs::create_dir_all(project.join("dinoco")).expect("dinoco dir");
    fs::write(project.join("dinoco/schema.dinoco"), schema).expect("schema");
}

fn run_cli(project: &Path, db_path: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    cli_command(project, db_path, args, envs).output().expect("dinoco cli")
}

fn cli_command(project: &Path, db_path: &Path, args: &[&str], envs: &[(&str, &str)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"));
    command
        .args(args)
        .env("DATABASE_URL", db_path)
        .env("NODE_ID", "1")
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .current_dir(project);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
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
        .map(|entry| entry.expect("migration").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    migrations.sort();
    migrations
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |row| {
        row.get::<_, i64>(0)
    })
    .expect("table lookup")
        > 0
}

fn table_sql(conn: &Connection, table: &str) -> String {
    conn.query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |row| row.get(0))
        .expect("table sql")
}

fn assert_migration_checksum_recorded(conn: &Connection, migration: &str) {
    let checksum: String = conn
        .query_row("SELECT checksum FROM dinoco_migration_checksums WHERE name = ?1", [migration], |row| row.get(0))
        .expect("migration checksum");
    assert_eq!(checksum.len(), 64);
    assert!(checksum.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
}

fn create_legacy_database(path: &Path) {
    let conn = Connection::open(path).expect("legacy sqlite");
    conn.execute_batch(
        "CREATE TABLE legacy_table (id TEXT PRIMARY KEY NOT NULL);
         CREATE TABLE dinoco_migrations (
             name TEXT PRIMARY KEY,
             applied_at TEXT DEFAULT CURRENT_TIMESTAMP
         );
         INSERT INTO dinoco_migrations (name) VALUES ('001_legacy');",
    )
    .expect("legacy database");
}

fn temp_project(name: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("dinoco-migration-state-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).expect("temp project");
    path
}

const THREE_TABLE_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    snowflake_node_id = env("NODE_ID")
}

model Account {
    id         Integer  @id @default(snowflake())
    name       String
    status     String   @default("active")
    attempts   Integer  @default(3)
    enabled    Boolean  @default(true)
    created_at DateTime @default(now())
}

model Business {
    id   Integer @id @default(snowflake())
    name String
}

model Address {
    id Integer @id @default(snowflake())
}
"#;

const FOUR_TABLE_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    snowflake_node_id = env("NODE_ID")
}

model Account {
    id         Integer  @id @default(snowflake())
    name       String
    status     String   @default("active")
    attempts   Integer  @default(3)
    enabled    Boolean  @default(true)
    created_at DateTime @default(now())
}

model Business {
    id   Integer @id @default(snowflake())
    name String
}

model Address {
    id Integer @id @default(snowflake())
}

model Location {
    id Integer @id @default(snowflake())
}
"#;

const UNILATERAL_RELATION_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    snowflake_node_id = env("NODE_ID")
}

model Account {
    id       Integer    @id @default(snowflake())
    business Business[]
}

model Business {
    id Integer @id @default(snowflake())
}
"#;

const EMPTY_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}
"#;

const CUSTOM_TABLE_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model CustomTable {
    id String @id
}
"#;

const ACCOUNT_ID_ONLY_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Account {
    id String @id
}
"#;

const ACCOUNT_WITH_REQUIRED_NAME_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Account {
    id   String @id
    name String
}
"#;

const SQLITE_REBUILD_INITIAL_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Item {
    id       Integer @id @default(autoincrement())
    code     String
    status   String
    obsolete Boolean?
    note     String?
    legacy_name String?
}
"#;

const SQLITE_REBUILD_DESIRED_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum ItemStatus {
    Active
    Archived
}

model Item {
    id     Integer    @id @default(autoincrement())
    code   Integer    @unique
    status ItemStatus
    added  String     @default("created")
    note   String
    display_name String?
}
"#;

const SQLITE_REBUILD_FINAL_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Item {
    id     Integer @id @default(autoincrement())
    code   Integer
    status String
    added  String
    note   String?
    display_name String?
}
"#;

const SQLITE_ENUM_INITIAL_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum ItemStatus {
    Active
    Legacy
}

model Item {
    id     Integer    @id @default(autoincrement())
    status ItemStatus
}
"#;

const SQLITE_ENUM_DESIRED_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum ItemStatus {
    Active
}

model Item {
    id     Integer    @id @default(autoincrement())
    status ItemStatus
}
"#;

const SQLITE_STRING_ID_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Account {
    id String @id
}
"#;

const SQLITE_INTEGER_ID_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Account {
    id Integer @id
}
"#;

const SQLITE_RELATIONS_INITIAL_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Parent {
    id       Integer @id @default(autoincrement())
    name     String
    children Child[]
}

model Child {
    id        Integer @id @default(autoincrement())
    parent_id Integer
    parent    Parent? @relation(fields: [parent_id], references: [id], onDelete: Cascade, onUpdate: Cascade)
}

model Node {
    id        Integer @id @default(autoincrement())
    label     String
    parent_id Integer?
    parent    Node?   @relation(name: "NodeTree", fields: [parent_id], references: [id], onDelete: SetNull)
    children  Node[]  @relation(name: "NodeTree")
}
"#;

const SQLITE_RELATIONS_DESIRED_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Parent {
    id       Integer @id @default(autoincrement())
    name     String  @unique
    children Child[]
}

model Child {
    id        Integer @id @default(autoincrement())
    parent_id Integer?
    parent    Parent? @relation(fields: [parent_id], references: [id], onDelete: SetNull, onUpdate: Cascade)
}

model Node {
    id        Integer @id @default(autoincrement())
    label     String  @unique
    parent_id Integer?
    parent    Node?   @relation(name: "NodeTree", fields: [parent_id], references: [id], onDelete: Cascade)
    children  Node[]  @relation(name: "NodeTree")
}
"#;

const CHECKSUM_PLACEHOLDER_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Account {
    id    String @id
    label String @default("__DINOCO_INTERNAL_SHA256_PLACEHOLDER_7F43A9C2__")
}
"#;

const RELATED_TABLES_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model ZParent {
    id       String   @id
    children AChild[]
}

model AChild {
    id        String  @id
    parent_id String
    parent    ZParent? @relation(fields: [parent_id], references: [id])
}
"#;

const PARENT_FIRST_RELATED_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model AParent {
    id       String   @id
    children ZChild[]
}

model ZChild {
    id        String  @id
    parent_id String
    parent    AParent? @relation(fields: [parent_id], references: [id])
}
"#;

const LEGACY_AND_NEW_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model LegacyTable {
    id String @id
}

model NewTable {
    id String @id
}
"#;
