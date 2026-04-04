use async_trait::async_trait;
use dinoco_compiler::{ParsedEnum, ParsedFieldDefault, ParsedFieldType, ParsedRelation, ParsedTable, ReferentialAction};
use futures::stream::{self, StreamExt};
use std::sync::Arc;

use deadpool_sqlite::{Config, Pool, Runtime};

use rusqlite::Row as RusqliteRow;
use rusqlite::types::{ToSqlOutput, Value, ValueRef};

use crate::mapper::{map_default, map_field_to_definition};
use crate::{
    AlterAction, AlterEnumStatement, AlterTableStatement, ColumnDefault, ColumnDefinition, ColumnType, CreateEnumStatement, DinocoAdapter, DinocoDatabaseRow, DinocoError,
    DinocoResult, DinocoRow, DinocoStream, DinocoType, DinocoValue, DropEnumStatement, SqlBuilder, SqlDialect, SqlDialectBuilders,
};

pub struct SqliteDialect;

pub struct SqliteAdapter {
    pub url: String,
    pub pool: Arc<Pool>,
}

static SQLITE_DIALECT: SqliteDialect = SqliteDialect;

#[async_trait]
impl DinocoAdapter for SqliteAdapter {
    type Dialect = SqliteDialect;

    fn dialect(&self) -> &Self::Dialect {
        &SQLITE_DIALECT
    }

    async fn connect(url: String) -> DinocoResult<Self> {
        let cfg = Config::new(&url);
        let pool = cfg.create_pool(Runtime::Tokio1).map_err(DinocoError::from)?;

        Ok(Self { url, pool: Arc::new(pool) })
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<()> {
        let conn = self.pool.get().await.map_err(DinocoError::from)?;
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        conn.interact(move |conn| {
            let params_refs: Vec<&dyn rusqlite::ToSql> = params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            conn.execute(&query_owned, params_refs.as_slice())
        })
        .await
        .map_err(DinocoError::from)?
        .map_err(DinocoError::from)?;

        Ok(())
    }

    async fn query_as<T: DinocoRow + Send + 'static>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>> {
        let conn = self.pool.get().await.map_err(DinocoError::from)?;
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        let results = conn
            .interact(move |conn| -> DinocoResult<Vec<T>> {
                let mut stmt = conn.prepare(&query_owned).map_err(DinocoError::from)?;
                let params_refs: Vec<&dyn rusqlite::ToSql> = params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

                let mut rows = stmt.query(params_refs.as_slice()).map_err(DinocoError::from)?;
                let mut results = Vec::new();

                while let Some(row) = rows.next().map_err(DinocoError::from)? {
                    results.push(T::from_row(row)?);
                }

                Ok(results)
            })
            .await
            .map_err(DinocoError::from)??;

        Ok(results)
    }

    async fn stream_as<T: DinocoRow + Send + 'static>(&self, query: &str, params: &[DinocoValue]) -> DinocoStream<T> {
        match self.query_as::<T>(query, params).await {
            Ok(results) => Box::pin(stream::iter(results.into_iter().map(Ok))),
            Err(e) => Box::pin(stream::once(async { Err(e) })),
        }
    }
}

impl rusqlite::ToSql for DinocoValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            DinocoValue::Null => Ok(ToSqlOutput::Owned(Value::Null)),
            DinocoValue::Integer(i) => Ok(ToSqlOutput::Owned(Value::Integer(*i))),
            DinocoValue::Float(f) => Ok(ToSqlOutput::Owned(Value::Real(*f))),
            DinocoValue::Boolean(b) => Ok(ToSqlOutput::Owned(Value::Integer(if *b { 1 } else { 0 }))),
            DinocoValue::String(s) => Ok(ToSqlOutput::Owned(Value::Text(s.clone()))),
            DinocoValue::Json(v) => Ok(ToSqlOutput::Owned(Value::Text(v.to_string()))),
            DinocoValue::Bytes(v) => Ok(ToSqlOutput::Owned(Value::Blob(v.clone()))),
            DinocoValue::DateTime(dt) => Ok(ToSqlOutput::Owned(Value::Text(dt.to_string()))),
        }
    }
}

