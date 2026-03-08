//! Webhook configuration and firing endpoints (T-4.3.1).
//!
//! CRUD for webhooks + non-blocking fire engine. Webhooks are triggered
//! on safety events, proposal decisions, and score updates.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation,
    execute_idempotent_json_mutation, prepare_json_operation, PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::websocket::WsEvent;
use crate::state::AppState;

const CREATE_WEBHOOK_ROUTE_TEMPLATE: &str = "/api/webhooks";
const UPDATE_WEBHOOK_ROUTE_TEMPLATE: &str = "/api/webhooks/:id";
const DELETE_WEBHOOK_ROUTE_TEMPLATE: &str = "/api/webhooks/:id";
const TEST_WEBHOOK_ROUTE_TEMPLATE: &str = "/api/webhooks/:id/test";

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct WebhookSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub headers: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateWebhookRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub active: Option<bool>,
    pub headers: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct WebhookListResponse {
    pub webhooks: Vec<WebhookSummary>,
}

// ── Valid event types ──────────────────────────────────────────────

const VALID_EVENTS: &[&str] = &[
    "intervention_change",
    "kill_switch",
    "proposal_decision",
    "agent_state_change",
    "score_update",
    "backup_complete",
];

fn webhook_actor(claims: Option<&Claims>) -> &str {
    claims.map(|claims| claims.sub.as_str()).unwrap_or("unknown")
}

fn validate_webhook_url(url: &str) -> Result<(), ApiError> {
    if url.trim().is_empty() {
        return Err(ApiError::bad_request("Webhook URL is required"));
    }
    if url.len() > 2048 {
        return Err(ApiError::bad_request(
            "Webhook URL exceeds 2048 character limit",
        ));
    }
    crate::api::ssrf::validate_url(url)
        .map_err(|e| ApiError::bad_request(format!("Webhook URL blocked: {e}")))?;
    Ok(())
}

