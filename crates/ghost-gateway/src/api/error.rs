//! Standard error response contract for all API endpoints.
//!
//! All errors return a consistent JSON envelope:
//! ```json
//! {
//!   "error": {
//!     "code": "MACHINE_READABLE_CODE",
//!     "message": "Human-readable description",
//!     "details": {}
//!   }
//! }
//! ```
//!
//! Ref: ADE_DESIGN_PLAN §5.0.9, tasks.md T-1.3.2

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Standard error response envelope.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

/// Error body within the envelope.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ErrorBody {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            error: ErrorBody {
                code: code.into(),
                message: message.into(),
                details: Some(details),
            },
        }
    }
}

/// Convenience type for API handler results.
pub type ApiResult<T> = Result<Json<T>, ApiError>;

/// API error that converts to a proper HTTP response.
pub struct ApiError {
    pub status: StatusCode,
    pub body: ErrorResponse,
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: ErrorResponse::new("NOT_FOUND", message),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorResponse::new("INTERNAL_ERROR", message),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ErrorResponse::new("BAD_REQUEST", message),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            body: ErrorResponse::new("CONFLICT", message),
        }
    }

    pub fn db_error(context: &str, err: impl std::fmt::Display) -> Self {
        tracing::error!(context = context, error = %err, "Database error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorResponse::new("DB_ERROR", format!("{context}: database error")),
        }
    }

    pub fn lock_poisoned(resource: &str) -> Self {
        tracing::error!(resource = resource, "Lock poisoned");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorResponse::new(
                "LOCK_POISONED",
                format!("{resource} lock poisoned — restart required"),
            ),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
