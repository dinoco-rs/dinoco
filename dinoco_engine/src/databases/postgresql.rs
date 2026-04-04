use async_trait::async_trait;

use std::str::FromStr;
use std::sync::Arc;

use futures::stream::{self, TryStreamExt};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};

use tokio_postgres::types::{IsNull, Json, ToSql, Type, private::BytesMut, to_sql_checked};
use tokio_postgres::{NoTls, Row};

use crate::{
    AlterAction, AlterTableStatement, ColumnDefinition, ColumnType, ConstraintType, DinocoAdapter, DinocoDatabaseRow, DinocoError, DinocoResult, DinocoRow, DinocoStream,
    DinocoType, DinocoValue, SqlBuilder, SqlDialect, SqlDialectBuilders,
};

pub struct PostgresDialect;
pub struct PostgresAdapter {
    pub url: String,
    pub client: Arc<Pool>,
}

static POSTGRES_DIALECT: PostgresDialect = PostgresDialect;

#[async_trait]
impl DinocoAdapter for PostgresAdapter {
    type Dialect = PostgresDialect;

    fn dialect(&self) -> &Self::Dialect {
        &POSTGRES_DIALECT
    }

    async fn connect(url: String) -> DinocoResult<Self> {
        let pg_config = tokio_postgres::Config::from_str(&url).map_err(|e| DinocoError::from(e))?;

        let mgr = Manager::from_config(
            pg_config,
            NoTls,
            ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
            },
        );

        let pool = Pool::builder(mgr).max_size(16).build().map_err(|e| DinocoError::from(e))?;

        Ok(Self { url, client: Arc::new(pool) })
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<()> {
        let pg_params: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as _).collect();
        let client = self.client.get().await.map_err(|e| DinocoError::from(e))?;

        client.execute(query, &pg_params).await?;

        Ok(())
    }

    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>> {
        let pg_params: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as _).collect();
        let client = self.client.get().await.map_err(|e| DinocoError::from(e))?;

        let db_rows = client.query(query, &pg_params).await?;
        let mut results = Vec::with_capacity(db_rows.len());

        for db_row in db_rows {
            results.push(T::from_row(&db_row)?);
        }

        Ok(results)
    }

    async fn stream_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoStream<T> {
        let client = self.client.clone();

        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        let stream = stream::once(async move {
            let client = client.get().await.map_err(|e| DinocoError::from(e))?;
            let pg_params: Vec<&(dyn ToSql + Sync)> = params_owned.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

            let row_stream = client.query_raw(&query_owned, pg_params.iter().copied()).await.map_err(DinocoError::from)?;
            let row_stream = row_stream.map_err(DinocoError::from);

            Ok::<_, DinocoError>(row_stream)
        })
        .try_flatten()
        .and_then(|row| async move { T::from_row(&row) });

        Box::pin(stream)
    }
}

impl ToSql for DinocoValue {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            DinocoValue::Null => Ok(IsNull::Yes),
            DinocoValue::Integer(i) => i.to_sql(ty, out),
            DinocoValue::Float(f) => f.to_sql(ty, out),
            DinocoValue::Boolean(b) => b.to_sql(ty, out),
            DinocoValue::String(s) => s.to_sql(ty, out),
            DinocoValue::Json(v) => Json(v).to_sql(ty, out),

            DinocoValue::Bytes(v) => v.to_sql(ty, out),
            DinocoValue::DateTime(dt) => dt.to_string().to_sql(ty, out),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

impl DinocoDatabaseRow for Row {
    fn get_i64(&self, idx: usize) -> DinocoResult<i64> {
        Ok(self.try_get(idx)?)
    }

    fn get_string(&self, idx: usize) -> DinocoResult<String> {
        Ok(self.try_get(idx)?)
    }

    fn get_bool(&self, idx: usize) -> DinocoResult<bool> {
        Ok(self.try_get(idx)?)
    }

    fn get_f64(&self, idx: usize) -> DinocoResult<f64> {
        Ok(self.try_get(idx)?)
    }

    fn get_bytes(&self, idx: usize) -> DinocoResult<Vec<u8>> {
        Ok(self.try_get(idx)?)
    }

