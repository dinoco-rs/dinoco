use std::sync::Arc;

use anyhow::{Context, anyhow};
use deadpool_sqlite::{Config, Pool, Runtime};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef};

mod compiler;

use crate::{DinocoAdapter, DinocoSqlite, DinocoValue};

pub struct SqliteAdapter {
    pub path: String,
    pub pool: Arc<Pool>,
}

#[async_trait::async_trait(?Send)]
impl DinocoAdapter for SqliteAdapter {
    async fn new(path: String) -> Result<Self, String> {
        let path = normalize_sqlite_path(path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let cfg = Config::new(&path);
        let Ok(pool) = cfg.create_pool(Runtime::Tokio1) else { return Err("Pool error".to_string()) };

        Ok(Self { path, pool: Arc::new(pool) })
    }

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        let conn = self.pool.get().await.context("Failed to get sqlite connection from pool")?;
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        conn.interact(move |conn| -> anyhow::Result<Vec<M>> {
            let mut stmt = conn.prepare_cached(&query_owned)?;
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            let mut rows = stmt.query(params_refs.as_slice())?;
            let mut result = Vec::new();

            while let Some(row) = rows.next()? {
                let item = M::from_sqlite_row(row).ok_or_else(|| anyhow!("Failed to parse sqlite row"))?;
                result.push(item);
            }

            Ok(result)
        })
        .await
        .map_err(|err| anyhow!(err.to_string()))?
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        let conn = self.pool.get().await.context("Failed to get sqlite connection from pool")?;
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        conn.interact(move |conn| -> anyhow::Result<Vec<M>> {
            let mut stmt = conn.prepare_cached(&query_owned)?;
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            let mut rows = stmt.query(params_refs.as_slice())?;
            let mut result = Vec::new();

            while let Some(row) = rows.next()? {
                if let Some(item) = M::from_sqlite_row(row) {
                    result.push(item);
                }
            }

            Ok(result)
        })
        .await
        .map_err(|err| anyhow!(err.to_string()))?
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        let conn = self.pool.get().await.context("Failed to get sqlite connection from pool")?;
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        conn.interact(move |conn| -> anyhow::Result<usize> {
            let mut stmt = conn.prepare_cached(&query_owned)?;
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            Ok(stmt.execute(params_refs.as_slice())?)
        })
        .await
        .map_err(|err| anyhow!(err.to_string()))?
    }
}

fn normalize_sqlite_path(path: String) -> String {
    if path == ":memory:" || path.starts_with('/') || path.starts_with("file:") || path.starts_with("dinoco/") {
        path
    } else {
        format!("dinoco/{path}")
    }
}

impl SqliteAdapter {
    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        let conn = self.pool.get().await.context("Failed to get sqlite connection from pool")?;
        let query_owned = query.to_string();
        let params_owned = params.to_vec();

        conn.interact(move |conn| -> anyhow::Result<i64> {
            let mut stmt = conn.prepare_cached(&query_owned)?;
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params_owned.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

            Ok(stmt.query_row(params_refs.as_slice(), |row| row.get(0))?)
        })
        .await
        .map_err(|err| anyhow!(err.to_string()))?
    }
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
            DinocoValue::Bytes(v) => Ok(ToSqlOutput::Owned(Value::Blob(v.clone()))),
            DinocoValue::Json(v) => Ok(ToSqlOutput::Owned(Value::Blob(v.to_string().into_bytes()))),
            DinocoValue::DateTime(dt) => Ok(ToSqlOutput::Owned(Value::Text(dt.to_rfc3339()))),
            DinocoValue::Date(date) => Ok(ToSqlOutput::Owned(Value::Text(date.to_string()))),
        }
    }
}

impl FromSql for DinocoValue {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Null => Ok(DinocoValue::Null),
            ValueRef::Integer(value) => Ok(DinocoValue::Integer(value)),
            ValueRef::Real(value) => Ok(DinocoValue::Float(value)),
            ValueRef::Text(value) => String::from_utf8(value.to_vec())
                .map(DinocoValue::String)
                .map_err(|err| FromSqlError::Other(Box::new(err))),
            ValueRef::Blob(value) => Ok(DinocoValue::Bytes(value.to_vec())),
        }
    }
}
