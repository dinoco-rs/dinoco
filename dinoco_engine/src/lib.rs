extern crate self as dinoco_engine;

use std::sync::atomic::{AtomicUsize, Ordering};

mod data;
mod databases;
mod error;
mod helpers;
mod planner;
mod query;
mod traits;
mod value;

pub use data::*;
pub use databases::*;
pub use error::*;
pub use helpers::*;
pub use planner::*;
pub use query::*;
pub use traits::*;
pub use value::*;

pub type DinocoResult<T> = Result<T, DinocoError>;

pub struct DinocoClient<T: DinocoAdapter> {
    pub adapter: T,
    pub read_replicas: Vec<T>,
    read_replica_state: AtomicUsize,
}

impl DinocoClient<PostgresAdapter> {
    pub async fn new(url: String, reads: Vec<String>) -> DinocoResult<Self> {
        let adapter = PostgresAdapter::connect(url).await?;

        let mut read_replicas: Vec<PostgresAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = PostgresAdapter::connect(read).await?;
            read_replicas.push(adapter);
        }

        Ok(Self { adapter, read_replicas, read_replica_state: AtomicUsize::new(0) })
    }
}

impl DinocoClient<MySqlAdapter> {
    pub async fn new(url: String, reads: Vec<String>) -> DinocoResult<Self> {
        let adapter = MySqlAdapter::connect(url).await?;

        let mut read_replicas: Vec<MySqlAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = MySqlAdapter::connect(read).await?;
            read_replicas.push(adapter);
        }

        Ok(Self { adapter, read_replicas, read_replica_state: AtomicUsize::new(0) })
    }
}

impl DinocoClient<SqliteAdapter> {
    pub async fn new(url: String, reads: Vec<String>) -> DinocoResult<Self> {
        let adapter = SqliteAdapter::connect(url).await?;

        let mut read_replicas: Vec<SqliteAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = SqliteAdapter::connect(read).await?;
            read_replicas.push(adapter);
        }

        Ok(Self { adapter, read_replicas, read_replica_state: AtomicUsize::new(0) })
    }
}

impl<T: DinocoAdapter> DinocoClient<T> {
    pub fn primary(&self) -> &T {
        &self.adapter
    }

    pub fn reader(&self) -> &T {
        match self.read_replicas.len() {
            0 => &self.adapter,
            1 => &self.read_replicas[0],
            len => &self.read_replicas[self.read_replica_state.fetch_add(1, Ordering::Relaxed) % len],
        }
    }

    pub fn read_adapter(&self, read_in_primary: bool) -> &T {
        if read_in_primary { self.primary() } else { self.reader() }
    }
}
