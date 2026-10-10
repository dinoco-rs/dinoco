use std::collections::{BTreeMap, BTreeSet};

use dinoco_compiler::{AttributeArgument, AttributeValue, ConfigValue, Model, ModelField, Schema};
use dinoco_engine::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CreateEnumMigration,
    CreateIndexMigration, CreateTableMigration, DropColumnMigration, DropEnumMigration, DropForeignKeyMigration,
    DropIndexMigration, DropTableMigration, MigrationColumn, MigrationColumnType, MigrationDefault,
    MigrationForeignKey, MigrationIndex, MigrationIndexKind, ReferentialAction, RenameColumnMigration,
    RenameTableMigration,
};

use crate::db::{DatabaseEnum, DatabaseSchema, DatabaseTable};

#[derive(Debug, Clone, Default)]
pub struct MigrationPlan {
    pub steps: Vec<MigrationStep>,
    pub warnings: Vec<MigrationWarning>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum MigrationStep {
    CreateEnum(CreateEnumMigration),
    DropEnum(DropEnumMigration),
    AlterEnum(AlterEnumMigration),
    CreateTable(CreateTableMigration),
    DropTable(DropTableMigration),
    RenameTable(RenameTableMigration),
    AddColumn(AddColumnMigration),
    DropColumn(DropColumnMigration),
    AlterColumn(AlterColumnMigration),
    RenameColumn(RenameColumnMigration),
    AddForeignKey(AddForeignKeyMigration),
    DropForeignKey(DropForeignKeyMigration),
    CreateIndex(CreateIndexMigration),
    DropIndex(DropIndexMigration),
    RebuildTable(SqliteTableRebuild),
}

#[derive(Debug, Clone)]
pub struct SqliteTableRebuild {
    pub current: DatabaseTable,
    pub desired: DatabaseTable,
    pub column_mappings: Vec<SqliteColumnMapping>,
    pub foreign_key_checks: Vec<SqliteForeignKeyCheck>,
    pub changes: Vec<MigrationStep>,
}

#[derive(Debug, Clone)]
pub struct SqliteColumnMapping {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct SqliteForeignKeyCheck {
    pub table: String,
    pub foreign_key: MigrationForeignKey,
}

#[derive(Debug, Clone)]
pub struct MigrationWarning {
    pub message: String,
    pub destructive: bool,
}

pub fn generate_create_table_migrations(schema: &Schema) -> Vec<CreateTableMigration> {
    let mut migrations = schema
        .models()
        .map(|model| {
            let relation_unique_columns = relation_unique_columns(model, schema);
            let columns = model
                .fields
                .iter()
                .filter(|field| !field.is_relation(schema))
                .map(|field| {
                    let mut column = migration_column(model, field, schema);
                    column.unique |= relation_unique_columns.contains(field.name.as_str());
                    column
                })
                .collect();

            CreateTableMigration {
                table: model_table_name(model),
                if_not_exists: true,
                columns,
                foreign_keys: relation_foreign_keys(&model.name, schema),
            }
        })
        .collect::<Vec<_>>();

    migrations.extend(generate_many_to_many_join_migrations(schema));
    migrations
}

pub fn plan_schema_migration(schema: &Schema, current: &DatabaseSchema) -> MigrationPlan {
    let desired = desired_database_schema(schema);
    plan_database_migration(&desired, current)
}

pub fn plan_database_migration(desired: &DatabaseSchema, current: &DatabaseSchema) -> MigrationPlan {
    let mut plan = MigrationPlan::default();
    let (current, table_renames) = normalize_legacy_table_names(desired, current);
    plan.steps.extend(table_renames.into_iter().map(MigrationStep::RenameTable));
    let (current, detected_renames) = detect_table_renames(&mut plan, desired, &current);
    plan.steps.extend(detected_renames.into_iter().map(MigrationStep::RenameTable));
    let current_tables = current.tables.iter().map(|table| (table.name.as_str(), table)).collect::<BTreeMap<_, _>>();
    let desired_tables = desired.tables.iter().map(|table| (table.name.as_str(), table)).collect::<BTreeMap<_, _>>();
    let current_enums = current.enums.iter().map(|item| (item.name.as_str(), item)).collect::<BTreeMap<_, _>>();
    let desired_enums = desired.enums.iter().map(|item| (item.name.as_str(), item)).collect::<BTreeMap<_, _>>();

    for (name, item) in &desired_enums {
        match current_enums.get(name) {
            None => plan.steps.push(MigrationStep::CreateEnum(CreateEnumMigration {
                name: item.name.clone(),
                values: item.values.clone(),
            })),
            Some(current) if current.values != item.values => {
                let removed = current.values.iter().filter(|value| !item.values.contains(value)).collect::<Vec<_>>();
                if !removed.is_empty() {
                    plan.warnings.push(MigrationWarning {
                        message: format!(
                            "Enum `{}` removes values: {}. Existing rows may become invalid.",
                            item.name,
                            removed.into_iter().map(|value| value.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                        destructive: true,
                    });
                }
                plan.steps.push(MigrationStep::AlterEnum(AlterEnumMigration {
                    name: item.name.clone(),
                    current_values: current.values.clone(),
                    desired_values: item.values.clone(),
                }));
            }
            _ => {}
        }
    }

    for (name, item) in &current_enums {
        if !desired_enums.contains_key(name) {
            plan.warnings.push(MigrationWarning {
                message: format!("Enum `{}` will be dropped.", item.name),
                destructive: true,
            });
            plan.steps.push(MigrationStep::DropEnum(DropEnumMigration { name: item.name.clone() }));
        }
    }

    for (name, desired_table) in &desired_tables {
        let Some(current_table) = current_tables.get(name) else {
            plan.steps.push(MigrationStep::CreateTable(CreateTableMigration {
                table: desired_table.name.clone(),
                if_not_exists: false,
                columns: desired_table.columns.clone(),
                foreign_keys: desired_table.foreign_keys.clone(),
            }));
            plan.steps.extend(
                desired_table.indexes.iter().filter(|index| !index_is_primary_key(index, desired_table)).cloned().map(
                    |index| {
                        MigrationStep::CreateIndex(CreateIndexMigration { table: desired_table.name.clone(), index })
                    },
                ),
            );
            continue;
        };

        diff_columns(&mut plan, current_table, desired_table);
        diff_foreign_keys(&mut plan, current_table, desired_table);
        diff_indexes(&mut plan, current_table, desired_table);
    }

    let dropped_tables =
        current_tables.keys().filter(|name| !desired_tables.contains_key(*name)).copied().collect::<BTreeSet<_>>();
    for name in &dropped_tables {
        let current_table = current_tables.get(name).expect("dropped table exists in current schema");
        for foreign_key in &current_table.foreign_keys {
            plan.steps.push(MigrationStep::DropForeignKey(DropForeignKeyMigration {
                table: current_table.name.clone(),
                name: foreign_key.name.clone(),
            }));
        }
    }
    for name in dropped_table_order(&current_tables, &dropped_tables) {
        let current_table = current_tables.get(name.as_str()).expect("ordered dropped table exists");
        plan.warnings.push(MigrationWarning {
            message: format!(
                "Table `{}` with {} row(s) will be dropped. Its schema and any data it contains cannot be recovered from this migration.",
                current_table.name, current_table.row_count
            ),
            destructive: true,
        });
        plan.steps
            .push(MigrationStep::DropTable(DropTableMigration { table: current_table.name.clone(), if_exists: false }));
    }

    plan
}

/// Produces the regular schema diff and lowers table changes that SQLite cannot
/// express safely with `ALTER TABLE` into explicit, data-preserving rebuilds.
pub fn plan_sqlite_database_migration(desired: &DatabaseSchema, current: &DatabaseSchema) -> MigrationPlan {
    let mut plan = plan_database_migration(desired, current);
    let (current, _) = normalize_legacy_table_names(desired, current);
    let dropped_tables = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::DropTable(item) => Some(item.table.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let rebuild_tables = plan
        .steps
        .iter()
        .filter_map(sqlite_rebuild_table)
        .filter(|table| !dropped_tables.contains(*table))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if rebuild_tables.is_empty() {
        return plan;
    }

    let current_tables = current.tables.iter().map(|table| (table.name.as_str(), table)).collect::<BTreeMap<_, _>>();
    let desired_tables = desired.tables.iter().map(|table| (table.name.as_str(), table)).collect::<BTreeMap<_, _>>();
    let renames = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::RenameColumn(item) => Some(((item.table.clone(), item.to.clone()), item.from.clone())),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let foreign_key_checks = desired
        .tables
        .iter()
        .flat_map(|table| {
            table.foreign_keys.iter().filter_map(|foreign_key| {
                (rebuild_tables.contains(table.name.as_str())
                    || rebuild_tables.contains(foreign_key.references_table.as_str()))
                .then(|| SqliteForeignKeyCheck { table: table.name.clone(), foreign_key: foreign_key.clone() })
            })
        })
        .collect::<Vec<_>>();
    let rebuild_changes = rebuild_tables
        .iter()
        .map(|table| {
            let changes = plan
                .steps
                .iter()
                .filter(|step| {
                    step_table(step).is_some_and(|step_table| step_table == table)
                        && !matches!(step, MigrationStep::RenameTable(_))
                })
                .cloned()
                .collect::<Vec<_>>();
            (table.clone(), changes)
        })
        .collect::<BTreeMap<_, _>>();

    plan.steps.retain(|step| {
        step_table(step)
            .is_none_or(|table| !rebuild_tables.contains(table) || matches!(step, MigrationStep::RenameTable(_)))
    });

    for table in rebuild_tables {
        let current_table = current_tables.get(table.as_str()).expect("a rebuilt table exists in the current schema");
        let desired_table = desired_tables.get(table.as_str()).expect("a rebuilt table exists in the desired schema");
        let current_columns =
            current_table.columns.iter().map(|column| (column.name.as_str(), column)).collect::<BTreeMap<_, _>>();
        let column_mappings = desired_table
            .columns
            .iter()
            .filter_map(|column| {
                let source = renames
                    .get(&(table.clone(), column.name.clone()))
                    .map(String::as_str)
                    .unwrap_or(column.name.as_str());
                current_columns
                    .contains_key(source)
                    .then(|| SqliteColumnMapping { from: source.to_string(), to: column.name.clone() })
            })
            .collect();
        plan.steps.push(MigrationStep::RebuildTable(SqliteTableRebuild {
            current: (*current_table).clone(),
            desired: (*desired_table).clone(),
            column_mappings,
            foreign_key_checks: foreign_key_checks.clone(),
            changes: rebuild_changes.get(&table).cloned().unwrap_or_default(),
        }));
    }

    plan
}

fn sqlite_rebuild_table(step: &MigrationStep) -> Option<&str> {
    match step {
        MigrationStep::AlterColumn(item) => Some(&item.table),
        MigrationStep::DropColumn(item) => Some(&item.table),
        MigrationStep::AddForeignKey(item) => Some(&item.table),
        MigrationStep::DropForeignKey(item) => Some(&item.table),
        MigrationStep::AddColumn(item)
            if item.column.primary_key
                || item.column.unique
                || !item.column.nullable
                    && matches!(item.column.default, None | Some(MigrationDefault::CurrentTimestamp)) =>
        {
            Some(&item.table)
        }
        _ => None,
    }
}

fn step_table(step: &MigrationStep) -> Option<&str> {
    match step {
        MigrationStep::CreateTable(item) => Some(&item.table),
        MigrationStep::DropTable(item) => Some(&item.table),
        MigrationStep::RenameTable(item) => Some(&item.to),
        MigrationStep::AddColumn(item) => Some(&item.table),
        MigrationStep::DropColumn(item) => Some(&item.table),
        MigrationStep::AlterColumn(item) => Some(&item.table),
        MigrationStep::RenameColumn(item) => Some(&item.table),
        MigrationStep::AddForeignKey(item) => Some(&item.table),
        MigrationStep::DropForeignKey(item) => Some(&item.table),
        MigrationStep::CreateIndex(item) => Some(&item.table),
        MigrationStep::DropIndex(item) => Some(&item.table),
        MigrationStep::RebuildTable(item) => Some(&item.desired.name),
        MigrationStep::CreateEnum(_) | MigrationStep::DropEnum(_) | MigrationStep::AlterEnum(_) => None,
    }
}

fn normalize_legacy_table_names(
    desired: &DatabaseSchema,
    current: &DatabaseSchema,
) -> (DatabaseSchema, Vec<RenameTableMigration>) {
    let desired_names = desired.tables.iter().map(|table| table.name.as_str()).collect::<BTreeSet<_>>();
    let current_names = current.tables.iter().map(|table| table.name.as_str()).collect::<BTreeSet<_>>();
    let mut claimed_current = BTreeSet::new();
    let mut rename_map = BTreeMap::new();

    for desired_table in &desired.tables {
        if current_names.contains(desired_table.name.as_str()) {
            continue;
        }

        let candidates = current
            .tables
            .iter()
            .filter(|current_table| {
                !desired_names.contains(current_table.name.as_str())
                    && !claimed_current.contains(current_table.name.as_str())
                    && current_table.name != desired_table.name
                    && table_name(&current_table.name) == desired_table.name
            })
            .collect::<Vec<_>>();

        if let [legacy_table] = candidates.as_slice() {
            claimed_current.insert(legacy_table.name.clone());
            rename_map.insert(legacy_table.name.clone(), desired_table.name.clone());
        }
    }

    let renames = rename_map
        .iter()
        .map(|(from, to)| RenameTableMigration { from: from.clone(), to: to.clone() })
        .collect::<Vec<_>>();
    if renames.is_empty() {
        return (current.clone(), renames);
    }

    let mut normalized = current.clone();
    for table in &mut normalized.tables {
        if let Some(name) = rename_map.get(&table.name) {
            table.name.clone_from(name);
        }
        for foreign_key in &mut table.foreign_keys {
            if let Some(name) = rename_map.get(&foreign_key.references_table) {
                foreign_key.references_table.clone_from(name);
            }
        }
    }

    (normalized, renames)
}

/// Detects a table rename authored directly in the schema: a table dropped
/// from `current` and a table created in `desired` whose column names match
/// exactly. Only fires when the match is unambiguous; multiple equally
/// plausible candidates are reported as an error instead of guessed, mirroring
/// the column-rename heuristic in [`diff_columns`].
fn detect_table_renames(
    plan: &mut MigrationPlan,
    desired: &DatabaseSchema,
    current: &DatabaseSchema,
) -> (DatabaseSchema, Vec<RenameTableMigration>) {
    let desired_names = desired.tables.iter().map(|table| table.name.as_str()).collect::<BTreeSet<_>>();
    let current_names = current.tables.iter().map(|table| table.name.as_str()).collect::<BTreeSet<_>>();
    let mut claimed_current = BTreeSet::new();
    let mut rename_map = BTreeMap::new();

    for desired_table in &desired.tables {
        if current_names.contains(desired_table.name.as_str()) {
            continue;
        }

        let candidates = current
            .tables
            .iter()
            .filter(|current_table| {
                !desired_names.contains(current_table.name.as_str())
                    && !claimed_current.contains(current_table.name.as_str())
                    && current_table.name != desired_table.name
                    && table_rename_compatible(current_table, desired_table)
            })
            .collect::<Vec<_>>();

        match candidates.as_slice() {
            [candidate] => {
                plan.warnings.push(MigrationWarning {
                    message: format!(
                        "Table `{}` looks like it was renamed to `{}`. Dinoco cannot prove that both tables have the same meaning; review the mapping before applying it.",
                        candidate.name, desired_table.name,
                    ),
                    destructive: true,
                });
                claimed_current.insert(candidate.name.clone());
                rename_map.insert(candidate.name.clone(), desired_table.name.clone());
            }
            [] => {}
            _ => {
                plan.errors.push(format!(
                    "Table `{}` could be a rename of more than one removed table: {}. Add an intermediate migration with an unambiguous name or provide a reviewed custom mapping.",
                    desired_table.name,
                    candidates.iter().map(|table| format!("`{}`", table.name)).collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }

    let renames = rename_map
        .iter()
        .map(|(from, to)| RenameTableMigration { from: from.clone(), to: to.clone() })
        .collect::<Vec<_>>();
    if renames.is_empty() {
        return (current.clone(), renames);
    }

    let mut normalized = current.clone();
    for table in &mut normalized.tables {
        if let Some(name) = rename_map.get(&table.name) {
            table.name.clone_from(name);
        }
        for foreign_key in &mut table.foreign_keys {
            if let Some(name) = rename_map.get(&foreign_key.references_table) {
                foreign_key.references_table.clone_from(name);
            }
        }
    }

    (normalized, renames)
}

/// A table is a plausible rename candidate only when its column *names*
/// match exactly; anything looser risks silently pairing two unrelated
/// tables and rewriting live data under the wrong identity.
fn table_rename_compatible(current: &DatabaseTable, desired: &DatabaseTable) -> bool {
    let current_columns = current.columns.iter().map(|column| column.name.as_str()).collect::<BTreeSet<_>>();
    let desired_columns = desired.columns.iter().map(|column| column.name.as_str()).collect::<BTreeSet<_>>();
    !current_columns.is_empty() && current_columns == desired_columns
}

fn dropped_table_order(
    current_tables: &BTreeMap<&str, &DatabaseTable>,
    dropped_tables: &BTreeSet<&str>,
) -> Vec<String> {
    let mut remaining = dropped_tables.iter().map(|name| (*name).to_string()).collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(remaining.len());

    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .find(|candidate| {
                !remaining.iter().any(|other| {
                    other != *candidate
                        && current_tables.get(other.as_str()).is_some_and(|table| {
                            table.foreign_keys.iter().any(|foreign_key| foreign_key.references_table == **candidate)
                        })
                })
            })
            .cloned()
            .unwrap_or_else(|| remaining.first().expect("remaining set is not empty").clone());
        remaining.remove(&next);
        ordered.push(next);
    }

    ordered
}

pub fn desired_database_schema(schema: &Schema) -> DatabaseSchema {
    DatabaseSchema {
        tables: generate_create_table_migrations(schema)
            .into_iter()
            .map(|migration| DatabaseTable {
                indexes: table_indexes(&migration, schema),
                name: migration.table,
                row_count: 0,
                columns: migration.columns,
                foreign_keys: migration.foreign_keys,
            })
            .collect(),
        enums: schema
            .enums()
            .map(|item| DatabaseEnum { name: item.name.clone(), values: item.values.clone() })
            .collect(),
    }
}

fn diff_columns(plan: &mut MigrationPlan, current_table: &DatabaseTable, desired_table: &DatabaseTable) {
    let current_columns =
        current_table.columns.iter().map(|column| (column.name.as_str(), column)).collect::<BTreeMap<_, _>>();
    let desired_columns =
        desired_table.columns.iter().map(|column| (column.name.as_str(), column)).collect::<BTreeMap<_, _>>();
    let mut renamed_current = BTreeSet::new();
    let mut renamed_desired = BTreeSet::new();

    for (desired_name, desired_column) in
        desired_columns.iter().filter(|(name, _)| !current_columns.contains_key(**name))
    {
        let candidates = current_columns
            .iter()
            .filter(|(name, current_column)| {
                !desired_columns.contains_key(**name)
                    && !renamed_current.contains(**name)
                    && rename_compatible(current_column, desired_column)
            })
            .collect::<Vec<_>>();

        if candidates.len() == 1 {
            let (current_name, current_column) = candidates[0];
            plan.warnings.push(MigrationWarning {
                message: format!(
                    "Column `{}.{}` looks like it was renamed to `{}`. Dinoco cannot prove that both fields have the same meaning; review the mapping before applying it.",
                    current_table.name, current_column.name, desired_column.name,
                ),
                destructive: true,
            });
            plan.steps.push(MigrationStep::RenameColumn(RenameColumnMigration {
                table: desired_table.name.clone(),
                from: (*current_name).to_string(),
                to: (*desired_name).to_string(),
            }));
            renamed_current.insert(*current_name);
            renamed_desired.insert(*desired_name);
        } else if candidates.len() > 1 {
            plan.errors.push(format!(
                "Column `{}.{}` could be a rename of more than one removed column: {}. Add an intermediate migration with an unambiguous name or provide a reviewed custom mapping.",
                desired_table.name,
                desired_column.name,
                candidates
                    .iter()
                    .map(|(_, column)| format!("`{}.{}`", current_table.name, column.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    for (name, desired_column) in &desired_columns {
        if renamed_desired.contains(name) {
            continue;
        }
        let Some(current_column) = current_columns.get(name) else {
            if current_table.row_count > 0 && !desired_column.nullable && desired_column.default.is_none() {
                plan.warnings.push(MigrationWarning {
                    message: format!(
                        "Required column `{}.{}` will be added without a default while the table has {} row(s).",
                        desired_table.name, desired_column.name, current_table.row_count
                    ),
                    destructive: true,
                });
            }
            plan.steps.push(MigrationStep::AddColumn(AddColumnMigration {
                table: desired_table.name.clone(),
                column: (*desired_column).clone(),
            }));
            continue;
        };

        if !columns_equivalent(current_column, desired_column) {
            if current_table.row_count > 0 {
                let destructive = column_change_destructive(current_column, desired_column);
                plan.warnings.push(MigrationWarning {
                    message: column_change_warning(
                        &desired_table.name,
                        current_column,
                        desired_column,
                        current_table.row_count,
                    ),
                    destructive,
                });
            }
            plan.steps.push(MigrationStep::AlterColumn(AlterColumnMigration {
                table: desired_table.name.clone(),
                current: (*current_column).clone(),
                desired: (*desired_column).clone(),
            }));
        }
    }

    for (name, current_column) in &current_columns {
        if renamed_current.contains(name) {
            continue;
        }
        if !desired_columns.contains_key(name) {
            plan.warnings.push(MigrationWarning {
                message: format!(
                    "Column `{}.{}` will be dropped from a table with {} row(s); its schema and any stored data will be lost.",
                    current_table.name, current_column.name, current_table.row_count
                ),
                destructive: true,
            });
            plan.steps.push(MigrationStep::DropColumn(DropColumnMigration {
                table: current_table.name.clone(),
                column: current_column.name.clone(),
            }));
        }
    }
}

fn rename_compatible(current: &MigrationColumn, desired: &MigrationColumn) -> bool {
    column_types_equivalent(&current.ty, &desired.ty)
        && current.primary_key == desired.primary_key
        && current.unique == desired.unique
        && current.nullable == desired.nullable
        && defaults_equivalent(current, desired)
}

fn diff_foreign_keys(plan: &mut MigrationPlan, current_table: &DatabaseTable, desired_table: &DatabaseTable) {
    let current_keys = current_table
        .foreign_keys
        .iter()
        .map(|foreign_key| (foreign_key.name.as_str(), foreign_key))
        .collect::<BTreeMap<_, _>>();
    let desired_keys = desired_table
        .foreign_keys
        .iter()
        .map(|foreign_key| (foreign_key.name.as_str(), foreign_key))
        .collect::<BTreeMap<_, _>>();

    let mut matched_current = BTreeSet::new();

    for (name, desired_key) in &desired_keys {
        if let Some(current_key) = current_keys.get(name)
            && foreign_keys_equivalent(current_key, desired_key)
        {
            matched_current.insert((*name).to_string());
            continue;
        }

        if let Some(current_key) = current_table.foreign_keys.iter().find(|current_key| {
            !matched_current.contains(&current_key.name) && foreign_keys_equivalent(current_key, desired_key)
        }) {
            matched_current.insert(current_key.name.clone());
            continue;
        }

        match current_keys.get(name) {
            Some(current_key) => {
                plan.warnings.push(MigrationWarning {
                    message: format!(
                        "Foreign key `{}` on `{}` will be recreated.",
                        desired_key.name, desired_table.name
                    ),
                    destructive: false,
                });
                plan.steps.push(MigrationStep::DropForeignKey(DropForeignKeyMigration {
                    table: desired_table.name.clone(),
                    name: (*name).to_string(),
                }));
                plan.steps.push(MigrationStep::AddForeignKey(AddForeignKeyMigration {
                    table: desired_table.name.clone(),
                    foreign_key: (*desired_key).clone(),
                }));
                matched_current.insert(current_key.name.clone());
            }
            None => plan.steps.push(MigrationStep::AddForeignKey(AddForeignKeyMigration {
                table: desired_table.name.clone(),
                foreign_key: (*desired_key).clone(),
            })),
        }
    }

    for (name, current_key) in &current_keys {
        if !matched_current.contains(*name) {
            plan.warnings.push(MigrationWarning {
                message: format!("Foreign key `{}` on `{}` will be dropped.", current_key.name, current_table.name),
                destructive: false,
            });
            plan.steps.push(MigrationStep::DropForeignKey(DropForeignKeyMigration {
                table: current_table.name.clone(),
                name: (*name).to_string(),
            }));
        }
    }
}

fn foreign_keys_equivalent(left: &MigrationForeignKey, right: &MigrationForeignKey) -> bool {
    left.columns == right.columns
        && left.references_table == right.references_table
        && left.references_columns == right.references_columns
        && left.on_update == right.on_update
        && left.on_delete == right.on_delete
}

fn diff_indexes(plan: &mut MigrationPlan, current_table: &DatabaseTable, desired_table: &DatabaseTable) {
    let current_indexes =
        current_table.indexes.iter().map(|index| (index.name.as_str(), index)).collect::<BTreeMap<_, _>>();
    let mut matched_current = BTreeSet::new();
    let mut scheduled_drops = BTreeSet::new();

    for desired_index in &desired_table.indexes {
        if let Some(current_index) = current_indexes.get(desired_index.name.as_str())
            && current_index.columns == desired_index.columns
            && current_index.kind == desired_index.kind
        {
            matched_current.insert(current_index.name.clone());
            continue;
        }

        if desired_index.automatic
            && let Some(current_index) = current_table.indexes.iter().find(|index| {
                !matched_current.contains(&index.name)
                    && index.columns == desired_index.columns
                    && index.kind == desired_index.kind
            })
        {
            matched_current.insert(current_index.name.clone());
            continue;
        }

        if index_is_primary_key(desired_index, current_table) {
            continue;
        }

        if let Some(current_index) = current_indexes.get(desired_index.name.as_str()) {
            scheduled_drops.insert(current_index.name.clone());
            plan.steps.push(MigrationStep::DropIndex(DropIndexMigration {
                table: current_table.name.clone(),
                index: (*current_index).clone(),
            }));
        }
        plan.steps.push(MigrationStep::CreateIndex(CreateIndexMigration {
            table: desired_table.name.clone(),
            index: desired_index.clone(),
        }));
    }

    for current_index in &current_table.indexes {
        if !matched_current.contains(&current_index.name) && !scheduled_drops.contains(&current_index.name) {
            plan.steps.push(MigrationStep::DropIndex(DropIndexMigration {
                table: current_table.name.clone(),
                index: current_index.clone(),
            }));
        }
    }
}

pub(crate) fn index_is_primary_key(index: &MigrationIndex, table: &DatabaseTable) -> bool {
    if !index.automatic || index.kind != MigrationIndexKind::Standard {
        return false;
    }

    let primary_key_columns =
        table.columns.iter().filter(|column| column.primary_key).map(|column| column.name.as_str()).collect::<Vec<_>>();

    !primary_key_columns.is_empty() && index.columns.iter().map(String::as_str).eq(primary_key_columns)
}

fn columns_equivalent(left: &MigrationColumn, right: &MigrationColumn) -> bool {
    column_types_equivalent(&left.ty, &right.ty)
        && left.primary_key == right.primary_key
        && left.unique == right.unique
        && left.nullable == right.nullable
        && defaults_equivalent(left, right)
}

fn column_change_destructive(current: &MigrationColumn, desired: &MigrationColumn) -> bool {
    !column_types_equivalent(&current.ty, &desired.ty)
        || (current.nullable && !desired.nullable)
        || (!current.unique && desired.unique)
}

fn defaults_equivalent(left: &MigrationColumn, right: &MigrationColumn) -> bool {
    normalize_default(&left.default) == normalize_default(&right.default)
}

fn column_change_warning(
    table: &str,
    current_column: &MigrationColumn,
    desired_column: &MigrationColumn,
    row_count: i64,
) -> String {
    if current_column.nullable && !desired_column.nullable {
        return format!(
            "Column `{}.{}` will become required while the table has {} row(s). Existing NULL values would make this migration fail; clean or backfill the data before applying it.",
            table, desired_column.name, row_count
        );
    }

    if !current_column.nullable && desired_column.nullable {
        return format!(
            "Column `{}.{}` will become optional while the table has {} row(s).",
            table, desired_column.name, row_count
        );
    }

    format!(
        "Column `{}.{}` will change from `{}` to `{}` while the table has {} row(s).",
        table,
        desired_column.name,
        describe_column(current_column),
        describe_column(desired_column),
        row_count
    )
}

fn column_types_equivalent(left: &MigrationColumnType, right: &MigrationColumnType) -> bool {
    left == right
        || matches!(
            (left, right),
            (MigrationColumnType::String, MigrationColumnType::Text)
                | (MigrationColumnType::Text, MigrationColumnType::String)
        )
}

fn normalize_default(default: &Option<MigrationDefault>) -> Option<String> {
    match default {
        Some(MigrationDefault::String(value)) => Some(format!("string:{value}")),
        Some(MigrationDefault::Boolean(value)) => Some(format!("bool:{value}")),
        Some(MigrationDefault::Integer(value)) => Some(format!("int:{value}")),
        Some(MigrationDefault::Float(value)) => Some(format!("float:{value}")),
        Some(MigrationDefault::CurrentTimestamp) => Some("current_timestamp".to_string()),
        Some(MigrationDefault::AutoIncrement) => Some("autoincrement".to_string()),
        None => None,
    }
}

fn describe_column(column: &MigrationColumn) -> String {
    format!(
        "{:?}, {}, {}, default {:?}",
        column.ty,
        if column.nullable { "nullable" } else { "required" },
        if column.unique { "unique" } else { "not unique" },
        column.default
    )
}

fn migration_column(model: &Model, field: &ModelField, schema: &Schema) -> MigrationColumn {
    MigrationColumn {
        name: field.name.clone(),
        ty: migration_type(field, schema),
        primary_key: is_primary_key_field(model, field),
        unique: field.attributes.iter().any(|attr| attr.name == "unique")
            || model
                .attributes("uniques")
                .filter_map(|attribute| attribute.field_names())
                .any(|fields| fields.as_slice() == [field.name.as_str()]),
        nullable: field.ty.optional,
        default: migration_default(field),
    }
}

fn relation_unique_columns<'a>(model: &'a dinoco_compiler::Model, schema: &Schema) -> BTreeSet<&'a str> {
    model
        .fields
        .iter()
        .filter(|field| {
            !field.ty.list
                && field.is_relation(schema)
                && field.attributes.iter().any(|attribute| attribute.name == "unique")
        })
        .filter_map(|field| field.attributes.iter().find(|attribute| attribute.name == "relation"))
        .filter_map(|relation| relation.argument("fields"))
        .filter_map(array_idents)
        .flatten()
        .filter_map(|name| model.fields.iter().find(|field| field.name == name).map(|field| field.name.as_str()))
        .collect()
}

fn relation_foreign_keys(model_name: &str, schema: &Schema) -> Vec<MigrationForeignKey> {
    let Some(model) = schema.models().find(|model| model.name == model_name) else {
        return Vec::new();
    };
    let mut keys = Vec::new();

    for field in &model.fields {
        if !field.is_relation(schema) || field.ty.list {
            continue;
        }
        let Some(relation) = field.attributes.iter().find(|attr| attr.name == "relation") else {
            continue;
        };
        let Some(columns) = relation.argument("fields").and_then(array_idents) else {
            continue;
        };
        let Some(references_columns) = relation.argument("references").and_then(array_idents) else {
            continue;
        };

        let table = model_table_name(model);
        let references_table = schema
            .models()
            .find(|candidate| candidate.name == field.ty.name)
            .map(model_table_name)
            .unwrap_or_else(|| table_name(&field.ty.name));
        keys.push(MigrationForeignKey {
            name: relation
                .argument("map")
                .and_then(string_or_ident)
                .unwrap_or_else(|| foreign_key_name(&table, &columns.iter().map(String::as_str).collect::<Vec<_>>())),
            columns,
            references_table,
            references_columns,
            on_update: relation
                .argument("onUpdate")
                .and_then(parse_referential_action)
                .unwrap_or(ReferentialAction::NoAction),
            on_delete: relation
                .argument("onDelete")
                .and_then(parse_referential_action)
                .unwrap_or(ReferentialAction::NoAction),
        });
    }

    keys
}

fn table_indexes(migration: &CreateTableMigration, schema: &Schema) -> Vec<MigrationIndex> {
    let mut indexes = Vec::new();
    let mut seen_names = BTreeSet::new();

    if let Some(model) = schema.models().find(|model| model_table_name(model) == migration.table) {
        for field in &model.fields {
            if let Some(attribute) = field.attributes.iter().find(|attribute| attribute.name == "index") {
                let columns = vec![field.name.clone()];
                let name = attribute
                    .argument("map")
                    .and_then(string_or_ident)
                    .unwrap_or_else(|| index_name(&migration.table, &[field.name.as_str()]));
                push_index(&mut indexes, &mut seen_names, name, columns, false, MigrationIndexKind::Standard);
            }

            if fulltext_indexes_supported(schema)
                && field.attributes.iter().any(|attribute| attribute.name == "fulltext")
            {
                push_index(
                    &mut indexes,
                    &mut seen_names,
                    format!("{}_fulltext", index_name(&migration.table, &[field.name.as_str()])),
                    vec![field.name.clone()],
                    false,
                    MigrationIndexKind::FullText,
                );
            }
        }

        for attribute in model.attributes("indexes") {
            let columns =
                attribute.field_names().unwrap_or_default().into_iter().map(str::to_string).collect::<Vec<_>>();
            let column_refs = columns.iter().map(String::as_str).collect::<Vec<_>>();
            push_index(
                &mut indexes,
                &mut seen_names,
                index_name(&migration.table, &column_refs),
                columns,
                false,
                MigrationIndexKind::Standard,
            );
        }

        for attribute in model.attributes("uniques") {
            let columns =
                attribute.field_names().unwrap_or_default().into_iter().map(str::to_string).collect::<Vec<_>>();
            if columns.len() <= 1 {
                continue;
            }
            let column_refs = columns.iter().map(String::as_str).collect::<Vec<_>>();
            push_index(
                &mut indexes,
                &mut seen_names,
                unique_index_name(&migration.table, &column_refs),
                columns,
                false,
                MigrationIndexKind::Unique,
            );
        }

        if fulltext_indexes_supported(schema) {
            for attribute in model.attributes("fulltexts") {
                let columns =
                    attribute.field_names().unwrap_or_default().into_iter().map(str::to_string).collect::<Vec<_>>();
                let column_refs = columns.iter().map(String::as_str).collect::<Vec<_>>();
                push_index(
                    &mut indexes,
                    &mut seen_names,
                    format!("{}_fulltext", index_name(&migration.table, &column_refs)),
                    columns,
                    false,
                    MigrationIndexKind::FullText,
                );
            }
        }
    }

    let primary_key_columns = migration
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();
    if !primary_key_columns.is_empty() {
        let column_refs = primary_key_columns.iter().map(String::as_str).collect::<Vec<_>>();
        let name = index_name(&migration.table, &column_refs);
        push_index(&mut indexes, &mut seen_names, name, primary_key_columns, true, MigrationIndexKind::Standard);
    }

    for foreign_key in &migration.foreign_keys {
        let columns = foreign_key.columns.clone();
        let column_refs = columns.iter().map(String::as_str).collect::<Vec<_>>();
        let name = index_name(&migration.table, &column_refs);
        push_index(&mut indexes, &mut seen_names, name, columns, true, MigrationIndexKind::Standard);
    }

    indexes
}

fn push_index(
    indexes: &mut Vec<MigrationIndex>,
    seen_names: &mut BTreeSet<String>,
    mut name: String,
    columns: Vec<String>,
    automatic: bool,
    kind: MigrationIndexKind,
) {
    if let Some(existing) =
        indexes.iter_mut().find(|index| index.name == name && index.columns == columns && index.kind == kind)
    {
        existing.automatic |= automatic;
        return;
    }
    if seen_names.contains(&name) {
        let base = name.clone();
        let mut suffix = 2;
        while seen_names.contains(&name) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
    }
    seen_names.insert(name.clone());
    indexes.push(MigrationIndex { name, columns, automatic, kind });
}

fn fulltext_indexes_supported(schema: &Schema) -> bool {
    schema
        .config()
        .and_then(|config| config.entries.iter().find(|entry| entry.key == "database"))
        .and_then(|entry| match &entry.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) => Some(value.as_str()),
            _ => None,
        })
        .is_none_or(|database| database != "sqlite")
}

fn is_primary_key_field(model: &Model, field: &ModelField) -> bool {
    field.attributes.iter().any(|attribute| attribute.name == "id")
        || model
            .attribute("ids")
            .and_then(|attribute| attribute.field_names())
            .is_some_and(|fields| fields.contains(&field.name.as_str()))
}

fn model_table_name(model: &Model) -> String {
    model
        .attribute("table_name")
        .and_then(|attribute| attribute.arguments.first())
        .and_then(|argument| match argument {
            AttributeArgument::Value(AttributeValue::String(value)) => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_else(|| table_name(&model.name))
}

fn generate_many_to_many_join_migrations(schema: &Schema) -> Vec<CreateTableMigration> {
    let mut seen = BTreeSet::new();
    let mut migrations = Vec::new();

    for model in schema.models() {
        for field in &model.fields {
            if !field.ty.list {
                continue;
            }
            let Some(target) = schema.models().find(|target| target.name == field.ty.name) else {
                continue;
            };

            if field
                .attributes
                .iter()
                .find(|attr| attr.name == "relation")
                .and_then(|attr| attr.argument("fields"))
                .is_some()
            {
                continue;
            }

            let left = model.name.as_str();
            let right = field.ty.name.as_str();
            let relation_label = field.attributes.iter().find(|attr| attr.name == "relation").and_then(relation_name);
            let has_list_opposite = target.fields.iter().any(|candidate| {
                (model.name != target.name || candidate.name != field.name)
                    && candidate.ty.list
                    && candidate.ty.name == model.name
                    && candidate.attributes.iter().find(|attr| attr.name == "relation").and_then(relation_name)
                        == relation_label
            });
            if !has_list_opposite {
                continue;
            }
            let key = relation_key(left, right, relation_label.as_deref());
            if !seen.insert(key) {
                continue;
            }

            let left_column = if left == right { "a_id".to_string() } else { format!("{}_id", table_name(left)) };
            let right_column = if left == right { "b_id".to_string() } else { format!("{}_id", table_name(right)) };
            let join_table = many_to_many_table_name(left, right, relation_label.as_deref());

            migrations.push(CreateTableMigration {
                table: join_table.clone(),
                if_not_exists: true,
                columns: vec![
                    MigrationColumn {
                        name: left_column.clone(),
                        ty: primary_column_type(schema, left),
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: right_column.clone(),
                        ty: primary_column_type(schema, right),
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                ],
                foreign_keys: vec![
                    MigrationForeignKey {
                        name: foreign_key_name(&join_table, &[left_column.as_str()]),
                        columns: vec![left_column],
                        references_table: table_name(left),
                        references_columns: vec![primary_column_name(schema, left)],
                        on_update: ReferentialAction::Cascade,
                        on_delete: ReferentialAction::Cascade,
                    },
                    MigrationForeignKey {
                        name: foreign_key_name(&join_table, &[right_column.as_str()]),
                        columns: vec![right_column],
                        references_table: table_name(right),
                        references_columns: vec![primary_column_name(schema, right)],
                        on_update: ReferentialAction::Cascade,
                        on_delete: ReferentialAction::Cascade,
                    },
                ],
            });
        }
    }

    migrations
}

fn primary_column_type(schema: &Schema, model_name: &str) -> MigrationColumnType {
    schema
        .models()
        .find(|model| model.name == model_name)
        .and_then(|model| model.fields.iter().find(|field| field.attributes.iter().any(|attr| attr.name == "id")))
        .map(|field| migration_type(field, schema))
        .unwrap_or(MigrationColumnType::String)
}

fn primary_column_name(schema: &Schema, model_name: &str) -> String {
    schema
        .models()
        .find(|model| model.name == model_name)
        .and_then(|model| model.fields.iter().find(|field| field.attributes.iter().any(|attr| attr.name == "id")))
        .map(|field| field.name.clone())
        .unwrap_or_else(|| "id".to_string())
}

fn relation_key(left: &str, right: &str, relation_name: Option<&str>) -> String {
    let mut names = [left, right];
    names.sort();
    relation_name
        .map(|name| format!("{}:{}:{name}", names[0], names[1]))
        .unwrap_or_else(|| format!("{}:{}", names[0], names[1]))
}

fn many_to_many_table_name(left: &str, right: &str, relation_name: Option<&str>) -> String {
    let base = if left <= right {
        format!("_{}_to_{}", table_name(left), table_name(right))
    } else {
        format!("_{}_to_{}", table_name(right), table_name(left))
    };

    relation_name.map(|name| format!("{base}_{}", table_name(name))).unwrap_or(base)
}

fn foreign_key_name(table: &str, columns: &[&str]) -> String {
    format!("fk_{}_{}", table, columns.join("_"))
}

fn index_name(table: &str, columns: &[&str]) -> String {
    format!("idx_{}_{}", table, columns.join("_"))
}

fn unique_index_name(table: &str, columns: &[&str]) -> String {
    format!("uq_{}_{}", table, columns.join("_"))
}

fn relation_name(attribute: &dinoco_compiler::Attribute) -> Option<String> {
    attribute.argument("name").and_then(string_or_ident).or_else(|| {
        attribute.arguments.iter().find_map(|argument| match argument {
            AttributeArgument::Value(value) => string_or_ident(value),
            _ => None,
        })
    })
}

fn array_idents(value: &AttributeValue) -> Option<Vec<String>> {
    let AttributeValue::Array(values) = value else {
        return None;
    };
    values
        .iter()
        .map(|value| match value {
            AttributeValue::Ident(value) | AttributeValue::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn string_or_ident(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String(value) | AttributeValue::Ident(value) => Some(value.clone()),
        _ => None,
    }
}

fn parse_referential_action(value: &AttributeValue) -> Option<ReferentialAction> {
    match string_or_ident(value)?.as_str() {
        "Cascade" | "cascade" => Some(ReferentialAction::Cascade),
        "Restrict" | "restrict" => Some(ReferentialAction::Restrict),
        "NoAction" | "noAction" | "no_action" => Some(ReferentialAction::NoAction),
        "SetNull" | "setNull" | "set_null" => Some(ReferentialAction::SetNull),
        "SetDefault" | "setDefault" | "set_default" => Some(ReferentialAction::SetDefault),
        _ => None,
    }
}

fn migration_type(field: &ModelField, schema: &Schema) -> MigrationColumnType {
    if let Some(item) = schema.enums().find(|item| item.name == field.ty.name) {
        return MigrationColumnType::Enum { name: item.name.clone(), values: item.values.clone() };
    }

    match field.ty.name.as_str() {
        "Boolean" => MigrationColumnType::Boolean,
        "Integer" => MigrationColumnType::Integer,
        "Float" => MigrationColumnType::Float,
        "Json" => MigrationColumnType::Json,
        "DateTime" => MigrationColumnType::DateTime,
        "Date" => MigrationColumnType::Date,
        _ => MigrationColumnType::String,
    }
}

fn migration_default(field: &ModelField) -> Option<MigrationDefault> {
    let attr = field.attributes.iter().find(|attr| attr.name == "default")?;
    let value = attr.arguments.first()?;
    let AttributeArgument::Value(value) = value else {
        return None;
    };

    match value {
        AttributeValue::Ident(value) if value == "true" => Some(MigrationDefault::Boolean(true)),
        AttributeValue::Ident(value) if value == "false" => Some(MigrationDefault::Boolean(false)),
        AttributeValue::Ident(value) => value
            .parse::<i64>()
            .map(MigrationDefault::Integer)
            .or_else(|_| value.parse::<f64>().map(MigrationDefault::Float))
            .ok()
            .or_else(|| Some(MigrationDefault::String(value.clone()))),
        AttributeValue::String(value) => Some(MigrationDefault::String(value.clone())),
        AttributeValue::Call { name, .. } if name == "now" => Some(MigrationDefault::CurrentTimestamp),
        AttributeValue::Call { name, .. } if name == "autoincrement" => Some(MigrationDefault::AutoIncrement),
        _ => None,
    }
}

fn table_name(name: &str) -> String {
    let mut out = String::new();
    let chars = name.chars().collect::<Vec<_>>();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_ascii_uppercase() {
            let previous_is_lowercase_or_digit =
                index > 0 && (chars[index - 1].is_ascii_lowercase() || chars[index - 1].is_ascii_digit());
            let starts_word_after_acronym = index > 0
                && chars[index - 1].is_ascii_uppercase()
                && chars.get(index + 1).is_some_and(|next| next.is_ascii_lowercase());
            if previous_is_lowercase_or_digit || starts_word_after_acronym {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
