//! Session API endpoints.

use axum::Json;

/// GET /api/sessions
pub async fn list_sessions() -> Json<Vec<serde_json::Value>> {
    Json(Vec::new())
}
