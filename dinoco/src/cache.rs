use dinoco_engine::{DinocoAdapter, DinocoClient, DinocoError, DinocoResult};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{FindFirst, FindMany, Model, Projection, execute_many};

#[derive(Debug, Clone)]
pub struct CachePolicy {
    pub key: String,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct CachedFindFirst<M, S = M> {
    pub inner: FindFirst<M, S>,
    pub policy: CachePolicy,
}

#[derive(Debug, Clone)]
pub struct CachedFindMany<M, S = M> {
    pub inner: FindMany<M, S>,
    pub policy: CachePolicy,
}

pub struct DinocoCache<'a, A: DinocoAdapter> {
    client: &'a DinocoClient<A>,
}

pub trait DinocoClientCacheExt<A>
where
    A: DinocoAdapter,
{
    fn cache(&self) -> DinocoCache<'_, A>;
}

fn missing_cache_error() -> DinocoError {
    DinocoError::ConnectionError("Redis cache is not configured for this DinocoClient.".to_string())
}

impl CachePolicy {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into(), ttl_seconds: None }
    }

    pub fn with_ttl(key: impl Into<String>, ttl_seconds: u64) -> Self {
        Self { key: key.into(), ttl_seconds: Some(ttl_seconds) }
    }
}

impl<'a, A> DinocoCache<'a, A>
where
    A: DinocoAdapter,
{
    pub fn new(client: &'a DinocoClient<A>) -> Self {
        Self { client }
    }

    pub async fn delete(&self, key: &str) -> DinocoResult<()> {
        let cache = self.client.cache_store().ok_or_else(missing_cache_error)?;

        cache.delete(key).await
    }

    pub async fn get<T>(&self, key: &str) -> DinocoResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        let cache = self.client.cache_store().ok_or_else(missing_cache_error)?;

        cache.get(key).await
    }

    pub async fn set<T>(&self, key: &str, value: &T) -> DinocoResult<()>
    where
        T: Serialize,
    {
        let cache = self.client.cache_store().ok_or_else(missing_cache_error)?;

        cache.set(key, value).await
    }

    pub async fn set_with_ttl<T>(&self, key: &str, value: &T, ttl_seconds: u64) -> DinocoResult<()>
    where
        T: Serialize,
    {
        let cache = self.client.cache_store().ok_or_else(missing_cache_error)?;

        cache.set_with_ttl(key, value, ttl_seconds).await
    }
}

impl<A> DinocoClientCacheExt<A> for DinocoClient<A>
where
    A: DinocoAdapter,
{
    fn cache(&self) -> DinocoCache<'_, A> {
        DinocoCache::new(self)
    }
}

impl<M, S> CachedFindFirst<M, S>
where
    M: Model,
    S: Projection<M> + Serialize + DeserializeOwned,
{
    pub fn new(inner: FindFirst<M, S>, policy: CachePolicy) -> Self {
        Self { inner, policy }
    }

    pub fn policy(&self) -> &CachePolicy {
        &self.policy
    }

    pub fn read_in_primary(self) -> Self {
        Self { inner: self.inner.read_in_primary(), policy: self.policy }
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<Option<S>>> + 'a
    where
        A: DinocoAdapter,
    {
        async move {
            let cache = DinocoCache::new(client);

            if let Some(cached) = cache.get::<Option<S>>(&self.policy.key).await? {
                client.log_cache_hit(&self.policy.key);
                return Ok(cached);
            }

            let mut rows = execute_many::<M, S, A>(
                self.inner.inner.statement.limit(1),
                &self.inner.inner.includes,
                &self.inner.inner.counts,
                self.inner.inner.read_mode,
                client,
            )
            .await?;
            let result = rows.drain(..).next();

            if let Some(ttl_seconds) = self.policy.ttl_seconds {
                cache.set_with_ttl(&self.policy.key, &result, ttl_seconds).await?;
            } else {
                cache.set(&self.policy.key, &result).await?;
            }

            Ok(result)
        }
    }
}

impl<M, S> CachedFindMany<M, S>
where
    M: Model,
    S: Projection<M> + Serialize + DeserializeOwned,
{
    pub fn new(inner: FindMany<M, S>, policy: CachePolicy) -> Self {
        Self { inner, policy }
    }

    pub fn policy(&self) -> &CachePolicy {
        &self.policy
    }

    pub fn read_in_primary(self) -> Self {
        Self { inner: self.inner.read_in_primary(), policy: self.policy }
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<Vec<S>>> + 'a
    where
        A: DinocoAdapter,
    {
        async move {
            let cache = DinocoCache::new(client);

            if let Some(cached) = cache.get::<Vec<S>>(&self.policy.key).await? {
                client.log_cache_hit(&self.policy.key);
                return Ok(cached);
            }

            let result = execute_many::<M, S, A>(
                self.inner.statement,
                &self.inner.includes,
                &self.inner.counts,
                self.inner.read_mode,
                client,
            )
            .await?;

            if let Some(ttl_seconds) = self.policy.ttl_seconds {
                cache.set_with_ttl(&self.policy.key, &result, ttl_seconds).await?;
            } else {
                cache.set(&self.policy.key, &result).await?;
            }

            Ok(result)
        }
    }
}
