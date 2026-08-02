//! Runtime migration support for databases opened by [`crate::DinocoClient`].
//!
//! Migrations are compiled into the application with `include_str!` and are
//! applied to the primary SQLite connection without compiling a Dinoco schema
//! or generating Rust models at runtime.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

use anyhow::Context;
use rusqlite::{OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use crate::{Backend, DinocoClient, SqliteAdapter};

const MIGRATION_CHECKSUM_MARKER: &str = "-- dinoco-checksum: ";
const MIGRATION_CHECKSUM_PLACEHOLDER: &str = "__DINOCO_INTERNAL_SHA256_PLACEHOLDER_7F43A9C2__";

/// A migration embedded in the application binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration<'a> {
    pub name: &'a str,
    pub sql: &'a str,
}

impl<'a> Migration<'a> {
    pub const fn new(name: &'a str, sql: &'a str) -> Self {
        Self { name, sql }
    }
}

/// Summary returned after checking and applying the supplied migrations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationReport {
    pub applied: Vec<String>,
    pub already_applied: Vec<String>,
}

impl MigrationReport {
    pub fn changed(&self) -> bool {
        !self.applied.is_empty()
    }
}

/// Applies pending migrations to the SQLite database already opened by
/// `DinocoClient`.
///
/// Migrations are ordered by name. Both reviewed SQL bodies and the generated
/// `up.sql` artifacts emitted by the Dinoco CLI are accepted. Every migration
/// runs atomically and is recorded with a SHA-256 checksum. Re-running the same
/// list is idempotent; changing or omitting an applied migration is rejected.
pub async fn run_migrations(client: &DinocoClient, migrations: &[Migration<'_>]) -> anyhow::Result<MigrationReport> {
    let Backend::Sqlite(adapter) = &client.backend else {
        anyhow::bail!("Runtime migrations are currently supported only for SQLite databases");
    };

    let mut prepared = migrations.iter().map(PreparedMigration::new).collect::<anyhow::Result<Vec<_>>>()?;
    prepared.sort_by(|left, right| left.name.cmp(&right.name));
    ensure_unique_names(&prepared)?;
    validate_existing_history(adapter, &prepared).await?;

    let mut report = MigrationReport::default();
    for migration in prepared {
        if apply_sqlite_migration(adapter, &migration).await? {
            report.applied.push(migration.name);
        } else {
            report.already_applied.push(migration.name);
        }
    }
    Ok(report)
}

#[derive(Debug)]
struct PreparedMigration {
    name: String,
    execution_sql: String,
    checksum: String,
    generated: bool,
}

impl PreparedMigration {
    fn new(migration: &Migration<'_>) -> anyhow::Result<Self> {
        let name = migration.name.trim();
        if name.is_empty() {
            anyhow::bail!("Runtime migration names cannot be empty");
        }

        let normalized = normalize_checksum_line_endings(migration.sql);
        let markers = checksum_markers(&normalized);
        if markers.is_empty() {
            return Ok(Self {
                name: name.to_string(),
                execution_sql: migration.sql.to_string(),
                checksum: raw_migration_checksum(migration.sql),
                generated: false,
            });
        }
        if markers.len() != 1 {
            anyhow::bail!("Generated runtime migration `{name}` contains more than one checksum marker");
        }

        let declared = markers[0];
        if declared.len() != 64 || !declared.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            anyhow::bail!("Generated runtime migration `{name}` contains an invalid checksum marker");
        }

        let checksum_insert = compile_insert_migration_checksum(name, declared);
        let actual_suffix =
            format!("{};\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{declared}", checksum_insert.trim_end_matches(';'));
        let Some(prefix) = normalized.strip_suffix(&actual_suffix) else {
            anyhow::bail!("Generated runtime migration `{name}` has malformed checksum metadata");
        };
        let checksum_template = compile_insert_migration_checksum(name, MIGRATION_CHECKSUM_PLACEHOLDER);
        let canonical_suffix = format!(
            "{};\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{MIGRATION_CHECKSUM_PLACEHOLDER}",
            checksum_template.trim_end_matches(';')
        );
        let computed = raw_migration_checksum(&format!("{prefix}{canonical_suffix}"));
        if declared != computed {
            anyhow::bail!(
                "Generated runtime migration `{name}` was modified after generation: expected checksum {declared}, computed {computed}"
            );
        }

        Ok(Self {
            name: name.to_string(),
            execution_sql: managed_sqlite_migration_sql(migration.sql, declared)
                .with_context(|| format!("failed to read generated runtime migration `{name}`"))?,
            checksum: computed,
            generated: true,
        })
    }
}

