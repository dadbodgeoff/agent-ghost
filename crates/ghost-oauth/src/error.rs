//! Error types for the OAuth brokering infrastructure.

use thiserror::Error;

/// Errors from OAuth operations.
#[derive(Debug, Error)]
pub enum OAuthError {
    /// The access token has expired and needs refresh.
    #[error("token expired for ref_id {0}")]
    TokenExpired(String),

    /// The token was revoked (e.g. by kill switch or user disconnect).
    #[error("token revoked for ref_id {0}")]
    TokenRevoked(String),

    /// A provider-specific error occurred during an API call.
    #[error("provider error: {0}")]
    ProviderError(String),

    /// The OAuth authorization flow failed.
    #[error("OAuth flow failed: {0}")]
    FlowFailed(String),

    /// Token refresh failed (refresh token invalid or expired).
    #[error("token refresh failed: {0}")]
    RefreshFailed(String),

    /// No connection exists for the given ref_id.
    #[error("not connected: {0}")]
    NotConnected(String),

    /// Invalid or tampered state parameter in OAuth callback.
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// Storage I/O error (encrypted token file operations).
    #[error("storage error: {0}")]
    StorageError(String),

    /// Encryption or decryption failure.
    #[error("encryption error: {0}")]
    EncryptionError(String),
}
