//! Monitor configuration.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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
    /// Signal weights for composite scoring (8 weights, default equal 1/8).
    /// Order: S1 session_duration, S2 inter_session_gap, S3 response_latency,
    /// S4 vocabulary_convergence, S5 goal_boundary_erosion, S6 initiative_balance,
    /// S7 disengagement_resistance, S8 behavioral_anomaly.
    pub signal_weights: [f64; 8],
    /// Enable native messaging transport for browser extensions (default false).
    /// When enabled, spawns a Chrome/Firefox native messaging listener on stdin.
    pub native_messaging_enabled: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        let home = dirs_path();
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
            signal_weights: [1.0 / 8.0; 8],
            native_messaging_enabled: false,
        }
    }
}

impl MonitorConfig {
    pub fn load() -> anyhow::Result<Self> {
        // In production, load from ghost.yml. For now, use defaults.
        Ok(Self::default())
    }
}

fn dirs_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ghost")
}

/// Minimal dirs helper — resolve home directory.
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
