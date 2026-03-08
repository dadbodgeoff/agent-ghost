//! Audit API endpoints (Req 25 AC1-2, Req 30).
//!
//! Phase 2: Wired to ghost_audit backing stores —
//! AuditQueryEngine (paginated queries with dynamic filters),
//! AuditAggregation (summary statistics), AuditExporter (JSON/CSV/JSONL).

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// Query parameters for audit log queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditQueryParams {
    pub time_start: Option<String>,
    pub time_end: Option<String>,
    pub agent_id: Option<String>,
    pub event_type: Option<String>,
    pub severity: Option<String>,
    pub tool_name: Option<String>,
    pub search: Option<String>,
    pub operation_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// GET /api/audit — paginated audit log query with full filter support.
///
/// Delegates to `ghost_audit::AuditQueryEngine::query()` which builds
/// dynamic WHERE clauses from all filter parameters.
pub async fn query_audit(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditQueryParams>,
) -> impl IntoResponse {
    let db = match state.db.read() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database read error"})),
            );
        }
    };

    let engine = ghost_audit::AuditQueryEngine::new(&db);
    let filter = ghost_audit::AuditFilter {
        time_start: params.time_start.clone(),
        time_end: params.time_end.clone(),
        agent_id: params.agent_id.clone(),
        event_type: params.event_type.clone(),
        severity: params.severity.clone(),
        tool_name: params.tool_name.clone(),
        search: params.search.clone(),
        operation_id: params.operation_id.clone(),
        page: params.page.unwrap_or(1),
        page_size: params.page_size.unwrap_or(50).min(200),
    };

    match engine.query(&filter) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "entries": result.items,
                "page": result.page,
                "page_size": result.page_size,
                "total": result.total,
                "filters_applied": {
                    "time_start": params.time_start,
                    "time_end": params.time_end,
                    "agent_id": params.agent_id,
                    "event_type": params.event_type,
                    "severity": params.severity,
                    "tool_name": params.tool_name,
                    "search": params.search,
                    "operation_id": params.operation_id,
                }
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("audit query failed: {e}")})),
        ),
    }
}

/// GET /api/audit/aggregation — summary statistics.
///
/// Delegates to `ghost_audit::AuditAggregation::summarize()` which computes
/// violations_per_day, violations_by_severity, policy_denials_by_tool,
/// and boundary_violations_by_pattern from the audit_log table.
pub async fn audit_aggregation(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditQueryParams>,
) -> impl IntoResponse {
    let db = match state.db.read() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database read error"})),
            );
        }
    };

    let agg = ghost_audit::AuditAggregation::new(&db);
    match agg.summarize(params.agent_id.as_deref()) {
        Ok(result) => match serde_json::to_value(&result) {
            Ok(json) => (StatusCode::OK, Json(json)),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("serialization failed: {e}")})),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("aggregation failed: {e}")})),
        ),
    }
}

/// GET /api/audit/export — export audit logs in JSON/CSV/JSONL.
///
/// Queries all matching entries via AuditQueryEngine, then pipes them
/// through AuditExporter to produce the requested format.
pub async fn audit_export(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditExportParams>,
) -> impl IntoResponse {
    let format_str = params.format.as_deref().unwrap_or("json");
    let export_format = match format_str {
        "csv" => ghost_audit::ExportFormat::Csv,
        "jsonl" => ghost_audit::ExportFormat::Jsonl,
        _ => ghost_audit::ExportFormat::Json,
    };
    let content_type = match format_str {
        "csv" => "text/csv",
        "jsonl" => "application/x-ndjson",
        _ => "application/json",
    };

    let db = match state.db.read() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                "{\"error\": \"database read error\"}".to_string(),
            );
        }
    };

    // Construct filter field-by-field (not ..Default::default()) so that
    // adding a new field to AuditFilter causes a compile error here,
    // forcing the export handler to be updated (F25 fix).
    let engine = ghost_audit::AuditQueryEngine::new(&db);
    let filter = ghost_audit::AuditFilter {
        time_start: params.time_start.clone(),
        time_end: params.time_end.clone(),
        agent_id: params.agent_id.clone(),
        event_type: None,
        severity: None,
        tool_name: None,
        search: None,
        operation_id: None,
        page: 1,
        page_size: 10_000, // Export up to 10k entries
    };

    let entries = match engine.query(&filter) {
        Ok(result) => result.items,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                format!("{{\"error\": \"export query failed: {e}\"}}"),
            );
        }
    };

    let mut buf = Vec::new();
    if let Err(e) = ghost_audit::AuditExporter::export(&entries, export_format, &mut buf) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            format!("{{\"error\": \"export failed: {e}\"}}"),
        );
    }

    let body = String::from_utf8_lossy(&buf).to_string();
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, content_type)],
        body,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportParams {
    pub format: Option<String>,
    pub agent_id: Option<String>,
    pub time_start: Option<String>,
    pub time_end: Option<String>,
}
