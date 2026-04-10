use redis::AsyncCommands;
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
}
