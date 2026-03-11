use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use rusqlite::types::ToSql;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ItpEventsQueryParams {
    /// Maximum number of rows to return. Default 100, max 500.
    pub limit: Option<u32>,
    /// Row offset for snapshot pagination.
    pub offset: Option<u32>,
    /// Filter to a specific runtime session.
    pub session_id: Option<String>,
    /// Filter to a specific event type.
    pub event_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ItpEvent {
    pub id: String,
    pub event_type: String,
    pub session_id: String,
    pub timestamp: String,
    pub sequence_number: i64,
    pub sender: Option<String>,
    pub source: Option<String>,
    pub platform: Option<String>,
    pub route: Option<String>,
    pub privacy_level: String,
    pub content_length: Option<i64>,
    pub session_path: String,
    pub replay_path: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ItpEventsResponse {
    pub events: Vec<ItpEvent>,
    pub limit: u32,
    pub offset: u32,
    /// Total persisted rows across the entire `itp_events` table.
    pub total_persisted: u64,
    /// Total rows that match the applied filters before pagination.
    pub total_filtered: u64,
    /// Count of rows returned in this response page.
    pub returned: u32,
    /// Truthful monitor connectivity bit from gateway health state.
    pub monitor_connected: bool,
    /// Indicates whether the client can keep this view fresh through WS + resync handling.
    pub live_updates_supported: bool,
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ItpEventsQueryParams>,
) -> ApiResult<ItpEventsResponse> {
    let limit = params.limit.unwrap_or(100).min(500);
    let offset = params.offset.unwrap_or(0);
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("itp_events", e))?;

    let total_persisted: u64 = db
        .query_row("SELECT COUNT(*) FROM itp_events", [], |row| row.get(0))
        .map_err(|e| ApiError::db_error("itp_events_count", e))?;

    let mut where_clauses = Vec::new();
    let mut query_params: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(session_id) = params.session_id.clone() {
        where_clauses.push(format!("session_id = ?{}", query_params.len() + 1));
        query_params.push(Box::new(session_id));
    }

    if let Some(event_type) = params.event_type.clone() {
        where_clauses.push(format!("event_type = ?{}", query_params.len() + 1));
        query_params.push(Box::new(event_type));
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM itp_events{where_sql}");
    let total_filtered: u64 = {
        let mut stmt = db
            .prepare(&count_sql)
            .map_err(|e| ApiError::db_error("itp_events_count_prepare", e))?;
        let param_refs: Vec<&dyn ToSql> = query_params.iter().map(|p| p.as_ref()).collect();
        stmt.query_row(param_refs.as_slice(), |row| row.get(0))
            .map_err(|e| ApiError::db_error("itp_events_filtered_count", e))?
    };

    let mut select_params = query_params;
    let limit_index = select_params.len() + 1;
    let offset_index = select_params.len() + 2;
    select_params.push(Box::new(i64::from(limit)));
    select_params.push(Box::new(i64::from(offset)));

    let select_sql = format!(
        "SELECT id, event_type, sender, session_id, timestamp, sequence_number, \
                content_length, privacy_level, attributes \
         FROM itp_events{where_sql} \
         ORDER BY timestamp DESC, sequence_number DESC \
         LIMIT ?{limit_index} OFFSET ?{offset_index}"
    );

    let mut stmt = db
        .prepare(&select_sql)
        .map_err(|e| ApiError::db_error("itp_events_prepare", e))?;

    let param_refs: Vec<&dyn ToSql> = select_params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            let session_id: String = row.get(3)?;
            let attributes_raw: Option<String> = row.get(8)?;
            let attributes = attributes_raw
                .as_deref()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());

            Ok(ItpEvent {
                id: row.get::<_, String>(0)?,
                event_type: row.get::<_, String>(1)?,
                sender: row.get::<_, Option<String>>(2)?,
                session_id: session_id.clone(),
                timestamp: row.get::<_, String>(4)?,
                sequence_number: row.get::<_, i64>(5)?,
                content_length: row.get::<_, Option<i64>>(6)?,
                privacy_level: row.get::<_, String>(7)?,
                source: json_string_field(attributes.as_ref(), "source"),
                platform: json_string_field(attributes.as_ref(), "platform"),
                route: json_string_field(attributes.as_ref(), "route"),
                session_path: format!("/sessions/{session_id}"),
                replay_path: format!("/sessions/{session_id}/replay"),
            })
        })
        .map_err(|e| ApiError::db_error("itp_events_query", e))?;

    let mut events = Vec::new();
    for row in rows {
        match row {
            Ok(event) => events.push(event),
            Err(e) => tracing::warn!(error = %e, "skipping malformed itp event row"),
        }
    }

    Ok(Json(ItpEventsResponse {
        returned: events.len() as u32,
        events,
        limit,
        offset,
        total_persisted,
        total_filtered,
        monitor_connected: state.monitor_healthy.load(Ordering::Relaxed),
        live_updates_supported: true,
    }))
}

fn json_string_field(value: Option<&serde_json::Value>, key: &str) -> Option<String> {
    value
        .and_then(|json| json.get(key))
        .and_then(|field| field.as_str())
        .map(|field| field.to_string())
}
