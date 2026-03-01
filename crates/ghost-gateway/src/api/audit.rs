//! Audit API endpoints (Req 25 AC1-2, Req 30).

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

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
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// GET /api/audit — paginated audit log query.
pub async fn query_audit(Query(params): Query<AuditQueryParams>) -> impl IntoResponse {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);

    // In production, this queries the SQLite audit_log table via ghost-audit.
    // Placeholder returns empty results with correct pagination structure.
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "entries": [],
            "page": page,
            "page_size": page_size,
            "total": 0,
            "filters_applied": {
                "time_start": params.time_start,
                "time_end": params.time_end,
                "agent_id": params.agent_id,
                "event_type": params.event_type,
                "severity": params.severity,
                "tool_name": params.tool_name,
                "search": params.search,
            }
        })),
    )
}

/// GET /api/audit/aggregation — summary statistics.
pub async fn audit_aggregation(
    Query(params): Query<AuditQueryParams>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "violations_per_day": [],
            "top_violation_types": [],
            "policy_denials_by_tool": [],
            "boundary_violations_by_pattern": [],
            "total_entries": 0,
            "agent_id": params.agent_id,
        })),
    )
}

/// GET /api/audit/export — export audit logs in JSON/CSV/JSONL.
pub async fn audit_export(Query(params): Query<AuditExportParams>) -> impl IntoResponse {
    let format = params.format.as_deref().unwrap_or("json");
    let content_type = match format {
        "csv" => "text/csv",
        "jsonl" => "application/x-ndjson",
        _ => "application/json",
    };

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, content_type)],
        match format {
            "csv" => "id,timestamp,agent_id,event_type,severity,tool_name,details,session_id\n"
                .to_string(),
            "jsonl" => String::new(),
            _ => "[]".to_string(),
        },
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportParams {
    pub format: Option<String>,
    pub agent_id: Option<String>,
    pub time_start: Option<String>,
    pub time_end: Option<String>,
}
