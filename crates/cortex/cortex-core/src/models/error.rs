//! Cortex error taxonomy.

use thiserror::Error;

/// Unified error type for all Cortex operations.
#[derive(Debug, Error)]
pub enum CortexError {
    #[error("memory not found: {id}")]
    NotFound { id: String },

    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    // ── Convergence additions (Req 2 AC6) ───────────────────────────
    #[error("authorization denied: {reason}")]
    AuthorizationDenied { reason: String },

    #[error("session boundary: {reason}")]
    SessionBoundary { reason: String },
}

/// Convenience alias used throughout Cortex crates.
pub type CortexResult<T> = Result<T, CortexError>;
