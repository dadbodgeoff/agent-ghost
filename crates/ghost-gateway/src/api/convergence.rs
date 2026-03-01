//! Convergence score API endpoints.
//!
//! Phase 2: Wired to cortex_storage convergence_score_queries.
//! For each registered agent, queries the latest convergence score
//! from the convergence_scores table.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct ConvergenceScoreResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub score: f64,
    pub level: i32,
    pub profile: String,
    pub signal_scores: serde_json::Value,
    pub computed_at: Option<String>,
}

/// GET /api/convergence/scores
///
/// Returns the latest convergence score for each registered agent.
/// Queries convergence_scores table via cortex_storage::queries.
pub async fn get_scores(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let agents = match state.agents.read() {
        Ok(agents) => agents,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "agent registry lock poisoned"})),
            )
                .into_response();
        }
    };
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

    let mut scores = Vec::new();
    let mut errors = Vec::new();

    for a in agents.all_agents() {
        let agent_id_str = a.id.to_string();
        match cortex_storage::queries::convergence_score_queries::latest_by_agent(
            &db,
            &agent_id_str,
        ) {
            Ok(Some(row)) => {
                let signal_scores = serde_json::from_str(&row.signal_scores)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                scores.push(ConvergenceScoreResponse {
                    agent_id: agent_id_str,
                    agent_name: a.name.clone(),
                    score: row.composite_score,
                    level: row.level,
                    profile: row.profile,
                    signal_scores,
                    computed_at: Some(row.computed_at),
                });
            }
            Ok(None) => {
                // No score computed yet — return defaults.
                scores.push(ConvergenceScoreResponse {
                    agent_id: agent_id_str,
                    agent_name: a.name.clone(),
                    score: 0.0,
                    level: 0,
                    profile: "standard".into(),
                    signal_scores: serde_json::Value::Object(serde_json::Map::new()),
                    computed_at: None,
                });
            }
            Err(e) => {
                tracing::error!(agent_id = %agent_id_str, error = %e, "DB error querying convergence score");
                errors.push(serde_json::json!({
                    "agent_id": agent_id_str,
                    "agent_name": a.name.clone(),
                    "error": "database query failed",
                }));
            }
        }
    }

    if !errors.is_empty() && scores.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "database query failed for all agents"})),
        )
            .into_response();
    }

    // Surface partial failures so API consumers know which agents had errors (F23 fix).
    let mut response = serde_json::json!({"scores": scores});
    if !errors.is_empty() {
        response["errors"] = serde_json::json!(errors);
    }

    (StatusCode::OK, Json(response)).into_response()
}
