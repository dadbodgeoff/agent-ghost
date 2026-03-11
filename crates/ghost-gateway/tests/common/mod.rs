//! Test gateway harness -- boots a real gateway on a random port
//! with a temp database for integration testing.

use std::net::TcpListener;
use std::sync::{Arc, Mutex, RwLock};

use reqwest::Client;
use tokio_util::sync::CancellationToken;

/// Test gateway harness -- boots a real gateway on a random port.
#[allow(dead_code)]
pub struct TestGateway {
    pub port: u16,
    pub client: Client,
    pub base_url: String,
    pub app_state: Arc<ghost_gateway::state::AppState>,
    shutdown_token: CancellationToken,
    server_handle: tokio::task::JoinHandle<()>,
    _tmp_dir: tempfile::TempDir,
}

#[allow(dead_code)]
impl TestGateway {
    /// Boot a gateway with a minimal config, temp database, and random port.
    ///
    /// The gateway is fully functional: DB is migrated, router is built with
    /// all middleware (auth, rate-limit, CORS, tracing), and health/ready
    /// endpoints are available.
    ///
    /// Returns once the `/api/health` endpoint responds with 200.
    pub async fn start() -> Self {
        Self::start_internal(100, false, false, false).await
    }

    pub async fn start_with_compiled_skills() -> Self {
        Self::start_internal(100, true, false, false).await
    }

    pub async fn start_with_external_skill_runtime() -> Self {
        Self::start_internal(100, false, false, true).await
    }

    /// Boot a gateway with a custom WebSocket replay buffer capacity.
    pub async fn start_with_replay_capacity(replay_capacity: usize) -> Self {
        Self::start_internal(replay_capacity, false, false, false).await
    }

    pub async fn start_with_ws_ticket_auth_only(ws_ticket_auth_only: bool) -> Self {
        Self::start_internal(100, false, ws_ticket_auth_only, false).await
    }

