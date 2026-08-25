use std::fmt;

/// Portable constraint categories exposed only when a driver supplies a
/// structured error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseConstraintError {
    UniqueViolation,
    ForeignKeyViolation,
    NotNullViolation,
    CheckViolation,
}

/// A database failure together with Dinoco's portable classification. The
/// original driver error remains available through the standard error chain.
#[derive(Debug)]
pub struct DatabaseError {
    constraint: Option<DatabaseConstraintError>,
    source: anyhow::Error,
}

impl DatabaseError {
    pub fn new(source: anyhow::Error) -> Self {
        let constraint = classify_constraint(&source);
        Self { constraint, source }
    }

    pub fn constraint(&self) -> Option<DatabaseConstraintError> {
        self.constraint
    }

    pub fn original(&self) -> &anyhow::Error {
        &self.source
    }

    pub fn into_original(self) -> anyhow::Error {
        self.source
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl std::error::Error for DatabaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Marker used by adapters when a row exists but a generated model cannot be
/// decoded from it.
#[derive(Debug)]
pub struct RowDecodeError {
    model: &'static str,
}

impl RowDecodeError {
    pub fn new(model: &'static str) -> Self {
        Self { model }
    }
}

impl fmt::Display for RowDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "failed to decode database row as `{}`", self.model)
    }
}

impl std::error::Error for RowDecodeError {}

pub fn is_decode_error(error: &anyhow::Error) -> bool {
    error.chain().any(|source| source.is::<RowDecodeError>())
}

fn classify_constraint(error: &anyhow::Error) -> Option<DatabaseConstraintError> {
    for source in error.chain() {
        if let Some(error) = source.downcast_ref::<rusqlite::Error>()
            && let rusqlite::Error::SqliteFailure(code, _) = error
        {
            return match code.extended_code {
                rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY => {
                    Some(DatabaseConstraintError::UniqueViolation)
                }
                rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY => Some(DatabaseConstraintError::ForeignKeyViolation),
                rusqlite::ffi::SQLITE_CONSTRAINT_NOTNULL => Some(DatabaseConstraintError::NotNullViolation),
                rusqlite::ffi::SQLITE_CONSTRAINT_CHECK => Some(DatabaseConstraintError::CheckViolation),
                _ => None,
            };
        }

        if let Some(error) = source.downcast_ref::<tokio_postgres::Error>()
            && let Some(error) = error.as_db_error()
        {
            return match *error.code() {
                tokio_postgres::error::SqlState::UNIQUE_VIOLATION => Some(DatabaseConstraintError::UniqueViolation),
                tokio_postgres::error::SqlState::FOREIGN_KEY_VIOLATION => {
                    Some(DatabaseConstraintError::ForeignKeyViolation)
                }
                tokio_postgres::error::SqlState::NOT_NULL_VIOLATION => Some(DatabaseConstraintError::NotNullViolation),
                tokio_postgres::error::SqlState::CHECK_VIOLATION => Some(DatabaseConstraintError::CheckViolation),
                _ => None,
            };
        }

        if let Some(mysql_async::Error::Server(error)) = source.downcast_ref::<mysql_async::Error>() {
            return match error.code {
                1062 => Some(DatabaseConstraintError::UniqueViolation),
                1451 | 1452 => Some(DatabaseConstraintError::ForeignKeyViolation),
                1048 => Some(DatabaseConstraintError::NotNullViolation),
                3819 => Some(DatabaseConstraintError::CheckViolation),
                _ => None,
            };
        }
    }

    None
}
