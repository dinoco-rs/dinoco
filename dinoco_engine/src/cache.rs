use redis::AsyncCommands;
use redis::ToRedisArgs;
use redis::aio::ConnectionManager;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{DinocoError, DinocoResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DinocoRedisConfig {
    Url { url: String },
    Parameters { host: String, password: Option<String>, username: Option<String> },
}

#[derive(Clone)]
pub struct DinocoCacheClient {
    connection: ConnectionManager,
}

fn build_redis_connection_url(host: &str, username: &Option<String>, password: &Option<String>) -> String {
    let output = if host.starts_with("redis://") || host.starts_with("rediss://") {
        host.to_string()
    } else {
        format!("redis://{host}")
    };

    if username.is_none() && password.is_none() {
        return output;
    }

    let (scheme, address) =
        output.split_once("://").map(|(scheme, address)| (scheme, address)).unwrap_or(("redis", output.as_str()));

    let credentials = match (username.as_deref(), password.as_deref()) {
        (Some(username), Some(password)) => format!("{username}:{password}@"),
        (Some(username), None) => format!("{username}@"),
        (None, Some(password)) => format!(":{password}@"),
        (None, None) => String::new(),
    };

    format!("{scheme}://{credentials}{address}")
}

impl DinocoRedisConfig {
    pub fn from_host(host: impl Into<String>) -> Self {
        Self::Parameters { host: host.into(), password: None, username: None }
    }

    pub fn from_url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }

    pub fn connection_url(&self) -> String {
        match self {
            Self::Url { url } => url.clone(),
            Self::Parameters { host, password, username } => build_redis_connection_url(host, username, password),
        }
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        if let Self::Parameters { password: current_password, .. } = &mut self {
            *current_password = Some(password.into());
        }

        self
    }

    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        if let Self::Parameters { username: current_username, .. } = &mut self {
            *current_username = Some(username.into());
        }

        self
    }
}

impl DinocoCacheClient {
    pub async fn connect(config: &DinocoRedisConfig) -> DinocoResult<Self> {
        let client = redis::Client::open(config.connection_url())?;
        let connection = ConnectionManager::new(client).await?;

        Ok(Self { connection })
    }

    pub async fn delete(&self, key: &str) -> DinocoResult<()> {
        let mut connection = self.connection.clone();

        connection.del::<_, ()>(key).await?;

        Ok(())
    }

    pub async fn get<T>(&self, key: &str) -> DinocoResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        let mut connection = self.connection.clone();
        let value: Option<String> = connection.get(key).await?;

        value
            .map(|value| serde_json::from_str::<T>(&value).map_err(|error| DinocoError::ParseError(error.to_string())))
            .transpose()
    }

    pub async fn set<T>(&self, key: &str, value: &T) -> DinocoResult<()>
    where
        T: Serialize,
    {
        let mut connection = self.connection.clone();
        let value = serde_json::to_string(value).map_err(|error| DinocoError::ParseError(error.to_string()))?;

        connection.set::<_, _, ()>(key, value).await?;

        Ok(())
    }

    pub async fn set_with_ttl<T>(&self, key: &str, value: &T, ttl_seconds: u64) -> DinocoResult<()>
    where
        T: Serialize,
    {
        let mut connection = self.connection.clone();
        let value = serde_json::to_string(value).map_err(|error| DinocoError::ParseError(error.to_string()))?;

        connection.set_ex::<_, _, ()>(key, value, ttl_seconds).await?;

        Ok(())
    }

    pub async fn hash_delete(&self, key: &str, field: &str) -> DinocoResult<()> {
        let mut connection = self.connection.clone();

        connection.hdel::<_, _, ()>(key, field).await?;

        Ok(())
    }

    pub async fn hash_get<T>(&self, key: &str, field: &str) -> DinocoResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        let mut connection = self.connection.clone();
        let value: Option<String> = connection.hget(key, field).await?;

        value
            .map(|value| serde_json::from_str::<T>(&value).map_err(|error| DinocoError::ParseError(error.to_string())))
            .transpose()
    }

    pub async fn hash_set<T>(&self, key: &str, field: &str, value: &T) -> DinocoResult<()>
    where
        T: Serialize,
    {
        let mut connection = self.connection.clone();
        let value = serde_json::to_string(value).map_err(|error| DinocoError::ParseError(error.to_string()))?;

        connection.hset::<_, _, _, ()>(key, field, value).await?;

        Ok(())
    }

    pub async fn sorted_set_add<V>(&self, key: &str, value: V, score: i64) -> DinocoResult<()>
    where
        V: ToRedisArgs + Send + Sync,
    {
        let mut connection = self.connection.clone();

        connection.zadd::<_, _, _, ()>(key, value, score).await?;

        Ok(())
    }

    pub async fn sorted_set_range_by_score(
        &self,
        key: &str,
        max_score: i64,
        limit: isize,
    ) -> DinocoResult<Vec<String>> {
        let mut connection = self.connection.clone();

        redis::cmd("ZRANGEBYSCORE")
            .arg(key)
            .arg("-inf")
            .arg(max_score)
            .arg("LIMIT")
            .arg(0)
            .arg(limit)
            .query_async::<Vec<String>>(&mut connection)
            .await
            .map_err(DinocoError::from)
    }

    pub async fn sorted_set_remove<V>(&self, key: &str, value: V) -> DinocoResult<usize>
    where
        V: ToRedisArgs + Send + Sync,
    {
        let mut connection = self.connection.clone();

        connection.zrem(key, value).await.map_err(DinocoError::from)
    }

    pub async fn sorted_set_pop_min_by_score(&self, key: &str, max_score: i64) -> DinocoResult<Option<String>> {
        let mut connection = self.connection.clone();

        redis::Script::new(
            r#"
            local items = redis.call("ZRANGEBYSCORE", KEYS[1], "-inf", ARGV[1], "LIMIT", 0, 1)

            if #items == 0 then
                return nil
            end

            if redis.call("ZREM", KEYS[1], items[1]) == 1 then
                return items[1]
            end

            return nil
            "#,
        )
        .key(key)
        .arg(max_score)
        .invoke_async(&mut connection)
        .await
        .map_err(DinocoError::from)
    }
}
