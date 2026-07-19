use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use inquire::Confirm;

use crate::db::CliDatabase;
use crate::schema::{Database, RuntimeConfig, read_schema, runtime_config};
use crate::sql::{MigrationPlan, MigrationStep, generate_create_table_migrations, plan_database_migration};
use crate::ui;

const MIGRATION_TEST_PREFIX: &str = "dinoco_migration_test_";

pub async fn generate() -> anyhow::Result<()> {
    let (_, schema) = read_schema()?;
    let config = runtime_config(&schema)?;
    let db = CliDatabase::connect(&config).await?;

    if config.database != Database::Sqlite {
        cleanup_migration_test_schema(&db).await?;
    }

    if !db.migrations_table_exists().await? && db.database_has_user_tables().await? {
        let confirm = Confirm::new("The database already has tables but no dinoco_migrations table. Continue anyway?")
            .with_default(false)
            .prompt()?;
        if !confirm {
            ui::warning("Migration generation cancelled.");
            return Ok(());
        }
    }

    fs::create_dir_all("dinoco/migrations")?;

    let current = db.inspect_schema().await?;
    let desired = inspect_shadow_schema(&db, &config, &schema).await?;
    let plan = plan_database_migration(&desired, &current);
    if plan.steps.is_empty() {
        ui::info("No schema changes were found.");
        dinoco_codegen::generate_models(&schema)?;
        ui::success("Rust models generated at dinoco/models/");
        return Ok(());
    }

    print_plan_summary(&plan);
    if plan.warnings.iter().any(|warning| warning.destructive) && !confirm_destructive_plan(&plan)? {
        ui::warning("Migration generation cancelled.");
        return Ok(());
    }

    let migration_name = migration_name("generated");
    let down_statements = compile_down_plan(&db, &plan, &migration_name);
    let planned_statements = compile_plan(&db, plan);

    let migration_dir = Path::new("dinoco/migrations").join(&migration_name);
    fs::create_dir_all(&migration_dir)?;

    let mut up_statements = vec![db.compile_create_migrations_table()];
    up_statements.extend(planned_statements);
    up_statements.push(db.compile_insert_migration_record(&migration_name));

    fs::write(migration_dir.join("up.sql"), up_statements.join("\n\n"))?;
    fs::write(migration_dir.join("down.sql"), down_statements.join("\n\n"))?;

    apply_statements(&db, &up_statements).await?;
    dinoco_codegen::generate_models(&schema)?;

    ui::success(format!("Migration generated and applied: {}", migration_dir.display()));
    ui::success("Rust models generated at dinoco/models/");

    Ok(())
}

async fn inspect_shadow_schema(
    primary: &CliDatabase,
    config: &RuntimeConfig,
    schema: &dinoco_compiler::Schema,
) -> anyhow::Result<crate::db::DatabaseSchema> {
    if config.database != Database::Sqlite {
        return inspect_migration_test_schema(primary, schema).await;
    }

    let mut shadow_config = config.clone();
    shadow_config.database_url =
        format!("/private/tmp/dinoco-shadow-{}-{}.sqlite", std::process::id(), current_millis());

    let shadow = CliDatabase::connect(&shadow_config).await?;
    reset_shadow_database(&shadow).await?;
    apply_desired_schema(&shadow, schema).await?;
    let inspected = shadow.inspect_schema().await?;

    if config.database == Database::Sqlite {
        let _ = std::fs::remove_file(&shadow_config.database_url);
    }

    Ok(inspected)
}

async fn inspect_migration_test_schema(
    db: &CliDatabase,
    schema: &dinoco_compiler::Schema,
) -> anyhow::Result<crate::db::DatabaseSchema> {
    cleanup_migration_test_schema(db).await?;

    let inspection = async {
        apply_migration_test_schema(db, schema).await?;
        Ok(normalize_migration_test_schema(db.inspect_schema().await?))
    }
    .await;
    let cleanup = cleanup_migration_test_schema(db).await;

    match (inspection, cleanup) {
        (Ok(schema), Ok(())) => Ok(schema),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error.context("failed to clean migration test tables")),
        (Err(error), Err(cleanup_error)) => {
            Err(error.context(format!("migration test cleanup also failed: {cleanup_error:#}")))
        }
    }
}

