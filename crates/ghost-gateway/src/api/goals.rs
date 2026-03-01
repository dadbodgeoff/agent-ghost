//! Goal/proposal approval API endpoints (Req 25 AC5-6).
//!
//! Phase 2: Fixed table name (proposals → goal_proposals),
//! wired to cortex_storage::queries::goal_proposal_queries for
//! resolve_proposal (AC10 safe) and query_pending.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::api::websocket::WsEvent;
use crate::state::AppState;

/// GET /api/goals — list pending proposals.
pub async fn list_goals(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            );
        }
    };

    match cortex_storage::queries::goal_proposal_queries::query_pending(&db) {
        Ok(rows) => {
            let proposals: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "agent_id": r.agent_id,
                        "session_id": r.session_id,
                        "proposer_type": r.proposer_type,
                        "operation": r.operation,
                        "target_type": r.target_type,
                        "decision": r.decision,
                        "created_at": r.created_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({"proposals": proposals})))
        }
        Err(e) => {
            tracing::error!(error = %e, "Goal query failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        }
    }
}

/// POST /api/goals/{id}/approve
///
/// Uses cortex_storage::resolve_proposal which atomically checks
/// `resolved_at IS NULL` before updating (AC10 — no double-resolve).
pub async fn approve_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::info!(goal_id = %id, "Goal approval requested");

    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            );
        }
    };

    let resolved_at = chrono::Utc::now().to_rfc3339();
    match cortex_storage::queries::goal_proposal_queries::resolve_proposal(
        &db, &id, "approved", "human_operator", &resolved_at,
    ) {
        Ok(true) => {
            // agent_id is NOT NULL per DDL — no COALESCE needed (F9 fix).
            let agent_id = match db.query_row(
                "SELECT agent_id FROM goal_proposals WHERE id = ?1",
                [&id],
                |row| row.get::<_, String>(0),
            ) {
                Ok(aid) => aid,
                Err(e) => {
                    tracing::warn!(goal_id = %id, error = %e, "Failed to fetch agent_id for approved proposal broadcast");
                    String::new()
                }
            };

            if let Err(e) = state.event_tx.send(WsEvent::ProposalDecision {
                proposal_id: id.clone(),
                decision: "approved".into(),
                agent_id,
            }) {
                tracing::warn!(error = %e, "Failed to broadcast proposal approval event");
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({"status": "approved", "id": id})),
            )
        }
        Ok(false) => {
            // resolve_proposal returned 0 rows updated — either not found or already resolved.
            // Check which case.
            let exists = match db.query_row(
                "SELECT COUNT(*) FROM goal_proposals WHERE id = ?1",
                [&id],
                |row| row.get::<_, u32>(0),
            ) {
                Ok(count) => count,
                Err(e) => {
                    tracing::warn!(goal_id = %id, error = %e, "Failed to check proposal existence after resolve");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal error", "id": id})),
                    );
                }
            };

            if exists > 0 {
                (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"error": "proposal already resolved", "id": id})),
                )
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "proposal not found", "id": id})),
                )
            }
        }
        Err(e) => {
            tracing::error!(goal_id = %id, error = %e, "Goal approval resolve failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        }
    }
}

/// POST /api/goals/{id}/reject
pub async fn reject_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::info!(goal_id = %id, "Goal rejection requested");

    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            );
        }
    };

    let resolved_at = chrono::Utc::now().to_rfc3339();
    match cortex_storage::queries::goal_proposal_queries::resolve_proposal(
        &db, &id, "rejected", "human_operator", &resolved_at,
    ) {
        Ok(true) => {
            let agent_id = match db.query_row(
                "SELECT agent_id FROM goal_proposals WHERE id = ?1",
                [&id],
                |row| row.get::<_, String>(0),
            ) {
                Ok(aid) => aid,
                Err(e) => {
                    tracing::warn!(goal_id = %id, error = %e, "Failed to fetch agent_id for rejected proposal broadcast");
                    String::new()
                }
            };

            if let Err(e) = state.event_tx.send(WsEvent::ProposalDecision {
                proposal_id: id.clone(),
                decision: "rejected".into(),
                agent_id,
            }) {
                tracing::warn!(error = %e, "Failed to broadcast proposal rejection event");
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({"status": "rejected", "id": id})),
            )
        }
        Ok(false) => {
            let exists = match db.query_row(
                "SELECT COUNT(*) FROM goal_proposals WHERE id = ?1",
                [&id],
                |row| row.get::<_, u32>(0),
            ) {
                Ok(count) => count,
                Err(e) => {
                    tracing::warn!(goal_id = %id, error = %e, "Failed to check proposal existence after reject");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal error", "id": id})),
                    );
                }
            };

            if exists > 0 {
                (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"error": "proposal already resolved", "id": id})),
                )
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "proposal not found", "id": id})),
                )
            }
        }
        Err(e) => {
            tracing::error!(goal_id = %id, error = %e, "Goal rejection resolve failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        }
    }
}
