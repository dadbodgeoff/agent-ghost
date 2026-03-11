//! Gateway-mediated A2A task endpoints (T-4.1.2).
//!
//! These endpoints provide a dashboard-friendly interface for sending tasks
//! to external A2A agents, checking task status, and discovering agents
//! on the mesh. The raw A2A protocol endpoint lives at `/a2a`.

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Response;
use axum::Extension;
use axum::Json;
use base64::Engine;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation, prepare_json_operation,
    start_operation_lease_heartbeat, PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::websocket::{WsEnvelope, WsEvent};
use crate::state::AppState;

const SEND_TASK_ROUTE_TEMPLATE: &str = "/api/a2a/tasks";

#[derive(Debug, Clone)]
struct VerifiedDiscoveredAgent {
    name: String,
    endpoint_url: String,
}

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ATask {
    pub task_id: String,
    pub target_agent: String,
    pub target_url: String,
    pub method: String,
    pub status: String,
    pub created_at: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SendTaskRequest {
    pub target_url: String,
    pub target_agent: Option<String>,
    pub input: serde_json::Value,
    pub method: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<A2ATask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredAgent {
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub capabilities: Vec<String>,
    pub trust_score: f64,
    pub version: String,
    pub reachable: bool,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub agents: Vec<DiscoveredAgent>,
}

fn a2a_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

// ── Handlers ───────────────────────────────────────────────────────

/// POST /api/a2a/tasks — send a task to an external A2A agent.
pub async fn send_task(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<SendTaskRequest>,
) -> Response {
    let normalized_target_url = normalize_a2a_endpoint_url(&req.target_url);
    if normalized_target_url.is_empty() {
        return error_response_with_idempotency(ApiError::bad_request("target_url is required"));
    }
    if let Err(e) = crate::api::ssrf::validate_url(&normalized_target_url) {
        return error_response_with_idempotency(ApiError::bad_request(format!(
            "A2A target URL blocked: {e}"
        )));
    }
    let verified_agent = match load_verified_discovered_agent(&state, &normalized_target_url) {
        Ok(agent) => agent,
        Err(error) => return error_response_with_idempotency(error),
    };

    let actor = a2a_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::to_value(&req).unwrap_or(serde_json::Value::Null);
    let method = req.method.clone().unwrap_or_else(|| "tasks/send".into());
    let task_id = operation_context
        .operation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let agent_name = req
        .target_agent
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| verified_agent.name.clone());
    let created_at = chrono::Utc::now().to_rfc3339();
    let input_str = serde_json::to_string(&req.input).unwrap_or_else(|_| "null".into());

    let jsonrpc_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": task_id.clone(),
        "method": method.clone(),
        "params": {
            "task_id": task_id.clone(),
            "task": input_str.clone(),
            "sender_id": actor,
            "recipient_id": agent_name.clone(),
            "id": task_id.clone(),
            "message": {
                "role": "user",
                "parts": [{ "type": "text", "text": req.input.clone() }],
            }
        }
    });
    let jsonrpc_body = match serde_json::to_vec(&jsonrpc_req) {
        Ok(body) => body,
        Err(error) => {
            return error_response_with_idempotency(ApiError::internal(format!(
                "failed to serialize A2A request: {error}"
            )));
        }
    };
    let dispatch_url = format!("{}/a2a", verified_agent.endpoint_url.trim_end_matches('/'));
    let signature = match sign_a2a_request(&state, &jsonrpc_body) {
        Ok(signature) => signature,
        Err(error) => return error_response_with_idempotency(error),
    };

    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            SEND_TASK_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                "a2a",
                "send_a2a_task",
                "medium",
                actor,
                "replayed",
                serde_json::json!({
                    "task_id": stored.body.get("task_id"),
                    "target_agent": stored.body.get("target_agent"),
                }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": SEND_TASK_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            {
                let db = state.db.write().await;
                if let Err(error) = db.execute(
                    "INSERT INTO a2a_tasks (id, target_agent, target_url, method, status, input, output, created_at) \
                     VALUES (?1, ?2, ?3, ?4, 'pending', ?5, NULL, ?6)",
                    rusqlite::params![
                        task_id,
                        agent_name,
                        verified_agent.endpoint_url,
                        method,
                        input_str,
                        created_at
                    ],
                ) {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(ApiError::db_error(
                        "insert a2a task",
                        error,
                    ));
                }
            }

            let heartbeat = start_operation_lease_heartbeat(Arc::clone(&state.db), lease.clone());
            let client = reqwest::Client::new();
            let resp = match client
                .post(&dispatch_url)
                .header("Content-Type", "application/json")
                .header("X-Ghost-Signature", signature)
                .timeout(std::time::Duration::from_secs(30))
                .body(jsonrpc_body.clone())
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let _ = heartbeat.stop().await;
                    let db = state.db.write().await;
                    let _ = db.execute(
                        "UPDATE a2a_tasks SET status = 'failed', output = ?1 WHERE id = ?2",
                        rusqlite::params![
                            serde_json::json!({"error": format!("A2A request failed: {error}")})
                                .to_string(),
                            task_id
                        ],
                    );
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(ApiError::internal(format!(
                        "A2A request failed: {error}"
                    )));
                }
            };
            if let Err(error) = heartbeat.stop().await {
                return error_response_with_idempotency(error);
            }

            let status_code = resp.status();
            let body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "failed to parse response"}));

            let task_status = derive_task_status(status_code, &body);

            let task = A2ATask {
                task_id: task_id.clone(),
                target_agent: agent_name.clone(),
                target_url: verified_agent.endpoint_url.clone(),
                method: method.clone(),
                status: task_status.into(),
                created_at: created_at.clone(),
                input: req.input.clone(),
                output: Some(body.clone()),
            };

            let response_body = serde_json::to_value(&task).unwrap_or(serde_json::Value::Null);
            let db = state.db.write().await;
            let _ = db.execute(
                "UPDATE a2a_tasks SET status = ?1, output = ?2 WHERE id = ?3",
                rusqlite::params![
                    task_status,
                    serde_json::to_string(&body).unwrap_or_default(),
                    task_id,
                ],
            );

            match commit_prepared_json_operation(
                &db,
                &operation_context,
                &lease,
                StatusCode::OK,
                &response_body,
            ) {
                Ok(outcome) => {
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::A2ATaskUpdate {
                            task_id: task_id.clone(),
                            status: task_status.into(),
                            agent_name: agent_name.clone(),
                        },
                    );
                    write_mutation_audit_entry(
                        &db,
                        "a2a",
                        "send_a2a_task",
                        "medium",
                        actor,
                        task_status,
                        serde_json::json!({
                            "task_id": task_id,
                            "target_agent": agent_name,
                            "target_url": verified_agent.endpoint_url,
                        }),
                        &operation_context,
                        &outcome.idempotency_status,
                    );
                    json_response_with_idempotency(
                        outcome.status,
                        outcome.body,
                        outcome.idempotency_status,
                    )
                }
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    error_response_with_idempotency(error)
                }
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/a2a/tasks/:task_id — check task status.
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> ApiResult<A2ATask> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_task", e))?;

    let task = db
        .query_row(
            "SELECT id, target_agent, target_url, method, status, input, output, created_at \
             FROM a2a_tasks WHERE id = ?1",
            rusqlite::params![task_id],
            |row| {
                let input_json: String = row.get::<_, String>(5).unwrap_or_else(|_| "null".into());
                let output_json: String = row.get::<_, String>(6).unwrap_or_else(|_| "null".into());
                Ok(A2ATask {
                    task_id: row.get(0)?,
                    target_agent: row.get(1)?,
                    target_url: row.get(2)?,
                    method: row.get(3)?,
                    status: row.get(4)?,
                    input: serde_json::from_str(&input_json).unwrap_or_default(),
                    output: serde_json::from_str(&output_json).ok(),
                    created_at: row.get(7)?,
                })
            },
        )
        .map_err(|_| ApiError::not_found(format!("A2A task '{task_id}' not found")))?;

    Ok(Json(task))
}

