use dinoco_engine::{DatabaseConstraintError, DatabaseError, is_decode_error};

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

operation_error!(CreateError, "create");
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