async fn apply_migration_test_schema(db: &CliDatabase, schema: &dinoco_compiler::Schema) -> anyhow::Result<()> {
    for item in schema.enums() {
        for statement in db.compile_create_enum_migration(dinoco_engine::CreateEnumMigration {
            name: migration_test_name(&item.name),
            values: item.values.clone(),
        }) {
            db.execute(&statement).await?;
        }
    }

    let mut foreign_keys = Vec::new();
    for migration in generate_create_table_migrations(schema) {
        let mut migration = namespace_create_table(migration);
        let table = migration.table.clone();
        foreign_keys.extend(
            migration
                .foreign_keys
                .drain(..)
                .map(|foreign_key| dinoco_engine::AddForeignKeyMigration { table: table.clone(), foreign_key }),
        );
        db.execute(&db.compile_create_table_migration(migration)).await?;
    }
    for migration in foreign_keys {
        for statement in db.compile_add_foreign_key_migration(migration) {
            db.execute(&statement).await?;
        }
    }

    Ok(())
}

async fn cleanup_migration_test_schema(db: &CliDatabase) -> anyhow::Result<()> {
    let schema = db.inspect_schema().await?;
    let test_tables =
        schema.tables.iter().filter(|table| table.name.starts_with(MIGRATION_TEST_PREFIX)).collect::<Vec<_>>();

    for table in &test_tables {
        for foreign_key in &table.foreign_keys {
            for statement in db.compile_drop_foreign_key_migration(dinoco_engine::DropForeignKeyMigration {
                table: table.name.clone(),
                name: foreign_key.name.clone(),
            }) {
                db.execute(&statement).await?;
            }
        }
    }
    for table in test_tables.into_iter().rev() {
        db.execute(&db.compile_drop_table_migration(dinoco_engine::DropTableMigration {
            table: table.name.clone(),
            if_exists: true,
        }))
        .await?;
    }
    for item in schema.enums.iter().filter(|item| item.name.starts_with(MIGRATION_TEST_PREFIX)) {
        for statement in db.compile_drop_enum_migration(dinoco_engine::DropEnumMigration { name: item.name.clone() }) {
            db.execute(&statement).await?;
        }
    }

    Ok(())
}

fn namespace_create_table(mut migration: dinoco_engine::CreateTableMigration) -> dinoco_engine::CreateTableMigration {
    migration.table = migration_test_name(&migration.table);
    for column in &mut migration.columns {
        if let dinoco_engine::MigrationColumnType::Enum { name, .. } = &mut column.ty {
            *name = migration_test_name(name);
        }
    }
    for foreign_key in &mut migration.foreign_keys {
        foreign_key.name = migration_test_name(&foreign_key.name);
        foreign_key.references_table = migration_test_name(&foreign_key.references_table);
    }
    migration
}

fn normalize_migration_test_schema(mut schema: crate::db::DatabaseSchema) -> crate::db::DatabaseSchema {
    schema.tables.retain(|table| table.name.starts_with(MIGRATION_TEST_PREFIX));
    schema.enums.retain(|item| item.name.starts_with(MIGRATION_TEST_PREFIX));

    for table in &mut schema.tables {
        table.name = remove_migration_test_prefix(&table.name);
        table.row_count = 0;
        for column in &mut table.columns {
            if let dinoco_engine::MigrationColumnType::Enum { name, .. } = &mut column.ty {
                *name = remove_migration_test_prefix(name);
            }
        }
        for foreign_key in &mut table.foreign_keys {
            foreign_key.name = remove_migration_test_prefix(&foreign_key.name);
            foreign_key.references_table = remove_migration_test_prefix(&foreign_key.references_table);
        }
    }
    for item in &mut schema.enums {
        item.name = remove_migration_test_prefix(&item.name);
    }

    schema
}

fn migration_test_name(name: &str) -> String {
    format!("{MIGRATION_TEST_PREFIX}{name}")
}

fn remove_migration_test_prefix(name: &str) -> String {
    name.strip_prefix(MIGRATION_TEST_PREFIX).unwrap_or(name).to_string()
}

async fn reset_shadow_database(db: &CliDatabase) -> anyhow::Result<()> {
    let schema = db.inspect_schema().await?;
    for table in &schema.tables {
        for foreign_key in &table.foreign_keys {
            for statement in db.compile_drop_foreign_key_migration(dinoco_engine::DropForeignKeyMigration {
                table: table.name.clone(),
                name: foreign_key.name.clone(),
            }) {
                db.execute(&statement).await?;
            }
        }
    }
    for table in schema.tables.into_iter().rev() {
        db.execute(
            &db.compile_drop_table_migration(dinoco_engine::DropTableMigration { table: table.name, if_exists: true }),
        )
        .await?;
    }
    for item in schema.enums {
        for statement in db.compile_drop_enum_migration(dinoco_engine::DropEnumMigration { name: item.name }) {
            db.execute(&statement).await?;
        }
    }
    Ok(())
}

