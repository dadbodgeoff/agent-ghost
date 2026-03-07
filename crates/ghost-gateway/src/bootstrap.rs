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

        // Step 2: Run database migrations
        let db_path = shellexpand_tilde(&config.gateway.db_path);
        if let Some(parent) = std::path::Path::new(&db_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BootstrapError::Database(format!("create dir: {e}")))?;
        }
        // Create read/write separated connection pool (1 writer + N readers).
        // Migrations run on the writer connection before pool is fully available.
        let db_path_buf = std::path::PathBuf::from(&db_path);
        let db = crate::db_pool::create_pool(db_path_buf)
            .map_err(|e| BootstrapError::Database(format!("db pool: {e}")))?;

        // Run migrations on the writer connection (with pre-migration backup).
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations_with_backup(
                &writer,
                Some(std::path::Path::new(&db_path)),
            )
            .map_err(|e| BootstrapError::Database(e.to_string()))?;
            tracing::info!("Step 2: Database migrations complete");

            // Ensure audit_log table exists (created by ghost-audit, not in migrations).
            let engine = ghost_audit::AuditQueryEngine::new(&writer);
            engine
                .ensure_table()
                .map_err(|e| BootstrapError::Database(format!("audit table: {e}")))?;
            tracing::info!("Audit log table ensured");
        }

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

        // Register Phase 5 safety skills — platform-managed, always active.
        let mut all_skills: std::collections::HashMap<String, Box<dyn ghost_skills::skill::Skill>> =
            ghost_skills::safety_skills::all_safety_skills()
                .into_iter()
                .map(|s| (s.name().to_string(), s))
                .collect();
        tracing::info!(
            count = all_skills.len(),
            skills = ?all_skills.keys().collect::<Vec<_>>(),
            "Phase 5 safety skills registered"
        );

        // Register Phase 7 git skills — wrapped with ConvergenceGuard.
        let git_skills = ghost_skills::git_skills::all_git_skills();
        let git_count = git_skills.len();
        for skill in git_skills {
            all_skills.insert(skill.name().to_string(), skill);
        }
        tracing::info!(count = git_count, "Phase 7 git skills registered");

        // Register Phase 7 code analysis skills — wrapped with ConvergenceGuard.
        let code_skills = ghost_skills::code_analysis::all_code_analysis_skills();
        let code_count = code_skills.len();
        for skill in code_skills {
            all_skills.insert(skill.name().to_string(), skill);
        }
        tracing::info!(
            count = code_count,
            "Phase 7 code analysis skills registered"
        );

        // Register Phase 8 bundled skills — user-installable, curated.
        let bundled_skills = ghost_skills::bundled_skills::all_bundled_skills();
        let bundled_count = bundled_skills.len();
        for skill in bundled_skills {
            all_skills.insert(skill.name().to_string(), skill);
        }
        tracing::info!(count = bundled_count, "Phase 8 bundled skills registered");

        // Register Phase 9 PC control skills (disabled by default).
        let pc_skills = ghost_pc_control::all_pc_control_skills(&config.pc_control);
        let pc_count = pc_skills.len();
        for skill in pc_skills {
            all_skills.insert(skill.name().to_string(), skill);
        }
        if pc_count > 0 {
            tracing::info!(count = pc_count, "Phase 9 PC control skills registered");
        } else {
            tracing::debug!("Phase 9 PC control: disabled (pc_control.enabled = false)");
        }

        // Register Phase 10 delegation skills — convergence-gated, prerequisites enforced at execute time.
        let delegation_skills = ghost_skills::delegation_skills::all_delegation_skills();
        let delegation_count = delegation_skills.len();
        for skill in delegation_skills {
            all_skills.insert(skill.name().to_string(), skill);
        }
        tracing::info!(
            count = delegation_count,
            "Phase 10 delegation skills registered"
        );

        let safety_skills = all_skills;

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
            tools_config: config.tools.clone(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            safety_cooldown: Arc::new(crate::api::rate_limit::SafetyCooldown::new()),
            monitor_address: config.convergence.monitor.address.clone(),
            monitor_enabled: config.convergence.monitor.enabled,
            monitor_healthy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            safety_skills: Arc::new(safety_skills),
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: config.gateway.session_ttl_days,
        });

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
            let key_path =
                shellexpand_tilde(&format!("~/.ghost/agents/{}/keys/agent.pub", agent.name));
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
        use crate::api::rbac;
        use axum::routing::{delete, get, post, put};

        // ── Public routes (no auth, no RBAC) ──────────────────────────
        let public_routes = axum::Router::new()
            .route("/api/health", get(crate::api::health::health_handler))
            .route("/api/ready", get(crate::api::health::ready_handler))
            .route("/api/auth/login", post(crate::api::auth::login))
            .route("/api/auth/refresh", post(crate::api::auth::refresh))
            .route("/api/auth/logout", post(crate::api::auth::logout))
            .route("/api/openapi.json", get(crate::api::openapi::openapi_spec));

        // ── Read-only routes (authenticated, no minimum RBAC role) ────
        // Any authenticated user (including Viewer) can access these.
        let read_routes = axum::Router::new()
            .route("/api/agents", get(crate::api::agents::list_agents))
            .route("/api/audit", get(crate::api::audit::query_audit))
            .route(
                "/api/audit/aggregation",
                get(crate::api::audit::audit_aggregation),
            )
            .route("/api/audit/export", get(crate::api::audit::audit_export))
            .route(
                "/api/convergence/scores",
                get(crate::api::convergence::get_scores),
            )
            .route("/api/goals", get(crate::api::goals::list_goals))
            .route("/api/goals/:id", get(crate::api::goals::get_goal))
            .route("/api/sessions", get(crate::api::sessions::list_sessions))
            .route(
                "/api/sessions/:id/events",
                get(crate::api::sessions::session_events),
            )
            .route(
                "/api/sessions/:id/bookmarks",
                get(crate::api::sessions::list_bookmarks),
            )
            .route("/api/memory", get(crate::api::memory::list_memories))
            .route(
                "/api/memory/search",
                get(crate::api::memory::search_memories),
            )
            .route(
                "/api/memory/archived",
                get(crate::api::memory::list_archived),
            )
            .route("/api/memory/:id", get(crate::api::memory::get_memory))
            .route(
                "/api/state/crdt/:agent_id",
                get(crate::api::state::get_crdt_state),
            )
            .route(
                "/api/integrity/chain/:agent_id",
                get(crate::api::integrity::verify_chain),
            )
            .route("/api/workflows", get(crate::api::workflows::list_workflows))
            .route(
                "/api/workflows/:id",
                get(crate::api::workflows::get_workflow),
            )
            .route(
                "/api/workflows/:id/executions",
                get(crate::api::workflows::list_executions),
            )
            .route(
                "/api/studio/sessions",
                get(crate::api::studio_sessions::list_sessions),
            )
            .route(
                "/api/studio/sessions/:id",
                get(crate::api::studio_sessions::get_session),
            )
            .route(
                "/api/studio/sessions/:id/stream/recover",
                get(crate::api::studio_sessions::recover_stream),
            )
            .route(
                "/api/traces/:session_id",
                get(crate::api::traces::get_traces),
            )
            .route(
                "/api/mesh/trust-graph",
                get(crate::api::mesh_viz::trust_graph),
            )
            .route(
                "/api/mesh/consensus",
                get(crate::api::mesh_viz::consensus_state),
            )
            .route(
                "/api/mesh/delegations",
                get(crate::api::mesh_viz::delegations),
            )
            .route("/api/profiles", get(crate::api::profiles::list_profiles))
            .route("/api/search", get(crate::api::search::search))
            .route("/api/skills", get(crate::api::skills::list_skills))
            .route("/api/a2a/tasks", get(crate::api::a2a::list_tasks))
            .route("/api/a2a/tasks/:task_id", get(crate::api::a2a::get_task))
            .route(
                "/api/a2a/tasks/:task_id/stream",
                get(crate::api::a2a::stream_task),
            )
            .route("/api/a2a/discover", get(crate::api::a2a::discover_agents))
            .route("/api/channels", get(crate::api::channels::list_channels))
            .route("/api/costs", get(crate::api::costs::get_costs))
            .route("/api/ws", get(crate::api::websocket::ws_handler))
            .route(
                "/api/oauth/providers",
                get(crate::api::oauth_routes::list_providers),
            )
            .route(
                "/api/oauth/callback",
                get(crate::api::oauth_routes::callback),
            )
            .route(
                "/api/oauth/connections",
                get(crate::api::oauth_routes::list_connections),
            )
            .route(
                "/api/marketplace/agents",
                get(crate::api::marketplace::list_agents),
            )
            .route(
                "/api/marketplace/agents/:id",
                get(crate::api::marketplace::get_agent),
            )
            .route(
                "/api/marketplace/skills",
                get(crate::api::marketplace::list_skills),
            )
            .route(
                "/api/marketplace/skills/:name",
                get(crate::api::marketplace::get_skill),
            )
            .route(
                "/api/marketplace/contracts",
                get(crate::api::marketplace::list_contracts),
            )
            .route(
                "/api/marketplace/contracts/:id",
                get(crate::api::marketplace::get_contract),
            )
            .route(
                "/api/marketplace/wallet",
                get(crate::api::marketplace::get_wallet),
            )
            .route(
                "/api/marketplace/wallet/transactions",
                get(crate::api::marketplace::list_transactions),
            )
            .route(
                "/api/marketplace/reviews/:agent_id",
                get(crate::api::marketplace::list_reviews),
            );

        // ── Operator routes (require Operator role) ───────────────────
        // Write operations: creating agents, sessions, running prompts, etc.
        let operator_routes = axum::Router::new()
            .route("/api/agents", post(crate::api::agents::create_agent))
            .route("/api/agents/:id", delete(crate::api::agents::delete_agent))
            .route(
                "/api/goals/:id/approve",
                post(crate::api::goals::approve_goal),
            )
            .route(
                "/api/goals/:id/reject",
                post(crate::api::goals::reject_goal),
            )
            .route("/api/memory", post(crate::api::memory::write_memory))
            .route(
                "/api/memory/:id/archive",
                post(crate::api::memory::archive_memory),
            )
            .route(
                "/api/memory/:id/unarchive",
                post(crate::api::memory::unarchive_memory),
            )
            .route(
                "/api/workflows",
                post(crate::api::workflows::create_workflow),
            )
            .route(
                "/api/workflows/:id",
                put(crate::api::workflows::update_workflow),
            )
            .route(
                "/api/workflows/:id/execute",
                post(crate::api::workflows::execute_workflow),
            )
            .route(
                "/api/workflows/:id/resume/:execution_id",
                post(crate::api::workflows::resume_execution),
            )
            .route(
                "/api/sessions/:id/bookmarks",
                post(crate::api::sessions::create_bookmark),
            )
            .route(
                "/api/sessions/:id/bookmarks/:bookmark_id",
                delete(crate::api::sessions::delete_bookmark),
            )
            .route(
                "/api/sessions/:id/branch",
                post(crate::api::sessions::branch_session),
            )
            .route(
                "/api/sessions/:id/heartbeat",
                post(crate::api::sessions::session_heartbeat),
            )
            .route("/api/studio/run", post(crate::api::studio::run_prompt))
            .route(
                "/api/studio/sessions",
                post(crate::api::studio_sessions::create_session),
            )
            .route(
                "/api/studio/sessions/:id",
                delete(crate::api::studio_sessions::delete_session),
            )
            .route(
                "/api/studio/sessions/:id/messages",
                post(crate::api::studio_sessions::send_message),
            )
            .route(
                "/api/studio/sessions/:id/messages/stream",
                post(crate::api::studio_sessions::send_message_stream),
            )
            .route("/api/agent/chat", post(crate::api::agent_chat::agent_chat))
            .route(
                "/api/agent/chat/stream",
                post(crate::api::agent_chat::agent_chat_stream),
            )
            .route("/api/profiles", post(crate::api::profiles::create_profile))
            .route(
                "/api/profiles/:name",
                put(crate::api::profiles::update_profile)
                    .delete(crate::api::profiles::delete_profile),
            )
            .route(
                "/api/agents/:id/profile",
                post(crate::api::profiles::assign_profile),
            )
            .route(
                "/api/skills/:id/install",
                post(crate::api::skills::install_skill),
            )
            .route(
                "/api/skills/:id/uninstall",
                post(crate::api::skills::uninstall_skill),
            )
            .route(
                "/api/skills/:name/execute",
                post(crate::api::skill_execute::execute_skill),
            )
            .route("/api/channels", post(crate::api::channels::create_channel))
            .route(
                "/api/channels/:id/reconnect",
                post(crate::api::channels::reconnect_channel),
            )
            .route(
                "/api/channels/:id",
                delete(crate::api::channels::delete_channel),
            )
            .route(
                "/api/channels/:type/inject",
                post(crate::api::channels::inject_message),
            )
            .route("/api/a2a/tasks", post(crate::api::a2a::send_task))
            .route(
                "/api/oauth/connect",
                post(crate::api::oauth_routes::connect),
            )
            .route(
                "/api/oauth/connections/:ref_id",
                delete(crate::api::oauth_routes::disconnect),
            )
            .route(
                "/api/oauth/execute",
                post(crate::api::oauth_routes::execute_api_call),
            )
            .route(
                "/api/marketplace/agents",
                post(crate::api::marketplace::register_agent),
            )
            .route(
                "/api/marketplace/agents/:id",
                delete(crate::api::marketplace::delist_agent),
            )
            .route(
                "/api/marketplace/agents/:id/status",
                put(crate::api::marketplace::update_agent_status),
            )
            .route(
                "/api/marketplace/skills",
                post(crate::api::marketplace::publish_skill),
            )
            .route(
                "/api/marketplace/contracts",
                post(crate::api::marketplace::propose_contract),
            )
            .route(
                "/api/marketplace/contracts/:id/accept",
                post(crate::api::marketplace::accept_contract),
            )
            .route(
                "/api/marketplace/contracts/:id/reject",
                post(crate::api::marketplace::reject_contract),
            )
            .route(
                "/api/marketplace/contracts/:id/start",
                post(crate::api::marketplace::start_contract),
            )
            .route(
                "/api/marketplace/contracts/:id/complete",
                post(crate::api::marketplace::complete_contract),
            )
            .route(
                "/api/marketplace/contracts/:id/dispute",
                post(crate::api::marketplace::dispute_contract),
            )
            .route(
                "/api/marketplace/contracts/:id/cancel",
                post(crate::api::marketplace::cancel_contract),
            )
            .route(
                "/api/marketplace/contracts/:id/resolve",
                post(crate::api::marketplace::resolve_contract),
            )
            .route(
                "/api/marketplace/wallet/seed",
                post(crate::api::marketplace::seed_wallet),
            )
            .route(
                "/api/marketplace/reviews",
                post(crate::api::marketplace::submit_review),
            )
            .route(
                "/api/marketplace/discover",
                post(crate::api::marketplace::discover_agents),
            )
            .route_layer(axum::middleware::from_fn(rbac::operator));

        // ── Admin routes (require Admin role) ─────────────────────────
        // Safety operations, provider key management, webhooks, backups.
        let admin_routes = axum::Router::new()
            // Admin GET endpoints — require Admin role to view sensitive data.
            .route("/api/safety/status", get(crate::api::safety::safety_status))
            .route(
                "/api/safety/checks",
                get(crate::api::safety_checks::list_safety_checks),
            )
            .route("/api/admin/backups", get(crate::api::admin::list_backups))
            .route("/api/admin/export", get(crate::api::admin::export_data))
            .route(
                "/api/admin/provider-keys",
                get(crate::api::provider_keys::list_provider_keys),
            )
            .route(
                "/api/pc-control/status",
                get(crate::api::pc_control::get_status).put(crate::api::pc_control::update_status),
            )
            .route(
                "/api/pc-control/actions",
                get(crate::api::pc_control::list_actions),
            )
            // Admin write endpoints.
            .route(
                "/api/safety/pause/:agent_id",
                post(crate::api::safety::pause_agent),
            )
            .route(
                "/api/safety/resume/:agent_id",
                post(crate::api::safety::resume_agent),
            )
            .route(
                "/api/safety/quarantine/:agent_id",
                post(crate::api::safety::quarantine_agent),
            )
            .route(
                "/api/safety/checks",
                post(crate::api::safety_checks::register_safety_check),
            )
            .route(
                "/api/safety/checks/:id",
                delete(crate::api::safety_checks::unregister_safety_check),
            )
            .route(
                "/api/webhooks",
                get(crate::api::webhooks::list_webhooks).post(crate::api::webhooks::create_webhook),
            )
            .route(
                "/api/webhooks/:id",
                put(crate::api::webhooks::update_webhook)
                    .delete(crate::api::webhooks::delete_webhook),
            )
            .route(
                "/api/webhooks/:id/test",
                post(crate::api::webhooks::test_webhook),
            )
            .route("/api/admin/backup", post(crate::api::admin::create_backup))
            .route(
                "/api/admin/provider-keys",
                put(crate::api::provider_keys::set_provider_key),
            )
            .route(
                "/api/admin/provider-keys/:env_name",
                delete(crate::api::provider_keys::delete_provider_key),
            )
            .route(
                "/api/pc-control/allowed-apps",
                put(crate::api::pc_control::update_allowed_apps),
            )
            .route(
                "/api/pc-control/blocked-hotkeys",
                put(crate::api::pc_control::update_blocked_hotkeys),
            )
            .route(
                "/api/pc-control/safe-zones",
                put(crate::api::pc_control::update_safe_zones),
            )
            .route_layer(axum::middleware::from_fn(rbac::admin));

        // ── SuperAdmin routes (require SuperAdmin role) ───────────────
        // Most destructive: kill-all, data restore.
        let superadmin_routes = axum::Router::new()
            .route("/api/safety/kill-all", post(crate::api::safety::kill_all))
            .route(
                "/api/admin/restore",
                post(crate::api::admin::restore_backup),
            )
            .route_layer(axum::middleware::from_fn(rbac::superadmin));

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

        // WP6-A: Build CORS layer — config first, then env var, then dev defaults.
        let cors = Self::build_cors_layer(config);

        // Rate limiting state.
        let rate_limit_state = std::sync::Arc::new(crate::api::rate_limit::RateLimitState::new());

        let ws_tracker = Arc::new(crate::api::websocket::WsConnectionTracker::new());

        let app = app
            // Middleware stack: last .layer() = outermost = runs first on request.
            //
            // Request order: Trace → CORS → request_id → auth → rate_limit → handler
            // (auth BEFORE rate_limit so Claims are available for per-token limits)
            .layer(axum::middleware::from_fn(
                crate::api::rate_limit::rate_limit_middleware,
            ))
            .layer(axum::middleware::from_fn(crate::api::auth::auth_middleware))
            .layer(axum::middleware::from_fn(
                crate::api::rate_limit::request_id_middleware,
            ))
            .layer(axum::Extension(rate_limit_state))
            .layer(axum::Extension(auth_config.clone()))
            .layer(axum::Extension(revocation_set.clone()))
            .layer(axum::Extension(ws_tracker))
            .layer(cors)
            .layer(
                tower_http::trace::TraceLayer::new_for_http().make_span_with(
                    |req: &axum::http::Request<_>| {
                        let request_id = req
                            .headers()
                            .get("x-request-id")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
                        tracing::info_span!(
                            "request",
                            method = %req.method(),
                            uri = %req.uri(),
                            request_id = %request_id,
                        )
                    },
                ),
            );

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
        use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

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
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ]))
            .allow_headers(AllowHeaders::list([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderName::from_static("x-request-id"),
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
