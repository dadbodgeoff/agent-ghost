use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ItpEventsQueryParams {
    pub limit: Option<u32>,
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ItpEventsQueryParams>,
) -> ApiResult<serde_json::Value> {
    let limit = params.limit.unwrap_or(200).min(500);
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("itp_events", e))?;

    let total: u64 = db
        .query_row("SELECT COUNT(*) FROM itp_events", [], |row| row.get(0))
        .map_err(|e| ApiError::db_error("itp_events_count", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, event_type, sender, session_id, timestamp
             FROM itp_events
             ORDER BY timestamp DESC
             LIMIT ?1",
        )
        .map_err(|e| ApiError::db_error("itp_events_prepare", e))?;

    let rows = stmt
        .query_map(rusqlite::params![limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "event_type": row.get::<_, String>(1)?,
                "platform": "gateway",
                "session_id": row.get::<_, String>(3)?,
                "timestamp": row.get::<_, String>(4)?,
                "source": row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "gateway".to_string()),
            }))
        })
        .map_err(|e| ApiError::db_error("itp_events_query", e))?;

    let mut events = Vec::new();
    for row in rows {
        match row {
            Ok(event) => events.push(event),
            Err(e) => tracing::warn!(error = %e, "skipping malformed itp event row"),
        }
    }

    Ok(Json(serde_json::json!({
        "events": events,
        "buffer_count": total,
        "extension_connected": state.monitor_healthy.load(Ordering::Relaxed),
    })))
}
