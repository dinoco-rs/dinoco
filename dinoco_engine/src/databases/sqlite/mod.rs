use async_trait::async_trait;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use deadpool_sqlite::{Config, Pool, Runtime};
use rusqlite::types::{ToSqlOutput, Value};

mod dialect;
mod handler;
mod migration;
mod row;

use crate::{
    ConstraintError, DinocoAdapter, DinocoClientConfig, DinocoError, DinocoQueryLog, DinocoQueryLogger, DinocoResult,
    DinocoRow, DinocoTransactionAdapter, DinocoValue, ExecutionResult,
};

pub use dialect::SqliteDialect;

static SQLITE_DIALECT: SqliteDialect = SqliteDialect;
tokio::task_local! {
    static SQLITE_TX_CONNECTION: Arc<tokio::sync::Mutex<deadpool_sqlite::Object>>;
}

#[derive(Clone)]
pub struct SqliteAdapter {
    pub url: String,
    pub pool: Arc<Pool>,
    pub query_logger: DinocoQueryLogger,
}

#[async_trait]
impl DinocoAdapter for SqliteAdapter {
    type Dialect = SqliteDialect;

    fn dialect(&self) -> &Self::Dialect {
        &SQLITE_DIALECT
    }

    async fn connect(url: String, config: DinocoClientConfig) -> DinocoResult<Self> {
        let cfg = Config::new(&url);
        let pool = cfg.create_pool(Runtime::Tokio1).map_err(DinocoError::from)?;

        Ok(Self { url, pool: Arc::new(pool), query_logger: config.query_logger })
    }

    async fn execute_result(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<ExecutionResult> {
        if let Ok(tx_conn) = SQLITE_TX_CONNECTION.try_with(Clone::clone) {
            let conn = tx_conn.lock().await;

            return execute_result_with_connection(&conn, query, params, &self.query_logger).await;
        }

        let conn = self.pool.get().await.map_err(DinocoError::from)?;

        execute_result_with_connection(&conn, query, params, &self.query_logger).await
    }

    async fn execute_script(&self, sql_content: &str) -> DinocoResult<()> {
        for statement in sql_content.split(';') {
            let clean_statement = statement.trim();

            if clean_statement.is_empty() {
                continue;
            }

            self.execute(clean_statement, &[]).await?;
        }

        Ok(())
    }

    async fn query_as<T: DinocoRow + Send + 'static>(
        &self,
        query: &str,
        params: &[DinocoValue],
    ) -> DinocoResult<Vec<T>> {
        if let Ok(tx_conn) = SQLITE_TX_CONNECTION.try_with(Clone::clone) {
            let conn = tx_conn.lock().await;

            return query_as_with_connection::<T>(&conn, query, params, &self.query_logger).await;
        }

        let conn = self.pool.get().await.map_err(DinocoError::from)?;

        query_as_with_connection::<T>(&conn, query, params, &self.query_logger).await
    }
}

impl DinocoTransactionAdapter for SqliteAdapter {
    fn with_transaction<'a, T, F>(&'a self, operation: F) -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>>
    where
        T: Send + 'a,
        F: FnOnce() -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>> + Send + 'a,
    {
        Box::pin(async move {
            if SQLITE_TX_CONNECTION.try_with(|_| ()).is_ok() {
                return operation().await;
            }

            let connection = self.pool.get().await.map_err(DinocoError::from)?;
            let tx_connection = Arc::new(tokio::sync::Mutex::new(connection));

            {
                let conn = tx_connection.lock().await;
                conn.interact(|conn| conn.execute("BEGIN", []))
                    .await
                    .map_err(DinocoError::from)?
                    .map_err(DinocoError::from)?;
            }

            let result = SQLITE_TX_CONNECTION.scope(tx_connection.clone(), async move { operation().await }).await;

            match result {
                Ok(output) => {
                    let conn = tx_connection.lock().await;
                    conn.interact(|conn| conn.execute("COMMIT", []))
                        .await
                        .map_err(DinocoError::from)?
                        .map_err(DinocoError::from)?;

                    Ok(output)
                }
                Err(error) => {
                    let conn = tx_connection.lock().await;
                    let _ = conn.interact(|conn| conn.execute("ROLLBACK", [])).await;

                    Err(error)
                }
            }
        })
    }
}

