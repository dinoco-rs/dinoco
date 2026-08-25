mod backends;
mod error;
mod query;
pub mod runtime;
mod traits;
mod transaction;
mod value;

use std::sync::atomic::{AtomicUsize, Ordering};

pub use backends::*;
pub use error::*;
pub use query::*;
pub use traits::*;
pub use transaction::*;
pub use value::*;

pub use rusqlite::Row as SqliteRow;
pub use tokio_postgres::Row as PostgresRow;

pub type DeadpoolPostgresRow = tokio_postgres::Row;
pub type MysqlRow = mysql_async::Row;

pub use chrono;
pub use deadpool_postgres;
pub use mysql_async;
pub use mysql_common;
pub use rusqlite;
pub use serde_json;
pub use tokio_postgres;

pub struct SingleIdRow {
    pub id: DinocoValue,
}

impl DinocoSqlite for SingleIdRow {
    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
        row.get::<_, String>("id")
            .map(|id| Self { id: DinocoValue::String(id) })
            .or_else(|_| row.get::<_, i64>("id").map(|id| Self { id: DinocoValue::Integer(id) }))
            .ok()
    }
}

impl DinocoPostgres for SingleIdRow {
    fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
        row.try_get::<_, String>("id")
            .map(|id| Self { id: DinocoValue::String(id) })
            .or_else(|_| row.try_get::<_, i64>("id").map(|id| Self { id: DinocoValue::Integer(id) }))
            .ok()
    }

    fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
        Self::from_deadpool_posgres_row(row)
    }
}

impl DinocoMysql for SingleIdRow {
    fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
        row.get::<String, _>("id")
            .map(|id| Self { id: DinocoValue::String(id) })
            .or_else(|| row.get::<i64, _>("id").map(|id| Self { id: DinocoValue::Integer(id) }))
    }
}

pub struct DinocoClient {
    pub backend: Backend,
    pub read_replicas: Vec<Backend>,
    read_replica_index: AtomicUsize,
}

impl DinocoClient {
    pub fn new(backend: Backend) -> Self {
        Self { backend, read_replicas: Vec::new(), read_replica_index: AtomicUsize::new(0) }
    }

    pub fn with_read_replicas(mut self, read_replicas: Vec<Backend>) -> Self {
        self.read_replicas = read_replicas;
        self
    }

    pub fn with_logger(mut self, enabled: bool) -> Self {
        self.backend.set_logger(enabled);
        for replica in &mut self.read_replicas {
            replica.set_logger(enabled);
        }
        self
    }

    pub fn read_backend(&self, primary: bool) -> &Backend {
        if primary || self.read_replicas.is_empty() {
            return &self.backend;
        }

        let index = self.read_replica_index.fetch_add(1, Ordering::Relaxed) % self.read_replicas.len();

        &self.read_replicas[index]
    }
}
