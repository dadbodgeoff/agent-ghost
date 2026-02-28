//! Error types for the secrets infrastructure.

use thiserror::Error;

/// Errors from secret provider operations.
#[derive(Debug, Error)]
pub enum SecretsError {
    /// The requested secret key was not found.
    #[error("secret not found: {0}")]
    NotFound(String),

    /// The storage backend is unavailable or read-only.
    #[error("storage unavailable: {0}")]
    StorageUnavailable(String),

    /// A provider-specific error occurred.
    #[error("provider error: {0}")]
    ProviderError(String),

    /// The secret key is invalid (empty, contains null bytes, etc.).
    #[error("invalid key: {0}")]
    InvalidKey(String),
}