    async fn start_internal(
        replay_capacity: usize,
        include_compiled_skills: bool,
        ws_ticket_auth_only: bool,
        enable_external_skill_runtime: bool,
    ) -> Self {
        // Find an available port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // Create temp directory for the test database.
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap().to_string();

        // Build a minimal test config.
        let mut config = ghost_gateway::config::GhostConfig::test_config(port, &db_path_str);
        config.gateway.ws_ticket_auth_only = ws_ticket_auth_only;
        if enable_external_skill_runtime {
            config.external_skills.enabled = true;
            config.external_skills.execution_enabled = true;
            config.external_skills.managed_storage_path =
                tmp_dir.path().join("managed").display().to_string();
        }
        let config_path = tmp_dir.path().join("ghost.yml");
        std::fs::write(
            &config_path,
            serde_yaml::to_string(&config).expect("failed to serialize test config"),
        )
        .expect("failed to write test config");

        // Create database pool and run migrations.
        let db =
            ghost_gateway::db_pool::create_pool(db_path).expect("failed to create test DB pool");

        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).expect("failed to run migrations");
        }

        // Build application state (minimal -- no skills, no mesh, no kill gates).
        let shared_state = Arc::new(ghost_gateway::gateway::GatewaySharedState::new());
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let replay_buffer = Arc::new(ghost_gateway::api::websocket::EventReplayBuffer::new(
            replay_capacity,
        ));
        let kill_switch = Arc::new(ghost_gateway::safety::kill_switch::KillSwitch::new());
        let cost_tracker = Arc::new(ghost_gateway::cost::tracker::CostTracker::new());
        let mesh_signing_key = Arc::new(Mutex::new(ghost_signing::generate_keypair().0));

        // Use env-based secret provider (no secrets needed for tests).
        let secret_provider: Box<dyn ghost_secrets::SecretProvider> =
            Box::new(ghost_secrets::EnvProvider);

        // Build a minimal OAuthBroker.
        let token_store = ghost_oauth::TokenStore::with_default_dir(Box::new(
            ghost_secrets::EnvProvider,
        )
            as Box<dyn ghost_secrets::SecretProvider>);
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(
            std::collections::BTreeMap::new(),
            token_store,
        ));

        // Embedding engine.
        let embedding_engine =
            cortex_embeddings::EmbeddingEngine::new(cortex_embeddings::EmbeddingConfig::default());

        let definitions = if include_compiled_skills {
            ghost_gateway::skill_catalog::build_compiled_skill_definitions(&config).definitions
        } else {
            Vec::new()
        };
        let skill_catalog = if include_compiled_skills
            || enable_external_skill_runtime
            || config.external_skills.enabled
        {
            Arc::new(
                ghost_gateway::skill_catalog::SkillCatalogService::new(
                    definitions,
                    Arc::clone(&db),
                    config.external_skills.clone(),
                )
                .await
                .expect("test skill catalog"),
            )
        } else {
            Arc::new(
                ghost_gateway::skill_catalog::SkillCatalogService::empty_for_tests(Arc::clone(&db)),
            )
        };
        let sandbox_reviews = ghost_gateway::sandbox_reviews::SandboxReviewCoordinator::new(
            Arc::clone(&db),
            Arc::clone(&replay_buffer),
            event_tx.clone(),
        );
        let ws_connection_tracker =
            Arc::new(ghost_gateway::api::websocket::WsConnectionTracker::new());
        let pc_control_runtime = Arc::new(
            ghost_gateway::pc_control_runtime::PcControlRuntimeService::new(
                &ghost_pc_control::safety::PcControlConfig::default(),
                "tests",
            ),
        );
        let agents = Arc::new(RwLock::new(
            ghost_gateway::agents::registry::AgentRegistry::new(),
        ));
        let channel_manager = Arc::new(ghost_gateway::channel_manager::ChannelManager::new(
            Arc::clone(&db),
            Arc::clone(&agents),
            event_tx.clone(),
            Arc::clone(&replay_buffer),
        ));

        let app_state = Arc::new(ghost_gateway::state::AppState {
            started_at: std::time::Instant::now(),
            gateway: Arc::clone(&shared_state),
            config_path: config_path.clone(),
            agents,
            channel_manager,
            kill_switch,
            quarantine: Arc::new(RwLock::new(
                ghost_gateway::safety::quarantine::QuarantineManager::new(),
            )),
            db: Arc::clone(&db),
            event_tx,
            trigger_sender:
                tokio::sync::mpsc::channel::<cortex_core::safety::trigger::TriggerEvent>(16).0,
            sandbox_reviews,
            itp_emitter: None,
            itp_router: None,
            itp_session_tracker: Some(Arc::new(ghost_gateway::itp_bridge::ITPSessionTracker::new(
                std::time::Duration::from_secs(30 * 60),
            ))),
            replay_buffer,
            cost_tracker,
            kill_gate: None,
            secret_provider: Arc::from(secret_provider),
            oauth_broker,
            mesh_signing_key: Some(mesh_signing_key),
            soul_drift_threshold: 0.15,
            convergence_profile: "standard".to_string(),
            model_providers: Vec::new(),
            default_model_provider: None,
            pc_control_circuit_breaker: ghost_pc_control::safety::PcControlConfig::default()
                .circuit_breaker(),
            pc_control_runtime,
            websocket_auth_tickets: Arc::new(dashmap::DashMap::new()),
            ws_connection_tracker,
            ws_ticket_auth_only: config.gateway.ws_ticket_auth_only,
            tools_config: ghost_gateway::config::ToolsConfig::default(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            live_execution_controls: Arc::new(dashmap::DashMap::new()),
            safety_cooldown: Arc::new(ghost_gateway::api::rate_limit::SafetyCooldown::new()),
            monitor_address: "127.0.0.1:0".to_string(),
            monitor_enabled: false,
            monitor_block_on_degraded: false,
            convergence_state_stale_after: std::time::Duration::from_secs(300),
            monitor_healthy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            monitor_runtime_status: Arc::new(RwLock::new(
                ghost_gateway::state::MonitorRuntimeStatus::default(),
            )),
            distributed_kill_enabled: false,
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            skill_catalog,
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: 90,
            backup_scheduler_status: Arc::new(RwLock::new(
                ghost_gateway::state::BackupSchedulerRuntimeStatus::default(),
            )),
            config_watcher_status: Arc::new(RwLock::new(
                ghost_gateway::state::ConfigWatcherRuntimeStatus::default(),
            )),
            autonomy: Arc::new(ghost_gateway::autonomy::AutonomyService::default()),
        });

        // Transition to Healthy so health endpoint returns 200.
        shared_state
            .transition_to(ghost_gateway::gateway::GatewayState::Healthy)
            .expect("failed to transition to Healthy");

        // Build the full API router (includes all middleware).
        let router = ghost_gateway::bootstrap::GatewayBootstrap::build_router(
            &config,
            Arc::clone(&app_state),
            None, // no mesh router
        );

        let base_url = format!("http://127.0.0.1:{}", port);
        let bind_addr = format!("127.0.0.1:{}", port);

        let shutdown_token = CancellationToken::new();
        let server_token = shutdown_token.clone();

        // Spawn the axum server in a background task.
        let server_handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(&bind_addr)
                .await
                .expect("failed to bind test server");

            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    server_token.cancelled().await;
                })
                .await
                .expect("test server error");
        });

        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            ghost_gateway::api::compatibility::CLIENT_NAME_HEADER,
            reqwest::header::HeaderValue::from_static("sdk"),
        );
        default_headers.insert(
            ghost_gateway::api::compatibility::CLIENT_VERSION_HEADER,
            reqwest::header::HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
        );

        let client = Client::builder()
            .default_headers(default_headers)
            .build()
            .expect("failed to build test client");

        // Wait for the health endpoint to become available.
        let health_url = format!("{}/api/health", &base_url);
        let mut healthy = false;
        for _ in 0..100 {
            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    healthy = true;
                    break;
                }
                _ => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
        assert!(
            healthy,
            "Test gateway did not become healthy within 5 seconds"
        );

        Self {
            port,
            client,
            base_url,
            app_state,
            shutdown_token,
            server_handle,
            _tmp_dir: tmp_dir,
        }
    }

    /// Build a URL for the given path (e.g., "/api/health").
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub fn temp_dir(&self) -> &std::path::Path {
        self._tmp_dir.path()
    }

    /// Gracefully stop the test gateway.
    pub async fn stop(self) {
        self.shutdown_token.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.server_handle).await;
        // _tmp_dir is dropped here, cleaning up the temp database.
    }
}
