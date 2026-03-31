use super::SqlBuilder;
use crate::{ColumnDefault, ColumnDefinition, ConstraintDefinition, ConstraintType, DinocoValue, QueryDialect};

pub struct CreateTableStatement<'a, D: QueryDialect> {
    pub table_name: &'a str,
    pub columns: Vec<ColumnDefinition<'a>>,
    pub constraints: Vec<ConstraintDefinition<'a>>,
    pub dialect: &'a D,
}

impl<'a, D: QueryDialect> CreateTableStatement<'a, D> {
    pub fn new(dialect: &'a D, table_name: &'a str) -> Self {
        Self {
            table_name,
            columns: vec![],
            constraints: vec![],
            dialect,
        }
    }

    pub fn column(mut self, column: ColumnDefinition<'a>) -> Self {
        self.columns.push(column);

        self
    }

    pub fn add_constraint(mut self, constraint: ConstraintDefinition<'a>) -> Self {
        self.constraints.push(constraint);

        self
    }

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self.dialect, 512);

        builder.push("CREATE TABLE ");
        builder.push_identifier(self.table_name);
        builder.push(" (\n");

        let pk_columns: Vec<&str> = self.columns.iter().filter(|c| c.primary_key).map(|c| c.name).collect();
        let is_composite_pk = pk_columns.len() > 1;

        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                builder.push(",\n");
            }

            builder.push("\t");
            builder.push_identifier(col.name);
            builder.push(" ");

            let is_inline_pk = col.primary_key && !is_composite_pk;

            builder.push(&self.dialect.column_type(&col.col_type, is_inline_pk, col.auto_increment));

            if col.not_null && !is_inline_pk {
                builder.push(" NOT NULL");
            }

            if let Some(ref default_val) = col.default {
                builder.push(" DEFAULT ");

                match default_val {
                    ColumnDefault::Function(func) => builder.push(&func.to_uppercase()),
                    ColumnDefault::Raw(v) => builder.push(v),
                    ColumnDefault::Value(val) => match val {
                        DinocoValue::String(s) => builder.push(&format!("'{}'", s)),
                        DinocoValue::Integer(i) => builder.push(&i.to_string()),
                        DinocoValue::Boolean(b) => builder.push(if *b { "TRUE" } else { "FALSE" }),
                        DinocoValue::Json(v) => {
                            let json = v.to_string().replace('\'', "''");
                            builder.push(&json);
                        }
                        DinocoValue::DateTime(b) => builder.push(&format!("'{}'", b)),
                        _ => builder.push("NULL"),
                    },
                }
            }
        }

        if is_composite_pk {
            builder.push(",\n\tPRIMARY KEY (");
            for (i, pk_name) in pk_columns.iter().enumerate() {
                if i > 0 {
                    builder.push(", ");
                }

                builder.push_identifier(pk_name);
            }

            builder.push(")");
        }

        for constraint in &self.constraints {
            builder.push(",\n\t CONSTRAINT ");
            builder.push_identifier(constraint.name);
            builder.push(" ");

            match &constraint.constraint_type {
                ConstraintType::Unique(cols) => {
                    builder.push("UNIQUE (");
                    for (j, col) in cols.iter().enumerate() {
                        if j > 0 {
                            builder.push(", ");
                        }

                        builder.push_identifier(col);
                    }

                    builder.push(")");
                }
                ConstraintType::Check(expr) => {
                    builder.push("CHECK (");
                    builder.push(expr);
                    builder.push(")");
                }
                ConstraintType::ForeignKey {
                    columns,
                    ref_table,
                    ref_columns,
                    on_delete,
                    on_update,
                } => {
                    builder.push("FOREIGN KEY (");

                    for (j, col) in columns.iter().enumerate() {
                        if j > 0 {
                            builder.push(", ");
                        }

                        builder.push_identifier(col);
                    }

                    builder.push(") REFERENCES ");
                    builder.push_identifier(ref_table);
                    builder.push(" (");

                    for (j, col) in ref_columns.iter().enumerate() {
                        if j > 0 {
                            builder.push(", ");
                        }

                        builder.push_identifier(col);
                    }

                    builder.push(")");

                    if let Some(action) = on_delete {
                        builder.push(" ON DELETE ");
                        builder.push(action);
                    }

                    if let Some(action) = on_update {
                        builder.push(" ON UPDATE ");
                        builder.push(action);
                    }
                }
                _ => {}
            }
        }

        builder.push("\n)");
        builder.finish()
    }
}