/// GET /api/a2a/tasks — list all A2A tasks.
pub async fn list_tasks(State(state): State<Arc<AppState>>) -> ApiResult<TaskListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_tasks", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, target_agent, target_url, method, status, input, output, created_at \
             FROM a2a_tasks ORDER BY created_at DESC LIMIT 100",
        )
        .map_err(|e| ApiError::db_error("prepare a2a tasks", e))?;

    let tasks: Vec<A2ATask> = stmt
        .query_map([], |row| {
            let input_json: String = row.get::<_, String>(5).unwrap_or_else(|_| "null".into());
            let output_json: String = row.get::<_, String>(6).unwrap_or_else(|_| "null".into());
            Ok(A2ATask {
                task_id: row.get(0)?,
                target_agent: row.get(1)?,
                target_url: row.get(2)?,
                method: row.get(3)?,
                status: row.get(4)?,
                input: serde_json::from_str(&input_json).unwrap_or_default(),
                output: serde_json::from_str(&output_json).ok(),
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| ApiError::db_error("query a2a tasks", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(TaskListResponse { tasks }))
}

/// GET /api/a2a/tasks/:task_id/stream — SSE stream for task updates.
///
/// T-5.3.5: Includes a 5-minute inactivity timeout. If no relevant event
/// arrives within 5 minutes, sends a `timeout` event and closes the stream.
pub async fn stream_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.event_tx.subscribe();
    let task_id_owned = task_id.clone();

    let stream = async_stream::stream! {
        // Send initial event.
        yield Ok(Event::default()
            .event("connected")
            .data(format!(r#"{{"task_id":"{}"}}"#, task_id_owned)));

        // T-5.3.5: 5-minute inactivity timeout.
        let inactivity_timeout = std::time::Duration::from_secs(300);

        loop {
            match tokio::time::timeout(inactivity_timeout, rx.recv()).await {
                Ok(Ok(WsEnvelope { event: WsEvent::A2ATaskUpdate { task_id: tid, status, agent_name }, .. })) => {
                    if tid == task_id_owned {
                        let data = serde_json::json!({
                            "task_id": tid,
                            "status": status,
                            "agent_name": agent_name,
                        });
                        yield Ok(Event::default()
                            .event("task_update")
                            .data(data.to_string()));

                        // End stream if terminal state.
                        if status == "completed" || status == "failed" || status == "canceled" {
                            break;
                        }
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Ok(_)) => continue,
                Err(_) => {
                    // T-5.3.5: Inactivity timeout — close the stream.
                    tracing::debug!(task_id = %task_id_owned, "SSE stream timed out after 5 minutes of inactivity");
                    yield Ok(Event::default()
                        .event("timeout")
                        .data(r#"{"reason":"inactivity_timeout","timeout_seconds":300}"#));
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

/// GET /api/a2a/discover — discover agents on the mesh.
///
/// Probes all known mesh peer endpoints by fetching their
/// `/.well-known/agent.json` agent card, upserts results back into the
/// `discovered_agents` table, and returns the updated list.
pub async fn discover_agents(State(state): State<Arc<AppState>>) -> ApiResult<DiscoverResponse> {
    // 1. Collect peer endpoints from authoritative sources:
    //    - mesh.known_agents in the live gateway config
    //    - previously discovered peer records in the DB
    let peer_urls = collect_discovery_peer_urls(&state)?;

    // 2. Probe each peer's /.well-known/agent.json with bounded concurrency (T-5.3.2).
    // Uses stream::iter + buffer_unordered(16) instead of unbounded tokio::spawn.
    use futures::stream::StreamExt;

    let client = reqwest::Client::new();

    let probe_futures = peer_urls.into_iter().map(|peer_url| {
        let normalized_peer_url = normalize_a2a_endpoint_url(&peer_url);
        let url = format!("{normalized_peer_url}/.well-known/agent.json");
        let client = client.clone();

        async move {
            let resp = client
                .get(&url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await;

            let card = match resp {
                Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.ok(),
                _ => None,
            };
            (peer_url, normalized_peer_url, card)
        }
    });

    // 3. Collect results with bounded concurrency (max 16 concurrent probes).
    let mut probed_agents: Vec<DiscoveredAgent> = Vec::new();
    let mut buffered = futures::stream::iter(probe_futures).buffer_unordered(16);

    while let Some((peer_url, normalized_peer_url, card)) = buffered.next().await {
        let card = match card {
            Some(card) => card,
            None => {
                // Peer unreachable — keep existing DB record but mark unreachable.
                probed_agents.push(DiscoveredAgent {
                    name: "unknown".into(),
                    description: String::new(),
                    endpoint_url: normalized_peer_url,
                    capabilities: vec![],
                    trust_score: 0.0,
                    version: String::new(),
                    reachable: false,
                    verified: false,
                });
                continue;
            }
        };

        let name = card
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let description = card
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = card
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let capabilities: Vec<String> = card
            .get("capabilities")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // T-5.2.3: Verify agent card signature if present.
        // Cards without a valid signature get trust_score: 0.0.
        let sig_verified = verify_agent_card_signature(&card);
        let trust_score = if sig_verified { 1.0 } else { 0.0 };
        if !sig_verified {
            tracing::warn!(
                peer = %peer_url,
                agent = %name,
                "Agent card signature missing or invalid — trust_score set to 0.0"
            );
        }

        probed_agents.push(DiscoveredAgent {
            name,
            description,
            endpoint_url: normalized_peer_url,
            capabilities,
            trust_score,
            version,
            reachable: true,
            verified: sig_verified,
        });
    }

    // 4. Upsert probed results back into the DB.
    {
        let db = state.db.write().await;
        for agent in &probed_agents {
            let caps_json =
                serde_json::to_string(&agent.capabilities).unwrap_or_else(|_| "[]".into());
            let _ = db.execute(
                "INSERT OR REPLACE INTO discovered_agents \
                 (endpoint_url, name, description, capabilities, trust_score, version) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    &agent.endpoint_url,
                    &agent.name,
                    &agent.description,
                    caps_json,
                    agent.trust_score,
                    &agent.version,
                ],
            );
        }
    }

    let db = state.db.read()?;
    let agents = read_discovered_agents(&db, Some(&probed_agents))?;

    Ok(Json(DiscoverResponse { agents }))
}

#[derive(Debug, Deserialize, Default)]
struct MeshDiscoveryConfig {
    #[serde(default)]
    mesh: crate::config::MeshConfig,
}

fn collect_discovery_peer_urls(state: &AppState) -> Result<Vec<String>, ApiError> {
    let mut urls: HashSet<String> = load_configured_peer_urls(&state.config_path)
        .into_iter()
        .collect();

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("discover_agents_read_peers", e))?;
    let stmt_result =
        db.prepare("SELECT endpoint_url FROM discovered_agents WHERE endpoint_url IS NOT NULL");
    if let Ok(mut stmt) = stmt_result {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for row in rows.filter_map(|row| row.ok()) {
                let normalized = normalize_a2a_endpoint_url(&row);
                if !normalized.is_empty() {
                    urls.insert(normalized);
                }
            }
        }
    }

    let mut peer_urls: Vec<String> = urls.into_iter().collect();
    peer_urls.sort();
    Ok(peer_urls)
}

fn load_configured_peer_urls(config_path: &std::path::Path) -> Vec<String> {
    let raw = match std::fs::read_to_string(config_path) {
        Ok(raw) => raw,
        Err(error) => {
            tracing::debug!(
                path = %config_path.display(),
                error = %error,
                "A2A discovery could not read gateway config; falling back to DB peer cache"
            );
            return Vec::new();
        }
    };

    let config: MeshDiscoveryConfig = match serde_yaml::from_str(&raw) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(
                path = %config_path.display(),
                error = %error,
                "A2A discovery could not parse mesh config; falling back to DB peer cache"
            );
            return Vec::new();
        }
    };

    config
        .mesh
        .known_agents
        .into_iter()
        .map(|agent| normalize_a2a_endpoint_url(&agent.endpoint))
        .filter(|endpoint| !endpoint.is_empty())
        .collect()
}

fn verify_agent_card_signature(card: &serde_json::Value) -> bool {
    serde_json::from_value::<ghost_mesh::types::AgentCard>(card.clone())
        .map(|card| card.verify_signature())
        .unwrap_or(false)
}

fn normalize_a2a_endpoint_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let without_agent_card = trimmed
        .strip_suffix("/.well-known/agent.json")
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    without_agent_card
        .strip_suffix("/a2a")
        .unwrap_or(without_agent_card)
        .trim_end_matches('/')
        .to_string()
}

fn load_verified_discovered_agent(
    state: &AppState,
    target_url: &str,
) -> Result<VerifiedDiscoveredAgent, ApiError> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("load_verified_discovered_agent", e))?;
    let normalized = normalize_a2a_endpoint_url(target_url);
    let candidate = db
        .query_row(
            "SELECT name, endpoint_url, trust_score
             FROM discovered_agents
             WHERE rtrim(endpoint_url, '/') = ?1
             ORDER BY trust_score DESC
             LIMIT 1",
            rusqlite::params![normalized],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| ApiError::db_error("load_verified_discovered_agent", e))?;

    match candidate {
        Some((name, endpoint_url, trust_score)) if trust_score > 0.0 => {
            Ok(VerifiedDiscoveredAgent {
                name,
                endpoint_url: normalize_a2a_endpoint_url(&endpoint_url),
            })
        }
        Some((_name, endpoint_url, _trust_score)) => Err(ApiError::with_details(
            StatusCode::PRECONDITION_FAILED,
            "A2A_TARGET_UNVERIFIED",
            "A2A target must be rediscovered with a verified Ed25519 agent card before dispatch",
            serde_json::json!({
                "target_url": endpoint_url,
                "required_trust_score": 1.0,
                "next_step": "Run A2A discovery and choose a reachable verified agent.",
            }),
        )),
        None => Err(ApiError::with_details(
            StatusCode::PRECONDITION_FAILED,
            "A2A_TARGET_UNDISCOVERED",
            "A2A target must be discovered before dispatch",
            serde_json::json!({
                "target_url": normalized,
                "next_step": "Run A2A discovery and choose a reachable verified agent.",
            }),
        )),
    }
}

fn sign_a2a_request(state: &AppState, body: &[u8]) -> Result<String, ApiError> {
    let Some(signing_key) = state.mesh_signing_key.as_ref() else {
        return Err(ApiError::with_details(
            StatusCode::PRECONDITION_FAILED,
            "MESH_NOT_ENABLED",
            "Mesh signing is not configured for outbound A2A dispatch",
            serde_json::json!({
                "next_step": "Enable mesh networking on the sending gateway before dispatching A2A tasks.",
            }),
        ));
    };
    let signing_key = signing_key
        .lock()
        .map_err(|_| ApiError::internal("mesh signing key lock poisoned"))?;
    let signature = ghost_signing::sign(body, &signing_key);
    Ok(base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()))
}

fn derive_task_status(status_code: StatusCode, body: &serde_json::Value) -> &'static str {
    if !status_code.is_success() {
        return "failed";
    }

    body.get("result")
        .and_then(|result| result.get("status"))
        .and_then(|status| status.as_str())
        .map(normalize_remote_task_status)
        .unwrap_or("submitted")
}

fn normalize_remote_task_status(status: &str) -> &'static str {
    if status.eq_ignore_ascii_case("submitted") {
        "submitted"
    } else if status.eq_ignore_ascii_case("working") {
        "working"
    } else if status.eq_ignore_ascii_case("completed") {
        "completed"
    } else if status.eq_ignore_ascii_case("canceled") {
        "canceled"
    } else if status.starts_with("failed") {
        "failed"
    } else if status.starts_with("input-required") {
        "working"
    } else {
        "submitted"
    }
}

fn read_discovered_agents(
    db: &rusqlite::Connection,
    probed_agents: Option<&[DiscoveredAgent]>,
) -> Result<Vec<DiscoveredAgent>, ApiError> {
    let mut agents = Vec::new();
    let probe_status: std::collections::HashMap<&str, bool> = probed_agents
        .unwrap_or(&[])
        .iter()
        .map(|agent| (agent.endpoint_url.as_str(), agent.reachable))
        .collect();

    let mut stmt = db.prepare(
        "SELECT name, description, endpoint_url, capabilities, trust_score, version
         FROM discovered_agents
         ORDER BY trust_score DESC, name ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        let caps_json: String = row.get::<_, String>(3).unwrap_or_else(|_| "[]".into());
        let endpoint: String = row.get(2)?;
        let trust_score: f64 = row.get(4)?;
        Ok(DiscoveredAgent {
            name: row.get(0)?,
            description: row.get(1)?,
            endpoint_url: endpoint.clone(),
            capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
            trust_score,
            version: row.get(5)?,
            reachable: probe_status
                .get(endpoint.as_str())
                .copied()
                .unwrap_or(false),
            verified: trust_score > 0.0,
        })
    })?;

    for row in rows {
        agents.push(row?);
    }

    Ok(agents)
}

#[cfg(test)]
mod tests {
    use super::{derive_task_status, verify_agent_card_signature};
    use axum::http::StatusCode;

    #[test]
    fn signed_mesh_card_json_verifies() {
        let (signing_key, verifying_key) = ghost_signing::generate_keypair();
        let mut card = ghost_mesh::types::AgentCard {
            name: "mesh-test".into(),
            description: "Signed mesh card".into(),
            capabilities: vec!["testing".into()],
            capability_flags: 0,
            input_types: vec!["text/plain".into()],
            output_types: vec!["application/json".into()],
            auth_schemes: vec!["ed25519".into()],
            endpoint_url: "http://127.0.0.1:39780".into(),
            public_key: verifying_key.to_bytes().to_vec(),
            convergence_profile: "standard".into(),
            trust_score: 1.0,
            sybil_lineage_hash: String::new(),
            version: "1.0.0".into(),
            signed_at: chrono::Utc::now(),
            signature: Vec::new(),
            supported_task_types: vec!["analysis".into()],
            default_input_modes: vec!["text/plain".into()],
            default_output_modes: vec!["application/json".into()],
            provider: "ghost-platform".into(),
            a2a_protocol_version: "0.2.0".into(),
        };
        card.sign(&signing_key);

        let card_json = serde_json::to_value(card).unwrap();
        assert!(verify_agent_card_signature(&card_json));
    }

    #[test]
    fn normalize_endpoint_url_accepts_agent_card_and_a2a_urls() {
        assert_eq!(
            super::normalize_a2a_endpoint_url("http://127.0.0.1:39780/.well-known/agent.json"),
            "http://127.0.0.1:39780"
        );
        assert_eq!(
            super::normalize_a2a_endpoint_url("http://127.0.0.1:39780/a2a"),
            "http://127.0.0.1:39780"
        );
        assert_eq!(
            super::normalize_a2a_endpoint_url("http://127.0.0.1:39780/"),
            "http://127.0.0.1:39780"
        );
    }

    #[test]
    fn derive_task_status_uses_remote_mesh_task_status_when_present() {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "status": "working"
            }
        });
        assert_eq!(derive_task_status(StatusCode::OK, &body), "working");
    }

    #[test]
    fn derive_task_status_defaults_to_submitted_for_success_without_status() {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "id": "task-1"
            }
        });
        assert_eq!(derive_task_status(StatusCode::OK, &body), "submitted");
    }

    #[test]
    fn derive_task_status_fails_closed_for_error_status_codes() {
        let body = serde_json::json!({
            "error": "boom"
        });
        assert_eq!(derive_task_status(StatusCode::BAD_GATEWAY, &body), "failed");
    }
}
