use dinoco_compiler::{FieldType, FunctionCall, ParsedConfig, ParsedField, ParsedFieldDefault, ParsedFieldType, ParsedRelation, ParsedSchema, ParsedTable, ReferentialAction};
use std::collections::{HashMap, HashSet};

use crate::{
    AlterAction, AlterTableStatement, ColumnDefault, ColumnDefinition, ColumnType, ConstraintDefinition, ConstraintType, CreateIndexStatement, CreateTableStatement, DinocoAdapter,
    DinocoValue, DropTableStatement, QueryDialect,
};

#[derive(Debug, Clone)]
pub enum MigrationStep {
    CreateTable(ParsedTable),
    DropTable(String),

    AddColumn {
        table_name: String,
        field: ParsedField,
    },

    DropColumn {
        table_name: String,
        column_name: String,
    },

    AlterColumn {
        table_name: String,
        old_field: ParsedField,
        new_field: ParsedField,
    },

    RenameColumn {
        table_name: String,
        old_name: String,
        new_name: String,
    },

    AddForeignKey {
        table_name: String,
        column_name: String,
        referenced_table: String,
        referenced_column: String,
        on_delete: Option<ReferentialAction>,
        on_update: Option<ReferentialAction>,
        constraint_name: String,
    },

    DropForeignKey {
        table_name: String,
        constraint_name: String,
    },

    CreateIndex {
        table_name: String,
        column_name: String,
        index_name: String,
        is_unique: bool,
    },
}

pub struct Migration<T: DinocoAdapter> {
    pub adapter: T,
    pub old_schema: Option<ParsedSchema>,
    pub new_schema: ParsedSchema,
}

impl<T: DinocoAdapter> Migration<T> {
    pub fn new(adapter: T, old_schema: Option<ParsedSchema>, new_schema: ParsedSchema) -> Self {
        Self { adapter, old_schema, new_schema }
    }

    pub fn to_up_sql(&self, changes: Vec<MigrationStep>) -> String {
        let mut sql_statements = Vec::new();
        let dialect = self.adapter.dialect();

        for change in changes {
            match change {
                MigrationStep::CreateTable(table) => {
                    let mut stmt = CreateTableStatement::new(dialect, &table.name);

                    for field in &table.fields {
                        if matches!(field.field_type, ParsedFieldType::Relation(_)) {
                            continue;
                        }

                        stmt = stmt.column(self.map_field_to_definition(field));
                    }

                    let (sql, _) = stmt.to_sql();
                    sql_statements.push(sql);
                }

                MigrationStep::DropTable(name) => {
                    let (sql, _) = DropTableStatement::new(dialect, &name).to_sql();
                    sql_statements.push(sql);
                }

                MigrationStep::AddColumn { table_name, field } => {
                    let mut stmt = AlterTableStatement::new(dialect, &table_name);
                    stmt = stmt.add_column(self.map_field_to_definition(&field));

                    for (sql, _) in stmt.to_sql() {
                        sql_statements.push(sql);
                    }
                }

                MigrationStep::DropColumn { table_name, column_name } => {
                    let mut stmt = AlterTableStatement::new(dialect, &table_name);
                    stmt = stmt.drop_column(&column_name);

                    for (sql, _) in stmt.to_sql() {
                        sql_statements.push(sql);
                    }
                }

                MigrationStep::AlterColumn { table_name, new_field, .. } => {
                    let mut stmt = AlterTableStatement::new(dialect, &table_name);

                    stmt = stmt.modify_column(self.map_field_to_definition(&new_field));

                    for (sql, _) in stmt.to_sql() {
                        sql_statements.push(sql);
                    }
                }

                MigrationStep::RenameColumn { table_name, old_name, new_name } => {
                    let mut stmt = AlterTableStatement::new(dialect, &table_name);
                    stmt = stmt.rename_column(&old_name, &new_name);

                    for (sql, _) in stmt.to_sql() {
                        sql_statements.push(sql);
                    }
                }

                MigrationStep::AddForeignKey {
                    table_name,
                    column_name,
                    referenced_table,
                    referenced_column,
                    on_delete,
                    on_update,
                    constraint_name,
                } => {
                    let mut stmt = AlterTableStatement::new(dialect, &table_name);

                    stmt = stmt.add_constraint(ConstraintDefinition {
                        name: Some(constraint_name),
                        columns: vec![column_name],
                        ref_table: Some(referenced_table),
                        ref_columns: vec![referenced_column],
                        on_delete: Self::map_referential_action(&on_delete),
                        on_update: Self::map_referential_action(&on_update),
                        constraint_type: ConstraintType::ForeignKey,
                    });

                    for (sql, _) in stmt.to_sql() {
                        sql_statements.push(sql);
                    }
                }

                MigrationStep::DropForeignKey { table_name, constraint_name } => {
                    let mut stmt = AlterTableStatement::new(dialect, &table_name);
                    stmt = stmt.drop_constraint(&constraint_name);

                    for (sql, _) in stmt.to_sql() {
                        sql_statements.push(sql);
                    }
                }

                MigrationStep::CreateIndex {
                    table_name,
                    column_name,
                    index_name,
                    is_unique,
                } => {
                    let mut stmt = CreateIndexStatement::new(dialect, &table_name, &index_name).column(&column_name);

                    if is_unique {
                        stmt = stmt.unique();
                    }

                    let (sql, _) = stmt.to_sql();
                    sql_statements.push(sql);
                }
            }
        }

        if sql_statements.is_empty() {
            String::new()
        } else {
            format!("{};\n", sql_statements.join(";\n\n"))
        }
    }