fn ensure_unique_names(migrations: &[PreparedMigration]) -> anyhow::Result<()> {
    let mut names = HashSet::new();
    for migration in migrations {
        if !names.insert(migration.name.as_str()) {
            anyhow::bail!("Runtime migration `{}` is declared more than once", migration.name);
        }
    }
    Ok(())
}

async fn validate_existing_history(adapter: &SqliteAdapter, migrations: &[PreparedMigration]) -> anyhow::Result<()> {
    let connection = adapter.pool.get().await.context("failed to get SQLite runtime migration connection")?;
    let expected = migrations
        .iter()
        .map(|migration| (migration.name.clone(), migration.checksum.clone()))
        .collect::<BTreeMap<_, _>>();

    connection
        .interact(move |connection| -> anyhow::Result<()> {
            let migrations_exist: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'dinoco_migrations')",
                [],
                |row| row.get(0),
            )?;
            if !migrations_exist {
                return Ok(());
            }

            let checksums_exist: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'dinoco_migration_checksums')",
                [],
                |row| row.get(0),
            )?;
            let mut statement = connection.prepare("SELECT name FROM dinoco_migrations ORDER BY name")?;
            let applied = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            if !applied.is_empty() && !checksums_exist {
                anyhow::bail!("SQLite runtime migration history exists without checksum metadata");
            }

            for name in &applied {
                let expected_checksum = expected.get(name).with_context(|| {
                    format!("Applied runtime migration `{name}` is missing from the migrations embedded in this binary")
                })?;
                let recorded = connection
                    .query_row(
                        "SELECT checksum FROM dinoco_migration_checksums WHERE name = ?1",
                        [name],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .with_context(|| format!("Applied runtime migration `{name}` has no checksum record"))?;
                if &recorded != expected_checksum {
                    anyhow::bail!(
                        "Applied runtime migration `{name}` has checksum {recorded}, but this binary contains {expected_checksum}"
                    );
                }
            }

            let applied = applied.into_iter().collect::<HashSet<_>>();
            let mut found_pending = false;
            for migration in expected.keys() {
                if applied.contains(migration) {
                    if found_pending {
                        anyhow::bail!(
                            "SQLite runtime migration history is out of order: `{migration}` is applied after a pending migration"
                        );
                    }
                } else {
                    found_pending = true;
                }
            }
            Ok(())
        })
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
}

async fn apply_sqlite_migration(adapter: &SqliteAdapter, migration: &PreparedMigration) -> anyhow::Result<bool> {
    let connection = adapter.pool.get().await.context("failed to get SQLite runtime migration connection")?;
    let name = migration.name.clone();
    let sql = migration.execution_sql.clone();
    let checksum = migration.checksum.clone();
    let generated = migration.generated;

    connection
        .interact(move |connection| -> anyhow::Result<bool> {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.pragma_update(None, "defer_foreign_keys", true)?;
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS dinoco_migrations (
                    name TEXT PRIMARY KEY,
                    applied_at TEXT DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS dinoco_migration_checksums (
                    name TEXT PRIMARY KEY,
                    checksum TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS dinoco_migrations_checksum_required ON dinoco_migrations(name);",
            )?;

            let already_applied: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM dinoco_migrations WHERE name = ?1)",
                [&name],
                |row| row.get(0),
            )?;
            if already_applied {
                verify_recorded_checksum(&transaction, &name, &checksum)?;
                ensure_sqlite_foreign_key_integrity(&transaction)?;
                transaction.commit()?;
                return Ok(false);
            }

            if generated {
                transaction.authorizer(Some(sqlite_generated_migration_authorizer))?;
            } else {
                transaction.authorizer(Some(sqlite_custom_migration_authorizer))?;
            }
            let application = transaction.execute_batch(&sql);
            transaction.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)?;
            application.with_context(|| format!("failed to execute SQLite runtime migration `{name}`"))?;

            transaction.execute("INSERT OR IGNORE INTO dinoco_migrations (name) VALUES (?1)", [&name])?;
            transaction.execute(
                "INSERT OR IGNORE INTO dinoco_migration_checksums (name, checksum) VALUES (?1, ?2)",
                rusqlite::params![&name, &checksum],
            )?;
            verify_recorded_checksum(&transaction, &name, &checksum)?;
            ensure_sqlite_foreign_key_integrity(&transaction)?;
            transaction.commit()?;
            Ok(true)
        })
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
}

