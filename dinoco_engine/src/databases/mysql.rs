use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::StreamExt;

use mysql_async::{Params::Positional, Pool, Row, Value, prelude::Queryable};

use crate::{DinocoAdapter, DinocoAdapterStream, DinocoDatabaseRow, DinocoError, DinocoResult, DinocoRow, DinocoStream, DinocoType, DinocoValue};

pub struct MySqlAdapter {
    pub url: String,
    pub client: Arc<Pool>,
}

#[async_trait]
impl DinocoAdapter for MySqlAdapter {
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
}

#[async_trait]
impl DinocoAdapterStream for MySqlAdapter {
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
        }
    }
}

impl DinocoDatabaseRow for Row {
    fn get_i64(&self, idx: usize) -> DinocoResult<i64> {
        self.get::<i64, _>(idx)
            .ok_or_else(|| DinocoError::ParseError(format!("Failed to get i64 at column {}", idx)))
    }

    fn get_string(&self, idx: usize) -> DinocoResult<String> {
        self.get::<String, _>(idx)
            .ok_or_else(|| DinocoError::ParseError(format!("Failed to get String at column {}", idx)))
    }

    fn get_bool(&self, idx: usize) -> DinocoResult<bool> {
        if let Some(v) = self.get::<bool, _>(idx) {
            return Ok(v);
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
