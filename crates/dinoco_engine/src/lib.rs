mod backends;
mod query;
mod traits;
mod value;

use std::sync::atomic::{AtomicUsize, Ordering};

pub use backends::*;
pub use query::*;
pub use traits::*;
pub use value::*;

pub use rusqlite::Row as SqliteRow;
pub use tokio_postgres::Row as PostgresRow;

pub type DeadpoolPostgresRow = tokio_postgres::Row;
pub type MysqlRow = mysql_async::Row;

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

    pub fn read_backend(&self, primary: bool) -> &Backend {
        if primary || self.read_replicas.is_empty() {
            return &self.backend;
        }

        let index = self.read_replica_index.fetch_add(1, Ordering::Relaxed) % self.read_replicas.len();

        &self.read_replicas[index]
    }
}
