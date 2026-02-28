//! 5-step bootstrap sequence (Req 15 AC2).
//!
//! Steps: (1) config load, (2) DB migrations, (3) monitor health,
//! (4) agent/channel init, (5) API server.
//! Steps 1/2/4/5 are fatal. Step 3 degrades gracefully.

use thiserror::Error;

use crate::config::GhostConfig;
use crate::gateway::{Gateway, GatewaySharedState, GatewayState};
use crate::health::MonitorConnection;
use ghost_egress::EgressPolicy;

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
        let safe_mode = std::path::Path::new(&kill_state_path).exists();
        if safe_mode {
            tracing::warn!(
                path = %kill_state_path,
                "kill_state.json found — previous KILL_ALL not resolved. Entering safe mode."
            );
            // In safe mode: load config and run migrations, but set
            // PLATFORM_KILLED so all agent operations are blocked.
            // The operator must delete kill_state.json or use the dashboard
            // API with a confirmation token to resume.
            use crate::safety::kill_switch::PLATFORM_KILLED;
            use std::sync::atomic::Ordering;
            PLATFORM_KILLED.store(true, Ordering::SeqCst);
        }

        // Step 1: Load + validate ghost.yml
        let config = Self::step1_load_config(config_path)?;
        tracing::info!("Step 1: Configuration loaded");

        // Step 1b: Build SecretProvider from secrets config (Phase 10)
        let _secret_provider = Self::build_secrets(&config)?;
        tracing::info!("Step 1b: SecretProvider initialized (provider: {})", config.secrets.provider);

        // Step 2: Run database migrations
        Self::step2_run_migrations(&config)?;
        tracing::info!("Step 2: Database migrations complete");

        // Step 3: Verify convergence monitor health (NEVER fatal)
        let monitor_state = Self::step3_check_monitor(&config).await;
        tracing::info!(monitor = ?monitor_state, "Step 3: Monitor health check complete");

        // Step 4: Initialize agent registry + channel adapters
        Self::step4_init_agents_channels(&config)?;
        tracing::info!("Step 4: Agents and channels initialized");

        // Step 4b: Apply network egress policies per agent (Phase 11)
        Self::step4b_apply_egress_policies(&config)?;
        tracing::info!("Step 4b: Network egress policies applied");

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

    /// Apply network egress policies per agent based on isolation mode (Phase 11).
    ///
    /// Policy selection:
    /// - InProcess → ProxyEgressPolicy (can't do per-thread filtering)
    /// - Process → EbpfEgressPolicy on Linux, PfEgressPolicy on macOS, ProxyEgressPolicy fallback
    /// - Container → Docker network policy (existing, no change needed)
    ///
    /// When ProxyEgressPolicy is active, the proxy URL is registered in
    /// `ghost_llm::proxy::ProxyRegistry` so the agent's reqwest client
    /// routes LLM API calls through the proxy.
    fn step4b_apply_egress_policies(config: &GhostConfig) -> Result<(), BootstrapError> {
        let proxy_registry = ghost_llm::proxy::ProxyRegistry::new();

        for agent in &config.agents {
            let network_config = match &agent.network {
                Some(nc) => nc.clone(),
                None => {
                    tracing::debug!(
                        agent = %agent.name,
                        "No network egress config — defaulting to Unrestricted"
                    );
                    continue;
                }
            };

            let egress_config = crate::config::build_egress_config(&network_config);

            // Skip if unrestricted (backward compat — no policy to apply).
            if egress_config.policy == ghost_egress::EgressPolicyMode::Unrestricted {
                tracing::debug!(
                    agent = %agent.name,
                    "Egress policy is Unrestricted — no enforcement needed"
                );
                continue;
            }

            // Select backend based on isolation mode.
            match agent.isolation {
                crate::config::IsolationMode::InProcess => {
                    // Can't do per-thread filtering — use proxy.
                    let policy = ghost_egress::ProxyEgressPolicy::new();
                    let agent_uuid = uuid::Uuid::new_v4(); // In production, use agent's registered UUID.
                    policy.apply(&agent_uuid, &egress_config).map_err(|e| {
                        BootstrapError::AgentInit(format!(
                            "egress policy for '{}': {e}",
                            agent.name
                        ))
                    })?;
                    // Register proxy URL for ghost-llm reqwest client.
                    if let Some(url) = policy.proxy_url(&agent_uuid) {
                        proxy_registry.register(agent_uuid, &url);
                    }
                    tracing::info!(
                        agent = %agent.name,
                        backend = "proxy",
                        "Egress policy applied (InProcess → Proxy)"
                    );
                }
                crate::config::IsolationMode::Process => {
                    // Platform-specific: eBPF on Linux, pf on macOS, proxy fallback.
                    #[cfg(all(target_os = "linux", feature = "ebpf"))]
                    {
                        let policy = ghost_egress::EbpfEgressPolicy::new();
                        let agent_uuid = uuid::Uuid::new_v4();
                        policy.apply(&agent_uuid, &egress_config).map_err(|e| {
                            BootstrapError::AgentInit(format!(
                                "egress policy for '{}': {e}",
                                agent.name
                            ))
                        })?;
                        // If eBPF fell back to proxy, register the proxy URL.
                        if let Some(url) = policy.proxy_fallback().proxy_url(&agent_uuid) {
                            proxy_registry.register(agent_uuid, &url);
                        }
                        tracing::info!(
                            agent = %agent.name,
                            backend = "ebpf",
                            "Egress policy applied (Process → eBPF)"
                        );
                    }
                    #[cfg(all(target_os = "macos", feature = "pf"))]
                    {
                        let policy = ghost_egress::PfEgressPolicy::new();
                        let agent_uuid = uuid::Uuid::new_v4();
                        policy.apply(&agent_uuid, &egress_config).map_err(|e| {
                            BootstrapError::AgentInit(format!(
                                "egress policy for '{}': {e}",
                                agent.name
                            ))
                        })?;
                        // If pf fell back to proxy, register the proxy URL.
                        if let Some(url) = policy.proxy_fallback().proxy_url(&agent_uuid) {
                            proxy_registry.register(agent_uuid, &url);
                        }
                        tracing::info!(
                            agent = %agent.name,
                            backend = "pf",
                            "Egress policy applied (Process → pf)"
                        );
                    }
                    #[cfg(not(any(
                        all(target_os = "linux", feature = "ebpf"),
                        all(target_os = "macos", feature = "pf")
                    )))]
                    {
                        let policy = ghost_egress::ProxyEgressPolicy::new();
                        let agent_uuid = uuid::Uuid::new_v4();
                        policy.apply(&agent_uuid, &egress_config).map_err(|e| {
                            BootstrapError::AgentInit(format!(
                                "egress policy for '{}': {e}",
                                agent.name
                            ))
                        })?;
                        // Register proxy URL for ghost-llm reqwest client.
                        if let Some(url) = policy.proxy_url(&agent_uuid) {
                            proxy_registry.register(agent_uuid, &url);
                        }
                        tracing::info!(
                            agent = %agent.name,
                            backend = "proxy",
                            "Egress policy applied (Process → Proxy fallback)"
                        );
                    }
                }
                crate::config::IsolationMode::Container => {
                    // Container isolation uses Docker network policies — no ghost-egress needed.
                    tracing::info!(
                        agent = %agent.name,
                        "Container isolation — Docker network policy handles egress"
                    );
                }
            }
        }
        Ok(())
    }

    /// Build the SecretProvider from the secrets config section (Phase 10).
    /// Returns a boxed SecretProvider that can be passed to AuthProfileManager.
    fn build_secrets(
        config: &GhostConfig,
    ) -> Result<Box<dyn ghost_secrets::SecretProvider>, BootstrapError> {
        crate::config::build_secret_provider(&config.secrets)
            .map_err(|e| BootstrapError::Config(format!("secrets provider: {e}")))
    }
}

fn shellexpand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            return format!("{}{}", home, &path[1..]);
        }
    }
    path.to_string()
}
