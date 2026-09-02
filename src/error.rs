//! Application error type, rendered as `{ "status": N, "message": "..." }`
//! exactly like GarupaSpeedTracker's `ApiErrorBody`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Box error with an HTTP status and a consumer-facing message.
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum AppError {
    /// HTTP 422 with `details`, matching GarupaSpeedTracker's validation
    /// error contract `{status, message: "Validation Failed", details}`.
    #[error("Validation Failed")]
    Validation { message: String, field: String },

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Unavailable(String),

    #[error("upstream service error: {0}")]
    Upstream(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[allow(dead_code)]
impl AppError {
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            field: field.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self::Upstream(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = match self {
            Self::Validation { message, field } => Json(json!({
                "status": 422,
                "message": "Validation Failed",
                "details": [{ "message": message, "code": "invalid", "field": field }],
            })),
            other => Json(json!({
                "status": status.as_u16(),
                "message": other.to_string(),
            })),
        };
        (status, body).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
