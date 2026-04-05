use std::collections::HashSet;

use dinoco_compiler::{ParsedField, ParsedFieldDefault, ParsedSchema};

use crate::{AdapterDialect, DinocoAdapter, MigrationExecutor, MigrationStep, MySqlAdapter};
use crate::{
    find_enum_columns, invert_step, map_referential_action, render_add_foreign_key_clause,
    render_column_definition, render_create_index_sql, render_create_table_sql,
    render_identifier_list,
};

impl MigrationExecutor for MySqlAdapter {
    fn build_migration(
        &self,
        steps: &[MigrationStep],
        schema: &ParsedSchema,
        reverse: bool,
    ) -> Vec<String> {
        let tables_with_primary_key_drop = steps
            .iter()
            .filter_map(|step| match step {
                MigrationStep::AlterColumn {
                    table_name,
                    old_field,
                    new_field,
                } if old_field.is_primary_key && !new_field.is_primary_key => {
                    Some(table_name.as_str())
                }
                MigrationStep::DropPrimaryKey { table_name, .. } => Some(table_name.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let dropped_tables = steps
            .iter()
            .filter_map(|step| match step {
                MigrationStep::DropTable(table_name) => Some(table_name.as_str()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut dropped_primary_keys = HashSet::new();

        let mut sqls = Vec::new();

        for step in steps {
            if let MigrationStep::AlterColumn { table_name, .. } = step {
                if tables_with_primary_key_drop.contains(table_name.as_str())
                    && dropped_primary_keys.insert(table_name.as_str())
                {
                    sqls.push(format!(
                        "ALTER TABLE {} DROP PRIMARY KEY;",
                        self.dialect().identifier(table_name)
                    ));
                }
            }

            if matches!(
                step,
                MigrationStep::DropForeignKey { table_name, .. } if dropped_tables.contains(table_name.as_str())
            ) {
                continue;
            }

            if matches!(
                step,
                MigrationStep::DropPrimaryKey { table_name, .. } if tables_with_primary_key_drop.contains(table_name.as_str())
            ) {
                continue;
            }

            let mut step_sqls = if reverse {
                self.build_reverse_step(step, schema)
            } else {
                self.build_step(step, schema)
            };

            for sql in &mut step_sqls {
                let trimmed = sql.trim_end();

                if !trimmed.ends_with(';') {
                    *sql = format!("{};", trimmed);
                }
            }

            sqls.extend(step_sqls);
        }

        sqls
    }

    fn build_reverse_step(&self, step: &MigrationStep, schema: &ParsedSchema) -> Vec<String> {
        invert_step(step, schema)
            .iter()
            .flat_map(|inverted| self.build_step(inverted, schema))
            .collect()
    }

    fn build_step(&self, step: &MigrationStep, schema: &ParsedSchema) -> Vec<String> {
        let dialect = self.dialect();

        match step {
            MigrationStep::CreateTable(table) => {
                vec![replace_mysql_default(&render_create_table_sql(
                    table, dialect, schema,
                ))]
            }
            MigrationStep::RenameTable { old_name, new_name } => {
                vec![format!(
                    "RENAME TABLE {} TO {}",
                    dialect.identifier(old_name),
                    dialect.identifier(new_name)
                )]
            }
            MigrationStep::DropTable(name) => {
                vec![format!("DROP TABLE {}", dialect.identifier(name))]
            }
            MigrationStep::CreateEnum { .. } | MigrationStep::DropEnum(_) => vec![],
            MigrationStep::AlterEnum { name, .. } => find_enum_columns(schema, name)
                .into_iter()
                .flat_map(|(table, field)| {
                    build_mysql_alter_enum_sql(&table.name, field, schema, dialect)
                })
                .collect(),

            MigrationStep::AddColumn { table_name, field } => vec![format!(
                "ALTER TABLE {} ADD COLUMN {}",
                dialect.identifier(table_name),
                replace_mysql_default(&render_column_definition(field, dialect, schema, true))
            )],
            MigrationStep::DropColumn { table_name, field } => vec![format!(
                "ALTER TABLE {} DROP COLUMN {}",
                dialect.identifier(table_name),
                dialect.identifier(&field.name)
            )],
            MigrationStep::AlterColumn {
                table_name,
                old_field,
                new_field,
            } => {
                let inline_primary_key = !old_field.is_primary_key && new_field.is_primary_key;
                let mut sqls = vec![format!(
                    "ALTER TABLE {} MODIFY COLUMN {}",
                    dialect.identifier(table_name),
                    replace_mysql_default(&render_column_definition(
                        new_field,
                        dialect,
                        schema,
                        inline_primary_key,
                    ))
                )];

                if old_field.is_unique != new_field.is_unique {
                    let unique_name = unique_key_name(table_name, &new_field.name);

                    if new_field.is_unique {
                        sqls.push(format!(
                            "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({})",
                            dialect.identifier(table_name),
                            dialect.identifier(&unique_name),
                            dialect.identifier(&new_field.name)
                        ));
                    } else {
                        sqls.push(format!(
                            "ALTER TABLE {} DROP INDEX {}",
                            dialect.identifier(table_name),
                            dialect.identifier(&unique_name)
                        ));
                    }
                }

                sqls
            }
            MigrationStep::RenameColumn {
                table_name,
                old_name,
                new_name,
            } => vec![format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                dialect.identifier(table_name),
                dialect.identifier(old_name),
                dialect.identifier(new_name)
            )],

            MigrationStep::AddPrimaryKey {
                table_name,
                columns,
                ..
            } => vec![format!(
                "ALTER TABLE {} ADD PRIMARY KEY ({})",
                dialect.identifier(table_name),
                render_identifier_list(
                    &columns
                        .iter()
                        .map(|column| column.as_str())
                        .collect::<Vec<_>>(),
                    dialect
                )
            )],
            MigrationStep::DropPrimaryKey { table_name, .. } => {
                vec![format!(
                    "ALTER TABLE {} DROP PRIMARY KEY",
                    dialect.identifier(table_name)
                )]
            }

            MigrationStep::AddForeignKey {
                table_name,
                columns,
                referenced_table,
                referenced_columns,
                on_delete,
                on_update,
                constraint_name,
            } => vec![render_add_foreign_key_clause(
                table_name,
                columns,
                referenced_table,
                referenced_columns,
                map_referential_action(on_delete),
                map_referential_action(on_update),
                constraint_name,
                dialect,
            )],
            MigrationStep::DropForeignKey {
                table_name,
                constraint_name,
            } => vec![format!(
                "ALTER TABLE {} DROP FOREIGN KEY {}",
                dialect.identifier(table_name),
                dialect.identifier(constraint_name)
            )],

            MigrationStep::CreateIndex {
                table_name,
                columns,
                index_name,
                is_unique,
            } => vec![render_create_index_sql(
                table_name, columns, index_name, *is_unique, dialect,
            )],
            MigrationStep::DropIndex {
                table_name,
                index_name,
            } => vec![format!(
                "DROP INDEX {} ON {}",
                dialect.identifier(index_name),
                dialect.identifier(table_name)
            )],
        }
    }
}

fn unique_key_name(table_name: &str, column_name: &str) -> String {
    format!("uq_{}_{}", table_name, column_name)
}

fn build_mysql_alter_enum_sql(
    table_name: &str,
    field: &ParsedField,
    schema: &ParsedSchema,
    dialect: &crate::MySqlDialect,
) -> Vec<String> {
    let mut sqls = Vec::new();

    if let Some(update_sql) = build_mysql_enum_cleanup_sql(table_name, field, schema, dialect) {
        sqls.push(update_sql);
    }

    sqls.push(format!(
        "ALTER TABLE {} MODIFY COLUMN {}",
        dialect.identifier(table_name),
        replace_mysql_default(&render_column_definition(field, dialect, schema, true))
    ));

    sqls
}

fn build_mysql_enum_cleanup_sql(
    table_name: &str,
    field: &ParsedField,
    schema: &ParsedSchema,
    dialect: &crate::MySqlDialect,
) -> Option<String> {
    let valid_values = enum_valid_values(field, schema)?;
    let fallback = mysql_enum_fallback_sql(field, dialect)?;

    Some(format!(
        "UPDATE {} SET {} = CASE WHEN {} IN ({}) THEN {} ELSE {} END WHERE {} IS NOT NULL AND {} NOT IN ({})",
        dialect.identifier(table_name),
        dialect.identifier(&field.name),
        dialect.identifier(&field.name),
        valid_values,
        dialect.identifier(&field.name),
        fallback,
        dialect.identifier(&field.name),
        dialect.identifier(&field.name),
        valid_values
    ))
}

fn enum_valid_values(field: &ParsedField, schema: &ParsedSchema) -> Option<String> {
    let dinoco_compiler::ParsedFieldType::Enum(enum_name) = &field.field_type else {
        return None;
    };

    let values = schema
        .enums
        .iter()
        .find(|parsed_enum| parsed_enum.name == *enum_name)?
        .values
        .iter()
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .collect::<Vec<_>>();

    if values.is_empty() {
        return None;
    }

    Some(values.join(", "))
}

fn mysql_enum_fallback_sql(field: &ParsedField, dialect: &crate::MySqlDialect) -> Option<String> {
    match &field.default_value {
        ParsedFieldDefault::EnumValue(value) => Some(dialect.literal_string(value)),
        ParsedFieldDefault::NotDefined if field.is_optional => Some("NULL".to_string()),
        _ => None,
    }
}

fn replace_mysql_default(sql: &str) -> String {
    sql.replace("now()", "NOW()")
}
