//! CLI error types (Task 6.6 — §7.1).

use thiserror::Error;

/// Unified error type for all CLI commands.
#[derive(Debug, Error)]
pub enum CliError {
    #[error("config error: {0}")]
    Config(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("authentication required")]
    AuthRequired,
    #[error("auth error: {0}")]
    Auth(String),
    #[error("gateway not available")]
    GatewayRequired,
    #[error("no backend available")]
    NoBackend,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("cancelled")]
    Cancelled,
    #[error("internal error: {0}")]
    Internal(String),
    #[error("usage error: {0}")]
    Usage(String),
}

impl CliError {
    /// Map error variant to sysexits.h exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) => 78,                          // EX_CONFIG
            Self::Database(_) => 76,                        // EX_PROTOCOL
            Self::GatewayRequired | Self::NoBackend => 69,  // EX_UNAVAILABLE
            Self::AuthRequired | Self::Auth(_) => 77,       // EX_NOPERM
            Self::Internal(_) => 70,                        // EX_SOFTWARE
            Self::Usage(_) => 64,                           // EX_USAGE
            Self::Http(_) | Self::NotFound(_) | Self::Conflict(_) | Self::Cancelled => 1,
        }
    }
}

impl From<crate::config::ConfigError> for CliError {
    fn from(e: crate::config::ConfigError) -> Self {
        Self::Config(e.to_string())
    }
}
