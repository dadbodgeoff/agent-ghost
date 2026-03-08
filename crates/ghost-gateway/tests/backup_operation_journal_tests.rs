mod common;

use std::collections::HashMap;
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
        exp: now + 3600,
        iat: now,
        jti: uuid::Uuid::now_v7().to_string(),
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
            let engine = ghost_audit::AuditQueryEngine::new(&writer);
            engine.ensure_table().unwrap();
        }

        let shared_state = Arc::new(ghost_gateway::gateway::GatewaySharedState::new());
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let replay_buffer = Arc::new(ghost_gateway::api::websocket::EventReplayBuffer::new(100));
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
            default_model_provider: None,
            pc_control_circuit_breaker: ghost_pc_control::safety::PcControlConfig::default()
                .circuit_breaker(),
            websocket_auth_tickets: Arc::new(dashmap::DashMap::new()),
            tools_config: ghost_gateway::config::ToolsConfig::default(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            safety_cooldown: Arc::new(ghost_gateway::api::rate_limit::SafetyCooldown::new()),
            monitor_address: "127.0.0.1:0".to_string(),
            monitor_enabled: false,
            monitor_block_on_degraded: false,
            convergence_state_stale_after: std::time::Duration::from_secs(300),
            monitor_healthy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            distributed_kill_enabled: false,
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            safety_skills: Arc::new(HashMap::new()),
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: 90,
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
        .post(format!("{}/api/webhooks/{webhook_id}/test", gateway.base_url))
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
        .post(format!("{}/api/webhooks/{webhook_id}/test", gateway.base_url))
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
