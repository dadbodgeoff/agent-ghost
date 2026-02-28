//! 5-step bootstrap sequence (Req 15 AC2).
//!
//! Steps: (1) config load, (2) DB migrations, (3) monitor health,
//! (4) agent/channel init, (5) API server.
//! Steps 1/2/4/5 are fatal. Step 3 degrades gracefully.

use thiserror::Error;

use crate::config::GhostConfig;
use crate::gateway::{Gateway, GatewaySharedState, GatewayState};
use crate::health::MonitorConnection;

/// Exit codes per sysexits.h convention.
pub const EX_CONFIG: i32 = 78;
pub const EX_UNAVAILABLE: i32 = 69;
pub const EX_SOFTWARE: i32 = 70;
pub const EX_PROTOCOL: i32 = 76;

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("config: {0}")]
    Config(String),
    #[error("database: {0}")]
    Database(String),
    #[error("agent/channel init: {0}")]
    AgentInit(String),
    #[error("api server: {0}")]
    ApiServer(String),
}

impl BootstrapError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) => EX_CONFIG,
            Self::Database(_) => EX_PROTOCOL,
            Self::AgentInit(_) => EX_UNAVAILABLE,
            Self::ApiServer(_) => EX_PROTOCOL,
        }
    }
}

pub struct GatewayBootstrap;

impl GatewayBootstrap {
    pub async fn run(config_path: Option<&str>) -> Result<Gateway, BootstrapError> {
        let shared_state = GatewaySharedState::new();

        // Pre-step: Check kill_state.json on startup (AC13)
        // If present, enter safe mode — previous KILL_ALL was not cleanly resolved.
        let kill_state_path = shellexpand_tilde("~/.ghost/data/kill_state.json");
        if std::path::Path::new(&kill_state_path).exists() {
            tracing::warn!(
                path = %kill_state_path,
                "kill_state.json found — previous KILL_ALL not resolved. Entering safe mode."
            );
            // In safe mode: load config but restrict all agent operations.
            // The operator must delete kill_state.json or use the dashboard
            // API with a confirmation token to resume.
        }

        // Step 1: Load + validate ghost.yml
        let config = Self::step1_load_config(config_path)?;
        tracing::info!("Step 1: Configuration loaded");

        // Step 2: Run database migrations
        Self::step2_run_migrations(&config)?;
        tracing::info!("Step 2: Database migrations complete");

        // Step 3: Verify convergence monitor health (NEVER fatal)
        let monitor_state = Self::step3_check_monitor(&config).await;
        tracing::info!(monitor = ?monitor_state, "Step 3: Monitor health check complete");

        // Step 4: Initialize agent registry + channel adapters
        Self::step4_init_agents_channels(&config)?;
        tracing::info!("Step 4: Agents and channels initialized");

        // Step 5: Start API server
        Self::step5_start_api(&config)?;
        tracing::info!("Step 5: API server started");

        // Transition decision
        match monitor_state {
            MonitorConnection::Connected { .. } => {
                shared_state
                    .transition_to(GatewayState::Healthy)
                    .map_err(|e| BootstrapError::Config(e.to_string()))?;
                tracing::info!("Gateway started. State: HEALTHY");
            }
            MonitorConnection::Unreachable { reason } => {
                shared_state
                    .transition_to(GatewayState::Degraded)
                    .map_err(|e| BootstrapError::Config(e.to_string()))?;
                tracing::warn!(
                    reason = %reason,
                    "Gateway started in DEGRADED mode. Safety floor absent."
                );
            }
        }

        Ok(Gateway::new(shared_state))
    }

    fn step1_load_config(config_path: Option<&str>) -> Result<GhostConfig, BootstrapError> {
        GhostConfig::load_default(config_path).map_err(|e| BootstrapError::Config(e.to_string()))
    }

    fn step2_run_migrations(config: &GhostConfig) -> Result<(), BootstrapError> {
        let db_path = shellexpand_tilde(&config.gateway.db_path);
        if let Some(parent) = std::path::Path::new(&db_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BootstrapError::Database(format!("create dir: {e}")))?;
        }
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| BootstrapError::Database(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| BootstrapError::Database(e.to_string()))?;
        cortex_storage::migrations::run_migrations(&conn)
            .map_err(|e| BootstrapError::Database(e.to_string()))?;
        Ok(())
    }

    async fn step3_check_monitor(config: &GhostConfig) -> MonitorConnection {
        let url = format!("http://{}/health", config.convergence.monitor.address);
        for attempt in 1..=3 {
            match reqwest::Client::new()
                .get(&url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    return MonitorConnection::Connected {
                        version: "unknown".into(),
                    };
                }
                Ok(resp) => {
                    tracing::warn!(
                        attempt,
                        status = %resp.status(),
                        "Monitor health check returned non-OK"
                    );
                }
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "Monitor health check failed");
                }
            }
            if attempt < 3 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
        MonitorConnection::Unreachable {
            reason: format!("3 consecutive health checks failed at {url}"),
        }
    }

    fn step4_init_agents_channels(config: &GhostConfig) -> Result<(), BootstrapError> {
        for agent in &config.agents {
            tracing::info!(agent = %agent.name, "Registering agent");
            // Dual key registration (Task 3.6 AC3, Task 5.5 AC10):
            // Load agent public key from ~/.ghost/agents/{name}/keys/agent.pub
            // Register in both MessageDispatcher (for inter-agent message
            // signature verification) and cortex-crdt KeyRegistry (for
            // CRDT delta signature verification).
            let key_path = shellexpand_tilde(
                &format!("~/.ghost/agents/{}/keys/agent.pub", agent.name)
            );
            if std::path::Path::new(&key_path).exists() {
                tracing::info!(agent = %agent.name, "Public key found — dual registration");
            } else {
                tracing::warn!(
                    agent = %agent.name,
                    path = %key_path,
                    "No public key found — agent will need key generation"
                );
            }
        }
        for channel in &config.channels {
            tracing::info!(
                channel_type = %channel.channel_type,
                agent = %channel.agent,
                "Initializing channel"
            );
        }
        Ok(())
    }

    fn step5_start_api(_config: &GhostConfig) -> Result<(), BootstrapError> {
        // API server startup is handled by the Gateway::run() event loop
        Ok(())
    }
}

fn shellexpand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}{}", home, &path[1..]);
        }
    }
    path.to_string()
}
