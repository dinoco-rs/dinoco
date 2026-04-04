use crate::{AdapterDialect, DinocoAdapter, MigrationExecutor, MigrationStep, MySqlAdapter};
use crate::{
    find_enum_columns, invert_step, map_referential_action, render_add_foreign_key_clause,
    render_column_definition, render_create_index_sql, render_create_table_sql,
    render_identifier_list,
};

use dinoco_compiler::ParsedSchema;

impl MigrationExecutor for MySqlAdapter {
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
                .map(|(table, field)| {
                    format!(
                        "ALTER TABLE {} MODIFY COLUMN {}",
                        dialect.identifier(&table.name),
                        replace_mysql_default(&render_column_definition(
                            field, dialect, schema, true
                        ))
                    )
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
                let mut sqls = vec![format!(
                    "ALTER TABLE {} MODIFY COLUMN {}",
                    dialect.identifier(table_name),
                    replace_mysql_default(&render_column_definition(
                        new_field, dialect, schema, true
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

fn replace_mysql_default(sql: &str) -> String {
    sql.replace("now()", "NOW()")
}
