use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::StreamExt;

use mysql_async::{Params::Positional, Pool, Row, Value, prelude::Queryable};

use crate::{
    AlterEnumStatement, ColumnDefinition, ColumnType, CreateEnumStatement, DinocoAdapter, DinocoDatabaseRow, DinocoError, DinocoResult, DinocoRow, DinocoStream, DinocoType,
    DinocoValue, DropEnumStatement, DropTableStatement, SqlBuilder, SqlDialect, SqlDialectBuilders,
};

pub struct MySqlAdapter {
    pub url: String,
    pub client: Arc<Pool>,
}

pub struct MySqlDialect;

static MYSQL_DIALECT: MySqlDialect = MySqlDialect;

#[async_trait]
impl DinocoAdapter for MySqlAdapter {
    type Dialect = MySqlDialect;

    fn dialect(&self) -> &Self::Dialect {
        &MYSQL_DIALECT
    }

    async fn connect(url: String) -> DinocoResult<Self> {
        Ok(Self {
            client: Arc::new(Pool::new(url.as_str())),
            url,
        })
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<()> {
        let params = Positional(params.iter().cloned().map(Into::into).collect());

        let mut conn = self.client.get_conn().await?;

        conn.exec_drop(query, params).await?;

        Ok(())
    }

    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>> {
        let params = Positional(params.iter().cloned().map(Into::into).collect());
        let mut conn = self.client.get_conn().await?;

        let db_rows: Vec<Row> = conn.exec(query, params).await?;
        let mut results = Vec::with_capacity(db_rows.len());

        for db_row in db_rows {
            results.push(T::from_row(&db_row)?);
        }

        Ok(results)
    }

    async fn stream_as<T: DinocoRow + Send + 'static>(&self, query: &str, params: &[DinocoValue]) -> DinocoStream<T> {
        let client = self.client.clone();
        let query_owned = query.to_string();
        let params_owned: Vec<Value> = params.iter().cloned().map(Into::into).collect();

        let s = async_stream::try_stream! {
            let mut conn = client.get_conn().await.map_err(DinocoError::from)?;

            let result = conn.exec_iter(query_owned, Positional(params_owned)).await.map_err(DinocoError::from)?;
            let mut row_stream = result.stream_and_drop::<Row>().await?.ok_or_else(|| DinocoError::ParseError("No rows returned".into()))?;

            while let Some(row) = row_stream.next().await {
                let row = row.map_err(DinocoError::from)?;
                yield T::from_row(&row)?;
            }
        };

        Box::pin(s)
    }
}

impl From<DinocoValue> for Value {
    fn from(val: DinocoValue) -> Self {
        match val {
            DinocoValue::Null => Value::NULL,
            DinocoValue::Integer(i) => Value::Int(i),
            DinocoValue::Float(f) => Value::Double(f),
            DinocoValue::String(s) => Value::Bytes(s.into_bytes()),
            DinocoValue::Boolean(b) => Value::Int(if b { 1 } else { 0 }),
            DinocoValue::Json(v) => Value::Bytes(v.to_string().into_bytes()),
            DinocoValue::Bytes(v) => Value::Bytes(v),
            DinocoValue::DateTime(dt) => Value::Bytes(dt.format("%Y-%m-%d %H:%M:%S").to_string().into_bytes()),
        }
    }
}

impl DinocoDatabaseRow for Row {
    fn get_i64(&self, idx: usize) -> DinocoResult<i64> {
        self.get::<i64, _>(idx)
            .ok_or_else(|| DinocoError::ParseError(format!("Failed to get i64 at column {}", idx)))
    }

    fn get_string(&self, idx: usize) -> DinocoResult<String> {
        if let Some(Ok(val)) = self.get_opt::<String, _>(idx) {
            return Ok(val);
        }

        if let Some(Ok(bytes)) = self.get_opt::<Vec<u8>, _>(idx) {
            return String::from_utf8(bytes).map_err(|_| DinocoError::ParseError(format!("Column {} is not valid UTF-8", idx)));
        }

        Err(DinocoError::ParseError(format!("Failed to get String at column {}. Is it Null?", idx)))
    }