fn verify_recorded_checksum(transaction: &rusqlite::Transaction<'_>, name: &str, expected: &str) -> anyhow::Result<()> {
    let recorded = transaction
        .query_row("SELECT checksum FROM dinoco_migration_checksums WHERE name = ?1", [name], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
        .with_context(|| format!("Applied runtime migration `{name}` has no checksum record"))?;
    if recorded != expected {
        anyhow::bail!(
            "Runtime migration `{name}` was already applied with checksum {recorded}, but this binary contains {expected}"
        );
    }
    Ok(())
}

fn sqlite_generated_migration_authorizer(context: AuthContext<'_>) -> Authorization {
    sqlite_authorization(context, true)
}

fn sqlite_custom_migration_authorizer(context: AuthContext<'_>) -> Authorization {
    sqlite_authorization(context, false)
}

fn sqlite_authorization(context: AuthContext<'_>, allow_metadata_mutation: bool) -> Authorization {
    let external_database = match context.action {
        AuthAction::AlterTable { database_name, .. } => database_name != "main" && database_name != "temp",
        _ => context.database_name.is_some_and(|name| name != "main" && name != "temp"),
    };
    let unsafe_action = match context.action {
        AuthAction::Attach { .. }
        | AuthAction::Detach { .. }
        | AuthAction::CreateVtable { .. }
        | AuthAction::DropVtable { .. }
        | AuthAction::Transaction { .. }
        | AuthAction::Savepoint { .. } => true,
        AuthAction::Pragma { pragma_name, pragma_value } => match pragma_name.to_ascii_lowercase().as_str() {
            "defer_foreign_keys" | "legacy_alter_table" | "foreign_key_check" => false,
            "foreign_keys" => pragma_value
                .is_some_and(|value| !matches!(value.to_ascii_lowercase().as_str(), "1" | "on" | "true" | "yes")),
            _ => true,
        },
        AuthAction::Function { function_name } => {
            matches!(function_name.to_ascii_lowercase().as_str(), "load_extension" | "readfile" | "writefile")
        }
        _ => false,
    };
    let metadata_mutation = !allow_metadata_mutation
        && match context.action {
            AuthAction::CreateTable { table_name }
            | AuthAction::DropTable { table_name }
            | AuthAction::Insert { table_name }
            | AuthAction::Delete { table_name } => is_migration_metadata_table(table_name),
            AuthAction::Update { table_name, .. }
            | AuthAction::CreateTrigger { table_name, .. }
            | AuthAction::DropTrigger { table_name, .. }
            | AuthAction::CreateIndex { table_name, .. }
            | AuthAction::DropIndex { table_name, .. }
            | AuthAction::AlterTable { table_name, .. } => is_migration_metadata_table(table_name),
            _ => false,
        };

    if external_database || unsafe_action || metadata_mutation { Authorization::Deny } else { Authorization::Allow }
}

fn is_migration_metadata_table(table: &str) -> bool {
    matches!(table, "dinoco_migrations" | "dinoco_migration_checksums" | "dinoco_migration_schemas")
}

fn ensure_sqlite_foreign_key_integrity(transaction: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
    let mut statement = transaction.prepare("PRAGMA foreign_key_check")?;
    let mut rows = statement.query([])?;
    if let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let row_id: Option<i64> = row.get(1)?;
        let parent: String = row.get(2)?;
        let foreign_key: i64 = row.get(3)?;
        anyhow::bail!(
            "SQLite foreign key integrity check failed for table `{table}`, row {}, parent table `{parent}`, foreign key #{foreign_key}",
            row_id.map_or_else(|| "without rowid".to_string(), |value| value.to_string())
        );
    }
    Ok(())
}