    fn get<T: DinocoType>(&self, idx: usize) -> DinocoResult<T> {
        T::from_row(self, idx)
    }
}

impl SqlDialect for PostgresDialect {
    fn default_schema(&self) -> String {
        "public".to_string()
    }

    fn cast_boolean(&self, expr: &str) -> String {
        format!("CAST({} = 'YES' AS BOOLEAN)", expr)
    }

    fn bind_param(&self, index: usize) -> String {
        format!("${}", index)
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
        "ALTER COLUMN".to_string()
    }

    fn supports_native_enums(&self) -> bool {
        true
    }

    fn query_get_foreign_keys(&self) -> String {
        "SELECT 
			table_name, constraint_name
				FROM information_schema.table_constraints 
					WHERE 
						constraint_type = 'FOREIGN KEY' 
							AND table_schema = current_schema();"
            .to_string()
    }

    fn query_get_enums(&self) -> String {
        "SELECT t.typname as name
			FROM pg_type t
				JOIN pg_enum e ON t.oid = e.enumtypid
				JOIN pg_namespace n ON n.oid = t.typnamespace
			WHERE n.nspname = current_schema()
				GROUP BY t.typname"
            .to_string()
    }

    fn column_type(&self, col: &ColumnDefinition, is_primary: bool, auto_increment: bool) -> String {
        let mut base_type = match &col.col_type {
            ColumnType::Integer => "BIGINT".to_string(),
            ColumnType::Float => "DOUBLE PRECISION".to_string(),
            ColumnType::Text => "TEXT".to_string(),
            ColumnType::Boolean => "BOOLEAN".to_string(),
            ColumnType::Json => "JSONB".to_string(),
            ColumnType::DateTime => "TIMESTAMP".to_string(),
            ColumnType::Bytes => "BYTEA".to_string(),

            ColumnType::Enum(name) => self.identifier(name),
            ColumnType::EnumInline(_) => "TEXT".into(),
        };

        if auto_increment {
            base_type.push_str(" GENERATED ALWAYS AS IDENTITY");
        }

        if is_primary {
            base_type.push_str(" PRIMARY KEY");
        }

        base_type
    }
}

impl SqlDialectBuilders for PostgresDialect {
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

                    builder.push(&self.column_type(&col, col.primary_key, col.auto_increment));

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

                AlterAction::ModifyColumn(_, _, col) => {
                    let table = self.identifier(stmt.table_name);
                    let column = self.identifier(col.name);

                    let col_type = self.column_type(&col, false, col.auto_increment);

                    let type_sql = if let ColumnType::Enum(enum_name) = &col.col_type {
                        format!(
                            "ALTER TABLE {} ALTER COLUMN {} TYPE {} USING {}::text::{}",
                            table,
                            column,
                            self.identifier(enum_name),
                            column,
                            self.identifier(enum_name),
                        )
                    } else {
                        format!("ALTER TABLE {} ALTER COLUMN {} TYPE {}", table, column, col_type)
                    };

                    statements.push((type_sql, vec![]));

                    if col.not_null {
                        statements.push((format!("ALTER TABLE {} ALTER COLUMN {} SET NOT NULL", table, column), vec![]));
                    } else {
                        statements.push((format!("ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL", table, column), vec![]));
                    }

                    match &col.default {
                        Some(default_val) => {
                            let mut builder = SqlBuilder::new(self, 128);

                            builder.push("ALTER TABLE ");
                            builder.push_identifier(stmt.table_name);
                            builder.push(" ALTER COLUMN ");
                            builder.push_identifier(col.name);
                            builder.push(" SET");

                            self.push_default_value(&mut builder, default_val);

                            statements.push(builder.finish());
                        }
                        None => {
                            statements.push((format!("ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT", table, column), vec![]));
                        }
                    }

                    continue;
                }

                AlterAction::RenameColumn { old_name, new_name } => {
                    builder.push("RENAME COLUMN ");
                    builder.push_identifier(old_name);
                    builder.push(" TO ");
                    builder.push_identifier(new_name);
                }

                AlterAction::AddConstraint(_, _, constraint) => {
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
                AlterAction::DropConstraint(_, _, name) => {
                    builder.push("DROP CONSTRAINT ");
                    builder.push_identifier(name);
                }
            }

            statements.push(builder.finish());
        }

        statements
    }
}
