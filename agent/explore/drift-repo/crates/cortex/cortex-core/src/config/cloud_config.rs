use serde::{Deserialize, Serialize};

use super::defaults;

/// Cloud sync configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudConfig {
    /// Cloud endpoint URL.
    pub endpoint_url: Option<String>,
    /// Authentication method: "api_key", "oauth", "none".
    pub auth_method: String,
    /// Sync interval in seconds.
    pub sync_interval_secs: u64,
    /// Conflict resolution strategy: "local_wins", "remote_wins", "latest_wins".
    pub conflict_resolution: String,
    /// Offline mode — disable cloud sync entirely.
    pub offline_mode: bool,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            endpoint_url: None,
            auth_method: "none".to_string(),
            sync_interval_secs: defaults::DEFAULT_SYNC_INTERVAL_SECS,
            conflict_resolution: "latest_wins".to_string(),
            offline_mode: defaults::DEFAULT_OFFLINE_MODE,
        }
    }
}