async fn apply_desired_schema(db: &CliDatabase, schema: &dinoco_compiler::Schema) -> anyhow::Result<()> {
    for item in schema.enums() {
        for statement in db.compile_create_enum_migration(dinoco_engine::CreateEnumMigration {
            name: item.name.clone(),
            values: item.values.clone(),
        }) {
            db.execute(&statement).await?;
        }
    }

    for migration in generate_create_table_migrations(schema) {
        db.execute(&db.compile_create_table_migration(migration)).await?;
    }

    Ok(())
}

pub async fn run() -> anyhow::Result<()> {
    let (_, schema) = read_schema()?;
    let config = runtime_config(&schema)?;
    let db = CliDatabase::connect(&config).await?;

    let mut migrations = migration_dirs()?;
    migrations.sort();

    if migrations.is_empty() {
        ui::info("No migrations were found.");
        return Ok(());
    }

    db.execute(&db.compile_create_migrations_table()).await?;

    for migration in migrations {
        let name = migration.file_name().and_then(|name| name.to_str()).context("invalid migration directory name")?;
        if db.migration_applied(name).await? {
            ui::info(format!("Skipping already applied migration: {name}"));
            continue;
        }

        let sql_path = migration.join("up.sql");
        let sql = fs::read_to_string(&sql_path).with_context(|| format!("failed to read {}", sql_path.display()))?;
        for statement in split_sql(&sql) {
            let statement = statement.trim();
            if statement.is_empty() || statement.starts_with("--") {
                continue;
            }
            db.execute(statement).await?;
        }
        ui::success(format!("Migration applied: {}", migration.display()));
    }

    ui::success("All pending migrations were applied.");

    Ok(())
}

async fn apply_statements(db: &CliDatabase, statements: &[String]) -> anyhow::Result<()> {
    for statement in statements {
        let statement = statement.trim();
        if statement.is_empty() || statement.starts_with("--") {
            continue;
        }
        db.execute(statement).await?;
    }
    Ok(())
}

fn compile_plan(db: &CliDatabase, plan: MigrationPlan) -> Vec<String> {
    let mut statements = Vec::new();
    for step in plan.steps {
        match step {
            MigrationStep::CreateEnum(migration) => statements.extend(db.compile_create_enum_migration(migration)),
            MigrationStep::DropEnum(migration) => statements.extend(db.compile_drop_enum_migration(migration)),
            MigrationStep::AlterEnum(migration) => statements.extend(db.compile_alter_enum_migration(migration)),
            MigrationStep::CreateTable(migration) => statements.push(db.compile_create_table_migration(migration)),
            MigrationStep::DropTable(migration) => statements.push(db.compile_drop_table_migration(migration)),
            MigrationStep::AddColumn(migration) => statements.push(db.compile_add_column_migration(migration)),
            MigrationStep::DropColumn(migration) => statements.push(db.compile_drop_column_migration(migration)),
            MigrationStep::AlterColumn(migration) => statements.extend(db.compile_alter_column_migration(migration)),
            MigrationStep::RenameColumn(migration) => statements.extend(db.compile_rename_column_migration(migration)),
            MigrationStep::AddForeignKey(migration) => {
                statements.extend(db.compile_add_foreign_key_migration(migration))
            }
            MigrationStep::DropForeignKey(migration) => {
                statements.extend(db.compile_drop_foreign_key_migration(migration))
            }
        }
    }
    statements
}

