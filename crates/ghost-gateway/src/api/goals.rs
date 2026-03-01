//! Goal/proposal approval API endpoints (Req 25 AC5-6).
//!
//! Phase 2: Fixed table name (proposals → goal_proposals),
//! wired to cortex_storage::queries::goal_proposal_queries for
//! resolve_proposal (AC10 safe) and query_pending.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::api::error::{ApiError, ApiResult};
use crate::api::websocket::WsEvent;
use crate::state::AppState;

/// Query parameters for goal/proposal listing (T-2.1.5).
#[derive(Debug, Deserialize)]
pub struct GoalQueryParams {
    /// Filter by status: "pending", "approved", "rejected", or omit for all.
    pub status: Option<String>,
    /// Filter by agent_id.
    pub agent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// GET /api/goals — list proposals with optional status/agent filters (T-2.1.5).
pub async fn list_goals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GoalQueryParams>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * page_size;

    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            );
        }
    };

    // Build dynamic query based on filters.
    let mut conditions = Vec::new();
    let mut bind_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;

    match params.status.as_deref() {
        Some("pending") => {
            conditions.push("resolved_at IS NULL".to_string());
        }
        Some("approved") => {
            conditions.push(format!("decision = ?{idx}"));
            bind_params.push(Box::new("approved".to_string()));
            idx += 1;
        }
        Some("rejected") => {
            conditions.push(format!("decision = ?{idx}"));
            bind_params.push(Box::new("rejected".to_string()));
            idx += 1;
        }
        _ => {} // No filter — return all.
    }

    if let Some(ref agent_id) = params.agent_id {
        conditions.push(format!("agent_id = ?{idx}"));
        bind_params.push(Box::new(agent_id.clone()));
        idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count total matching.
    let count_query = format!("SELECT COUNT(*) FROM goal_proposals {where_clause}");
    let count_refs: Vec<&dyn rusqlite::types::ToSql> =
        bind_params.iter().map(|p| p.as_ref()).collect();
    let total: u32 = match db.query_row(&count_query, count_refs.as_slice(), |row| row.get(0)) {
        Ok(count) => count,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("count query failed: {e}")})),
            );
        }
    };

    // Fetch page.
    let query = format!(
        "SELECT id, agent_id, session_id, proposer_type, operation, target_type, \
                decision, dimension_scores, flags, created_at, resolved_at \
         FROM goal_proposals {where_clause} \
         ORDER BY created_at DESC \
         LIMIT ?{idx} OFFSET ?{}",
        idx + 1
    );
    bind_params.push(Box::new(page_size));
    bind_params.push(Box::new(offset));

    let all_refs: Vec<&dyn rusqlite::types::ToSql> =
        bind_params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = match db.prepare(&query) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
            );
        }
    };

    let mut proposals = Vec::new();
    match stmt.query_map(all_refs.as_slice(), |row| {
        let dim_scores_str: Option<String> = row.get(7)?;
        let flags_str: Option<String> = row.get(8)?;
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "agent_id": row.get::<_, String>(1)?,
            "session_id": row.get::<_, String>(2)?,
            "proposer_type": row.get::<_, String>(3)?,
            "operation": row.get::<_, String>(4)?,
            "target_type": row.get::<_, String>(5)?,
            "decision": row.get::<_, Option<String>>(6)?,
            "dimension_scores": dim_scores_str.as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            "flags": flags_str.as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .unwrap_or(serde_json::Value::Array(vec![])),
            "created_at": row.get::<_, String>(9)?,
            "resolved_at": row.get::<_, Option<String>>(10)?,
        }))
    }) {
        Ok(rows) => {
            for row in rows {
                match row {
                    Ok(r) => proposals.push(r),
                    Err(e) => tracing::warn!(error = %e, "skipping malformed proposal row"),
                }
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query failed: {e}")})),
            );
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "proposals": proposals,
            "page": page,
            "page_size": page_size,
            "total": total,
        })),
    )
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

/// GET /api/goals/:id — get a single proposal with full validation breakdown (T-2.1.6).
///
/// Returns the complete proposal including 7-dimension validation scores
/// from the dimension_scores column, flags, and all metadata.
pub async fn get_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    let proposal = db
        .query_row(
            "SELECT id, agent_id, session_id, proposer_type, operation, target_type, \
                    content, cited_memory_ids, decision, resolved_at, resolver, \
                    flags, dimension_scores, denial_reason, created_at \
             FROM goal_proposals WHERE id = ?1",
            [&id],
            |row| {
                let content_str: String = row.get(6)?;
                let cited_str: String = row.get(7)?;
                let flags_str: Option<String> = row.get(11)?;
                let dim_str: Option<String> = row.get(12)?;

                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "agent_id": row.get::<_, String>(1)?,
                    "session_id": row.get::<_, String>(2)?,
                    "proposer_type": row.get::<_, String>(3)?,
                    "operation": row.get::<_, String>(4)?,
                    "target_type": row.get::<_, String>(5)?,
                    "content": serde_json::from_str::<serde_json::Value>(&content_str)
                        .unwrap_or(serde_json::Value::String(content_str)),
                    "cited_memory_ids": serde_json::from_str::<serde_json::Value>(&cited_str)
                        .unwrap_or(serde_json::Value::Array(vec![])),
                    "decision": row.get::<_, Option<String>>(8)?,
                    "resolved_at": row.get::<_, Option<String>>(9)?,
                    "resolver": row.get::<_, Option<String>>(10)?,
                    "flags": flags_str.as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .unwrap_or(serde_json::Value::Array(vec![])),
                    "dimension_scores": dim_str.as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                    "denial_reason": row.get::<_, Option<String>>(13)?,
                    "created_at": row.get::<_, String>(14)?,
                }))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::not_found(format!("Proposal {id} not found"))
            }
            _ => ApiError::db_error("goal_get", e),
        })?;

    Ok(Json(proposal))
}
