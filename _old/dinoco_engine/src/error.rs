#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    Unique,
    ForeignKey,
    NotNull,
    Check,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintError {
    pub kind: ConstraintKind,
    pub table: Option<String>,
    pub columns: Vec<String>,
    pub constraint: Option<String>,
    pub message: String,
}

#[derive(Debug)]
pub enum DinocoError {
    Constraint(ConstraintError),
    Postgres(tokio_postgres::Error),
    MySql(mysql_async::Error),
    Redis(redis::RedisError),
    Sqlite(rusqlite::Error),
    TaskJoin(tokio::task::JoinError),
    ParseError(String),
    ConnectionError(String),
    RecordNotFound(String),
    TypeMismatch,
    ColumnNotFound,
}

impl std::fmt::Display for DinocoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Constraint(error) => write!(f, "{}", error.message),
            Self::Postgres(e) => {
                if let Some(db_error) = e.as_db_error() {
                    write!(f, "Postgres error [{}]: {}", db_error.code().code(), db_error.message())?;

                    if let Some(detail) = db_error.detail() {
                        write!(f, " | detail: {}", detail)?;
                    }

                    if let Some(hint) = db_error.hint() {
                        write!(f, " | hint: {}", hint)?;
                    }

                    if let Some(table) = db_error.table() {
                        write!(f, " | table: {}", table)?;
                    }

                    if let Some(column) = db_error.column() {
                        write!(f, " | column: {}", column)?;
                    }

                    if let Some(constraint) = db_error.constraint() {
                        write!(f, " | constraint: {}", constraint)?;
                    }

                    Ok(())
                } else {
                    write!(f, "Postgres error: {}", e)
                }
            }
            Self::MySql(e) => write!(f, "MySQL error: {}", e),
            Self::Redis(e) => write!(f, "Redis error: {}", e),
            Self::Sqlite(e) => write!(f, "Sqlite error: {}", e),

            Self::TaskJoin(e) => write!(f, "Async task join error: {}", e),
            Self::ParseError(msg) => write!(f, "Data parse error: {}", msg),
            Self::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            Self::RecordNotFound(msg) => write!(f, "Record not found: {}", msg),

            Self::TypeMismatch => write!(f, "Row type mismatch.",),
            Self::ColumnNotFound => write!(f, "Column not found.",),
        }
    }
}

impl std::error::Error for DinocoError {}

impl From<std::io::Error> for DinocoError {
    fn from(err: std::io::Error) -> Self {
        Self::ParseError(err.to_string())
    }
}

impl From<redis::RedisError> for DinocoError {
    fn from(err: redis::RedisError) -> Self {
        Self::Redis(err)
    }
}

impl ConstraintError {
    pub fn unique(table: Option<String>, columns: Vec<String>, constraint: Option<String>, message: String) -> Self {
        Self { kind: ConstraintKind::Unique, table, columns, constraint, message }
    }

    pub fn foreign_key(
        table: Option<String>,
        columns: Vec<String>,
        constraint: Option<String>,
        message: String,
    ) -> Self {
        Self { kind: ConstraintKind::ForeignKey, table, columns, constraint, message }
    }

    pub fn not_null(table: Option<String>, columns: Vec<String>, constraint: Option<String>, message: String) -> Self {
        Self { kind: ConstraintKind::NotNull, table, columns, constraint, message }
    }

    pub fn check(table: Option<String>, columns: Vec<String>, constraint: Option<String>, message: String) -> Self {
        Self { kind: ConstraintKind::Check, table, columns, constraint, message }
    }
}