fn compile_insert_migration_checksum(name: &str, checksum: &str) -> String {
    format!(
        "INSERT INTO dinoco_migration_checksums (name, checksum) VALUES ('{}', '{}') \
         ON CONFLICT(name) DO UPDATE SET checksum = CASE \
         WHEN dinoco_migration_checksums.checksum = excluded.checksum \
         THEN dinoco_migration_checksums.checksum ELSE NULL END",
        escape_sql_literal(name),
        escape_sql_literal(checksum)
    )
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn checksum_markers(sql: &str) -> Vec<&str> {
    sql.lines().filter_map(|line| line.trim().strip_prefix(MIGRATION_CHECKSUM_MARKER)).collect()
}

fn managed_sqlite_migration_sql(sql: &str, checksum: &str) -> anyhow::Result<String> {
    let normalized = normalize_all_sql_line_endings(sql);
    let prefix = "PRAGMA foreign_keys = ON;\n\nBEGIN IMMEDIATE;\n\nPRAGMA defer_foreign_keys = ON;\n\n";
    let suffix = format!("\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{checksum}");
    let normalized_body = normalized
        .strip_prefix(prefix)
        .and_then(|sql| sql.strip_suffix(&suffix))
        .context("generated SQLite migration is missing its managed transaction wrapper")?;
    let normalized_start = prefix.len();
    let normalized_end = normalized_start + normalized_body.len();
    let original_start = original_offset_for_normalized_offset(sql, normalized_start)
        .context("failed to locate generated SQLite migration body")?;
    let original_end = original_offset_for_normalized_offset(sql, normalized_end)
        .context("failed to locate generated SQLite migration body")?;
    Ok(sql[original_start..original_end].to_string())
}

fn raw_migration_checksum(sql: &str) -> String {
    Sha256::digest(sql.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn normalize_all_sql_line_endings(sql: &str) -> Cow<'_, str> {
    if sql.contains('\r') { Cow::Owned(sql.replace("\r\n", "\n").replace('\r', "\n")) } else { Cow::Borrowed(sql) }
}

fn normalize_checksum_line_endings(sql: &str) -> Cow<'_, str> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Normal,
        SingleQuote,
        DoubleQuote,
        Backtick,
        Bracket,
        LineComment,
        BlockComment,
    }

    if !sql.contains('\r') {
        return Cow::Borrowed(sql);
    }

    let bytes = sql.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut state = State::Normal;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        match state {
            State::Normal => match (byte, next) {
                (b'-', Some(b'-')) => {
                    output.extend_from_slice(b"--");
                    index += 2;
                    state = State::LineComment;
                }
                (b'/', Some(b'*')) => {
                    output.extend_from_slice(b"/*");
                    index += 2;
                    state = State::BlockComment;
                }
                (b'\'', _) => {
                    output.push(byte);
                    index += 1;
                    state = State::SingleQuote;
                }
                (b'"', _) => {
                    output.push(byte);
                    index += 1;
                    state = State::DoubleQuote;
                }
                (b'`', _) => {
                    output.push(byte);
                    index += 1;
                    state = State::Backtick;
                }
                (b'[', _) => {
                    output.push(byte);
                    index += 1;
                    state = State::Bracket;
                }
                (b'\r', _) => push_normalized_newline(bytes, &mut output, &mut index),
                _ => {
                    output.push(byte);
                    index += 1;
                }
            },
            State::LineComment => {
                if byte == b'\r' {
                    push_normalized_newline(bytes, &mut output, &mut index);
                    state = State::Normal;
                } else {
                    output.push(byte);
                    index += 1;
                    if byte == b'\n' {
                        state = State::Normal;
                    }
                }
            }
            State::BlockComment => {
                if byte == b'*' && next == Some(b'/') {
                    output.extend_from_slice(b"*/");
                    index += 2;
                    state = State::Normal;
                } else if byte == b'\r' {
                    push_normalized_newline(bytes, &mut output, &mut index);
                } else {
                    output.push(byte);
                    index += 1;
                }
            }
            State::SingleQuote | State::DoubleQuote | State::Backtick | State::Bracket => {
                let closing = match state {
                    State::SingleQuote => b'\'',
                    State::DoubleQuote => b'"',
                    State::Backtick => b'`',
                    State::Bracket => b']',
                    _ => unreachable!(),
                };
                output.push(byte);
                index += 1;
                if byte == closing {
                    if bytes.get(index) == Some(&closing) {
                        output.push(closing);
                        index += 1;
                    } else {
                        state = State::Normal;
                    }
                }
            }
        }
    }

    Cow::Owned(String::from_utf8(output).expect("normalizing line endings preserves UTF-8"))
}

fn push_normalized_newline(bytes: &[u8], output: &mut Vec<u8>, index: &mut usize) {
    output.push(b'\n');
    *index += 1;
    if bytes.get(*index) == Some(&b'\n') {
        *index += 1;
    }
}

