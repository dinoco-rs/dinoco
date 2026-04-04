use crate::AdapterDialect;
use crate::{
    DinocoAdapter, MigrationExecutor, MigrationStep, SqliteAdapter, find_enum_columns,
    find_table_in_schema, invert_step, render_column_definition, render_create_index_sql,
    render_sqlite_create_table_sql, render_sqlite_rebuild_table_sql,
};
use dinoco_compiler::{ParsedFieldType, ParsedSchema, ParsedTable};

impl MigrationExecutor for SqliteAdapter {
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
                vec![replace_sqlite_default(&render_sqlite_create_table_sql(
                    table, dialect, schema,
                ))]
            }
            MigrationStep::RenameTable { old_name, new_name } => vec![format!(
                "ALTER TABLE {} RENAME TO {}",
                dialect.identifier(old_name),
                dialect.identifier(new_name)
            )],
            MigrationStep::DropTable(name) => {
                vec![format!("DROP TABLE {}", dialect.identifier(name))]
            }

            MigrationStep::CreateEnum { .. } | MigrationStep::DropEnum(_) => vec![],
            MigrationStep::AlterEnum { name, .. } => find_enum_columns(schema, name)
                .into_iter()
                .flat_map(|(table, _)| {
                    rebuild_table_sql(&table.name, schema, table_column_names(table))
                })
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
                        replace_sqlite_default(&render_column_definition(
                            field, dialect, schema, true
                        ))
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
            MigrationStep::AlterColumn {
                table_name,
                old_field,
                new_field,
            } => {
                let preserved_columns = find_table_in_schema(schema, table_name)
                    .map(table_column_names)
                    .unwrap_or_else(|| vec![old_field.name.clone(), new_field.name.clone()]);

                rebuild_table_sql(table_name, schema, preserved_columns)
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

            MigrationStep::AddPrimaryKey { table_name, .. }
            | MigrationStep::DropPrimaryKey { table_name, .. }
            | MigrationStep::AddForeignKey { table_name, .. }
            | MigrationStep::DropForeignKey { table_name, .. } => rebuild_table_sql(
                table_name,
                schema,
                find_table_in_schema(schema, table_name)
                    .map(table_column_names)
                    .unwrap_or_default(),
            ),

            MigrationStep::CreateIndex {
                table_name,
                columns,
                index_name,
                is_unique,
            } => vec![render_create_index_sql(
                table_name, columns, index_name, *is_unique, dialect,
            )],
            MigrationStep::DropIndex { index_name, .. } => {
                vec![format!("DROP INDEX {}", dialect.identifier(index_name))]
            }
        }
    }
}

fn rebuild_table_sql(
    table_name: &str,
    schema: &ParsedSchema,
    preserved_columns: Vec<String>,
) -> Vec<String> {
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
