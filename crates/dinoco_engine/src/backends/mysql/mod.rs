use std::sync::Arc;

use anyhow::{Context, anyhow};
use mysql_async::prelude::Queryable;
use mysql_async::{Pool, Value};

mod compiler;

use crate::{DinocoAdapter, DinocoRowModel, DinocoValue};

pub struct MySqlAdapter {
    pub url: String,
    pub pool: Arc<Pool>,
}

#[async_trait::async_trait(?Send)]
impl DinocoAdapter for MySqlAdapter {
    async fn new(url: String) -> Result<Self, String> {
        Ok(Self::new(url))
    }

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        let rows: Vec<mysql_async::Row> = conn.exec(query, mysql_params(params)).await?;

        rows.into_iter()
            .map(|row| M::from_mysql_row(&row).ok_or_else(|| anyhow!("Failed to parse mysql row")))
            .collect()
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        let rows: Vec<mysql_async::Row> = conn.exec(query, mysql_params(params)).await?;

        Ok(rows.into_iter().filter_map(|row| M::from_mysql_row(&row)).collect())
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        conn.exec_drop(query, mysql_params(params)).await?;

        Ok(conn.affected_rows() as usize)
    }
}

impl MySqlAdapter {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        let pool = Pool::new(url.as_str());

        Self { url, pool: Arc::new(pool) }
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        let count = conn.exec_first::<i64, _, _>(query, mysql_params(params)).await?.unwrap_or_default();

        Ok(count)
    }
}

fn mysql_params(params: &[DinocoValue]) -> Vec<Value> {
    params
        .iter()
        .map(|param| match param {
            DinocoValue::Null => Value::NULL,
            DinocoValue::Integer(value) => Value::Int(*value),
            DinocoValue::Float(value) => Value::Double(*value),
            DinocoValue::String(value) => Value::Bytes(value.clone().into_bytes()),
            DinocoValue::Enum(_, value) => Value::Bytes(value.clone().into_bytes()),
            DinocoValue::Boolean(value) => Value::Int(if *value { 1 } else { 0 }),
            DinocoValue::Bytes(value) => Value::Bytes(value.clone()),
        })
        .collect()
}

impl mysql_common::prelude::FromValue for DinocoValue {
    type Intermediate = DinocoValue;
}

impl TryFrom<Value> for DinocoValue {
    type Error = mysql_common::value::convert::FromValueError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::NULL => Ok(DinocoValue::Null),
            Value::Bytes(value) => {
                String::from_utf8(value.clone()).map(DinocoValue::String).or_else(|_| Ok(DinocoValue::Bytes(value)))
            }
            Value::Int(value) => Ok(DinocoValue::Integer(value)),
            Value::UInt(value) => Ok(DinocoValue::Integer(value as i64)),
            Value::Float(value) => Ok(DinocoValue::Float(value as f64)),
            Value::Double(value) => Ok(DinocoValue::Float(value)),
            value => Err(mysql_common::value::convert::FromValueError(value)),
        }
    }
}

impl From<DinocoValue> for Value {
    fn from(value: DinocoValue) -> Self {
        match value {
            DinocoValue::Null => Value::NULL,
            DinocoValue::Integer(value) => Value::Int(value),
            DinocoValue::Float(value) => Value::Double(value),
            DinocoValue::String(value) | DinocoValue::Enum(_, value) => Value::Bytes(value.into_bytes()),
            DinocoValue::Boolean(value) => Value::Int(if value { 1 } else { 0 }),
            DinocoValue::Bytes(value) => Value::Bytes(value),
        }
    }
}
