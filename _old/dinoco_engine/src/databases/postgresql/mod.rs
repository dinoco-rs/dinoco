use async_trait::async_trait;

use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};

use tokio_postgres::NoTls;
use tokio_postgres::types::{IsNull, Json, ToSql, Type, private::BytesMut, to_sql_checked};

use crate::{
    ConstraintError, DinocoAdapter, DinocoClientConfig, DinocoError, DinocoQueryLog, DinocoQueryLogger, DinocoResult,
    DinocoRow, DinocoTransactionAdapter, DinocoValue, ExecutionResult,
};

mod dialect;
mod handler;
mod migration;
mod row;

pub use dialect::PostgresDialect;

static POSTGRES_DIALECT: PostgresDialect = PostgresDialect;
tokio::task_local! {
    static POSTGRES_TX_CONNECTION: Arc<tokio::sync::Mutex<deadpool_postgres::Object>>;
}

#[derive(Clone)]
pub struct PostgresAdapter {
    pub url: String,
    pub client: Arc<Pool>,
    pub query_logger: DinocoQueryLogger,
}

#[async_trait]
impl DinocoAdapter for PostgresAdapter {
    type Dialect = PostgresDialect;

    fn dialect(&self) -> &Self::Dialect {
        &POSTGRES_DIALECT
    }

    async fn connect(url: String, config: DinocoClientConfig) -> DinocoResult<Self> {
        let pg_config = tokio_postgres::Config::from_str(&url).map_err(|e| DinocoError::from(e))?;

        let mgr = Manager::from_config(pg_config, NoTls, ManagerConfig { recycling_method: RecyclingMethod::Fast });

        let pool = Pool::builder(mgr).max_size(16).build().map_err(|e| DinocoError::from(e))?;

        Ok(Self { url, client: Arc::new(pool), query_logger: config.query_logger })
    }

    async fn execute_result(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<ExecutionResult> {
        if let Ok(tx_connection) = POSTGRES_TX_CONNECTION.try_with(Clone::clone) {
            let connection = tx_connection.lock().await;

            return execute_result_with_connection(&connection, query, params, &self.query_logger).await;
        }

        let connection = self.client.get().await.map_err(|e| DinocoError::from(e))?;

        execute_result_with_connection(&connection, query, params, &self.query_logger).await
    }

    async fn execute_script(&self, sql_content: &str) -> DinocoResult<()> {
        let clean_sql = sql_content.trim();

        if clean_sql.is_empty() {
            return Ok(());
        }

        let client = self.client.get().await.map_err(|e| DinocoError::from(e))?;
        let started_at = Instant::now();

        client.batch_execute(clean_sql).await?;
        self.query_logger.log(DinocoQueryLog {
            adapter: "postgresql",
            duration: started_at.elapsed(),
            params: Vec::new(),
            query: clean_sql.to_string(),
        });

        Ok(())
    }

    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>> {
        if let Ok(tx_connection) = POSTGRES_TX_CONNECTION.try_with(Clone::clone) {
            let connection = tx_connection.lock().await;

            return query_as_with_connection::<T>(&connection, query, params, &self.query_logger).await;
        }

        let connection = self.client.get().await.map_err(|e| DinocoError::from(e))?;

        query_as_with_connection::<T>(&connection, query, params, &self.query_logger).await
    }
}

impl DinocoTransactionAdapter for PostgresAdapter {
    fn with_transaction<'a, T, F>(&'a self, operation: F) -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>>
    where
        T: Send + 'a,
        F: FnOnce() -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>> + Send + 'a,
    {
        Box::pin(async move {
            if POSTGRES_TX_CONNECTION.try_with(|_| ()).is_ok() {
                return operation().await;
            }

            let connection = self.client.get().await.map_err(|error| DinocoError::from(error))?;
            let tx_connection = Arc::new(tokio::sync::Mutex::new(connection));

            {
                let connection = tx_connection.lock().await;
                connection.batch_execute("BEGIN").await?;
            }

            let result = POSTGRES_TX_CONNECTION.scope(tx_connection.clone(), async move { operation().await }).await;

            match result {
                Ok(output) => {
                    let connection = tx_connection.lock().await;
                    connection.batch_execute("COMMIT").await?;

                    Ok(output)
                }
                Err(error) => {
                    let connection = tx_connection.lock().await;
                    let _ = connection.batch_execute("ROLLBACK").await;

                    Err(error)
                }
            }
        })
    }
}

