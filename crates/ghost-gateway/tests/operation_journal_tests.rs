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

use common::TestGateway;

fn proposal_hash() -> [u8; 32] {
    [7u8; 32]
}

async fn seed_goal_proposal(app_state: &Arc<ghost_gateway::state::AppState>, id: &str) {
    seed_goal_proposal_with_contract(app_state, id, &format!("goal:{id}:primary"), "rev-1").await;
}

async fn seed_goal_proposal_with_contract(
    app_state: &Arc<ghost_gateway::state::AppState>,
    id: &str,
    subject_key: &str,
    reviewed_revision: &str,
) {
    let db = app_state.db.write().await;
    cortex_storage::queries::goal_proposal_queries::insert_proposal(
        &db,
        id,
        "agent-1",
        "session-1",
        "agent",
        "GoalChange",
        "AgentGoal",
        &serde_json::json!({
            "subject_key": subject_key,
            "reviewed_revision": reviewed_revision,
            "goal_text": "ship idempotency",
        })
        .to_string(),
        "[]",
        "HumanReviewRequired",
        &proposal_hash(),
        &proposal_hash(),
    )
    .unwrap();
}

async fn seed_itp_event(
    app_state: &Arc<ghost_gateway::state::AppState>,
    event_id: &str,
    session_id: &str,
    sequence_number: i64,
) {
    let db = app_state.db.write().await;
    db.execute(
        "INSERT INTO itp_events (id, session_id, event_type, sender, timestamp, sequence_number, content_hash, content_length, event_hash, previous_hash, attributes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            event_id,
            session_id,
            "InteractionMessage",
            "agent-1",
            "2026-03-07T12:00:00Z",
            sequence_number,
            format!("hash-{event_id}"),
            16i64,
            vec![1u8; 32],
            vec![0u8; 32],
            "{}",
        ],
    )
    .unwrap();
}

async fn json_body(response: reqwest::Response) -> serde_json::Value {
    response.json::<serde_json::Value>().await.unwrap()
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn hold_env_lock() -> MutexGuard<'static, ()> {
    env_lock().lock().unwrap_or_else(|error| error.into_inner())
}

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
        let app_hits = Arc::clone(&hits);
        let shutdown = CancellationToken::new();
        let shutdown_token = shutdown.clone();
        let app = Router::new().route(
            "/a2a",
            post(move |Json(payload): Json<serde_json::Value>| {
                let hits = Arc::clone(&app_hits);
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": payload.get("id").cloned().unwrap_or(serde_json::Value::Null),
                            "result": { "accepted": true }
                        })),
                    )
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
            url: format!("http://127.0.0.1:{port}/a2a"),
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

async fn fetch_goal_detail(
    client: &reqwest::Client,
    base_url: &str,
    id: &str,
) -> serde_json::Value {
    client
        .get(format!("{base_url}/api/goals/{id}"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()
}

fn decision_body(
    expected_lineage_id: &str,
    expected_subject_key: &str,
    expected_reviewed_revision: &str,
) -> serde_json::Value {
    serde_json::json!({
        "expected_state": "pending_review",
        "expected_lineage_id": expected_lineage_id,
        "expected_subject_key": expected_subject_key,
        "expected_reviewed_revision": expected_reviewed_revision,
        "rationale": null,
    })
}

fn memory_write_body(memory_id: &str, delta: &str) -> serde_json::Value {
    serde_json::json!({
        "memory_id": memory_id,
        "event_type": "note",
        "delta": delta,
        "actor_id": "operator-1",
        "snapshot": serde_json::json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "agent_id": "agent-1",
            "memory_type": "semantic",
            "summary": delta,
            "content": delta,
            "importance": "medium",
            "confidence": 0.9,
            "tags": ["hardening"],
            "created_at": "2026-03-07T12:00:00Z",
            "updated_at": "2026-03-07T12:00:00Z"
        })
        .to_string(),
    })
}

fn seed_registered_agent(
    app_state: &Arc<ghost_gateway::state::AppState>,
    name: &str,
) -> uuid::Uuid {
    let agent_id = ghost_gateway::agents::registry::durable_agent_id(name);
    app_state
        .agents
        .write()
        .unwrap()
        .register(ghost_gateway::agents::registry::RegisteredAgent {
            id: agent_id,
            name: name.to_string(),
            state: ghost_gateway::agents::registry::AgentLifecycleState::Ready,
            channel_bindings: vec!["slack".into()],
            capabilities: Vec::new(),
            spending_cap: 0.0,
            template: None,
        });
    agent_id
}

