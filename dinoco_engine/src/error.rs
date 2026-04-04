use std::convert::Infallible;

#[derive(Debug)]
pub enum DinocoError {
    Postgres(tokio_postgres::Error),
    MySql(mysql_async::Error),
    Sqlite(rusqlite::Error),
    TaskJoin(tokio::task::JoinError),
    ParseError(String),
    ConnectionError(String),
    TypeMismatch,
    ColumnNotFound,
}

impl std::fmt::Display for DinocoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Postgres(e) => write!(f, "Postgres error: {}", e),
            Self::MySql(e) => write!(f, "MySQL error: {}", e),
            Self::Sqlite(e) => write!(f, "Sqlite error: {}", e),
            Self::TaskJoin(e) => write!(f, "Async task join error: {}", e),
            Self::ParseError(msg) => write!(f, "Data parse error: {}", msg),
            Self::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            Self::TypeMismatch => write!(f, "Row type mismatch.",),
            Self::ColumnNotFound => write!(f, "Column not found.",),
        }
    }
}

impl std::error::Error for DinocoError {}

impl From<tokio_postgres::Error> for DinocoError {
    fn from(e: tokio_postgres::Error) -> Self {
        Self::Postgres(e)
    }
}

impl From<deadpool_postgres::PoolError> for DinocoError {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        Self::ConnectionError(format!("Failed to get connection from pool: {}", e))
    }
}

impl From<deadpool_postgres::BuildError> for DinocoError {
    fn from(e: deadpool_postgres::BuildError) -> Self {
        Self::ConnectionError(format!("Failed to build connection pool: {}", e))
    }
}

impl From<deadpool_sqlite::CreatePoolError> for DinocoError {
    fn from(e: deadpool_sqlite::CreatePoolError) -> Self {
        Self::ConnectionError(format!("Failed to get connection from pool: {}", e))
    }
}

impl From<deadpool_sqlite::PoolError> for DinocoError {
    fn from(e: deadpool_sqlite::PoolError) -> Self {
        Self::ConnectionError(format!("Failed to get connection from pool: {}", e))
    }
}

impl From<deadpool_sqlite::BuildError> for DinocoError {
    fn from(e: deadpool_sqlite::BuildError) -> Self {
        Self::ConnectionError(format!("Failed to build connection pool: {}", e))
    }
}

impl From<deadpool_sqlite::InteractError> for DinocoError {
    fn from(e: deadpool_sqlite::InteractError) -> Self {
        Self::ParseError(e.to_string())
    }
}

impl From<rusqlite::Error> for DinocoError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<mysql_async::Error> for DinocoError {
    fn from(e: mysql_async::Error) -> Self {
        Self::MySql(e)
    }
}

impl From<tokio::task::JoinError> for DinocoError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::TaskJoin(e)
    }
}
