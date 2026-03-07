//! Test gateway harness -- boots a real gateway on a random port
//! with a temp database for integration testing.

use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, RwLock};

use reqwest::Client;
use tokio_util::sync::CancellationToken;

/// Test gateway harness -- boots a real gateway on a random port.
pub struct TestGateway {
    pub port: u16,
    pub client: Client,
    pub base_url: String,
    shutdown_token: CancellationToken,
    server_handle: tokio::task::JoinHandle<()>,
    _tmp_dir: tempfile::TempDir,
}

impl TestGateway {
    /// Boot a gateway with a minimal config, temp database, and random port.
    ///
    /// The gateway is fully functional: DB is migrated, router is built with
    /// all middleware (auth, rate-limit, CORS, tracing), and health/ready
    /// endpoints are available.
    ///
    /// Returns once the `/api/health` endpoint responds with 200.
    pub async fn start() -> Self {
        // Find an available port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // Create temp directory for the test database.
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap().to_string();

        // Build a minimal test config.
        let config = ghost_gateway::config::GhostConfig::test_config(port, &db_path_str);

        // Create database pool and run migrations.
        let db = ghost_gateway::db_pool::create_pool(db_path)
            .expect("failed to create test DB pool");

        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer)
                .expect("failed to run migrations");
            let engine = ghost_audit::AuditQueryEngine::new(&writer);
            engine.ensure_table().expect("failed to ensure audit table");
        }

        // Build application state (minimal -- no skills, no mesh, no kill gates).
        let shared_state = Arc::new(ghost_gateway::gateway::GatewaySharedState::new());
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let replay_buffer = Arc::new(
            ghost_gateway::api::websocket::EventReplayBuffer::new(100),
        );
        let kill_switch = Arc::new(ghost_gateway::safety::kill_switch::KillSwitch::new());
        let cost_tracker = Arc::new(ghost_gateway::cost::tracker::CostTracker::new());

        // Use env-based secret provider (no secrets needed for tests).
        let secret_provider: Box<dyn ghost_secrets::SecretProvider> =
            Box::new(ghost_secrets::EnvProvider);

        // Build a minimal OAuthBroker.
        let token_store = ghost_oauth::TokenStore::with_default_dir(
            Box::new(ghost_secrets::EnvProvider) as Box<dyn ghost_secrets::SecretProvider>,
        );
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(
            std::collections::BTreeMap::new(),
            token_store,
        ));

        // Embedding engine.
        let embedding_engine = cortex_embeddings::EmbeddingEngine::new(
            cortex_embeddings::EmbeddingConfig::default(),
        );

        let app_state = Arc::new(ghost_gateway::state::AppState {
            gateway: Arc::clone(&shared_state),
            agents: Arc::new(RwLock::new(
                ghost_gateway::agents::registry::AgentRegistry::new(),
            )),
            kill_switch,
            quarantine: Arc::new(RwLock::new(
                ghost_gateway::safety::quarantine::QuarantineManager::new(),
            )),
            db,
            event_tx,
            replay_buffer,
            cost_tracker,
            kill_gate: None,
            secret_provider: Arc::from(secret_provider),
            oauth_broker,
            soul_drift_threshold: 0.15,
            convergence_profile: "standard".to_string(),
            model_providers: Vec::new(),
            tools_config: ghost_gateway::config::ToolsConfig::default(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            safety_cooldown: Arc::new(
                ghost_gateway::api::rate_limit::SafetyCooldown::new(),
            ),
            monitor_address: "127.0.0.1:0".to_string(),
            monitor_enabled: false,
            monitor_healthy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            safety_skills: Arc::new(HashMap::new()),
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: 90,
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

        let client = Client::new();

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
        assert!(healthy, "Test gateway did not become healthy within 5 seconds");

        Self {
            port,
            client,
            base_url,
            shutdown_token,
            server_handle,
            _tmp_dir: tmp_dir,
        }
    }

    /// Build a URL for the given path (e.g., "/api/health").
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Gracefully stop the test gateway.
    pub async fn stop(self) {
        self.shutdown_token.cancel();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.server_handle,
        )
        .await;
        // _tmp_dir is dropped here, cleaning up the temp database.
    }
}
