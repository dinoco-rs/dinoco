extern crate self as dinoco_engine;

use std::sync::atomic::{AtomicUsize, Ordering};

mod config;
mod data;
mod databases;
mod error;
mod helpers;
mod planner;
mod query;
mod traits;
mod value;

pub use config::*;
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
    pub adapter_name: &'static str,
    pub config: DinocoClientConfig,
    pub primary_url: String,
    pub query_logger: DinocoQueryLogger,
    pub read_replica_urls: Vec<String>,
    pub read_replicas: Vec<T>,
    read_replica_state: AtomicUsize,
}

impl<T> Clone for DinocoClient<T>
where
    T: DinocoAdapter + Clone,
{
    fn clone(&self) -> Self {
        Self {
            adapter: self.adapter.clone(),
            adapter_name: self.adapter_name,
            config: self.config.clone(),
            primary_url: self.primary_url.clone(),
            query_logger: self.query_logger.clone(),
            read_replica_urls: self.read_replica_urls.clone(),
            read_replicas: self.read_replicas.clone(),
            read_replica_state: AtomicUsize::new(self.read_replica_state.load(Ordering::Relaxed)),
        }
    }
}

impl DinocoClient<PostgresAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        Self::build(url, reads, config, "postgresql").await
    }
}

impl DinocoClient<MySqlAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        Self::build(url, reads, config, "mysql").await
    }
}

impl DinocoClient<SqliteAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        Self::build(url, reads, config, "sqlite").await
    }
}

impl<T> DinocoClient<T>
where
    T: DinocoAdapter,
{
    async fn build(
        url: String,
        reads: Vec<String>,
        config: DinocoClientConfig,
        adapter_name: &'static str,
    ) -> DinocoResult<Self> {
        let query_logger = config.query_logger.clone();
        config.initialize_runtime();
        let adapter = T::connect(url.clone(), config.clone()).await?;
        let mut read_replicas: Vec<T> = Vec::with_capacity(reads.len());

        for read in &reads {
            let adapter = T::connect(read.clone(), config.clone()).await?;
            read_replicas.push(adapter);
        }

        Ok(Self {
            adapter,
            adapter_name,
            config,
            primary_url: url,
            query_logger,
            read_replica_urls: reads,
            read_replicas,
            read_replica_state: AtomicUsize::new(0),
        })
    }

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