async fn seed_workflow(
    app_state: &Arc<ghost_gateway::state::AppState>,
    workflow_id: &str,
    name: &str,
    nodes: serde_json::Value,
    edges: serde_json::Value,
) {
    let db = app_state.db.write().await;
    db.execute(
        "INSERT INTO workflows (id, name, description, nodes, edges, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            workflow_id,
            name,
            "integration test workflow",
            serde_json::to_string(&nodes).unwrap(),
            serde_json::to_string(&edges).unwrap(),
            "test-operator",
        ],
    )
    .unwrap();
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
            reqwest::header::HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
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
async fn goal_approval_replays_committed_response_and_writes_audit_metadata() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_goal_proposal(&gateway.app_state, "goal-replay").await;
    let detail = fetch_goal_detail(&gateway.client, &gateway.base_url, "goal-replay").await;
    let body = decision_body(
        detail["lineage_id"].as_str().unwrap(),
        detail["subject_key"].as_str().unwrap(),
        detail["reviewed_revision"].as_str().unwrap(),
    );

    let operation_id = "018f0f23-8c65-7abc-9def-1234567890ab";
    let idempotency_key = "goal-replay-approve";

    let first = gateway
        .client
        .post(gateway.url("/api/goals/goal-replay/approve"))
        .json(&body)
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
    assert_eq!(json_body(first).await["status"], "approved");

    let replay = gateway
        .client
        .post(gateway.url("/api/goals/goal-replay/approve"))
        .json(&body)
        .header("x-request-id", "retry-request-id")
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
    assert_eq!(json_body(replay).await["status"], "approved");

    let db = gateway.app_state.db.read().unwrap();
    let journal =
        cortex_storage::queries::operation_journal_queries::get_by_operation_id(&db, operation_id)
            .unwrap()
            .unwrap();
    assert_eq!(journal.status, "committed");
    assert_eq!(journal.response_status_code, Some(200));

    let audit_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE operation_id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 2);
}

#[tokio::test]
async fn idempotency_key_reuse_with_different_route_returns_conflict() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_goal_proposal(&gateway.app_state, "goal-mismatch").await;
    let detail = fetch_goal_detail(&gateway.client, &gateway.base_url, "goal-mismatch").await;
    let body = decision_body(
        detail["lineage_id"].as_str().unwrap(),
        detail["subject_key"].as_str().unwrap(),
        detail["reviewed_revision"].as_str().unwrap(),
    );

    let key = "goal-mismatch-key";
    let operation_id = "018f0f23-8c65-7abc-9def-1234567890ac";

    let first = gateway
        .client
        .post(gateway.url("/api/goals/goal-mismatch/approve"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let conflict = gateway
        .client
        .post(gateway.url("/api/goals/goal-mismatch/reject"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", key)
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
}

#[tokio::test]
async fn in_progress_duplicate_returns_conflict() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_goal_proposal(&gateway.app_state, "goal-progress").await;
    let detail = fetch_goal_detail(&gateway.client, &gateway.base_url, "goal-progress").await;
    let body = decision_body(
        detail["lineage_id"].as_str().unwrap(),
        detail["subject_key"].as_str().unwrap(),
        detail["reviewed_revision"].as_str().unwrap(),
    );
    let body_string = serde_json::to_string(&body).unwrap();

    {
        let db = gateway.app_state.db.write().await;
        let request_fingerprint = ghost_gateway::api::idempotency::fingerprint_json_request(
            "POST",
            "/api/goals/:id/approve",
            "anonymous",
            &body,
        );
        let created_at = chrono::Utc::now().to_rfc3339();
        let lease_expires_at = (chrono::Utc::now() + chrono::Duration::seconds(60)).to_rfc3339();
        let entry = cortex_storage::queries::operation_journal_queries::NewOperationJournalEntry {
            id: "journal-progress",
            actor_key: "anonymous",
            method: "POST",
            route_template: "/api/goals/:id/approve",
            operation_id: "018f0f23-8c65-7abc-9def-1234567890ad",
            request_id: Some("request-progress"),
            idempotency_key: "goal-progress-key",
            request_fingerprint: &request_fingerprint,
            request_body: &body_string,
            created_at: &created_at,
            lease_expires_at: &lease_expires_at,
        };
        cortex_storage::queries::operation_journal_queries::insert_in_progress(&db, &entry)
            .unwrap();
    }

    let response = gateway
        .client
        .post(gateway.url("/api/goals/goal-progress/approve"))
        .json(&body)
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1234567890ad",
        )
        .header("idempotency-key", "goal-progress-key")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("in_progress")
    );
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "IDEMPOTENCY_IN_PROGRESS");
}

