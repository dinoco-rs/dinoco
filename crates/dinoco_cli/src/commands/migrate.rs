use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use inquire::Confirm;
use sha2::{Digest, Sha256};

use crate::db::CliDatabase;
use crate::schema::{Database, RuntimeConfig, read_schema_for_workspace, runtime_config};
use crate::sql::{MigrationPlan, MigrationStep, desired_database_schema, plan_database_migration};
use crate::ui;

const MIGRATION_CHECKSUM_MARKER: &str = "-- dinoco-checksum: ";
const MIGRATION_CHECKSUM_PLACEHOLDER: &str = "__DINOCO_INTERNAL_SHA256_PLACEHOLDER_7F43A9C2__";

pub async fn generate(workspace: Option<String>) -> anyhow::Result<()> {
    let (_, schema, workspace) = read_schema_for_workspace(workspace.as_deref())?;
    let config = runtime_config(&schema)?;
    let db = CliDatabase::connect(&config).await?;
    db.adopt_legacy_migration_history().await?;
    let migrations_root = migrations_root(workspace.as_deref());
    let mut migrations = migration_dirs(&migrations_root)?;
    migrations.sort();

    fs::create_dir_all(&migrations_root)?;
    let upgraded = upgrade_legacy_migration_artifacts(&migrations)?;
    if upgraded > 0 {
        ui::info(format!("Upgraded {upgraded} legacy migration(s) to the current artifact layout."));
    }

    let current = db.inspect_schema().await?;
    let history = inspect_sqlite_migration_history(&db, &config, &migrations).await?;
    let server_history = inspect_server_migration_history(&db, &config, &migrations).await?;
    let desired = inspect_shadow_schema(&db, &config, &schema).await?;
    if let Some(server_history) = &server_history {
        let pending = server_history.pending_names(&migrations)?;
        if !pending.is_empty() {
            anyhow::bail!(
                "Cannot generate a migration while local migrations are pending: {}. Run `dinoco migrate run` first.",
                pending.join(", ")
            );
        }
    }
    if let Some(history) = &history {
        let pending = history.pending_names(&migrations)?;
        if !pending.is_empty() {
            let drift = plan_database_migration(&history.expected, &current);
            if drift.steps.is_empty() {
                anyhow::bail!(
                    "Cannot generate a migration while local migrations are pending: {}. Run `dinoco migrate run` first.",
                    pending.join(", ")
                );
            }
            if history.applied.is_empty() && (!current.tables.is_empty() || !current.enums.is_empty()) {
                anyhow::bail!(
                    "Cannot adopt an untracked SQLite baseline while local migrations are pending: {}. Back up the database and reconcile whether those migrations are already represented before continuing.",
                    pending.join(", ")
                );
            }

            repair_pending_history_drift(&db, history, drift).await?;
            ui::success(format!(
                "SQLite schema drift repaired against applied history. No migration was generated; run `dinoco migrate run` to apply: {}.",
                pending.join(", ")
            ));
            return Ok(());
        }
    }
    let mut live_plan = plan_database_migration(&desired, &current);
    mark_unvalidated_legacy_foreign_keys(&db, &mut live_plan);
    if let Some(server_history) =
        server_history.as_ref().filter(|history| !history.applied.is_empty() && history.expected.is_none())
    {
        if live_plan.steps.is_empty() {
            if !confirm_untracked_baseline()? {
                ui::warning("Migration history snapshot adoption cancelled.");
                return Ok(());
            }
            let latest = server_history.applied.last().expect("legacy history is not empty");
            db.record_server_schema_snapshot(latest, &current).await?;
            persist_legacy_checksums(&db, history.as_ref(), Some(server_history)).await?;
            dinoco_codegen::generate_models_for_workspace(&schema, workspace.as_deref())?;
            ui::success("Legacy migration history adopted with a canonical schema snapshot.");
            ui::success("Rust models generated at dinoco/models/");
            return Ok(());
        }
        ui::info("Legacy migration history will be normalized through a new data-preserving migration.");
    }
    let mut migration_plan = live_plan.clone();
    let mut repairing_drift = false;
    let history_expected = history
        .as_ref()
        .map(|history| &history.expected)
        .or_else(|| server_history.as_ref().and_then(|history| history.expected.as_ref()));
    let history_drift = history_expected.map(|expected| {
        if server_history.as_ref().is_some_and(|history| !history.tracks_indexes) {
            let mut compatible_expected = expected.clone();
            for table in &mut compatible_expected.tables {
                if let Some(current_table) = current.tables.iter().find(|current| current.name == table.name) {
                    table.indexes = current_table.indexes.clone();
                }
            }
            plan_database_migration(&compatible_expected, &current)
        } else {
            plan_database_migration(expected, &current)
        }
    });
    let untracked_sqlite_schema = history
        .as_ref()
        .map(|history| history.applied.is_empty() && (!current.tables.is_empty() || !current.enums.is_empty()))
        .unwrap_or(false);
    let untracked_server_schema = server_history
        .as_ref()
        .map(|history| history.applied.is_empty() && (!current.tables.is_empty() || !current.enums.is_empty()))
        .unwrap_or(false);
    if untracked_sqlite_schema || untracked_server_schema {
        print_untracked_schema(&current);
        if !live_plan.steps.is_empty() {
            ui::warning("The untracked database schema does not match the Dinoco schema.");
            print_plan_summary(&live_plan);
            anyhow::bail!(
                "Dinoco will not modify an existing untracked database while creating its baseline. No schema changes were applied. Back up and reconcile the listed differences manually, then run `dinoco migrate generate` again."
            );
        }
        if !confirm_untracked_baseline()? {
            ui::warning("Migration generation cancelled.");
            return Ok(());
        }

        migration_plan = plan_database_migration(&desired, &crate::db::DatabaseSchema::default());
        for step in &mut migration_plan.steps {
            if let MigrationStep::CreateTable(item) = step {
                item.if_not_exists = true;
            }
        }
    } else if let Some(drift) = history_drift.as_ref().filter(|drift| !drift.steps.is_empty()) {
        print_schema_drift(drift);
        ensure_drift_is_repairable(drift)?;
        if !confirm_schema_drift(drift)? {
            ui::warning("Migration generation cancelled.");
            return Ok(());
        }
        repairing_drift = true;
        migration_plan =
            plan_database_migration(&desired, history_expected.expect("drift requires canonical migration history"));
        make_table_drift_steps_idempotent(&mut live_plan, drift);
        make_table_drift_steps_idempotent(&mut migration_plan, drift);
    }

    if repairing_drift {
        if !live_plan.steps.is_empty() {
            ui::info("Changes required to reconcile the live SQLite database:");
            print_plan_summary(&live_plan);
            ensure_plan_is_supported(&db, &live_plan)?;
        }
        if !migration_plan.steps.is_empty() {
            ui::info("Schema evolution that will be recorded for other databases:");
            print_plan_summary(&migration_plan);
            ensure_plan_is_supported(&db, &migration_plan)?;
        }
    }

    if migration_plan.steps.is_empty() && !repairing_drift {
        persist_legacy_checksums(&db, history.as_ref(), server_history.as_ref()).await?;
        ui::info("No schema changes were found.");
        dinoco_codegen::generate_models_for_workspace(&schema, workspace.as_deref())?;
        ui::success("Rust models generated at dinoco/models/");
        return Ok(());
    }

    if migration_plan.steps.is_empty() {
        let mut repair_statements = Vec::new();
        append_checksum_statements(
            &db,
            &mut repair_statements,
            history.as_ref().map(|history| history.legacy_checksums.as_slice()).unwrap_or_default(),
        );
        repair_statements.extend(compile_plan(&db, live_plan));
        apply_statements(&db, &repair_statements)
            .await
            .context("failed to repair SQLite schema drift; all changes were rolled back")?;
        ensure_live_schema_matches(&db, &desired).await?;
        dinoco_codegen::generate_models_for_workspace(&schema, workspace.as_deref())?;
        ui::success(
            "SQLite schema drift repaired; no new migration was needed because the Dinoco schema did not change.",
        );
        ui::success("Rust models generated at dinoco/models/");
        return Ok(());
    }

    if !repairing_drift {
        print_plan_summary(&migration_plan);
        ensure_plan_is_supported(&db, &migration_plan)?;
    }

    let plans = if repairing_drift { vec![&live_plan, &migration_plan] } else { vec![&migration_plan] };
    if !confirm_migration_generation(&plans)? {
        ui::warning("Migration generation cancelled. No migration was created or applied.");
        return Ok(());
    }

    let migration_name = migration_name("generated");
    let preserved_tables = preexisting_created_tables(&migration_plan, &current);
    let down_statements = compile_down_plan(&db, &migration_plan, &migration_name, &preserved_tables);
    let migration_statements = compile_plan(&db, migration_plan);
    let (up_sql, checksum) = compile_up_artifact(
        &db,
        &migration_name,
        &migration_statements,
        history.as_ref().map(|history| history.legacy_checksums.as_slice()).unwrap_or_default(),
    );
    let live_statements = compile_plan(&db, live_plan);

    let migration_dir = publish_migration_artifacts(
        &migrations_root,
        workspace.as_deref(),
        &migration_name,
        &up_sql,
        &down_statements.join("\n\n"),
    )?;

    persist_legacy_checksums(&db, history.as_ref(), server_history.as_ref()).await?;
    let live_sql = statements_sql(&live_statements);
    let checksum = checksum.context("generated migration did not produce an integrity checksum")?;
    if let Err(error) = apply_migration_sql(&db, &migration_name, &live_sql, &checksum, true).await {
        if db.is_sqlite() {
            fs::remove_dir_all(&migration_dir)
                .with_context(|| format!("failed to remove incomplete migration {}", migration_dir.display()))?;
            return Err(error.context("failed to apply generated migration; all SQLite changes were rolled back"));
        }
        return Err(error.context(format!(
            "failed to apply generated migration; the migration files were preserved at {} because the database may be partially changed",
            migration_dir.display()
        )));
    }
    ensure_live_schema_matches(&db, &desired).await?;
    if !db.is_sqlite() {
        db.record_server_schema_snapshot(&migration_name, &desired).await?;
    }
    dinoco_codegen::generate_models_for_workspace(&schema, workspace.as_deref())?;

    ui::success(format!("Migration generated and applied: {}", migration_dir.display()));
    ui::success("Rust models generated at dinoco/models/");

    Ok(())
}

