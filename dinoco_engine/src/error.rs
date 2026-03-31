#[derive(Debug)]
pub enum DinocoError {
    Postgres(tokio_postgres::Error),
    MySql(mysql_async::Error),
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
        // Mapeamos falhas ao pegar a conexão para o seu ConnectionError
        Self::ConnectionError(format!("Failed to get connection from pool: {}", e))
    }
}

impl From<deadpool_postgres::BuildError> for DinocoError {
    fn from(e: deadpool_postgres::BuildError) -> Self {
        // Mapeamos falhas na construção da Pool para o seu ConnectionError
        Self::ConnectionError(format!("Failed to build connection pool: {}", e))
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
