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
///
/// Each variant maps to a specific HTTP status code and machine-readable error
/// code in the JSON envelope.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Not found: {entity} {id}")]
    NotFound { entity: String, id: String },

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Kill switch active")]
    KillSwitchActive,

    #[error("Database error: {0}")]
    Database(String),

    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Internal error: {0}")]
    Internal(String),

    /// Escape-hatch variant for custom status code / error code combinations
    /// that don't fit the standard variants (e.g. skill execution errors).
    #[error("{message}")]
    Custom {
        status: StatusCode,
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
}

impl ApiError {
    // ── Backward-compatible convenience constructors ────────────────

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            entity: "resource".into(),
            id: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn db_error(context: &str, err: impl std::fmt::Display) -> Self {
        tracing::error!(context = context, error = %err, "Database error");
        Self::Database(format!("{context}: database error"))
    }

    pub fn lock_poisoned(resource: &str) -> Self {
        tracing::error!(resource = resource, "Lock poisoned");
        Self::LockPoisoned(format!("{resource} lock poisoned — restart required"))
    }

    /// Build an error with a custom HTTP status, error code, and optional
    /// JSON details.  Used by skill execution and other specialised handlers.
    pub fn with_details(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self::Custom {
            status,
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }

    /// Build an error with a custom HTTP status and error code but no details.
    pub fn custom(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Custom {
            status,
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }
}

// ── From impls ─────────────────────────────────────────────────────

impl From<rusqlite::Error> for ApiError {
    fn from(err: rusqlite::Error) -> Self {
        // Log the full error server-side; return a generic message to the client
        // to avoid leaking SQL statements, table names, or schema details.
        tracing::error!(error = %err, "rusqlite error");
        Self::Database("internal database error".into())
    }
}

impl From<crate::db_pool::DbPoolError> for ApiError {
    fn from(err: crate::db_pool::DbPoolError) -> Self {
        tracing::error!(error = %err, "db pool error");
        Self::Database("internal database error".into())
    }
}

// ── IntoResponse ───────────────────────────────────────────────────

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match &self {
            ApiError::NotFound { entity, id } => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND".to_owned(),
                format!("Not found: {entity} {id}"),
                None,
            ),
            ApiError::Validation(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR".to_owned(),
                msg.clone(),
                None,
            ),
            ApiError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED".to_owned(),
                msg.clone(),
                None,
            ),
            ApiError::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN".to_owned(),
                msg.clone(),
                None,
            ),
            ApiError::Conflict(msg) => (
                StatusCode::CONFLICT,
                "CONFLICT".to_owned(),
                msg.clone(),
                None,
            ),
            ApiError::KillSwitchActive => (
                StatusCode::SERVICE_UNAVAILABLE,
                "KILL_SWITCH_ACTIVE".to_owned(),
                "Kill switch active".to_owned(),
                None,
            ),
            ApiError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR".to_owned(),
                "An internal database error occurred".to_owned(),
                None,
            ),
            ApiError::LockPoisoned(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR".to_owned(),
                "An internal error occurred — please retry or restart the service".to_owned(),
                None,
            ),
            ApiError::Provider(_) => (
                StatusCode::BAD_GATEWAY,
                "PROVIDER_ERROR".to_owned(),
                "An upstream provider error occurred".to_owned(),
                None,
            ),
            ApiError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR".to_owned(),
                "An internal error occurred".to_owned(),
                None,
            ),
            ApiError::Custom {
                status,
                code,
                message,
                details,
            } => (*status, code.clone(), message.clone(), details.clone()),
        };

        let body = ErrorResponse {
            error: ErrorBody {
                code,
                message,
                details,
            },
        };

        (status, Json(body)).into_response()
    }
}