async fn inspect_shadow_schema(
    _primary: &CliDatabase,
    config: &RuntimeConfig,
    schema: &dinoco_compiler::Schema,
) -> anyhow::Result<crate::db::DatabaseSchema> {
    if config.database != Database::Sqlite {
        let mut desired = desired_database_schema(schema);
        if config.database == Database::Mysql {
            // MySQL stores ENUM values inline and does not persist the Dinoco enum type name.
            // Normalize that non-observable name so snapshots and live introspection compare
            // the actual database representation instead of reporting perpetual drift.
            for column in desired.tables.iter_mut().flat_map(|table| &mut table.columns) {
                if let dinoco_engine::MigrationColumnType::Enum { name, .. } = &mut column.ty {
                    name.clear();
                }
            }
        }
        return Ok(desired);
    }

    let shadow_file = tempfile::Builder::new()
        .prefix("dinoco-shadow-")
        .suffix(".sqlite")
        .tempfile()
        .context("failed to create a secure SQLite shadow database")?;
    let mut shadow_config = config.clone();
    shadow_config.database_url = shadow_file.path().to_string_lossy().into_owned();
    let shadow = CliDatabase::connect(&shadow_config).await?;

    for item in schema.enums() {
        for statement in shadow.compile_create_enum_migration(dinoco_engine::CreateEnumMigration {
            name: item.name.clone(),
            values: item.values.clone(),
        }) {
            shadow.execute(&statement).await?;
        }
    }
    for migration in crate::sql::generate_create_table_migrations(schema) {
        // SQLite accepts references to tables created later, and cannot add a foreign
        // key with ALTER TABLE. Keep constraints inline in the shadow schema.
        shadow.execute(&shadow.compile_create_table_migration(migration)).await?;
    }
    for table in desired_database_schema(schema).tables {
        for index in table.indexes.iter().filter(|index| !crate::sql::index_is_primary_key(index, &table)).cloned() {
            shadow
                .execute(&shadow.compile_create_index_migration(dinoco_engine::CreateIndexMigration {
                    table: table.name.clone(),
                    index,
                }))
                .await?;
        }
    }
    let inspected = shadow.inspect_schema().await?;
    drop(shadow);
    drop(shadow_file);
    Ok(inspected)
}

struct ValidatedMigration {
    execution_sql: String,
    checksum: String,
    generated: bool,
}

struct SqliteMigrationHistory {
    expected: crate::db::DatabaseSchema,
    applied: BTreeSet<String>,
    legacy_checksums: Vec<(String, String)>,
    validated: BTreeMap<String, ValidatedMigration>,
}

struct ServerMigrationHistory {
    history_exists: bool,
    expected: Option<crate::db::DatabaseSchema>,
    tracks_indexes: bool,
    applied: BTreeSet<String>,
    legacy_checksums: Vec<(String, String)>,
    validated: BTreeMap<String, ValidatedMigration>,
}

impl ServerMigrationHistory {
    fn pending_names(&self, migrations: &[PathBuf]) -> anyhow::Result<Vec<String>> {
        migrations
            .iter()
            .map(|migration| migration_directory_name(migration))
            .filter(|name| name.as_ref().map(|name| !self.applied.contains(name)).unwrap_or(true))
            .collect()
    }
}

async fn inspect_server_migration_history(
    primary: &CliDatabase,
    config: &RuntimeConfig,
    migrations: &[PathBuf],
) -> anyhow::Result<Option<ServerMigrationHistory>> {
    if config.database == Database::Sqlite {
        return Ok(None);
    }

    let local = migrations
        .iter()
        .map(|path| Ok((migration_directory_name(path)?, path)))
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    let metadata = primary.migration_metadata().await?;
    let history_exists = metadata.history_exists;
    let applied = metadata.applied.into_iter().collect::<BTreeSet<_>>();
    let recorded_checksums = metadata.checksums;
    let checksums_required = metadata.checksums_required;
    let schema_snapshots = metadata.schema_snapshots;
    let schema_snapshots_required = metadata.schema_snapshots_required;
    if checksums_required && recorded_checksums.is_none() && !applied.is_empty() {
        anyhow::bail!(
            "Migration checksum metadata is missing: this database requires the `dinoco_migration_checksums` table. Restore it from a trusted backup before continuing."
        );
    }

    validate_history_shape(&local, &applied, recorded_checksums.as_ref())?;
    if schema_snapshots_required && schema_snapshots.is_none() && !applied.is_empty() {
        anyhow::bail!(
            "Canonical migration schema snapshots are missing. Restore `dinoco_migration_schemas` from a trusted backup before continuing."
        );
    }
    if let Some(snapshots) = &schema_snapshots {
        let orphaned = snapshots.keys().filter(|name| !applied.contains(*name)).cloned().collect::<Vec<_>>();
        if !orphaned.is_empty() {
            anyhow::bail!("Migration schema snapshot metadata is inconsistent for: {}.", orphaned.join(", "));
        }
    }
    let mut tracks_indexes = true;
    let expected = if let Some(latest) = applied.last() {
        match schema_snapshots.as_ref() {
            Some(snapshots) => {
                let snapshot = snapshots
                    .get(latest)
                    .with_context(|| format!("Applied migration `{latest}` has no canonical schema snapshot"))?;
                let snapshot_value: serde_json::Value = serde_json::from_str(snapshot)
                    .with_context(|| format!("canonical schema snapshot for migration `{latest}` is invalid"))?;
                tracks_indexes = snapshot_tracks_indexes(&snapshot_value);
                Some(
                    serde_json::from_value(snapshot_value)
                        .with_context(|| format!("canonical schema snapshot for migration `{latest}` is invalid"))?,
                )
            }
            None => None,
        }
    } else {
        Some(crate::db::DatabaseSchema::default())
    };

    let mut legacy_checksums = Vec::new();
    let mut validated = BTreeMap::new();
    for (name, path) in local {
        let sql_path = migration_sql_path(path)?;
        let sql = fs::read_to_string(&sql_path).with_context(|| format!("failed to read {}", sql_path.display()))?;
        let checksum = server_migration_checksum(&sql);
        if applied.contains(&name) {
            match recorded_checksums.as_ref() {
                Some(recorded) if !recorded.contains_key(&name) => {
                    anyhow::bail!(
                        "Migration checksum metadata is incomplete: applied migration `{name}` has no checksum row. Restore the missing checksum record from a trusted backup before continuing."
                    );
                }
                Some(recorded) if recorded.get(&name) != Some(&checksum) => {
                    let original = recorded.get(&name).expect("checksum presence was checked");
                    anyhow::bail!(
                        "Applied migration `{name}` was modified after it ran: its current SQL checksum is {checksum}, but the database recorded {original}. Restore the original migration file before continuing."
                    );
                }
                Some(_) => {}
                None => legacy_checksums.push((name.clone(), checksum.clone())),
            }
        }

        let stripped = legacy_generated_migration_sql(primary, &name, &sql);
        let generated = stripped.is_some();
        let execution_sql = stripped.unwrap_or(sql);
        validated.insert(name, ValidatedMigration { execution_sql, checksum, generated });
    }

    Ok(Some(ServerMigrationHistory { history_exists, expected, tracks_indexes, applied, legacy_checksums, validated }))
}

fn snapshot_tracks_indexes(snapshot: &serde_json::Value) -> bool {
    snapshot
        .get("tables")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|tables| tables.is_empty() || tables.iter().all(|table| table.get("indexes").is_some()))
}

fn validate_history_shape(
    local: &BTreeMap<String, &PathBuf>,
    applied: &BTreeSet<String>,
    recorded_checksums: Option<&BTreeMap<String, String>>,
) -> anyhow::Result<()> {
    let missing_directories = applied.iter().filter(|name| !local.contains_key(*name)).cloned().collect::<Vec<_>>();
    if !missing_directories.is_empty() {
        anyhow::bail!(
            "Migration history cannot be verified because applied migration directories are missing: {}.",
            missing_directories.join(", ")
        );
    }

    let mut found_pending = false;
    for name in local.keys() {
        if applied.contains(name) {
            if found_pending {
                anyhow::bail!(
                    "Migration history order is inconsistent: applied migration `{name}` appears after a pending migration."
                );
            }
        } else {
            found_pending = true;
        }
    }

    if let Some(recorded_checksums) = recorded_checksums {
        let orphaned = recorded_checksums.keys().filter(|name| !applied.contains(*name)).cloned().collect::<Vec<_>>();
        if !orphaned.is_empty() {
            anyhow::bail!(
                "Migration checksum metadata is inconsistent: checksum rows exist without matching applied history records for: {}.",
                orphaned.join(", ")
            );
        }
    }
    Ok(())
}

impl SqliteMigrationHistory {
    fn pending_names(&self, migrations: &[PathBuf]) -> anyhow::Result<Vec<String>> {
        migrations
            .iter()
            .map(|migration| migration_directory_name(migration))
            .filter(|name| name.as_ref().map(|name| !self.applied.contains(name)).unwrap_or(true))
            .collect()
    }
}

