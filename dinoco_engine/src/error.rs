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