#[tokio::test]
async fn committed_response_replays_after_gateway_restart() {
    let _guard = hold_env_lock();
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("journal-restart.db");

    let gateway = PersistentGateway::start(&db_path).await;
    seed_goal_proposal(&gateway.app_state, "goal-restart").await;
    let detail = fetch_goal_detail(&gateway.client, &gateway.base_url, "goal-restart").await;
    let body = decision_body(
        detail["lineage_id"].as_str().unwrap(),
        detail["subject_key"].as_str().unwrap(),
        detail["reviewed_revision"].as_str().unwrap(),
    );

    let operation_id = "018f0f23-8c65-7abc-9def-1234567890ae";
    let idempotency_key = "goal-restart-key";
    let first = gateway
        .client
        .post(format!(
            "{}/api/goals/goal-restart/approve",
            gateway.base_url
        ))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    gateway.stop().await;

    let restarted = PersistentGateway::start(&db_path).await;
    let replay = restarted
        .client
        .post(format!(
            "{}/api/goals/goal-restart/approve",
            restarted.base_url
        ))
        .json(&body)
        .header("x-request-id", "restart-retry")
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

    restarted.stop().await;
}

#[tokio::test]
async fn stale_approval_after_superseding_replacement_is_rejected() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_goal_proposal_with_contract(
        &gateway.app_state,
        "goal-stale-old",
        "goal:shared:primary",
        "rev-1",
    )
    .await;
    let old_detail = fetch_goal_detail(&gateway.client, &gateway.base_url, "goal-stale-old").await;

    seed_goal_proposal_with_contract(
        &gateway.app_state,
        "goal-stale-new",
        "goal:shared:primary",
        "rev-2",
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/goals/goal-stale-old/approve"))
        .json(&decision_body(
            old_detail["lineage_id"].as_str().unwrap(),
            old_detail["subject_key"].as_str().unwrap(),
            old_detail["reviewed_revision"].as_str().unwrap(),
        ))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1234567890af",
        )
        .header("idempotency-key", "goal-stale-old-approve")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "STALE_DECISION_STATE");
    assert_eq!(body["error"]["details"]["actual_state"], "superseded");
}

#[tokio::test]
async fn reject_with_wrong_reviewed_revision_is_rejected() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_goal_proposal(&gateway.app_state, "goal-wrong-revision").await;
    let detail = fetch_goal_detail(&gateway.client, &gateway.base_url, "goal-wrong-revision").await;

    let response = gateway
        .client
        .post(gateway.url("/api/goals/goal-wrong-revision/reject"))
        .json(&decision_body(
            detail["lineage_id"].as_str().unwrap(),
            detail["subject_key"].as_str().unwrap(),
            "rev-does-not-match",
        ))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1234567890b0",
        )
        .header("idempotency-key", "goal-wrong-revision-reject")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "STALE_DECISION_REVIEWED_REVISION");
}

