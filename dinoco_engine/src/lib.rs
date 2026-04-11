extern crate self as dinoco_engine;

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
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
    pub config: DinocoClientConfig,
    pub primary_url: String,
    pub query_logger: DinocoQueryLogger,
    pub read_replica_urls: Vec<String>,
    pub read_replicas: Vec<T>,
    read_replica_state: AtomicUsize,
}

#[derive(Clone)]
struct RegisteredWorkerClient {
    adapter_name: &'static str,
    config: DinocoClientConfig,
    primary_url: String,
    read_replica_urls: Vec<String>,
}

impl<T> Clone for DinocoClient<T>
where
    T: DinocoAdapter + Clone,
{
    fn clone(&self) -> Self {
        Self {
            adapter: self.adapter.clone(),
            adapter_name: self.adapter_name,
            cache: self.cache.clone(),
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
        register_worker_client::<PostgresAdapter>("postgresql", url.clone(), reads.clone(), config.clone());
        Self::build(url, reads, config, "postgresql").await
    }
}

impl DinocoClient<MySqlAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        register_worker_client::<MySqlAdapter>("mysql", url.clone(), reads.clone(), config.clone());
        Self::build(url, reads, config, "mysql").await
    }
}

impl DinocoClient<SqliteAdapter> {
    pub async fn new(url: String, reads: Vec<String>, config: DinocoClientConfig) -> DinocoResult<Self> {
        register_worker_client::<SqliteAdapter>("sqlite", url.clone(), reads.clone(), config.clone());
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
        let cache = match &config.redis {
            Some(redis) => Some(DinocoCacheClient::connect(redis).await?),
            None => None,
        };
        let adapter = T::connect(url.clone(), config.clone()).await?;
        let mut read_replicas: Vec<T> = Vec::with_capacity(reads.len());

        for read in &reads {
            let adapter = T::connect(read.clone(), config.clone()).await?;
            read_replicas.push(adapter);
        }

        Ok(Self {
            adapter,
            adapter_name,
            cache,
            config,
            primary_url: url,
            query_logger,
            read_replica_urls: reads,
            read_replicas,
            read_replica_state: AtomicUsize::new(0),
        })
    }

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

    pub async fn worker_client(&self) -> DinocoResult<Self> {
        Self::build(
            self.primary_url.clone(),
            self.read_replica_urls.clone(),
            self.config.clone(),
            self.adapter_name,
        )
        .await
    }

}

impl<T> DinocoClient<T>
where
    T: DinocoAdapter + 'static,
{
    pub async fn registered_worker_client() -> DinocoResult<Self> {
        let registry = worker_client_registry().lock().expect("worker client registry mutex poisoned");
        let Some(settings) = registry.get(&TypeId::of::<T>()).cloned() else {
            return Err(DinocoError::ConnectionError(
                "Dinoco worker client is not configured for this adapter yet.".to_string(),
            ));
        };
        drop(registry);

        Self::build(settings.primary_url, settings.read_replica_urls, settings.config, settings.adapter_name).await
    }
}

fn register_worker_client<T>(
    adapter_name: &'static str,
    primary_url: String,
    read_replica_urls: Vec<String>,
    config: DinocoClientConfig,
)
where
    T: DinocoAdapter + 'static,
{
    let mut registry = worker_client_registry().lock().expect("worker client registry mutex poisoned");
    registry.insert(TypeId::of::<T>(), RegisteredWorkerClient { adapter_name, config, primary_url, read_replica_urls });
}

fn worker_client_registry() -> &'static Mutex<HashMap<TypeId, RegisteredWorkerClient>> {
    static WORKER_CLIENT_REGISTRY: OnceLock<Mutex<HashMap<TypeId, RegisteredWorkerClient>>> = OnceLock::new();

    WORKER_CLIENT_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}
