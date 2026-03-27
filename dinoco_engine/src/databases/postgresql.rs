use async_trait::async_trait;

use std::str::FromStr;
use std::sync::Arc;

use futures::stream::{self, TryStreamExt};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};

use tokio_postgres::types::{IsNull, ToSql, Type, private::BytesMut, to_sql_checked};
use tokio_postgres::{NoTls, Row};

use crate::{DinocoAdapter, DinocoAdapterStream, DinocoDatabaseRow, DinocoError, DinocoResult, DinocoRow, DinocoStream, DinocoType, DinocoValue};

pub struct PostgresAdapter {
    pub url: String,
    pub client: Arc<Pool>,
}

#[async_trait]
impl DinocoAdapter for PostgresAdapter {
    async fn connect(url: String) -> DinocoResult<Self> {
        let pg_config = tokio_postgres::Config::from_str(&url).map_err(|e| DinocoError::from(e))?;

        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, NoTls, mgr_config);

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
}

#[async_trait]
impl DinocoAdapterStream for PostgresAdapter {
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
            DinocoValue::String(s) => s.to_sql(ty, out),
            DinocoValue::Boolean(b) => b.to_sql(ty, out),
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

    fn get<T: DinocoType>(&self, idx: usize) -> DinocoResult<T> {
        T::from_row(self, idx)
    }
}
