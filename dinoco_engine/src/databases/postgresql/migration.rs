use crate::{AdapterDialect, DinocoAdapter, MigrationExecutor, MigrationStep, PostgresAdapter};
use crate::{
    find_enum_columns, invert_step, map_field_to_definition, map_referential_action,
    render_add_foreign_key_clause, render_column_default_from_mapped, render_create_index_sql,
    render_create_table_sql, render_identifier_list,
};
use dinoco_compiler::{ParsedField, ParsedSchema};

impl MigrationExecutor for PostgresAdapter {
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
                vec![replace_postgres_default(&render_create_table_sql(
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

            MigrationStep::CreateEnum { name, variants } => {
                vec![format!(
                    "CREATE TYPE {} AS ENUM ({})",
                    dialect.identifier(name),
                    render_enum_values(variants, dialect)
                )]
            }
            MigrationStep::AlterEnum {
                name,
                old_variants,
                new_variants,
            } => build_postgres_alter_enum_sql(name, old_variants, new_variants, schema, dialect),
            MigrationStep::DropEnum(name) => {
                vec![format!("DROP TYPE {}", dialect.identifier(name))]
            }

            MigrationStep::AddColumn { table_name, field } => vec![format!(
                "ALTER TABLE {} ADD COLUMN {}",
                dialect.identifier(table_name),
                replace_postgres_default(&crate::render_column_definition(
                    field, dialect, schema, true
                ))
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
            } => build_postgres_alter_column_sql(table_name, old_field, new_field, schema, dialect),
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
                constraint_name,
            } => vec![format!(
                "ALTER TABLE {} ADD CONSTRAINT {} PRIMARY KEY ({})",
                dialect.identifier(table_name),
                dialect.identifier(&primary_key_name(table_name, constraint_name)),
                render_identifier_list(
                    &columns
                        .iter()
                        .map(|column| column.as_str())
                        .collect::<Vec<_>>(),
                    dialect
                )
            )],
            MigrationStep::DropPrimaryKey {
                table_name,
                constraint_name,
            } => vec![format!(
                "ALTER TABLE {} DROP CONSTRAINT {}",
                dialect.identifier(table_name),
                dialect.identifier(&primary_key_name(table_name, constraint_name))
            )],

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
                "ALTER TABLE {} DROP CONSTRAINT {}",
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
            MigrationStep::DropIndex { index_name, .. } => {
                vec![format!("DROP INDEX {}", dialect.identifier(index_name))]
            }
        }
    }
}

fn build_postgres_alter_column_sql(
    table_name: &str,
    old_field: &ParsedField,
    new_field: &ParsedField,
    schema: &ParsedSchema,
    dialect: &crate::PostgresDialect,
) -> Vec<String> {
    let mut sqls = Vec::new();
    let table_ident = dialect.identifier(table_name);
    let column_ident = dialect.identifier(&new_field.name);
    let new_definition = map_field_to_definition(new_field, dialect, &schema.enums);
    let new_type = dialect.column_type(&new_definition, false, false);

    sqls.push(format!(
        "ALTER TABLE {} ALTER COLUMN {} TYPE {} USING {}::text::{}",
        table_ident, column_ident, new_type, column_ident, new_type
    ));

    if old_field.is_optional != new_field.is_optional {
        sqls.push(format!(
            "ALTER TABLE {} ALTER COLUMN {} {} NOT NULL",
            table_ident,
            column_ident,
            if new_field.is_optional { "DROP" } else { "SET" }
        ));
    }

    if old_field.default_value != new_field.default_value {
        if let Some(default_sql) = render_column_default_sql(new_field, dialect, schema) {
            sqls.push(format!(
                "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                table_ident, column_ident, default_sql
            ));
        } else {
            sqls.push(format!(
                "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT",
                table_ident, column_ident
            ));
        }
    }

    if old_field.is_unique != new_field.is_unique {
        let constraint_name = unique_constraint_name(table_name, &new_field.name);

        if new_field.is_unique {
            sqls.push(format!(
                "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({})",
                table_ident,
                dialect.identifier(&constraint_name),
                column_ident
            ));
        } else {
            sqls.push(format!(
                "ALTER TABLE {} DROP CONSTRAINT {}",
                table_ident,
                dialect.identifier(&constraint_name)
            ));
        }
    }

    sqls
}

fn build_postgres_alter_enum_sql(
    enum_name: &str,
    old_variants: &[String],
    new_variants: &[String],
    schema: &ParsedSchema,
    dialect: &crate::PostgresDialect,
) -> Vec<String> {
    let removed_variants = old_variants
        .iter()
        .filter(|variant| !new_variants.contains(variant))
        .collect::<Vec<_>>();

    if removed_variants.is_empty() {
        return new_variants
            .iter()
            .filter(|variant| !old_variants.contains(variant))
            .map(|variant| {
                format!(
                    "ALTER TYPE {} ADD VALUE IF NOT EXISTS {}",
                    dialect.identifier(enum_name),
                    dialect.literal_string(variant)
                )
            })
            .collect();
    }

    let old_type_name = format!("{}_old", enum_name);
    let mut sqls = vec![
        format!(
            "ALTER TYPE {} RENAME TO {}",
            dialect.identifier(enum_name),
            dialect.identifier(&old_type_name)
        ),
        format!(
            "CREATE TYPE {} AS ENUM ({})",
            dialect.identifier(enum_name),
            render_enum_values(new_variants, dialect)
        ),
    ];

    for (table, field) in find_enum_columns(schema, enum_name) {
        let table_ident = dialect.identifier(&table.name);
        let column_ident = dialect.identifier(&field.name);
        let using_expression = build_postgres_enum_using_expression(&field, schema, dialect)
            .unwrap_or_else(|| {
                format!("{}::text::{}", column_ident, dialect.identifier(enum_name))
            });

        sqls.push(format!(
            "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT",
            table_ident, column_ident
        ));
        sqls.push(format!(
            "ALTER TABLE {} ALTER COLUMN {} TYPE {} USING {}",
            table_ident,
            column_ident,
            dialect.identifier(enum_name),
            using_expression
        ));

        if let Some(default_sql) = render_column_default_sql(field, dialect, schema) {
            sqls.push(format!(
                "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                table_ident, column_ident, default_sql
            ));
        }
    }

    sqls.push(format!("DROP TYPE {}", dialect.identifier(&old_type_name)));

    sqls
}

fn build_postgres_enum_using_expression(
    field: &ParsedField,
    schema: &ParsedSchema,
    dialect: &crate::PostgresDialect,
) -> Option<String> {
    let dinoco_compiler::ParsedFieldType::Enum(enum_name) = &field.field_type else {
        return None;
    };

    let valid_values = schema
        .enums
        .iter()
        .find(|parsed_enum| parsed_enum.name == *enum_name)?
        .values
        .iter()
        .map(|value| dialect.literal_string(value))
        .collect::<Vec<_>>();

    if valid_values.is_empty() {
        return None;
    }

    let fallback = match &field.default_value {
        dinoco_compiler::ParsedFieldDefault::EnumValue(value) => dialect.literal_string(value),
        dinoco_compiler::ParsedFieldDefault::NotDefined if field.is_optional => "NULL".to_string(),
        _ => return None,
    };

    let column_ident = dialect.identifier(&field.name);

    Some(format!(
        "(CASE WHEN {}::text IN ({}) THEN {}::text ELSE {} END)::{}",
        column_ident,
        valid_values.join(", "),
        column_ident,
        fallback,
        dialect.identifier(enum_name)
    ))
}

fn render_column_default_sql(
    field: &ParsedField,
    dialect: &crate::PostgresDialect,
    schema: &ParsedSchema,
) -> Option<String> {
    let definition = map_field_to_definition(field, dialect, &schema.enums);
    definition.default.as_ref().map(|default| {
        replace_postgres_default(&render_column_default_from_mapped(default, dialect))
    })
}

fn render_enum_values(values: &[String], dialect: &crate::PostgresDialect) -> String {
    values
        .iter()
        .map(|value| dialect.literal_string(value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn unique_constraint_name(table_name: &str, column_name: &str) -> String {
    format!("uq_{}_{}", table_name, column_name)
}

fn primary_key_name(table_name: &str, constraint_name: &Option<String>) -> String {
    constraint_name
        .clone()
        .unwrap_or_else(|| format!("{}_pkey", table_name))
}

fn replace_postgres_default(sql: &str) -> String {
    sql.replace("now()", "NOW()")
}