    fn get_bytes(&self, idx: usize) -> DinocoResult<Vec<u8>> {
        self.get::<Vec<u8>, _>(idx)
            .ok_or_else(|| DinocoError::ParseError(format!("Failed to get Bytes at column {}", idx)))
    }

    fn get_bool(&self, idx: usize) -> DinocoResult<bool> {
        if let Some(v) = self.get::<bool, _>(idx) {
            return Ok(v);
        }

        if let Some(v) = self.get::<i64, _>(idx) {
            return Ok(v != 0);
        }

        if let Some(v) = self.get::<i8, _>(idx) {
            return Ok(v != 0);
        }

        if let Some(v) = self.get::<u8, _>(idx) {
            return Ok(v != 0);
        }

        Err(DinocoError::ParseError(format!("Failed to get bool at column {}", idx)))
    }

    fn get_f64(&self, idx: usize) -> DinocoResult<f64> {
        self.get::<f64, _>(idx)
            .ok_or_else(|| DinocoError::ParseError(format!("Failed to get f64 at column {}", idx)))
    }

    fn get<T: DinocoType>(&self, idx: usize) -> DinocoResult<T> {
        T::from_row(self, idx)
    }
}

impl SqlDialect for MySqlDialect {
    fn default_schema(&self) -> String {
        "DATABASE()".to_string()
    }

    fn cast_boolean(&self, expr: &str) -> String {
        format!("CASE WHEN {} = 'YES' THEN TRUE ELSE FALSE END", expr)
    }

    fn bind_param(&self, _index: usize) -> String {
        "?".to_string()
    }

    fn identifier(&self, v: &str) -> String {
        let escaped = v.replace('`', "``");
        format!("`{}`", escaped)
    }

    fn literal_string(&self, v: &str) -> String {
        let escaped = v.replace('\'', "''");
        format!("'{}'", escaped)
    }

    fn modify_column(&self) -> String {
        "MODIFY COLUMN".to_string()
    }

    fn supports_native_enums(&self) -> bool {
        false
    }

    fn query_get_foreign_keys(&self) -> String {
        "SELECT 
			TABLE_NAME as table_name, CONSTRAINT_NAME as constraint_name
				FROM information_schema.KEY_COLUMN_USAGE 
					WHERE REFERENCED_TABLE_SCHEMA = DATABASE() 
						AND REFERENCED_TABLE_NAME IS NOT NULL;"
            .to_string()
    }

    fn column_type(&self, col: &ColumnDefinition, is_primary: bool, auto_increment: bool) -> String {
        let base_type = match &col.col_type {
            ColumnType::Integer => "BIGINT".to_string(),
            ColumnType::Float => "DOUBLE PRECISION".to_string(),
            ColumnType::Text => "VARCHAR(255)".to_string(),
            ColumnType::Boolean => "TINYINT(1)".to_string(),
            ColumnType::Json => "JSON".to_string(),
            ColumnType::DateTime => "TIMESTAMP".to_string(),
            ColumnType::Bytes => "BLOB".to_string(),
            ColumnType::Enum(name) => {
                format!("VARCHAR(255) /* enum {} */", name)
            }
            ColumnType::EnumInline(values) => {
                let safe_values = values.iter().map(|v| format!("'{}'", v.replace('\'', "''"))).collect::<Vec<_>>().join(", ");

                format!("ENUM({})", safe_values)
            }
        };

        let mut definition = base_type;

        if auto_increment {
            definition.push_str(" AUTO_INCREMENT");
        }

        if is_primary {
            definition.push_str(" PRIMARY KEY");
        }

        definition
    }
}

impl SqlDialectBuilders for MySqlDialect {
    fn build_create_enum<'a>(&self, _stmt: &CreateEnumStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        ("".to_string(), vec![])
    }

    fn build_alter_enum<'a>(&self, _stmt: &AlterEnumStatement<'a, Self>) -> Vec<(String, Vec<DinocoValue>)> {
        vec![]
    }

    fn build_drop_enum<'a>(&self, _stmt: &DropEnumStatement<'a, Self>) -> (String, Vec<DinocoValue>) {
        ("".to_string(), vec![])
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
}