async fn execute_result_with_connection(
    connection: &deadpool_postgres::Object,
    query: &str,
    params: &[DinocoValue],
    query_logger: &DinocoQueryLogger,
) -> DinocoResult<ExecutionResult> {
    let pg_params: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as _).collect();
    let started_at = Instant::now();
    let affected_rows = connection.execute(query, &pg_params).await?;

    query_logger.log(DinocoQueryLog {
        adapter: "postgresql",
        duration: started_at.elapsed(),
        params: params.to_vec(),
        query: query.to_string(),
    });

    Ok(ExecutionResult { affected_rows, last_insert_id: None })
}

async fn query_as_with_connection<T: DinocoRow>(
    connection: &deadpool_postgres::Object,
    query: &str,
    params: &[DinocoValue],
    query_logger: &DinocoQueryLogger,
) -> DinocoResult<Vec<T>> {
    let pg_params: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| p as _).collect();
    let started_at = Instant::now();
    let db_rows = connection.query(query, &pg_params).await?;
    let mut results = Vec::with_capacity(db_rows.len());

    for db_row in db_rows {
        results.push(T::from_row(&db_row)?);
    }

    query_logger.log(DinocoQueryLog {
        adapter: "postgresql",
        duration: started_at.elapsed(),
        params: params.to_vec(),
        query: query.to_string(),
    });

    Ok(results)
}

impl ToSql for DinocoValue {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            DinocoValue::Null => Ok(IsNull::Yes),
            DinocoValue::Integer(i) => i.to_sql(ty, out),
            DinocoValue::Float(f) => f.to_sql(ty, out),
            DinocoValue::Boolean(b) => b.to_sql(ty, out),
            DinocoValue::String(s) => s.as_str().to_sql(ty, out),
            DinocoValue::Enum(_, s) => s.as_str().to_sql(ty, out),
            DinocoValue::Json(v) => Json(v).to_sql(ty, out),
            DinocoValue::Bytes(v) => v.to_sql(ty, out),
            DinocoValue::DateTime(dt) => dt.to_sql(ty, out),
            DinocoValue::Date(date) => date.to_sql(ty, out),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

impl From<tokio_postgres::Error> for DinocoError {
    fn from(e: tokio_postgres::Error) -> Self {
        if let Some(error) = map_postgres_constraint_error(&e) {
            return Self::Constraint(error);
        }

        Self::Postgres(e)
    }
}

impl From<deadpool_postgres::PoolError> for DinocoError {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        Self::ConnectionError(format!("Failed to get connection from pool: {}", e))
    }
}

impl From<deadpool_postgres::BuildError> for DinocoError {
    fn from(e: deadpool_postgres::BuildError) -> Self {
        Self::ConnectionError(format!("Failed to build connection pool: {}", e))
    }
}

fn map_postgres_constraint_error(error: &tokio_postgres::Error) -> Option<ConstraintError> {
    let db_error = error.as_db_error()?;
    let code = db_error.code().code();
    let table = db_error.table().map(str::to_string);
    let columns = db_error.column().map(|item| vec![item.to_string()]).unwrap_or_default();
    let constraint = db_error.constraint().map(str::to_string);
    let message = db_error.message().to_string();

    match code {
        "23505" => Some(ConstraintError::unique(table, columns, constraint, message)),
        "23503" => Some(ConstraintError::foreign_key(table, columns, constraint, message)),
        "23502" => Some(ConstraintError::not_null(table, columns, constraint, message)),
        "23514" => Some(ConstraintError::check(table, columns, constraint, message)),
        _ => None,
    }
}
