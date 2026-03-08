//! Monitor configuration.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionThresholdConfig {
    /// Minimum signal score that forces a Level 2 minimum on safety-critical signals.
    pub critical_override_threshold: f64,
}

impl Default for InterventionThresholdConfig {
    fn default() -> Self {
        Self {
            critical_override_threshold: 0.85,
        }
    }
}

/// Top-level monitor configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Path to SQLite database.
    pub db_path: PathBuf,
    /// HTTP API port (default 18790).
    pub http_port: u16,
    /// Unix socket path.
    pub socket_path: PathBuf,
    /// State publication directory.
    pub state_dir: PathBuf,
    /// Calibration sessions before scoring begins (default 10).
    pub calibration_sessions: u32,
    /// Rate limit: events per minute per connection (default 100).
    pub rate_limit_per_min: u32,
    /// Clock skew tolerance (default 5 minutes).
    pub clock_skew_tolerance: Duration,
    /// Score cache TTL (default 30 seconds).
    pub score_cache_ttl: Duration,
    /// Max provisional tracking sessions for unknown agents (default 3).
    pub max_provisional_sessions: u32,
    /// Health check interval (default 30 seconds).
    pub health_check_interval: Duration,
    /// Idle horizon before a session is considered stale and pruned.
    pub session_idle_horizon: Duration,
    /// Idle horizon before an unused rate-limit bucket is pruned.
    pub rate_limit_bucket_idle_horizon: Duration,
    /// Dual-key confirmation TTL.
    pub dual_key_ttl: Duration,
    /// Runtime-configurable intervention thresholds.
    pub intervention_thresholds: InterventionThresholdConfig,
    /// Default profile name from gateway config.
    pub default_profile: String,
    /// Signal weights for composite scoring (8 weights, default equal 1/8).
    pub signal_weights: [f64; 8],
    /// Enable native messaging transport for browser extensions (default false).
    pub native_messaging_enabled: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        let home = ghost_gateway::bootstrap::ghost_home();
        Self {
            db_path: home.join("data/ghost.db"),
            http_port: 18790,
            socket_path: home.join("monitor.sock"),
            state_dir: home.join("data/convergence_state"),
            calibration_sessions: 10,
            rate_limit_per_min: 100,
            clock_skew_tolerance: Duration::from_secs(300),
            score_cache_ttl: Duration::from_secs(30),
            max_provisional_sessions: 3,
            health_check_interval: Duration::from_secs(30),
            session_idle_horizon: Duration::from_secs(30 * 60),
            rate_limit_bucket_idle_horizon: Duration::from_secs(30 * 60),
            dual_key_ttl: Duration::from_secs(300),
            intervention_thresholds: InterventionThresholdConfig::default(),
            default_profile: "standard".to_string(),
            signal_weights: [1.0 / 8.0; 8],
            native_messaging_enabled: false,
        }
    }
}

impl MonitorConfig {
    pub fn load() -> anyhow::Result<Self> {
        let ghost_config = ghost_gateway::config::GhostConfig::load_default(None)?;
        let mut config = Self::default();
        config.db_path = PathBuf::from(ghost_gateway::bootstrap::shellexpand_tilde(
            &ghost_config.gateway.db_path,
        ));
        config.http_port = parse_monitor_port(&ghost_config.convergence.monitor.address)?;
        config.default_profile = ghost_config.convergence.profile;
        Ok(config)
    }
}

fn parse_monitor_port(address: &str) -> anyhow::Result<u16> {
    let (_, port) = address.rsplit_once(':').ok_or_else(|| {
        anyhow::anyhow!("invalid monitor address '{address}': expected host:port")
    })?;
    port.parse::<u16>()
        .map_err(|error| anyhow::anyhow!("invalid monitor port in '{address}': {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn load_uses_same_db_path_source_as_gateway_on_non_default_config() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("ghost.yml");
        let yaml = format!(
            "gateway:\n  db_path: \"~/.ghost/data/custom.db\"\nconvergence:\n  monitor:\n    address: \"127.0.0.1:28790\"\n"
        );
        std::fs::write(&config_path, yaml).unwrap();

        std::env::set_var("GHOST_HOME", temp_dir.path());
        std::env::set_var("GHOST_CONFIG", &config_path);

        let config = MonitorConfig::load().unwrap();
        assert_eq!(config.db_path, temp_dir.path().join("data/custom.db"));
        assert_eq!(config.http_port, 28790);

        std::env::remove_var("GHOST_CONFIG");
        std::env::remove_var("GHOST_HOME");
    }
}
