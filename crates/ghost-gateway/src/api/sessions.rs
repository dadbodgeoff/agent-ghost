//! Session API endpoints.
//!
//! Phase 2: Sessions are derived from itp_events grouped by session_id.
//! There is no dedicated sessions table — session_id is a column on
//! itp_events (created in v017_convergence_tables migration).

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::state::AppState;

/// Query parameters for session listing (F15 fix — was hardcoded LIMIT 100).
#[derive(Debug, Deserialize)]
pub struct SessionQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
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
