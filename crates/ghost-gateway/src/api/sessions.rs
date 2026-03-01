//! Session API endpoints.
//!
//! Phase 2: Sessions are derived from itp_events grouped by session_id.
//! There is no dedicated sessions table — session_id is a column on
//! itp_events (created in v017_convergence_tables migration).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Query parameters for session listing (F15 fix — was hardcoded LIMIT 100).
#[derive(Debug, Deserialize)]
pub struct SessionQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Query parameters for session events listing (T-2.1.1).
#[derive(Debug, Deserialize)]
pub struct SessionEventParams {
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

/// GET /api/sessions — list sessions derived from itp_events.
///
/// Groups itp_events by session_id, returning the first/last timestamp,
/// event count, and sender (agent) for each session.
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SessionQueryParams>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * page_size;

    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            )
                .into_response();
        }
    };

    // Count total sessions.
    let total: u64 = match db.query_row(
        "SELECT COUNT(DISTINCT session_id) FROM itp_events",
        [],
        |row| row.get(0),
    ) {
        Ok(count) => count,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("count query failed: {e}")})),
            )
                .into_response();
        }
    };

    // Use COALESCE on sender to handle NULLs cleanly (F3 fix).
    // GROUP_CONCAT(DISTINCT sender) skips NULLs, but if ALL senders are NULL
    // the result is SQL NULL which would fail row.get::<_, String>.
    let mut stmt = match db.prepare(
        "SELECT session_id, \
                MIN(timestamp) as started_at, \
                MAX(timestamp) as last_event_at, \
                COUNT(*) as event_count, \
                GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown')) as agents \
         FROM itp_events \
         GROUP BY session_id \
         ORDER BY started_at DESC \
         LIMIT ?1 OFFSET ?2",
    ) {
        Ok(stmt) => stmt,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
            )
                .into_response();
        }
    };

    let mut sessions = Vec::new();
    match stmt.query_map(rusqlite::params![page_size, offset], |row| {
        Ok(serde_json::json!({
            "session_id": row.get::<_, String>(0)?,
            "started_at": row.get::<_, String>(1)?,
            "last_event_at": row.get::<_, String>(2)?,
            "event_count": row.get::<_, i64>(3)?,
            "agents": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        }))
    }) {
        Ok(rows) => {
            for row in rows {
                match row {
                    Ok(r) => sessions.push(r),
                    Err(e) => tracing::warn!(error = %e, "skipping malformed session row"),
                }
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query failed: {e}")})),
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "sessions": sessions,
            "page": page,
            "page_size": page_size,
            "total": total,
        })),
    )
        .into_response()
}

/// PII redaction patterns (T-2.1.1 — regex-based, NER deferred to P3).
static PII_PATTERNS: std::sync::LazyLock<Vec<(regex::Regex, &'static str)>> =
    std::sync::LazyLock::new(|| {
        vec![
            (regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(), "[EMAIL]"),
            (regex::Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap(), "[PHONE]"),
            (regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(), "[SSN]"),
        ]
    });

/// Apply regex-based PII redaction to a string.
fn redact_pii(text: &str) -> String {
    let mut result = text.to_string();
    for (pattern, replacement) in PII_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }
    result
}

/// GET /api/sessions/:id/events — list events for a specific session (T-2.1.1).
///
/// Returns paginated events with hash chain verification, cumulative cost,
/// and basic PII redaction on the content fields.
pub async fn session_events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<SessionEventParams>,
) -> ApiResult<serde_json::Value> {
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).min(500);

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Total event count for this session.
    let total: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM itp_events WHERE session_id = ?1",
            [&session_id],
            |row| row.get(0),
        )
        .map_err(|e| ApiError::db_error("session_events_count", e))?;

    if total == 0 {
        return Err(ApiError::not_found(format!(
            "Session {session_id} not found or has no events"
        )));
    }

    // Fetch events ordered by sequence_number.
    let mut stmt = db
        .prepare(
            "SELECT id, event_type, sender, timestamp, sequence_number, \
                    content_hash, content_length, privacy_level, \
                    latency_ms, token_count, \
                    hex(event_hash) as event_hash_hex, \
                    hex(previous_hash) as prev_hash_hex, \
                    attributes \
             FROM itp_events \
             WHERE session_id = ?1 \
             ORDER BY sequence_number ASC \
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| ApiError::db_error("session_events_prepare", e))?;

    let mut events = Vec::new();
    let mut cumulative_cost = 0.0f64;
    let mut prev_hash: Option<String> = None;
    let mut chain_valid = true;

    let rows = stmt
        .query_map(rusqlite::params![&session_id, limit, offset], |row| {
            let event_hash_hex: String = row.get::<_, Option<String>>(10)?.unwrap_or_default();
            let prev_hash_hex: String = row.get::<_, Option<String>>(11)?.unwrap_or_default();
            let token_count: Option<i64> = row.get(9)?;
            let attributes: Option<String> = row.get(12)?;

            Ok((
                serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "event_type": row.get::<_, String>(1)?,
                    "sender": row.get::<_, Option<String>>(2)?,
                    "timestamp": row.get::<_, String>(3)?,
                    "sequence_number": row.get::<_, i64>(4)?,
                    "content_hash": row.get::<_, Option<String>>(5)?,
                    "content_length": row.get::<_, Option<i64>>(6)?,
                    "privacy_level": row.get::<_, String>(7)?,
                    "latency_ms": row.get::<_, Option<i64>>(8)?,
                    "token_count": token_count,
                    "event_hash": &event_hash_hex,
                    "previous_hash": &prev_hash_hex,
                    "attributes": attributes.as_deref()
                        .and_then(|a| serde_json::from_str::<serde_json::Value>(a).ok())
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                }),
                event_hash_hex,
                prev_hash_hex,
                token_count.unwrap_or(0) as f64,
            ))
        })
        .map_err(|e| ApiError::db_error("session_events_query", e))?;

    for row in rows {
        match row {
            Ok((mut event, hash, event_prev_hash, tokens)) => {
                // Hash chain verification: previous event's hash == this event's previous_hash.
                if let Some(ref expected) = prev_hash {
                    if *expected != event_prev_hash {
                        chain_valid = false;
                    }
                }
                prev_hash = Some(hash);

                // Approximate cost: $0.003 per 1K tokens (rough Claude estimate).
                cumulative_cost += tokens * 0.000003;

                // PII redaction on attributes.
                if let Some(attrs) = event.get("attributes") {
                    let redacted = redact_pii(&attrs.to_string());
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&redacted) {
                        event["attributes"] = parsed;
                    }
                }

                events.push(event);
            }
            Err(e) => tracing::warn!(error = %e, "skipping malformed session event row"),
        }
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "events": events,
        "total": total,
        "offset": offset,
        "limit": limit,
        "chain_valid": chain_valid,
        "cumulative_cost": cumulative_cost,
    })))
}