    pub fn diff(&self) -> Vec<MigrationStep> {
        let old_schema = self.old_schema.clone().unwrap_or(ParsedSchema {
            config: self.new_schema.config.clone(),
            enums: vec![],
            tables: vec![],
        });

        let old_map: HashMap<&String, &ParsedTable> = old_schema.tables.iter().map(|t| (&t.name, t)).collect();

        let new_map: HashMap<&String, &ParsedTable> = self.new_schema.tables.iter().map(|t| (&t.name, t)).collect();

        let mut steps = Vec::new();

        // 1. Drop FK (futuro: implementar diff de FK)

        // 2. Drop Tables
        for name in old_map.keys() {
            if !new_map.contains_key(name) {
                steps.push(MigrationStep::DropTable((*name).clone()));
            }
        }

        // 3. Create Tables
        for (name, table) in &new_map {
            if !old_map.contains_key(name) {
                steps.push(MigrationStep::CreateTable((*table).clone()));
            }
        }

        // 4. Columns
        for (name, new_table) in &new_map {
            if let Some(old_table) = old_map.get(name) {
                steps.extend(Self::diff_columns(old_table, new_table));
            }
        }

        // 5. Relations (FK + join tables)
        for table in &self.new_schema.tables {
            let (fks, joins) = Self::extract_relations(table, &self.new_schema.tables);

            for jt in joins {
                if !old_map.contains_key(&jt.name) {
                    steps.push(MigrationStep::CreateTable(jt));
                }
            }

            steps.extend(fks);
        }

        steps
    }

