use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
pub enum GhostDesktopError {
    #[error("Gateway not running")]
    GatewayNotRunning,
    #[error("Gateway failed to start: {reason}")]
    GatewayStartFailed { reason: String },
    #[error("Gateway health check failed: {reason}")]
    HealthCheckFailed { reason: String },
    #[error("Configuration error: {reason}")]
    ConfigError { reason: String },
    #[error("IO error: {reason}")]
    IoError { reason: String },
}
