#![allow(clippy::await_holding_lock)]

mod common;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::StatusCode;
use secrecy::SecretString;
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

async fn fetch_live_execution(
    client: &reqwest::Client,
    base_url: &str,
    execution_id: &str,
) -> reqwest::Response {
    client
        .get(format!("{base_url}/api/live-executions/{execution_id}"))
        .send()
        .await
        .unwrap()
}

fn extract_stream_session_id(stream: &str) -> String {
    let marker = "\"session_id\":\"";
    let start = stream
        .find(marker)
        .map(|index| index + marker.len())
        .expect("stream should contain session_id");
    let end = stream[start..]
        .find('"')
        .map(|offset| start + offset)
        .expect("session_id should terminate");
    stream[start..end].to_string()
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

#[derive(Debug, Clone)]
struct CapturedA2ARequest {
    path: String,
    signature: Option<String>,
}

struct CaptureServer {
    url: String,
    hits: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<CapturedA2ARequest>>>,
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
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app_requests = Arc::clone(&requests);
        let shutdown = CancellationToken::new();
        let shutdown_token = shutdown.clone();
        let app = Router::new().route(
            "/a2a",
            post(
                move |headers: axum::http::HeaderMap,
                      uri: axum::http::Uri,
                      Json(payload): Json<serde_json::Value>| {
                    let hits = Arc::clone(&app_hits);
                    let requests = Arc::clone(&app_requests);
                    async move {
                        hits.fetch_add(1, Ordering::SeqCst);
                        requests.lock().unwrap().push(CapturedA2ARequest {
                            path: uri.path().to_string(),
                            signature: headers
                                .get("X-Ghost-Signature")
                                .and_then(|value| value.to_str().ok())
                                .map(ToString::to_string),
                        });
                        (
                            StatusCode::OK,
                            Json(serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": payload.get("id").cloned().unwrap_or(serde_json::Value::Null),
                                "result": { "accepted": true }
                            })),
                        )
                    }
                },
            ),
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
            url: format!("http://127.0.0.1:{port}"),
            hits,
            requests,
            shutdown,
            handle,
        }
    }

    fn requests(&self) -> Vec<CapturedA2ARequest> {
        self.requests.lock().unwrap().clone()
    }

    async fn stop(self) {
        self.shutdown.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.handle).await;
    }
}

struct MockOpenAICompatServer {
    base_url: String,
    hits: Arc<AtomicUsize>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl MockOpenAICompatServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let hits = Arc::new(AtomicUsize::new(0));
        let app_hits = Arc::clone(&hits);
        let shutdown = CancellationToken::new();
        let shutdown_token = shutdown.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |Json(payload): Json<serde_json::Value>| {
                let hits = Arc::clone(&app_hits);
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    let streaming = payload
                        .get("stream")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);

                    if streaming {
                        let body = concat!(
                            "data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
                            "data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n",
                            "data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        let mut response = (axum::http::StatusCode::OK, body).into_response();
                        response.headers_mut().insert(
                            axum::http::header::CONTENT_TYPE,
                            axum::http::HeaderValue::from_static("text/event-stream"),
                        );
                        response
                    } else {
                        (
                            axum::http::StatusCode::OK,
                            Json(serde_json::json!({
                                "id": "chatcmpl-test",
                                "model": "test-model",
                                "choices": [
                                    {
                                        "message": {
                                            "role": "assistant",
                                            "content": "Hello world"
                                        },
                                        "finish_reason": "stop"
                                    }
                                ],
                                "usage": {
                                    "prompt_tokens": 5,
                                    "completion_tokens": 2,
                                    "total_tokens": 7
                                }
                            })),
                        )
                            .into_response()
                    }
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
            base_url: format!("http://127.0.0.1:{port}"),
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

struct MockOpenAICompatToolCallServer {
    base_url: String,
    hits: Arc<AtomicUsize>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl MockOpenAICompatToolCallServer {
    async fn start_shell_then_text(command: &str, final_text: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let hits = Arc::new(AtomicUsize::new(0));
        let app_hits = Arc::clone(&hits);
        let shutdown = CancellationToken::new();
        let shutdown_token = shutdown.clone();

        let tool_arguments = serde_json::json!({ "command": command }).to_string();
        let first_stream = Arc::new(format!(
            "data: {}\n\n\
             data: {}\n\n\
             data: {}\n\n\
             data: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-tool-call",
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_shell_1",
                            "function": {
                                "name": "shell",
                                "arguments": ""
                            }
                        }]
                    }
                }]
            }),
            serde_json::json!({
                "id": "chatcmpl-tool-call",
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "function": {
                                "arguments": tool_arguments
                            }
                        }]
                    }
                }]
            }),
            serde_json::json!({
                "id": "chatcmpl-tool-call",
                "choices": [{
                    "delta": {}
                }],
                "usage": {
                    "prompt_tokens": 11,
                    "completion_tokens": 5
                }
            })
        ));
        let second_stream = Arc::new(format!(
            "data: {}\n\n\
             data: {}\n\n\
             data: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-tool-result",
                "choices": [{
                    "delta": {
                        "content": final_text
                    }
                }]
            }),
            serde_json::json!({
                "id": "chatcmpl-tool-result",
                "choices": [{
                    "delta": {}
                }],
                "usage": {
                    "prompt_tokens": 7,
                    "completion_tokens": 3
                }
            })
        ));

        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |Json(payload): Json<serde_json::Value>| {
                let hits = Arc::clone(&app_hits);
                let first_stream = Arc::clone(&first_stream);
                let second_stream = Arc::clone(&second_stream);
                async move {
                    let request_index = hits.fetch_add(1, Ordering::SeqCst);
                    let streaming = payload
                        .get("stream")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    assert!(streaming, "tool-call mock expects a streaming request");

                    let body = if request_index == 0 {
                        (*first_stream).clone()
                    } else {
                        (*second_stream).clone()
                    };
                    let mut response = (axum::http::StatusCode::OK, body).into_response();
                    response.headers_mut().insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("text/event-stream"),
                    );
                    response
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
            base_url: format!("http://127.0.0.1:{port}"),
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

fn openai_compat_provider_configs(base_url: &str) -> Vec<ghost_gateway::config::ProviderConfig> {
    vec![ghost_gateway::config::ProviderConfig {
        name: "openai_compat".into(),
        api_key_env: Some("OPENAI_API_KEY".into()),
        model: Some("test-model".into()),
        base_url: Some(base_url.to_string()),
    }]
}

#[derive(Default)]
struct OAuthProviderCounters {
    revoke_hits: AtomicUsize,
    execute_hits: AtomicUsize,
}

struct TestOAuthProvider {
    name: String,
    counters: Arc<OAuthProviderCounters>,
}

impl TestOAuthProvider {
    fn new(name: &str, counters: Arc<OAuthProviderCounters>) -> Self {
        Self {
            name: name.to_string(),
            counters,
        }
    }
}

impl ghost_oauth::OAuthProvider for TestOAuthProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn authorization_url(
        &self,
        scopes: &[String],
        state: &str,
        redirect_uri: &str,
    ) -> Result<(String, ghost_oauth::PkceChallenge), ghost_oauth::OAuthError> {
        let pkce = ghost_oauth::PkceChallenge::generate();
        Ok((
            format!(
                "https://{}.example.com/auth?state={state}&redirect_uri={redirect_uri}&scope={}",
                self.name,
                scopes.join(",")
            ),
            pkce,
        ))
    }

    fn exchange_code(
        &self,
        code: &str,
        _pkce_verifier: &str,
        _redirect_uri: &str,
    ) -> Result<ghost_oauth::TokenSet, ghost_oauth::OAuthError> {
        Ok(ghost_oauth::TokenSet {
            access_token: SecretString::from(format!("access-{code}")),
            refresh_token: Some(SecretString::from("refresh-token".to_string())),
            expires_at: Utc::now() + ChronoDuration::minutes(15),
            scopes: vec!["read".into()],
        })
    }

    fn refresh_token(
        &self,
        _refresh_token: &str,
    ) -> Result<ghost_oauth::TokenSet, ghost_oauth::OAuthError> {
        Ok(ghost_oauth::TokenSet {
            access_token: SecretString::from("refreshed-access-token".to_string()),
            refresh_token: Some(SecretString::from("refresh-token".to_string())),
            expires_at: Utc::now() + ChronoDuration::minutes(15),
            scopes: vec!["read".into()],
        })
    }

    fn revoke_token(&self, _token: &str) -> Result<(), ghost_oauth::OAuthError> {
        self.counters.revoke_hits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn execute_api_call(
        &self,
        access_token: &str,
        request: &ghost_oauth::ApiRequest,
    ) -> Result<ghost_oauth::ApiResponse, ghost_oauth::OAuthError> {
        self.counters.execute_hits.fetch_add(1, Ordering::SeqCst);
        if request.url.contains("/provider-error") {
            return Err(ghost_oauth::OAuthError::ProviderError(
                "API call failed: synthetic provider failure after dispatch".into(),
            ));
        }

        Ok(ghost_oauth::ApiResponse {
            status: 200,
            headers: std::collections::BTreeMap::new(),
            body: serde_json::json!({
                "provider": self.name,
                "access_token": access_token,
                "method": request.method,
                "url": request.url,
            })
            .to_string(),
        })
    }
}

fn oauth_provider_map(
    counters: Arc<OAuthProviderCounters>,
) -> std::collections::BTreeMap<String, Box<dyn ghost_oauth::OAuthProvider>> {
    let mut providers: std::collections::BTreeMap<String, Box<dyn ghost_oauth::OAuthProvider>> =
        std::collections::BTreeMap::new();
    providers.insert(
        "mock".into(),
        Box::new(TestOAuthProvider::new("mock", counters)),
    );
    providers
}

fn oauth_execute_request_body(ref_id: &str, method: &str, url: &str) -> serde_json::Value {
    serde_json::json!({
        "ref_id": ref_id,
        "api_request": {
            "method": method,
            "url": url,
            "headers": {},
            "body": serde_json::Value::Null,
        }
    })
}

async fn oauth_connect_ref_id(gateway: &PersistentGateway, code: &str) -> String {
    let connect = gateway
        .client
        .post(gateway.url("/api/oauth/connect"))
        .json(&serde_json::json!({
            "provider": "mock",
            "scopes": ["read"],
            "redirect_uri": "http://localhost/cb",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(connect.status(), StatusCode::OK);

    let connect_body = json_body(connect).await;
    let auth_url = connect_body["authorization_url"]
        .as_str()
        .unwrap()
        .to_string();
    let ref_id = connect_body["ref_id"].as_str().unwrap().to_string();
    let state = auth_url
        .split("state=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_string();
    let callback = gateway
        .client
        .get(gateway.url(&format!("/api/oauth/callback?code={code}&state={state}")))
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::OK);
    ref_id
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
    seed_registered_agent_with_capabilities(app_state, name, &[])
}

fn seed_registered_agent_with_capabilities(
    app_state: &Arc<ghost_gateway::state::AppState>,
    name: &str,
    capabilities: &[&str],
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
            full_access: false,
            capabilities: capabilities.iter().map(|cap| (*cap).to_string()).collect(),
            skills: None,
            baseline_capabilities: capabilities.iter().map(|cap| (*cap).to_string()).collect(),
            baseline_skills: None,
            access_pullback_active: false,
            spending_cap: 10.0,
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

async fn seed_studio_session(
    app_state: &Arc<ghost_gateway::state::AppState>,
    session_id: &str,
    agent_id: &str,
) {
    let db = app_state.db.write().await;
    cortex_storage::queries::studio_chat_queries::create_session(
        &db,
        session_id,
        agent_id,
        "New Chat",
        "test-model",
        "",
        0.5,
        512,
    )
    .unwrap();
}

fn studio_message_request_body(content: &str, session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "content": content,
        "model": serde_json::Value::Null,
        "temperature": serde_json::Value::Null,
        "max_tokens": serde_json::Value::Null,
    })
}

fn agent_chat_request_body(message: &str, session_id: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "message": message,
        "agent_id": serde_json::Value::Null,
        "session_id": session_id,
        "model": serde_json::Value::Null,
    })
}

async fn seed_operation_journal_in_progress(
    app_state: &Arc<ghost_gateway::state::AppState>,
    actor: &str,
    operation_id: &str,
    request_id: &str,
    idempotency_key: &str,
    route_template: &str,
    request_body: &serde_json::Value,
) -> String {
    let db = app_state.db.write().await;
    let journal_id = uuid::Uuid::now_v7().to_string();
    let request_fingerprint = ghost_gateway::api::idempotency::fingerprint_json_request(
        "POST",
        route_template,
        actor,
        request_body,
    );
    let request_body = serde_json::to_string(request_body).unwrap();
    let stale_created_at = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    let stale_lease_expires_at = (chrono::Utc::now() - chrono::Duration::minutes(4)).to_rfc3339();
    db.execute(
        "INSERT INTO operation_journal (
            id,
            actor_key,
            method,
            route_template,
            operation_id,
            request_id,
            idempotency_key,
            request_fingerprint,
            request_body,
            status,
            created_at,
            last_seen_at,
            lease_expires_at,
            owner_token,
            lease_epoch
         ) VALUES (?1, ?2, 'POST', ?3, ?4, ?5, ?6, ?7, ?8, 'in_progress', ?9, ?9, ?10, ?11, 0)",
        rusqlite::params![
            journal_id,
            actor,
            route_template,
            operation_id,
            request_id,
            idempotency_key,
            request_fingerprint,
            request_body,
            stale_created_at,
            stale_lease_expires_at,
            format!("seed-owner-{journal_id}"),
        ],
    )
    .unwrap();
    journal_id
}

