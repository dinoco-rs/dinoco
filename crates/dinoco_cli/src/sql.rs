use std::collections::{BTreeMap, BTreeSet};

use dinoco_compiler::{AttributeArgument, AttributeValue, ConfigValue, Model, ModelField, Schema};
use dinoco_engine::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CreateEnumMigration,
    CreateIndexMigration, CreateTableMigration, DropColumnMigration, DropEnumMigration, DropForeignKeyMigration,
    DropIndexMigration, DropTableMigration, MigrationColumn, MigrationColumnType, MigrationDefault,
    MigrationForeignKey, MigrationIndex, MigrationIndexKind, ReferentialAction, RenameColumnMigration,
};

use crate::db::{DatabaseEnum, DatabaseSchema, DatabaseTable};

#[derive(Debug, Clone, Default)]
pub struct MigrationPlan {
    pub steps: Vec<MigrationStep>,
    pub warnings: Vec<MigrationWarning>,
}

#[derive(Debug, Clone)]
pub enum MigrationStep {
    CreateEnum(CreateEnumMigration),
    DropEnum(DropEnumMigration),
    AlterEnum(AlterEnumMigration),
    CreateTable(CreateTableMigration),
    DropTable(DropTableMigration),
    AddColumn(AddColumnMigration),
    DropColumn(DropColumnMigration),
    AlterColumn(AlterColumnMigration),
    RenameColumn(RenameColumnMigration),
    AddForeignKey(AddForeignKeyMigration),
    DropForeignKey(DropForeignKeyMigration),
    CreateIndex(CreateIndexMigration),
    DropIndex(DropIndexMigration),
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

    for (name, desired_key) in &desired_keys {
        match current_keys.get(name) {
            None => plan.steps.push(MigrationStep::AddForeignKey(AddForeignKeyMigration {
                table: desired_table.name.clone(),
                foreign_key: (*desired_key).clone(),
            })),
            Some(current_key) if *current_key != *desired_key => {
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
            }
            _ => {}
        }
    }

