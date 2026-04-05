use async_trait::async_trait;

use std::sync::Arc;

use deadpool_sqlite::{Config, Pool, Runtime};
use rusqlite::types::{ToSqlOutput, Value};

mod dialect;
mod handler;
mod migration;
mod row;

use crate::{DinocoAdapter, DinocoError, DinocoResult, DinocoRow, DinocoValue};

pub use dialect::SqliteDialect;

static SQLITE_DIALECT: SqliteDialect = SqliteDialect;

pub struct SqliteAdapter {
    pub url: String,
    pub pool: Arc<Pool>,
}

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
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            conn.execute(&query_owned, params_refs.as_slice())
        })
        .await
        .map_err(DinocoError::from)?
        .map_err(DinocoError::from)?;

        Ok(())
    }

    async fn query_as<T: DinocoRow + Send + 'static>(
        &self,
        query: &str,
        params: &[DinocoValue],
    ) -> DinocoResult<Vec<T>> {
        let conn = self.pool.get().await.map_err(DinocoError::from)?;
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        let results = conn
            .interact(move |conn| -> DinocoResult<Vec<T>> {
                let mut stmt = conn.prepare(&query_owned).map_err(DinocoError::from)?;
                let params_refs: Vec<&dyn rusqlite::ToSql> =
                    params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

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
            DinocoValue::Date(date) => Ok(ToSqlOutput::Owned(Value::Text(date.to_string()))),
        }
    }
}

impl From<deadpool_sqlite::CreatePoolError> for DinocoError {
    fn from(e: deadpool_sqlite::CreatePoolError) -> Self {
        Self::ConnectionError(format!("Failed to get connection from pool: {}", e))
    }
}

impl From<deadpool_sqlite::PoolError> for DinocoError {
    fn from(e: deadpool_sqlite::PoolError) -> Self {
        Self::ConnectionError(format!("Failed to get connection from pool: {}", e))
    }
}

impl From<deadpool_sqlite::BuildError> for DinocoError {
    fn from(e: deadpool_sqlite::BuildError) -> Self {
        Self::ConnectionError(format!("Failed to build connection pool: {}", e))
    }
}

impl From<deadpool_sqlite::InteractError> for DinocoError {
    fn from(e: deadpool_sqlite::InteractError) -> Self {
        Self::ParseError(e.to_string())
    }
}

impl From<rusqlite::Error> for DinocoError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