impl<'stmt> DinocoDatabaseRow for RusqliteRow<'stmt> {
    fn get_i64(&self, idx: usize) -> DinocoResult<i64> {
        Ok(self.get(idx).map_err(DinocoError::from)?)
    }

    fn get_string(&self, idx: usize) -> DinocoResult<String> {
        Ok(self.get(idx).map_err(DinocoError::from)?)
    }

    fn get_bool(&self, idx: usize) -> DinocoResult<bool> {
        let val: i64 = self.get(idx).map_err(DinocoError::from)?;
        Ok(val != 0)
    }

    fn get_f64(&self, idx: usize) -> DinocoResult<f64> {
        Ok(self.get(idx).map_err(DinocoError::from)?)
    }

    fn get_bytes(&self, idx: usize) -> DinocoResult<Vec<u8>> {
        Ok(self.get(idx).map_err(DinocoError::from)?)
    }

    fn get<T: DinocoType>(&self, idx: usize) -> DinocoResult<T> {
        T::from_row(self, idx)
    }
}

impl SqlDialect for SqliteDialect {
    fn default_schema(&self) -> String {
        "".to_string()
    }

    fn cast_boolean(&self, expr: &str) -> String {
        format!("({} = 1)", expr)
    }

    fn bind_param(&self, index: usize) -> String {
        format!("?{}", index)
    }

    fn identifier(&self, v: &str) -> String {
        let escaped = v.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    }

    fn literal_string(&self, v: &str) -> String {
        let escaped = v.replace('\'', "''");
        format!("'{}'", escaped)
    }

    fn modify_column(&self) -> String {
        "".to_string()
    }

    fn supports_native_enums(&self) -> bool {
        false
    }

    fn supports_drop_constraints(&self) -> bool {
        false
    }

    fn supports_information_schema(&self) -> bool {
        false
    }

    fn query_get_foreign_keys(&self) -> String {
        "PRAGMA foreign_key_list".to_string()
    }

    fn query_get_enums(&self) -> String {
        "".to_string()
    }

    fn column_type(&self, col: &ColumnDefinition, is_primary: bool, auto_increment: bool) -> String {
        if is_primary && auto_increment {
            return "INTEGER PRIMARY KEY AUTOINCREMENT".to_string();
        }

        let mut base_type = match &col.col_type {
            ColumnType::Integer => "INTEGER".to_string(),
            ColumnType::Float => "REAL".to_string(),
            ColumnType::Text => "TEXT".to_string(),
            ColumnType::Boolean => "INTEGER".to_string(),
            ColumnType::Json => "TEXT".to_string(),
            ColumnType::DateTime => "TEXT".to_string(),
            ColumnType::Bytes => "BLOB".to_string(),
            ColumnType::Enum(_) => "TEXT".to_string(),
            ColumnType::EnumInline(values) => {
                let check_values = values.iter().map(|v| self.literal_string(v)).collect::<Vec<_>>().join(", ");

                format!("TEXT CHECK ({} IN ({}))", col.name, check_values)
            }
        };

        if is_primary {
            base_type.push_str(" PRIMARY KEY");
        }

        base_type
    }
}