#[tokio::test]
async fn memory_write_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let body = memory_write_body("memory-replay", "first durable note");
    let operation_id = "018f0f23-8c65-7abc-9def-2234567890ab";
    let idempotency_key = "memory-replay-key";

    let first = gateway
        .client
        .post(gateway.url("/api/memory"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
        .post(gateway.url("/api/memory"))
        .json(&body)
        .header("x-request-id", "memory-retry-request")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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

    let db = gateway.app_state.db.write().await;
    let event_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM memory_events WHERE memory_id = ?1",
            ["memory-replay"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(event_count, 1, "memory event should commit exactly once");

    let mut stmt = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status, details
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'memory_write'
             ORDER BY rowid ASC",
        )
        .unwrap();
    let rows = stmt
        .query_map([operation_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        rows.len(),
        2,
        "executed and replayed attempts should be audited"
    );
    assert_eq!(rows[0].1.as_deref(), Some(idempotency_key));
    assert_eq!(rows[0].2.as_deref(), Some("executed"));
    assert_eq!(rows[1].0.as_deref(), Some("memory-retry-request"));
    assert_eq!(rows[1].2.as_deref(), Some("replayed"));

    let executed_details: serde_json::Value = serde_json::from_str(&rows[0].3).unwrap();
    assert_eq!(executed_details["actor"], "operator-1");
    assert_eq!(executed_details["outcome"], "ok");
    assert_eq!(executed_details["details"]["memory_id"], "memory-replay");

    drop(stmt);
    drop(db);
    gateway.stop().await;
}

#[tokio::test]
async fn memory_write_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let key = "memory-mismatch-key";

    let first = gateway
        .client
        .post(gateway.url("/api/memory"))
        .json(&memory_write_body("memory-mismatch", "original note"))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-3234567890ab",
        )
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let conflict = gateway
        .client
        .post(gateway.url("/api/memory"))
        .json(&memory_write_body("memory-mismatch", "tampered note"))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-4234567890ab",
        )
        .header("idempotency-key", key)
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
async fn memory_write_replays_after_gateway_restart() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("memory-restart.db");
    let operation_id = "018f0f23-8c65-7abc-9def-5234567890ab";
    let idempotency_key = "memory-restart-key";
    let body = memory_write_body("memory-restart", "restart-safe note");

    let gateway = PersistentGateway::start(&db_path).await;
    let first = gateway
        .client
        .post(format!("{}/api/memory", gateway.base_url))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    gateway.stop().await;

    let restarted = PersistentGateway::start(&db_path).await;
    let replay = restarted
        .client
        .post(format!("{}/api/memory", restarted.base_url))
        .json(&body)
        .header("x-request-id", "restart-retry-request")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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

    let db = restarted.app_state.db.write().await;
    let event_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM memory_events WHERE memory_id = ?1",
            ["memory-restart"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        event_count, 1,
        "restart replay should not duplicate memory events"
    );

    drop(db);
    restarted.stop().await;
}

