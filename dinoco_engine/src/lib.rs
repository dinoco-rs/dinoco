extern crate self as dinoco_engine;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod cache;
mod config;
mod data;
mod databases;
mod error;
mod helpers;
mod planner;
mod query;
mod traits;
mod value;

pub use cache::*;
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
    pub cache: Option<DinocoCacheClient>,
    pub query_logger: DinocoQueryLogger,
    pub read_replicas: Vec<T>,
    read_replica_state: AtomicUsize,
}

impl DinocoClient<PostgresAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        let query_logger = config.query_logger.clone();
        config.initialize_runtime();
        let cache = match &config.redis {
            Some(redis) => Some(DinocoCacheClient::connect(redis).await?),
            None => None,
        };
        let adapter = PostgresAdapter::connect(url, config.clone()).await?;

        let mut read_replicas: Vec<PostgresAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = PostgresAdapter::connect(read, config.clone()).await?;
            read_replicas.push(adapter);
        }

        Ok(Self {
            adapter,
            adapter_name: "postgresql",
            cache,
            query_logger,
            read_replicas,
            read_replica_state: AtomicUsize::new(0),
        })
    }
}

impl DinocoClient<MySqlAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        let query_logger = config.query_logger.clone();
        config.initialize_runtime();
        let cache = match &config.redis {
            Some(redis) => Some(DinocoCacheClient::connect(redis).await?),
            None => None,
        };
        let adapter = MySqlAdapter::connect(url, config.clone()).await?;

        let mut read_replicas: Vec<MySqlAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = MySqlAdapter::connect(read, config.clone()).await?;
            read_replicas.push(adapter);
        }

        Ok(Self {
            adapter,
            adapter_name: "mysql",
            cache,
            query_logger,
            read_replicas,
            read_replica_state: AtomicUsize::new(0),
        })
    }
}

impl DinocoClient<SqliteAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        let query_logger = config.query_logger.clone();
        config.initialize_runtime();
        let cache = match &config.redis {
            Some(redis) => Some(DinocoCacheClient::connect(redis).await?),
            None => None,
        };
        let adapter = SqliteAdapter::connect(url, config.clone()).await?;

        let mut read_replicas: Vec<SqliteAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = SqliteAdapter::connect(read, config.clone()).await?;
            read_replicas.push(adapter);
        }

        Ok(Self {
            adapter,
            adapter_name: "sqlite",
            cache,
            query_logger,
            read_replicas,
            read_replica_state: AtomicUsize::new(0),
        })
    }
}

impl<T: DinocoAdapter> DinocoClient<T> {
    pub fn cache_store(&self) -> Option<&DinocoCacheClient> {
        self.cache.as_ref()
    }

    pub fn has_cache(&self) -> bool {
        self.cache.is_some()
    }

    pub fn log_cache_hit(&self, key: &str) {
        self.query_logger.log(DinocoQueryLog {
            adapter: self.adapter_name,
            duration: Duration::default(),
            params: Vec::new(),
            query: format!("CACHE HIT key={key}"),
        });
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