async fn inspect_sqlite_migration_history(
    primary: &CliDatabase,
    config: &RuntimeConfig,
    migrations: &[PathBuf],
) -> anyhow::Result<Option<SqliteMigrationHistory>> {
    if config.database != Database::Sqlite {
        return Ok(None);
    }

    let local = migrations
        .iter()
        .map(|path| Ok((migration_directory_name(path)?, path)))
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    let metadata = primary.sqlite_migration_metadata().await?;
    let applied = metadata.applied.into_iter().collect::<BTreeSet<_>>();
    let recorded_checksums = metadata.checksums;
    let checksums_required = metadata.checksums_required;
    if checksums_required && recorded_checksums.is_none() && !applied.is_empty() {
        anyhow::bail!(
            "Migration checksum metadata is missing: this database requires the `dinoco_migration_checksums` table. Restore it from a trusted backup before continuing."
        );
    }

    let missing_directories = applied.iter().filter(|name| !local.contains_key(*name)).cloned().collect::<Vec<_>>();
    if !missing_directories.is_empty() {
        anyhow::bail!(
            "Migration history cannot be verified because applied migration directories are missing: {}.",
            missing_directories.join(", ")
        );
    }

    let mut found_pending = false;
    for name in local.keys() {
        if applied.contains(name) {
            if found_pending {
                anyhow::bail!(
                    "Migration history order is inconsistent: applied migration `{name}` appears after a pending migration."
                );
            }
        } else {
            found_pending = true;
        }
    }

    if let Some(recorded_checksums) = &recorded_checksums {
        let orphaned = recorded_checksums.keys().filter(|name| !applied.contains(*name)).cloned().collect::<Vec<_>>();
        if !orphaned.is_empty() {
            anyhow::bail!(
                "Migration checksum metadata is inconsistent: checksum rows exist without matching applied history records for: {}.",
                orphaned.join(", ")
            );
        }
    }

    let shadow_file = tempfile::Builder::new()
        .prefix("dinoco-history-")
        .suffix(".sqlite")
        .tempfile()
        .context("failed to create a secure SQLite history database")?;
    let mut shadow_config = config.clone();
    shadow_config.database_url = shadow_file.path().to_string_lossy().into_owned();
    let shadow = CliDatabase::connect(&shadow_config).await?;
    let inspection = async {
        shadow.execute_transaction(&[shadow.compile_create_migrations_table()]).await?;
        let mut legacy_checksums = Vec::new();
        let mut validated = BTreeMap::new();
        let mut expected = None;
        for (name, path) in &local {
            let sql_path = migration_sql_path(path)?;
            let sql =
                fs::read_to_string(&sql_path).with_context(|| format!("failed to read {}", sql_path.display()))?;
            let checksum = migration_checksum(&shadow, name, &sql)?;
            let generated = contains_checksum_marker(&sql);
            if applied.contains(name) {
                match recorded_checksums.as_ref() {
                    Some(recorded_checksums) if !recorded_checksums.contains_key(name) => {
                        anyhow::bail!(
                            "Migration checksum metadata is incomplete: applied migration `{name}` has no checksum row. Restore the missing `dinoco_migration_checksums` record from a trusted backup before continuing."
                        );
                    }
                    Some(recorded_checksums) if recorded_checksums.get(name) != Some(&checksum) => {
                        let recorded = recorded_checksums.get(name).expect("checksum presence was checked");
                        anyhow::bail!(
                            "Applied migration `{name}` was modified after it ran: its current SQL checksum is {checksum}, but the database recorded {recorded}. Restore the original migration file before continuing."
                        );
                    }
                    Some(_) => {}
                    None if generated => {
                        anyhow::bail!(
                            "Migration checksum metadata is missing: applied generated migration `{name}` requires the `dinoco_migration_checksums` table. Restore it from a trusted backup before continuing."
                        );
                    }
                    None => legacy_checksums.push((name.clone(), checksum.clone())),
                }
            } else if expected.is_none() {
                expected = Some(shadow.inspect_schema().await?);
            }

            let execution_sql = if generated {
                managed_sqlite_migration_sql(&sql)?
            } else {
                legacy_generated_migration_sql(&shadow, name, &sql).unwrap_or_else(|| sql.clone())
            };
            let state = if applied.contains(name) { "applied" } else { "pending" };
            shadow.replay_sqlite_history_migration(execution_sql.clone(), generated).await.with_context(|| {
                format!("failed to safely replay {state} migration `{name}` in the SQLite history database")
            })?;
            validated.insert(name.clone(), ValidatedMigration { execution_sql, checksum, generated });
        }
        let expected = match expected {
            Some(expected) => expected,
            None => shadow.inspect_schema().await?,
        };
        Ok((expected, legacy_checksums, validated))
    }
    .await;
    drop(shadow);
    drop(shadow_file);

    let (expected, legacy_checksums, validated) = inspection?;
    Ok(Some(SqliteMigrationHistory { expected, applied, legacy_checksums, validated }))
}

fn migration_directory_name(path: &Path) -> anyhow::Result<String> {
    path.file_name().and_then(|name| name.to_str()).map(str::to_string).context("invalid migration directory name")
}

fn migration_sql_path(directory: &Path) -> anyhow::Result<PathBuf> {
    let current = directory.join("up.sql");
    if current.is_file() {
        return Ok(current);
    }

    let legacy = directory.join("migration.sql");
    if legacy.is_file() {
        return Ok(legacy);
    }

    anyhow::bail!("migration directory {} contains neither up.sql nor the legacy migration.sql", directory.display())
}

fn upgrade_legacy_migration_artifacts(migrations: &[PathBuf]) -> anyhow::Result<usize> {
    let mut upgraded = 0;
    for directory in migrations {
        let legacy = directory.join("migration.sql");
        if !legacy.is_file() {
            continue;
        }

        let current = directory.join("up.sql");
        let down = directory.join("down.sql");
        let mut changed = false;
        if !current.exists() {
            let sql = fs::read(&legacy).with_context(|| format!("failed to read {}", legacy.display()))?;
            write_atomic_file(&current, &sql)?;
            changed = true;
        }
        if !down.exists() {
            let name = migration_directory_name(directory)?;
            let rollback = format!(
                "-- Migration `{name}` was imported from the legacy Dinoco format.\n\
                 -- No safe automatic rollback was recorded by that format.\n\
                 -- This file intentionally leaves application data and schema unchanged.\n"
            );
            write_atomic_file(&down, rollback.as_bytes())?;
            changed = true;
        }
        if changed {
            OpenOptions::new()
                .read(true)
                .open(directory)?
                .sync_all()
                .with_context(|| format!("failed to sync upgraded migration directory {}", directory.display()))?;
            upgraded += 1;
        }
    }
    Ok(upgraded)
}

fn print_schema_drift(drift: &MigrationPlan) {
    ui::warning("Database schema drift detected: the live schema differs from applied migration history.");
    for step in &drift.steps {
        ui::warning(format!("  {}", describe_drift_step(step)));
    }
    ui::warning("A repair can recreate structure, but it cannot recover rows or column values deleted manually.");
}

fn print_untracked_schema(schema: &crate::db::DatabaseSchema) {
    ui::warning("The database already contains schema that is not tracked by Dinoco migrations.");
    for table in &schema.tables {
        ui::warning(format!("  Existing table `{}` with {} row(s)", table.name, table.row_count));
    }
    ui::warning("Dinoco can record a baseline only when the existing structure exactly matches the Dinoco schema.");
}

fn describe_drift_step(step: &MigrationStep) -> String {
    match step {
        MigrationStep::CreateEnum(item) => format!("Missing enum `{}`", item.name),
        MigrationStep::DropEnum(item) => format!("Unexpected enum `{}`", item.name),
        MigrationStep::AlterEnum(item) => format!("Changed enum `{}`", item.name),
        MigrationStep::CreateTable(item) => format!("Missing table `{}`", item.table),
        MigrationStep::DropTable(item) => format!("Unexpected table `{}`", item.table),
        MigrationStep::RenameTable(item) => format!("Table `{}` differs from expected `{}`", item.from, item.to),
        MigrationStep::AddColumn(item) => format!("Missing column `{}.{}`", item.table, item.column.name),
        MigrationStep::DropColumn(item) => format!("Unexpected column `{}.{}`", item.table, item.column),
        MigrationStep::AlterColumn(item) => format!("Changed column `{}.{}`", item.table, item.desired.name),
        MigrationStep::RenameColumn(item) => {
            format!("Column `{}.{}` differs from expected `{}`", item.table, item.from, item.to)
        }
        MigrationStep::AddForeignKey(item) => {
            format!("Missing foreign key `{}.{}`", item.table, item.foreign_key.name)
        }
        MigrationStep::DropForeignKey(item) => format!("Unexpected foreign key `{}.{}`", item.table, item.name),
        MigrationStep::CreateIndex(item) => format!("Missing index `{}.{}`", item.table, item.index.name),
        MigrationStep::DropIndex(item) => format!("Unexpected index `{}.{}`", item.table, item.index.name),
    }
}

fn ensure_drift_is_repairable(drift: &MigrationPlan) -> anyhow::Result<()> {
    let unsupported = drift
        .steps
        .iter()
        .filter(|step| {
            !matches!(
                step,
                MigrationStep::CreateTable(_)
                    | MigrationStep::RenameTable(_)
                    | MigrationStep::CreateIndex(_)
                    | MigrationStep::DropIndex(_)
            )
        })
        .map(describe_drift_step)
        .collect::<Vec<_>>();
    if !unsupported.is_empty() {
        anyhow::bail!(
            "Dinoco will not automatically repair this drift because doing so could overwrite data: {}. Restore the database from a backup or reconcile these objects manually, then run the command again.",
            unsupported.join("; ")
        );
    }
    Ok(())
}

