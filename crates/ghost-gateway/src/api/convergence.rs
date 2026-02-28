//! Convergence score API endpoints.

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct ConvergenceScoreResponse {
    pub agent_id: String,
    pub score: f64,
    pub level: u8,
}

/// GET /api/convergence/scores
pub async fn get_scores() -> Json<Vec<ConvergenceScoreResponse>> {
    Json(Vec::new())
}
