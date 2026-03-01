//! OTel trace retrieval endpoint (T-3.1.4).
//!
//! Returns spans for a session in OTel-compatible JSON format.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct TraceResponse {
    pub session_id: String,
    pub traces: Vec<TraceGroup>,
    pub total_spans: usize,
}

#[derive(Debug, Serialize)]
pub struct TraceGroup {
    pub trace_id: String,
    pub spans: Vec<SpanRecord>,
}

#[derive(Debug, Serialize)]
pub struct SpanRecord {
    pub span_id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub operation_name: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub attributes: serde_json::Value,
    pub status: String,
}

/// GET /api/traces/:session_id — return OTel spans for a session.
pub async fn get_traces(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> ApiResult<TraceResponse> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    let mut stmt = db
        .prepare(
            "SELECT span_id, trace_id, parent_span_id, operation_name, \
                    start_time, end_time, attributes, status \
             FROM otel_spans WHERE session_id = ?1 \
             ORDER BY start_time ASC",
        )
        .map_err(|e| ApiError::db_error("prepare traces", e))?;

    let spans: Vec<SpanRecord> = stmt
        .query_map(rusqlite::params![session_id], |row| {
            Ok(SpanRecord {
                span_id: row.get(0)?,
                trace_id: row.get(1)?,
                parent_span_id: row.get(2)?,
                operation_name: row.get(3)?,
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                attributes: row
                    .get::<_, String>(6)
                    .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Object(Default::default())))?,
                status: row.get(7)?,
            })
        })
        .map_err(|e| ApiError::db_error("query traces", e))?
        .filter_map(|r| r.ok())
        .collect();

    let total_spans = spans.len();

    // Group spans by trace_id.
    let mut groups: std::collections::BTreeMap<String, Vec<SpanRecord>> =
        std::collections::BTreeMap::new();
    for span in spans {
        groups
            .entry(span.trace_id.clone())
            .or_default()
            .push(span);
    }

    let traces: Vec<TraceGroup> = groups
        .into_iter()
        .map(|(trace_id, spans)| TraceGroup { trace_id, spans })
        .collect();

    Ok(Json(TraceResponse {
        session_id,
        traces,
        total_spans,
    }))
}
