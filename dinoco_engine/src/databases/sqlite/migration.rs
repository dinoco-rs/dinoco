use dinoco_compiler::{ParsedFieldType, ParsedSchema, ParsedTable};
use std::collections::{HashMap, HashSet};

use crate::{AdapterDialect, DinocoAdapter, MigrationExecutor, MigrationStep, SqliteAdapter};
use crate::{
    find_enum_columns, find_table_in_schema, invert_step, render_column_definition, render_create_index_sql,
    render_sqlite_create_table_sql, render_sqlite_rebuild_table_sql,
    render_sqlite_rebuild_table_sql_with_copy_mappings,
};

#[derive(Default)]
struct SqliteRebuildPlan {
    ordered_steps: Vec<MigrationStep>,
}

impl MigrationExecutor for SqliteAdapter {
    fn build_migration(&self, steps: &[MigrationStep], schema: &ParsedSchema, reverse: bool) -> Vec<String> {
        if reverse {
            let mut sqls = Vec::new();

            for step in steps {
                let mut step_sqls = self.build_reverse_step(step, schema);

                for sql in &mut step_sqls {
                    let trimmed = sql.trim_end();

                    if !trimmed.ends_with(';') {
                        *sql = format!("{};", trimmed);
                    }
                }

                sqls.extend(step_sqls);
            }

            return sqls;
        }

        let rebuild_plans = build_rebuild_plans(steps, schema);
        let rebuild_tables = rebuild_plans.keys().cloned().collect::<HashSet<_>>();
        let grouped_rebuild_sql = collect_rebuild_tables(steps, schema)
            .into_iter()
            .flat_map(|table_name| {
                rebuild_plans
                    .get(&table_name)
                    .map(|plan| build_grouped_rebuild_sql(&table_name, plan, schema))
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let mut emitted_rebuilds = false;
        let mut sqls = Vec::new();

        for step in steps {
            if !emitted_rebuilds && should_skip_direct_step(step, &rebuild_tables, schema) {
                if !grouped_rebuild_sql.is_empty() {
                    sqls.push("PRAGMA foreign_keys = OFF".to_string());
                    sqls.extend(grouped_rebuild_sql.clone());
                    sqls.push("PRAGMA foreign_keys = ON".to_string());
                }
                emitted_rebuilds = true;
            }

            if should_skip_direct_step(step, &rebuild_tables, schema) {
                continue;
            }

            sqls.extend(self.build_step(step, schema));
        }

        if !emitted_rebuilds && !grouped_rebuild_sql.is_empty() {
            sqls.push("PRAGMA foreign_keys = OFF".to_string());
            sqls.extend(grouped_rebuild_sql);
            sqls.push("PRAGMA foreign_keys = ON".to_string());
        }

        for sql in &mut sqls {
            let trimmed = sql.trim_end();

            if !trimmed.ends_with(';') {
                *sql = format!("{};", trimmed);
            }
        }

        sqls
    }

    fn build_reverse_step(&self, step: &MigrationStep, schema: &ParsedSchema) -> Vec<String> {
        invert_step(step, schema).iter().flat_map(|inverted| self.build_step(inverted, schema)).collect()
    }

    fn build_step(&self, step: &MigrationStep, schema: &ParsedSchema) -> Vec<String> {
        let dialect = self.dialect();

        match step {
            MigrationStep::CreateTable(table) => {
                vec![replace_sqlite_default(&render_sqlite_create_table_sql(table, dialect, schema))]
            }
            MigrationStep::RenameTable { old_name, new_name } => {
                vec![format!("ALTER TABLE {} RENAME TO {}", dialect.identifier(old_name), dialect.identifier(new_name))]
            }
            MigrationStep::DropTable(name) => {
                vec![format!("DROP TABLE {}", dialect.identifier(name))]
            }

            MigrationStep::CreateEnum { .. } | MigrationStep::DropEnum(_) => vec![],
            MigrationStep::AlterEnum { name, .. } => find_enum_columns(schema, name)
                .into_iter()
                .flat_map(|(table, _)| rebuild_table_sql(&table.name, schema, table_column_names(table)))
                .collect(),

            MigrationStep::AddColumn { table_name, field } => {
                if field.is_primary_key || field.is_unique {
                    let preserved_columns = find_table_in_schema(schema, table_name)
                        .map(table_column_names)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|column| column != &field.name)
                        .collect::<Vec<_>>();

                    rebuild_table_sql(table_name, schema, preserved_columns)
                } else {
                    vec![format!(
                        "ALTER TABLE {} ADD COLUMN {}",
                        dialect.identifier(table_name),
                        replace_sqlite_default(&render_column_definition(field, dialect, schema, true))
                    )]
                }
            }
            MigrationStep::DropColumn { table_name, field } => {
                let preserved_columns = find_table_in_schema(schema, table_name)
                    .map(table_column_names)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|column| column != &field.name)
                    .collect::<Vec<_>>();

                rebuild_table_sql(table_name, schema, preserved_columns)
            }
            MigrationStep::AlterColumn { table_name, old_field, new_field } => {
                let preserved_columns = find_table_in_schema(schema, table_name)
                    .map(table_column_names)
                    .unwrap_or_else(|| vec![old_field.name.clone(), new_field.name.clone()]);

                rebuild_table_sql(table_name, schema, preserved_columns)
            }
            MigrationStep::RenameColumn { table_name, old_name, new_name } => vec![format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                dialect.identifier(table_name),
                dialect.identifier(old_name),
                dialect.identifier(new_name)
            )],

            MigrationStep::AddPrimaryKey { table_name, .. }
            | MigrationStep::DropPrimaryKey { table_name, .. }
            | MigrationStep::AddForeignKey { table_name, .. }
            | MigrationStep::DropForeignKey { table_name, .. } => rebuild_table_sql(
                table_name,
                schema,
                find_table_in_schema(schema, table_name).map(table_column_names).unwrap_or_default(),
            ),

            MigrationStep::CreateIndex { table_name, columns, index_name, is_unique } => {
                vec![render_create_index_sql(table_name, columns, index_name, *is_unique, dialect)]
            }
            MigrationStep::DropIndex { index_name, .. } => {
                vec![format!("DROP INDEX {}", dialect.identifier(index_name))]
            }
        }
    }
}

