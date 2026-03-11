//! Convergence score API endpoints.
//!
//! Phase 2: Wired to cortex_storage convergence_score_queries.
//! For each registered agent, queries the latest convergence score
//! from the convergence_scores table.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ConvergenceHistoryQueryParams {
    pub since: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConvergenceScoreResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub score: f64,
    pub level: i32,
    pub profile: String,
    #[schema(value_type = std::collections::BTreeMap<String, f64>)]
    pub signal_scores: serde_json::Value,
    pub computed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConvergenceErrorResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConvergenceScoresResponse {
    pub scores: Vec<ConvergenceScoreResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ConvergenceErrorResponse>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConvergenceHistoryEntryResponse {
    pub session_id: Option<String>,
    pub score: f64,
    pub level: i32,
    pub profile: String,
    #[schema(value_type = std::collections::BTreeMap<String, f64>)]
    pub signal_scores: serde_json::Value,
    pub computed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConvergenceHistoryResponse {
    pub agent_id: String,
    pub entries: Vec<ConvergenceHistoryEntryResponse>,
}

/// GET /api/convergence/scores
///
/// Returns the latest convergence score for each registered agent.
/// Queries convergence_scores table via cortex_storage::queries.
pub async fn get_scores(
    State(state): State<Arc<AppState>>,
) -> ApiResult<ConvergenceScoresResponse> {
    let agents = state
        .agents
        .read()
        .map_err(|_| ApiError::lock_poisoned("agent registry"))?;
    let db = state
        .db
        .read()
        .map_err(|_| ApiError::lock_poisoned("database"))?;

    let mut scores = Vec::new();
    let mut errors = Vec::new();

    for a in agents.all_agents() {
        let agent_id_str = a.id.to_string();
        match cortex_storage::queries::convergence_score_queries::latest_by_agent(
            &db,
            &agent_id_str,
        ) {
            Ok(Some(row)) => {
                scores.push(ConvergenceScoreResponse {
                    agent_id: agent_id_str,
                    agent_name: a.name.clone(),
                    score: row.composite_score,
                    level: row.level,
                    profile: row.profile,
                    signal_scores: parse_signal_scores(&row.signal_scores),
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
                tracing::error!(
                    agent_id = %agent_id_str,
                    error = %e,
                    "DB error querying convergence score"
                );
                errors.push(ConvergenceErrorResponse {
                    agent_id: agent_id_str,
                    agent_name: a.name.clone(),
                    error: "database query failed".into(),
                });
            }
        }
    }

    if !errors.is_empty() && scores.is_empty() {
        return Err(ApiError::db_error(
            "convergence_scores",
            "database query failed for all agents",
        ));
    }

    // Surface partial failures so API consumers know which agents had errors (F23 fix).
    Ok(Json(ConvergenceScoresResponse {
        scores,
        errors: (!errors.is_empty()).then_some(errors),
    }))
}

/// GET /api/convergence/history/:agent_id
///
/// Returns persisted convergence history for a single agent in chronological order.
pub async fn get_history(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Query(params): Query<ConvergenceHistoryQueryParams>,
) -> ApiResult<ConvergenceHistoryResponse> {
    let limit = params.limit.unwrap_or(100).min(500) as usize;
    let db = state
        .db
        .read()
        .map_err(|_| ApiError::lock_poisoned("database"))?;

    let rows = cortex_storage::queries::convergence_score_queries::query_history(
        &db,
        &agent_id,
        params.since.as_deref(),
        Some(limit),
    )
    .map_err(|e| {
        tracing::error!(agent_id = %agent_id, error = %e, "DB error querying convergence history");
        ApiError::db_error("convergence_history", e)
    })?;

    let entries = rows
        .into_iter()
        .map(|row| ConvergenceHistoryEntryResponse {
            session_id: row.session_id,
            score: row.composite_score,
            level: row.level,
            profile: row.profile,
            signal_scores: parse_signal_scores(&row.signal_scores),
            computed_at: row.computed_at,
        })
        .collect();

    Ok(Json(ConvergenceHistoryResponse { agent_id, entries }))
}

fn parse_signal_scores(raw: &str) -> serde_json::Value {
    match serde_json::from_str(raw) {
        Ok(scores) => scores,
        Err(error) => {
            tracing::warn!(error = %error, raw, "Malformed convergence signal_scores JSON");
            serde_json::Value::Object(serde_json::Map::new())
        }
    }
}