impl SqlDialectBuilders for SqliteDialect {
    fn rebuild_table_shadow<'a>(
        &self,
        table_name: &str,
        parsed_table: &ParsedTable,
        enums: &[ParsedEnum],
        modified_col: Option<&ColumnDefinition<'a>>,
    ) -> Vec<(String, Vec<DinocoValue>)> {
        let mut statements = Vec::new();
        let temp_table = format!("{}_old", table_name);

        statements.push(("PRAGMA foreign_keys=off;".to_string(), vec![]));

        statements.push((format!("ALTER TABLE {} RENAME TO {}", self.identifier(table_name), self.identifier(&temp_table)), vec![]));

        let mut create_builder = SqlBuilder::new(self, 512);
        create_builder.push("CREATE TABLE ");
        create_builder.push_identifier(table_name);
        create_builder.push(" (\n");

        let pk_columns: Vec<&str> = parsed_table
            .fields
            .iter()
            .filter(|f| f.is_primary_key && !matches!(f.field_type, ParsedFieldType::Relation(_)))
            .map(|f| f.name.as_str())
            .collect();
        let is_composite_pk = pk_columns.len() > 1;

        let real_fields: Vec<_> = parsed_table.fields.iter().filter(|f| !matches!(f.field_type, ParsedFieldType::Relation(_))).collect();

        for (i, field) in real_fields.iter().enumerate() {
            if i > 0 {
                create_builder.push(",\n");
            }
            create_builder.push("\t");
            create_builder.push_identifier(&field.name);
            create_builder.push(" ");

            let is_inline_pk = field.is_primary_key && !is_composite_pk;

            if let Some(col) = modified_col.filter(|c| c.name == field.name) {
                let col_def = self.column_type(&col, is_inline_pk, col.auto_increment);
                create_builder.push(&col_def.replace("{}", &self.identifier(col.name)));

                if col.not_null && !is_inline_pk {
                    create_builder.push(" NOT NULL");
                }

                if let Some(ref default_val) = col.default {
                    self.push_default_value(&mut create_builder, default_val);
                }
            } else {
                let col = map_field_to_definition(field, self, enums);

                let is_auto_inc = is_inline_pk && matches!(field.field_type, ParsedFieldType::Integer);
                let col_def = self.column_type(&col, is_inline_pk, is_auto_inc);
                create_builder.push(&col_def.replace("{}", &self.identifier(&field.name)));

                if !field.is_optional && !is_inline_pk {
                    create_builder.push(" NOT NULL");
                }

                if !matches!(field.default_value, ParsedFieldDefault::NotDefined) {
                    if let Some(mapped_default) = map_default(&field.default_value) {
                        self.push_default_value(&mut create_builder, &mapped_default);
                    }
                }
            }
        }

        if is_composite_pk {
            create_builder.push(",\n\tPRIMARY KEY (");
            let pk_str = pk_columns.iter().map(|c| self.identifier(c)).collect::<Vec<_>>().join(", ");
            create_builder.push(&pk_str);
            create_builder.push(")");
        }

        for field in parsed_table.fields.iter() {
            if field.is_unique {
                create_builder.push(",\n\tUNIQUE (");
                create_builder.push_identifier(&field.name);
                create_builder.push(")");
            }

            match &field.relation {
                ParsedRelation::ManyToOne(_, local_cols, foreign_cols, on_delete, on_update) | ParsedRelation::OneToOneOwner(_, local_cols, foreign_cols, on_delete, on_update) => {
                    if let ParsedFieldType::Relation(ref_table) = &field.field_type {
                        create_builder.push(",\n\tFOREIGN KEY (");

                        let locals = local_cols.iter().map(|c| self.identifier(c)).collect::<Vec<_>>().join(", ");
                        create_builder.push(&locals);

                        create_builder.push(") REFERENCES ");
                        create_builder.push_identifier(ref_table);
                        create_builder.push(" (");

                        let foreigns = foreign_cols.iter().map(|c| self.identifier(c)).collect::<Vec<_>>().join(", ");
                        create_builder.push(&foreigns);
                        create_builder.push(")");

                        let ref_action_to_str = |action: &ReferentialAction| match action {
                            ReferentialAction::Cascade => "CASCADE",
                            ReferentialAction::SetNull => "SET NULL",
                            ReferentialAction::SetDefault => "SET DEFAULT",
                        };

                        if let Some(act) = on_delete {
                            create_builder.push(" ON DELETE ");
                            create_builder.push(ref_action_to_str(act));
                        }

                        if let Some(act) = on_update {
                            create_builder.push(" ON UPDATE ");
                            create_builder.push(ref_action_to_str(act));
                        }
                    }
                }
                _ => {}
            }
        }

        create_builder.push("\n)");
        statements.push(create_builder.finish());

        let mut insert_builder = SqlBuilder::new(self, 256);
        insert_builder.push("INSERT INTO ");
        insert_builder.push_identifier(table_name);
        insert_builder.push(" (");

        let column_names: Vec<String> = real_fields.iter().map(|f| self.identifier(&f.name)).collect();

        insert_builder.push(&column_names.join(", "));
        insert_builder.push(") SELECT ");
        insert_builder.push(&column_names.join(", "));
        insert_builder.push(" FROM ");
        insert_builder.push_identifier(&temp_table);

        statements.push(insert_builder.finish());

        statements.push((format!("DROP TABLE {}", self.identifier(&temp_table)), vec![]));
        statements.push(("PRAGMA foreign_keys=on;".to_string(), vec![]));

        statements
    }

    fn build_alter_table<'a>(&self, stmt: &AlterTableStatement<'a, Self>) -> Vec<(String, Vec<DinocoValue>)> {
        let mut statements = Vec::new();

        for action in &stmt.actions {
            match action {
                AlterAction::AddColumn(col) => {
                    let mut builder = SqlBuilder::new(self, 256);
                    builder.push("ALTER TABLE ");
                    builder.push_identifier(stmt.table_name);
                    builder.push(" ADD COLUMN ");
                    builder.push_identifier(col.name);
                    builder.push(" ");

                    let col_def = self.column_type(&col, col.primary_key, col.auto_increment);
                    builder.push(&col_def.replace("{}", &self.identifier(col.name)));

                    if col.not_null && !col.primary_key {
                        builder.push(" NOT NULL");
                    }

                    if let Some(ref default_val) = col.default {
                        self.push_default_value(&mut builder, default_val);
                    }

                    statements.push(builder.finish());
                }
                AlterAction::DropColumn(name) => {
                    let mut builder = SqlBuilder::new(self, 128);

                    builder.push("ALTER TABLE ");
                    builder.push_identifier(stmt.table_name);
                    builder.push(" DROP COLUMN ");
                    builder.push_identifier(name);

                    statements.push(builder.finish());
                }
                AlterAction::RenameColumn { old_name, new_name } => {
                    let mut builder = SqlBuilder::new(self, 128);
                    builder.push("ALTER TABLE ");
                    builder.push_identifier(stmt.table_name);
                    builder.push(" RENAME COLUMN ");
                    builder.push_identifier(old_name);
                    builder.push(" TO ");
                    builder.push_identifier(new_name);
                    statements.push(builder.finish());
                }
                AlterAction::ModifyColumn(parsed_table, enums, col) => {
                    statements.extend(self.rebuild_table_shadow(stmt.table_name, parsed_table, enums, Some(col)));
                }
                AlterAction::AddConstraint(parsed_table, enums, _) | AlterAction::DropConstraint(parsed_table, enums, _) => {
                    statements.extend(self.rebuild_table_shadow(stmt.table_name, parsed_table, enums, None));
                }
            }
        }

        statements
    }

    fn build_create_enum<'a>(&self, _stmt: &CreateEnumStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        ("".to_string(), vec![])
    }

    fn build_alter_enum<'a>(&self, stmt: &AlterEnumStatement<'a, Self>) -> Vec<(String, Vec<DinocoValue>)> {
        vec![]
    }

    fn build_drop_enum<'a>(&self, _stmt: &DropEnumStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        ("".to_string(), vec![])
    }

    fn push_default_value(&self, builder: &mut SqlBuilder<'_, Self>, value: &ColumnDefault) {
        builder.push(" DEFAULT ");

        match value {
            ColumnDefault::Function(func) => {
                let f = func.trim().to_lowercase();

                match f.as_str() {
                    "now()" | "current_timestamp" => builder.push("CURRENT_TIMESTAMP"),
                    "current_date" => builder.push("CURRENT_DATE"),
                    "current_time" => builder.push("CURRENT_TIME"),
                    _ => builder.push(&func.to_uppercase()),
                }
            }

            ColumnDefault::Raw(v) => builder.push(v),
            ColumnDefault::EnumValue(v) => {
                builder.push(&self.literal_string(v));
            }
            ColumnDefault::Value(val) => match val {
                DinocoValue::String(s) => {
                    let escaped = s.replace('\'', "''");
                    builder.push(&format!("'{}'", escaped));
                }

                DinocoValue::Integer(i) => builder.push(&i.to_string()),
                DinocoValue::Boolean(b) => builder.push(if *b { "1" } else { "0" }),
                DinocoValue::Json(v) => {
                    let json = v.to_string().replace('\'', "''");
                    builder.push(&format!("'{}'", json));
                }

                DinocoValue::DateTime(dt) => {
                    let val = dt.to_string().replace('\'', "''");
                    builder.push(&format!("'{}'", val));
                }

                _ => builder.push("NULL"),
            },
        }
    }
}
