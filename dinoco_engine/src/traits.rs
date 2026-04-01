use async_trait::async_trait;

use crate::{
    AlterAction, AlterTableStatement, ColumnDefault, ConstraintType, CreateIndexStatement, CreateTableStatement, DinocoValue, DropIndexStatement, DropTableStatement, SqlBuilder,
};

use crate::{ColumnType, DinocoResult, DinocoStream};

#[async_trait]
pub trait DinocoAdapter: Sized {
    type Dialect: SqlDialect;

    fn dialect(&self) -> &Self::Dialect;

    async fn connect(url: String) -> DinocoResult<Self>;
    async fn execute(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<()>;
    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>>;

    async fn stream_as<T: DinocoRow + Send + 'static>(&self, query: &str, params: &[DinocoValue]) -> DinocoStream<T>;
}

pub trait FromDinocoValue: Sized {
    fn from_value(value: &DinocoValue) -> DinocoResult<Self>;
}

pub trait RowExt {
    fn get_value<T: FromDinocoValue>(&self, index: usize) -> DinocoResult<T>;
}

pub trait DinocoDatabaseRow {
    fn get_i64(&self, idx: usize) -> DinocoResult<i64>;
    fn get_string(&self, idx: usize) -> DinocoResult<String>;
    fn get_bool(&self, idx: usize) -> DinocoResult<bool>;
    fn get_f64(&self, idx: usize) -> DinocoResult<f64>;
    fn get_bytes(&self, idx: usize) -> DinocoResult<Vec<u8>>;

    fn get<T: DinocoType>(&self, idx: usize) -> DinocoResult<T>;
}

pub trait DinocoType: Sized {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self>;
}

pub trait DinocoRow: Sized {
    fn from_row<R: DinocoDatabaseRow>(row: &R) -> DinocoResult<Self>;
}

pub trait SqlDialect {
    fn bind_param(&self, index: usize) -> String;
    fn identifier(&self, v: &str) -> String;
    fn literal_string(&self, v: &str) -> String;

    fn column_type(&self, t: &ColumnType, is_primary: bool, auto_increment: bool) -> String;
    fn modify_column(&self) -> String;

    fn default_schema(&self) -> String;
    fn cast_boolean(&self, expr: &str) -> String;
}

pub trait SqlDialectBuilders: SqlDialect + Sized {
    fn build_create_index<'a>(&self, stmt: &CreateIndexStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self, 256);

        builder.push("CREATE ");

        if stmt.is_unique {
            builder.push("UNIQUE ");
        }

        builder.push("INDEX ");
        builder.push_identifier(stmt.index_name);
        builder.push(" ON ");
        builder.push_identifier(stmt.table_name);
        builder.push(" (");

        for (i, col) in stmt.columns.iter().enumerate() {
            if i > 0 {
                builder.push(", ");
            }
            builder.push_identifier(col);
        }

        builder.push(")");

        builder.finish()
    }

    fn build_drop_index<'a>(&self, stmt: &DropIndexStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self, 128);

        builder.push("DROP INDEX ");
        builder.push_identifier(stmt.index_name);

        if let Some(table) = stmt.table_name {
            builder.push(" ON ");
            builder.push_identifier(table);
        }

        builder.finish()
    }

    fn build_create_table<'a>(&self, stmt: &CreateTableStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self, 512);

        builder.push("CREATE TABLE ");
        builder.push_identifier(stmt.table_name);
        builder.push(" (\n");

        let pk_columns: Vec<&str> = stmt.columns.iter().filter(|c| c.primary_key).map(|c| c.name).collect();
        let is_composite_pk = pk_columns.len() > 1;

        for (i, col) in stmt.columns.iter().enumerate() {
            if i > 0 {
                builder.push(",\n");
            }

            builder.push("\t");
            builder.push_identifier(col.name);
            builder.push(" ");

            let is_inline_pk = col.primary_key && !is_composite_pk;

            builder.push(&self.column_type(&col.col_type, is_inline_pk, col.auto_increment));

            if col.not_null && !is_inline_pk {
                builder.push(" NOT NULL");
            }

            if let Some(ref default_val) = col.default {
                self.push_default_value(&mut builder, default_val);
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

        for constraint in &stmt.constraints {
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

    fn build_drop_table<'a>(&self, stmt: &DropTableStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self, 128);

        builder.push("DROP TABLE ");

        builder.push_identifier(stmt.table_name);

        if stmt.cascade {
            builder.push(" CASCADE;");
        }

        builder.finish()
    }

    fn build_alter_table<'a>(&self, stmt: &AlterTableStatement<'a, Self>) -> Vec<(String, Vec<DinocoValue>)> {
        let mut statements = Vec::new();

        for action in &stmt.actions {
            let mut builder = SqlBuilder::new(self, 256);

            builder.push("ALTER TABLE ");
            builder.push_identifier(stmt.table_name);
            builder.push(" ");

            match action {
                AlterAction::AddColumn(col) => {
                    builder.push("ADD COLUMN ");
                    builder.push_identifier(col.name);
                    builder.push(" ");

                    builder.push(&self.column_type(&col.col_type, col.primary_key, col.auto_increment));

                    if col.not_null && !col.primary_key {
                        builder.push(" NOT NULL");
                    }

                    if let Some(ref default_val) = col.default {
                        self.push_default_value(&mut builder, default_val);
                    }
                }
                AlterAction::DropColumn(name) => {
                    builder.push("DROP COLUMN ");
                    builder.push_identifier(name);
                }

                AlterAction::ModifyColumn(col) => {
                    builder.push(&self.modify_column());
                    builder.push(" ");
                    builder.push_identifier(col.name);
                    builder.push(" ");

                    builder.push(&self.column_type(&col.col_type, col.primary_key, col.auto_increment));

                    if col.not_null && !col.primary_key {
                        builder.push(" NOT NULL");
                    } else if !col.not_null {
                        builder.push(" NULL");
                    }

                    if let Some(ref default_val) = col.default {
                        self.push_default_value(&mut builder, default_val);
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

    fn push_default_value(&self, builder: &mut SqlBuilder<'_, Self>, value: &ColumnDefault) {
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
                    builder.push(&json);
                }
                DinocoValue::DateTime(b) => builder.push(&format!("'{}'", b)),
                _ => builder.push("NULL"),
            },
        }
    }
}
