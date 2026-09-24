use dinoco_engine::{ConstraintDetails, DatabaseConstraintError, DatabaseError, is_decode_error};

macro_rules! operation_error {
    ($name:ident, $label:literal) => {
        #[derive(Debug, thiserror::Error)]
        pub enum $name {
            #[error("invalid operation: {0}")]
            InvalidOperation(String),
            #[error("database row decode failed: {0}")]
            Decode(#[source] DatabaseError),
            #[error("database constraint violation ({kind:?}): {source}")]
            Constraint {
                kind: DatabaseConstraintError,
                #[source]
                source: DatabaseError,
            },
            #[error("database error: {0}")]
            Database(#[source] DatabaseError),
        }

        impl $name {
            pub(crate) fn from_database(error: anyhow::Error) -> Self {
                if is_decode_error(&error) {
                    return Self::Decode(DatabaseError::new(error));
                }
                let error = DatabaseError::new(error);
                if let Some(kind) = error.constraint() {
                    Self::Constraint { kind, source: error }
                } else {
                    Self::Database(error)
                }
            }
        }
    };
}

/// Failures produced by `insert_into` and `insert_many`.
///
/// Constraint violations get one variant per kind, carrying the table,
/// constraint name, and columns the driver reported (see
/// [`ConstraintDetails`]); the original driver error stays in `source`.
#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error("invalid operation: {0}")]
    InvalidOperation(String),

    #[error("unique constraint violated{}: {source}", violation_target(.table, .constraint, .columns))]
    UniqueViolation {
        table: Option<String>,
        constraint: Option<String>,
        columns: Vec<String>,
        #[source]
        source: DatabaseError,
    },

    #[error("foreign key constraint violated{}: {source}", violation_target(.table, .constraint, .columns))]
    ForeignKeyViolation {
        table: Option<String>,
        constraint: Option<String>,
        columns: Vec<String>,
        #[source]
        source: DatabaseError,
    },

    #[error("not null constraint violated{}: {source}", violation_target(.table, .constraint, .columns))]
    NotNullViolation {
        table: Option<String>,
        constraint: Option<String>,
        columns: Vec<String>,
        #[source]
        source: DatabaseError,
    },

    #[error("check constraint violated{}: {source}", violation_target(.table, .constraint, .columns))]
    CheckViolation {
        table: Option<String>,
        constraint: Option<String>,
        columns: Vec<String>,
        #[source]
        source: DatabaseError,
    },

    #[error("record from table `{table}` could not be returned after insert")]
    NotReturned { table: &'static str },

    #[error("database row decode failed: {0}")]
    Decode(#[source] DatabaseError),

    #[error("database error: {0}")]
    Database(#[source] DatabaseError),
}

impl CreateError {
    pub(crate) fn from_database(error: anyhow::Error) -> Self {
        if is_decode_error(&error) {
            return Self::Decode(DatabaseError::new(error));
        }

        let source = DatabaseError::new(error);
        let Some(kind) = source.constraint() else {
            return Self::Database(source);
        };
        let ConstraintDetails { table, constraint, columns } = source.constraint_details().clone();

        match kind {
            DatabaseConstraintError::UniqueViolation => Self::UniqueViolation { table, constraint, columns, source },
            DatabaseConstraintError::ForeignKeyViolation => {
                Self::ForeignKeyViolation { table, constraint, columns, source }
            }
            DatabaseConstraintError::NotNullViolation => Self::NotNullViolation { table, constraint, columns, source },
            DatabaseConstraintError::CheckViolation => Self::CheckViolation { table, constraint, columns, source },
        }
    }

    /// The portable constraint category, when the insert violated one.
    pub fn constraint(&self) -> Option<DatabaseConstraintError> {
        match self {
            Self::UniqueViolation { .. } => Some(DatabaseConstraintError::UniqueViolation),
            Self::ForeignKeyViolation { .. } => Some(DatabaseConstraintError::ForeignKeyViolation),
            Self::NotNullViolation { .. } => Some(DatabaseConstraintError::NotNullViolation),
            Self::CheckViolation { .. } => Some(DatabaseConstraintError::CheckViolation),
            _ => None,
        }
    }

    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::UniqueViolation { .. })
    }

    /// Columns involved in a constraint violation, as reported by the driver.
    pub fn columns(&self) -> &[String] {
        match self {
            Self::UniqueViolation { columns, .. }
            | Self::ForeignKeyViolation { columns, .. }
            | Self::NotNullViolation { columns, .. }
            | Self::CheckViolation { columns, .. } => columns,
            _ => &[],
        }
    }

    /// The underlying database error, when the failure came from the driver.
    pub fn database_error(&self) -> Option<&DatabaseError> {
        match self {
            Self::UniqueViolation { source, .. }
            | Self::ForeignKeyViolation { source, .. }
            | Self::NotNullViolation { source, .. }
            | Self::CheckViolation { source, .. } => Some(source),
            Self::Decode(source) | Self::Database(source) => Some(source),
            Self::InvalidOperation(_) | Self::NotReturned { .. } => None,
        }
    }
}

fn violation_target(table: &Option<String>, constraint: &Option<String>, columns: &[String]) -> String {
    let mut target = String::new();

    if let Some(table) = table {
        target.push_str(&format!(" on `{table}`"));
    }
    if !columns.is_empty() {
        target.push_str(&format!(" ({})", columns.join(", ")));
    }
    if let Some(constraint) = constraint {
        target.push_str(&format!(" [{constraint}]"));
    }

    target
}

operation_error!(UpdateError, "update");
operation_error!(DeleteError, "delete");

/// Failures produced by `find_and_update` and other single-row atomic
/// mutations.
#[derive(Debug, thiserror::Error)]
pub enum AtomicUpdateError {
    #[error("no row satisfied the atomic update conditions")]
    RowNotAffected,

    #[error("find_and_update requires at least one update operation")]
    EmptyUpdate,

    #[error("field `{0}` is updated more than once in one statement")]
    DuplicateField(&'static str),

    #[error("failed to decode the row returned by the atomic update: {0}")]
    Decode(#[source] DatabaseError),

    #[error("atomic update violated a database constraint ({kind:?}): {source}")]
    Constraint {
        kind: DatabaseConstraintError,
        #[source]
        source: DatabaseError,
    },

    #[error("atomic update database error: {0}")]
    Database(#[source] DatabaseError),
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("failed to begin transaction: {0}")]
    Begin(#[source] DatabaseError),
    #[error("create failed: {0}")]
    Create(#[from] CreateError),
    #[error("update failed: {0}")]
    Update(#[from] UpdateError),
    #[error("delete failed: {0}")]
    Delete(#[from] DeleteError),
    #[error("atomic update failed: {0}")]
    AtomicUpdate(#[from] AtomicUpdateError),
    #[error("transaction operation failed: {0}")]
    Operation(#[source] anyhow::Error),
    #[error("failed to commit transaction: {0}")]
    Commit(#[source] DatabaseError),
    #[error("rollback failed after `{source}`: {rollback_error}")]
    RollbackFailed {
        source: Box<TransactionError>,
        #[source]
        rollback_error: DatabaseError,
    },
}

impl TransactionError {
    pub(crate) fn from_operation(error: anyhow::Error) -> Self {
        if error.is::<AtomicUpdateError>() {
            return Self::AtomicUpdate(error.downcast().expect("checked atomic update error"));
        }
        if error.is::<CreateError>() {
            return Self::Create(error.downcast().expect("checked create error"));
        }
        if error.is::<UpdateError>() {
            return Self::Update(error.downcast().expect("checked update error"));
        }
        if error.is::<DeleteError>() {
            return Self::Delete(error.downcast().expect("checked delete error"));
        }
        Self::Operation(error)
    }
}

impl AtomicUpdateError {
    pub(crate) fn from_database(error: anyhow::Error) -> Self {
        if is_decode_error(&error) {
            return Self::Decode(DatabaseError::new(error));
        }

        let error = DatabaseError::new(error);
        if let Some(kind) = error.constraint() {
            Self::Constraint { kind, source: error }
        } else {
            Self::Database(error)
        }
    }
}
