#![allow(clippy::await_holding_lock)]

mod common;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

use axum::routing::post;
use axum::{Json, Router};
use reqwest::StatusCode;
use tokio_util::sync::CancellationToken;

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(ref value) = self.previous {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn hold_env_lock() -> MutexGuard<'static, ()> {
    env_lock().lock().unwrap_or_else(|error| error.into_inner())
}

fn jwt_for_role(sub: &str, role: &str, secret: &str) -> String {
    let now = chrono::Utc::now().timestamp() as u64;
    let claims = ghost_gateway::api::auth::Claims {
        sub: sub.to_string(),
        role: role.to_string(),
        capabilities: Vec::new(),
        authz_v: None,
        exp: now + 3600,
        iat: now,
        jti: uuid::Uuid::now_v7().to_string(),
        iss: None,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

async fn json_body(response: reqwest::Response) -> serde_json::Value {
    response.json::<serde_json::Value>().await.unwrap()
}

struct CaptureServer {
    url: String,
    hits: Arc<AtomicUsize>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl CaptureServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let hits = Arc::new(AtomicUsize::new(0));
        let shutdown = CancellationToken::new();
        let shutdown_token = shutdown.clone();
        let app_hits = Arc::clone(&hits);
        let app = Router::new().route(
            "/hook",
            post(move |Json(_payload): Json<serde_json::Value>| {
                let hits = Arc::clone(&app_hits);
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }
            }),
        );

        let bind_addr = format!("127.0.0.1:{port}");
        let handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    shutdown_token.cancelled().await;
                })
                .await
                .unwrap();
        });

        Self {
            url: format!("http://127.0.0.1:{port}/hook"),
            hits,
            shutdown,
            handle,
        }
    }

    async fn stop(self) {
        self.shutdown.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.handle).await;
    }
}

struct BlockingCaptureServer {
    url: String,
    hits: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
    hit_notifier: Arc<tokio::sync::Notify>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl BlockingCaptureServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let hits = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(tokio::sync::Notify::new());
        let hit_notifier = Arc::new(tokio::sync::Notify::new());
        let shutdown = CancellationToken::new();
        let shutdown_token = shutdown.clone();
        let app_hits = Arc::clone(&hits);
        let app_release = Arc::clone(&release);
        let app_hit_notifier = Arc::clone(&hit_notifier);
        let app = Router::new().route(
            "/hook",
            post(move |Json(_payload): Json<serde_json::Value>| {
                let hits = Arc::clone(&app_hits);
                let release = Arc::clone(&app_release);
                let hit_notifier = Arc::clone(&app_hit_notifier);
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    hit_notifier.notify_waiters();
                    release.notified().await;
                    StatusCode::NO_CONTENT
                }
            }),
        );

        let bind_addr = format!("127.0.0.1:{port}");
        let handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    shutdown_token.cancelled().await;
                })
                .await
                .unwrap();
        });

        Self {
            url: format!("http://127.0.0.1:{port}/hook"),
            hits,
            release,
            hit_notifier,
            shutdown,
            handle,
        }
    }

    async fn wait_for_hit(&self) {
        if self.hits.load(Ordering::SeqCst) > 0 {
            return;
        }
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.hit_notifier.notified(),
        )
        .await
        .expect("timed out waiting for blocking webhook hit");
    }

    fn release(&self) {
        self.release.notify_waiters();
    }

    async fn stop(self) {
        self.release();
        self.shutdown.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.handle).await;
    }
}

struct PersistentGateway {
    base_url: String,
    client: reqwest::Client,
    app_state: Arc<ghost_gateway::state::AppState>,
    shutdown_token: CancellationToken,
    server_handle: tokio::task::JoinHandle<()>,
}

impl PersistentGateway {
    async fn start(db_path: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let config =
            ghost_gateway::config::GhostConfig::test_config(port, db_path.to_str().unwrap());
        let db = ghost_gateway::db_pool::create_pool(PathBuf::from(db_path)).unwrap();

        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }

        let shared_state = Arc::new(ghost_gateway::gateway::GatewaySharedState::new());
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let replay_buffer = Arc::new(ghost_gateway::api::websocket::EventReplayBuffer::new(100));
        let sandbox_reviews = ghost_gateway::sandbox_reviews::SandboxReviewCoordinator::new(
            Arc::clone(&db),
            Arc::clone(&replay_buffer),
            event_tx.clone(),
        );
        let kill_switch = Arc::new(ghost_gateway::safety::kill_switch::KillSwitch::new());
        let cost_tracker = Arc::new(ghost_gateway::cost::tracker::CostTracker::new());
        let secret_provider: Box<dyn ghost_secrets::SecretProvider> =
            Box::new(ghost_secrets::EnvProvider);
        let token_store = ghost_oauth::TokenStore::with_default_dir(Box::new(
            ghost_secrets::EnvProvider,
        )
            as Box<dyn ghost_secrets::SecretProvider>);
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(
            std::collections::BTreeMap::new(),
            token_store,
        ));
        let embedding_engine =
            cortex_embeddings::EmbeddingEngine::new(cortex_embeddings::EmbeddingConfig::default());
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
            config_path: std::path::PathBuf::from("ghost.yml"),
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
            itp_session_tracker: None,
            replay_buffer,
            cost_tracker,
            kill_gate: None,
            secret_provider: Arc::from(secret_provider),
            oauth_broker,
            mesh_signing_key: None,
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
            skill_catalog: Arc::new(
                ghost_gateway::skill_catalog::SkillCatalogService::empty_for_tests(Arc::clone(&db)),
            ),
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

        shared_state
            .transition_to(ghost_gateway::gateway::GatewayState::Healthy)
            .unwrap();

        let router = ghost_gateway::bootstrap::GatewayBootstrap::build_router(
            &config,
            Arc::clone(&app_state),
            None,
        );

        let bind_addr = format!("127.0.0.1:{port}");
        let base_url = format!("http://127.0.0.1:{port}");
        let shutdown_token = CancellationToken::new();
        let server_token = shutdown_token.clone();

        let server_handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    server_token.cancelled().await;
                })
                .await
                .unwrap();
        });

        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            ghost_gateway::api::compatibility::CLIENT_NAME_HEADER,
            reqwest::header::HeaderValue::from_static("sdk"),
        );
        default_headers.insert(
            ghost_gateway::api::compatibility::CLIENT_VERSION_HEADER,
            reqwest::header::HeaderValue::from_static("0.1.0"),
        );
        let client = reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .unwrap();

        for _ in 0..100 {
            if client
                .get(format!("{base_url}/api/health"))
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        Self {
            base_url,
            client,
            app_state,
            shutdown_token,
            server_handle,
        }
    }

    async fn stop(self) {
        self.shutdown_token.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.server_handle).await;
    }
}

