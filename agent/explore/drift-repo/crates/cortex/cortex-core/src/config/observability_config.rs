use serde::{Deserialize, Serialize};

use super::defaults;

/// Observability subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// Metrics export interval in seconds.
    pub metrics_export_interval_secs: u64,
    /// Log level: "trace", "debug", "info", "warn", "error".
    pub log_level: String,
    /// Enable distributed tracing.
    pub tracing_enabled: bool,
    /// Health check interval in seconds.
    pub health_check_interval_secs: u64,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_export_interval_secs: defaults::DEFAULT_METRICS_EXPORT_INTERVAL_SECS,
            log_level: defaults::DEFAULT_LOG_LEVEL.to_string(),
            tracing_enabled: defaults::DEFAULT_TRACING_ENABLED,
            health_check_interval_secs: defaults::DEFAULT_HEALTH_CHECK_INTERVAL_SECS,
        }
    }
}