async fn seed_live_execution_record(
    app_state: &Arc<ghost_gateway::state::AppState>,
    execution_id: &str,
    journal_id: &str,
    operation_id: &str,
    route_kind: &str,
    actor: &str,
    status: &str,
    state_json: &str,
) {
    seed_live_execution_record_with_version(
        app_state,
        execution_id,
        journal_id,
        operation_id,
        route_kind,
        actor,
        1,
        status,
        state_json,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn seed_live_execution_record_with_version(
    app_state: &Arc<ghost_gateway::state::AppState>,
    execution_id: &str,
    journal_id: &str,
    operation_id: &str,
    route_kind: &str,
    actor: &str,
    state_version: i64,
    status: &str,
    state_json: &str,
) {
    let db = app_state.db.write().await;
    cortex_storage::queries::live_execution_queries::insert(
        &db,
        &cortex_storage::queries::live_execution_queries::NewLiveExecutionRecord {
            id: execution_id,
            journal_id,
            operation_id,
            route_kind,
            actor_key: actor,
            state_version,
            status,
            state_json,
        },
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn seed_workflow_execution_record(
    app_state: &Arc<ghost_gateway::state::AppState>,
    execution_id: &str,
    workflow_id: &str,
    workflow_name: &str,
    journal_id: &str,
    operation_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    state_version: i64,
    status: &str,
    current_step_index: Option<i64>,
    current_node_id: Option<&str>,
    recovery_action: Option<&str>,
    state_json: &str,
    final_response_status: Option<i64>,
    final_response_body: Option<&str>,
    started_at: &str,
    completed_at: Option<&str>,
) {
    let db = app_state.db.write().await;
    cortex_storage::queries::workflow_execution_queries::insert(
        &db,
        &cortex_storage::queries::workflow_execution_queries::NewWorkflowExecutionRow {
            id: execution_id,
            workflow_id,
            workflow_name,
            journal_id,
            operation_id,
            owner_token,
            lease_epoch,
            state_version,
            status,
            current_step_index,
            current_node_id,
            recovery_action,
            state: state_json,
            final_response_status,
            final_response_body,
            started_at,
            completed_at,
            updated_at: started_at,
        },
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
        Self::start_with_runtime_config(
            db_path,
            std::collections::BTreeMap::new(),
            Vec::new(),
            None,
        )
        .await
    }

    async fn start_with_oauth_providers(
        db_path: &Path,
        oauth_providers: std::collections::BTreeMap<String, Box<dyn ghost_oauth::OAuthProvider>>,
    ) -> Self {
        Self::start_with_runtime_config(db_path, oauth_providers, Vec::new(), None).await
    }

    async fn start_with_model_providers(
        db_path: &Path,
        model_providers: Vec<ghost_gateway::config::ProviderConfig>,
    ) -> Self {
        Self::start_with_model_providers_and_tools(
            db_path,
            model_providers,
            ghost_gateway::config::ToolsConfig::default(),
        )
        .await
    }

    async fn start_with_model_providers_and_tools(
        db_path: &Path,
        model_providers: Vec<ghost_gateway::config::ProviderConfig>,
        tools_config: ghost_gateway::config::ToolsConfig,
    ) -> Self {
        let default_model_provider = model_providers
            .first()
            .map(|provider| provider.name.clone());
        Self::start_with_runtime_config_and_tools(
            db_path,
            std::collections::BTreeMap::new(),
            model_providers,
            default_model_provider,
            tools_config,
        )
        .await
    }

    async fn start_with_runtime_config(
        db_path: &Path,
        oauth_providers: std::collections::BTreeMap<String, Box<dyn ghost_oauth::OAuthProvider>>,
        model_providers: Vec<ghost_gateway::config::ProviderConfig>,
        default_model_provider: Option<String>,
    ) -> Self {
        Self::start_with_runtime_config_and_tools(
            db_path,
            oauth_providers,
            model_providers,
            default_model_provider,
            ghost_gateway::config::ToolsConfig::default(),
        )
        .await
    }

    async fn start_with_runtime_config_and_tools(
        db_path: &Path,
        oauth_providers: std::collections::BTreeMap<String, Box<dyn ghost_oauth::OAuthProvider>>,
        model_providers: Vec<ghost_gateway::config::ProviderConfig>,
        default_model_provider: Option<String>,
        tools_config: ghost_gateway::config::ToolsConfig,
    ) -> Self {
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
        let kill_switch = Arc::new(ghost_gateway::safety::kill_switch::KillSwitch::new());
        let cost_tracker = Arc::new(ghost_gateway::cost::tracker::CostTracker::new());
        let secret_provider: Box<dyn ghost_secrets::SecretProvider> =
            Box::new(ghost_secrets::EnvProvider);
        let oauth_store_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("oauth-store");
        let token_store =
            ghost_oauth::TokenStore::new(oauth_store_dir, Box::new(ghost_secrets::EnvProvider));
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(oauth_providers, token_store));
        let embedding_engine =
            cortex_embeddings::EmbeddingEngine::new(cortex_embeddings::EmbeddingConfig::default());

        let app_state = Arc::new(ghost_gateway::state::AppState {
            gateway: Arc::clone(&shared_state),
            config_path: std::path::PathBuf::from("ghost.yml"),
            agents: Arc::new(RwLock::new(
                ghost_gateway::agents::registry::AgentRegistry::new(),
            )),
            kill_switch,
            quarantine: Arc::new(RwLock::new(
                ghost_gateway::safety::quarantine::QuarantineManager::new(),
            )),
            db: Arc::clone(&db),
            event_tx,
            trigger_sender:
                tokio::sync::mpsc::channel::<cortex_core::safety::trigger::TriggerEvent>(16).0,
            replay_buffer,
            cost_tracker,
            kill_gate: None,
            secret_provider: Arc::from(secret_provider),
            oauth_broker,
            mesh_signing_key: None,
            soul_drift_threshold: 0.15,
            convergence_profile: "standard".to_string(),
            model_providers,
            default_model_provider,
            pc_control_circuit_breaker: ghost_pc_control::safety::PcControlConfig::default()
                .circuit_breaker(),
            websocket_auth_tickets: Arc::new(dashmap::DashMap::new()),
            ws_ticket_auth_only: config.gateway.ws_ticket_auth_only,
            tools_config,
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
            distributed_kill_enabled: false,
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            skill_catalog: Arc::new(
                ghost_gateway::skill_catalog::SkillCatalogService::empty_for_tests(Arc::clone(&db)),
            ),
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: 90,
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

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
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
            owner_token: "journal-progress-owner",
            lease_epoch: 0,
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
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
async fn execute_workflow_fails_closed_on_unknown_node_types() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_workflow(
        &gateway.app_state,
        "workflow-unknown-node",
        "Unknown node workflow",
        serde_json::json!([{ "id": "n1", "type": "mystery" }]),
        serde_json::json!([]),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/workflows/workflow-unknown-node/execute"))
        .json(&serde_json::json!({ "input": { "hello": "world" } }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-d234567890ac",
        )
        .header("idempotency-key", "workflow-unknown-node-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "failed");
}

#[tokio::test]
async fn workflow_resume_requires_existing_execution_record() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_workflow(
        &gateway.app_state,
        "workflow-resume-missing",
        "Resume missing workflow",
        serde_json::json!([{ "id": "n1", "type": "transform" }]),
        serde_json::json!([]),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/workflows/workflow-resume-missing/resume/execution-123"))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-d234567890ad",
        )
        .header("idempotency-key", "workflow-resume-missing-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        None
    );
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn workflow_resume_replays_safe_inflight_step_after_crash() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    seed_workflow(
        &gateway.app_state,
        "workflow-resume-safe",
        "Resume safe workflow",
        serde_json::json!([
            { "id": "wait1", "type": "wait", "wait_ms": 1 },
            { "id": "done", "type": "transform" }
        ]),
        serde_json::json!([{ "source": "wait1", "target": "done" }]),
    )
    .await;

    let execution_id = "018f0f23-8c65-7abc-9def-d234567890ae";
    let state_json = serde_json::json!({
        "version": 2,
        "execution_id": execution_id,
        "workflow_id": "workflow-resume-safe",
        "workflow_name": "Resume safe workflow",
        "input": { "hello": "world" },
        "nodes": [
            { "id": "wait1", "type": "wait", "wait_ms": 1 },
            { "id": "done", "type": "transform" }
        ],
        "edges": [{ "source": "wait1", "target": "done" }],
        "order": ["wait1", "done"],
        "node_states": {},
        "node_outputs": {},
        "next_step_index": 0,
        "active_step": {
            "step_index": 0,
            "node_id": "wait1",
            "node_type": "wait",
            "started_at": "2026-03-01T00:00:00Z",
            "retry_safe": true
        },
        "started_at": "2026-03-01T00:00:00Z",
        "completed_at": null,
        "final_status": null,
        "final_response_status": null,
        "final_response_body": null,
        "recovery_required": false,
        "recovery_action": null,
        "recovery_reason": null
    })
    .to_string();
    seed_workflow_execution_record(
        &gateway.app_state,
        execution_id,
        "workflow-resume-safe",
        "Resume safe workflow",
        "old-journal-safe",
        "old-op-safe",
        "old-owner-safe",
        0,
        2,
        "running",
        Some(0),
        Some("wait1"),
        None,
        &state_json,
        None,
        None,
        "2026-03-01T00:00:00Z",
        None,
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url(&format!(
            "/api/workflows/workflow-resume-safe/resume/{execution_id}"
        )))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-d234567890af",
        )
        .header("idempotency-key", "workflow-resume-safe-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "completed");
    assert_eq!(body["execution_id"], execution_id);
    assert_eq!(body["steps"].as_array().unwrap().len(), 2);

    let db = gateway.app_state.db.read().unwrap();
    let row = cortex_storage::queries::workflow_execution_queries::get_by_id(&db, execution_id)
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "completed");
    assert_eq!(row.final_response_status, Some(200));
}

#[tokio::test]
async fn workflow_resume_fails_closed_for_non_retry_safe_inflight_step() {
    let _guard = hold_env_lock();
    let _openai_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("workflow-resume-unsafe.db");
    let gateway = PersistentGateway::start_with_runtime_config(
        &db_path,
        std::collections::BTreeMap::new(),
        openai_compat_provider_configs(&provider.base_url),
        Some("openai_compat".into()),
    )
    .await;

    let execution_id = "018f0f23-8c65-7abc-9def-d234567890b0";
    let state_json = serde_json::json!({
        "version": 2,
        "execution_id": execution_id,
        "workflow_id": "workflow-resume-unsafe",
        "workflow_name": "Resume unsafe workflow",
        "input": { "hello": "world" },
        "nodes": [
            { "id": "llm1", "type": "llm_call", "prompt": "Say hi" },
            { "id": "done", "type": "transform" }
        ],
        "edges": [{ "source": "llm1", "target": "done" }],
        "order": ["llm1", "done"],
        "node_states": {},
        "node_outputs": {},
        "next_step_index": 0,
        "active_step": {
            "step_index": 0,
            "node_id": "llm1",
            "node_type": "llm_call",
            "started_at": "2026-03-01T00:00:00Z",
            "retry_safe": false
        },
        "started_at": "2026-03-01T00:00:00Z",
        "completed_at": null,
        "final_status": null,
        "final_response_status": null,
        "final_response_body": null,
        "recovery_required": false,
        "recovery_action": null,
        "recovery_reason": null
    })
    .to_string();
    seed_workflow_execution_record(
        &gateway.app_state,
        execution_id,
        "workflow-resume-unsafe",
        "Resume unsafe workflow",
        "old-journal-unsafe",
        "old-op-unsafe",
        "old-owner-unsafe",
        0,
        2,
        "running",
        Some(0),
        Some("llm1"),
        None,
        &state_json,
        None,
        None,
        "2026-03-01T00:00:00Z",
        None,
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url(&format!(
            "/api/workflows/workflow-resume-unsafe/resume/{execution_id}"
        )))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-d234567890b1",
        )
        .header("idempotency-key", "workflow-resume-unsafe-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["status"], "recovery_required");
    assert_eq!(body["recovery_action"], "manual_recovery_required");
    assert_eq!(provider.hits.load(Ordering::SeqCst), 0);

    {
        let db = gateway.app_state.db.read().unwrap();
        let row = cortex_storage::queries::workflow_execution_queries::get_by_id(&db, execution_id)
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "recovery_required");
        assert_eq!(row.final_response_status, Some(409));
    }

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn execute_workflow_retry_resumes_from_durable_progress_without_rerunning_completed_steps() {
    let _guard = hold_env_lock();
    let _openai_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("workflow-retry-resume.db");
    let gateway = PersistentGateway::start_with_runtime_config(
        &db_path,
        std::collections::BTreeMap::new(),
        openai_compat_provider_configs(&provider.base_url),
        Some("openai_compat".into()),
    )
    .await;

    seed_workflow(
        &gateway.app_state,
        "workflow-retry-progress",
        "Workflow retry progress",
        serde_json::json!([
            { "id": "llm1", "type": "llm_call", "prompt": "Say hi" },
            { "id": "done", "type": "transform" }
        ]),
        serde_json::json!([{ "source": "llm1", "target": "done" }]),
    )
    .await;

    let request_body = serde_json::json!({
        "workflow_id": "workflow-retry-progress",
        "input": { "hello": "world" }
    });
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        "anonymous",
        "old-op-retry",
        "old-request-retry",
        "workflow-retry-progress-key",
        "/api/workflows/:id/execute",
        &request_body,
    )
    .await;
    let execution_id = "018f0f23-8c65-7abc-9def-d234567890b2";
    let state_json = serde_json::json!({
        "version": 2,
        "execution_id": execution_id,
        "workflow_id": "workflow-retry-progress",
        "workflow_name": "Workflow retry progress",
        "input": { "hello": "world" },
        "nodes": [
            { "id": "llm1", "type": "llm_call", "prompt": "Say hi" },
            { "id": "done", "type": "transform" }
        ],
        "edges": [{ "source": "llm1", "target": "done" }],
        "order": ["llm1", "done"],
        "node_states": {
            "llm1": {
                "node_id": "llm1",
                "node_type": "llm_call",
                "status": "completed",
                "result": { "status": "completed", "tokens": 7 },
                "started_at": "2026-03-01T00:00:00Z",
                "completed_at": "2026-03-01T00:00:01Z"
            }
        },
        "node_outputs": {
            "llm1": { "text": "from prior durable output" }
        },
        "next_step_index": 1,
        "active_step": null,
        "started_at": "2026-03-01T00:00:00Z",
        "completed_at": null,
        "final_status": null,
        "final_response_status": null,
        "final_response_body": null,
        "recovery_required": false,
        "recovery_action": null,
        "recovery_reason": null
    })
    .to_string();
    seed_workflow_execution_record(
        &gateway.app_state,
        execution_id,
        "workflow-retry-progress",
        "Workflow retry progress",
        &journal_id,
        "old-op-retry",
        &format!("seed-owner-{journal_id}"),
        0,
        2,
        "running",
        Some(1),
        Some("done"),
        None,
        &state_json,
        None,
        None,
        "2026-03-01T00:00:00Z",
        None,
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/workflows/workflow-retry-progress/execute"))
        .json(&serde_json::json!({ "input": { "hello": "world" } }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-d234567890b3",
        )
        .header("idempotency-key", "workflow-retry-progress-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "completed");
    assert_eq!(body["execution_id"], execution_id);
    assert_eq!(provider.hits.load(Ordering::SeqCst), 0);

    {
        let db = gateway.app_state.db.read().unwrap();
        let row = cortex_storage::queries::workflow_execution_queries::get_by_id(&db, execution_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            row.operation_id.as_deref(),
            Some("018f0f23-8c65-7abc-9def-d234567890b3")
        );
        assert_eq!(row.status, "completed");
    }

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn workflow_resume_rejects_legacy_state_versions() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    let execution_id = "018f0f23-8c65-7abc-9def-d234567890b4";
    seed_workflow_execution_record(
        &gateway.app_state,
        execution_id,
        "workflow-legacy-state",
        "Workflow legacy state",
        "legacy-journal",
        "legacy-op",
        "legacy-owner",
        0,
        0,
        "recovery_required",
        None,
        None,
        Some("legacy_state_upgrade_required"),
        "{\"version\":1}",
        None,
        None,
        "2026-03-01T00:00:00Z",
        None,
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url(&format!(
            "/api/workflows/workflow-legacy-state/resume/{execution_id}"
        )))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-d234567890b5",
        )
        .header("idempotency-key", "workflow-legacy-state-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["status"], "recovery_required");
    assert_eq!(body["recovery_action"], "legacy_state_upgrade_required");
}

#[tokio::test]
async fn create_workflow_rejects_non_array_graph_payload() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;

    let response = gateway
        .client
        .post(gateway.url("/api/workflows"))
        .json(&serde_json::json!({
            "name": "invalid-graph",
            "nodes": { "id": "n1", "type": "transform" },
            "edges": [],
        }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-d234567890ae",
        )
        .header("idempotency-key", "workflow-invalid-graph-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(body["error"]["message"], "workflow nodes must be an array");
}

#[tokio::test]
async fn get_workflow_fails_closed_on_corrupt_persisted_graph_json() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    {
        let db = gateway.app_state.db.write().await;
        db.execute(
            "INSERT INTO workflows (id, name, description, nodes, edges, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "workflow-corrupt-row",
                "Corrupt workflow",
                "bad persisted graph",
                "{not-json",
                "[]",
                "test-operator",
            ],
        )
        .unwrap();
    }

    let get_response = gateway
        .client
        .get(gateway.url("/api/workflows/workflow-corrupt-row"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let get_body = json_body(get_response).await;
    assert_eq!(get_body["error"]["code"], "INTERNAL_ERROR");
    assert_eq!(get_body["error"]["message"], "An internal error occurred");

    let list_response = gateway
        .client
        .get(gateway.url("/api/workflows"))
        .send()
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let list_body = json_body(list_response).await;
    assert_eq!(list_body["error"]["code"], "INTERNAL_ERROR");
    assert_eq!(list_body["error"]["message"], "An internal error occurred");
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
async fn delete_bookmark_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    {
        let db = gateway.app_state.db.write().await;
        db.execute(
            "INSERT INTO session_bookmarks (id, session_id, event_index, label) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["bookmark-delete-target", "session-bookmarks", 3u32, "remove me"],
        )
        .unwrap();
    }

    let operation_id = "018f0f23-8c65-7abc-9def-1454567890ab";
    let idempotency_key = "bookmark-delete-key";
    let first = gateway
        .client
        .delete(gateway.url("/api/sessions/session-bookmarks/bookmarks/bookmark-delete-target"))
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

    let replay = gateway
        .client
        .delete(gateway.url("/api/sessions/session-bookmarks/bookmarks/bookmark-delete-target"))
        .header("x-request-id", "bookmark-delete-retry")
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

    let db = gateway.app_state.db.read().unwrap();
    let bookmark_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM session_bookmarks WHERE id = ?1",
            ["bookmark-delete-target"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bookmark_count, 0);

    let audit_rows: Vec<(Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'delete_session_bookmark'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("bookmark-delete-retry"));
    assert_eq!(audit_rows[1].1.as_deref(), Some("replayed"));
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
    {
        let db = gateway.app_state.db.write().await;
        db.execute(
            "INSERT INTO discovered_agents (name, description, endpoint_url, capabilities, trust_score, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "mesh-agent",
                "Verified test agent",
                capture.url,
                "[]",
                1.0_f64,
                "1.0.0",
            ],
        )
        .unwrap();
    }
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
    let captured_requests = capture.requests();
    assert_eq!(captured_requests.len(), 1);
    assert_eq!(captured_requests[0].path, "/a2a");
    assert!(
        captured_requests[0]
            .signature
            .as_ref()
            .is_some_and(|value| !value.is_empty()),
        "A2A dispatch should include X-Ghost-Signature"
    );

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
        .query_row(
            "SELECT COUNT(*) FROM a2a_tasks WHERE id = ?1",
            [operation_id],
            |row| row.get(0),
        )
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

#[tokio::test]
async fn send_a2a_task_rejects_unverified_discovered_agent() {
    let _guard = hold_env_lock();
    let _allow_local = EnvVarGuard::set("GHOST_WEBHOOK_ALLOWED_HOSTS", "127.0.0.1");
    let capture = CaptureServer::start().await;
    let gateway = TestGateway::start().await;
    {
        let db = gateway.app_state.db.write().await;
        db.execute(
            "INSERT INTO discovered_agents (name, description, endpoint_url, capabilities, trust_score, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "mesh-agent",
                "Unverified test agent",
                capture.url,
                "[]",
                0.0_f64,
                "1.0.0",
            ],
        )
        .unwrap();
    }

    let response = gateway
        .client
        .post(gateway.url("/api/a2a/tasks"))
        .json(&serde_json::json!({
            "target_url": capture.url,
            "target_agent": "mesh-agent",
            "input": { "task": "ping" },
            "method": "tasks/send"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);

    let error: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        error["error"]["code"].as_str(),
        Some("A2A_TARGET_UNVERIFIED")
    );
    assert_eq!(capture.hits.load(Ordering::SeqCst), 0);

    capture.stop().await;
}

#[tokio::test]
async fn delete_studio_session_replays_committed_response_and_records_audit_provenance() {
    let _guard = hold_env_lock();
    let gateway = TestGateway::start().await;
    {
        let db = gateway.app_state.db.write().await;
        cortex_storage::queries::studio_chat_queries::create_session(
            &db,
            "studio-delete-target",
            "00000000-0000-0000-0000-000000000001",
            "Delete me",
            "qwen3.5:9b",
            "",
            0.5,
            512,
        )
        .unwrap();
    }

    let operation_id = "018f0f23-8c65-7abc-9def-1734567890ab";
    let idempotency_key = "studio-delete-key";
    let first = gateway
        .client
        .delete(gateway.url("/api/studio/sessions/studio-delete-target"))
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

    let replay = gateway
        .client
        .delete(gateway.url("/api/studio/sessions/studio-delete-target"))
        .header("x-request-id", "studio-delete-retry")
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

    let db = gateway.app_state.db.read().unwrap();
    let session_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM studio_chat_sessions WHERE id = ?1",
            ["studio-delete-target"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(session_count, 0);

    let audit_rows: Vec<(Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'delete_studio_session'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("studio-delete-retry"));
    assert_eq!(audit_rows[1].1.as_deref(), Some("replayed"));
}

#[tokio::test]
async fn oauth_connect_replays_after_restart_and_callback_survives_restart() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-connect.db");
    let counters = Arc::new(OAuthProviderCounters::default());

    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let body = serde_json::json!({
        "provider": "mock",
        "scopes": ["read"],
        "redirect_uri": "http://localhost/cb",
    });
    let operation_id = "018f0f23-8c65-7abc-9def-1834567890ab";
    let idempotency_key = "oauth-connect-key";

    let first = gateway
        .client
        .post(format!("{}/api/oauth/connect", gateway.base_url))
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
    let first_body = json_body(first).await;
    let auth_url = first_body["authorization_url"]
        .as_str()
        .unwrap()
        .to_string();
    let ref_id = first_body["ref_id"].as_str().unwrap().to_string();
    gateway.stop().await;

    let restarted = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let replay = restarted
        .client
        .post(format!("{}/api/oauth/connect", restarted.base_url))
        .json(&body)
        .header("x-request-id", "oauth-connect-retry")
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
    let replay_body = json_body(replay).await;
    assert_eq!(replay_body["authorization_url"], auth_url);
    assert_eq!(replay_body["ref_id"], ref_id);

    let state = auth_url
        .split("state=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    let callback = restarted
        .client
        .get(format!(
            "{}/api/oauth/callback?code=auth-code-123&state={state}",
            restarted.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::OK);
    let callback_body = json_body(callback).await;
    assert_eq!(callback_body["status"], "connected");
    assert_eq!(callback_body["ref_id"], ref_id);

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'oauth_connect'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("oauth-connect-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);
    restarted.stop().await;
}

#[tokio::test]
async fn oauth_connect_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-connect-mismatch.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway =
        PersistentGateway::start_with_oauth_providers(&db_path, oauth_provider_map(counters)).await;
    let key = "oauth-connect-mismatch-key";

    let first = gateway
        .client
        .post(format!("{}/api/oauth/connect", gateway.base_url))
        .json(&serde_json::json!({
            "provider": "mock",
            "scopes": ["read"],
            "redirect_uri": "http://localhost/cb",
        }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1934567890ab",
        )
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let conflict = gateway
        .client
        .post(format!("{}/api/oauth/connect", gateway.base_url))
        .json(&serde_json::json!({
            "provider": "mock",
            "scopes": ["calendar"],
            "redirect_uri": "http://localhost/cb",
        }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1a34567890ab",
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn oauth_callback_invalid_params_return_error_envelope() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-callback-invalid.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;

    let missing_state = gateway
        .client
        .get(gateway.url("/api/oauth/callback?code=auth-code-123&state="))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_state.status(), StatusCode::BAD_REQUEST);
    let missing_state_body = json_body(missing_state).await;
    assert_eq!(missing_state_body["error"]["code"], "OAUTH_INVALID_STATE");
    assert_eq!(missing_state_body["error"]["message"], "invalid state");

    let missing_code = gateway
        .client
        .get(gateway.url("/api/oauth/callback?code=&state=test-state"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_code.status(), StatusCode::BAD_REQUEST);
    let missing_code_body = json_body(missing_code).await;
    assert_eq!(missing_code_body["error"]["code"], "OAUTH_CODE_REQUIRED");
    assert_eq!(missing_code_body["error"]["message"], "missing code");

    gateway.stop().await;
}

#[tokio::test]
async fn oauth_disconnect_replays_after_restart_and_does_not_double_revoke() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-disconnect.db");
    let counters = Arc::new(OAuthProviderCounters::default());

    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let connect = gateway
        .client
        .post(format!("{}/api/oauth/connect", gateway.base_url))
        .json(&serde_json::json!({
            "provider": "mock",
            "scopes": ["read"],
            "redirect_uri": "http://localhost/cb",
        }))
        .send()
        .await
        .unwrap();
    let connect_body = json_body(connect).await;
    let auth_url = connect_body["authorization_url"]
        .as_str()
        .unwrap()
        .to_string();
    let ref_id = connect_body["ref_id"].as_str().unwrap().to_string();
    let state = auth_url
        .split("state=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_string();
    let callback = gateway
        .client
        .get(format!(
            "{}/api/oauth/callback?code=auth-code-456&state={state}",
            gateway.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::OK);

    let operation_id = "018f0f23-8c65-7abc-9def-1b34567890ab";
    let idempotency_key = "oauth-disconnect-key";
    let first = gateway
        .client
        .delete(format!(
            "{}/api/oauth/connections/{ref_id}",
            gateway.base_url
        ))
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
    assert_eq!(counters.revoke_hits.load(Ordering::SeqCst), 1);
    gateway.stop().await;

    let restarted = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let replay = restarted
        .client
        .delete(format!(
            "{}/api/oauth/connections/{ref_id}",
            restarted.base_url
        ))
        .header("x-request-id", "oauth-disconnect-retry")
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
    assert_eq!(counters.revoke_hits.load(Ordering::SeqCst), 1);

    let connections = restarted
        .client
        .get(format!("{}/api/oauth/connections", restarted.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(connections.status(), StatusCode::OK);
    assert_eq!(json_body(connections).await, serde_json::json!([]));

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'oauth_disconnect'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("oauth-disconnect-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);
    restarted.stop().await;
}

#[tokio::test]
async fn oauth_disconnect_idempotency_key_reuse_with_different_ref_id_conflicts() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-disconnect-mismatch.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway =
        PersistentGateway::start_with_oauth_providers(&db_path, oauth_provider_map(counters)).await;

    let mut ref_ids = Vec::new();
    for code in ["auth-code-a", "auth-code-b"] {
        let connect = gateway
            .client
            .post(format!("{}/api/oauth/connect", gateway.base_url))
            .json(&serde_json::json!({
                "provider": "mock",
                "scopes": ["read"],
                "redirect_uri": "http://localhost/cb",
            }))
            .send()
            .await
            .unwrap();
        let connect_body = json_body(connect).await;
        let auth_url = connect_body["authorization_url"]
            .as_str()
            .unwrap()
            .to_string();
        let ref_id = connect_body["ref_id"].as_str().unwrap().to_string();
        let state = auth_url
            .split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_string();
        let callback = gateway
            .client
            .get(format!(
                "{}/api/oauth/callback?code={code}&state={state}",
                gateway.base_url
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(callback.status(), StatusCode::OK);
        ref_ids.push(ref_id);
    }

    let key = "oauth-disconnect-mismatch-key";
    let first = gateway
        .client
        .delete(format!(
            "{}/api/oauth/connections/{}",
            gateway.base_url, ref_ids[0]
        ))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1c34567890ab",
        )
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let conflict = gateway
        .client
        .delete(format!(
            "{}/api/oauth/connections/{}",
            gateway.base_url, ref_ids[1]
        ))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1d34567890ab",
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn oauth_connect_rejects_empty_provider() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-connect-invalid.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway =
        PersistentGateway::start_with_oauth_providers(&db_path, oauth_provider_map(counters)).await;

    let response = gateway
        .client
        .post(format!("{}/api/oauth/connect", gateway.base_url))
        .json(&serde_json::json!({
            "provider": "",
            "scopes": ["read"],
            "redirect_uri": "http://localhost/cb",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "OAUTH_PROVIDER_REQUIRED"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn oauth_disconnect_rejects_invalid_ref_id() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-disconnect-invalid.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway =
        PersistentGateway::start_with_oauth_providers(&db_path, oauth_provider_map(counters)).await;

    let response = gateway
        .client
        .delete(format!(
            "{}/api/oauth/connections/not-a-uuid",
            gateway.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "OAUTH_INVALID_REF_ID"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn oauth_execute_replays_after_restart_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-execute-success.db");
    let counters = Arc::new(OAuthProviderCounters::default());

    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let ref_id = oauth_connect_ref_id(&gateway, "oauth-execute-success-code").await;
    let body = oauth_execute_request_body(&ref_id, "POST", "https://mock.example.com/tasks");
    let operation_id = "018f0f23-8c65-7abc-9def-2a34567890ab";
    let idempotency_key = "oauth-execute-success-key";

    let first = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
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
    let first_body = json_body(first).await;
    assert_eq!(first_body["status"], 200);
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 1);
    gateway.stop().await;

    let restarted = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let replay = restarted
        .client
        .post(restarted.url("/api/oauth/execute"))
        .json(&body)
        .header("x-request-id", "oauth-execute-success-retry")
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
    assert_eq!(json_body(replay).await, first_body);
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 1);

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'oauth_execute_api_call'
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
    assert_eq!(
        audit_rows[1].0.as_deref(),
        Some("oauth-execute-success-retry")
    );
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);
    restarted.stop().await;
}

#[tokio::test]
async fn oauth_execute_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-execute-mismatch.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let ref_id = oauth_connect_ref_id(&gateway, "oauth-execute-mismatch-code").await;
    let key = "oauth-execute-mismatch-key";

    let first = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
        .json(&oauth_execute_request_body(
            &ref_id,
            "POST",
            "https://mock.example.com/tasks",
        ))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-2b34567890ab",
        )
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 1);

    let conflict = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
        .json(&oauth_execute_request_body(
            &ref_id,
            "POST",
            "https://mock.example.com/other-task",
        ))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-2c34567890ab",
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 1);
    gateway.stop().await;
}

#[tokio::test]
async fn oauth_execute_rejects_invalid_method() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-execute-invalid-method.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
        .json(&oauth_execute_request_body(
            &uuid::Uuid::now_v7().to_string(),
            "TRACE",
            "https://mock.example.com/tasks",
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "OAUTH_EXECUTE_INVALID_METHOD"
    );
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 0);
    gateway.stop().await;
}

#[tokio::test]
async fn oauth_execute_replays_committed_not_connected_failure_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-execute-not-connected.db");
    let counters = Arc::new(OAuthProviderCounters::default());

    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let body = oauth_execute_request_body(
        &uuid::Uuid::now_v7().to_string(),
        "POST",
        "https://mock.example.com/tasks",
    );
    let operation_id = "018f0f23-8c65-7abc-9def-2d34567890ab";
    let idempotency_key = "oauth-execute-not-connected-key";

    let first = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    assert!(json_body(first).await["error"]
        .as_str()
        .unwrap()
        .contains("not connected"));
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 0);
    gateway.stop().await;

    let restarted = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let replay = restarted
        .client
        .post(restarted.url("/api/oauth/execute"))
        .json(&body)
        .header("x-request-id", "oauth-execute-not-connected-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );
    assert!(json_body(replay).await["error"]
        .as_str()
        .unwrap()
        .contains("not connected"));
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 0);

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'oauth_execute_api_call'
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
    assert_eq!(
        audit_rows[1].0.as_deref(),
        Some("oauth-execute-not-connected-retry")
    );
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);
    restarted.stop().await;
}

#[tokio::test]
async fn oauth_execute_provider_error_returns_accepted_and_replay_does_not_refire_provider() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-execute-provider-error.db");
    let counters = Arc::new(OAuthProviderCounters::default());

    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let ref_id = oauth_connect_ref_id(&gateway, "oauth-execute-provider-error-code").await;
    let body =
        oauth_execute_request_body(&ref_id, "POST", "https://mock.example.com/provider-error");
    let operation_id = "018f0f23-8c65-7abc-9def-2e34567890ab";
    let idempotency_key = "oauth-execute-provider-error-key";

    let first = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
        .json(&body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let first_body = json_body(first).await;
    assert_eq!(first_body["status"], "accepted");
    assert_eq!(first_body["ref_id"], ref_id);
    assert_eq!(first_body["recovery_required"], true);
    let execution_id = first_body["execution_id"].as_str().unwrap().to_string();
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 1);

    let execution = fetch_live_execution(&gateway.client, &gateway.base_url, &execution_id).await;
    assert_eq!(execution.status(), StatusCode::OK);
    let execution_body = json_body(execution).await;
    assert_eq!(execution_body["execution_id"], execution_id);
    assert_eq!(execution_body["route_kind"], "oauth_execute_api_call");
    assert_eq!(execution_body["status"], "recovery_required");
    assert_eq!(execution_body["recovery_required"], true);
    assert_eq!(execution_body["accepted_response"]["ref_id"], ref_id);
    assert_eq!(
        execution_body["result_status_code"],
        serde_json::Value::Null
    );
    assert_eq!(execution_body["result_body"], serde_json::Value::Null);
    gateway.stop().await;

    let restarted = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let replay = restarted
        .client
        .post(restarted.url("/api/oauth/execute"))
        .json(&body)
        .header("x-request-id", "oauth-execute-provider-error-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );
    assert_eq!(json_body(replay).await, first_body);
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 1);

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'oauth_execute_api_call'
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
    assert_eq!(
        audit_rows[1].0.as_deref(),
        Some("oauth-execute-provider-error-retry")
    );
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);
    restarted.stop().await;
}

#[tokio::test]
async fn oauth_execute_retry_after_accepted_takeover_executes_once() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-execute-accepted.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let actor = "anonymous";
    let ref_id = oauth_connect_ref_id(&gateway, "oauth-execute-accepted-code").await;
    let request_body =
        oauth_execute_request_body(&ref_id, "POST", "https://mock.example.com/tasks");
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        actor,
        "018f0f23-8c65-7abc-9def-2f34567890ab",
        "oauth-execute-accepted-request",
        "oauth-execute-accepted-key",
        "/api/oauth/execute",
        &request_body,
    )
    .await;
    let execution_id = uuid::Uuid::now_v7().to_string();
    seed_live_execution_record(
        &gateway.app_state,
        &execution_id,
        &journal_id,
        "018f0f23-8c65-7abc-9def-2f34567890ab",
        "oauth_execute_api_call",
        actor,
        "accepted",
        &serde_json::json!({
            "version": 1,
            "ref_id": ref_id,
            "accepted_response": {
                "status": "accepted",
                "ref_id": ref_id,
                "execution_id": execution_id,
            },
            "final_status_code": serde_json::Value::Null,
            "final_response": serde_json::Value::Null,
        })
        .to_string(),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
        .json(&request_body)
        .header("x-request-id", "oauth-execute-accepted-retry")
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-3034567890ab",
        )
        .header("idempotency-key", "oauth-execute-accepted-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    assert_eq!(json_body(response).await["status"], 200);
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 1);

    let execution = fetch_live_execution(&gateway.client, &gateway.base_url, &execution_id).await;
    assert_eq!(execution.status(), StatusCode::OK);
    let execution_body = json_body(execution).await;
    assert_eq!(execution_body["route_kind"], "oauth_execute_api_call");
    assert_eq!(execution_body["status"], "completed");
    assert_eq!(execution_body["recovery_required"], false);
    assert_eq!(execution_body["result_status_code"], 200);
    assert_eq!(execution_body["result_body"]["status"], 200);
    gateway.stop().await;
}

#[tokio::test]
async fn oauth_execute_running_takeover_fails_closed_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _vault_key = EnvVarGuard::set("ghost-oauth-vault-key", "gateway-oauth-vault-key");
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oauth-execute-running.db");
    let counters = Arc::new(OAuthProviderCounters::default());
    let gateway = PersistentGateway::start_with_oauth_providers(
        &db_path,
        oauth_provider_map(Arc::clone(&counters)),
    )
    .await;
    let actor = "anonymous";
    let ref_id = oauth_connect_ref_id(&gateway, "oauth-execute-running-code").await;
    let request_body =
        oauth_execute_request_body(&ref_id, "POST", "https://mock.example.com/tasks");
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        actor,
        "018f0f23-8c65-7abc-9def-3134567890ab",
        "oauth-execute-running-request",
        "oauth-execute-running-key",
        "/api/oauth/execute",
        &request_body,
    )
    .await;
    let execution_id = uuid::Uuid::now_v7().to_string();
    seed_live_execution_record(
        &gateway.app_state,
        &execution_id,
        &journal_id,
        "018f0f23-8c65-7abc-9def-3134567890ab",
        "oauth_execute_api_call",
        actor,
        "running",
        &serde_json::json!({
            "version": 1,
            "ref_id": ref_id,
            "accepted_response": {
                "status": "accepted",
                "ref_id": ref_id,
                "execution_id": execution_id,
            },
            "final_status_code": serde_json::Value::Null,
            "final_response": serde_json::Value::Null,
        })
        .to_string(),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/oauth/execute"))
        .json(&request_body)
        .header("x-request-id", "oauth-execute-running-retry")
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-3234567890ab",
        )
        .header("idempotency-key", "oauth-execute-running-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let body = json_body(response).await;
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["recovery_required"], true);
    assert_eq!(counters.execute_hits.load(Ordering::SeqCst), 0);
    gateway.stop().await;
}

#[tokio::test]
async fn live_execution_status_hides_other_actor_records() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start(&tmp.path().join("live-execution-visibility.db")).await;
    let execution_id = "018f0f23-8c65-7abc-9def-3334567890ac";
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        "legacy-token-user",
        "018f0f23-8c65-7abc-9def-3334567890ab",
        "live-execution-visibility-request",
        "live-execution-visibility-key",
        "/api/oauth/execute",
        &serde_json::json!({
            "ref_id": "018f0f23-8c65-7abc-9def-3334567890ad",
            "api_request": {
                "method": "POST",
                "url": "https://mock.example.com/tasks",
                "headers": {},
                "body": serde_json::Value::Null,
            }
        }),
    )
    .await;
    seed_live_execution_record(
        &gateway.app_state,
        execution_id,
        &journal_id,
        "018f0f23-8c65-7abc-9def-3334567890ab",
        "oauth_execute_api_call",
        "legacy-token-user",
        "recovery_required",
        &serde_json::json!({
            "version": 1,
            "ref_id": "018f0f23-8c65-7abc-9def-3334567890ad",
            "accepted_response": {
                "status": "accepted",
                "ref_id": "018f0f23-8c65-7abc-9def-3334567890ad",
                "execution_id": execution_id,
            },
            "final_status_code": serde_json::Value::Null,
            "final_response": serde_json::Value::Null,
        })
        .to_string(),
    )
    .await;

    let response = fetch_live_execution(&gateway.client, &gateway.base_url, execution_id).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    gateway.stop().await;
}

#[tokio::test]
async fn studio_message_stream_replays_after_restart_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-stream-success.db");
    let session_id = "studio-stream-session";
    let operation_id = "018f0f23-8c65-7abc-9def-1e34567890ab";
    let idempotency_key = "studio-stream-success-key";
    let request_body = serde_json::json!({ "content": "Say hello" });

    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let first = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            gateway.base_url
        ))
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
    let first_stream = first.text().await.unwrap();
    assert!(first_stream.contains("event: stream_start"));
    assert!(first_stream.contains("event: stream_end"));
    assert!(first_stream.contains("Hello"));
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    {
        let db = gateway.app_state.db.read().unwrap();
        let messages =
            cortex_storage::queries::studio_chat_queries::list_messages(&db, session_id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "Hello world");

        let events = cortex_storage::queries::stream_event_queries::recover_events_after(
            &db,
            session_id,
            &messages[1].id,
            0,
        )
        .unwrap();
        assert!(!events.is_empty());
    }

    gateway.stop().await;

    let restarted = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    let replay = restarted
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            restarted.base_url
        ))
        .json(&request_body)
        .header("x-request-id", "studio-stream-retry")
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
    let replay_stream = replay.text().await.unwrap();
    assert!(replay_stream.contains("event: stream_start"));
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let db = restarted.app_state.db.read().unwrap();
    let messages =
        cortex_storage::queries::studio_chat_queries::list_messages(&db, session_id).unwrap();
    assert_eq!(messages.len(), 2);
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'send_studio_message_stream'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("studio-stream-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);

    restarted.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_message_stream_cancel_marks_execution_cancelled_and_stops_shell_tool() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let tmp = tempfile::tempdir().unwrap();
    let marker_path = tmp.path().join("studio-stream-cancel-marker.txt");
    let command = format!(
        "sleep 5; printf cancelled-test > \"{}\"",
        marker_path.display()
    );
    let provider =
        MockOpenAICompatToolCallServer::start_shell_then_text(&command, "should not complete")
            .await;
    let db_path = tmp.path().join("studio-stream-cancel.db");
    let session_id = "studio-stream-cancel-session";
    let operation_id = "018f0f23-8c65-7abc-9def-1e34567890ba";
    let idempotency_key = "studio-stream-cancel-key";
    let request_body = studio_message_request_body("Run the long shell command", session_id);

    let mut tools_config = ghost_gateway::config::ToolsConfig::default();
    tools_config.shell.allowed_prefixes = vec![String::new()];
    tools_config.shell.timeout_secs = 10;

    let gateway = PersistentGateway::start_with_model_providers_and_tools(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
        tools_config,
    )
    .await;
    let agent_id = seed_registered_agent_with_capabilities(
        &gateway.app_state,
        "studio-shell-agent",
        &["shell_execute"],
    );
    seed_studio_session(&gateway.app_state, session_id, &agent_id.to_string()).await;

    let stream_client = gateway.client.clone();
    let stream_url = format!(
        "{}/api/studio/sessions/{session_id}/messages/stream",
        gateway.base_url
    );
    let mut stream_task = tokio::spawn(async move {
        let response = stream_client
            .post(stream_url)
            .json(&request_body)
            .header("x-request-id", "studio-stream-cancel-request")
            .header("x-ghost-operation-id", operation_id)
            .header("idempotency-key", idempotency_key)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await.unwrap();
        (status, headers, body)
    });

    let execution_record = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            {
                let db = gateway.app_state.db.read().unwrap();
                if let Some(record) =
                    cortex_storage::queries::live_execution_queries::get_by_operation_id(
                        &db,
                        operation_id,
                    )
                    .unwrap()
                {
                    break record;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let execution_id = execution_record.id.clone();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if gateway
                .app_state
                .live_execution_controls
                .contains_key(&execution_id)
            {
                break;
            }
            if let Ok(Ok((status, _headers, body))) =
                tokio::time::timeout(std::time::Duration::from_millis(10), &mut stream_task).await
            {
                panic!(
                    "studio stream finished before control registration: status={status}, body={body}"
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();

    let cancel = gateway
        .client
        .post(format!(
            "{}/api/live-executions/{execution_id}/cancel",
            gateway.base_url
        ))
        .send()
        .await
        .unwrap();
    let cancel_status = cancel.status();
    let cancel_text = cancel.text().await.unwrap();
    assert_eq!(cancel_status, StatusCode::OK, "cancel body: {cancel_text}");
    let cancel_body: serde_json::Value = serde_json::from_str(&cancel_text).unwrap();
    assert_eq!(cancel_body["execution_id"], execution_id);
    assert_eq!(cancel_body["status"], "cancelled");
    assert_eq!(cancel_body["cancel_signal_sent"], true);

    let (stream_status, stream_headers, stream_text) =
        tokio::time::timeout(std::time::Duration::from_secs(10), stream_task)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(stream_status, StatusCode::OK);
    assert_eq!(
        stream_headers
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    assert!(
        stream_text.contains("event: stream_start"),
        "stream output: {stream_text}"
    );
    assert!(
        stream_text.contains("event: tool_use"),
        "stream output: {stream_text}"
    );
    assert!(
        stream_text.contains("\"status\":\"running\""),
        "stream output: {stream_text}"
    );
    assert!(
        stream_text.contains("event: error"),
        "stream output: {stream_text}"
    );
    assert!(
        stream_text.contains("\"cancelled\":true"),
        "stream output: {stream_text}"
    );
    assert!(
        stream_text.contains("Execution cancelled by user"),
        "stream output: {stream_text}"
    );

    let execution = fetch_live_execution(&gateway.client, &gateway.base_url, &execution_id).await;
    assert_eq!(execution.status(), StatusCode::OK);
    let execution_body = json_body(execution).await;
    assert_eq!(execution_body["route_kind"], "studio_send_message_stream");
    assert_eq!(execution_body["status"], "cancelled");
    assert_eq!(execution_body["recovery_required"], false);
    assert_eq!(
        execution_body["accepted_response"]["execution_id"],
        execution_id
    );

    {
        let db = gateway.app_state.db.read().unwrap();
        let record = cortex_storage::queries::live_execution_queries::get_by_id(&db, &execution_id)
            .unwrap()
            .unwrap();
        let state: serde_json::Value = serde_json::from_str(&record.state_json).unwrap();
        assert_eq!(state["terminal_event_type"], "error");
        assert_eq!(state["terminal_payload"]["cancelled"], true);
        assert_eq!(state["recovery_required"], false);
    }

    tokio::time::sleep(std::time::Duration::from_millis(5500)).await;
    assert!(
        !marker_path.exists(),
        "cancelled shell command still wrote marker"
    );
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_stream_recover_marks_reconstructed_suffix_and_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-stream-recover-reconstructed.db");
    let session_id = "studio-recover-session";
    let assistant_message_id = "studio-recover-assistant";
    let gateway = PersistentGateway::start(&db_path).await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let delivered_text_seq = {
        let db = gateway.app_state.db.write().await;
        cortex_storage::queries::studio_chat_queries::insert_message(
            &db,
            assistant_message_id,
            session_id,
            "assistant",
            "Hello world",
            11,
            "clean",
        )
        .unwrap();
        let start_payload = serde_json::json!({
            "session_id": session_id,
            "message_id": assistant_message_id,
        });
        cortex_storage::queries::stream_event_queries::insert_stream_event(
            &db,
            session_id,
            assistant_message_id,
            "stream_start",
            &start_payload.to_string(),
        )
        .unwrap();
        let text_payload = serde_json::json!({
            "content": "Hello ",
        });
        cortex_storage::queries::stream_event_queries::insert_stream_event(
            &db,
            session_id,
            assistant_message_id,
            "text_chunk",
            &text_payload.to_string(),
        )
        .unwrap()
    };

    let recover = gateway
        .client
        .get(format!(
            "{}/api/studio/sessions/{session_id}/stream/recover?message_id={assistant_message_id}&after_seq={delivered_text_seq}",
            gateway.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(recover.status(), StatusCode::OK);
    let recover_body = json_body(recover).await;
    let events = recover_body["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event_type"], "text_delta");
    assert_eq!(events[0]["reconstructed"], true);
    assert_eq!(events[0]["payload"]["reconstructed"], true);
    assert_eq!(events[0]["payload"]["content"], "world");
    assert_eq!(events[1]["event_type"], "stream_end");
    assert_eq!(events[1]["reconstructed"], true);
    assert_eq!(events[1]["payload"]["reconstructed"], true);
    assert_eq!(events[1]["payload"]["message_id"], assistant_message_id);

    let synthetic_terminal_seq = events[1]["seq"].as_i64().unwrap();
    let second_recover = gateway
        .client
        .get(format!(
            "{}/api/studio/sessions/{session_id}/stream/recover?message_id={assistant_message_id}&after_seq={synthetic_terminal_seq}",
            gateway.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(second_recover.status(), StatusCode::OK);
    assert_eq!(
        json_body(second_recover).await["events"],
        serde_json::json!([])
    );

    gateway.stop().await;
}

#[tokio::test]
async fn studio_message_stream_replay_marks_reconstructed_fallback_events() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-stream-replay-reconstructed.db");
    let session_id = "studio-replay-session";
    let user_message_id = "studio-replay-user";
    let assistant_message_id = "studio-replay-assistant";
    let operation_id = "018f0f23-8c65-7abc-9def-1e34567890c1";
    let idempotency_key = "studio-stream-replay-reconstructed-key";
    let request_body = serde_json::json!({ "content": "Recovered output" });
    let gateway = PersistentGateway::start(&db_path).await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let context = ghost_gateway::api::operation_context::OperationContext {
        request_id: "studio-stream-replay-reconstructed-request".to_string(),
        operation_id: Some(operation_id.to_string()),
        idempotency_key: Some(idempotency_key.to_string()),
        idempotency_status: None,
        is_mutating: true,
        client_supplied_operation_id: true,
        client_supplied_idempotency_key: true,
    };

    {
        let db = gateway.app_state.db.write().await;
        let prepared = ghost_gateway::api::idempotency::prepare_json_operation(
            &db,
            &context,
            "anonymous",
            "POST",
            "/api/studio/sessions/:id/messages/stream",
            &studio_message_request_body("Recovered output", session_id),
        )
        .unwrap();
        let ghost_gateway::api::idempotency::PreparedOperation::Acquired { lease } = prepared
        else {
            panic!("expected an acquired journal entry");
        };

        cortex_storage::queries::studio_chat_queries::insert_message(
            &db,
            user_message_id,
            session_id,
            "user",
            "Recovered output",
            0,
            "clean",
        )
        .unwrap();
        cortex_storage::queries::studio_chat_queries::insert_message(
            &db,
            assistant_message_id,
            session_id,
            "assistant",
            "Recovered final answer",
            17,
            "clean",
        )
        .unwrap();

        let start_payload = serde_json::json!({
            "session_id": session_id,
            "message_id": assistant_message_id,
        });
        let start_seq = cortex_storage::queries::stream_event_queries::insert_stream_event(
            &db,
            session_id,
            assistant_message_id,
            "stream_start",
            &start_payload.to_string(),
        )
        .unwrap();
        let accepted_body = serde_json::json!({
            "status": "accepted",
            "session_id": session_id,
            "user_message_id": user_message_id,
            "assistant_message_id": assistant_message_id,
            "stream_start_seq": start_seq,
        });
        ghost_gateway::api::idempotency::commit_prepared_json_operation(
            &db,
            &context,
            &lease,
            StatusCode::OK,
            &accepted_body,
        )
        .unwrap();
    }

    let replay = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            gateway.base_url
        ))
        .json(&request_body)
        .header("x-request-id", "studio-stream-replay-reconstructed-retry")
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
    let replay_stream = replay.text().await.unwrap();
    assert!(replay_stream.contains("event: stream_start"));
    assert!(replay_stream.contains("event: text_delta"));
    assert!(replay_stream.contains("event: stream_end"));
    assert!(replay_stream.contains("\"reconstructed\":true"));
    assert!(replay_stream.contains("Recovered final answer"));

    gateway.stop().await;
}

#[tokio::test]
async fn studio_message_stream_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-stream-mismatch.db");
    let session_id = "studio-stream-mismatch";
    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let key = "studio-stream-mismatch-key";
    let first = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": "Say hello" }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-1f34567890ab",
        )
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let _ = first.text().await.unwrap();
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let conflict = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": "Say something else" }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-2034567890ab",
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_message_stream_rejects_empty_content() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-stream-empty.db");
    let session_id = "studio-stream-empty";
    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let response = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "VALIDATION_ERROR"
    );

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_message_stream_replays_committed_failure_without_duplicate_user_message() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-stream-failure.db");
    let session_id = "studio-stream-failure";
    let operation_id = "018f0f23-8c65-7abc-9def-2134567890ab";
    let idempotency_key = "studio-stream-failure-key";
    let request_body = serde_json::json!({ "content": "No providers configured" });

    let gateway = PersistentGateway::start(&db_path).await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let first = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            gateway.base_url
        ))
        .json(&request_body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let first_body = json_body(first).await;
    assert_eq!(first_body["error"]["code"], "VALIDATION_ERROR");

    {
        let db = gateway.app_state.db.read().unwrap();
        let messages =
            cortex_storage::queries::studio_chat_queries::list_messages(&db, session_id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
    }

    gateway.stop().await;

    let restarted = PersistentGateway::start(&db_path).await;
    let replay = restarted
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages/stream",
            restarted.base_url
        ))
        .json(&request_body)
        .header("x-request-id", "studio-stream-failure-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );
    let replay_body = json_body(replay).await;
    assert_eq!(replay_body["error"]["code"], "VALIDATION_ERROR");

    let db = restarted.app_state.db.read().unwrap();
    let messages =
        cortex_storage::queries::studio_chat_queries::list_messages(&db, session_id).unwrap();
    assert_eq!(messages.len(), 1);
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'send_studio_message_stream'
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
    assert_eq!(
        audit_rows[1].0.as_deref(),
        Some("studio-stream-failure-retry")
    );
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);

    restarted.stop().await;
}

#[tokio::test]
async fn studio_message_replays_after_restart_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-message-success.db");
    let session_id = "studio-message-session";
    let operation_id = "018f0f23-8c65-7abc-9def-2234567890ab";
    let idempotency_key = "studio-message-success-key";
    let request_body = serde_json::json!({ "content": "Say hello" });

    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let first = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            gateway.base_url
        ))
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
    assert_eq!(first_body["assistant_message"]["content"], "Hello world");
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;

    let restarted = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    let replay = restarted
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            restarted.base_url
        ))
        .json(&request_body)
        .header("x-request-id", "studio-message-retry")
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
    let replay_body = json_body(replay).await;
    assert_eq!(replay_body["assistant_message"]["content"], "Hello world");
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'send_studio_message'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("studio-message-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);

    restarted.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_message_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-message-mismatch.db");
    let session_id = "studio-message-mismatch";
    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let key = "studio-message-mismatch-key";
    let first = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": "Say hello" }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-2334567890ab",
        )
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let conflict = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": "Say something else" }))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-2434567890ab",
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_message_rejects_empty_content() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-message-empty.db");
    let session_id = "studio-message-empty";
    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let response = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "VALIDATION_ERROR"
    );

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_message_replays_committed_failure_without_duplicate_user_message() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-message-failure.db");
    let session_id = "studio-message-failure";
    let operation_id = "018f0f23-8c65-7abc-9def-2534567890ab";
    let idempotency_key = "studio-message-failure-key";
    let request_body = serde_json::json!({ "content": "No providers configured" });

    let gateway = PersistentGateway::start(&db_path).await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;

    let first = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            gateway.base_url
        ))
        .json(&request_body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    assert_eq!(json_body(first).await["error"]["code"], "VALIDATION_ERROR");

    {
        let db = gateway.app_state.db.read().unwrap();
        let messages =
            cortex_storage::queries::studio_chat_queries::list_messages(&db, session_id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
    }

    gateway.stop().await;

    let restarted = PersistentGateway::start(&db_path).await;
    let replay = restarted
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            restarted.base_url
        ))
        .json(&request_body)
        .header("x-request-id", "studio-message-failure-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );
    assert_eq!(json_body(replay).await["error"]["code"], "VALIDATION_ERROR");

    let db = restarted.app_state.db.read().unwrap();
    let messages =
        cortex_storage::queries::studio_chat_queries::list_messages(&db, session_id).unwrap();
    assert_eq!(messages.len(), 1);
    drop(db);

    restarted.stop().await;
}

