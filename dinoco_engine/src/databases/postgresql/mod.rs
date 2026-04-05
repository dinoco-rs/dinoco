use async_trait::async_trait;

use std::str::FromStr;
use std::sync::Arc;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};

use tokio_postgres::NoTls;
use tokio_postgres::types::{IsNull, Json, ToSql, Type, private::BytesMut, to_sql_checked};

use crate::{DinocoAdapter, DinocoError, DinocoResult, DinocoRow, DinocoValue};

mod dialect;
mod handler;
mod migration;
mod row;

pub use dialect::PostgresDialect;

static POSTGRES_DIALECT: PostgresDialect = PostgresDialect;

pub struct PostgresAdapter {
    pub url: String,
    pub client: Arc<Pool>,
}

#[async_trait]
impl DinocoAdapter for PostgresAdapter {
    type Dialect = PostgresDialect;

    fn dialect(&self) -> &Self::Dialect {
        &POSTGRES_DIALECT
    }

    async fn connect(url: String) -> DinocoResult<Self> {
        let pg_config = tokio_postgres::Config::from_str(&url).map_err(|e| DinocoError::from(e))?;

        let mgr = Manager::from_config(pg_config, NoTls, ManagerConfig { recycling_method: RecyclingMethod::Fast });

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

impl ToSql for DinocoValue {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            DinocoValue::Null => Ok(IsNull::Yes),
            DinocoValue::Integer(i) => i.to_sql(ty, out),
            DinocoValue::Float(f) => f.to_sql(ty, out),
            DinocoValue::Boolean(b) => b.to_sql(ty, out),
            DinocoValue::String(s) => s.to_sql(ty, out),
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