#[tokio::test]
async fn create_backup_replays_after_restart_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("backup-restart.db");
    let ghost_dir = tmp.path().join("ghost");
    let backup_dir = tmp.path().join("backups");
    std::fs::create_dir_all(ghost_dir.join("data")).unwrap();
    std::fs::create_dir_all(ghost_dir.join("config")).unwrap();
    std::fs::write(ghost_dir.join("data/memory.json"), "{\"ok\":true}").unwrap();
    std::fs::write(ghost_dir.join("config/ghost.yml"), "gateway:\n  port: 0\n").unwrap();

    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "backup-jwt-secret");
    let _ghost_dir = EnvVarGuard::set("GHOST_DIR", ghost_dir.to_str().unwrap());
    let _backup_dir = EnvVarGuard::set("GHOST_BACKUP_DIR", backup_dir.to_str().unwrap());
    let _passphrase = EnvVarGuard::set("GHOST_BACKUP_PASSPHRASE", "test-backup-passphrase");
    let token = jwt_for_role("backup-admin", "admin", "backup-jwt-secret");

    let gateway = PersistentGateway::start(&db_path).await;
    let operation_id = "018f0f23-8c65-7abc-9def-6234567890ab";
    let idempotency_key = "backup-restart-key";

    let first = gateway
        .client
        .post(format!("{}/api/admin/backup", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let first_body = json_body(first).await;
    assert_eq!(first_body["backup_id"], operation_id);
    gateway.stop().await;

    let restarted = PersistentGateway::start(&db_path).await;
    let replay = restarted
        .client
        .post(format!("{}/api/admin/backup", restarted.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header("x-request-id", "backup-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );

    let archive_path = backup_dir.join(format!("ghost-backup-{operation_id}.ghost-backup"));
    assert!(archive_path.exists());

    let db = restarted.app_state.db.read().unwrap();
    let manifest_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM backup_manifest WHERE id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(manifest_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'create_backup'
             ORDER BY rowid ASC",
        )
        .unwrap()
        .query_map([operation_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(audit_rows.len(), 2);
    assert_eq!(audit_rows[0].1.as_deref(), Some(idempotency_key));
    assert_eq!(audit_rows[0].2.as_deref(), Some("executed"));
    assert_eq!(audit_rows[1].0.as_deref(), Some("backup-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));

    drop(db);
    restarted.stop().await;
}

#[tokio::test]
async fn superadmin_can_create_backup_on_admin_routes() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("superadmin-backup.db");
    let ghost_dir = tmp.path().join("ghost");
    let backup_dir = tmp.path().join("backups");
    std::fs::create_dir_all(ghost_dir.join("data")).unwrap();
    std::fs::write(ghost_dir.join("data/memory.json"), "{\"ok\":true}").unwrap();

    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "superadmin-backup-secret");
    let _ghost_dir = EnvVarGuard::set("GHOST_DIR", ghost_dir.to_str().unwrap());
    let _backup_dir = EnvVarGuard::set("GHOST_BACKUP_DIR", backup_dir.to_str().unwrap());
    let _passphrase = EnvVarGuard::set("GHOST_BACKUP_PASSPHRASE", "superadmin-passphrase");
    let token = jwt_for_role("backup-root", "superadmin", "superadmin-backup-secret");

    let gateway = PersistentGateway::start(&db_path).await;
    let response = gateway
        .client
        .post(format!("{}/api/admin/backup", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-9234567890ab",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    let backup_id = body["backup_id"].as_str().unwrap();
    assert!(backup_dir
        .join(format!("ghost-backup-{backup_id}.ghost-backup"))
        .exists());

    gateway.stop().await;
}

#[tokio::test]
async fn create_backup_failure_cleans_up_temp_archive_and_allows_retry() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("backup-cleanup.db");
    let ghost_dir = tmp.path().join("ghost");
    let backup_dir = tmp.path().join("backups");
    std::fs::create_dir_all(ghost_dir.join("data")).unwrap();
    std::fs::write(ghost_dir.join("data/memory.json"), "{\"ok\":true}").unwrap();
    std::fs::create_dir_all(&backup_dir).unwrap();

    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "backup-cleanup-secret");
    let _ghost_dir = EnvVarGuard::set("GHOST_DIR", ghost_dir.to_str().unwrap());
    let _backup_dir = EnvVarGuard::set("GHOST_BACKUP_DIR", backup_dir.to_str().unwrap());
    let _passphrase = EnvVarGuard::set("GHOST_BACKUP_PASSPHRASE", "backup-cleanup-passphrase");
    let token = jwt_for_role("backup-admin", "admin", "backup-cleanup-secret");

    let operation_id = "018f0f23-8c65-7abc-9def-9734567890ab";
    let output_path = backup_dir.join(format!("ghost-backup-{operation_id}.ghost-backup"));
    let temp_path = backup_dir.join(format!(".ghost-backup-{operation_id}.tmp"));
    std::fs::create_dir_all(&output_path).unwrap();

    let gateway = PersistentGateway::start(&db_path).await;
    let failed = gateway
        .client
        .post(format!("{}/api/admin/backup", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", "backup-cleanup-key")
        .send()
        .await
        .unwrap();
    assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        !temp_path.exists(),
        "staged temp archive must be cleaned up"
    );
    assert!(output_path.is_dir());

    std::fs::remove_dir_all(&output_path).unwrap();

    let retry = gateway
        .client
        .post(format!("{}/api/admin/backup", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header("x-request-id", "backup-cleanup-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", "backup-cleanup-key")
        .send()
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(
        retry
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    assert!(output_path.is_file());
    assert!(!temp_path.exists());

    gateway.stop().await;
}

#[tokio::test]
async fn superadmin_can_list_provider_keys() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("provider-keys-superadmin.db");
    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "provider-keys-superadmin-secret");
    let token = jwt_for_role(
        "provider-root",
        "superadmin",
        "provider-keys-superadmin-secret",
    );

    let gateway = PersistentGateway::start(&db_path).await;
    let response = gateway
        .client
        .get(format!("{}/api/admin/provider-keys", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert!(body["providers"].is_array());

    gateway.stop().await;
}

#[tokio::test]
async fn operator_can_read_safety_status_route() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("safety-status-operator.db");
    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "safety-status-operator-secret");
    let token = jwt_for_role(
        "safety-operator",
        "operator",
        "safety-status-operator-secret",
    );

    let gateway = PersistentGateway::start(&db_path).await;
    let response = gateway
        .client
        .get(format!("{}/api/safety/status", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert!(body.get("per_agent").is_some());
    assert!(body.get("platform_level").is_some());

    gateway.stop().await;
}

#[tokio::test]
async fn viewer_cannot_read_safety_status_route() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("safety-status-viewer.db");
    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "safety-status-viewer-secret");
    let token = jwt_for_role("safety-viewer", "viewer", "safety-status-viewer-secret");

    let gateway = PersistentGateway::start(&db_path).await;
    let response = gateway
        .client
        .get(format!("{}/api/safety/status", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    gateway.stop().await;
}

#[tokio::test]
async fn admin_cannot_verify_restore_on_superadmin_route() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("restore-admin-forbidden.db");
    let source_dir = tmp.path().join("source");
    let backup_path = tmp.path().join("forbidden.ghost-backup");
    std::fs::create_dir_all(source_dir.join("data")).unwrap();
    std::fs::write(source_dir.join("data/file.txt"), "forbidden").unwrap();

    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "restore-admin-secret");
    let _passphrase = EnvVarGuard::set("GHOST_BACKUP_PASSPHRASE", "restore-admin-passphrase");
    let token = jwt_for_role("restore-admin", "admin", "restore-admin-secret");
    ghost_backup::BackupExporter::new(&source_dir)
        .export(&backup_path, "restore-admin-passphrase")
        .unwrap();

    let gateway = PersistentGateway::start(&db_path).await;
    let response = gateway
        .client
        .post(format!("{}/api/admin/restore", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "backup_path": backup_path }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    gateway.stop().await;
}

#[tokio::test]
async fn restore_backup_mismatch_reuse_fails_loudly() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("restore-mismatch.db");
    let ghost_dir = tmp.path().join("ghost");
    let backup_dir = tmp.path().join("backups");
    std::fs::create_dir_all(ghost_dir.join("data")).unwrap();
    std::fs::write(ghost_dir.join("data/memory.json"), "{\"ok\":true}").unwrap();
    std::fs::create_dir_all(&backup_dir).unwrap();

    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "restore-jwt-secret");
    let _ghost_dir = EnvVarGuard::set("GHOST_DIR", ghost_dir.to_str().unwrap());
    let _backup_dir = EnvVarGuard::set("GHOST_BACKUP_DIR", backup_dir.to_str().unwrap());
    let _passphrase = EnvVarGuard::set("GHOST_BACKUP_PASSPHRASE", "test-backup-passphrase");
    let token = jwt_for_role("restore-root", "superadmin", "restore-jwt-secret");

    let source_a = tmp.path().join("source-a");
    std::fs::create_dir_all(source_a.join("data")).unwrap();
    std::fs::write(source_a.join("data/file.txt"), "alpha").unwrap();
    let archive_a = backup_dir.join("a.ghost-backup");
    ghost_backup::BackupExporter::new(&source_a)
        .export(&archive_a, "test-backup-passphrase")
        .unwrap();

    let source_b = tmp.path().join("source-b");
    std::fs::create_dir_all(source_b.join("data")).unwrap();
    std::fs::write(source_b.join("data/file.txt"), "bravo").unwrap();
    let archive_b = backup_dir.join("b.ghost-backup");
    ghost_backup::BackupExporter::new(&source_b)
        .export(&archive_b, "test-backup-passphrase")
        .unwrap();

    let gateway = PersistentGateway::start(&db_path).await;
    let first = gateway
        .client
        .post(format!("{}/api/admin/restore", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-7234567890ab",
        )
        .header("idempotency-key", "restore-shared-key")
        .json(&serde_json::json!({ "backup_path": archive_a }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let conflict = gateway
        .client
        .post(format!("{}/api/admin/restore", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-8234567890ab",
        )
        .header("idempotency-key", "restore-shared-key")
        .json(&serde_json::json!({ "backup_path": archive_b }))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        conflict
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("mismatch")
    );
    let body = json_body(conflict).await;
    assert_eq!(body["error"]["code"], "IDEMPOTENCY_KEY_REUSED");

    gateway.stop().await;
}

#[tokio::test]
async fn create_webhook_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("webhook-create.db");
    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "webhook-jwt-secret");
    let token = jwt_for_role("webhook-admin", "admin", "webhook-jwt-secret");

    let gateway = PersistentGateway::start(&db_path).await;
    let operation_id = "018f0f23-8c65-7abc-9def-f234567890ab";
    let idempotency_key = "webhook-create-key";
    let body = serde_json::json!({
        "name": "Ops webhook",
        "url": "https://example.com/ops-hook",
        "secret": "whsec_test",
        "events": ["backup_complete"],
        "headers": { "x-env": "test" }
    });

    let first = gateway
        .client
        .post(format!("{}/api/webhooks", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );

    let replay = gateway
        .client
        .post(format!("{}/api/webhooks", gateway.base_url))
        .header("authorization", format!("Bearer {token}"))
        .header("x-request-id", "webhook-create-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );

    let db = gateway.app_state.db.read().unwrap();
    let webhook_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM webhooks WHERE id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(webhook_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'create_webhook'
             ORDER BY rowid ASC",
        )
        .unwrap()
        .query_map([operation_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(audit_rows.len(), 2);
    assert_eq!(audit_rows[0].2.as_deref(), Some("executed"));
    assert_eq!(audit_rows[1].0.as_deref(), Some("webhook-create-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));

    drop(db);
    gateway.stop().await;
}

#[tokio::test]
async fn test_webhook_replay_does_not_refire_side_effect() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("webhook-test.db");
    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "webhook-test-secret");
    let token = jwt_for_role("webhook-admin", "admin", "webhook-test-secret");
    let capture = CaptureServer::start().await;

    let gateway = PersistentGateway::start(&db_path).await;
    let webhook_id = "capture-webhook".to_string();
    {
        let db = gateway.app_state.db.write().await;
        db.execute(
            "INSERT INTO webhooks (id, name, url, secret, events, headers, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            rusqlite::params![
                webhook_id,
                "Capture webhook",
                capture.url,
                "whsec_capture",
                serde_json::to_string(&vec!["backup_complete"]).unwrap(),
                "{}",
            ],
        )
        .unwrap();
    }

    let operation_id = "018f0f23-8c65-7abc-9def-1234567890bb";
    let idempotency_key = "webhook-test-key";
    let first = gateway
        .client
        .post(format!(
            "{}/api/webhooks/{webhook_id}/test",
            gateway.base_url
        ))
        .header("authorization", format!("Bearer {token}"))
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    assert_eq!(capture.hits.load(Ordering::SeqCst), 1);

    let replay = gateway
        .client
        .post(format!(
            "{}/api/webhooks/{webhook_id}/test",
            gateway.base_url
        ))
        .header("authorization", format!("Bearer {token}"))
        .header("x-request-id", "webhook-test-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );
    assert_eq!(
        capture.hits.load(Ordering::SeqCst),
        1,
        "replayed webhook test must not fire the outbound request again"
    );

    let db = gateway.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'test_webhook'
             ORDER BY rowid ASC",
        )
        .unwrap()
        .query_map([operation_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(audit_rows.len(), 2);
    assert_eq!(audit_rows[0].1.as_deref(), Some("executed"));
    assert_eq!(audit_rows[1].0.as_deref(), Some("webhook-test-retry"));
    assert_eq!(audit_rows[1].1.as_deref(), Some("replayed"));

    drop(db);
    gateway.stop().await;
    capture.stop().await;
}

#[tokio::test]
async fn test_webhook_lost_ownership_does_not_broadcast_websocket_event() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("webhook-test-ownership.db");
    let _jwt_secret = EnvVarGuard::set("GHOST_JWT_SECRET", "webhook-ownership-secret");
    let token = jwt_for_role("webhook-admin", "admin", "webhook-ownership-secret");
    let capture = BlockingCaptureServer::start().await;

    let gateway = PersistentGateway::start(&db_path).await;
    let mut event_rx = gateway.app_state.event_tx.subscribe();
    let webhook_id = "capture-webhook-ownership".to_string();
    {
        let db = gateway.app_state.db.write().await;
        db.execute(
            "INSERT INTO webhooks (id, name, url, secret, events, headers, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            rusqlite::params![
                webhook_id,
                "Capture webhook ownership",
                capture.url,
                "whsec_capture",
                serde_json::to_string(&vec!["backup_complete"]).unwrap(),
                "{}",
            ],
        )
        .unwrap();
    }

    let operation_id = "018f0f23-8c65-7abc-9def-1234567890bc";
    let request = gateway
        .client
        .post(format!(
            "{}/api/webhooks/{webhook_id}/test",
            gateway.base_url
        ))
        .header("authorization", format!("Bearer {token}"))
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", "webhook-ownership-key");

    let response_task = tokio::spawn(async move { request.send().await.unwrap() });

    capture.wait_for_hit().await;

    {
        let db = gateway.app_state.db.write().await;
        let journal = cortex_storage::queries::operation_journal_queries::get_by_operation_id(
            &db,
            operation_id,
        )
        .unwrap()
        .expect("operation journal row should exist while request is in flight");
        db.execute(
            "UPDATE operation_journal
             SET owner_token = ?2,
                 lease_epoch = ?3
             WHERE id = ?1",
            rusqlite::params![journal.id, "stolen-owner-token", journal.lease_epoch + 1],
        )
        .unwrap();
    }

    capture.release();

    let response = response_task.await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(capture.hits.load(Ordering::SeqCst), 1);

    let next_event =
        tokio::time::timeout(std::time::Duration::from_millis(250), event_rx.recv()).await;
    assert!(
        next_event.is_err(),
        "WebhookFired must not be broadcast when commit ownership is lost"
    );

    gateway.stop().await;
    capture.stop().await;
}
