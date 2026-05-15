use chrono::{Datelike, Timelike};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use mysql_async::{Params::Positional, Pool, Row, Value, prelude::Queryable};

use crate::{
    ConstraintError, DinocoAdapter, DinocoClientConfig, DinocoError, DinocoQueryLog, DinocoQueryLogger, DinocoResult,
    DinocoRow, DinocoTransactionAdapter, DinocoValue, ExecutionResult,
};

mod dialect;
mod handler;
mod migration;
mod row;

pub use dialect::MySqlDialect;

static MYSQL_DIALECT: MySqlDialect = MySqlDialect;
tokio::task_local! {
    static MYSQL_TX_CONNECTION: Arc<tokio::sync::Mutex<mysql_async::Conn>>;
}

#[derive(Clone)]
pub struct MySqlAdapter {
    pub url: String,
    pub client: Arc<Pool>,
    pub query_logger: DinocoQueryLogger,
}

#[async_trait]
impl DinocoAdapter for MySqlAdapter {
    type Dialect = MySqlDialect;

    fn dialect(&self) -> &Self::Dialect {
        &MYSQL_DIALECT
    }

    async fn connect(url: String, config: DinocoClientConfig) -> DinocoResult<Self> {
        Ok(Self { client: Arc::new(Pool::new(url.as_str())), query_logger: config.query_logger, url })
    }

    async fn execute_result(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<ExecutionResult> {
        if let Ok(tx_connection) = MYSQL_TX_CONNECTION.try_with(Clone::clone) {
            let mut connection = tx_connection.lock().await;

            return execute_result_with_connection(&mut connection, query, params, &self.query_logger).await;
        }

        let mut connection = self.client.get_conn().await?;

        execute_result_with_connection(&mut connection, query, params, &self.query_logger).await
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

    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>> {
        if let Ok(tx_connection) = MYSQL_TX_CONNECTION.try_with(Clone::clone) {
            let mut connection = tx_connection.lock().await;

            return query_as_with_connection::<T>(&mut connection, query, params, &self.query_logger).await;
        }

        let mut connection = self.client.get_conn().await?;

        query_as_with_connection::<T>(&mut connection, query, params, &self.query_logger).await
    }
}

impl DinocoTransactionAdapter for MySqlAdapter {
    fn with_transaction<'a, T, F>(&'a self, operation: F) -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>>
    where
        T: Send + 'a,
        F: FnOnce() -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>> + Send + 'a,
    {
        Box::pin(async move {
            if MYSQL_TX_CONNECTION.try_with(|_| ()).is_ok() {
                return operation().await;
            }

            let mut connection = self.client.get_conn().await?;
            connection.query_drop("START TRANSACTION").await?;

            let tx_connection = Arc::new(tokio::sync::Mutex::new(connection));
            let result = MYSQL_TX_CONNECTION.scope(tx_connection.clone(), async move { operation().await }).await;

            match result {
                Ok(output) => {
                    let mut connection = tx_connection.lock().await;
                    connection.query_drop("COMMIT").await?;

                    Ok(output)
                }
                Err(error) => {
                    let mut connection = tx_connection.lock().await;
                    let _ = connection.query_drop("ROLLBACK").await;

                    Err(error)
                }
            }
        })
    }
}

async fn execute_result_with_connection(
    connection: &mut mysql_async::Conn,
    query: &str,
    params: &[DinocoValue],
    query_logger: &DinocoQueryLogger,
) -> DinocoResult<ExecutionResult> {
    let logged_params = params.to_vec();
    let params = Positional(logged_params.iter().cloned().map(Into::into).collect());
    let started_at = Instant::now();

    connection.exec_drop(query, params).await?;
    let affected_rows = connection.affected_rows();
    let last_insert_id = connection.last_insert_id().map(|value| value as i64);

    query_logger.log(DinocoQueryLog {
        adapter: "mysql",
        duration: started_at.elapsed(),
        params: logged_params,
        query: query.to_string(),
    });

    Ok(ExecutionResult { affected_rows, last_insert_id })
}

async fn query_as_with_connection<T: DinocoRow>(
    connection: &mut mysql_async::Conn,
    query: &str,
    params: &[DinocoValue],
    query_logger: &DinocoQueryLogger,
) -> DinocoResult<Vec<T>> {
    let logged_params = params.to_vec();
    let params = Positional(logged_params.iter().cloned().map(Into::into).collect());
    let started_at = Instant::now();

    let db_rows: Vec<Row> = connection.exec(query, params).await?;
    let mut results = Vec::with_capacity(db_rows.len());

    for db_row in db_rows {
        results.push(T::from_row(&db_row)?);
    }

    query_logger.log(DinocoQueryLog {
        adapter: "mysql",
        duration: started_at.elapsed(),
        params: logged_params,
        query: query.to_string(),
    });

    Ok(results)
}

impl From<DinocoValue> for Value {
    fn from(val: DinocoValue) -> Self {
        match val {
            DinocoValue::Null => Value::NULL,
            DinocoValue::Integer(i) => Value::Int(i),
            DinocoValue::Float(f) => Value::Double(f),
            DinocoValue::String(s) => Value::Bytes(s.into_bytes()),
            DinocoValue::Enum(_, s) => Value::Bytes(s.into_bytes()),
            DinocoValue::Boolean(b) => Value::Int(if b { 1 } else { 0 }),
            DinocoValue::Json(v) => Value::Bytes(v.to_string().into_bytes()),
            DinocoValue::Bytes(v) => Value::Bytes(v),
            DinocoValue::DateTime(dt) => Value::Date(
                dt.year() as u16,
                dt.month() as u8,
                dt.day() as u8,
                dt.hour() as u8,
                dt.minute() as u8,
                dt.second() as u8,
                dt.timestamp_subsec_micros(),
            ),
            DinocoValue::Date(date) => {
                Value::Date(date.year() as u16, date.month() as u8, date.day() as u8, 0, 0, 0, 0)
            }
        }
    }
}

impl From<mysql_async::Error> for DinocoError {
    fn from(e: mysql_async::Error) -> Self {
        if let Some(error) = map_mysql_constraint_error(&e) {
            return Self::Constraint(error);
        }

        Self::MySql(e)
    }
}

fn map_mysql_constraint_error(error: &mysql_async::Error) -> Option<ConstraintError> {
    let mysql_async::Error::Server(server_error) = error else {
        return None;
    };
    let message = server_error.message.clone();

    match server_error.code {
        1062 => Some(ConstraintError::unique(None, Vec::new(), extract_mysql_constraint_name(&message), message)),
        1048 => Some(ConstraintError::not_null(
            None,
            extract_mysql_column_name(&message).into_iter().collect(),
            None,
            message,
        )),
        1451 | 1452 => {
            Some(ConstraintError::foreign_key(None, Vec::new(), extract_mysql_constraint_name(&message), message))
        }
        3819 | 4025 => Some(ConstraintError::check(None, Vec::new(), extract_mysql_constraint_name(&message), message)),
        _ => None,
    }
}

fn extract_mysql_constraint_name(message: &str) -> Option<String> {
    extract_quoted_segments(message).into_iter().last()
}

fn extract_mysql_column_name(message: &str) -> Option<String> {
    extract_quoted_segments(message).into_iter().next()
}

fn extract_quoted_segments(message: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for ch in message.chars() {
        match ch {
            '\'' if in_quote => {
                segments.push(current.clone());
                current.clear();
                in_quote = false;
            }
            '\'' => in_quote = true,
            _ if in_quote => current.push(ch),
            _ => {}
        }
    }

    segments
}
