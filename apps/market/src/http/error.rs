use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde::Serialize;
use std::fmt;

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(&'static str),
    PayloadTooLarge,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict(&'static str),
    Unavailable,
    Internal,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: &'static str,
}

impl ApiError {
    fn response_parts(&self) -> (StatusCode, &'static str, &'static str) {
        match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, "invalid_request", message),
            Self::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Request exceeds the size limit",
            ),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "Sign in first"),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "Operation is not permitted",
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "Resource was not found"),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "Service is temporarily unavailable",
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Operation could not be completed",
            ),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.response_parts().2)
    }
}

impl std::error::Error for ApiError {}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        self.response_parts().0
    }
    fn error_response(&self) -> HttpResponse {
        let (status, error, message) = self.response_parts();
        HttpResponse::build(status).json(ErrorBody { error, message })
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::Database(database) if database.is_unique_violation() => {
                return Self::Conflict("Record already exists");
            }
            _ => {}
        }
        tracing::error!(error = %error, "database operation failed");
        Self::Internal
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        tracing::error!(kind = ?error.kind(), "media storage operation failed");
        Self::Internal
    }
}
