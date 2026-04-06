use chrono::{Datelike, Timelike};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use mysql_async::{Params::Positional, Pool, Row, Value, prelude::Queryable};

use crate::{
    ConstraintError, DinocoAdapter, DinocoClientConfig, DinocoError, DinocoQueryLog, DinocoQueryLogger, DinocoResult,
    DinocoRow, DinocoValue,
};

mod dialect;
mod handler;
mod migration;
mod row;

pub use dialect::MySqlDialect;

static MYSQL_DIALECT: MySqlDialect = MySqlDialect;

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

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<()> {
        let logged_params = params.to_vec();
        let params = Positional(logged_params.iter().cloned().map(Into::into).collect());

        let mut conn = self.client.get_conn().await?;
        let started_at = Instant::now();

        conn.exec_drop(query, params).await?;
        self.query_logger.log(DinocoQueryLog {
            adapter: "mysql",
            duration: started_at.elapsed(),
            params: logged_params,
            query: query.to_string(),
        });

        Ok(())
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
        let logged_params = params.to_vec();
        let params = Positional(logged_params.iter().cloned().map(Into::into).collect());
        let mut conn = self.client.get_conn().await?;
        let started_at = Instant::now();

        let db_rows: Vec<Row> = conn.exec(query, params).await?;
        let mut results = Vec::with_capacity(db_rows.len());

        for db_row in db_rows {
            results.push(T::from_row(&db_row)?);
        }

        self.query_logger.log(DinocoQueryLog {
            adapter: "mysql",
            duration: started_at.elapsed(),
            params: logged_params,
            query: query.to_string(),
        });

        Ok(results)
    }
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
