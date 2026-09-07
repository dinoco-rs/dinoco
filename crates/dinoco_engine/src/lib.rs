mod backends;
mod error;
mod json_row;
mod pluck;
mod query;
mod traits;
mod transaction;
mod value;

use std::sync::atomic::{AtomicUsize, Ordering};

pub use backends::*;
pub use error::*;
pub use json_row::*;
pub use pluck::*;
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

/// Execution strategy for `find_batch(...)`.
///
/// Configurable per [`DinocoClient`] via `.with_query_mode(...)`, or from
/// `schema.dinoco` via `config { query_mode = "single_query" | "batch_query"
/// }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryMode {
    /// Runs every `find_many`/`find_first` passed to `find_batch(...)` as its
    /// own query, one per item. This is the default: it matches dinoco's
    /// pre-existing behavior and has no extra requirements on the database.
    #[default]
    BatchQuery,
    /// Combines every item into a single round trip: each becomes a
    /// JSON-aggregated subquery (`json_build_object`/`json_agg` on Postgres,
    /// `JSON_OBJECT`/`JSON_ARRAYAGG` on MySQL, `json_object`/
    /// `json_group_array` on SQLite) selected together in one statement.
    SingleQuery,
}

pub struct DinocoClient {
    pub backend: Backend,
    pub read_replicas: Vec<Backend>,
    read_replica_index: AtomicUsize,
    query_mode: QueryMode,
}

impl DinocoClient {
    pub fn new(backend: Backend) -> Self {
        Self { backend, read_replicas: Vec::new(), read_replica_index: AtomicUsize::new(0), query_mode: QueryMode::default() }
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

    pub fn with_query_mode(mut self, mode: QueryMode) -> Self {
        self.query_mode = mode;
        self
    }

    pub fn query_mode(&self) -> QueryMode {
        self.query_mode
    }

    pub fn read_backend(&self, primary: bool) -> &Backend {
        if primary || self.read_replicas.is_empty() {
            return &self.backend;
        }

        let index = self.read_replica_index.fetch_add(1, Ordering::Relaxed) % self.read_replicas.len();

        &self.read_replicas[index]
    }
}