    for (name, current_key) in &current_keys {
        if !desired_keys.contains_key(name) {
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
    !schema
        .config()
        .and_then(|config| config.entries.iter().find(|entry| entry.key == "database"))
        .and_then(|entry| match &entry.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) => Some(value.as_str()),
            _ => None,
        })
        .is_some_and(|database| database == "sqlite")
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
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DatabaseSchema, DatabaseTable};

    #[test]
    fn plan_detects_dropped_column_with_existing_rows_as_destructive() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    String @id
                email String
            }
            "#,
        )
        .expect("schema");
        let current = DatabaseSchema {
            tables: vec![DatabaseTable {
                name: "user".to_string(),
                row_count: 2,
                columns: vec![
                    MigrationColumn {
                        name: "id".to_string(),
                        ty: MigrationColumnType::String,
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: "email".to_string(),
                        ty: MigrationColumnType::String,
                        primary_key: false,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: "password".to_string(),
                        ty: MigrationColumnType::String,
                        primary_key: false,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                ],
                foreign_keys: Vec::new(),
                indexes: Vec::new(),
            }],
            enums: Vec::new(),
        };

        let plan = plan_schema_migration(&schema, &current);

        assert!(
            plan.steps
                .iter()
                .any(|step| matches!(step, MigrationStep::DropColumn(column) if column.column == "password"))
        );
        assert!(
            plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("data will be lost"))
        );
    }

    #[test]
    fn plan_detects_added_required_column_on_populated_table_as_destructive() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id     String @id
                email  String
                office String
            }
            "#,
        )
        .expect("schema");
        let current = DatabaseSchema {
            tables: vec![DatabaseTable {
                name: "user".to_string(),
                row_count: 1,
                columns: vec![
                    MigrationColumn {
                        name: "id".to_string(),
                        ty: MigrationColumnType::String,
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: "email".to_string(),
                        ty: MigrationColumnType::String,
                        primary_key: false,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                ],
                foreign_keys: Vec::new(),
                indexes: Vec::new(),
            }],
            enums: Vec::new(),
        };

        let plan = plan_schema_migration(&schema, &current);

        assert!(
            plan.steps
                .iter()
                .any(|step| matches!(step, MigrationStep::AddColumn(column) if column.column.name == "office"))
        );
        assert!(
            plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("without a default"))
        );
    }

    #[test]
    fn plan_detects_optional_field_becoming_required_as_destructive() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    String @id
                email String
            }
            "#,
        )
        .expect("schema");
        let current = DatabaseSchema {
            tables: vec![DatabaseTable {
                name: "user".to_string(),
                row_count: 3,
                columns: vec![string_column("id", true), nullable_string_column("email")],
                foreign_keys: Vec::new(),
                indexes: Vec::new(),
            }],
            enums: Vec::new(),
        };

        let plan = plan_schema_migration(&schema, &current);

        assert!(plan.steps.iter().any(
            |step| matches!(step, MigrationStep::AlterColumn(column) if column.desired.name == "email" && !column.desired.nullable)
        ));
        assert!(
            plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("will become required"))
        );
    }

    #[test]
    fn plan_detects_required_field_becoming_optional_as_safe_alter() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    String  @id
                email String?
            }
            "#,
        )
        .expect("schema");
        let current = DatabaseSchema {
            tables: vec![DatabaseTable {
                name: "user".to_string(),
                row_count: 3,
                columns: vec![string_column("id", true), string_column("email", false)],
                foreign_keys: Vec::new(),
                indexes: Vec::new(),
            }],
            enums: Vec::new(),
        };

        let plan = plan_schema_migration(&schema, &current);

        assert!(plan.steps.iter().any(
            |step| matches!(step, MigrationStep::AlterColumn(column) if column.desired.name == "email" && column.desired.nullable)
        ));
        assert!(
            plan.warnings
                .iter()
                .any(|warning| !warning.destructive && warning.message.contains("will become optional"))
        );
    }

    #[test]
    fn plan_detects_enum_additions_and_removals() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            enum OfficeType {
                admin
                owner
            }
            "#,
        )
        .expect("schema");
        let current = DatabaseSchema {
            tables: Vec::new(),
            enums: vec![DatabaseEnum {
                name: "OfficeType".to_string(),
                values: vec!["admin".to_string(), "member".to_string()],
            }],
        };

        let plan = plan_schema_migration(&schema, &current);

        assert!(
            plan.steps.iter().any(|step| matches!(step, MigrationStep::AlterEnum(item) if item.name == "OfficeType"))
        );
        assert!(plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("removes values")));
    }

    #[test]
    fn desired_schema_includes_enum_columns_defaults_and_many_to_many_join_tables() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            enum Role {
                USER
                ADMIN
            }

            model User {
                id     Integer @id @default(autoincrement())
                role   Role    @default(USER)
                posts  Post[]
            }

            model Post {
                id     Integer @id @default(autoincrement())
                users  User[]
            }
            "#,
        )
        .expect("schema");

        let migrations = generate_create_table_migrations(&schema);
        let user = migrations.iter().find(|migration| migration.table == "user").expect("user table");
        let role = user.columns.iter().find(|column| column.name == "role").expect("role column");

        assert!(matches!(role.ty, MigrationColumnType::Enum { ref name, .. } if name == "Role"));
        assert_eq!(role.default, Some(MigrationDefault::String("USER".to_string())));
        assert!(migrations.iter().any(|migration| migration.table == "_post_to_user"));
    }

    #[test]
    fn desired_schema_uses_relation_names_to_disambiguate_repeated_many_to_many_relations() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            model User {
                id         Integer @id @default(autoincrement())
                following  User[]  @relation(name: "following")
                followers  User[]  @relation(name: "following")
                blocked    User[]  @relation(name: "blocked")
                blocked_by User[]  @relation(name: "blocked")
            }
            "#,
        )
        .expect("schema");

        let migrations = generate_create_table_migrations(&schema);

        assert!(migrations.iter().any(|migration| migration.table == "_user_to_user_following"));
        assert!(migrations.iter().any(|migration| migration.table == "_user_to_user_blocked"));
        assert_eq!(migrations.iter().filter(|migration| migration.table.starts_with("_user_to_user")).count(), 2);
    }

    #[test]
    fn relation_field_unique_is_materialized_on_its_single_foreign_key_column() {
        let schema = dinoco_compiler::compile(
            r#"
            model User {
                id      Integer  @id
                profile Profile?
            }

            model Profile {
                id      Integer @id
                user_id Integer?
                user    User?   @unique @relation(fields: [user_id], references: [id])
            }
            "#,
        )
        .expect("one-to-one schema");

        let profile = generate_create_table_migrations(&schema)
            .into_iter()
            .find(|migration| migration.table == "profile")
            .expect("profile table");
        assert!(profile.columns.iter().any(|column| column.name == "user_id" && column.unique));
    }

    #[test]
    fn desired_schema_includes_relation_foreign_key_actions() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            model User {
                id    Integer @id @default(autoincrement())
                posts Post[]
            }

            model Post {
                id      Integer @id @default(autoincrement())
                user_id Integer?
                user    User?   @relation(fields: [user_id], references: [id], onDelete: SetNull, onUpdate: Cascade)
            }
            "#,
        )
        .expect("schema");

        let migrations = generate_create_table_migrations(&schema);
        let post = migrations.iter().find(|migration| migration.table == "post").expect("post table");
        let foreign_key = post.foreign_keys.iter().find(|foreign_key| foreign_key.name == "fk_post_user_id").unwrap();

        assert_eq!(foreign_key.on_delete, ReferentialAction::SetNull);
        assert_eq!(foreign_key.on_update, ReferentialAction::Cascade);
    }

    #[test]
    fn plan_detects_column_rename_without_data_loss() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "sqlite"
                database_url = env("DATABASE_URL")
            }

            model User {
                id        String @id
                full_name String
            }
            "#,
        )
        .expect("schema");
        let current = DatabaseSchema {
            tables: vec![DatabaseTable {
                name: "user".to_string(),
                row_count: 5,
                columns: vec![string_column("id", true), string_column("name", false)],
                foreign_keys: Vec::new(),
                indexes: Vec::new(),
            }],
            enums: Vec::new(),
        };

        let plan = plan_schema_migration(&schema, &current);

        assert!(plan.steps.iter().any(
            |step| matches!(step, MigrationStep::RenameColumn(item) if item.from == "name" && item.to == "full_name")
        ));
        assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::DropColumn(_))));
        assert!(!plan.steps.iter().any(|step| matches!(step, MigrationStep::AddColumn(_))));
        assert!(plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("renamed")));
    }

    #[test]
    fn plan_detects_relation_add_remove_and_referential_action_changes() {
        let current_fk = MigrationForeignKey {
            name: "fk_post_user_id".to_string(),
            columns: vec!["user_id".to_string()],
            references_table: "user".to_string(),
            references_columns: vec!["id".to_string()],
            on_update: ReferentialAction::NoAction,
            on_delete: ReferentialAction::NoAction,
        };
        let desired_fk = MigrationForeignKey {
            on_update: ReferentialAction::Cascade,
            on_delete: ReferentialAction::SetNull,
            ..current_fk.clone()
        };

        let current = DatabaseSchema {
            tables: vec![
                table("user", vec![integer_column("id", true)], vec![], 1),
                table(
                    "post",
                    vec![integer_column("id", true), nullable_integer_column("user_id")],
                    vec![current_fk],
                    2,
                ),
                table(
                    "old_relation",
                    vec![integer_column("id", true), integer_column("user_id", false)],
                    vec![MigrationForeignKey {
                        name: "fk_old_relation_user_id".to_string(),
                        columns: vec!["user_id".to_string()],
                        references_table: "user".to_string(),
                        references_columns: vec!["id".to_string()],
                        on_update: ReferentialAction::NoAction,
                        on_delete: ReferentialAction::NoAction,
                    }],
                    0,
                ),
            ],
            enums: Vec::new(),
        };
        let desired = DatabaseSchema {
            tables: vec![
                table("user", vec![integer_column("id", true)], vec![], 0),
                table(
                    "post",
                    vec![integer_column("id", true), nullable_integer_column("user_id")],
                    vec![desired_fk],
                    0,
                ),
                table("old_relation", vec![integer_column("id", true), integer_column("user_id", false)], vec![], 0),
                table(
                    "new_relation",
                    vec![integer_column("id", true), integer_column("user_id", false)],
                    vec![MigrationForeignKey {
                        name: "fk_new_relation_user_id".to_string(),
                        columns: vec!["user_id".to_string()],
                        references_table: "user".to_string(),
                        references_columns: vec!["id".to_string()],
                        on_update: ReferentialAction::Restrict,
                        on_delete: ReferentialAction::Cascade,
                    }],
                    0,
                ),
            ],
            enums: Vec::new(),
        };

        let plan = plan_database_migration(&desired, &current);

        assert!(
            plan.steps
                .iter()
                .any(|step| matches!(step, MigrationStep::DropForeignKey(item) if item.name == "fk_post_user_id"))
        );
        assert!(plan.steps.iter().any(
            |step| matches!(step, MigrationStep::AddForeignKey(item) if item.foreign_key.name == "fk_post_user_id" && item.foreign_key.on_delete == ReferentialAction::SetNull)
        ));
        assert!(
            plan.steps.iter().any(
                |step| matches!(step, MigrationStep::DropForeignKey(item) if item.name == "fk_old_relation_user_id")
            )
        );
        assert!(plan.steps.iter().any(
            |step| matches!(step, MigrationStep::CreateTable(item) if item.table == "new_relation" && item.foreign_keys.len() == 1)
        ));
    }

    #[test]
    fn desired_schema_supports_all_relation_shapes() {
        let schema = dinoco_compiler::compile(
            r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
            }

            model User {
                id         Integer @id @default(autoincrement())
                manager_id Integer?
                manager    User?   @relation(name: "management", fields: [manager_id], references: [id], onDelete: SetNull)
                reports    User[]  @relation(name: "management")
                posts      Post[]
                profile    Profile?
                groups     Group[]
                following  User[]  @relation(name: "following")
                followers  User[]  @relation(name: "following")
            }

            model Post {
                id        Integer @id @default(autoincrement())
                author_id Integer
                author    User    @relation(fields: [author_id], references: [id], onDelete: Cascade)
            }

            model Profile {
                id      Integer @id @default(autoincrement())
                user_id Integer @unique
                user    User    @relation(fields: [user_id], references: [id], onDelete: Cascade)
            }

            model Group {
                id    Integer @id @default(autoincrement())
                users User[]
            }
            "#,
        )
        .expect("schema");

        let migrations = generate_create_table_migrations(&schema);
        let user = migrations.iter().find(|migration| migration.table == "user").expect("user table");
        let post = migrations.iter().find(|migration| migration.table == "post").expect("post table");
        let profile = migrations.iter().find(|migration| migration.table == "profile").expect("profile table");

        assert!(user.foreign_keys.iter().any(|fk| {
            fk.name == "fk_user_manager_id"
                && fk.references_table == "user"
                && fk.on_delete == ReferentialAction::SetNull
        }));
        assert!(post.foreign_keys.iter().any(|fk| {
            fk.name == "fk_post_author_id"
                && fk.references_table == "user"
                && fk.on_delete == ReferentialAction::Cascade
        }));
        assert!(profile.foreign_keys.iter().any(|fk| {
            fk.name == "fk_profile_user_id"
                && fk.references_table == "user"
                && fk.on_delete == ReferentialAction::Cascade
        }));
        assert!(
            profile.columns.iter().any(|column| column.name == "user_id" && column.unique),
            "one-to-one relation uniqueness must be materialized in the database"
        );
        assert!(
            !migrations.iter().any(|migration| migration.table == "_post_to_user"),
            "one-to-many list sides must not create an implicit join table"
        );
        for join_table in ["_group_to_user", "_user_to_user_following"] {
            let join = migrations.iter().find(|migration| migration.table == join_table).expect("many-to-many table");
            assert_eq!(
                join.columns.iter().filter(|column| column.primary_key).count(),
                2,
                "implicit many-to-many tables need a composite primary key"
            );
        }
    }

    fn table(
        name: &str,
        columns: Vec<MigrationColumn>,
        foreign_keys: Vec<MigrationForeignKey>,
        row_count: i64,
    ) -> DatabaseTable {
        DatabaseTable { name: name.to_string(), row_count, columns, foreign_keys, indexes: Vec::new() }
    }

    fn string_column(name: &str, primary_key: bool) -> MigrationColumn {
        MigrationColumn {
            name: name.to_string(),
            ty: MigrationColumnType::String,
            primary_key,
            unique: false,
            nullable: false,
            default: None,
        }
    }

    fn nullable_string_column(name: &str) -> MigrationColumn {
        MigrationColumn {
            name: name.to_string(),
            ty: MigrationColumnType::String,
            primary_key: false,
            unique: false,
            nullable: true,
            default: None,
        }
    }

    fn integer_column(name: &str, primary_key: bool) -> MigrationColumn {
        MigrationColumn {
            name: name.to_string(),
            ty: MigrationColumnType::Integer,
            primary_key,
            unique: false,
            nullable: false,
            default: None,
        }
    }

    fn nullable_integer_column(name: &str) -> MigrationColumn {
        MigrationColumn {
            name: name.to_string(),
            ty: MigrationColumnType::Integer,
            primary_key: false,
            unique: false,
            nullable: true,
            default: None,
        }
    }
}