fn validate_webhook_headers(headers: &Option<serde_json::Value>) -> Result<(), ApiError> {
    if let Some(headers) = headers {
        if let Some(obj) = headers.as_object() {
            const BLOCKED_HEADERS: &[&str] = &[
                "authorization",
                "host",
                "content-length",
                "transfer-encoding",
                "x-ghost-webhook-signature",
                "connection",
                "upgrade",
            ];
            if obj.len() > 10 {
                return Err(ApiError::bad_request(
                    "Maximum 10 custom headers per webhook",
                ));
            }
            for (key, val) in obj {
                if BLOCKED_HEADERS.contains(&key.to_lowercase().as_str()) {
                    return Err(ApiError::bad_request(format!(
                        "Header '{key}' is reserved and cannot be used as a custom header"
                    )));
                }
                if let Some(v) = val.as_str() {
                    if v.len() > 256 {
                        return Err(ApiError::bad_request(format!(
                            "Header '{key}' value exceeds 256 character limit"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_webhook_events(events: &[String]) -> Result<(), ApiError> {
    for evt in events {
        if !VALID_EVENTS.contains(&evt.as_str()) {
            return Err(ApiError::bad_request(format!("Invalid event type: {evt}")));
        }
    }
    Ok(())
}

// ── Handlers ───────────────────────────────────────────────────────

/// GET /api/webhooks — list all webhooks.
pub async fn list_webhooks(State(state): State<Arc<AppState>>) -> ApiResult<WebhookListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_webhooks", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, name, url, events, active, created_at, updated_at \
             FROM webhooks ORDER BY created_at DESC",
        )
        .map_err(|e| ApiError::db_error("prepare webhook list", e))?;

    let webhooks: Vec<WebhookSummary> = stmt
        .query_map([], |row| {
            let events_json: String = row.get::<_, String>(3).unwrap_or_else(|_| "[]".into());
            Ok(WebhookSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                events: serde_json::from_str(&events_json).unwrap_or_default(),
                active: row.get::<_, i64>(4).unwrap_or(1) != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| ApiError::db_error("query webhooks", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(WebhookListResponse { webhooks }))
}

/// POST /api/webhooks — create a webhook.
pub async fn create_webhook(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<CreateWebhookRequest>,
) -> Response {
    if req.name.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request(
            "Webhook name is required",
        ));
    }
    if let Err(error) = validate_webhook_url(&req.url) {
        return error_response_with_idempotency(error);
    }
    if let Err(error) = validate_webhook_events(&req.events) {
        return error_response_with_idempotency(error);
    }
    if let Err(error) = validate_webhook_headers(&req.headers) {
        return error_response_with_idempotency(error);
    }

    let secret = req.secret.clone().unwrap_or_default();
    if secret.is_empty() {
        return error_response_with_idempotency(ApiError::bad_request(
            "Webhook secret is required — every webhook must have a verifiable HMAC signature",
        ));
    }

    {
        let db = state
            .db
            .read()
            .map_err(|e| ApiError::db_error("webhook_count_check", e));
        match db {
            Ok(db) => {
                let count: i64 = db
                    .query_row("SELECT COUNT(*) FROM webhooks", [], |row| row.get(0))
                    .unwrap_or(0);
                if count >= 50 {
                    return error_response_with_idempotency(ApiError::bad_request(
                        "Maximum 50 webhooks allowed. Delete unused webhooks before creating new ones.",
                    ));
                }
            }
            Err(error) => return error_response_with_idempotency(error),
        }
    }

    let actor = webhook_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "name": req.name,
        "url": req.url,
        "events": req.events,
        "headers": req.headers,
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_WEBHOOK_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let id = operation_context
                .operation_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
            let events_json = serde_json::to_string(&req.events)
                .map_err(|e| ApiError::internal(format!("serialize events: {e}")))?;
            let headers_json =
                serde_json::to_string(&req.headers.clone().unwrap_or(serde_json::json!({})))
                    .map_err(|e| ApiError::internal(format!("serialize headers: {e}")))?;

            conn.execute(
                "INSERT INTO webhooks (id, name, url, secret, events, headers) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![id, req.name, req.url, secret, events_json, headers_json],
            )
            .map_err(|e| ApiError::db_error("insert webhook", e))?;

            Ok((
                StatusCode::CREATED,
                serde_json::to_value(WebhookSummary {
                    id,
                    name: req.name.clone(),
                    url: req.url.clone(),
                    events: req.events.clone(),
                    active: true,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "create_webhook",
                "high",
                actor,
                "created",
                serde_json::json!({
                    "webhook_id": outcome.body["id"],
                    "name": outcome.body["name"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// PUT /api/webhooks/:id — update a webhook.
pub async fn update_webhook(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Response {
    if let Some(url) = &req.url {
        if let Err(error) = validate_webhook_url(url) {
            return error_response_with_idempotency(error);
        }
    }
    if let Some(events) = &req.events {
        if let Err(error) = validate_webhook_events(events) {
            return error_response_with_idempotency(error);
        }
    }
    if let Err(error) = validate_webhook_headers(&req.headers) {
        return error_response_with_idempotency(error);
    }

    let actor = webhook_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "webhook_id": id,
        "body": req,
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "PUT",
        UPDATE_WEBHOOK_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let mut sets: Vec<String> = vec!["updated_at = datetime('now')".into()];
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1u32;

            if let Some(name) = &req.name {
                sets.push(format!("name = ?{idx}"));
                params.push(Box::new(name.clone()));
                idx += 1;
            }
            if let Some(url) = &req.url {
                sets.push(format!("url = ?{idx}"));
                params.push(Box::new(url.clone()));
                idx += 1;
            }
            if let Some(events) = &req.events {
                let json = serde_json::to_string(events).unwrap_or_else(|_| "[]".into());
                sets.push(format!("events = ?{idx}"));
                params.push(Box::new(json));
                idx += 1;
            }
            if let Some(active) = req.active {
                sets.push(format!("active = ?{idx}"));
                params.push(Box::new(active as i64));
                idx += 1;
            }
            if let Some(headers) = &req.headers {
                let json = serde_json::to_string(headers).unwrap_or_else(|_| "{}".into());
                sets.push(format!("headers = ?{idx}"));
                params.push(Box::new(json));
                idx += 1;
            }

            params.push(Box::new(id.clone()));
            let sql = format!("UPDATE webhooks SET {} WHERE id = ?{idx}", sets.join(", "));

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let affected = conn
                .execute(&sql, param_refs.as_slice())
                .map_err(|e| ApiError::db_error("update webhook", e))?;

            if affected == 0 {
                return Err(ApiError::not_found(format!("Webhook '{id}' not found")));
            }

            Ok((
                StatusCode::OK,
                serde_json::json!({ "updated": id }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "update_webhook",
                "high",
                actor,
                "updated",
                serde_json::json!({ "webhook_id": id }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// DELETE /api/webhooks/:id — delete a webhook.
pub async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(id): Path<String>,
) -> Response {
    let actor = webhook_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "webhook_id": id.clone() });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "DELETE",
        DELETE_WEBHOOK_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let affected = conn
                .execute("DELETE FROM webhooks WHERE id = ?1", rusqlite::params![id])
                .map_err(|e| ApiError::db_error("delete webhook", e))?;

            if affected == 0 {
                return Err(ApiError::not_found(format!("Webhook '{id}' not found")));
            }

            Ok((
                StatusCode::OK,
                serde_json::json!({ "deleted": id }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "delete_webhook",
                "high",
                actor,
                "deleted",
                serde_json::json!({ "webhook_id": id }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/webhooks/:id/test — fire a test webhook.
pub async fn test_webhook(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(id): Path<String>,
) -> Response {
    let actor = webhook_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "webhook_id": id.clone() });

    let prepared = {
        let db = state.db.write().await;
        prepare_json_operation(
            &db,
            &operation_context,
            actor,
            "POST",
            TEST_WEBHOOK_ROUTE_TEMPLATE,
            &request_body,
        )
    };

    match prepared {
        Ok(PreparedOperation::Replay(stored)) => {
            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                "platform",
                "test_webhook",
                "high",
                actor,
                "replayed",
                serde_json::json!({
                    "webhook_id": id,
                    "status_code": stored.body.get("status_code"),
                }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(
                stored.status,
                stored.body,
                IdempotencyStatus::Replayed,
            )
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": TEST_WEBHOOK_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { journal_id }) => {
            let webhook_config = {
                let db = match state.db.read() {
                    Ok(db) => db,
                    Err(error) => {
                        let db = state.db.write().await;
                        let _ = abort_prepared_json_operation(&db, &journal_id);
                        return error_response_with_idempotency(ApiError::db_error(
                            "test_webhook",
                            error,
                        ));
                    }
                };
                db.query_row(
                    "SELECT url, secret, headers FROM webhooks WHERE id = ?1",
                    rusqlite::params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .ok()
            };

            let Some((url, secret, headers_json)) = webhook_config else {
                let db = state.db.write().await;
                let _ = abort_prepared_json_operation(&db, &journal_id);
                return error_response_with_idempotency(ApiError::not_found(format!(
                    "Webhook '{id}' not found"
                )));
            };

            let payload = serde_json::json!({
                "event": "test",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "message": "This is a test webhook from GHOST ADE",
            });

            let status = fire_single_webhook(&url, &secret, &headers_json, &payload).await;
            crate::api::websocket::broadcast_event(
                &state,
                WsEvent::WebhookFired {
                    webhook_id: id.clone(),
                    event_type: "test".into(),
                    status_code: status,
                },
            );

            let response_body = serde_json::json!({
                "webhook_id": id,
                "status_code": status,
                "success": (200..300).contains(&(status as u32)),
            });

            let db = state.db.write().await;
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
                        "platform",
                        "test_webhook",
                        "high",
                        actor,
                        "executed",
                        serde_json::json!({
                            "webhook_id": outcome.body["webhook_id"],
                            "status_code": outcome.body["status_code"],
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
                Err(error) => error_response_with_idempotency(error),
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

// ── Webhook firing engine ──────────────────────────────────────────

/// Maximum concurrent webhook fires (T-5.3.1, default for GHOST_MAX_CONCURRENT_WEBHOOKS).
const MAX_CONCURRENT_WEBHOOKS: usize = 32;

/// Fire webhooks matching the given event type.
pub async fn fire_webhooks(
    db: &std::sync::Arc<crate::db_pool::DbPool>,
    app_state: &std::sync::Arc<crate::state::AppState>,
    event_type: &str,
    payload: serde_json::Value,
) {
    let matching = {
        let Ok(conn) = db.read() else { return };
        let Ok(mut stmt) =
            conn.prepare("SELECT id, url, secret, headers, events FROM webhooks WHERE active = 1")
        else {
            return;
        };
        let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }) else {
            return;
        };
        let event_type_owned = event_type.to_string();
        rows.filter_map(|r| r.ok())
            .filter(|(_, _, _, _, events_json)| {
                let events: Vec<String> = serde_json::from_str(events_json).unwrap_or_default();
                events.contains(&event_type_owned)
            })
            .collect::<Vec<_>>()
    };

    if matching.is_empty() {
        return;
    }

    let max_concurrent: usize = std::env::var("GHOST_MAX_CONCURRENT_WEBHOOKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MAX_CONCURRENT_WEBHOOKS);

    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let mut join_set = tokio::task::JoinSet::new();

    for (wh_id, url, secret, headers_json, _) in matching {
        let payload = payload.clone();
        let wh_id_clone = wh_id.clone();
        let event_type_owned = event_type.to_string();
        let state_ref = Arc::clone(app_state);
        let sem = semaphore.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let status = fire_single_webhook(&url, &secret, &headers_json, &payload).await;
            crate::api::websocket::broadcast_event(
                &state_ref,
                WsEvent::WebhookFired {
                    webhook_id: wh_id_clone,
                    event_type: event_type_owned,
                    status_code: status,
                },
            );
        });
    }

    while let Some(result) = join_set.join_next().await {
        if let Err(e) = result {
            tracing::warn!(error = %e, "Webhook fire task panicked");
        }
    }
}

/// Compute HMAC-SHA256 signature for a webhook payload (T-5.2.2).
fn compute_hmac_sha256(secret: &str, body: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC-SHA256 accepts any key size");
    mac.update(body.as_bytes());
    let result = mac.finalize();
    let hex: String = result
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256={hex}")
}

async fn fire_single_webhook(
    url: &str,
    secret: &str,
    headers_json: &str,
    payload: &serde_json::Value,
) -> u16 {
    let body = serde_json::to_string(payload).unwrap_or_default();

    let signature = if !secret.is_empty() {
        compute_hmac_sha256(secret, &body)
    } else {
        tracing::warn!(
            url,
            "Webhook has empty secret — delivery unsigned (legacy webhook)"
        );
        String::new()
    };

    let client = reqwest::Client::new();
    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(10));

    if !signature.is_empty() {
        req = req.header("X-Ghost-Webhook-Signature", &signature);
    }

    if let Ok(headers) =
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(headers_json)
    {
        for (key, val) in headers {
            if let Some(v) = val.as_str() {
                req = req.header(key.as_str(), v);
            }
        }
    }

    match req.body(body).send().await {
        Ok(resp) => resp.status().as_u16(),
        Err(e) => {
            tracing::warn!(url, error = %e, "Webhook delivery failed");
            0
        }
    }
}
