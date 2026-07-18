use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use deadpool_postgres::{ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::types::ToSql;
use tokio_postgres::{Config, NoTls};

mod compiler;

use crate::{DinocoAdapter, DinocoRowModel, DinocoValue};

#[derive(Clone, Copy, Debug)]
pub enum PostgresMode {
    Direct,
    PgBouncer,
}

pub struct PostgresAdapter {
    pub url: String,
    pub pool: Arc<Pool>,
    pub mode: PostgresMode,
}

pub struct PgBouncerAdapter {
    inner: PostgresAdapter,
}

#[async_trait::async_trait(?Send)]
impl DinocoAdapter for PostgresAdapter {
    async fn new(url: String) -> Result<Self, String> {
        Self::direct(url).await.map_err(|err| err.to_string())
    }

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let rows = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.query(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.query(query, &params).await?,
        };

        rows.into_iter()
            .map(|row| M::from_deadpool_posgres_row(&row).ok_or_else(|| anyhow!("Failed to parse postgres row")))
            .collect()
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let rows = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.query(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.query(query, &params).await?,
        };

        Ok(rows.into_iter().filter_map(|row| M::from_deadpool_posgres_row(&row)).collect())
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let affected = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.execute(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.execute(query, &params).await?,
        };

        Ok(affected as usize)
    }
}

impl PostgresAdapter {
    pub async fn direct(url: impl Into<String>) -> anyhow::Result<Self> {
        Self::from_url(url.into(), PostgresMode::Direct).await
    }

    pub async fn pgbouncer(url: impl Into<String>) -> anyhow::Result<Self> {
        Self::from_url(url.into(), PostgresMode::PgBouncer).await
    }

    async fn from_url(url: String, mode: PostgresMode) -> anyhow::Result<Self> {
        let pg_config = Config::from_str(&url).context("Invalid postgres url")?;
        let manager_config = ManagerConfig {
            recycling_method: match mode {
                PostgresMode::Direct => RecyclingMethod::Fast,
                PostgresMode::PgBouncer => RecyclingMethod::Fast,
            },
        };
        let manager = deadpool_postgres::Manager::from_config(pg_config, NoTls, manager_config);
        let pool = Pool::builder(manager).runtime(Runtime::Tokio1).build()?;

        Ok(Self { url, pool: Arc::new(pool), mode })
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let row = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.query_one(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.query_one(query, &params).await?,
        };

        Ok(row.try_get(0)?)
    }
}

#[async_trait::async_trait(?Send)]
impl DinocoAdapter for PgBouncerAdapter {
    async fn new(url: String) -> Result<Self, String> {
        Self::new(url).await.map_err(|err| err.to_string())
    }

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.inner.query(query, params).await
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.inner.query_optional(query, params).await
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        self.inner.execute(query, params).await
    }
}

impl PgBouncerAdapter {
    pub async fn new(url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self { inner: PostgresAdapter::pgbouncer(url).await? })
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        self.inner.query_count(query, params).await
    }
}

fn postgres_params(params: &[DinocoValue]) -> Vec<Box<dyn ToSql + Sync>> {
    params
        .iter()
        .map(|param| match param {
            DinocoValue::Null => Box::new(None::<String>) as Box<dyn ToSql + Sync>,
            DinocoValue::Integer(value) => Box::new(*value) as Box<dyn ToSql + Sync>,
            DinocoValue::Float(value) => Box::new(*value) as Box<dyn ToSql + Sync>,
            DinocoValue::String(value) => Box::new(value.clone()) as Box<dyn ToSql + Sync>,
            DinocoValue::Enum(_, value) => Box::new(value.clone()) as Box<dyn ToSql + Sync>,
            DinocoValue::Boolean(value) => Box::new(*value) as Box<dyn ToSql + Sync>,
            DinocoValue::Bytes(value) => Box::new(value.clone()) as Box<dyn ToSql + Sync>,
        })
        .collect()
}

fn postgres_param_refs(params: &[Box<dyn ToSql + Sync>]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|param| param.as_ref()).collect()
}

impl<'a> tokio_postgres::types::FromSql<'a> for DinocoValue {
    fn from_sql(
        ty: &tokio_postgres::types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if *ty == tokio_postgres::types::Type::BOOL {
            return Ok(DinocoValue::Boolean(<bool as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::FLOAT4 {
            return Ok(DinocoValue::Float(<f32 as tokio_postgres::types::FromSql>::from_sql(ty, raw)? as f64));
        }

        if *ty == tokio_postgres::types::Type::FLOAT8 {
            return Ok(DinocoValue::Float(<f64 as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::INT2 {
            return Ok(DinocoValue::Integer(<i16 as tokio_postgres::types::FromSql>::from_sql(ty, raw)? as i64));
        }

        if *ty == tokio_postgres::types::Type::INT4 {
            return Ok(DinocoValue::Integer(<i32 as tokio_postgres::types::FromSql>::from_sql(ty, raw)? as i64));
        }

        if *ty == tokio_postgres::types::Type::INT8 {
            return Ok(DinocoValue::Integer(<i64 as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::BYTEA {
            return Ok(DinocoValue::Bytes(<Vec<u8> as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        Ok(DinocoValue::String(<String as tokio_postgres::types::FromSql>::from_sql(ty, raw)?))
    }

    fn accepts(_ty: &tokio_postgres::types::Type) -> bool {
        true
    }
}