fn rebuild_table_sql(table_name: &str, schema: &ParsedSchema, preserved_columns: Vec<String>) -> Vec<String> {
    let dialect = crate::SqliteDialect;

    find_table_in_schema(schema, table_name)
        .map(|table| {
            render_sqlite_rebuild_table_sql(table, &dialect, schema, &preserved_columns)
                .into_iter()
                .map(|sql| replace_sqlite_default(&sql))
                .collect()
        })
        .unwrap_or_default()
}

fn table_column_names(table: &ParsedTable) -> Vec<String> {
    table
        .fields
        .iter()
        .filter(|field| !matches!(field.field_type, ParsedFieldType::Relation(..)))
        .map(|field| field.name.clone())
        .collect()
}

fn replace_sqlite_default(sql: &str) -> String {
    sql.replace("now()", "CURRENT_TIMESTAMP")
}

fn build_grouped_rebuild_sql(table_name: &str, plan: &SqliteRebuildPlan, schema: &ParsedSchema) -> Vec<String> {
    let dialect = crate::SqliteDialect;

    find_table_in_schema(schema, table_name)
        .map(|table| {
            render_sqlite_rebuild_table_sql_with_copy_mappings(
                table,
                &dialect,
                schema,
                &compute_copy_mappings(table, &plan.ordered_steps, schema, &dialect),
            )
            .into_iter()
            .map(|sql| replace_sqlite_default(&sql))
            .collect()
        })
        .unwrap_or_default()
}

fn build_rebuild_plans(steps: &[MigrationStep], schema: &ParsedSchema) -> HashMap<String, SqliteRebuildPlan> {
    let rebuild_tables = collect_rebuild_tables(steps, schema);
    let mut plans = rebuild_tables
        .iter()
        .cloned()
        .map(|table_name| (table_name, SqliteRebuildPlan::default()))
        .collect::<HashMap<_, _>>();

    for step in steps {
        match step {
            MigrationStep::AlterEnum { name, .. } => {
                for (table, _) in find_enum_columns(schema, name) {
                    if let Some(plan) = plans.get_mut(&table.name) {
                        plan.ordered_steps.push(step.clone());
                    }
                }
            }
            MigrationStep::AddColumn { table_name, .. }
            | MigrationStep::DropColumn { table_name, .. }
            | MigrationStep::AlterColumn { table_name, .. }
            | MigrationStep::RenameColumn { table_name, .. }
            | MigrationStep::AddPrimaryKey { table_name, .. }
            | MigrationStep::DropPrimaryKey { table_name, .. }
            | MigrationStep::AddForeignKey { table_name, .. }
            | MigrationStep::DropForeignKey { table_name, .. } => {
                if let Some(plan) = plans.get_mut(table_name) {
                    plan.ordered_steps.push(step.clone());
                }
            }
            _ => {}
        }
    }

    plans
}

fn collect_rebuild_tables(steps: &[MigrationStep], schema: &ParsedSchema) -> Vec<String> {
    let mut ordered_tables = Vec::new();
    let mut seen = HashSet::new();

    for step in steps {
        match step {
            MigrationStep::AlterEnum { name, .. } => {
                for (table, _) in find_enum_columns(schema, name) {
                    push_table_name(&mut ordered_tables, &mut seen, &table.name);
                }
            }
            MigrationStep::AddColumn { table_name, field } => {
                if field.is_primary_key || field.is_unique {
                    push_table_name(&mut ordered_tables, &mut seen, table_name);
                }
            }
            MigrationStep::DropColumn { table_name, .. }
            | MigrationStep::AlterColumn { table_name, .. }
            | MigrationStep::AddPrimaryKey { table_name, .. }
            | MigrationStep::DropPrimaryKey { table_name, .. }
            | MigrationStep::AddForeignKey { table_name, .. }
            | MigrationStep::DropForeignKey { table_name, .. } => {
                push_table_name(&mut ordered_tables, &mut seen, table_name);
            }
            _ => {}
        }
    }

    ordered_tables
}

