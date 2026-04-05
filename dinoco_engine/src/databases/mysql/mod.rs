use chrono::{Datelike, Timelike};
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::StreamExt;

use mysql_async::{Params::Positional, Pool, Row, Value, prelude::Queryable};

use crate::{DinocoAdapter, DinocoError, DinocoResult, DinocoRow, DinocoStream, DinocoValue};

mod dialect;
mod handler;
mod migration;
mod row;

pub use dialect::MySqlDialect;

static MYSQL_DIALECT: MySqlDialect = MySqlDialect;

pub struct MySqlAdapter {
    pub url: String,
    pub client: Arc<Pool>,
}

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

    async fn query_as<T: DinocoRow>(
        &self,
        query: &str,
        params: &[DinocoValue],
    ) -> DinocoResult<Vec<T>> {
        let params = Positional(params.iter().cloned().map(Into::into).collect());
        let mut conn = self.client.get_conn().await?;

        let db_rows: Vec<Row> = conn.exec(query, params).await?;
        let mut results = Vec::with_capacity(db_rows.len());

        for db_row in db_rows {
            results.push(T::from_row(&db_row)?);
        }

        Ok(results)
    }

    async fn stream_as<T: DinocoRow + Send + 'static>(
        &self,
        query: &str,
        params: &[DinocoValue],
    ) -> DinocoStream<T> {
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
            DinocoValue::DateTime(dt) => {
                Value::Date(
                    dt.year() as u16,
                    dt.month() as u8,
                    dt.day() as u8,
                    dt.hour() as u8,
                    dt.minute() as u8,
                    dt.second() as u8,
                    dt.timestamp_subsec_micros(),
                )
            }
            DinocoValue::Date(date) => Value::Date(
                date.year() as u16,
                date.month() as u8,
                date.day() as u8,
                0,
                0,
                0,
                0,
            ),
        }
    }
}

impl From<mysql_async::Error> for DinocoError {
    fn from(e: mysql_async::Error) -> Self {
        Self::MySql(e)
    }
}
