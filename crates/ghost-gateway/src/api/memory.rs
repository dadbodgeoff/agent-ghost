//! Memory API endpoints (Req 25 AC1-2).

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

/// Query parameters for memory listing.
#[derive(Debug, Deserialize)]
pub struct MemoryQueryParams {
    pub agent_id: Option<String>,
    pub memory_type: Option<String>,
    pub importance: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// GET /api/memory — list memories with filtering.
pub async fn list_memories(Query(params): Query<MemoryQueryParams>) -> impl IntoResponse {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "memories": [],
            "page": page,
            "page_size": page_size,
            "total": 0,
        })),
    )
}

/// GET /api/memory/:id — get a specific memory.
pub async fn get_memory(Path(id): Path<String>) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "memory not found", "id": id})),
    )
}