#[tokio::test]
async fn studio_message_retry_after_accepted_takeover_executes_once() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-message-accepted.db");
    let session_id = "studio-message-accepted";
    let content = "Resume safely";
    let operation_id = "018f0f23-8c65-7abc-9def-2634567890ab";
    let idempotency_key = "studio-message-accepted-key";
    let request_id = "studio-message-accepted-request";
    let request_body = studio_message_request_body(content, session_id);
    let execution_id = "018f0f23-8c65-7abc-9def-2634567890ad";
    let user_message_id = "018f0f23-8c65-7abc-9def-2634567890ae";
    let assistant_message_id = "018f0f23-8c65-7abc-9def-2634567890af";

    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;
    {
        let db = gateway.app_state.db.write().await;
        cortex_storage::queries::studio_chat_queries::insert_message(
            &db,
            user_message_id,
            session_id,
            "user",
            content,
            0,
            "clean",
        )
        .unwrap();
    }
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        "anonymous",
        operation_id,
        request_id,
        idempotency_key,
        "/api/studio/sessions/:id/messages",
        &request_body,
    )
    .await;
    let state_json = serde_json::json!({
        "version": 1,
        "session_id": session_id,
        "user_message_id": user_message_id,
        "assistant_message_id": assistant_message_id,
        "accepted_response": {
            "status": "accepted",
            "session_id": session_id,
            "user_message_id": user_message_id,
            "assistant_message_id": assistant_message_id,
            "execution_id": execution_id
        },
        "final_status_code": serde_json::Value::Null,
        "final_response": serde_json::Value::Null
    })
    .to_string();
    seed_live_execution_record(
        &gateway.app_state,
        execution_id,
        &journal_id,
        operation_id,
        "studio_send_message",
        "anonymous",
        "accepted",
        &state_json,
    )
    .await;

    let response = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": content }))
        .header("x-request-id", "studio-message-accepted-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    let response_status = response.status();
    let response_headers = response.headers().clone();
    let response_text = response.text().await.unwrap();
    assert_eq!(response_status, StatusCode::OK, "body: {response_text}");
    assert_eq!(
        response_headers
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let response_body: serde_json::Value = serde_json::from_str(&response_text).unwrap();
    assert_eq!(response_body["assistant_message"]["content"], "Hello world");
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let execution = fetch_live_execution(&gateway.client, &gateway.base_url, execution_id).await;
    assert_eq!(execution.status(), StatusCode::OK);
    let execution_body = json_body(execution).await;
    assert_eq!(execution_body["route_kind"], "studio_send_message");
    assert_eq!(execution_body["status"], "completed");
    assert_eq!(execution_body["recovery_required"], false);
    assert_eq!(execution_body["result_status_code"], 200);
    assert_eq!(
        execution_body["result_body"]["assistant_message"]["content"],
        "Hello world"
    );

    let db = gateway.app_state.db.read().unwrap();
    let messages =
        cortex_storage::queries::studio_chat_queries::list_messages(&db, session_id).unwrap();
    assert_eq!(messages.len(), 2);
    drop(db);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn studio_message_running_takeover_fails_closed_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("studio-message-running.db");
    let session_id = "studio-message-running";
    let content = "Do not rerun";
    let operation_id = "018f0f23-8c65-7abc-9def-2734567890ab";
    let idempotency_key = "studio-message-running-key";
    let request_id = "studio-message-running-request";
    let request_body = studio_message_request_body(content, session_id);
    let execution_id = "018f0f23-8c65-7abc-9def-2734567890ad";
    let user_message_id = "018f0f23-8c65-7abc-9def-2734567890ae";
    let assistant_message_id = "018f0f23-8c65-7abc-9def-2734567890af";

    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    seed_studio_session(&gateway.app_state, session_id, "studio-agent").await;
    {
        let db = gateway.app_state.db.write().await;
        cortex_storage::queries::studio_chat_queries::insert_message(
            &db,
            user_message_id,
            session_id,
            "user",
            content,
            0,
            "clean",
        )
        .unwrap();
    }
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        "anonymous",
        operation_id,
        request_id,
        idempotency_key,
        "/api/studio/sessions/:id/messages",
        &request_body,
    )
    .await;
    let state_json = serde_json::json!({
        "version": 1,
        "session_id": session_id,
        "user_message_id": user_message_id,
        "assistant_message_id": assistant_message_id,
        "accepted_response": {
            "status": "accepted",
            "session_id": session_id,
            "user_message_id": user_message_id,
            "assistant_message_id": assistant_message_id,
            "execution_id": execution_id
        },
        "final_status_code": serde_json::Value::Null,
        "final_response": serde_json::Value::Null
    })
    .to_string();
    seed_live_execution_record(
        &gateway.app_state,
        execution_id,
        &journal_id,
        operation_id,
        "studio_send_message",
        "anonymous",
        "running",
        &state_json,
    )
    .await;

    let response = gateway
        .client
        .post(format!(
            "{}/api/studio/sessions/{session_id}/messages",
            gateway.base_url
        ))
        .json(&serde_json::json!({ "content": content }))
        .header("x-request-id", "studio-message-running-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    let response_status = response.status();
    let response_headers = response.headers().clone();
    let response_text = response.text().await.unwrap();
    assert_eq!(
        response_status,
        StatusCode::ACCEPTED,
        "body: {response_text}"
    );
    assert_eq!(
        response_headers
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let body: serde_json::Value = serde_json::from_str(&response_text).unwrap();
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["recovery_required"], true);
    assert_eq!(body["execution_id"], execution_id);
    assert_eq!(provider.hits.load(Ordering::SeqCst), 0);

    let execution = fetch_live_execution(&gateway.client, &gateway.base_url, execution_id).await;
    assert_eq!(execution.status(), StatusCode::OK);
    let execution_body = json_body(execution).await;
    assert_eq!(execution_body["route_kind"], "studio_send_message");
    assert_eq!(execution_body["status"], "recovery_required");
    assert_eq!(execution_body["recovery_required"], true);
    assert_eq!(
        execution_body["accepted_response"]["execution_id"],
        execution_id
    );
    assert_eq!(
        execution_body["accepted_response"]["assistant_message_id"],
        assistant_message_id
    );

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_replays_after_restart_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("agent-chat-success.db");
    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890ab";
    let idempotency_key = "agent-chat-success-key";
    let request_body = agent_chat_request_body("Say hello", None);

    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let first = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
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
    assert_eq!(first_body["content"], "Hello world");
    assert!(first_body["session_id"].as_str().unwrap().len() > 10);
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;

    let restarted = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    let replay = restarted
        .client
        .post(restarted.url("/api/agent/chat"))
        .json(&request_body)
        .header("x-request-id", "agent-chat-retry")
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
    let replay_body = json_body(replay).await;
    assert_eq!(replay_body["content"], "Hello world");
    assert_eq!(replay_body["session_id"], first_body["session_id"]);
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'agent_chat'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("agent-chat-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);

    restarted.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start_with_model_providers(
        &tmp.path().join("agent-chat-mismatch.db"),
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let key = "agent-chat-mismatch-key";
    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890ac";

    let first = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&agent_chat_request_body("first message", None))
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let conflict = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&agent_chat_request_body("second message", None))
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_materializes_runtime_session_for_sessions_api() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("agent-chat-session-materialization.db");
    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&agent_chat_request_body("Read the repo name", None))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-4e34567890c1",
        )
        .header("idempotency-key", "agent-chat-session-materialization")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let session_id = body["session_id"].as_str().unwrap();

    let events = gateway
        .client
        .get(gateway.url(&format!("/api/sessions/{session_id}/events?limit=10")))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    let events_body = json_body(events).await;
    assert_eq!(events_body["session_id"], session_id);
    assert_eq!(events_body["chain_valid"], true);
    assert!(
        !events_body["events"].as_array().unwrap().is_empty(),
        "runtime session should expose at least one persisted event"
    );

    let sessions = gateway
        .client
        .get(gateway.url("/api/sessions?page=1&page_size=10"))
        .send()
        .await
        .unwrap();
    assert_eq!(sessions.status(), StatusCode::OK);
    let sessions_body = json_body(sessions).await;
    assert!(
        sessions_body["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| session["session_id"] == session_id),
        "runtime session should appear in session listings"
    );

    let heartbeat = gateway
        .client
        .post(gateway.url(&format!("/api/sessions/{session_id}/heartbeat")))
        .header("x-ghost-client-name", "dashboard")
        .header("x-ghost-client-version", "0.1.0")
        .send()
        .await
        .unwrap();
    assert_eq!(heartbeat.status(), StatusCode::NO_CONTENT);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_rejects_empty_message() {
    let _guard = hold_env_lock();
    let gateway = PersistentGateway::start(
        &tempfile::tempdir()
            .unwrap()
            .path()
            .join("agent-chat-empty.db"),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&agent_chat_request_body("   ", None))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "VALIDATION_ERROR"
    );

    gateway.stop().await;
}