    fn map_field_to_definition<'a>(&self, field: &'a ParsedField) -> ColumnDefinition<'a> {
        ColumnDefinition {
            name: field.name.as_str(),
            col_type: self.map_column_type(&field.field_type),
            primary_key: field.is_primary_key,
            not_null: !field.is_optional,
            auto_increment: matches!(field.default_value, ParsedFieldDefault::Function(FunctionCall::AutoIncrement)),
            default: self.map_default(&field.default_value),
        }
    }

    fn map_column_type(&self, field_type: &ParsedFieldType) -> ColumnType {
        match field_type {
            ParsedFieldType::String => ColumnType::Text,
            ParsedFieldType::Boolean => ColumnType::Boolean,
            ParsedFieldType::Integer => ColumnType::Integer,
            ParsedFieldType::Float => ColumnType::Float,
            ParsedFieldType::Json => ColumnType::Json,
            ParsedFieldType::DateTime => ColumnType::DateTime,
            ParsedFieldType::Enum(name) => ColumnType::Enum(name.clone()),
            ParsedFieldType::Relation(_) => ColumnType::Integer,
        }
    }

    fn map_default(&self, df: &ParsedFieldDefault) -> Option<ColumnDefault> {
        match df {
            ParsedFieldDefault::String(s) => Some(ColumnDefault::Value(DinocoValue::String(s.clone()))),
            ParsedFieldDefault::Integer(i) => Some(ColumnDefault::Value(DinocoValue::Integer(*i))),
            ParsedFieldDefault::Boolean(b) => Some(ColumnDefault::Value(DinocoValue::Boolean(*b))),
            ParsedFieldDefault::Function(FunctionCall::Now) => Some(ColumnDefault::Function("NOW()".to_string())),
            _ => None,
        }
    }

    fn map_referential_action(action: &Option<ReferentialAction>) -> Option<&'static str> {
        match action {
            Some(ReferentialAction::Cascade) => Some("CASCADE"),
            Some(ReferentialAction::SetNull) => Some("SET NULL"),
            Some(ReferentialAction::SetDefault) => Some("SET DEFAULT"),
            None => None,
        }
    }

    fn diff_columns(old_table: &ParsedTable, new_table: &ParsedTable) -> Vec<MigrationStep> {
        let mut steps = Vec::new();

        let old_fields: HashMap<&String, &ParsedField> = old_table.fields.iter().map(|f| (&f.name, f)).collect();

        let new_fields: HashMap<&String, &ParsedField> = new_table.fields.iter().map(|f| (&f.name, f)).collect();

        for (name, new_field) in &new_fields {
            if let Some(old_field) = old_fields.get(name) {
                if old_field.field_type != new_field.field_type || old_field.is_optional != new_field.is_optional || old_field.default_value != new_field.default_value {
                    steps.push(MigrationStep::AlterColumn {
                        table_name: new_table.name.clone(),
                        old_field: (*old_field).clone(),
                        new_field: (*new_field).clone(),
                    });
                }

                continue;
            }

            steps.push(MigrationStep::AddColumn {
                table_name: new_table.name.clone(),
                field: (*new_field).clone(),
            });
        }

        for name in old_fields.keys() {
            if !new_fields.contains_key(name) {
                steps.push(MigrationStep::DropColumn {
                    table_name: old_table.name.clone(),
                    column_name: (*name).clone(),
                });
            }
        }

        steps
    }

    fn extract_relations(table: &ParsedTable, all_tables: &Vec<ParsedTable>) -> (Vec<MigrationStep>, Vec<ParsedTable>) {
        let mut fk_steps = Vec::new();
        let mut join_tables = Vec::new();

        for field in &table.fields {
            match &field.relation {
                ParsedRelation::ManyToOne(_, local, foreign, on_delete, on_update) | ParsedRelation::OneToOneOwner(_, local, foreign, on_delete, on_update) => {
                    if let (Some(l), Some(f)) = (local.first(), foreign.first()) {
                        let ref_table = match &field.field_type {
                            ParsedFieldType::Relation(name) => name.clone(),
                            _ => continue,
                        };

                        let constraint_name = format!("fk_{}_{}_{}", table.name, l, ref_table);

                        fk_steps.push(MigrationStep::AddForeignKey {
                            table_name: table.name.clone(),
                            column_name: l.clone(),
                            referenced_table: ref_table,
                            referenced_column: f.clone(),
                            on_delete: on_delete.clone(),
                            on_update: on_update.clone(),
                            constraint_name,
                        });
                    }
                }

                _ => {}
            }
        }

        (fk_steps, join_tables)
    }
}
