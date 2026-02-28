//! Agent API endpoints.

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct AgentInfo {
    pub name: String,
    pub status: String,
}

/// GET /api/agents
pub async fn list_agents() -> Json<Vec<AgentInfo>> {
    Json(Vec::new())
}