fn should_skip_direct_step(step: &MigrationStep, rebuild_tables: &HashSet<String>, schema: &ParsedSchema) -> bool {
    match step {
        MigrationStep::AlterEnum { name, .. } => {
            find_enum_columns(schema, name).into_iter().any(|(table, _)| rebuild_tables.contains(&table.name))
        }
        MigrationStep::AddColumn { table_name, .. }
        | MigrationStep::DropColumn { table_name, .. }
        | MigrationStep::AlterColumn { table_name, .. }
        | MigrationStep::AddPrimaryKey { table_name, .. }
        | MigrationStep::DropPrimaryKey { table_name, .. }
        | MigrationStep::AddForeignKey { table_name, .. }
        | MigrationStep::DropForeignKey { table_name, .. } => rebuild_tables.contains(table_name),
        MigrationStep::RenameColumn { table_name, .. } => rebuild_tables.contains(table_name),
        _ => false,
    }
}

fn compute_copy_mappings(
    table: &ParsedTable,
    steps: &[MigrationStep],
    schema: &ParsedSchema,
    dialect: &crate::SqliteDialect,
) -> Vec<(String, String)> {
    let mut mappings =
        table_column_names(table).into_iter().map(|column| (column.clone(), Some(column))).collect::<Vec<_>>();

    for step in steps.iter().rev() {
        match step {
            MigrationStep::AddColumn { field, .. } => {
                remove_added_column_mappings(&mut mappings, &field.name);
            }
            MigrationStep::AlterColumn { old_field, new_field, .. } => {
                replace_mapping_source(&mut mappings, &new_field.name, &old_field.name);
            }
            MigrationStep::RenameColumn { old_name, new_name, .. } => {
                replace_mapping_source(&mut mappings, new_name, old_name);
            }
            _ => {}
        }
    }

    mappings
        .into_iter()
        .filter_map(|(target, source)| {
            source.map(|source| {
                let source_expression = build_copy_expression(table, &target, &source, steps, schema, dialect);

                (target, source_expression)
            })
        })
        .collect()
}

fn build_copy_expression(
    table: &ParsedTable,
    target_column: &str,
    source_column: &str,
    steps: &[MigrationStep],
    schema: &ParsedSchema,
    dialect: &crate::SqliteDialect,
) -> String {
    let Some(field) = table.fields.iter().find(|field| field.name == target_column) else {
        return dialect.identifier(source_column);
    };

    let ParsedFieldType::Enum(enum_name) = &field.field_type else {
        return dialect.identifier(source_column);
    };

    let enum_was_altered =
        steps.iter().any(|step| matches!(step, MigrationStep::AlterEnum { name, .. } if name == enum_name));
    if !enum_was_altered {
        return dialect.identifier(source_column);
    }

    let valid_values = schema
        .enums
        .iter()
        .find(|parsed_enum| parsed_enum.name == *enum_name)
        .map(|parsed_enum| parsed_enum.values.clone())
        .unwrap_or_default();

    if valid_values.is_empty() {
        return dialect.identifier(source_column);
    }

    let fallback = match &field.default_value {
        dinoco_compiler::ParsedFieldDefault::EnumValue(value) => dialect.literal_string(value),
        dinoco_compiler::ParsedFieldDefault::NotDefined if field.is_optional => "NULL".to_string(),
        _ => return dialect.identifier(source_column),
    };

    format!(
        "CASE WHEN {} IN ({}) THEN {} ELSE {} END",
        dialect.identifier(source_column),
        valid_values.iter().map(|value| dialect.literal_string(value)).collect::<Vec<_>>().join(", "),
        dialect.identifier(source_column),
        fallback
    )
}

fn remove_added_column_mappings(mappings: &mut Vec<(String, Option<String>)>, column_name: &str) {
    for (_, source) in mappings.iter_mut() {
        if source.as_deref() == Some(column_name) {
            *source = None;
        }
    }

    if let Some((_, source)) = mappings.iter_mut().find(|(target, _)| target == column_name) {
        *source = None;
    }
}

fn replace_mapping_source(mappings: &mut Vec<(String, Option<String>)>, current_source: &str, previous_source: &str) {
    for (_, source) in mappings.iter_mut() {
        if source.as_deref() == Some(current_source) {
            *source = Some(previous_source.to_string());
        }
    }
}

fn push_table_name(ordered_tables: &mut Vec<String>, seen: &mut HashSet<String>, table_name: &str) {
    if seen.insert(table_name.to_string()) {
        ordered_tables.push(table_name.to_string());
    }
}
