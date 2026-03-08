//! Gateway-mediated A2A task endpoints (T-4.1.2).
//!
//! These endpoints provide a dashboard-friendly interface for sending tasks
//! to external A2A agents, checking task status, and discovering agents
//! on the mesh. The raw A2A protocol endpoint lives at `/a2a`.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Response;
use axum::Extension;
use axum::Json;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation, prepare_json_operation,
    PreparedOperation,
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

    let jsonrpc_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": task_id,
        "method": method,
        "params": {
            "id": task_id,
            "message": {
                "role": "user",
                "parts": [{ "type": "text", "text": req.input }],
            }
        }
    });

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
            return json_response_with_idempotency(
                stored.status,
                stored.body,
                IdempotencyStatus::Replayed,
            );
        }
        Ok(PreparedOperation::Mismatch) => {
            return error_response_with_idempotency(ApiError::with_details(
                StatusCode::CONFLICT,
                "IDEMPOTENCY_KEY_REUSED",
                "Idempotency key was reused with a different request payload",
                serde_json::json!({
                    "route_template": SEND_TASK_ROUTE_TEMPLATE,
                    "method": "POST",
                }),
            ));
        }
        Ok(PreparedOperation::InProgress) => {
            return error_response_with_idempotency(ApiError::custom(
                StatusCode::CONFLICT,
                "IDEMPOTENCY_IN_PROGRESS",
                "An equivalent request is already in progress",
            ));
        }
        Ok(PreparedOperation::Acquired { journal_id }) => {
            let input_str = serde_json::to_string(&req.input).unwrap_or_else(|_| "null".into());
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
                    let _ = abort_prepared_json_operation(&db, &journal_id);
                    return error_response_with_idempotency(ApiError::db_error(
                        "insert a2a task",
                        error,
                    ));
                }
            }

            let client = reqwest::Client::new();
            let resp = match client
                .post(&verified_agent.endpoint_url)
                .header("Content-Type", "application/json")
                .timeout(std::time::Duration::from_secs(30))
                .json(&jsonrpc_req)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let db = state.db.write().await;
                    let _ = db.execute(
                        "UPDATE a2a_tasks SET status = 'failed', output = ?1 WHERE id = ?2",
                        rusqlite::params![
                            serde_json::json!({"error": format!("A2A request failed: {error}")})
                                .to_string(),
                            task_id
                        ],
                    );
                    let _ = abort_prepared_json_operation(&db, &journal_id);
                    return error_response_with_idempotency(ApiError::internal(format!(
                        "A2A request failed: {error}"
                    )));
                }
            };

            let status_code = resp.status();
            let body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "failed to parse response"}));

            let task_status = if status_code.is_success() {
                "submitted"
            } else {
                "failed"
            };

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

            crate::api::websocket::broadcast_event(
                &state,
                WsEvent::A2ATaskUpdate {
                    task_id: task_id.clone(),
                    status: task_status.into(),
                    agent_name: agent_name.clone(),
                },
            );

            match commit_prepared_json_operation(
                &db,
                &operation_context,
                &journal_id,
                StatusCode::OK,
                &response_body,
            ) {
                Ok(outcome) => {
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
                    return json_response_with_idempotency(
                        outcome.status,
                        outcome.body,
                        outcome.idempotency_status,
                    );
                }
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &journal_id);
                    return error_response_with_idempotency(error);
                }
            }
        }
        Err(error) => return error_response_with_idempotency(error),
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
                        .data(r#"{"reason":"inactivity_timeout","timeout_seconds":300}"#.to_string()));
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
    // 1. Collect known peer endpoint URLs from the DB.
    let peer_urls: Vec<String> = {
        let db = state
            .db
            .read()
            .map_err(|e| ApiError::db_error("discover_agents_read_peers", e))?;
        let mut urls = Vec::new();
        let stmt_result =
            db.prepare("SELECT endpoint_url FROM discovered_agents WHERE endpoint_url IS NOT NULL");
        if let Ok(mut stmt) = stmt_result {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                urls.extend(rows.filter_map(|r| r.ok()));
            }
        }
        urls
    };

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
                    agent.endpoint_url,
                    agent.name,
                    agent.description,
                    caps_json,
                    agent.trust_score,
                    agent.version,
                ],
            );
        }
    }

    // 5. Re-read the full table so we return any agents not in our probe list too.
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("discover_agents_reread", e))?;
    let mut agents: Vec<DiscoveredAgent> = Vec::new();

    let stmt_result = db.prepare(
        "SELECT name, description, endpoint_url, capabilities, trust_score, version \
         FROM discovered_agents ORDER BY trust_score DESC",
    );

    if let Ok(mut stmt) = stmt_result {
        let rows = stmt.query_map([], |row| {
            let caps_json: String = row.get::<_, String>(3).unwrap_or_else(|_| "[]".into());
            let endpoint: String = row.get(2)?;
            // Mark reachable based on probe results.
            let reachable = probed_agents
                .iter()
                .any(|a| a.endpoint_url == endpoint && a.reachable);
            Ok(DiscoveredAgent {
                name: row.get(0)?,
                description: row.get(1)?,
                endpoint_url: endpoint,
                capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
                trust_score: row.get(4)?,
                version: row.get(5)?,
                reachable,
            })
        });

        if let Ok(rows) = rows {
            agents.extend(rows.filter_map(|r| r.ok()));
        }
    }

    Ok(Json(DiscoverResponse { agents }))
}

/// T-5.2.3: Verify the Ed25519 signature on an agent card JSON.
///
/// The agent card should contain `public_key` (hex-encoded 32 bytes) and
/// `signature` (hex-encoded 64 bytes). The signature is verified against
/// the card body minus the signature field itself.
fn verify_agent_card_signature(card: &serde_json::Value) -> bool {
    let public_key_hex = match card.get("public_key").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => return false, // No public key in card
    };

    let signature_hex = match card.get("signature").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return false, // No signature in card
    };

    // Decode public key from hex.
    let pk_bytes: Vec<u8> = match (0..public_key_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&public_key_hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
    {
        Ok(b) if b.len() == 32 => b,
        _ => return false,
    };

    let pk_array: [u8; 32] = match pk_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };

    let verifying_key = match ghost_signing::VerifyingKey::from_bytes(&pk_array) {
        Some(k) => k,
        None => return false,
    };

    // Decode signature from hex.
    let sig_bytes: Vec<u8> = match (0..signature_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&signature_hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
    {
        Ok(b) if b.len() == 64 => b,
        _ => return false,
    };

    let signature = match ghost_signing::Signature::from_bytes(&sig_bytes) {
        Some(s) => s,
        None => return false,
    };

    // Build the canonical card body (card without the signature field) for verification.
    let mut card_for_signing = card.clone();
    if let Some(obj) = card_for_signing.as_object_mut() {
        obj.remove("signature");
    }
    let canonical = serde_json::to_string(&card_for_signing).unwrap_or_default();

    ghost_signing::verify(canonical.as_bytes(), &signature, &verifying_key)
}

fn normalize_a2a_endpoint_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
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
