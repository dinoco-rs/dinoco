use crate::{AdapterDialect, ColumnDefault, DinocoValue, MigrationStep, map_field_to_definition};
use dinoco_compiler::{
    FunctionCall, ParsedField, ParsedFieldDefault, ParsedFieldType, ParsedRelation, ParsedSchema,
    ParsedTable,
};

pub fn invert_step(step: &MigrationStep, schema: &ParsedSchema) -> Vec<MigrationStep> {
    match step {
        MigrationStep::CreateTable(table) => vec![MigrationStep::DropTable(table.name.clone())],
        MigrationStep::RenameTable { old_name, new_name } => vec![MigrationStep::RenameTable {
            old_name: new_name.clone(),
            new_name: old_name.clone(),
        }],
        MigrationStep::DropTable(name) => schema
            .tables
            .iter()
            .find(|table| table.name == *name)
            .cloned()
            .map(MigrationStep::CreateTable)
            .into_iter()
            .collect(),

        MigrationStep::CreateEnum { name, .. } => vec![MigrationStep::DropEnum(name.clone())],
        MigrationStep::AlterEnum {
            name,
            old_variants,
            new_variants,
        } => vec![MigrationStep::AlterEnum {
            name: name.clone(),
            old_variants: new_variants.clone(),
            new_variants: old_variants.clone(),
        }],
        MigrationStep::DropEnum(name) => schema
            .enums
            .iter()
            .find(|parsed_enum| parsed_enum.name == *name)
            .map(|parsed_enum| MigrationStep::CreateEnum {
                name: parsed_enum.name.clone(),
                variants: parsed_enum.values.clone(),
            })
            .into_iter()
            .collect(),

        MigrationStep::AddColumn { table_name, field } => vec![MigrationStep::DropColumn {
            table_name: table_name.clone(),
            field: field.clone(),
        }],
        MigrationStep::DropColumn { table_name, field } => vec![MigrationStep::AddColumn {
            table_name: table_name.clone(),
            field: field.clone(),
        }],
        MigrationStep::RenameColumn {
            table_name,
            old_name,
            new_name,
        } => vec![MigrationStep::RenameColumn {
            table_name: table_name.clone(),
            old_name: new_name.clone(),
            new_name: old_name.clone(),
        }],
        MigrationStep::AlterColumn {
            table_name,
            old_field,
            new_field,
        } => vec![MigrationStep::AlterColumn {
            table_name: table_name.clone(),
            old_field: new_field.clone(),
            new_field: old_field.clone(),
        }],

        MigrationStep::AddPrimaryKey {
            table_name,
            constraint_name,
            ..
        } => vec![MigrationStep::DropPrimaryKey {
            table_name: table_name.clone(),
            constraint_name: constraint_name.clone(),
        }],
        MigrationStep::DropPrimaryKey {
            table_name,
            constraint_name,
        } => {
            let columns = schema
                .tables
                .iter()
                .find(|table| table.name == *table_name)
                .map(|table| {
                    table
                        .fields
                        .iter()
                        .filter(|field| field.is_primary_key)
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if columns.is_empty() {
                vec![]
            } else {
                vec![MigrationStep::AddPrimaryKey {
                    table_name: table_name.clone(),
                    columns,
                    constraint_name: constraint_name.clone(),
                }]
            }
        }

        MigrationStep::AddForeignKey {
            table_name,
            constraint_name,
            ..
        } => vec![MigrationStep::DropForeignKey {
            table_name: table_name.clone(),
            constraint_name: constraint_name.clone(),
        }],
        MigrationStep::DropForeignKey {
            table_name,
            constraint_name,
        } => find_foreign_key_in_schema(table_name, constraint_name, schema)
            .into_iter()
            .collect(),

        MigrationStep::CreateIndex {
            table_name,
            index_name,
            ..
        } => vec![MigrationStep::DropIndex {
            table_name: table_name.clone(),
            index_name: index_name.clone(),
        }],
        MigrationStep::DropIndex { .. } => vec![],
    }
}

pub fn invert_steps(steps: &[MigrationStep], schema: &ParsedSchema) -> Vec<MigrationStep> {
    let mut inverted = Vec::new();

    for step in steps.iter().rev() {
        inverted.extend(invert_step(step, schema));
    }

    inverted
}

fn find_foreign_key_in_schema(
    table_name: &str,
    constraint_name: &str,
    schema: &ParsedSchema,
) -> Option<MigrationStep> {
    let table = schema
        .tables
        .iter()
        .find(|table| table.name == table_name)?;

    table.fields.iter().find_map(|field| {
        let (columns, referenced_columns, on_delete, on_update) = match &field.relation {
            dinoco_compiler::ParsedRelation::ManyToOne(
                _,
                columns,
                referenced_columns,
                on_delete,
                on_update,
            )
            | dinoco_compiler::ParsedRelation::OneToOneOwner(
                _,
                columns,
                referenced_columns,
                on_delete,
                on_update,
            ) => (columns, referenced_columns, on_delete, on_update),
            _ => return None,
        };

        let expected_constraint = columns
            .first()
            .map(|column| format!("fk_{}_{}", table_name, column))?;

        if expected_constraint != constraint_name {
            return None;
        }

        let referenced_table = match &field.field_type {
            ParsedFieldType::Relation(name) => name.clone(),
            other => other.to_string(),
        };

        Some(MigrationStep::AddForeignKey {
            table_name: table_name.to_string(),
            columns: columns.clone(),
            referenced_table,
            referenced_columns: referenced_columns.clone(),
            on_delete: on_delete.clone(),
            on_update: on_update.clone(),
            constraint_name: constraint_name.to_string(),
        })
    })
}

pub fn render_column_definition<D: AdapterDialect>(
    field: &ParsedField,
    dialect: &D,
    schema: &ParsedSchema,
    inline_primary_key: bool,
) -> String {
    let mut definition = map_field_to_definition(field, dialect, &schema.enums);
    definition.primary_key = inline_primary_key && field.is_primary_key;

    let mut parts = vec![
        dialect.identifier(&field.name),
        dialect.column_type(
            &definition,
            definition.primary_key,
            definition.auto_increment,
        ),
    ];

    if definition.not_null && !definition.primary_key {
        parts.push("NOT NULL".to_string());
    }

    if let Some(default_sql) = render_default_sql(&field.default_value, dialect) {
        parts.push(format!("DEFAULT {}", default_sql));
    }

    if field.is_unique && !field.is_primary_key {
        parts.push("UNIQUE".to_string());
    }

    parts.join(" ")
}

pub fn render_create_table_sql<D: AdapterDialect>(
    table: &ParsedTable,
    dialect: &D,
    schema: &ParsedSchema,
) -> String {
    let data_fields = table
        .fields
        .iter()
        .filter(|field| !matches!(field.field_type, ParsedFieldType::Relation(..)))
        .collect::<Vec<_>>();

    let primary_key_columns = data_fields
        .iter()
        .filter(|field| field.is_primary_key)
        .collect::<Vec<_>>();
    let inline_primary_key = primary_key_columns.len() <= 1;

    let mut definitions = data_fields
        .iter()
        .map(|field| render_column_definition(field, dialect, schema, inline_primary_key))
        .collect::<Vec<_>>();

    if !inline_primary_key && !primary_key_columns.is_empty() {
        definitions.push(format!(
            "PRIMARY KEY ({})",
            render_identifier_list(
                &primary_key_columns
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>(),
                dialect
            )
        ));
    }

    format!(
        "CREATE TABLE {} ({})",
        dialect.identifier(&table.name),
        definitions.join(", ")
    )
}

pub fn render_sqlite_create_table_sql<D: AdapterDialect>(
    table: &ParsedTable,
    dialect: &D,
    schema: &ParsedSchema,
) -> String {
    let base_sql = render_create_table_sql(table, dialect, schema);
    let inline_fks = sqlite_inline_foreign_keys(table, dialect);

    if inline_fks.is_empty() {
        base_sql
    } else {
        base_sql.strip_suffix(')').unwrap_or(&base_sql).to_string()
            + ", "
            + &inline_fks.join(", ")
            + ")"
    }
}

pub fn render_sqlite_rebuild_table_sql<D: AdapterDialect>(
    table: &ParsedTable,
    dialect: &D,
    schema: &ParsedSchema,
    preserved_columns: &[String],
) -> Vec<String> {
    let copy_mappings = preserved_columns
        .iter()
        .cloned()
        .map(|column| (column.clone(), column))
        .collect::<Vec<_>>();
    let mut sqls = vec!["PRAGMA foreign_keys = OFF".to_string()];

    sqls.extend(render_sqlite_rebuild_table_sql_with_copy_mappings(
        table,
        dialect,
        schema,
        &copy_mappings,
    ));
    sqls.push("PRAGMA foreign_keys = ON".to_string());

    sqls
}

pub fn render_sqlite_rebuild_table_sql_with_copy_mappings<D: AdapterDialect>(
    table: &ParsedTable,
    dialect: &D,
    schema: &ParsedSchema,
    copy_mappings: &[(String, String)],
) -> Vec<String> {
    let temp_table_name = format!("__dinoco_rebuild_{}", table.name);
    let mut sqls = vec![render_sqlite_create_table_sql(
        &ParsedTable {
            name: temp_table_name.clone(),
            fields: table.fields.clone(),
        },
        dialect,
        schema,
    )];

    if !copy_mappings.is_empty() {
        let target_columns_sql = render_identifier_list(
            &copy_mappings
                .iter()
                .map(|(target, _)| target.as_str())
                .collect::<Vec<_>>(),
            dialect,
        );
        let source_columns_sql = copy_mappings
            .iter()
            .map(|(_, source)| source.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        sqls.push(format!(
            "INSERT INTO {} ({}) SELECT {} FROM {}",
            dialect.identifier(&temp_table_name),
            target_columns_sql,
            source_columns_sql,
            dialect.identifier(&table.name)
        ));
    }

    sqls.push(format!("DROP TABLE {}", dialect.identifier(&table.name)));
    sqls.push(format!(
        "ALTER TABLE {} RENAME TO {}",
        dialect.identifier(&temp_table_name),
        dialect.identifier(&table.name)
    ));

    sqls
}

pub fn render_add_foreign_key_clause<D: AdapterDialect>(
    table_name: &str,
    columns: &[String],
    referenced_table: &str,
    referenced_columns: &[String],
    on_delete: Option<&'static str>,
    on_update: Option<&'static str>,
    constraint_name: &str,
    dialect: &D,
) -> String {
    let mut sql = format!(
        "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
        dialect.identifier(table_name),
        dialect.identifier(constraint_name),
        render_identifier_list(
            &columns
                .iter()
                .map(|column| column.as_str())
                .collect::<Vec<_>>(),
            dialect
        ),
        dialect.identifier(referenced_table),
        render_identifier_list(
            &referenced_columns
                .iter()
                .map(|column| column.as_str())
                .collect::<Vec<_>>(),
            dialect
        )
    );

    if let Some(on_delete) = on_delete {
        sql.push_str(&format!(" ON DELETE {}", on_delete));
    }

    if let Some(on_update) = on_update {
        sql.push_str(&format!(" ON UPDATE {}", on_update));
    }

    sql
}

pub fn render_create_index_sql<D: AdapterDialect>(
    table_name: &str,
    columns: &[String],
    index_name: &str,
    is_unique: bool,
    dialect: &D,
) -> String {
    format!(
        "CREATE {}INDEX {} ON {} ({})",
        if is_unique { "UNIQUE " } else { "" },
        dialect.identifier(index_name),
        dialect.identifier(table_name),
        render_identifier_list(
            &columns
                .iter()
                .map(|column| column.as_str())
                .collect::<Vec<_>>(),
            dialect
        )
    )
}

pub fn render_identifier_list<D: AdapterDialect>(items: &[&str], dialect: &D) -> String {
    items
        .iter()
        .map(|item| dialect.identifier(item))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn render_default_sql<D: AdapterDialect>(
    default: &ParsedFieldDefault,
    dialect: &D,
) -> Option<String> {
    match default {
        ParsedFieldDefault::NotDefined => None,
        ParsedFieldDefault::String(value) => Some(dialect.literal_string(value)),
        ParsedFieldDefault::Boolean(value) => Some(if *value {
            "TRUE".to_string()
        } else {
            "FALSE".to_string()
        }),
        ParsedFieldDefault::Integer(value) => Some(value.to_string()),
        ParsedFieldDefault::Float(value) => Some(value.to_string()),
        ParsedFieldDefault::EnumValue(value) => Some(dialect.literal_string(value)),
        ParsedFieldDefault::Function(function) => match function {
            FunctionCall::Now => Some("now()".to_string()),
            FunctionCall::AutoIncrement | FunctionCall::Uuid | FunctionCall::Snowflake => None,
            FunctionCall::Env(..) => None,
        },
    }
}

pub fn render_value_sql<D: AdapterDialect>(value: &DinocoValue, dialect: &D) -> String {
    match value {
        DinocoValue::Null => "NULL".to_string(),
        DinocoValue::Integer(value) => value.to_string(),
        DinocoValue::Float(value) => value.to_string(),
        DinocoValue::Boolean(value) => {
            if *value {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        DinocoValue::String(value) => dialect.literal_string(value),
        DinocoValue::Json(value) => dialect.literal_string(&value.to_string()),
        DinocoValue::Bytes(value) => {
            let hex = value
                .iter()
                .map(|byte| format!("{:02x}", byte))
                .collect::<String>();
            dialect.literal_string(&hex)
        }
        DinocoValue::DateTime(value) => dialect.literal_string(&value.to_string()),
    }
}

pub fn render_column_default_from_mapped<D: AdapterDialect>(
    default: &ColumnDefault,
    dialect: &D,
) -> String {
    match default {
        ColumnDefault::Value(value) => render_value_sql(value, dialect),
        ColumnDefault::Function(value) => value.clone(),
        ColumnDefault::Raw(value) => value.clone(),
        ColumnDefault::EnumValue(value) => dialect.literal_string(value),
    }
}

pub fn find_enum_columns<'a>(
    schema: &'a ParsedSchema,
    enum_name: &str,
) -> Vec<(&'a ParsedTable, &'a ParsedField)> {
    schema
        .tables
        .iter()
        .flat_map(|table| {
            table
                .fields
                .iter()
                .filter_map(move |field| match &field.field_type {
                    ParsedFieldType::Enum(name) if name == enum_name => Some((table, field)),
                    _ => None,
                })
        })
        .collect()
}

pub fn find_table_in_schema<'a>(
    schema: &'a ParsedSchema,
    table_name: &str,
) -> Option<&'a ParsedTable> {
    schema.tables.iter().find(|table| table.name == table_name)
}

pub fn sqlite_inline_foreign_keys<D: AdapterDialect>(
    table: &ParsedTable,
    dialect: &D,
) -> Vec<String> {
    table
        .fields
        .iter()
        .filter_map(|field| match &field.relation {
            ParsedRelation::ManyToOne(_, columns, referenced_columns, on_delete, on_update)
            | ParsedRelation::OneToOneOwner(_, columns, referenced_columns, on_delete, on_update) =>
            {
                let referenced_table = match &field.field_type {
                    ParsedFieldType::Relation(name) => name.as_str(),
                    _ => return None,
                };

                Some(format!(
                    "CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}){}{}",
                    dialect.identifier(&format!("fk_{}_{}", table.name, columns.first()?)),
                    render_identifier_list(
                        &columns
                            .iter()
                            .map(|column| column.as_str())
                            .collect::<Vec<_>>(),
                        dialect
                    ),
                    dialect.identifier(referenced_table),
                    render_identifier_list(
                        &referenced_columns
                            .iter()
                            .map(|column| column.as_str())
                            .collect::<Vec<_>>(),
                        dialect
                    ),
                    on_delete
                        .as_ref()
                        .and_then(|_| crate::map_referential_action(on_delete))
                        .map(|value| format!(" ON DELETE {}", value))
                        .unwrap_or_default(),
                    on_update
                        .as_ref()
                        .and_then(|_| crate::map_referential_action(on_update))
                        .map(|value| format!(" ON UPDATE {}", value))
                        .unwrap_or_default(),
                ))
            }
            _ => None,
        })
        .collect()
}