async fn execute_result_with_connection(
    conn: &deadpool_sqlite::Object,
    query: &str,
    params: &[DinocoValue],
    query_logger: &DinocoQueryLogger,
) -> DinocoResult<ExecutionResult> {
    let query_owned = query.to_string();
    let params_owned = params.to_vec();
    let logged_query = query.to_string();
    let logged_params = params.to_vec();
    let started_at = Instant::now();

    let affected_rows = conn
        .interact(move |conn| {
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            conn.execute(&query_owned, params_refs.as_slice())
                .map(|affected_rows| (affected_rows, conn.last_insert_rowid()))
        })
        .await
        .map_err(DinocoError::from)?
        .map_err(DinocoError::from)?;

    query_logger.log(DinocoQueryLog {
        adapter: "sqlite",
        duration: started_at.elapsed(),
        params: logged_params,
        query: logged_query,
    });

    Ok(ExecutionResult { affected_rows: affected_rows.0 as u64, last_insert_id: Some(affected_rows.1) })
}

async fn query_as_with_connection<T: DinocoRow + Send + 'static>(
    conn: &deadpool_sqlite::Object,
    query: &str,
    params: &[DinocoValue],
    query_logger: &DinocoQueryLogger,
) -> DinocoResult<Vec<T>> {
    let query_owned = query.to_string();
    let params_owned = params.to_vec();
    let logged_query = query.to_string();
    let logged_params = params.to_vec();
    let started_at = Instant::now();

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

    query_logger.log(DinocoQueryLog {
        adapter: "sqlite",
        duration: started_at.elapsed(),
        params: logged_params,
        query: logged_query,
    });

    Ok(results)
}

impl rusqlite::ToSql for DinocoValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            DinocoValue::Null => Ok(ToSqlOutput::Owned(Value::Null)),
            DinocoValue::Integer(i) => Ok(ToSqlOutput::Owned(Value::Integer(*i))),
            DinocoValue::Float(f) => Ok(ToSqlOutput::Owned(Value::Real(*f))),
            DinocoValue::Boolean(b) => Ok(ToSqlOutput::Owned(Value::Integer(if *b { 1 } else { 0 }))),
            DinocoValue::String(s) => Ok(ToSqlOutput::Owned(Value::Text(s.clone()))),
            DinocoValue::Enum(_, s) => Ok(ToSqlOutput::Owned(Value::Text(s.clone()))),
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
        if let Some(error) = map_sqlite_constraint_error(&e) {
            return Self::Constraint(error);
        }

        Self::Sqlite(e)
    }
}

fn map_sqlite_constraint_error(error: &rusqlite::Error) -> Option<ConstraintError> {
    let rusqlite::Error::SqliteFailure(_, message) = error else {
        return None;
    };
    let message = message.clone()?;
    let normalized = message.to_ascii_lowercase();

    if normalized.starts_with("unique constraint failed:") {
        let targets = parse_sqlite_constraint_targets(&message, "UNIQUE constraint failed:");
        let table = targets.first().and_then(|target| target.split('.').next()).map(str::to_string);
        let columns =
            targets.into_iter().map(|target| target.split('.').nth(1).unwrap_or(target.as_str()).to_string()).collect();

        return Some(ConstraintError::unique(table, columns, None, message));
    }

    if normalized.starts_with("not null constraint failed:") {
        let targets = parse_sqlite_constraint_targets(&message, "NOT NULL constraint failed:");
        let table = targets.first().and_then(|target| target.split('.').next()).map(str::to_string);
        let columns =
            targets.into_iter().map(|target| target.split('.').nth(1).unwrap_or(target.as_str()).to_string()).collect();

        return Some(ConstraintError::not_null(table, columns, None, message));
    }

    if normalized.starts_with("foreign key constraint failed") {
        return Some(ConstraintError::foreign_key(None, Vec::new(), None, message));
    }

    if normalized.starts_with("check constraint failed:") {
        let constraint =
            message.split_once(':').map(|(_, rest)| rest.trim().to_string()).filter(|item| !item.is_empty());

        return Some(ConstraintError::check(None, Vec::new(), constraint, message));
    }

    None
}

fn parse_sqlite_constraint_targets(message: &str, prefix: &str) -> Vec<String> {
    message
        .strip_prefix(prefix)
        .unwrap_or(message)
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}
