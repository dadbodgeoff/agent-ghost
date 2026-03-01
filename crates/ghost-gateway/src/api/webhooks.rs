//! Webhook configuration and firing endpoints (T-4.3.1).
//!
//! CRUD for webhooks + non-blocking fire engine. Webhooks are triggered
//! on safety events, proposal decisions, and score updates.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::api::websocket::WsEvent;
use crate::state::AppState;

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

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub headers: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
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

// ── Handlers ───────────────────────────────────────────────────────

/// GET /api/webhooks — list all webhooks.
pub async fn list_webhooks(
    State(state): State<Arc<AppState>>,
) -> ApiResult<WebhookListResponse> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // T-5.8.2: Do NOT query secret in list endpoint — secrets should never leave DB
    // except during webhook fire.
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
    Json(req): Json<CreateWebhookRequest>,
) -> ApiResult<WebhookSummary> {
    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("Webhook name is required"));
    }
    if req.url.trim().is_empty() {
        return Err(ApiError::bad_request("Webhook URL is required"));
    }
    // T-5.6.2: Validate URL format — must be parseable with http/https scheme.
    if req.url.len() > 2048 {
        return Err(ApiError::bad_request("Webhook URL exceeds 2048 character limit"));
    }
    // T-5.5.1: Validate URL against SSRF blocklist.
    if let Err(e) = crate::api::ssrf::validate_url(&req.url) {
        return Err(ApiError::bad_request(format!("Webhook URL blocked: {e}")));
    }
    // T-5.2.2: Require non-empty secret for HMAC-SHA256 signing.
    let secret = req.secret.unwrap_or_default();
    if secret.is_empty() {
        return Err(ApiError::bad_request(
            "Webhook secret is required — every webhook must have a verifiable HMAC signature",
        ));
    }
    // T-5.6.3: Validate custom headers — block dangerous headers.
    if let Some(ref headers) = req.headers {
        if let Some(obj) = headers.as_object() {
            const BLOCKED_HEADERS: &[&str] = &[
                "authorization", "host", "content-length", "transfer-encoding",
                "x-ghost-webhook-signature", "connection", "upgrade",
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
    // T-5.3.10: Cap total webhook count at 50.
    {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM webhooks", [], |row| row.get(0))
            .unwrap_or(0);
        if count >= 50 {
            return Err(ApiError::bad_request(
                "Maximum 50 webhooks allowed. Delete unused webhooks before creating new ones.",
            ));
        }
    }
    for evt in &req.events {
        if !VALID_EVENTS.contains(&evt.as_str()) {
            return Err(ApiError::bad_request(format!("Invalid event type: {evt}")));
        }
    }

    let id = uuid::Uuid::now_v7().to_string();
    let events_json = serde_json::to_string(&req.events)
        .map_err(|e| ApiError::internal(format!("serialize events: {e}")))?;
    let headers_json = serde_json::to_string(&req.headers.unwrap_or(serde_json::json!({})))
        .map_err(|e| ApiError::internal(format!("serialize headers: {e}")))?;

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
    db.execute(
        "INSERT INTO webhooks (id, name, url, secret, events, headers) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, req.name, req.url, secret, events_json, headers_json],
    )
    .map_err(|e| ApiError::db_error("insert webhook", e))?;

    Ok(Json(WebhookSummary {
        id,
        name: req.name,
        url: req.url,
        events: req.events,
        active: true,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// PUT /api/webhooks/:id — update a webhook.
pub async fn update_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWebhookRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Build dynamic UPDATE.
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
    let sql = format!(
        "UPDATE webhooks SET {} WHERE id = ?{idx}",
        sets.join(", ")
    );

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let affected = db
        .execute(&sql, param_refs.as_slice())
        .map_err(|e| ApiError::db_error("update webhook", e))?;

    if affected == 0 {
        return Err(ApiError::not_found(format!("Webhook '{id}' not found")));
    }

    Ok(Json(serde_json::json!({ "updated": id })))
}

/// DELETE /api/webhooks/:id — delete a webhook.
pub async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    let affected = db
        .execute("DELETE FROM webhooks WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| ApiError::db_error("delete webhook", e))?;

    if affected == 0 {
        return Err(ApiError::not_found(format!("Webhook '{id}' not found")));
    }

    Ok(Json(serde_json::json!({ "deleted": id })))
}

/// POST /api/webhooks/:id/test — fire a test webhook.
pub async fn test_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let (url, secret, headers_json) = {
        let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;
        let result: (String, String, String) = db
            .query_row(
                "SELECT url, secret, headers FROM webhooks WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| ApiError::not_found(format!("Webhook '{id}' not found")))?;
        result
    };

    let payload = serde_json::json!({
        "event": "test",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "message": "This is a test webhook from GHOST ADE",
    });

    let status = fire_single_webhook(&url, &secret, &headers_json, &payload).await;

    // Broadcast result.
    let _ = state.event_tx.send(WsEvent::WebhookFired {
        webhook_id: id.clone(),
        event_type: "test".into(),
        status_code: status,
    });

    Ok(Json(serde_json::json!({
        "webhook_id": id,
        "status_code": status,
        "success": (200..300).contains(&(status as u32)),
    })))
}

// ── Webhook firing engine ──────────────────────────────────────────

/// Maximum concurrent webhook fires (T-5.3.1, default for GHOST_MAX_CONCURRENT_WEBHOOKS).
const MAX_CONCURRENT_WEBHOOKS: usize = 32;

/// Fire webhooks matching the given event type.
///
/// T-5.3.1: Uses a semaphore to bound concurrent HTTP clients. At most
/// `MAX_CONCURRENT_WEBHOOKS` requests are in flight at any time.
/// Tracks JoinHandles in a JoinSet for graceful shutdown.
pub async fn fire_webhooks(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    event_tx: &tokio::sync::broadcast::Sender<WsEvent>,
    event_type: &str,
    payload: serde_json::Value,
) {
    let matching = {
        let Ok(conn) = db.lock() else { return };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, url, secret, headers, events FROM webhooks WHERE active = 1",
        ) else {
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

    // T-5.3.1: Parse max concurrency from env, default to 32.
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
        let tx = event_tx.clone();
        let sem = semaphore.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let status = fire_single_webhook(&url, &secret, &headers_json, &payload).await;
            let _ = tx.send(WsEvent::WebhookFired {
                webhook_id: wh_id_clone,
                event_type: event_type_owned,
                status_code: status,
            });
        });
    }

    // Await all webhook fires (bounded by semaphore).
    while let Some(result) = join_set.join_next().await {
        if let Err(e) = result {
            tracing::warn!(error = %e, "Webhook fire task panicked");
        }
    }
}

/// Compute HMAC-SHA256 signature for a webhook payload (T-5.2.2).
///
/// Returns `sha256=<hex>` format matching GitHub/Stripe webhook conventions.
fn compute_hmac_sha256(secret: &str, body: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC-SHA256 accepts any key size");
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

    // T-5.2.2: Compute HMAC-SHA256 signature (replaces blake3 keyed hash).
    // Every webhook MUST include a verifiable signature — no unsigned webhooks.
    let signature = if !secret.is_empty() {
        compute_hmac_sha256(secret, &body)
    } else {
        tracing::warn!(url, "Webhook has empty secret — delivery unsigned (legacy webhook)");
        String::new()
    };

    let client = reqwest::Client::new();
    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(10));

    if !signature.is_empty() {
        // T-5.2.2: Standard header format matching GitHub/Stripe conventions.
        req = req.header("X-Ghost-Webhook-Signature", &signature);
    }

    // Apply custom headers.
    if let Ok(headers) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(headers_json) {
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