fn compile_down_plan(db: &CliDatabase, plan: &MigrationPlan, migration_name: &str) -> Vec<String> {
    let mut statements = vec![format!("DELETE FROM dinoco_migrations WHERE name = '{migration_name}';")];

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
            MigrationStep::CreateTable(migration) => {
                statements.push(db.compile_drop_table_migration(dinoco_engine::DropTableMigration {
                    table: migration.table.clone(),
                    if_exists: true,
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
            MigrationStep::AddForeignKey(migration) => {
                statements.extend(db.compile_drop_foreign_key_migration(dinoco_engine::DropForeignKeyMigration {
                    table: migration.table.clone(),
                    name: migration.foreign_key.name.clone(),
                }));
            }
            MigrationStep::DropForeignKey(migration) => {
                statements.push(format!(
                    "-- Dropping foreign key `{}.{}` is not reversible without the previous relation definition.",
                    migration.table, migration.name
                ));
            }
        }
    }

    statements
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

fn confirm_destructive_plan(plan: &MigrationPlan) -> anyhow::Result<bool> {
    if std::env::var("DINOCO_CLI_CONFIRM_DESTRUCTIVE").ok().as_deref() == Some("true") {
        return Ok(true);
    }

    let destructive_count = plan.warnings.iter().filter(|warning| warning.destructive).count();
    Confirm::new(&format!(
        "This migration contains {destructive_count} destructive change(s) that may permanently delete data. Make sure you have a backup. Continue?"
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
        MigrationStep::AddColumn(item) => format!("Add column `{}.{}`", item.table, item.column.name),
        MigrationStep::DropColumn(item) => format!("Drop column `{}.{}`", item.table, item.column),
        MigrationStep::AlterColumn(item) => format!("Alter column `{}.{}`", item.table, item.desired.name),
        MigrationStep::RenameColumn(item) => format!("Rename column `{}.{}` to `{}`", item.table, item.from, item.to),
        MigrationStep::AddForeignKey(item) => format!("Add foreign key `{}.{}`", item.table, item.foreign_key.name),
        MigrationStep::DropForeignKey(item) => format!("Drop foreign key `{}.{}`", item.table, item.name),
    }
}

fn migration_dirs() -> anyhow::Result<Vec<PathBuf>> {
    let path = Path::new("dinoco/migrations");
    if !path.exists() {
        return Ok(Vec::new());
    }

    fs::read_dir(path)?
        .map(|entry| Ok(entry?.path()))
        .filter(|entry: &anyhow::Result<PathBuf>| entry.as_ref().map(|path| path.is_dir()).unwrap_or(true))
        .collect()
}

fn migration_name(name: &str) -> String {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    format!("{millis}_{name}")
}

fn current_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn split_sql(sql: &str) -> impl Iterator<Item = String> {
    let cleaned = sql.lines().filter(|line| !line.trim_start().starts_with("--")).collect::<Vec<_>>().join("\n");
    let statements = cleaned
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    statements.into_iter()
}

#[cfg(test)]
mod tests {
    use dinoco_engine::{MigrationColumnType, MigrationForeignKey, ReferentialAction};

    use super::*;
    use crate::db::{DatabaseEnum, DatabaseSchema, DatabaseTable};

    #[test]
    fn migration_test_schema_names_are_isolated_and_normalized() {
        let migration = dinoco_engine::CreateTableMigration {
            table: "user_token".to_string(),
            if_not_exists: true,
            columns: vec![dinoco_engine::MigrationColumn {
                name: "role".to_string(),
                ty: MigrationColumnType::Enum {
                    name: "role".to_string(),
                    values: vec!["USER".to_string(), "ADMIN".to_string()],
                },
                primary_key: false,
                nullable: false,
                default: None,
            }],
            foreign_keys: vec![MigrationForeignKey {
                name: "fk_user_token_user_id".to_string(),
                columns: vec!["user_id".to_string()],
                references_table: "user".to_string(),
                references_columns: vec!["id".to_string()],
                on_update: ReferentialAction::Cascade,
                on_delete: ReferentialAction::Cascade,
            }],
        };

        let namespaced = namespace_create_table(migration);
        assert_eq!(namespaced.table, "dinoco_migration_test_user_token");
        assert_eq!(namespaced.foreign_keys[0].name, "dinoco_migration_test_fk_user_token_user_id");
        assert_eq!(namespaced.foreign_keys[0].references_table, "dinoco_migration_test_user");
        assert!(matches!(
            &namespaced.columns[0].ty,
            MigrationColumnType::Enum { name, .. } if name == "dinoco_migration_test_role"
        ));

        let normalized = normalize_migration_test_schema(DatabaseSchema {
            tables: vec![
                DatabaseTable {
                    name: namespaced.table,
                    row_count: 0,
                    columns: namespaced.columns,
                    foreign_keys: namespaced.foreign_keys,
                },
                DatabaseTable {
                    name: "application_table".to_string(),
                    row_count: 3,
                    columns: Vec::new(),
                    foreign_keys: Vec::new(),
                },
            ],
            enums: vec![
                DatabaseEnum {
                    name: "dinoco_migration_test_role".to_string(),
                    values: vec!["USER".to_string(), "ADMIN".to_string()],
                },
                DatabaseEnum { name: "application_enum".to_string(), values: vec!["VALUE".to_string()] },
            ],
        });

        assert_eq!(normalized.tables.len(), 1);
        assert_eq!(normalized.tables[0].name, "user_token");
        assert_eq!(normalized.tables[0].foreign_keys[0].name, "fk_user_token_user_id");
        assert_eq!(normalized.tables[0].foreign_keys[0].references_table, "user");
        assert_eq!(normalized.enums.len(), 1);
        assert_eq!(normalized.enums[0].name, "role");
    }
}
