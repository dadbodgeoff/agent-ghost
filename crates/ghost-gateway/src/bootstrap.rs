//! 5-step bootstrap sequence (Req 15 AC2).
//!
//! Steps: (1) config load, (2) DB migrations, (3) monitor health,
//! (4) agent/channel init, (5) API server.
//! Steps 1/2/4/5 are fatal. Step 3 degrades gracefully.

use thiserror::Error;

use std::sync::{Arc, RwLock};

use crate::agents::registry::AgentRegistry;
use crate::api::websocket::{EventReplayBuffer, WsEnvelope};
use crate::config::GhostConfig;
use crate::gateway::{GatewaySharedState, GatewayState};
use crate::health::MonitorConnection;
use crate::runtime::GatewayRuntime;
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
    #[tracing::instrument(skip(config_path), fields(otel.kind = "internal"))]
    pub async fn run(
        config_path: Option<&str>,
    ) -> Result<(GatewayRuntime, GhostConfig), BootstrapError> {
        let shared_state = Arc::new(GatewaySharedState::new());

        let kill_state_path = crate::api::safety::persisted_safety_state_path();
        let restored_safety_state = if std::path::Path::new(&kill_state_path).exists() {
            match crate::api::safety::load_persisted_runtime_safety_state(std::path::Path::new(
                &kill_state_path,
            )) {
                Ok(Some(state)) => {
                    tracing::warn!(
                        path = %kill_state_path,
                        "Persisted safety state found on startup. Restoring kill-switch state."
                    );
                    Some(state)
                }
                Ok(None) => None,
                Err(error) => {
                    tracing::error!(
                        path = %kill_state_path,
                        error = %error,
                        "Persisted safety state is unreadable. Entering kill-all safe mode."
                    );
                    let mut fallback = crate::safety::kill_switch::KillSwitchState::default();
                    fallback.platform_level = crate::safety::kill_switch::KillLevel::KillAll;
                    fallback.activated_at = Some(chrono::Utc::now());
                    fallback.trigger = Some("persisted safety state unreadable on startup".into());
                    Some(crate::api::safety::RestoredSafetyRuntimeState {
                        state: fallback,
                        distributed_gate: None,
                    })
                }
            }
        } else {
            None
        };

        // Step 1: Load + validate ghost.yml
        let config = Self::step1_load_config(config_path)?;
        tracing::info!("Step 1: Configuration loaded");

        // Step 1.0: Validate production auth requirements (WP0-A).
        // If GHOST_ENV=production and no auth is configured, this exits immediately.
        crate::api::auth::AuthConfig::from_env().validate_production();

        // Export gateway port for subsystems that need it (e.g. OAuth redirect URI).
        // Uses the thread-safe key store instead of std::env::set_var (which is
        // unsound in multi-threaded contexts since Rust 1.66).
        crate::state::set_api_key("GHOST_GATEWAY_PORT", &config.gateway.port.to_string());

        // Step 1a: Pre-launch check — detect stale gateway processes.
        let pid_action = crate::pid::pre_launch_check(config.gateway.port).await;
        match &pid_action {
            crate::pid::PreLaunchAction::ProceedWithStartup => {
                tracing::info!("Step 1a: No existing gateway — proceeding");
            }
            crate::pid::PreLaunchAction::ReuseExisting { url } => {
                return Err(BootstrapError::Config(format!(
                    "Gateway already running at {url}. Kill it first or use a different port."
                )));
            }
            crate::pid::PreLaunchAction::CleanedStaleProcess { old_pid } => {
                tracing::warn!(
                    old_pid,
                    "Step 1a: Cleaned stale PID file (process was dead)"
                );
            }
            crate::pid::PreLaunchAction::KilledUnresponsive { old_pid } => {
                tracing::warn!(old_pid, "Step 1a: Killed unresponsive previous gateway");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }

        // Step 1b: Build SecretProvider from secrets config (Phase 10)
        let secret_provider = Self::build_secrets(&config)?;
        tracing::info!(
            "Step 1b: SecretProvider initialized (provider: {})",
            config.secrets.provider
        );

        // Step 1b.1: Hydrate provider API keys from secret_provider into env vars.
        // This ensures keys saved through the dashboard UI survive gateway restarts.
        for provider in &config.models.providers {
            if matches!(provider.name.as_str(), "ollama") {
                continue; // No API key needed for local providers.
            }
            let env_name =
                provider
                    .api_key_env
                    .as_deref()
                    .unwrap_or(match provider.name.as_str() {
                        "anthropic" => "ANTHROPIC_API_KEY",
                        "openai" => "OPENAI_API_KEY",
                        "gemini" => "GEMINI_API_KEY",
                        _ => continue,
                    });
            // Only hydrate if key is not already set (env/store takes precedence).
            if crate::state::get_api_key(env_name).is_some() {
                continue;
            }
            if let Ok(secret) = secret_provider.get_secret(env_name) {
                use ghost_secrets::ExposeSecret;
                let value = secret.expose_secret().to_string();
                if !value.is_empty() {
                    crate::state::set_api_key(env_name, &value);
                    tracing::info!(env_name = %env_name, "Hydrated provider API key from secret store");
                }
            }
        }

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

        // Step 2: Verify database readiness (never run schema DDL here).
        let db_path = std::path::PathBuf::from(shellexpand_tilde(&config.gateway.db_path));
        let db = Self::step2_open_verified_db(&db_path)?;
        tracing::info!(path = %db_path.display(), "Step 2: Database schema verified");

        // Step 3: Verify convergence monitor health (NEVER fatal).
        // Skipped entirely when monitor is disabled in config.
        let monitor_handle = if config.convergence.monitor.enabled {
            // Runs concurrently with step 4 — no dependency between them.
            let monitor_config = config.clone();
            Some(tokio::spawn(async move {
                Self::step3_check_monitor(&monitor_config).await
            }))
        } else {
            tracing::info!("Step 3: Convergence monitor disabled — skipping health check");
            None
        };

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
        // WP7-B: Configurable broadcast capacity (default 1024).
        let (event_tx, _) =
            tokio::sync::broadcast::channel::<WsEnvelope>(config.gateway.ws_broadcast_capacity);
        // WP7-C: Configurable replay buffer size (default 1000).
        let replay_buffer = Arc::new(EventReplayBuffer::new(config.gateway.ws_replay_buffer_size));
        let kill_switch = Arc::new(KillSwitch::new());
        let cost_tracker = Arc::new(crate::cost::tracker::CostTracker::new());

        // WP4-A: Restore cost state from previous run (same day only).
        if let Ok(conn) = db.read() {
            if let Err(e) = cost_tracker.restore(&conn) {
                tracing::warn!(error = %e, "failed to restore cost tracker state — starting fresh");
            }
        }

        // Build distributed kill gate bridge only when the feature gate is on.
        let restored_gate_state = restored_safety_state
            .as_ref()
            .and_then(|state| state.distributed_gate.clone());
        let kill_gate = if crate::runtime_status::should_enable_distributed_kill(
            config.mesh.enabled,
            config.mesh.distributed_kill_enabled,
        ) {
            let gate_config = ghost_kill_gates::config::KillGateConfig::default();
            let bridge = if let Some(persisted) = restored_gate_state {
                crate::safety::kill_gate_bridge::KillGateBridge::from_persisted_state(
                    Arc::clone(&kill_switch),
                    gate_config,
                    persisted,
                )
            } else {
                crate::safety::kill_gate_bridge::KillGateBridge::new(
                    uuid::Uuid::now_v7(),
                    Arc::clone(&kill_switch),
                    gate_config,
                )
            };
            tracing::info!(
                node_id = %bridge.node_id(),
                "Distributed kill gate bridge initialized"
            );
            Some(Arc::new(RwLock::new(bridge)))
        } else {
            if config.mesh.enabled {
                tracing::info!(
                    "Distributed kill remains feature-gated and is disabled in this milestone"
                );
            }
            None
        };

        if let Some(restored) = restored_safety_state {
            kill_switch.restore_state(restored.state);
        }

        // Build OAuthBroker with token store. Requires its own SecretProvider instance
        // because TokenStore takes ownership of the Box<dyn SecretProvider>.
        let token_store = ghost_oauth::TokenStore::with_default_dir(
            crate::config::build_secret_provider(&config.secrets)
                .map_err(|e| BootstrapError::Config(format!("oauth token store: {e}")))?,
        );
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(
            std::collections::BTreeMap::new(), // Providers registered at runtime via config
            token_store,
        ));
        tracing::info!("OAuth broker initialized");

        // Initialize embedding engine (TF-IDF provider, in-memory cache).
        let embedding_engine =
            cortex_embeddings::EmbeddingEngine::new(cortex_embeddings::EmbeddingConfig::default());
        tracing::info!(
            provider = "tfidf",
            dimensions = 128,
            "Embedding engine initialized"
        );

        let compiled_skills =
            crate::skill_catalog::definitions::build_compiled_skill_definitions(&config);
        let pc_control_circuit_breaker = Arc::clone(&compiled_skills.pc_control_circuit_breaker);
        let skill_catalog = Arc::new(
            crate::skill_catalog::service::SkillCatalogService::new(
                compiled_skills.definitions,
                Arc::clone(&db),
                config.external_skills.clone(),
            )
            .await
            .map_err(|e| BootstrapError::Database(format!("skill catalog: {e}")))?,
        );
        if config.external_skills.enabled && config.external_skills.rescan_on_boot {
            skill_catalog
                .rescan_external_skills("bootstrap")
                .await
                .map_err(|e| BootstrapError::Database(format!("skill ingest: {e}")))?;
        }
        let skill_names = skill_catalog
            .list_skills()
            .map_err(|e| BootstrapError::Database(format!("skill catalog: {e}")))?
            .installed
            .into_iter()
            .map(|skill| skill.name)
            .collect::<Vec<_>>();
        tracing::info!(
            count = skill_names.len(),
            skills = ?skill_names,
            "Skill catalog initialized"
        );

        let app_state = Arc::new(AppState {
            gateway: Arc::clone(&shared_state),
            agents: Arc::new(RwLock::new(agent_registry)),
            kill_switch,
            quarantine: Arc::new(RwLock::new(QuarantineManager::new())),
            db,
            event_tx,
            replay_buffer,
            cost_tracker,
            kill_gate,
            secret_provider: Arc::from(secret_provider),
            oauth_broker,
            soul_drift_threshold: config.security.soul_drift_threshold,
            convergence_profile: config.convergence.profile.clone(),
            model_providers: config.models.providers.clone(),
            default_model_provider: config.models.default_provider.clone(),
            pc_control_circuit_breaker,
            websocket_auth_tickets: Arc::new(dashmap::DashMap::new()),
            ws_ticket_auth_only: config.gateway.ws_ticket_auth_only,
            tools_config: config.tools.clone(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            safety_cooldown: Arc::new(crate::api::rate_limit::SafetyCooldown::new()),
            monitor_address: config.convergence.monitor.address.clone(),
            monitor_enabled: config.convergence.monitor.enabled,
            monitor_block_on_degraded: config.convergence.monitor.block_on_degraded,
            convergence_state_stale_after: std::time::Duration::from_secs(
                config.convergence.monitor.stale_after_secs,
            ),
            monitor_healthy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            distributed_kill_enabled: config.mesh.distributed_kill_enabled,
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            skill_catalog,
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: config.gateway.session_ttl_days,
        });

        if app_state.kill_gate.is_some() {
            crate::api::safety::persist_current_safety_state(&app_state).map_err(|error| {
                BootstrapError::ApiServer(format!("persist safety state: {error}"))
            })?;
        }

        // Step 5: Start API server
        Self::step5_start_api(&config)?;
        tracing::info!("Step 5: API server started");

        // Build the GatewayRuntime — single owner of the process lifecycle.
        let mut runtime = GatewayRuntime::new(Arc::clone(&shared_state), Arc::clone(&app_state));
        runtime.mesh_router = mesh_router;

        // Step 5b: Spawn background tasks through the runtime's TaskTracker.
        // Every task goes through spawn_tracked() — guaranteed cancellation + await.
        runtime.spawn_tracked(
            "wal_checkpoint",
            crate::db_pool::wal_checkpoint_task(Arc::clone(&app_state.db)),
        );
        tracing::info!("Step 5b: WAL checkpoint task started (every 5 min, tracked)");

        runtime.spawn_tracked(
            "convergence_watcher",
            crate::convergence_watcher::convergence_watcher_task(Arc::clone(&app_state)),
        );
        tracing::info!("Step 5b: Convergence score watcher started (tracked)");

        runtime.spawn_tracked(
            "config_watcher",
            crate::config_watcher::config_watcher_task(Arc::clone(&app_state)),
        );
        tracing::info!("Step 5c: Config file watcher started (tracked)");

        runtime.spawn_tracked(
            "backup_scheduler",
            crate::backup_scheduler::backup_scheduler_task(Arc::clone(&app_state)),
        );
        tracing::info!("Step 5d: Backup scheduler started (tracked)");

        // WP4-A: Periodic cost tracker persistence (every 5 minutes).
        {
            let ct = Arc::clone(&app_state.cost_tracker);
            let db_for_cost = Arc::clone(&app_state.db);
            runtime.spawn_tracked("cost_persist", async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    // Snapshot first (no DB lock needed), then acquire writer briefly.
                    let snap = ct.snapshot();
                    let conn = db_for_cost.write().await;
                    if let Err(e) =
                        crate::cost::tracker::CostTracker::persist_snapshot(&snap, &conn)
                    {
                        tracing::warn!(error = %e, "periodic cost persistence failed");
                    }
                    drop(conn); // Release writer lock immediately after persist.
                }
            });
            tracing::info!("Step 5e: Cost persistence task started (every 5 min, tracked)");
        }

        // WP9-F: Pre-flight LLM provider health checks.
        // Run health checks for all configured providers concurrently.
        // Failures are logged as warnings — they don't block startup.
        {
            let providers = &app_state.model_providers;
            if !providers.is_empty() {
                tracing::info!(
                    count = providers.len(),
                    "Step 5e: Running LLM provider health checks"
                );
                for pc in providers {
                    let provider: Option<Arc<dyn ghost_llm::provider::LLMProvider>> =
                        match pc.name.as_str() {
                            "anthropic" => {
                                let key_env =
                                    pc.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY");
                                let key = crate::state::get_api_key(key_env).unwrap_or_default();
                                Some(Arc::new(ghost_llm::provider::AnthropicProvider {
                                    model: pc
                                        .model
                                        .clone()
                                        .unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
                                    api_key: std::sync::RwLock::new(key),
                                }))
                            }
                            "openai" => {
                                let key_env = pc.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
                                let key = crate::state::get_api_key(key_env).unwrap_or_default();
                                Some(Arc::new(ghost_llm::provider::OpenAIProvider {
                                    model: pc.model.clone().unwrap_or_else(|| "gpt-4o".into()),
                                    api_key: std::sync::RwLock::new(key),
                                }))
                            }
                            "gemini" => {
                                let key_env = pc.api_key_env.as_deref().unwrap_or("GEMINI_API_KEY");
                                let key = crate::state::get_api_key(key_env).unwrap_or_default();
                                Some(Arc::new(ghost_llm::provider::GeminiProvider {
                                    model: pc
                                        .model
                                        .clone()
                                        .unwrap_or_else(|| "gemini-2.0-flash".into()),
                                    api_key: std::sync::RwLock::new(key),
                                }))
                            }
                            "ollama" => {
                                let base_url = pc
                                    .base_url
                                    .clone()
                                    .unwrap_or_else(|| "http://localhost:11434".into());
                                let model = pc.model.clone().unwrap_or_else(|| "llama3.1".into());
                                Some(Arc::new(ghost_llm::provider::OllamaProvider {
                                    model,
                                    base_url,
                                }))
                            }
                            _ => None,
                        };
                    if let Some(p) = provider {
                        let name = pc.name.clone();
                        match p.health_check().await {
                            Ok(()) => {
                                tracing::info!(provider = %name, "LLM provider health check passed")
                            }
                            Err(e) => {
                                tracing::warn!(provider = %name, error = %e, "LLM provider health check failed — provider may be unreachable or API key invalid");
                                crate::api::websocket::broadcast_event(
                                    &app_state,
                                    crate::api::websocket::WsEvent::SystemWarning {
                                        message: format!(
                                            "LLM provider '{name}' health check failed: {e}"
                                        ),
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }

        // WP4-C: Pre-flight check — warn if no backup passphrase is configured.
        {
            let has_passphrase = std::env::var("GHOST_BACKUP_PASSPHRASE")
                .map(|p| !p.is_empty())
                .unwrap_or(false);
            if !has_passphrase {
                let key_path = shellexpand_tilde("~/.ghost/backup.key");
                let has_key_file = std::fs::read_to_string(&key_path)
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if !has_key_file {
                    tracing::warn!(
                        "No backup passphrase configured. Set GHOST_BACKUP_PASSPHRASE \
                         or create ~/.ghost/backup.key to enable encrypted backups."
                    );
                    crate::api::websocket::broadcast_event(
                        &app_state,
                        crate::api::websocket::WsEvent::SystemWarning {
                            message:
                                "No backup passphrase configured — encrypted backups are disabled"
                                    .into(),
                        },
                    );
                }
            }
        }

        // Await monitor health check result (started concurrently in step 3).
        let monitor_state = if let Some(handle) = monitor_handle {
            let state = handle.await.unwrap_or(MonitorConnection::Unreachable {
                reason: "monitor health check task panicked".into(),
            });
            tracing::info!(monitor = ?state, "Step 3: Monitor health check complete");
            state
        } else {
            // Monitor disabled — treat as unreachable but go Healthy
            MonitorConnection::Unreachable {
                reason: "monitor disabled in config".into(),
            }
        };

        // Set initial monitor health from startup check result.
        app_state.monitor_healthy.store(
            matches!(monitor_state, MonitorConnection::Connected { .. }),
            std::sync::atomic::Ordering::Relaxed,
        );

        // Transition decision — monitor disabled means go straight to Healthy.
        match (&monitor_state, config.convergence.monitor.enabled) {
            (MonitorConnection::Connected { .. }, _) => {
                shared_state
                    .transition_to(GatewayState::Healthy)
                    .map_err(|e| BootstrapError::Config(e.to_string()))?;
                tracing::info!("Gateway started. State: HEALTHY");
            }
            (MonitorConnection::Unreachable { .. }, false) => {
                shared_state
                    .transition_to(GatewayState::Healthy)
                    .map_err(|e| BootstrapError::Config(e.to_string()))?;
                tracing::info!("Gateway started. State: HEALTHY (monitor disabled)");
            }
            (MonitorConnection::Unreachable { reason }, true) => {
                shared_state
                    .transition_to(GatewayState::Degraded)
                    .map_err(|e| BootstrapError::Config(e.to_string()))?;
                tracing::warn!(
                    reason = %reason,
                    "Gateway started in DEGRADED mode. Safety floor absent."
                );
            }
        }

        // Write PID file now that we're ready to bind.
        if let Err(e) = crate::pid::write_pid_file(config.gateway.port) {
            tracing::warn!(error = %e, "Failed to write PID file — continuing anyway");
        }

        // Spawn recurring MonitorHealthChecker only if monitor is enabled.
        if config.convergence.monitor.enabled {
            use crate::gateway::GatewayState;
            use crate::health::{MonitorHealthChecker, MonitorHealthConfig, RecoveryCoordinator};
            use std::sync::atomic::Ordering;

            let health_cfg = MonitorHealthConfig {
                address: config.convergence.monitor.address.clone(),
                ..MonitorHealthConfig::default()
            };
            let mut checker = MonitorHealthChecker::new(health_cfg, Arc::clone(&shared_state));
            let monitor_flag = Arc::clone(&app_state.monitor_healthy);
            let gateway_shared = Arc::clone(&shared_state);
            let monitor_addr = config.convergence.monitor.address.clone();

            runtime.spawn_tracked("monitor_health_checker", async move {
                let mut interval = tokio::time::interval(checker.config.check_interval);
                loop {
                    interval.tick().await;
                    let ok = checker.check_once().await;
                    monitor_flag.store(ok, Ordering::Relaxed);

                    if ok && gateway_shared.current_state() == GatewayState::Degraded {
                        if gateway_shared
                            .transition_to(GatewayState::Recovering)
                            .is_ok()
                        {
                            tracing::info!("Monitor recovered — initiating recovery sequence");
                            let coord = RecoveryCoordinator {
                                shared_state: Arc::clone(&gateway_shared),
                                monitor_address: monitor_addr.clone(),
                            };
                            tokio::spawn(async move {
                                let _ = coord.attempt_recovery().await;
                            });
                        }
                    }
                }
            });

            tracing::info!(
                interval_secs = 30,
                failure_threshold = 3,
                "MonitorHealthChecker background task spawned (tracked)"
            );
        } else {
            tracing::debug!("Monitor health checker skipped (convergence.monitor.enabled = false)");
        }

        Ok((runtime, config))
    }

    fn step1_load_config(config_path: Option<&str>) -> Result<GhostConfig, BootstrapError> {
        GhostConfig::load_default(config_path).map_err(|e| BootstrapError::Config(e.to_string()))
    }

    fn step2_open_verified_db(
        db_path: &std::path::Path,
    ) -> Result<std::sync::Arc<crate::db_pool::DbPool>, BootstrapError> {
        if !db_path.exists() {
            return Err(BootstrapError::Database(format!(
                "database {} is missing; run `ghost db migrate` before startup",
                db_path.display()
            )));
        }
        cortex_storage::sqlite::ensure_maintenance_lock_absent(db_path)
            .map_err(|e| BootstrapError::Database(e.to_string()))?;

        let db = crate::db_pool::create_existing_pool(db_path.to_path_buf())
            .map_err(|e| BootstrapError::Database(format!("db pool: {e}")))?;
        {
            let read = db
                .read()
                .map_err(|e| BootstrapError::Database(format!("db read: {e}")))?;
            cortex_storage::schema_contract::require_schema_ready(&read)
                .map_err(|e| BootstrapError::Database(e.to_string()))?;
        }

        Ok(db)
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
            // Load-time key presence check only. Do not claim runtime
            // registration here unless the consuming subsystem actually wires
            // the verifying key into its acceptance path.
            let key_path =
                shellexpand_tilde(&format!("~/.ghost/agents/{}/keys/agent.pub", agent.name));
            if std::path::Path::new(&key_path).exists() {
                tracing::info!(agent = %agent.name, "Public key found");
            } else {
                tracing::warn!(
                    agent = %agent.name,
                    path = %key_path,
                    "No public key found — agent will need key generation"
                );
            }

            let registered = crate::agents::registry::RegisteredAgent {
                id: crate::agents::registry::durable_agent_id(&agent.name),
                name: agent.name.clone(),
                state: crate::agents::registry::AgentLifecycleState::Starting,
                channel_bindings: Vec::new(),
                capabilities: agent.capabilities.clone(),
                skills: agent.skills.clone(),
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
    ///
    /// Routes are organized into groups by RBAC role requirement:
    /// - **Public**: health, auth, openapi — no auth/RBAC
    /// - **Read (no extra RBAC)**: GET endpoints for agents, sessions, etc.
    /// - **Operator**: write operations (POST/PUT/DELETE on agents, sessions, studio, etc.)
    /// - **Admin**: safety, admin, provider keys, webhooks, custom safety checks
    /// - **SuperAdmin**: kill-all, data restore
    pub fn build_router(
        config: &GhostConfig,
        app_state: Arc<AppState>,
        mesh_router: Option<axum::Router>,
    ) -> axum::Router {
        let public_routes = crate::route_sets::public_routes();
        let read_routes = crate::route_sets::read_routes();
        let operator_routes = crate::route_sets::operator_routes();
        let admin_routes = crate::route_sets::admin_routes();
        let superadmin_routes = crate::route_sets::superadmin_routes();

        // Auth config — resolve once at startup, not per-request.
        let auth_config = Arc::new(crate::api::auth::AuthConfig::from_env());
        let revocation_set = Arc::new(crate::api::auth::RevocationSet::new());

        // WP0-B: Load persisted revocations from DB and attach pool for write-through.
        match app_state.db.read() {
            Ok(reader) => revocation_set.load_from_db(&reader),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load revoked tokens — starting with empty set")
            }
        }
        revocation_set.set_db(&app_state.db);

        // ── Merge all route groups ────────────────────────────────────
        let mut app = public_routes
            .merge(read_routes)
            .merge(operator_routes)
            .merge(admin_routes)
            .merge(superadmin_routes)
            .with_state(Arc::clone(&app_state));

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

        // WP6-A: Build CORS layer — config first, then env var, then dev defaults.
        let cors = Self::build_cors_layer(config);

        // Rate limiting state.
        let rate_limit_state = std::sync::Arc::new(crate::api::rate_limit::RateLimitState::new(
            Arc::clone(&app_state.db),
            config.gateway.rate_limit_scope,
        ));

        let ws_tracker = Arc::new(crate::api::websocket::WsConnectionTracker::new());

        let app = app
            // Middleware stack: last .layer() = outermost = runs first on request.
            //
            // Request order: CORS → extensions → operation_context → Trace → auth → rate_limit → handler
            // (auth BEFORE rate_limit so Claims are available for per-token limits)
            .layer(axum::middleware::from_fn(
                crate::api::rate_limit::rate_limit_middleware,
            ))
            .layer(axum::middleware::from_fn(crate::api::auth::auth_middleware))
            .layer(
                tower_http::trace::TraceLayer::new_for_http().make_span_with(
                    |req: &axum::http::Request<_>| {
                        let request_id = req
                            .headers()
                            .get(crate::api::operation_context::REQUEST_ID_HEADER)
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
                        let operation_id = req
                            .headers()
                            .get(crate::api::operation_context::OPERATION_ID_HEADER)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        tracing::info_span!(
                            "request",
                            method = %req.method(),
                            uri = %req.uri(),
                            request_id = %request_id,
                            operation_id = %operation_id,
                        )
                    },
                ),
            )
            .layer(axum::middleware::from_fn(
                crate::api::operation_context::operation_context_middleware,
            ))
            .layer(axum::Extension(rate_limit_state))
            .layer(axum::Extension(auth_config.clone()))
            .layer(axum::Extension(revocation_set.clone()))
            .layer(axum::Extension(ws_tracker))
            .layer(cors);

        tracing::info!(
            routes = "health, ready, agents, audit, convergence, goals, sessions, memory, state, integrity, workflows, studio, traces, mesh-viz, profiles, search, admin, safety, safety-checks, webhooks, skills, a2a, costs, ws, oauth, auth, openapi",
            "API router built with RBAC middleware (Task 1.12)"
        );

        app
    }

    /// Build CORS layer — WP6-A: config first, then env var, then dev defaults.
    ///
    /// Priority: ghost.yml `security.cors_origins` → `GHOST_CORS_ORIGINS` env → dev defaults.
    /// Production: must have origins configured via config or env var.
    fn build_cors_layer(config: &GhostConfig) -> tower_http::cors::CorsLayer {
        use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders};

        let gateway_port = config.gateway.port;
        let origins_env = std::env::var("GHOST_CORS_ORIGINS").ok();
        let is_production = std::env::var("GHOST_ENV")
            .map(|v| v.eq_ignore_ascii_case("production"))
            .unwrap_or(false);

        // WP6-A: Check config first, then env var, then defaults.
        let config_origins = &config.security.cors_origins;

        // T-5.1.5: In production, origins MUST be set (config or env).
        if is_production
            && config_origins.is_empty()
            && origins_env.as_ref().map(|v| v.is_empty()).unwrap_or(true)
        {
            eprintln!(
                "FATAL: GHOST_ENV=production but no CORS origins configured. \
                 Set security.cors_origins in ghost.yml or GHOST_CORS_ORIGINS env var."
            );
            std::process::exit(1);
        }

        let explicit_origins: Vec<String> = if !config_origins.is_empty() {
            // WP6-A: Use config origins.
            tracing::info!(
                count = config_origins.len(),
                "CORS origins loaded from ghost.yml"
            );
            config_origins.clone()
        } else if let Some(ref val) = origins_env {
            if !val.is_empty() {
                val.split(',').map(|s| s.trim().to_string()).collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let origins: Vec<String> = if explicit_origins.is_empty() {
            // T-5.1.5: Warn when using default dev origins.
            tracing::warn!(
                "No CORS origins configured — using default localhost origins (dev mode only)"
            );
            vec![
                format!("http://localhost:{}", gateway_port + 1),
                format!("http://localhost:{}", gateway_port),
                format!("http://127.0.0.1:{}", gateway_port + 1),
                format!("http://127.0.0.1:{}", gateway_port),
            ]
        } else {
            explicit_origins
        };

        let parsed: Vec<axum::http::HeaderValue> =
            origins.iter().filter_map(|o| o.parse().ok()).collect();

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(parsed))
            .allow_methods(AllowMethods::list([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PATCH,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ]))
            .allow_headers(AllowHeaders::list([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderName::from_static(
                    crate::api::operation_context::REQUEST_ID_HEADER,
                ),
                axum::http::HeaderName::from_static(
                    crate::api::operation_context::OPERATION_ID_HEADER,
                ),
                axum::http::HeaderName::from_static(
                    crate::api::operation_context::IDEMPOTENCY_KEY_HEADER,
                ),
                axum::http::HeaderName::from_static(crate::api::compatibility::CLIENT_NAME_HEADER),
                axum::http::HeaderName::from_static(
                    crate::api::compatibility::CLIENT_VERSION_HEADER,
                ),
                axum::http::HeaderName::from_static("x-ghost-client-id"),
                axum::http::HeaderName::from_static("x-ghost-session-epoch"),
            ]))
            .expose_headers(ExposeHeaders::list([
                axum::http::HeaderName::from_static(
                    crate::api::operation_context::REQUEST_ID_HEADER,
                ),
                axum::http::HeaderName::from_static(
                    crate::api::operation_context::OPERATION_ID_HEADER,
                ),
                axum::http::HeaderName::from_static(
                    crate::api::operation_context::IDEMPOTENCY_KEY_HEADER,
                ),
                axum::http::HeaderName::from_static(
                    crate::api::operation_context::IDEMPOTENCY_STATUS_HEADER,
                ),
            ]))
            .allow_credentials(true)
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
        db: Option<Arc<crate::db_pool::DbPool>>,
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
            endpoint_url: format!("http://{}:{}", config.gateway.bind, config.gateway.port),
            public_key: Vec::new(), // Populated from signing key in production
            convergence_profile: "standard".to_string(),
            trust_score: 1.0,
            sybil_lineage_hash: String::new(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            signed_at: chrono::Utc::now(),
            signature: Vec::new(),
            supported_task_types: vec![
                "code_review".to_string(),
                "summarization".to_string(),
                "analysis".to_string(),
            ],
            default_input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            default_output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            provider: "ghost-platform".to_string(),
            a2a_protocol_version: "0.2.0".to_string(),
        };

        let state = std::sync::Arc::new(std::sync::Mutex::new(
            ghost_mesh::transport::a2a_server::A2AServerState::new(card),
        ));

        let router = crate::api::mesh_routes::mesh_router_with_db(state, known_keys, db);
        Ok(Some(router))
    }
}

/// Resolve the GHOST home directory.
///
/// Precedence: `GHOST_HOME` env var → `~/.ghost/`.
pub fn ghost_home() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("GHOST_HOME") {
        return std::path::PathBuf::from(home);
    }
    match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(home) => std::path::PathBuf::from(home).join(".ghost"),
        Err(_) => std::path::PathBuf::from(".ghost"),
    }
}

/// Expand `~/.ghost` prefix using `GHOST_HOME`, then expand remaining `~/` with `$HOME`.
pub fn shellexpand_tilde(path: &str) -> String {
    // If the path starts with ~/.ghost, resolve via GHOST_HOME.
    if path.starts_with("~/.ghost/") || path == "~/.ghost" {
        let home = ghost_home();
        let suffix = path.strip_prefix("~/.ghost").unwrap_or("");
        return format!("{}{suffix}", home.display());
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_snapshot(conn: &rusqlite::Connection) -> Vec<(String, String, Option<String>)> {
        conn.prepare(
            "SELECT type, name, sql FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    }

    #[test]
    fn step2_verification_against_ready_db_performs_no_schema_ddl() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("bootstrap.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        let before = schema_snapshot(&conn);
        drop(conn);

        let db = GatewayBootstrap::step2_open_verified_db(&db_path).unwrap();
        drop(db);

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let after = schema_snapshot(&conn);
        assert_eq!(before, after);
    }

    #[test]
    fn step2_fails_when_maintenance_lock_is_held() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("locked.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        let _lock = cortex_storage::sqlite::acquire_maintenance_lock(&db_path).unwrap();

        match GatewayBootstrap::step2_open_verified_db(&db_path) {
            Ok(_) => panic!("expected maintenance lock failure"),
            Err(err) => assert!(err.to_string().contains("maintenance lock"), "{err}"),
        }
    }
}
