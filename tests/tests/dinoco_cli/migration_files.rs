use std::fs;

use dinoco_cli::commands::migrate::{
    ValidatedMigration, is_generated_legacy_normalization_migration, migration_sql_path,
    postgres_preserve_legacy_foreign_key_rows, server_migration_checksum, split_sql,
    upgrade_legacy_migration_artifacts, validate_server_migration_sql,
};
use dinoco_cli::db::redact_database_url;

#[test]
fn migration_sql_path_falls_back_to_the_legacy_filename() {
    let directory = tempfile::tempdir().expect("temporary migration");
    fs::write(directory.path().join("migration.sql"), "SELECT 1;").expect("legacy migration");
    assert_eq!(migration_sql_path(directory.path()).expect("legacy path"), directory.path().join("migration.sql"));

    fs::write(directory.path().join("up.sql"), "SELECT 2;").expect("current migration");
    assert_eq!(migration_sql_path(directory.path()).expect("current path"), directory.path().join("up.sql"));
}

#[test]
fn legacy_artifact_upgrade_preserves_the_original_and_never_overwrites_current_files() {
    let root = tempfile::tempdir().expect("temporary migrations");
    let directory = root.path().join("001_legacy");
    fs::create_dir(&directory).expect("legacy directory");
    fs::write(directory.join("migration.sql"), b"SELECT 1;\r\n").expect("legacy SQL");
    fs::write(directory.join("schema.bin"), b"snapshot").expect("legacy snapshot");

    assert_eq!(upgrade_legacy_migration_artifacts(std::slice::from_ref(&directory)).expect("upgrade"), 1);
    assert_eq!(fs::read(directory.join("up.sql")).expect("up.sql"), b"SELECT 1;\r\n");
    assert!(fs::read_to_string(directory.join("down.sql")).expect("down.sql").contains("unchanged"));
    assert_eq!(fs::read(directory.join("migration.sql")).expect("legacy SQL"), b"SELECT 1;\r\n");
    assert_eq!(fs::read(directory.join("schema.bin")).expect("legacy snapshot"), b"snapshot");

    fs::write(directory.join("up.sql"), "SELECT 2;").expect("custom current SQL");
    assert_eq!(upgrade_legacy_migration_artifacts(std::slice::from_ref(&directory)).expect("second upgrade"), 0);
    assert_eq!(fs::read_to_string(directory.join("up.sql")).expect("up.sql"), "SELECT 2;");
}

#[test]
fn server_sql_splitter_preserves_literals_comments_and_postgres_dollar_quotes() {
    let sql = "INSERT INTO events(value) VALUES ('a;b');\n\
                   -- semicolon ; in a comment\n\
                   CREATE FUNCTION f() RETURNS void AS $$ BEGIN PERFORM ';'; END; $$ LANGUAGE plpgsql;";
    let statements = split_sql(sql).expect("valid SQL");
    assert_eq!(statements.len(), 2);
    assert!(statements[0].contains("'a;b'"));
    assert!(statements[1].contains("PERFORM ';'; END;"));
}

#[test]
fn server_sql_validation_rejects_context_and_metadata_mutation() {
    for sql in [
        "COMMIT; CREATE TABLE leaked(id INT);",
        "USE another_database;",
        "SET FOREIGN_KEY_CHECKS=0;",
        "DELETE FROM dinoco_migrations;",
        "DELETE FROM dinoco_migration_schemas;",
        "SELECT RELEASE_LOCK('dinoco:migrations');",
        "SELECT pg_advisory_xact_lock(123);",
        "DO $$ BEGIN EXECUTE 'DROP TABLE account'; END $$;",
        "CREATE OR REPLACE FUNCTION mutate_history() RETURNS void AS $$ BEGIN DELETE FROM dinoco_migrations; END; $$ LANGUAGE plpgsql;",
        "CREATE DEFINER = root@localhost PROCEDURE mutate_history() DELETE FROM dinoco_migrations;",
        "PREPARE hidden FROM 'DROP TABLE account';",
        "CALL mutate_history();",
    ] {
        assert!(validate_server_migration_sql(sql).is_err(), "{sql}");
    }
}

#[test]
fn server_sql_validation_ignores_reserved_words_inside_values() {
    validate_server_migration_sql(
        "INSERT INTO audit(message) VALUES ('do not DELETE FROM dinoco_migrations; or COMMIT');",
    )
    .expect("reserved text inside a value is harmless");
}

#[test]
fn server_checksums_normalize_file_line_endings_but_preserve_literal_bytes() {
    let lf = "INSERT INTO audit(message) VALUES ('line 1\r\nline 2');\nCREATE TABLE item(id INT);\n";
    let crlf = "INSERT INTO audit(message) VALUES ('line 1\r\nline 2');\r\nCREATE TABLE item(id INT);\r\n";
    assert_eq!(server_migration_checksum(lf), server_migration_checksum(crlf));

    let changed_literal = "INSERT INTO audit(message) VALUES ('line 1\nline 2');\nCREATE TABLE item(id INT);\n";
    assert_ne!(server_migration_checksum(lf), server_migration_checksum(changed_literal));
}

#[test]
fn pending_legacy_postgres_foreign_keys_become_not_valid_without_changing_other_statements() {
    let sql = r#"
            ALTER TABLE "AudioVariation" RENAME TO "audio_variation";
            ALTER TABLE audio_variation ADD CONSTRAINT "fk_audio_variation_creation_id"
                FOREIGN KEY (creation_id) REFERENCES audio_creation (id) ON UPDATE CASCADE ON DELETE CASCADE;
            CREATE INDEX idx_audio_variation_creation_id ON audio_variation (creation_id);
        "#;

    let recovered = postgres_preserve_legacy_foreign_key_rows(sql).expect("recover legacy SQL");

    assert!(recovered.contains("ON DELETE CASCADE NOT VALID;"), "{recovered}");
    assert!(recovered.contains("CREATE INDEX idx_audio_variation_creation_id"), "{recovered}");
    assert_eq!(recovered.matches("NOT VALID").count(), 1);
    let migration = ValidatedMigration { execution_sql: recovered, checksum: "unused".to_string(), generated: true };
    assert!(is_generated_legacy_normalization_migration(&migration));
}

#[test]
fn redact_database_url_hides_credentials_but_keeps_host_and_database() {
    assert_eq!(
        redact_database_url("postgresql://app_user:s3cret@db.internal:5432/app"),
        "postgresql://***@db.internal:5432/app"
    );
    assert_eq!(redact_database_url("file:./dinoco/dev.sqlite"), "file:./dinoco/dev.sqlite");
    assert_eq!(redact_database_url("not a url"), "not a url");
}
