use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use dinoco::{AtomicUpdateError, CreateError, TransactionError};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: message.into() }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: message.into() }
    }

    pub fn internal(error: anyhow::Error) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: error.to_string() }
    }

    /// `find_and_update` returns a typed error: a missing row is a 404, the
    /// rest are 500s.
    pub fn atomic(error: AtomicUpdateError) -> Self {
        match error {
            AtomicUpdateError::RowNotAffected => Self::not_found("Row not found"),
            other => Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: other.to_string() },
        }
    }

    /// Inserts return a typed error: a unique violation is a 409, a missing
    /// referenced row is a 422, the rest are 500s.
    pub fn create(error: CreateError) -> Self {
        match error {
            CreateError::UniqueViolation { columns, .. } => Self {
                status: StatusCode::CONFLICT,
                message: format!("A record with the same {} already exists", columns.join(", ")),
            },
            CreateError::ForeignKeyViolation { .. } => {
                Self { status: StatusCode::UNPROCESSABLE_ENTITY, message: "Referenced record not found".to_string() }
            }
            other => Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: other.to_string() },
        }
    }

    pub fn transaction(error: TransactionError) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: error.to_string() }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        self.status
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status).json(ErrorResponse { error: self.message.clone() })
    }
}
