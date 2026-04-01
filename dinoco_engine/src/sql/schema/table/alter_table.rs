use crate::{AlterAction, ColumnDefault, ColumnDefinition, ConstraintDefinition, ConstraintType, DinocoValue, SqlDialect, SqlBuilder};

pub struct AlterTableStatement<'a, D: SqlDialect> {
    pub table_name: &'a str,
    pub actions: Vec<AlterAction<'a>>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> AlterTableStatement<'a, D> {
    pub fn new(dialect: &'a D, table_name: &'a str) -> Self {
        Self {
            table_name,
            actions: vec![],
            dialect,
        }
    }

    pub fn add_column(mut self, column: ColumnDefinition<'a>) -> Self {
        self.actions.push(AlterAction::AddColumn(column));

        self
    }

    pub fn drop_column(mut self, column_name: &'a str) -> Self {
        self.actions.push(AlterAction::DropColumn(column_name));

        self
    }

    pub fn modify_column(mut self, column: ColumnDefinition<'a>) -> Self {
        self.actions.push(AlterAction::ModifyColumn(column));

        self
    }

    pub fn rename_column(mut self, old_name: &'a str, new_name: &'a str) -> Self {
        self.actions.push(AlterAction::RenameColumn { old_name, new_name });

        self
    }

    pub fn add_constraint(mut self, constraint: ConstraintDefinition<'a>) -> Self {
        self.actions.push(AlterAction::AddConstraint(constraint));

        self
    }

    pub fn drop_constraint(mut self, constraint_name: &'a str) -> Self {
        self.actions.push(AlterAction::DropConstraint(constraint_name));

        self
    }

    pub fn to_sql(&self) -> Vec<(String, Vec<DinocoValue>)> {
        let mut statements = Vec::new();

        for action in &self.actions {
            let mut builder = SqlBuilder::new(self.dialect, 256);

            builder.push("ALTER TABLE ");
            builder.push_identifier(self.table_name);
            builder.push(" ");

            match action {
                AlterAction::AddColumn(col) => {
                    builder.push("ADD COLUMN ");
                    builder.push_identifier(col.name);
                    builder.push(" ");

                    builder.push(&self.dialect.column_type(&col.col_type, col.primary_key, col.auto_increment));

                    if col.not_null && !col.primary_key {
                        builder.push(" NOT NULL");
                    }

                    if let Some(ref default_val) = col.default {
                        Self::push_default_value(&mut builder, default_val);
                    }
                }
                AlterAction::DropColumn(name) => {
                    builder.push("DROP COLUMN ");
                    builder.push_identifier(name);
                }

                AlterAction::ModifyColumn(col) => {
                    builder.push(&self.dialect.modify_column());
                    builder.push(" ");
                    builder.push_identifier(col.name);
                    builder.push(" ");

                    builder.push(&self.dialect.column_type(&col.col_type, col.primary_key, col.auto_increment));

                    if col.not_null && !col.primary_key {
                        builder.push(" NOT NULL");
                    } else if !col.not_null {
                        builder.push(" NULL");
                    }

                    if let Some(ref default_val) = col.default {
                        Self::push_default_value(&mut builder, default_val);
                    }
                }

                AlterAction::RenameColumn { old_name, new_name } => {
                    builder.push("RENAME COLUMN ");
                    builder.push_identifier(old_name);
                    builder.push(" TO ");
                    builder.push_identifier(new_name);
                }

                AlterAction::AddConstraint(constraint) => {
                    builder.push("ADD CONSTRAINT ");
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
                        ConstraintType::PrimaryKey(cols) => {
                            builder.push("PRIMARY KEY (");
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
                    }
                }
                AlterAction::DropConstraint(name) => {
                    builder.push("DROP CONSTRAINT ");
                    builder.push_identifier(name);
                }
            }

            statements.push(builder.finish());
        }

        statements
    }

    fn push_default_value(builder: &mut SqlBuilder<'_, D>, value: &ColumnDefault) {
        builder.push(" DEFAULT ");

        match value {
            ColumnDefault::Function(func) => builder.push(&func.to_uppercase()),
            ColumnDefault::Raw(v) => builder.push(v),
            ColumnDefault::Value(val) => match val {
                DinocoValue::String(s) => builder.push(&format!("'{}'", s)),
                DinocoValue::Integer(i) => builder.push(&i.to_string()),
                DinocoValue::Boolean(b) => builder.push(if *b { "TRUE" } else { "FALSE" }),
                DinocoValue::Json(v) => {
                    let json = v.to_string().replace('\'', "''");

                    builder.push(&format!("{}", json));
                }
                DinocoValue::DateTime(b) => builder.push(&format!("'{}'", b)),
                _ => builder.push("NULL"),
            },
        }
    }
}