fn make_table_drift_steps_idempotent(plan: &mut MigrationPlan, drift: &MigrationPlan) {
    let drift_tables = drift
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::CreateTable(item) => Some(item.table.as_str()),
            MigrationStep::DropTable(item) => Some(item.table.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for step in &mut plan.steps {
        match step {
            MigrationStep::CreateTable(item) if drift_tables.contains(item.table.as_str()) => {
                item.if_not_exists = true;
            }
            MigrationStep::DropTable(item) if drift_tables.contains(item.table.as_str()) => {
                item.if_exists = true;
            }
            _ => {}
        }
    }
}

fn confirm_schema_drift(_drift: &MigrationPlan) -> anyhow::Result<bool> {
    if std::env::var("DINOCO_CLI_CONFIRM_DRIFT").ok().as_deref() == Some("true") {
        return Ok(true);
    }

    Confirm::new(
        "The database was changed outside Dinoco migrations. Continue by recording and applying a schema repair?",
    )
    .with_default(false)
    .prompt()
    .map_err(Into::into)
}

fn confirm_untracked_baseline() -> anyhow::Result<bool> {
    if std::env::var("DINOCO_CLI_CONFIRM_DRIFT").ok().as_deref() == Some("true") {
        return Ok(true);
    }

    Confirm::new(
        "The existing tables match the Dinoco schema and will be adopted without being recreated. Record this baseline?",
    )
    .with_default(false)
    .prompt()
    .map_err(Into::into)
}

async fn repair_pending_history_drift(
    db: &CliDatabase,
    history: &SqliteMigrationHistory,
    mut drift: MigrationPlan,
) -> anyhow::Result<()> {
    print_schema_drift(&drift);
    ensure_drift_is_repairable(&drift)?;
    if !confirm_schema_drift(&drift)? {
        anyhow::bail!("Migration generation cancelled before repairing SQLite schema drift.");
    }

    let drift_snapshot = drift.clone();
    make_table_drift_steps_idempotent(&mut drift, &drift_snapshot);
    print_plan_summary(&drift);
    ensure_plan_is_supported(db, &drift)?;
    if drift.warnings.iter().any(|warning| warning.destructive) && !confirm_destructive_plan(&drift)? {
        anyhow::bail!("Migration generation cancelled before applying destructive SQLite drift repair.");
    }

    let mut statements = Vec::new();
    append_checksum_statements(db, &mut statements, &history.legacy_checksums);
    statements.extend(compile_plan(db, drift));
    apply_statements(db, &statements)
        .await
        .context("failed to repair SQLite schema drift; all changes were rolled back")?;
    ensure_live_schema_matches(db, &history.expected).await
}

pub async fn run(workspace: Option<String>) -> anyhow::Result<()> {
    let (_, schema, workspace) = read_schema_for_workspace(workspace.as_deref())?;
    let config = runtime_config(&schema)?;
    let db = CliDatabase::connect(&config).await?;
    db.adopt_legacy_migration_history().await?;

    let migrations_root = migrations_root(workspace.as_deref());
    let mut migrations = migration_dirs(&migrations_root)?;
    migrations.sort();
    let mut history = inspect_sqlite_migration_history(&db, &config, &migrations).await?;
    let server_history = inspect_server_migration_history(&db, &config, &migrations).await?;
    if server_history.as_ref().is_some_and(|history| !history.history_exists)
        && db.database_has_user_tables().await?
        && !migrations.is_empty()
    {
        anyhow::bail!(
            "Refusing to run migrations against a populated database without a `dinoco_migrations` table. Generate a baseline migration or back up and reconcile the database first."
        );
    }

    while let Some(snapshot) = &history {
        let current = db.inspect_schema().await?;
        let drift = plan_database_migration(&snapshot.expected, &current);
        if drift.steps.is_empty() {
            break;
        }

        let refreshed = inspect_sqlite_migration_history(&db, &config, &migrations)
            .await?
            .context("SQLite migration history disappeared while checking schema drift")?;
        if refreshed.applied != snapshot.applied {
            history = Some(refreshed);
            continue;
        }

        print_schema_drift(&drift);
        anyhow::bail!(
            "Refusing to run pending migrations while SQLite schema drift exists. Restore the expected schema or use `dinoco migrate generate` to review a supported table-level repair."
        );
    }
    if let Some(server_history) = &server_history {
        if let Some(expected) = &server_history.expected {
            let current = db.inspect_schema().await?;
            let drift = plan_database_migration(expected, &current);
            if !drift.steps.is_empty() {
                print_schema_drift(&drift);
                anyhow::bail!(
                    "Refusing to run pending migrations while server schema drift exists. Restore the expected schema or use `dinoco migrate generate` to review a supported repair."
                );
            }
        } else if !server_history.applied.is_empty() {
            let pending = server_history.pending_names(&migrations)?;
            if !pending.is_empty() {
                let recoverable = matches!(db, CliDatabase::Postgres(_) | CliDatabase::PgBouncer(_))
                    && pending.iter().all(|name| {
                        server_history.validated.get(name).is_some_and(is_generated_legacy_normalization_migration)
                    });
                if !recoverable {
                    anyhow::bail!(
                        "Refusing to run pending migrations because this legacy server history has no canonical schema snapshot. Reconcile schema.dinoco with the live database and run `dinoco migrate generate` to adopt the history first."
                    );
                }
                ui::info(
                    "Recovering a pending generated legacy normalization; PostgreSQL foreign keys will preserve unvalidated historical rows.",
                );
            }
        }
    }
    persist_legacy_checksums(&db, history.as_ref(), server_history.as_ref()).await?;

    if migrations.is_empty() {
        ui::info("No migrations were found.");
        return Ok(());
    }

    for migration in &migrations {
        let name = migration_directory_name(migration)?;
        let applied = history
            .as_ref()
            .map(|history| history.applied.contains(&name))
            .or_else(|| server_history.as_ref().map(|history| history.applied.contains(&name)))
            .unwrap_or(false);
        if applied {
            ui::info(format!("Skipping already applied migration: {name}"));
            continue;
        }

        let (mut sql, checksum, generated) = if let Some(history) = &history {
            let migration = history
                .validated
                .get(&name)
                .with_context(|| format!("validated SQLite migration `{name}` was not available"))?;
            (migration.execution_sql.clone(), migration.checksum.clone(), migration.generated)
        } else if let Some(history) = &server_history {
            let migration = history
                .validated
                .get(&name)
                .with_context(|| format!("validated server migration `{name}` was not available"))?;
            (migration.execution_sql.clone(), migration.checksum.clone(), migration.generated)
        } else {
            let sql_path = migration_sql_path(migration)?;
            let sql =
                fs::read_to_string(&sql_path).with_context(|| format!("failed to read {}", sql_path.display()))?;
            let checksum = migration_checksum(&db, &name, &sql)?;
            (sql, checksum, false)
        };
        if generated
            && matches!(db, CliDatabase::Postgres(_) | CliDatabase::PgBouncer(_))
            && server_history.as_ref().is_some_and(|history| history.expected.is_none())
        {
            sql = postgres_preserve_legacy_foreign_key_rows(&sql)?;
        }
        let applied = apply_migration_sql(&db, &name, &sql, &checksum, generated).await.with_context(|| {
            match config.database {
                Database::Sqlite => {
                    format!("migration `{name}` failed; its SQLite changes and history record were rolled back")
                }
                Database::Postgresql => {
                    format!("migration `{name}` failed; its PostgreSQL changes and history record were rolled back")
                }
                Database::Mysql => format!(
                    "migration `{name}` failed; MySQL may have committed earlier DDL statements, but Dinoco did not record the migration as applied"
                ),
            }
        })?;
        if applied {
            ui::success(format!("Migration applied: {}", migration.display()));
        } else {
            ui::info(format!("Skipping migration applied concurrently: {name}"));
        }
        if !db.is_sqlite() {
            let snapshot = db.inspect_schema().await?;
            db.record_server_schema_snapshot(&name, &snapshot).await?;
        }
    }

    if config.database == Database::Sqlite {
        let history = inspect_sqlite_migration_history(&db, &config, &migrations)
            .await?
            .context("SQLite migration history was not available after applying migrations")?;
        let current = db.inspect_schema().await?;
        let drift = plan_database_migration(&history.expected, &current);
        if !drift.steps.is_empty() {
            print_schema_drift(&drift);
            anyhow::bail!("Migrations finished but the resulting SQLite schema does not match their recorded history.");
        }
    }
    ui::success("All pending migrations were applied.");

    Ok(())
}

fn is_generated_legacy_normalization_migration(migration: &ValidatedMigration) -> bool {
    migration.generated
        && split_sql(&migration.execution_sql).is_ok_and(|statements| {
            statements.iter().any(|statement| {
                let normalized = statement.to_ascii_uppercase();
                normalized.contains("ALTER TABLE") && normalized.contains(" RENAME TO ")
            })
        })
}

fn postgres_preserve_legacy_foreign_key_rows(sql: &str) -> anyhow::Result<String> {
    let statements = split_sql(sql)?
        .into_iter()
        .map(|statement| {
            let normalized = statement.to_ascii_uppercase();
            if normalized.contains("ALTER TABLE")
                && normalized.contains(" ADD ")
                && normalized.contains("FOREIGN KEY")
                && !normalized.contains("NOT VALID")
            {
                format!("{} NOT VALID", statement.trim_end())
            } else {
                statement
            }
        })
        .collect::<Vec<_>>();
    Ok(statements_sql(&statements))
}

async fn apply_statements(db: &CliDatabase, statements: &[String]) -> anyhow::Result<()> {
    let statements = statements
        .iter()
        .filter(|statement| {
            let statement = statement.trim();
            !statement.is_empty() && !statement.starts_with("--")
        })
        .cloned()
        .collect::<Vec<_>>();
    db.execute_transaction(&statements).await
}

async fn ensure_live_schema_matches(db: &CliDatabase, desired: &crate::db::DatabaseSchema) -> anyhow::Result<()> {
    let current = db.inspect_schema().await?;
    let remaining = plan_database_migration(desired, &current);
    if !remaining.steps.is_empty() {
        print_schema_drift(&remaining);
        anyhow::bail!("The migration finished, but the live database still does not match the Dinoco schema.");
    }
    Ok(())
}

async fn apply_migration_sql(
    db: &CliDatabase,
    name: &str,
    sql: &str,
    checksum: &str,
    generated: bool,
) -> anyhow::Result<bool> {
    if db.is_sqlite() {
        return db.apply_sqlite_migration(name.to_string(), sql.to_string(), checksum.to_string(), generated).await;
    }

    validate_server_migration_sql(sql)?;
    let statements = split_sql(sql)?;
    db.apply_server_migration(name, &statements, checksum).await
}

async fn persist_legacy_checksums(
    db: &CliDatabase,
    sqlite: Option<&SqliteMigrationHistory>,
    server: Option<&ServerMigrationHistory>,
) -> anyhow::Result<()> {
    let legacy_checksums = sqlite
        .map(|history| history.legacy_checksums.as_slice())
        .or_else(|| server.map(|history| history.legacy_checksums.as_slice()))
        .unwrap_or_default();
    if db.is_sqlite() {
        let mut statements = Vec::new();
        append_checksum_statements(db, &mut statements, legacy_checksums);
        db.execute_transaction(&statements).await?;
    } else {
        db.record_legacy_migration_checksums(legacy_checksums).await?;
    }
    if !legacy_checksums.is_empty() {
        ui::info(format!("Recorded integrity checksums for {} legacy migration(s).", legacy_checksums.len()));
    }
    Ok(())
}

fn append_checksum_statements(db: &CliDatabase, statements: &mut Vec<String>, checksums: &[(String, String)]) {
    if !db.is_sqlite() || checksums.is_empty() {
        return;
    }

    statements.push(db.compile_create_migration_checksums_table());
    statements.push(db.compile_create_migration_checksum_guard());
    statements.extend(checksums.iter().map(|(name, checksum)| db.compile_insert_migration_checksum(name, checksum)));
}

fn compile_up_artifact(
    db: &CliDatabase,
    migration_name: &str,
    migration_statements: &[String],
    legacy_checksums: &[(String, String)],
) -> (String, Option<String>) {
    let mut statements = vec![db.compile_create_migrations_table()];
    if db.is_sqlite() {
        statements.push(db.compile_create_migration_checksums_table());
        statements.push(db.compile_create_migration_checksum_guard());
        statements.extend(
            legacy_checksums
                .iter()
                .map(|(name, checksum)| db.compile_insert_migration_checksum_if_applied(name, checksum)),
        );
    }
    statements.extend(migration_statements.iter().cloned());
    statements.push(db.compile_insert_migration_record(migration_name));
    let sql_body = statements
        .iter()
        .map(|statement| {
            let statement = statement.trim_end();
            if statement.ends_with(';') { statement.to_string() } else { format!("{statement};") }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    if !db.is_sqlite() {
        let checksum = server_migration_checksum(&sql_body);
        return (sql_body, Some(checksum));
    }

    let checksum_template = db.compile_insert_migration_checksum(migration_name, MIGRATION_CHECKSUM_PLACEHOLDER);
    let canonical = format!(
        "PRAGMA foreign_keys = ON;\n\nBEGIN IMMEDIATE;\n\nPRAGMA defer_foreign_keys = ON;\n\n{sql_body}\n\n{};\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{MIGRATION_CHECKSUM_PLACEHOLDER}",
        checksum_template.trim_end_matches(';')
    );
    let checksum = raw_migration_checksum(&canonical);
    let checksum_insert = db.compile_insert_migration_checksum(migration_name, &checksum);
    let artifact = format!(
        "PRAGMA foreign_keys = ON;\n\nBEGIN IMMEDIATE;\n\nPRAGMA defer_foreign_keys = ON;\n\n{sql_body}\n\n{};\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{checksum}",
        checksum_insert.trim_end_matches(';')
    );
    (artifact, Some(checksum))
}

fn migration_checksum(db: &CliDatabase, migration_name: &str, sql: &str) -> anyhow::Result<String> {
    if !db.is_sqlite() {
        return Ok(server_migration_checksum(sql));
    }
    let normalized = normalize_checksum_line_endings(sql);
    let markers = checksum_markers(&normalized);
    if markers.is_empty() {
        return Ok(raw_migration_checksum(sql));
    }
    if markers.len() != 1 {
        anyhow::bail!("Generated migration contains more than one Dinoco checksum marker.");
    }

    let declared = markers[0];
    if declared.len() != 64 || !declared.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        anyhow::bail!("Generated migration contains an invalid Dinoco checksum marker.");
    }

    let checksum_insert = db.compile_insert_migration_checksum(migration_name, declared);
    let actual_suffix =
        format!("{};\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{declared}", checksum_insert.trim_end_matches(';'));
    let Some(prefix) = normalized.strip_suffix(&actual_suffix) else {
        anyhow::bail!(
            "Generated migration checksum metadata is malformed or does not match migration directory `{migration_name}`."
        );
    };
    let checksum_template = db.compile_insert_migration_checksum(migration_name, MIGRATION_CHECKSUM_PLACEHOLDER);
    let canonical_suffix = format!(
        "{};\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{MIGRATION_CHECKSUM_PLACEHOLDER}",
        checksum_template.trim_end_matches(';')
    );
    let computed = raw_migration_checksum(&format!("{prefix}{canonical_suffix}"));
    if declared != computed {
        anyhow::bail!(
            "Generated migration checksum marker is invalid: the file declares {declared}, but its contents hash to {computed}."
        );
    }
    Ok(computed)
}

fn contains_checksum_marker(sql: &str) -> bool {
    !checksum_markers(&normalize_checksum_line_endings(sql)).is_empty()
}

fn checksum_markers(sql: &str) -> Vec<&str> {
    sql.lines().filter_map(|line| line.trim().strip_prefix(MIGRATION_CHECKSUM_MARKER)).collect()
}

fn managed_sqlite_migration_sql(sql: &str) -> anyhow::Result<String> {
    let normalized = normalize_all_sql_line_endings(sql);
    let markers = checksum_markers(&normalized);
    if markers.is_empty() {
        return Ok(sql.to_string());
    }
    if markers.len() != 1 {
        anyhow::bail!("Generated migration contains more than one Dinoco checksum marker.");
    }

    let prefix = "PRAGMA foreign_keys = ON;\n\nBEGIN IMMEDIATE;\n\nPRAGMA defer_foreign_keys = ON;\n\n";
    let suffix = format!("\n\nCOMMIT;\n\n{MIGRATION_CHECKSUM_MARKER}{}", markers[0]);
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

fn legacy_generated_migration_sql(db: &CliDatabase, migration_name: &str, sql: &str) -> Option<String> {
    let sql = sql.trim();
    let prefix = db.compile_create_migrations_table();
    let suffix = db.compile_insert_migration_record(migration_name);
    if let Some(body) = sql.strip_prefix(&prefix).and_then(|body| body.strip_suffix(&suffix)) {
        return Some(body.trim().to_string());
    }

    let statements = top_level_sql_statement_ranges(sql);
    let (first_start, first_end) = *statements.first()?;
    let (last_start, last_end) = *statements.last()?;
    if statements.len() < 2
        || normalize_sql_statement_whitespace(&sql[first_start..first_end])
            != normalize_sql_statement_whitespace(&prefix)
        || normalize_sql_statement_whitespace(&sql[last_start..last_end]) != normalize_sql_statement_whitespace(&suffix)
    {
        return None;
    }

    Some(sql[first_end + 1..last_start].trim().to_string())
}

fn normalize_sql_statement_whitespace(statement: &str) -> String {
    statement.chars().filter(|character| !character.is_whitespace()).collect()
}

fn top_level_sql_statement_ranges(sql: &str) -> Vec<(usize, usize)> {
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

    let bytes = sql.as_bytes();
    let mut ranges = Vec::new();
    let mut state = State::Normal;
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        match state {
            State::Normal => match (byte, next) {
                (b'-', Some(b'-')) => {
                    state = State::LineComment;
                    index += 2;
                }
                (b'/', Some(b'*')) => {
                    state = State::BlockComment;
                    index += 2;
                }
                (b'\'', _) => {
                    state = State::SingleQuote;
                    index += 1;
                }
                (b'"', _) => {
                    state = State::DoubleQuote;
                    index += 1;
                }
                (b'`', _) => {
                    state = State::Backtick;
                    index += 1;
                }
                (b'[', _) => {
                    state = State::Bracket;
                    index += 1;
                }
                (b';', _) => {
                    if !sql[start..index].trim().is_empty() {
                        ranges.push((start, index));
                    }
                    start = index + 1;
                    index += 1;
                }
                _ => index += 1,
            },
            State::LineComment => {
                index += 1;
                if byte == b'\n' || byte == b'\r' {
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                if byte == b'*' && next == Some(b'/') {
                    state = State::Normal;
                    index += 2;
                } else {
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
                index += 1;
                if byte == closing {
                    if bytes.get(index) == Some(&closing) {
                        index += 1;
                    } else {
                        state = State::Normal;
                    }
                }
            }
        }
    }
    if !sql[start..].trim().is_empty() {
        ranges.push((start, sql.len()));
    }
    ranges
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

fn raw_migration_checksum(sql: &str) -> String {
    Sha256::digest(sql.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn server_migration_checksum(sql: &str) -> String {
    raw_migration_checksum(&normalize_checksum_line_endings(sql))
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

fn ensure_plan_is_supported(db: &CliDatabase, plan: &MigrationPlan) -> anyhow::Result<()> {
    let primary_key_changes = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::AlterColumn(item) if item.current.primary_key != item.desired.primary_key => {
                Some(format!("`{}.{}`", item.table, item.desired.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !primary_key_changes.is_empty() {
        anyhow::bail!(
            "Changing primary-key membership on existing columns requires a reviewed custom migration so dependent foreign keys and duplicate/null data can be handled explicitly: {}. Dinoco stopped before writing or applying a partial migration.",
            primary_key_changes.join(", ")
        );
    }

    let unique_changes = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::AlterColumn(item) if item.current.unique != item.desired.unique => {
                Some(format!("`{}.{}`", item.table, item.desired.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !unique_changes.is_empty() {
        anyhow::bail!(
            "Changing @unique on existing columns requires a reviewed custom migration because constraint names and duplicate data must be verified explicitly: {}. Dinoco stopped before writing or applying a partial migration.",
            unique_changes.join(", ")
        );
    }

    let unsupported_enum_changes = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::AlterEnum(item)
                if item.desired_values.len() < item.current_values.len()
                    || item.desired_values[..item.current_values.len()] != item.current_values =>
            {
                Some(format!("`{}`", item.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !unsupported_enum_changes.is_empty() {
        anyhow::bail!(
            "Removing, reordering, or inserting values before existing PostgreSQL enum values requires a reviewed custom migration: {}. Dinoco only generates safe append-only enum changes and stopped before writing migration history.",
            unsupported_enum_changes.join(", ")
        );
    }

    if matches!(db, CliDatabase::Postgres(_) | CliDatabase::PgBouncer(_)) {
        let identity_changes = plan
            .steps
            .iter()
            .filter_map(|step| match step {
                MigrationStep::AlterColumn(item)
                    if matches!(item.current.default, Some(dinoco_engine::MigrationDefault::AutoIncrement))
                        != matches!(item.desired.default, Some(dinoco_engine::MigrationDefault::AutoIncrement)) =>
                {
                    Some(format!("`{}.{}`", item.table, item.desired.name))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !identity_changes.is_empty() {
            anyhow::bail!(
                "Adding or removing PostgreSQL identity generation on existing columns requires a reviewed custom migration: {}. Dinoco stopped before emitting invalid ALTER COLUMN SQL.",
                identity_changes.join(", ")
            );
        }
    }

    if !db.is_sqlite() {
        return Ok(());
    }

    let dropped_tables = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::DropTable(item) => Some(item.table.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let unsupported = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::AlterColumn(item) => Some(format!("alter column `{}.{}`", item.table, item.desired.name)),
            MigrationStep::AddColumn(item) if item.column.primary_key || item.column.unique => {
                Some(format!("add constrained column `{}.{}`", item.table, item.column.name))
            }
            MigrationStep::AddColumn(item)
                if !item.column.nullable
                    && matches!(
                        item.column.default,
                        None | Some(dinoco_engine::MigrationDefault::CurrentTimestamp)
                    ) =>
            {
                Some(format!(
                    "add required column `{}.{}` without a SQLite-compatible constant default",
                    item.table, item.column.name
                ))
            }
            MigrationStep::AddForeignKey(item) => {
                Some(format!("add foreign key `{}.{}`", item.table, item.foreign_key.name))
            }
            MigrationStep::DropForeignKey(item) if !dropped_tables.contains(item.table.as_str()) => {
                Some(format!("drop foreign key `{}.{}`", item.table, item.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !unsupported.is_empty() {
        anyhow::bail!(
            "SQLite cannot safely apply the following generated changes in place: {}. Dinoco stopped before writing or recording a no-op migration; use a reviewed table-rebuild migration instead.",
            unsupported.join(", ")
        );
    }

    Ok(())
}

fn mark_unvalidated_legacy_foreign_keys(db: &CliDatabase, plan: &mut MigrationPlan) {
    if !matches!(db, CliDatabase::Postgres(_) | CliDatabase::PgBouncer(_)) {
        return;
    }

    let renamed_tables = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::RenameTable(item) => Some(item.to.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let foreign_keys = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::AddForeignKey(item) if renamed_tables.contains(item.table.as_str()) => {
                Some(format!("{}.{}", item.table, item.foreign_key.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if foreign_keys.is_empty() {
        return;
    }

    plan.warnings.push(crate::sql::MigrationWarning {
        message: format!(
            "Legacy foreign keys will be installed as PostgreSQL NOT VALID constraints to preserve historical rows: {}. They protect new writes immediately; clean any orphan rows and run VALIDATE CONSTRAINT when ready.",
            foreign_keys.join(", ")
        ),
        destructive: false,
    });
}

fn compile_plan(db: &CliDatabase, plan: MigrationPlan) -> Vec<String> {
    let mut create_enums = Vec::new();
    let mut alter_enums = Vec::new();
    let mut drop_enums = Vec::new();
    let mut drop_foreign_keys = Vec::new();
    let mut drop_indexes = Vec::new();
    let mut create_tables = Vec::new();
    let mut rename_tables = Vec::new();
    let mut rename_columns = Vec::new();
    let mut add_columns = Vec::new();
    let mut alter_columns = Vec::new();
    let mut drop_columns = Vec::new();
    let mut add_foreign_keys = Vec::new();
    let mut create_indexes = Vec::new();
    let mut drop_tables = Vec::new();

    for step in plan.steps {
        match step {
            MigrationStep::CreateEnum(item) => create_enums.push(item),
            MigrationStep::AlterEnum(item) => alter_enums.push(item),
            MigrationStep::DropEnum(item) => drop_enums.push(item),
            MigrationStep::DropForeignKey(item) => drop_foreign_keys.push(item),
            MigrationStep::DropIndex(item) => drop_indexes.push(item),
            MigrationStep::CreateTable(mut item) => {
                if !db.is_sqlite() {
                    let table = item.table.clone();
                    add_foreign_keys.extend(item.foreign_keys.drain(..).map(|foreign_key| {
                        dinoco_engine::AddForeignKeyMigration { table: table.clone(), foreign_key }
                    }));
                }
                create_tables.push(item);
            }
            MigrationStep::RenameTable(item) => rename_tables.push(item),
            MigrationStep::RenameColumn(item) => rename_columns.push(item),
            MigrationStep::AddColumn(item) => add_columns.push(item),
            MigrationStep::AlterColumn(item) => alter_columns.push(item),
            MigrationStep::DropColumn(item) => drop_columns.push(item),
            MigrationStep::AddForeignKey(item) => add_foreign_keys.push(item),
            MigrationStep::CreateIndex(item) => create_indexes.push(item),
            MigrationStep::DropTable(item) => drop_tables.push(item),
        }
    }

    let mut statements = Vec::new();
    let renamed_tables = rename_tables.iter().map(|item| item.to.clone()).collect::<BTreeSet<_>>();
    for item in rename_tables {
        statements.extend(db.compile_rename_table_migration(item));
    }
    for item in drop_foreign_keys {
        statements.extend(db.compile_drop_foreign_key_migration(item));
    }
    for item in drop_indexes {
        statements.push(db.compile_drop_index_migration(item));
    }
    for item in create_enums {
        statements.extend(db.compile_create_enum_migration(item));
    }
    for item in alter_enums {
        statements.extend(db.compile_alter_enum_migration(item));
    }
    for item in create_tables {
        statements.push(db.compile_create_table_migration(item));
    }
    for item in rename_columns {
        statements.extend(db.compile_rename_column_migration(item));
    }
    for item in add_columns {
        statements.push(db.compile_add_column_migration(item));
    }
    for item in alter_columns {
        statements.extend(db.compile_alter_column_migration(item));
    }
    for item in drop_columns {
        statements.push(db.compile_drop_column_migration(item));
    }
    for item in create_indexes {
        statements.push(db.compile_create_index_migration(item));
    }
    for item in add_foreign_keys {
        if renamed_tables.contains(item.table.as_str()) {
            statements.extend(db.compile_add_unvalidated_foreign_key_migration(item));
        } else {
            statements.extend(db.compile_add_foreign_key_migration(item));
        }
    }
    for item in drop_tables {
        statements.push(db.compile_drop_table_migration(item));
    }
    for item in drop_enums {
        statements.extend(db.compile_drop_enum_migration(item));
    }
    statements
}

fn preexisting_created_tables(plan: &MigrationPlan, current: &crate::db::DatabaseSchema) -> BTreeSet<String> {
    let existing = current.tables.iter().map(|table| table.name.as_str()).collect::<BTreeSet<_>>();
    plan.steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::CreateTable(item) if existing.contains(item.table.as_str()) => Some(item.table.clone()),
            _ => None,
        })
        .collect()
}

fn compile_down_plan(
    db: &CliDatabase,
    plan: &MigrationPlan,
    migration_name: &str,
    preserved_tables: &BTreeSet<String>,
) -> Vec<String> {
    let irreversible = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::CreateTable(item) if preserved_tables.contains(&item.table) => {
                Some(format!("table `{}` existed before this migration and was only adopted", item.table))
            }
            MigrationStep::DropEnum(item) => Some(format!("enum `{}` was dropped", item.name)),
            MigrationStep::AlterEnum(item) => Some(format!("enum `{}` lost or changed values", item.name)),
            MigrationStep::DropTable(item) => Some(format!("table `{}` was dropped", item.table)),
            MigrationStep::DropColumn(item) => Some(format!("column `{}.{}` was dropped", item.table, item.column)),
            MigrationStep::DropForeignKey(item) => {
                Some(format!("foreign key `{}.{}` was dropped without its old definition", item.table, item.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !irreversible.is_empty() {
        return vec![
            "-- Automatic down migration intentionally not generated: this migration cannot be reversed without risking existing data.".to_string(),
            format!("-- Non-reversible detail(s): {}.", irreversible.join("; ")),
            "-- The dinoco_migrations and checksum records are intentionally retained. Restore a backup or write and review a manual down migration.".to_string(),
        ];
    }

    let migration_name = migration_name.replace('\'', "''");
    let mut statements = Vec::new();
    let created_tables = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::CreateTable(table) => Some(table.table.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if !db.is_sqlite() {
        for step in &plan.steps {
            if let MigrationStep::CreateTable(table) = step {
                for foreign_key in &table.foreign_keys {
                    statements.extend(db.compile_drop_foreign_key_migration(dinoco_engine::DropForeignKeyMigration {
                        table: table.table.clone(),
                        name: foreign_key.name.clone(),
                    }));
                }
            }
        }
    }
    for step in &plan.steps {
        if let MigrationStep::AddForeignKey(migration) = step {
            statements.extend(db.compile_drop_foreign_key_migration(dinoco_engine::DropForeignKeyMigration {
                table: migration.table.clone(),
                name: migration.foreign_key.name.clone(),
            }));
        }
    }
    for table in created_table_drop_order(plan) {
        statements.push(db.compile_drop_table_migration(dinoco_engine::DropTableMigration { table, if_exists: true }));
    }
    for step in plan.steps.iter().rev() {
        match step {
            MigrationStep::CreateEnum(migration) => {
                statements.extend(
                    db.compile_drop_enum_migration(dinoco_engine::DropEnumMigration { name: migration.name.clone() }),
                );
            }
            MigrationStep::DropEnum(migration) => {
                statements.push(format!(
                    "-- Dropping enum `{}` is not reversible without its previous values.",
                    migration.name
                ));
            }
            MigrationStep::AlterEnum(migration) => {
                statements
                    .push(format!("-- Altering enum `{}` is not safely reversible by generated SQL.", migration.name));
            }
            MigrationStep::CreateTable(_) => {}
            MigrationStep::RenameTable(migration) => {
                statements.extend(db.compile_rename_table_migration(dinoco_engine::RenameTableMigration {
                    from: migration.to.clone(),
                    to: migration.from.clone(),
                }));
            }
            MigrationStep::DropTable(migration) => {
                statements.push(format!("-- Dropping table `{}` is not reversible without a backup.", migration.table));
            }
            MigrationStep::AddColumn(migration) => {
                statements.push(db.compile_drop_column_migration(dinoco_engine::DropColumnMigration {
                    table: migration.table.clone(),
                    column: migration.column.name.clone(),
                }));
            }
            MigrationStep::DropColumn(migration) => {
                statements.push(format!(
                    "-- Dropping column `{}.{}` is not reversible without a backup.",
                    migration.table, migration.column
                ));
            }
            MigrationStep::AlterColumn(migration) => {
                statements.extend(db.compile_alter_column_migration(dinoco_engine::AlterColumnMigration {
                    table: migration.table.clone(),
                    current: migration.desired.clone(),
                    desired: migration.current.clone(),
                }));
            }
            MigrationStep::RenameColumn(migration) => {
                statements.extend(db.compile_rename_column_migration(dinoco_engine::RenameColumnMigration {
                    table: migration.table.clone(),
                    from: migration.to.clone(),
                    to: migration.from.clone(),
                }));
            }
            MigrationStep::AddForeignKey(_) => {}
            MigrationStep::DropForeignKey(migration) => {
                statements.push(format!(
                    "-- Dropping foreign key `{}.{}` is not reversible without the previous relation definition.",
                    migration.table, migration.name
                ));
            }
            MigrationStep::CreateIndex(migration) => {
                if !created_tables.contains(migration.table.as_str()) {
                    statements.push(db.compile_drop_index_migration(dinoco_engine::DropIndexMigration {
                        table: migration.table.clone(),
                        index: migration.index.clone(),
                    }));
                }
            }
            MigrationStep::DropIndex(migration) => {
                statements.push(db.compile_create_index_migration(dinoco_engine::CreateIndexMigration {
                    table: migration.table.clone(),
                    index: migration.index.clone(),
                }));
            }
        }
    }

    if !db.is_sqlite() {
        statements.push(format!("DELETE FROM dinoco_migration_schemas WHERE name = '{migration_name}';"));
    }
    statements.push(format!("DELETE FROM dinoco_migration_checksums WHERE name = '{migration_name}';"));
    statements.push(format!("DELETE FROM dinoco_migrations WHERE name = '{migration_name}';"));

    if db.is_sqlite() {
        let mut transactional = vec![
            "PRAGMA foreign_keys = ON;".to_string(),
            "BEGIN IMMEDIATE;".to_string(),
            "PRAGMA defer_foreign_keys = ON;".to_string(),
        ];
        transactional.extend(statements);
        transactional.push("COMMIT;".to_string());
        transactional
    } else {
        statements
    }
}

fn created_table_drop_order(plan: &MigrationPlan) -> Vec<String> {
    let tables = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::CreateTable(table) => Some((table.table.clone(), table)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut incoming = tables.keys().map(|name| (name.clone(), 0usize)).collect::<BTreeMap<_, _>>();
    let mut parents = BTreeMap::<String, BTreeSet<String>>::new();

    for (child, table) in &tables {
        for parent in table
            .foreign_keys
            .iter()
            .map(|foreign_key| &foreign_key.references_table)
            .filter(|parent| *parent != child && tables.contains_key(*parent))
        {
            if parents.entry(child.clone()).or_default().insert(parent.clone()) {
                *incoming.get_mut(parent).expect("created parent table has an incoming counter") += 1;
            }
        }
    }

    let mut ready =
        incoming.iter().filter(|(_, count)| **count == 0).map(|(name, _)| name.clone()).collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(tables.len());
    while let Some(name) = ready.pop_first() {
        ordered.push(name.clone());
        for parent in parents.get(&name).into_iter().flatten() {
            let count = incoming.get_mut(parent).expect("created parent table has an incoming counter");
            *count -= 1;
            if *count == 0 {
                ready.insert(parent.clone());
            }
        }
    }

    let remaining = tables.keys().filter(|name| !ordered.contains(name)).cloned().collect::<Vec<_>>();
    ordered.extend(remaining);
    ordered
}

fn print_plan_summary(plan: &MigrationPlan) {
    ui::info(format!("Detected {} schema change(s).", plan.steps.len()));
    for step in &plan.steps {
        ui::info(format!("  {}", describe_step(step)));
    }

    for warning in &plan.warnings {
        if warning.destructive {
            ui::warning(format!("Destructive change: {}", warning.message));
        } else {
            ui::warning(format!("Potentially unsafe change: {}", warning.message));
        }
    }
}

fn confirm_migration_generation(plans: &[&MigrationPlan]) -> anyhow::Result<bool> {
    if std::env::var("DINOCO_CLI_CONFIRM_MIGRATION").ok().as_deref() == Some("true") {
        return Ok(true);
    }

    let destructive_count = plans.iter().flat_map(|plan| &plan.warnings).filter(|warning| warning.destructive).count();
    if destructive_count > 0 && std::env::var("DINOCO_CLI_CONFIRM_DESTRUCTIVE").ok().as_deref() == Some("true") {
        return Ok(true);
    }

    let message = if destructive_count > 0 {
        format!(
            "This migration contains {destructive_count} destructive change(s) that may permanently delete data. Make sure you have a backup. Generate and apply it now?"
        )
    } else {
        "Generate and apply this migration, then generate the Rust models?".to_string()
    };

    print!("? {message} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("y"))
}

fn confirm_destructive_plan(plan: &MigrationPlan) -> anyhow::Result<bool> {
    if std::env::var("DINOCO_CLI_CONFIRM_DESTRUCTIVE").ok().as_deref() == Some("true") {
        return Ok(true);
    }

    let destructive_count = plan.warnings.iter().filter(|warning| warning.destructive).count();
    Confirm::new(&format!(
        "This repair contains {destructive_count} destructive change(s) that may permanently delete data. Make sure you have a backup. Continue?"
    ))
    .with_default(false)
    .prompt()
    .map_err(Into::into)
}

fn describe_step(step: &MigrationStep) -> String {
    match step {
        MigrationStep::CreateEnum(item) => format!("Create enum `{}`", item.name),
        MigrationStep::DropEnum(item) => format!("Drop enum `{}`", item.name),
        MigrationStep::AlterEnum(item) => format!("Alter enum `{}`", item.name),
        MigrationStep::CreateTable(item) => format!("Create table `{}`", item.table),
        MigrationStep::DropTable(item) => format!("Drop table `{}`", item.table),
        MigrationStep::RenameTable(item) => format!("Rename table `{}` to `{}`", item.from, item.to),
        MigrationStep::AddColumn(item) => format!("Add column `{}.{}`", item.table, item.column.name),
        MigrationStep::DropColumn(item) => format!("Drop column `{}.{}`", item.table, item.column),
        MigrationStep::AlterColumn(item) => format!("Alter column `{}.{}`", item.table, item.desired.name),
        MigrationStep::RenameColumn(item) => format!("Rename column `{}.{}` to `{}`", item.table, item.from, item.to),
        MigrationStep::AddForeignKey(item) => format!("Add foreign key `{}.{}`", item.table, item.foreign_key.name),
        MigrationStep::DropForeignKey(item) => format!("Drop foreign key `{}.{}`", item.table, item.name),
        MigrationStep::CreateIndex(item) => format!("Create index `{}.{}`", item.table, item.index.name),
        MigrationStep::DropIndex(item) => format!("Drop index `{}.{}`", item.table, item.index.name),
    }
}

fn migrations_root(workspace: Option<&str>) -> PathBuf {
    let root = Path::new("dinoco/migrations");
    workspace.map_or_else(|| root.to_path_buf(), |workspace| root.join(workspace))
}

fn migration_dirs(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    fs::read_dir(path)?
        .map(|entry| Ok(entry?.path()))
        .filter(|entry: &anyhow::Result<PathBuf>| entry.as_ref().map(|path| path.is_dir()).unwrap_or(true))
        .collect()
}

fn write_migration_artifacts(directory: &Path, up: &str, down: &str) -> anyhow::Result<()> {
    write_atomic_file(&directory.join("up.sql"), up.as_bytes())?;
    write_atomic_file(&directory.join("down.sql"), down.as_bytes())?;
    OpenOptions::new().read(true).open(directory)?.sync_all().context("failed to sync migration directory")?;
    Ok(())
}

fn publish_migration_artifacts(
    migrations: &Path,
    workspace: Option<&str>,
    name: &str,
    up: &str,
    down: &str,
) -> anyhow::Result<PathBuf> {
    let destination = migrations.join(name);
    let staging_parent = Path::new("dinoco/.migration-staging");
    let staging_root = staging_parent.join(workspace.unwrap_or("__default__"));
    fs::create_dir_all(&staging_root).context("failed to create the migration staging directory")?;
    let staged = staging_root.join(name);
    fs::create_dir(&staged)
        .with_context(|| format!("failed to reserve unique migration staging directory {}", staged.display()))?;

    let publish = (|| -> anyhow::Result<()> {
        write_migration_artifacts(&staged, up, down)?;
        fs::rename(&staged, &destination).with_context(|| {
            format!("failed to atomically publish migration {} to {}", staged.display(), destination.display())
        })?;
        OpenOptions::new()
            .read(true)
            .open(migrations)?
            .sync_all()
            .context("failed to sync the migrations directory")?;
        Ok(())
    })();

    if publish.is_err() {
        let _ = fs::remove_dir_all(&staged);
    }
    let _ = fs::remove_dir(&staging_root);
    let _ = fs::remove_dir(staging_parent);
    publish?;
    Ok(destination)
}

fn write_atomic_file(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let file_name = path.file_name().and_then(|name| name.to_str()).context("invalid migration artifact name")?;
    let temporary = path.with_file_name(format!(".{file_name}.tmp"));
    let write = (|| -> anyhow::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .with_context(|| format!("failed to stage {}", path.display()))?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if write.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write
}

fn statements_sql(statements: &[String]) -> String {
    statements
        .iter()
        .map(|statement| {
            let statement = statement.trim();
            if statement.ends_with(';') { statement.to_string() } else { format!("{statement};") }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn migration_name(name: &str) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{nanos}_{}_{name}", std::process::id())
}

fn split_sql(sql: &str) -> anyhow::Result<Vec<String>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Normal,
        SingleQuote,
        DoubleQuote,
        Backtick,
        Bracket,
        LineComment,
        BlockComment,
        DollarQuote,
    }

    let bytes = sql.as_bytes();
    let mut statements = Vec::new();
    let mut state = State::Normal;
    let mut dollar_delimiter = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        match state {
            State::Normal => match (byte, next) {
                (b'-', Some(b'-')) => {
                    state = State::LineComment;
                    index += 2;
                }
                (b'/', Some(b'*')) => {
                    state = State::BlockComment;
                    index += 2;
                }
                (b'\'', _) => {
                    state = State::SingleQuote;
                    index += 1;
                }
                (b'"', _) => {
                    state = State::DoubleQuote;
                    index += 1;
                }
                (b'`', _) => {
                    state = State::Backtick;
                    index += 1;
                }
                (b'[', _) => {
                    state = State::Bracket;
                    index += 1;
                }
                (b'$', _) => {
                    if let Some(end) = dollar_quote_delimiter_end(bytes, index) {
                        dollar_delimiter = bytes[index..end].to_vec();
                        state = State::DollarQuote;
                        index = end;
                    } else {
                        index += 1;
                    }
                }
                (b';', _) => {
                    if !sql[start..index].trim().is_empty() {
                        statements.push(sql[start..index].trim().to_string());
                    }
                    start = index + 1;
                    index += 1;
                }
                _ => index += 1,
            },
            State::LineComment => {
                index += 1;
                if byte == b'\n' || byte == b'\r' {
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                if byte == b'*' && next == Some(b'/') {
                    state = State::Normal;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            State::DollarQuote => {
                if bytes[index..].starts_with(&dollar_delimiter) {
                    index += dollar_delimiter.len();
                    state = State::Normal;
                } else {
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
                if byte == b'\\' && state != State::Bracket {
                    index = (index + 2).min(bytes.len());
                } else {
                    index += 1;
                    if byte == closing {
                        if bytes.get(index) == Some(&closing) {
                            index += 1;
                        } else {
                            state = State::Normal;
                        }
                    }
                }
            }
        }
    }
    if matches!(
        state,
        State::SingleQuote
            | State::DoubleQuote
            | State::Backtick
            | State::Bracket
            | State::BlockComment
            | State::DollarQuote
    ) {
        anyhow::bail!("Migration SQL contains an unterminated quote or block comment.");
    }
    if !sql[start..].trim().is_empty() {
        statements.push(sql[start..].trim().to_string());
    }
    Ok(statements)
}

fn dollar_quote_delimiter_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    while let Some(byte) = bytes.get(index).copied() {
        if byte == b'$' {
            return Some(index + 1);
        }
        if !(byte == b'_' || byte.is_ascii_alphanumeric()) {
            return None;
        }
        index += 1;
    }
    None
}

fn validate_server_migration_sql(sql: &str) -> anyhow::Result<()> {
    for statement in split_sql(sql)? {
        let surface = sql_control_surface(&statement);
        let normalized = surface.split_whitespace().collect::<Vec<_>>().join(" ");
        let tokens = surface
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        let forbidden_context = [
            "begin",
            "start transaction",
            "commit",
            "rollback",
            "savepoint",
            "release savepoint",
            "use",
            "set search_path",
            "set schema",
            "set foreign_key_checks",
            "set autocommit",
            "delimiter",
        ];
        let token_context_control = matches!(
            tokens.as_slice(),
            ["begin", ..]
                | ["commit", ..]
                | ["rollback", ..]
                | ["savepoint", ..]
                | ["release", "savepoint", ..]
                | ["start", "transaction", ..]
                | ["use", ..]
                | ["delimiter", ..]
                | ["set", ..]
        );
        if token_context_control
            || forbidden_context
                .iter()
                .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix} ")))
        {
            anyhow::bail!(
                "Migration SQL may not control transactions, switch databases/schemas, disable integrity checks, or use client DELIMITER directives: `{}`.",
                statement.lines().next().unwrap_or_default().trim()
            );
        }
        let creates_dynamic_sql = tokens.first() == Some(&"create")
            && tokens.iter().any(|token| matches!(*token, "function" | "procedure" | "trigger" | "event"));
        if tokens.first().is_some_and(|token| matches!(*token, "do" | "call" | "prepare" | "execute"))
            || creates_dynamic_sql
        {
            anyhow::bail!(
                "Migration SQL may not define or invoke dynamic/procedural SQL because its effects cannot be validated safely: `{}`. Use explicit reviewable DDL/DML statements.",
                statement.lines().next().unwrap_or_default().trim()
            );
        }
        if [
            "dinoco_migrations",
            "dinoco_migration_checksums",
            "dinoco_migration_schemas",
            "dinoco_migrations_checksum_required",
            "dinoco_migrations_schema_snapshots_required",
            "get_lock",
            "release_lock",
            "pg_advisory_lock",
            "pg_advisory_xact_lock",
            "pg_advisory_unlock",
        ]
        .iter()
        .any(|reserved| {
            surface
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .any(|token| token == *reserved)
        }) {
            anyhow::bail!("Migration SQL may not read or mutate Dinoco history metadata or migration locks.");
        }
    }
    Ok(())
}

fn sql_control_surface(sql: &str) -> String {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Normal,
        SingleQuote,
        LineComment,
        BlockComment,
        DollarQuote,
    }
    let bytes = sql.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut state = State::Normal;
    let mut dollar_delimiter = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        match state {
            State::Normal => match (byte, next) {
                (b'-', Some(b'-')) => {
                    output.extend_from_slice(b"  ");
                    index += 2;
                    state = State::LineComment;
                }
                (b'/', Some(b'*')) => {
                    output.extend_from_slice(b"  ");
                    index += 2;
                    state = State::BlockComment;
                }
                (b'\'', _) => {
                    output.push(b' ');
                    index += 1;
                    state = State::SingleQuote;
                }
                (b'$', _) => {
                    if let Some(end) = dollar_quote_delimiter_end(bytes, index) {
                        output.resize(output.len() + end - index, b' ');
                        dollar_delimiter = bytes[index..end].to_vec();
                        state = State::DollarQuote;
                        index = end;
                    } else {
                        output.push(byte.to_ascii_lowercase());
                        index += 1;
                    }
                }
                _ => {
                    output.push(byte.to_ascii_lowercase());
                    index += 1;
                }
            },
            State::SingleQuote => {
                output.push(b' ');
                if byte == b'\\' {
                    index += 1;
                    if index < bytes.len() {
                        output.push(b' ');
                        index += 1;
                    }
                } else {
                    index += 1;
                    if byte == b'\'' {
                        if bytes.get(index) == Some(&b'\'') {
                            output.push(b' ');
                            index += 1;
                        } else {
                            state = State::Normal;
                        }
                    }
                }
            }
            State::LineComment => {
                output.push(if byte == b'\n' || byte == b'\r' { byte } else { b' ' });
                index += 1;
                if byte == b'\n' || byte == b'\r' {
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                output.push(b' ');
                if byte == b'*' && next == Some(b'/') {
                    output.push(b' ');
                    index += 2;
                    state = State::Normal;
                } else {
                    index += 1;
                }
            }
            State::DollarQuote => {
                if bytes[index..].starts_with(&dollar_delimiter) {
                    output.resize(output.len() + dollar_delimiter.len(), b' ');
                    index += dollar_delimiter.len();
                    state = State::Normal;
                } else {
                    output.push(b' ');
                    index += 1;
                }
            }
        }
    }
    String::from_utf8(output).expect("SQL control surface preserves UTF-8 bytes").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let migration =
            ValidatedMigration { execution_sql: recovered, checksum: "unused".to_string(), generated: true };
        assert!(is_generated_legacy_normalization_migration(&migration));
    }
}