fn original_offset_for_normalized_offset(sql: &str, target: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut original = 0;
    let mut normalized = 0;
    while normalized < target {
        match bytes.get(original).copied()? {
            b'\r' => {
                original += 1;
                if bytes.get(original) == Some(&b'\n') {
                    original += 1;
                }
            }
            _ => original += 1,
        }
        normalized += 1;
    }
    Some(original)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DinocoAdapter, SqliteAdapter};

    async fn client(path: &std::path::Path) -> DinocoClient {
        let adapter = SqliteAdapter::new(path.to_string_lossy().into_owned()).await.expect("sqlite adapter");
        DinocoClient::new(Backend::Sqlite(adapter))
    }

    async fn sqlite_i64(client: &DinocoClient, sql: &str) -> i64 {
        let Backend::Sqlite(adapter) = &client.backend else { unreachable!() };
        let connection = adapter.pool.get().await.expect("sqlite connection");
        let sql = sql.to_string();
        connection
            .interact(move |connection| connection.query_row(&sql, [], |row| row.get(0)))
            .await
            .expect("sqlite interaction")
            .expect("sqlite query")
    }

    #[tokio::test]
    async fn connect_creates_the_sqlite_file_and_runtime_migrations_are_idempotent() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("nested/runtime.sqlite");
        let client = client(&path).await;
        assert!(path.exists(), "SqliteAdapter::new must eagerly create the database file");

        let migrations = [
            Migration::new("002_seed", "INSERT INTO note (body) VALUES ('hello');"),
            Migration::new("001_create_note", "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT NOT NULL);"),
        ];
        let first = run_migrations(&client, &migrations).await.expect("apply migrations");
        assert_eq!(first.applied, ["001_create_note", "002_seed"]);
        assert!(first.already_applied.is_empty());
        assert_eq!(sqlite_i64(&client, "SELECT COUNT(*) FROM note").await, 1);

        let second = run_migrations(&client, &migrations).await.expect("skip applied migrations");
        assert!(second.applied.is_empty());
        assert_eq!(second.already_applied, ["001_create_note", "002_seed"]);

        let changed = [migrations[1], Migration::new("002_seed", "INSERT INTO note (body) VALUES ('changed');")];
        let error = run_migrations(&client, &changed).await.expect_err("changed history must be rejected");
        assert!(error.to_string().contains("checksum"), "{error:#}");
        assert_eq!(sqlite_i64(&client, "SELECT COUNT(*) FROM note").await, 1);
    }

    #[tokio::test]
    async fn failed_runtime_migration_rolls_back_schema_and_history() {
        let directory = tempfile::tempdir().expect("temp directory");
        let client = client(&directory.path().join("rollback.sqlite")).await;
        let migration = Migration::new(
            "001_broken",
            "CREATE TABLE should_rollback (id INTEGER PRIMARY KEY); INSERT INTO missing_table VALUES (1);",
        );

        run_migrations(&client, &[migration]).await.expect_err("migration must fail");
        assert_eq!(
            sqlite_i64(
                &client,
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'should_rollback'",
            )
            .await,
            0
        );
        assert_eq!(
            sqlite_i64(
                &client,
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'dinoco_migrations'",
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn accepts_generated_cli_up_sql_artifacts() {
        let directory = tempfile::tempdir().expect("temp directory");
        let client = client(&directory.path().join("generated.sqlite")).await;
        let name = "001_generated";
        let sql_body = format!(
            "CREATE TABLE IF NOT EXISTS dinoco_migrations (name TEXT PRIMARY KEY, applied_at TEXT DEFAULT CURRENT_TIMESTAMP);\n\n\
             CREATE TABLE IF NOT EXISTS dinoco_migration_checksums (name TEXT PRIMARY KEY, checksum TEXT NOT NULL);\n\n\
             CREATE INDEX IF NOT EXISTS dinoco_migrations_checksum_required ON dinoco_migrations(name);\n\n\
             CREATE TABLE generated_note (id INTEGER PRIMARY KEY);\n\n\
             INSERT INTO dinoco_migrations (name) VALUES ('{name}');"
        );
        let checksum_template = compile_insert_migration_checksum(name, MIGRATION_CHECKSUM_PLACEHOLDER);
        let canonical = format!(
            "PRAGMA foreign_keys = ON;\n\nBEGIN IMMEDIATE;\n\nPRAGMA defer_foreign_keys = ON;\n\n{sql_body}\n\n{};\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{MIGRATION_CHECKSUM_PLACEHOLDER}",
            checksum_template.trim_end_matches(';')
        );
        let checksum = raw_migration_checksum(&canonical);
        let artifact = canonical.replace(MIGRATION_CHECKSUM_PLACEHOLDER, &checksum);

        let report = run_migrations(&client, &[Migration::new(name, &artifact)]).await.expect("generated migration");
        assert_eq!(report.applied, [name]);
        assert_eq!(
            sqlite_i64(&client, "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'generated_note'",)
                .await,
            1
        );
    }
}