#[tokio::test]
async fn agent_chat_replays_committed_failure_without_refiring_provider() {
    let _guard = hold_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start(&tmp.path().join("agent-chat-no-providers.db")).await;
    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890ad";
    let idempotency_key = "agent-chat-no-providers-key";
    let request_body = agent_chat_request_body("Hello", None);

    let first = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&request_body)
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        first
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let first_body = json_body(first).await;
    assert_eq!(first_body["error"]["code"], "VALIDATION_ERROR");
    assert!(first_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No model providers configured"));

    let replay = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&request_body)
        .header("x-request-id", "agent-chat-no-provider-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        replay
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("replayed")
    );
    let replay_body = json_body(replay).await;
    assert_eq!(replay_body, first_body);

    let db = gateway.app_state.db.read().unwrap();
    let journal =
        cortex_storage::queries::operation_journal_queries::get_by_operation_id(&db, operation_id)
            .unwrap()
            .unwrap();
    assert_eq!(journal.status, "committed");
    assert_eq!(journal.response_status_code, Some(422));
    drop(db);

    gateway.stop().await;
}

#[tokio::test]
async fn agent_chat_retry_after_accepted_takeover_executes_once() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start_with_model_providers(
        &tmp.path().join("agent-chat-takeover.db"),
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890ae";
    let request_id = "agent-chat-seed-request";
    let idempotency_key = "agent-chat-takeover-key";
    let session_id = "018f0f23-8c65-7abc-9def-4e34567890af";
    let execution_id = "018f0f23-8c65-7abc-9def-4e34567890b0";
    let request_body = agent_chat_request_body("Take over", Some(session_id));
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        "anonymous",
        operation_id,
        request_id,
        idempotency_key,
        "/api/agent/chat",
        &request_body,
    )
    .await;
    let agent_id = ghost_gateway::agents::registry::durable_agent_id(
        ghost_gateway::runtime_safety::API_SYNTHETIC_AGENT_NAME,
    )
    .to_string();
    let state_json = serde_json::json!({
        "version": 1,
        "session_id": session_id,
        "accepted_response": {
            "status": "accepted",
            "session_id": session_id,
            "agent_id": agent_id,
            "execution_id": execution_id
        },
        "final_status_code": serde_json::Value::Null,
        "final_response": serde_json::Value::Null
    })
    .to_string();
    seed_live_execution_record(
        &gateway.app_state,
        execution_id,
        &journal_id,
        operation_id,
        "agent_chat",
        "anonymous",
        "accepted",
        &state_json,
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&request_body)
        .header("x-request-id", "agent-chat-takeover-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    assert_eq!(json_body(response).await["content"], "Hello world");
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let execution = fetch_live_execution(&gateway.client, &gateway.base_url, execution_id).await;
    assert_eq!(execution.status(), StatusCode::OK);
    let execution_body = json_body(execution).await;
    assert_eq!(execution_body["route_kind"], "agent_chat");
    assert_eq!(execution_body["status"], "completed");
    assert_eq!(execution_body["recovery_required"], false);
    assert_eq!(execution_body["result_status_code"], 200);
    assert_eq!(execution_body["result_body"]["content"], "Hello world");

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_running_takeover_fails_closed_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start_with_model_providers(
        &tmp.path().join("agent-chat-running.db"),
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890b1";
    let request_id = "agent-chat-running-seed";
    let idempotency_key = "agent-chat-running-key";
    let session_id = "018f0f23-8c65-7abc-9def-4e34567890b2";
    let execution_id = "018f0f23-8c65-7abc-9def-4e34567890b3";
    let request_body = agent_chat_request_body("Still running", Some(session_id));
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        "anonymous",
        operation_id,
        request_id,
        idempotency_key,
        "/api/agent/chat",
        &request_body,
    )
    .await;
    let agent_id = ghost_gateway::agents::registry::durable_agent_id(
        ghost_gateway::runtime_safety::API_SYNTHETIC_AGENT_NAME,
    )
    .to_string();
    let state_json = serde_json::json!({
        "version": 1,
        "session_id": session_id,
        "accepted_response": {
            "status": "accepted",
            "session_id": session_id,
            "agent_id": agent_id,
            "execution_id": execution_id
        },
        "final_status_code": serde_json::Value::Null,
        "final_response": serde_json::Value::Null
    })
    .to_string();
    seed_live_execution_record(
        &gateway.app_state,
        execution_id,
        &journal_id,
        operation_id,
        "agent_chat",
        "anonymous",
        "running",
        &state_json,
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&request_body)
        .header("x-request-id", "agent-chat-running-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    let response_status = response.status();
    let response_headers = response.headers().clone();
    let response_text = response.text().await.unwrap();
    assert_eq!(
        response_status,
        StatusCode::ACCEPTED,
        "body: {response_text}"
    );
    assert_eq!(
        response_headers
            .get("x-ghost-idempotency-status")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
    let body: serde_json::Value = serde_json::from_str(&response_text).unwrap();
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["recovery_required"], true);
    assert_eq!(body["execution_id"], execution_id);
    assert_eq!(provider.hits.load(Ordering::SeqCst), 0);

    let execution = fetch_live_execution(&gateway.client, &gateway.base_url, execution_id).await;
    assert_eq!(execution.status(), StatusCode::OK);
    let execution_body = json_body(execution).await;
    assert_eq!(execution_body["route_kind"], "agent_chat");
    assert_eq!(execution_body["status"], "recovery_required");
    assert_eq!(execution_body["recovery_required"], true);
    assert_eq!(
        execution_body["accepted_response"]["execution_id"],
        execution_id
    );
    assert_eq!(
        execution_body["accepted_response"]["session_id"],
        session_id
    );

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_legacy_live_execution_state_aborts_retry_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start_with_model_providers(
        &tmp.path().join("agent-chat-legacy-live-execution.db"),
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890b4";
    let request_id = "agent-chat-legacy-state-request";
    let idempotency_key = "agent-chat-legacy-state-key";
    let session_id = "018f0f23-8c65-7abc-9def-4e34567890b5";
    let execution_id = "018f0f23-8c65-7abc-9def-4e34567890b6";
    let request_body = agent_chat_request_body("Legacy state", Some(session_id));
    let journal_id = seed_operation_journal_in_progress(
        &gateway.app_state,
        "anonymous",
        operation_id,
        request_id,
        idempotency_key,
        "/api/agent/chat",
        &request_body,
    )
    .await;
    let agent_id = ghost_gateway::agents::registry::durable_agent_id(
        ghost_gateway::runtime_safety::API_SYNTHETIC_AGENT_NAME,
    )
    .to_string();
    let state_json = serde_json::json!({
        "version": 1,
        "session_id": session_id,
        "accepted_response": {
            "status": "accepted",
            "session_id": session_id,
            "agent_id": agent_id,
            "execution_id": execution_id
        },
        "final_status_code": serde_json::Value::Null,
        "final_response": serde_json::Value::Null
    })
    .to_string();
    seed_live_execution_record_with_version(
        &gateway.app_state,
        execution_id,
        &journal_id,
        operation_id,
        "agent_chat",
        "anonymous",
        0,
        "accepted",
        &state_json,
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/agent/chat"))
        .json(&request_body)
        .header("x-request-id", "agent-chat-legacy-state-retry")
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", idempotency_key)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(provider.hits.load(Ordering::SeqCst), 0);

    {
        let db = gateway.app_state.db.read().unwrap();
        let status: String = db
            .query_row(
                "SELECT status FROM operation_journal WHERE id = ?1",
                rusqlite::params![journal_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "aborted");
    }

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_stream_replays_after_restart_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("agent-chat-stream-success.db");
    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890b4";
    let idempotency_key = "agent-chat-stream-success-key";
    let request_body = agent_chat_request_body("Stream hello", None);

    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let first = gateway
        .client
        .post(gateway.url("/api/agent/chat/stream"))
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
    let first_stream = first.text().await.unwrap();
    assert!(first_stream.contains("event: stream_start"));
    assert!(first_stream.contains("event: stream_end"));
    assert!(first_stream.contains("Hello"));
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;

    let restarted = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;
    let replay = restarted
        .client
        .post(restarted.url("/api/agent/chat/stream"))
        .json(&request_body)
        .header("x-request-id", "agent-chat-stream-retry")
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
    let replay_stream = replay.text().await.unwrap();
    assert!(replay_stream.contains("event: stream_start"));
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    let db = restarted.app_state.db.read().unwrap();
    let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
        .prepare(
            "SELECT request_id, idempotency_key, idempotency_status
             FROM audit_log
             WHERE operation_id = ?1 AND event_type = 'agent_chat_stream'
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
    assert_eq!(audit_rows[1].0.as_deref(), Some("agent-chat-stream-retry"));
    assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    drop(db);

    restarted.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_stream_replays_accepted_metadata_without_refiring_provider() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start_with_model_providers(
        &tmp.path().join("agent-chat-stream-accepted.db"),
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890b5";
    let idempotency_key = "agent-chat-stream-accepted-key";
    let request_body = agent_chat_request_body(
        "Accepted only",
        Some("018f0f23-8c65-7abc-9def-4e34567890b6"),
    );
    let context = ghost_gateway::api::operation_context::OperationContext {
        request_id: "agent-chat-stream-seed-request".to_string(),
        operation_id: Some(operation_id.to_string()),
        idempotency_key: Some(idempotency_key.to_string()),
        idempotency_status: None,
        is_mutating: true,
        client_supplied_operation_id: true,
        client_supplied_idempotency_key: true,
    };
    let agent_id = ghost_gateway::agents::registry::durable_agent_id(
        ghost_gateway::runtime_safety::API_SYNTHETIC_AGENT_NAME,
    )
    .to_string();
    let session_id = request_body["session_id"].as_str().unwrap();
    let message_id = "018f0f23-8c65-7abc-9def-4e34567890b7";
    {
        let db = gateway.app_state.db.write().await;
        let prepared = ghost_gateway::api::idempotency::prepare_json_operation(
            &db,
            &context,
            "anonymous",
            "POST",
            "/api/agent/chat/stream",
            &request_body,
        )
        .unwrap();
        let ghost_gateway::api::idempotency::PreparedOperation::Acquired { lease } = prepared
        else {
            panic!("expected an acquired journal entry");
        };
        let start_payload = serde_json::json!({
            "session_id": session_id,
            "message_id": message_id,
        });
        let start_seq = cortex_storage::queries::stream_event_queries::insert_stream_event(
            &db,
            session_id,
            message_id,
            "stream_start",
            &start_payload.to_string(),
        )
        .unwrap();
        let accepted_body = serde_json::json!({
            "status": "accepted",
            "session_id": session_id,
            "agent_id": agent_id,
            "message_id": message_id,
            "stream_start_seq": start_seq,
        });
        ghost_gateway::api::idempotency::commit_prepared_json_operation(
            &db,
            &context,
            &lease,
            StatusCode::OK,
            &accepted_body,
        )
        .unwrap();
    }

    let replay = gateway
        .client
        .post(gateway.url("/api/agent/chat/stream"))
        .json(&request_body)
        .header("x-request-id", "agent-chat-stream-accepted-retry")
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
    let replay_stream = replay.text().await.unwrap();
    assert!(replay_stream.contains("event: stream_start"));
    assert!(!replay_stream.contains("event: stream_end"));
    assert_eq!(provider.hits.load(Ordering::SeqCst), 0);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_stream_materializes_runtime_session_for_sessions_api() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp
        .path()
        .join("agent-chat-stream-session-materialization.db");
    let gateway = PersistentGateway::start_with_model_providers(
        &db_path,
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/agent/chat/stream"))
        .json(&agent_chat_request_body("Stream the repo name", None))
        .header(
            "x-ghost-operation-id",
            "018f0f23-8c65-7abc-9def-4e34567890c2",
        )
        .header(
            "idempotency-key",
            "agent-chat-stream-session-materialization",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let stream = response.text().await.unwrap();
    assert!(stream.contains("event: stream_start"));
    assert!(stream.contains("event: stream_end"));
    let session_id = extract_stream_session_id(&stream);

    let events = gateway
        .client
        .get(gateway.url(&format!("/api/sessions/{session_id}/events?limit=20")))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    let events_body = json_body(events).await;
    assert_eq!(events_body["session_id"], session_id);
    assert_eq!(events_body["chain_valid"], true);
    let event_types = events_body["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["event_type"].as_str())
        .collect::<Vec<_>>();
    assert!(event_types.contains(&"stream_start"));
    assert!(event_types.contains(&"turn_complete"));

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_stream_idempotency_key_reuse_with_different_payload_conflicts() {
    let _guard = hold_env_lock();
    let _api_key = EnvVarGuard::set("OPENAI_API_KEY", "test-openai-key");
    let provider = MockOpenAICompatServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let gateway = PersistentGateway::start_with_model_providers(
        &tmp.path().join("agent-chat-stream-mismatch.db"),
        openai_compat_provider_configs(&provider.base_url),
    )
    .await;

    let key = "agent-chat-stream-mismatch-key";
    let operation_id = "018f0f23-8c65-7abc-9def-4e34567890b8";

    let first = gateway
        .client
        .post(gateway.url("/api/agent/chat/stream"))
        .json(&agent_chat_request_body("first message", None))
        .header("x-ghost-operation-id", operation_id)
        .header("idempotency-key", key)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let _ = first.text().await.unwrap();

    let conflict = gateway
        .client
        .post(gateway.url("/api/agent/chat/stream"))
        .json(&agent_chat_request_body("second message", None))
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
    assert_eq!(
        json_body(conflict).await["error"]["code"],
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(provider.hits.load(Ordering::SeqCst), 1);

    gateway.stop().await;
    provider.stop().await;
}

#[tokio::test]
async fn agent_chat_stream_rejects_empty_message() {
    let _guard = hold_env_lock();
    let gateway = PersistentGateway::start(
        &tempfile::tempdir()
            .unwrap()
            .path()
            .join("agent-chat-stream-empty.db"),
    )
    .await;

    let response = gateway
        .client
        .post(gateway.url("/api/agent/chat/stream"))
        .json(&agent_chat_request_body("   ", None))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "VALIDATION_ERROR"
    );

    gateway.stop().await;
}
