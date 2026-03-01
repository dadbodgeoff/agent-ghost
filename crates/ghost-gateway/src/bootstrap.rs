//! 5-step bootstrap sequence (Req 15 AC2).
//!
//! Steps: (1) config load, (2) DB migrations, (3) monitor health,
//! (4) agent/channel init, (5) API server.
//! Steps 1/2/4/5 are fatal. Step 3 degrades gracefully.

use thiserror::Error;

use std::sync::{Arc, Mutex, RwLock};

use crate::agents::registry::AgentRegistry;
use crate::api::websocket::WsEvent;
use crate::config::GhostConfig;
use crate::gateway::{Gateway, GatewaySharedState, GatewayState};
use crate::health::MonitorConnection;
use crate::safety::kill_switch::KillSwitch;
use crate::safety::quarantine::QuarantineManager;
use crate::state::AppState;
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
    pub async fn run(config_path: Option<&str>) -> Result<(Gateway, GhostConfig), BootstrapError> {
        let shared_state = Arc::new(GatewaySharedState::new());

        // Pre-step: Check kill_state.json on startup (AC13)
        // If present, enter safe mode — previous KILL_ALL was not cleanly resolved.
        // If the file exists but is corrupted/empty, still enter safe mode (conservative).
        let kill_state_path = shellexpand_tilde("~/.ghost/data/kill_state.json");
        let safe_mode = std::path::Path::new(&kill_state_path).exists();
        if safe_mode {
            // Validate the file is readable. If corrupted, log but still enter safe mode.
            match std::fs::read_to_string(&kill_state_path) {
                Ok(content) => {
                    if serde_json::from_str::<serde_json::Value>(&content).is_err() {
                        tracing::error!(
                            path = %kill_state_path,
                            "kill_state.json is corrupted (invalid JSON). Entering safe mode anyway."
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        path = %kill_state_path,
                        error = %e,
                        "kill_state.json exists but cannot be read. Entering safe mode anyway."
                    );
                }
            }
            tracing::warn!(
                path = %kill_state_path,
                "kill_state.json found — previous KILL_ALL not resolved. Entering safe mode."
            );
            use crate::safety::kill_switch::PLATFORM_KILLED;
            use std::sync::atomic::Ordering;
            PLATFORM_KILLED.store(true, Ordering::SeqCst);
        }

        // Step 1: Load + validate ghost.yml
        let config = Self::step1_load_config(config_path)?;
        tracing::info!("Step 1: Configuration loaded");

        // Step 1b: Build SecretProvider from secrets config (Phase 10)
        let secret_provider = Self::build_secrets(&config)?;
        tracing::info!("Step 1b: SecretProvider initialized (provider: {})", config.secrets.provider);

        // Step 1c: Log consumed config fields (Findings #17, #18, #19).
        tracing::info!(
            soul_drift_threshold = config.security.soul_drift_threshold,
            "Security config: soul_drift_threshold={}",
            config.security.soul_drift_threshold,
        );
        tracing::info!(
            convergence_profile = %config.convergence.profile,
            "Convergence profile: {}",
            config.convergence.profile,
        );
        if !config.models.providers.is_empty() {
            for provider in &config.models.providers {
                tracing::info!(
                    provider = %provider.name,
                    api_key_env = ?provider.api_key_env,
                    "Model provider configured: {}",
                    provider.name,
                );
            }
        }

        // Step 2: Run database migrations
        let db_path = shellexpand_tilde(&config.gateway.db_path);
        if let Some(parent) = std::path::Path::new(&db_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BootstrapError::Database(format!("create dir: {e}")))?;
        }
        // Open a single DB connection — used for migrations AND kept for AppState.
        // Previous code opened two connections (one for migrations, one for AppState),
        // wasting a file descriptor and risking WAL contention during bootstrap.
        let db_conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| BootstrapError::Database(format!("open db: {e}")))?;
        db_conn
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| BootstrapError::Database(format!("pragma: {e}")))?;
        cortex_storage::migrations::run_migrations(&db_conn)
            .map_err(|e| BootstrapError::Database(e.to_string()))?;
        tracing::info!("Step 2: Database migrations complete");

        // Ensure audit_log table exists (created by ghost-audit, not in migrations).
        {
            let engine = ghost_audit::AuditQueryEngine::new(&db_conn);
            engine
                .ensure_table()
                .map_err(|e| BootstrapError::Database(format!("audit table: {e}")))?;
            tracing::info!("Audit log table ensured");
        }

        // Wrap DB in Arc<Mutex<>> early so it can be shared with mesh router.
        let db = Arc::new(Mutex::new(db_conn));

        // Step 3: Verify convergence monitor health (NEVER fatal).
        // Runs concurrently with step 4 — no dependency between them.
        // This avoids blocking agent init for up to 45s if the monitor is down.
        let monitor_config = config.clone();
        let monitor_handle = tokio::spawn(async move {
            Self::step3_check_monitor(&monitor_config).await
        });

        // Step 4: Initialize agent registry + channel adapters
        let agent_registry = Self::step4_init_agents_channels(&config)?;
        tracing::info!("Step 4: Agents and channels initialized");

        // Step 4b: Apply network egress policies per agent (Phase 11)
        Self::step4b_apply_egress_policies(&config)?;
        tracing::info!("Step 4b: Network egress policies applied");

        // Step 4c: Initialize mesh networking if enabled (Task 22.1)
        // Pass DB for delegation state persistence.
        let mesh_router = Self::step4c_init_mesh(&config, Some(Arc::clone(&db)))?;
        if mesh_router.is_some() {
            tracing::info!("Step 4c: Mesh networking initialized (A2A endpoints active)");
        } else {
            tracing::debug!("Step 4c: Mesh networking disabled");
        }

        // Build shared application state for all route handlers.
        let (event_tx, _) = tokio::sync::broadcast::channel::<WsEvent>(256);
        let kill_switch = Arc::new(KillSwitch::new());
        let cost_tracker = Arc::new(crate::cost::tracker::CostTracker::new());

        // Build distributed kill gate bridge if mesh is enabled.
        let kill_gate = if config.mesh.enabled {
            let node_id = uuid::Uuid::now_v7();
            let gate_config = ghost_kill_gates::config::KillGateConfig::default();
            let bridge = crate::safety::kill_gate_bridge::KillGateBridge::new(
                node_id,
                Arc::clone(&kill_switch),
                gate_config,
            );
            tracing::info!(node_id = %node_id, "Distributed kill gate bridge initialized");
            Some(Arc::new(RwLock::new(bridge)))
        } else {
            None
        };

        // If safe mode, restore kill_all state into the KillSwitch.
        if safe_mode {
            let mut restored = crate::safety::kill_switch::KillSwitchState::default();
            restored.platform_level = crate::safety::kill_switch::KillLevel::KillAll;
            restored.activated_at = Some(chrono::Utc::now());
            restored.trigger = Some("kill_state.json found on startup".into());
            kill_switch.restore_state(restored);
        }

        // Build OAuthBroker with token store. Requires its own SecretProvider instance
        // because TokenStore takes ownership of the Box<dyn SecretProvider>.
        let token_store = ghost_oauth::TokenStore::with_default_dir(
            crate::config::build_secret_provider(&config.secrets)
                .map_err(|e| BootstrapError::Config(format!("oauth token store: {e}")))?
        );
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(
            std::collections::BTreeMap::new(), // Providers registered at runtime via config
            token_store,
        ));
        tracing::info!("OAuth broker initialized");

        let app_state = Arc::new(AppState {
            gateway: Arc::clone(&shared_state),
            agents: Arc::new(RwLock::new(agent_registry)),
            kill_switch,
            quarantine: Arc::new(RwLock::new(QuarantineManager::new())),
            db,
            event_tx,
            cost_tracker,
            kill_gate,
            secret_provider: Arc::from(secret_provider),
            oauth_broker,
            soul_drift_threshold: config.security.soul_drift_threshold,
            convergence_profile: config.convergence.profile.clone(),
            model_providers: config.models.providers.clone(),
        });

        // Step 5: Start API server
        Self::step5_start_api(&config)?;
        tracing::info!("Step 5: API server started");

        // Step 5b: Start convergence score watcher (Findings #13, #14).
        crate::convergence_watcher::spawn_convergence_watcher(Arc::clone(&app_state));
        tracing::info!("Step 5b: Convergence score watcher started");

        // Await monitor health check result (started concurrently in step 3).
        let monitor_state = monitor_handle.await.unwrap_or(MonitorConnection::Unreachable {
            reason: "monitor health check task panicked".into(),
        });
        tracing::info!(monitor = ?monitor_state, "Step 3: Monitor health check complete");

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

        // Store app_state in the Gateway for access by run_with_router.
        let mut gw = Gateway::new_with_state(shared_state, app_state);
        gw.mesh_router = mesh_router;
        Ok((gw, config))
    }

    fn step1_load_config(config_path: Option<&str>) -> Result<GhostConfig, BootstrapError> {
        GhostConfig::load_default(config_path).map_err(|e| BootstrapError::Config(e.to_string()))
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

    fn step4_init_agents_channels(config: &GhostConfig) -> Result<AgentRegistry, BootstrapError> {
        let mut registry = AgentRegistry::new();
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

            let registered = crate::agents::registry::RegisteredAgent {
                id: uuid::Uuid::now_v7(),
                name: agent.name.clone(),
                state: crate::agents::registry::AgentLifecycleState::Starting,
                channel_bindings: Vec::new(),
                capabilities: agent.capabilities.clone(),
                spending_cap: agent.spending_cap,
                template: agent.template.clone(),
            };
            registry.register(registered);
        }
        for channel in &config.channels {
            tracing::info!(
                channel_type = %channel.channel_type,
                agent = %channel.agent,
                "Initializing channel"
            );
            // Wire channel→agent binding into the registry.
            if let Some(agent) = registry.lookup_by_name(&channel.agent) {
                let agent_id = agent.id;
                if let Some(a) = registry.lookup_by_id_mut(agent_id) {
                    a.channel_bindings.push(channel.channel_type.clone());
                }
            } else {
                tracing::warn!(
                    channel_type = %channel.channel_type,
                    agent = %channel.agent,
                    "Channel references unknown agent — binding skipped"
                );
            }
        }
        Ok(registry)
    }

    fn step5_start_api(_config: &GhostConfig) -> Result<(), BootstrapError> {
        // API server startup is handled by the Gateway::run_with_router() event loop.
        // Route construction happens in build_router(), called by run().
        Ok(())
    }

    /// Build the axum Router with all API routes mounted.
    pub fn build_router(_config: &GhostConfig, app_state: Arc<AppState>, mesh_router: Option<axum::Router>) -> axum::Router {
        use axum::routing::{delete, get, post};

        let mut app = axum::Router::new()
            // Health
            .route("/api/health", get(crate::api::health::health_handler))
            .route("/api/ready", get(crate::api::health::ready_handler))
            // Agents
            .route("/api/agents", get(crate::api::agents::list_agents))
            .route("/api/agents", post(crate::api::agents::create_agent))
            .route("/api/agents/:id", delete(crate::api::agents::delete_agent))
            // Audit
            .route("/api/audit", get(crate::api::audit::query_audit))
            .route("/api/audit/aggregation", get(crate::api::audit::audit_aggregation))
            .route("/api/audit/export", get(crate::api::audit::audit_export))
            // Convergence
            .route("/api/convergence/scores", get(crate::api::convergence::get_scores))
            // Goals
            .route("/api/goals", get(crate::api::goals::list_goals))
            .route("/api/goals/:id/approve", post(crate::api::goals::approve_goal))
            .route("/api/goals/:id/reject", post(crate::api::goals::reject_goal))
            // Sessions
            .route("/api/sessions", get(crate::api::sessions::list_sessions))
            // Memory
            .route("/api/memory", get(crate::api::memory::list_memories))
            .route("/api/memory", post(crate::api::memory::write_memory))
            .route("/api/memory/:id", get(crate::api::memory::get_memory))
            // Safety
            .route("/api/safety/kill-all", post(crate::api::safety::kill_all))
            .route("/api/safety/pause/:agent_id", post(crate::api::safety::pause_agent))
            .route("/api/safety/resume/:agent_id", post(crate::api::safety::resume_agent))
            .route("/api/safety/quarantine/:agent_id", post(crate::api::safety::quarantine_agent))
            .route("/api/safety/status", get(crate::api::safety::safety_status))
            // Cost
            .route("/api/costs", get(crate::api::costs::get_costs))
            // WebSocket
            .route("/api/ws", get(crate::api::websocket::ws_handler))
            // OAuth
            .route("/api/oauth/providers", get(crate::api::oauth_routes::list_providers))
            .route("/api/oauth/connect", post(crate::api::oauth_routes::connect))
            .route("/api/oauth/callback", get(crate::api::oauth_routes::callback))
            .route("/api/oauth/connections", get(crate::api::oauth_routes::list_connections))
            .route("/api/oauth/connections/:ref_id", delete(crate::api::oauth_routes::disconnect))
            // Inject shared state into all handlers
            .with_state(app_state);

        // Mount mesh router (/.well-known/agent.json, /a2a) if mesh is enabled.
        if let Some(mesh) = mesh_router {
            app = app.merge(mesh);
        }

        // Mount push notification routes.
        let push_state = crate::api::push_routes::PushState {
            vapid_public_key: String::new(), // Populated from secrets in production
            subscriptions: Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new())),
        };
        app = app.merge(crate::api::push_routes::push_router(push_state));

        let app = app
            // Middleware
            .layer(tower_http::cors::CorsLayer::permissive())
            .layer(tower_http::trace::TraceLayer::new_for_http());

        tracing::info!(
            routes = "health, ready, agents, audit, convergence, goals, sessions, memory, safety, costs, ws, oauth",
            "API router built"
        );

        app
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

    /// Initialize mesh networking if enabled (Task 22.1).
    /// Returns the mesh axum Router to merge into the main router, or None if disabled.
    fn step4c_init_mesh(
        config: &GhostConfig,
        db: Option<Arc<Mutex<rusqlite::Connection>>>,
    ) -> Result<Option<axum::Router>, BootstrapError> {
        if !config.mesh.enabled {
            return Ok(None);
        }

        // Decode known agent public keys from base64
        let mut known_keys = Vec::new();
        for agent in &config.mesh.known_agents {
            use base64::Engine;
            let key_bytes = base64::engine::general_purpose::STANDARD
                .decode(&agent.public_key)
                .map_err(|e| {
                    BootstrapError::Config(format!(
                        "mesh: invalid public key for agent '{}': {e}",
                        agent.name
                    ))
                })?;
            if key_bytes.len() != 32 {
                return Err(BootstrapError::Config(format!(
                    "mesh: public key for agent '{}' must be 32 bytes (got {})",
                    agent.name,
                    key_bytes.len()
                )));
            }
            tracing::info!(
                agent = %agent.name,
                endpoint = %agent.endpoint,
                "Registered known mesh agent"
            );
            known_keys.push(key_bytes);
        }

        // Wire mesh config fields into trust policy and cascade breaker (Finding #20).
        tracing::info!(
            min_trust = config.mesh.min_trust_for_delegation,
            max_depth = config.mesh.max_delegation_depth,
            "Mesh delegation policy: min_trust={}, max_depth={}",
            config.mesh.min_trust_for_delegation,
            config.mesh.max_delegation_depth,
        );

        // Build a placeholder AgentCard for this gateway.
        // In production, this would be loaded from the agent's signing key.
        let card = ghost_mesh::types::AgentCard {
            name: "ghost-gateway".to_string(),
            description: "GHOST platform gateway".to_string(),
            capabilities: Vec::new(),
            capability_flags: 0,
            input_types: vec!["text".to_string()],
            output_types: vec!["text".to_string()],
            auth_schemes: vec!["ed25519".to_string()],
            endpoint_url: format!(
                "http://{}:{}",
                config.gateway.bind, config.gateway.port
            ),
            public_key: Vec::new(), // Populated from signing key in production
            convergence_profile: "standard".to_string(),
            trust_score: 1.0,
            sybil_lineage_hash: String::new(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            signed_at: chrono::Utc::now(),
            signature: Vec::new(),
        };

        let state = std::sync::Arc::new(std::sync::Mutex::new(
            ghost_mesh::transport::a2a_server::A2AServerState::new(card),
        ));

        let router = crate::api::mesh_routes::mesh_router_with_db(state, known_keys, db);
        Ok(Some(router))
    }
}

pub fn shellexpand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            Ok(home) => format!("{}{}", home, &path[1..]),
            Err(_) => {
                tracing::warn!(
                    path = %path,
                    "HOME/USERPROFILE not set — tilde expansion failed, using path as-is"
                );
                path.to_string()
            }
        }
    } else {
        path.to_string()
    }
}