#[tokio::test]
async fn create_channel_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let agent_id = seed_registered_agent(&gateway.app_state, "channel-agent");
    let body = serde_json::json!({
        "channel_type": "slack",
        "agent_id": agent_id.to_string(),
        "config": { "workspace": "ops" }
    });
    let operation_id = "018f0f23-8c65-7abc-9def-9234567890ab";
    let idempotency_key = "channel-create-key";

    let first = gateway
        .client
        .post(gateway.url("/api/channels"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
        .post(gateway.url("/api/channels"))
        .json(&body)
        .header("x-request-id", "channel-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
    let channel_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM channels WHERE id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(channel_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'create_channel'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("channel-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn create_workflow_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let body = serde_json::json!({
        "name": "Replay-safe workflow",
        "description": "exercise idempotency",
        "nodes": [
            { "id": "n1", "type": "transform" }
        ],
        "edges": [],
    });
    let operation_id = "018f0f23-8c65-7abc-9def-a234567890ab";
    let idempotency_key = "workflow-create-key";

    let first = gateway
        .client
        .post(gateway.url("/api/workflows"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
        .post(gateway.url("/api/workflows"))
        .json(&body)
        .header("x-request-id", "workflow-create-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
    let workflow_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM workflows WHERE id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(workflow_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'create_workflow'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("workflow-create-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn create_workflow_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let key = "workflow-create-mismatch-key";

    let first = gateway
        .client
        .post(gateway.url("/api/workflows"))
        .json(&serde_json::json!({
            "name": "Original workflow",
            "nodes": [{ "id": "n1", "type": "transform" }],
            "edges": [],
        }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-b234567890ab",
        )
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let conflict = gateway
        .client
        .post(gateway.url("/api/workflows"))
        .json(&serde_json::json!({
            "name": "Tampered workflow",
            "nodes": [{ "id": "n2", "type": "wait", "wait_ms": 1 }],
            "edges": [],
        }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-c234567890ab",
        )
        .header("idempotency-key", key)
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
    assert_eq!(json_body(conflict).await["error"]["code"], "IDEMPOTENCY_KEY_REUSED");
}

#[tokio::test]
async fn execute_workflow_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_workflow(
        &gateway.app_state,
        "workflow-execute-replay",
        "Executable workflow",
        serde_json::json!([{ "id": "n1", "type": "transform" }]),
        serde_json::json!([]),
    )
    .await;

    let operation_id = "018f0f23-8c65-7abc-9def-d234567890ab";
    let idempotency_key = "workflow-execute-key";
    let request_body = serde_json::json!({ "input": { "hello": "world" } });

    let first = gateway
        .client
        .post(gateway.url("/api/workflows/workflow-execute-replay/execute"))
        .json(&request_body)
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
    let execution_id = first_body["execution_id"].as_str().unwrap().to_string();

    let replay = gateway
        .client
        .post(gateway.url("/api/workflows/workflow-execute-replay/execute"))
        .json(&request_body)
        .header("x-request-id", "workflow-execute-retry")
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
    assert_eq!(json_body(replay).await["execution_id"], execution_id);

    let db = gateway.app_state.db.read().unwrap();
    let execution_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM workflow_executions WHERE id = ?1",
            [&execution_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(execution_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'execute_workflow'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("workflow-execute-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn create_profile_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let body = serde_json::json!({
        "name": "incident-hardening",
        "description": "for replay tests",
        "weights": [0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125],
        "thresholds": [0.2, 0.4, 0.6, 0.8],
    });
    let operation_id = "018f0f23-8c65-7abc-9def-e234567890ab";
    let idempotency_key = "profile-create-key";

    let first = gateway
        .client
        .post(gateway.url("/api/profiles"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
        .post(gateway.url("/api/profiles"))
        .json(&body)
        .header("x-request-id", "profile-create-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
    let profile_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM convergence_profiles WHERE name = ?1",
            ["incident-hardening"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(profile_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'create_profile'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("profile-create-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn create_studio_session_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let body = serde_json::json!({
        "title": "Replay-safe chat",
        "model": "qwen3.5:9b",
        "system_prompt": "stay deterministic",
        "temperature": 0.2,
        "max_tokens": 1024
    });
    let operation_id = "018f0f23-8c65-7abc-9def-1334567890ab";
    let idempotency_key = "studio-session-create-key";

    let first = gateway
        .client
        .post(gateway.url("/api/studio/sessions"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
        .post(gateway.url("/api/studio/sessions"))
        .json(&body)
        .header("x-request-id", "studio-create-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
    let session_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM studio_chat_sessions WHERE id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(session_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'create_studio_session'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("studio-create-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn create_bookmark_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let operation_id = "018f0f23-8c65-7abc-9def-1434567890ab";
    let idempotency_key = "bookmark-create-key";
    let body = serde_json::json!({
        "eventIndex": 7,
        "label": "checkpoint"
    });

    let first = gateway
        .client
        .post(gateway.url("/api/sessions/session-bookmarks/bookmarks"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
        .post(gateway.url("/api/sessions/session-bookmarks/bookmarks"))
        .json(&body)
        .header("x-request-id", "bookmark-create-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
    let bookmark_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM session_bookmarks WHERE id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bookmark_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'create_session_bookmark'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("bookmark-create-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn branch_session_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_itp_event(&gateway.app_state, "evt-branch-1", "branch-source", 1).await;
    seed_itp_event(&gateway.app_state, "evt-branch-2", "branch-source", 2).await;

    let operation_id = "018f0f23-8c65-7abc-9def-1534567890ab";
    let idempotency_key = "branch-session-key";
    let body = serde_json::json!({ "from_event_index": 2 });

    let first = gateway
        .client
        .post(gateway.url("/api/sessions/branch-source/branch"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
        .post(gateway.url("/api/sessions/branch-source/branch"))
        .json(&body)
        .header("x-request-id", "branch-session-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
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
    let branched_events: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM itp_events WHERE session_id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(branched_events, 2);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'branch_session'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("branch-session-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn send_a2a_task_replay_does_not_refire_side_effect() {
    let _guard = hold_env_lock();
    let _allow_local = EnvVarGuard::set("GHOST_WEBHOOK_ALLOWED_HOSTS", "127.0.0.1");
    let capture = CaptureServer::start().await;
    let gateway = TestGateway::start().await;
    let operation_id = "018f0f23-8c65-7abc-9def-1634567890ab";
    let idempotency_key = "a2a-send-key";
    let body = serde_json::json!({
        "target_url": capture.url,
        "target_agent": "mesh-agent",
        "input": { "task": "ping" },
        "method": "tasks/send"
    });

    let first = gateway
        .client
        .post(gateway.url("/api/a2a/tasks"))
        .json(&body)
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
        .post(gateway.url("/api/a2a/tasks"))
        .json(&body)
        .header("x-request-id", "a2a-retry")
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
    assert_eq!(capture.hits.load(Ordering::SeqCst), 1);

    let db = gateway.app_state.db.read().unwrap();
    let task_count: i64 = db
        .query_row("SELECT COUNT(*) FROM a2a_tasks WHERE id = ?1", [operation_id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(task_count, 1);

    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'send_a2a_task'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("a2a-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));

    drop(db);
    capture.stop().await;
}
