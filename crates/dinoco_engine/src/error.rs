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

/// Where a constraint violation happened, as far as the driver reports it.
///
/// Every field is best effort: PostgreSQL reports them structurally, while
/// SQLite and MySQL only mention them in the error message, so a field is left
/// empty whenever the driver does not name it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConstraintDetails {
    pub table: Option<String>,
    pub constraint: Option<String>,
    pub columns: Vec<String>,
}

/// A database failure together with Dinoco's portable classification. The
/// original driver error remains available through the standard error chain.
#[derive(Debug)]
pub struct DatabaseError {
    constraint: Option<DatabaseConstraintError>,
    details: ConstraintDetails,
    source: anyhow::Error,
}

impl DatabaseError {
    pub fn new(source: anyhow::Error) -> Self {
        let constraint = classify_constraint(&source);
        let details = if constraint.is_some() { constraint_details(&source) } else { ConstraintDetails::default() };
        Self { constraint, details, source }
    }

    pub fn constraint(&self) -> Option<DatabaseConstraintError> {
        self.constraint
    }

    /// Table, constraint name, and columns involved in a constraint violation.
    /// Empty when the error is not a classified constraint violation.
    pub fn constraint_details(&self) -> &ConstraintDetails {
        &self.details
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
                1048 | 1364 => Some(DatabaseConstraintError::NotNullViolation),
                3819 => Some(DatabaseConstraintError::CheckViolation),
                _ => None,
            };
        }
    }

    None
}

fn constraint_details(error: &anyhow::Error) -> ConstraintDetails {
    for source in error.chain() {
        if let Some(rusqlite::Error::SqliteFailure(_, Some(message))) = source.downcast_ref::<rusqlite::Error>() {
            return sqlite_constraint_details(message);
        }

        if let Some(error) = source.downcast_ref::<tokio_postgres::Error>()
            && let Some(error) = error.as_db_error()
        {
            let mut columns = error.column().map(|column| vec![column.to_string()]).unwrap_or_default();
            if columns.is_empty() {
                columns = error.detail().map(postgres_detail_columns).unwrap_or_default();
            }

            return ConstraintDetails {
                table: error.table().map(str::to_string),
                constraint: error.constraint().map(str::to_string),
                columns,
            };
        }

        if let Some(mysql_async::Error::Server(error)) = source.downcast_ref::<mysql_async::Error>() {
            return mysql_constraint_details(error.code, &error.message);
        }
    }

    ConstraintDetails::default()
}

/// `UNIQUE constraint failed: account.email, account.tenant_id`,
/// `NOT NULL constraint failed: account.email`, `CHECK constraint failed: name`.
fn sqlite_constraint_details(message: &str) -> ConstraintDetails {
    let Some((kind, target)) = message.split_once(" constraint failed: ") else {
        return ConstraintDetails::default();
    };

    if kind == "CHECK" {
        return ConstraintDetails { constraint: Some(target.trim().to_string()), ..Default::default() };
    }

    let mut details = ConstraintDetails::default();
    for column in target.split(',').map(str::trim) {
        match column.rsplit_once('.') {
            Some((table, column)) => {
                details.table.get_or_insert_with(|| table.to_string());
                details.columns.push(column.to_string());
            }
            None => details.columns.push(column.to_string()),
        }
    }

    details
}

/// `Key (email)=(duplicate@dinoco.rs) already exists.`
fn postgres_detail_columns(detail: &str) -> Vec<String> {
    detail
        .strip_prefix("Key (")
        .and_then(|rest| rest.split_once(")="))
        .map(|(columns, _)| columns.split(',').map(|column| column.trim().trim_matches('"').to_string()).collect())
        .unwrap_or_default()
}

fn mysql_constraint_details(code: u16, message: &str) -> ConstraintDetails {
    match code {
        // Duplicate entry 'value' for key 'table.key_name'
        1062 => {
            let key = message.rsplit_once(" for key '").map(|(_, key)| key.trim_end_matches('\''));
            match key.and_then(|key| key.split_once('.')) {
                Some((table, constraint)) => ConstraintDetails {
                    table: Some(table.to_string()),
                    constraint: Some(constraint.to_string()),
                    columns: Vec::new(),
                },
                None => ConstraintDetails { constraint: key.map(str::to_string), ..Default::default() },
            }
        }
        // Column 'email' cannot be null / Field 'email' doesn't have a default value
        1048 | 1364 => ConstraintDetails {
            columns: between(message, "'", "'").map(|column| vec![column.to_string()]).unwrap_or_default(),
            ..Default::default()
        },
        // ... a foreign key constraint fails (`db`.`table`, CONSTRAINT `name` FOREIGN KEY (`column`) REFERENCES ...
        1451 | 1452 => ConstraintDetails {
            table: between(message, "`.`", "`").map(str::to_string),
            constraint: between(message, "CONSTRAINT `", "`").map(str::to_string),
            columns: between(message, "FOREIGN KEY (", ")")
                .map(|columns| columns.split(',').map(|column| column.trim().trim_matches('`').to_string()).collect())
                .unwrap_or_default(),
        },
        // Check constraint 'name' is violated.
        3819 => ConstraintDetails { constraint: between(message, "'", "'").map(str::to_string), ..Default::default() },
        _ => ConstraintDetails::default(),
    }
}

fn between<'a>(message: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = message.split_once(start)?;
    rest.split_once(end).map(|(value, _)| value)
}
