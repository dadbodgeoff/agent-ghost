//! Goal/proposal approval API endpoints (Req 25 AC5-6).
//!
//! Phase 2: Fixed table name (proposals → goal_proposals),
//! wired to cortex_storage::queries::goal_proposal_queries for
//! resolve_proposal (AC10 safe) and query_pending.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use cortex_core::memory::types::convergence::{AgentGoalContent, GoalOrigin, GoalScope};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::websocket::WsEvent;
use crate::state::AppState;

const APPROVE_ROUTE_TEMPLATE: &str = "/api/goals/:id/approve";
const REJECT_ROUTE_TEMPLATE: &str = "/api/goals/:id/reject";

pub(crate) const PROPOSAL_STATUS_PENDING_REVIEW: &str = "pending_review";
pub(crate) const PROPOSAL_STATUS_APPROVED: &str = "approved";
pub(crate) const PROPOSAL_STATUS_REJECTED: &str = "rejected";
pub(crate) const PROPOSAL_STATUS_SUPERSEDED: &str = "superseded";
pub(crate) const PROPOSAL_STATUS_TIMED_OUT: &str = "timed_out";
pub(crate) const PROPOSAL_STATUS_AUTO_APPLIED: &str = "auto_applied";
pub(crate) const PROPOSAL_STATUS_AUTO_REJECTED: &str = "auto_rejected";

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct GoalDecisionRequestBody {
    pub expected_state: String,
    pub expected_lineage_id: String,
    pub expected_subject_key: String,
    pub expected_reviewed_revision: String,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GoalDecisionResponse {
    pub status: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GoalProposalSummary {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub proposer_type: String,
    pub operation: String,
    pub target_type: String,
    pub status: String,
    pub decision: Option<String>,
    #[schema(value_type = std::collections::BTreeMap<String, f64>)]
    pub dimension_scores: BTreeMap<String, f64>,
    pub flags: Vec<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub current_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GoalProposalTransition {
    pub from_state: Option<String>,
    pub to_state: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub reason_code: Option<String>,
    pub rationale: Option<String>,
    pub expected_state: Option<String>,
    pub expected_revision: Option<String>,
    pub operation_id: Option<String>,
    pub request_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GoalProposalDetail {
    #[serde(flatten)]
    pub proposal: GoalProposalSummary,
    pub content: serde_json::Value,
    pub cited_memory_ids: Vec<String>,
    pub resolver: Option<String>,
    pub denial_reason: Option<String>,
    pub lineage_id: Option<String>,
    pub subject_type: Option<String>,
    pub subject_key: Option<String>,
    pub reviewed_revision: Option<String>,
    pub validation_disposition: Option<String>,
    pub supersedes_proposal_id: Option<String>,
    pub transition_history: Vec<GoalProposalTransition>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GoalListResponse {
    pub proposals: Vec<GoalProposalSummary>,
    pub page: u32,
    pub page_size: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActiveGoalSummary {
    pub id: String,
    pub proposal_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub state: String,
    pub subject_type: String,
    pub subject_key: String,
    pub reviewed_revision: String,
    pub goal_text: String,
    pub scope: String,
    pub origin: String,
    pub parent_goal_id: Option<String>,
    pub content: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActiveGoalListResponse {
    pub goals: Vec<ActiveGoalSummary>,
    pub total: u32,
}

fn parse_decision_request(
    payload: Result<Json<GoalDecisionRequestBody>, JsonRejection>,
) -> Result<GoalDecisionRequestBody, ApiError> {
    payload.map(|Json(body)| body).map_err(|error| {
        ApiError::bad_request(format!("invalid goal decision request body: {error}"))
    })
}

fn stale_decision_response(
    goal_id: &str,
    code: &str,
    message: &str,
    details: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let mut merged_details = serde_json::Map::new();
    merged_details.insert(
        "proposal_id".to_string(),
        serde_json::Value::String(goal_id.to_string()),
    );
    if let Some(extra) = details.as_object() {
        for (key, value) in extra {
            merged_details.insert(key.clone(), value.clone());
        }
    }

    (
        StatusCode::CONFLICT,
        serde_json::json!({
            "error": {
                "code": code,
                "message": message,
                "details": merged_details,
            }
        }),
    )
}

fn parse_string_vec(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

fn parse_score_map(raw: Option<&str>) -> BTreeMap<String, f64> {
    raw.and_then(|value| serde_json::from_str::<BTreeMap<String, f64>>(value).ok())
        .unwrap_or_default()
}

pub(crate) fn canonical_status_from_parts(
    current_state: Option<&str>,
    decision: Option<&str>,
    _resolved_at: Option<&str>,
) -> String {
    if let Some(current_state) = current_state.filter(|value| !value.is_empty()) {
        return current_state.to_string();
    }

    match decision {
        Some("approved") => PROPOSAL_STATUS_APPROVED.to_string(),
        Some("rejected") => PROPOSAL_STATUS_REJECTED.to_string(),
        Some("Superseded") => PROPOSAL_STATUS_SUPERSEDED.to_string(),
        Some("TimedOut") => PROPOSAL_STATUS_TIMED_OUT.to_string(),
        Some("AutoApproved") | Some("ApprovedWithFlags") => {
            PROPOSAL_STATUS_AUTO_APPLIED.to_string()
        }
        Some("AutoRejected") => PROPOSAL_STATUS_AUTO_REJECTED.to_string(),
        Some("HumanReviewRequired") | None => PROPOSAL_STATUS_PENDING_REVIEW.to_string(),
        Some(_) => PROPOSAL_STATUS_PENDING_REVIEW.to_string(),
    }
}

fn canonical_status_sql_expr(table_alias: &str) -> String {
    format!(
        "COALESCE(
            (SELECT to_state
             FROM goal_proposal_transitions t
             WHERE t.proposal_id = {table_alias}.id
             ORDER BY rowid DESC
             LIMIT 1),
            CASE
                WHEN {table_alias}.decision = 'approved' THEN '{approved}'
                WHEN {table_alias}.decision = 'rejected' THEN '{rejected}'
                WHEN {table_alias}.decision = 'Superseded' THEN '{superseded}'
                WHEN {table_alias}.decision = 'TimedOut' THEN '{timed_out}'
                WHEN {table_alias}.decision IN ('AutoApproved', 'ApprovedWithFlags') THEN '{auto_applied}'
                WHEN {table_alias}.decision = 'AutoRejected' THEN '{auto_rejected}'
                ELSE '{pending_review}'
            END
        )",
        approved = PROPOSAL_STATUS_APPROVED,
        rejected = PROPOSAL_STATUS_REJECTED,
        superseded = PROPOSAL_STATUS_SUPERSEDED,
        timed_out = PROPOSAL_STATUS_TIMED_OUT,
        auto_applied = PROPOSAL_STATUS_AUTO_APPLIED,
        auto_rejected = PROPOSAL_STATUS_AUTO_REJECTED,
        pending_review = PROPOSAL_STATUS_PENDING_REVIEW,
    )
}

fn fetch_transition_history(
    conn: &rusqlite::Connection,
    goal_id: &str,
) -> Result<Vec<GoalProposalTransition>, ApiError> {
    let mut stmt = conn
        .prepare(
            "SELECT from_state, to_state, actor_type, actor_id, reason_code,
                    rationale, expected_state, expected_revision, operation_id,
                    request_id, idempotency_key, created_at
             FROM goal_proposal_transitions
             WHERE proposal_id = ?1
             ORDER BY rowid ASC",
        )
        .map_err(|e| ApiError::db_error("goal_transition_history_prepare", e))?;

    let rows = stmt
        .query_map([goal_id], |row| {
            Ok(GoalProposalTransition {
                from_state: row.get::<_, Option<String>>(0)?,
                to_state: row.get::<_, String>(1)?,
                actor_type: row.get::<_, String>(2)?,
                actor_id: row.get::<_, Option<String>>(3)?,
                reason_code: row.get::<_, Option<String>>(4)?,
                rationale: row.get::<_, Option<String>>(5)?,
                expected_state: row.get::<_, Option<String>>(6)?,
                expected_revision: row.get::<_, Option<String>>(7)?,
                operation_id: row.get::<_, Option<String>>(8)?,
                request_id: row.get::<_, Option<String>>(9)?,
                idempotency_key: row.get::<_, Option<String>>(10)?,
                created_at: row.get::<_, String>(11)?,
            })
        })
        .map_err(|e| ApiError::db_error("goal_transition_history_query", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::db_error("goal_transition_history_collect", e))?;

    Ok(rows)
}

fn actor_key(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("anonymous")
}

pub(crate) fn parse_goal_content(value: &serde_json::Value) -> Option<AgentGoalContent> {
    serde_json::from_value::<AgentGoalContent>(value.clone())
        .ok()
        .or_else(|| {
            let object = value.as_object()?;
            let goal_text = object
                .get("goal_text")
                .or_else(|| object.get("goal"))
                .or_else(|| object.get("summary"))
                .and_then(|candidate| candidate.as_str())
                .map(str::trim)
                .filter(|candidate| !candidate.is_empty())?
                .to_string();
            let scope = match object
                .get("scope")
                .and_then(|candidate| candidate.as_str())
                .unwrap_or("Session")
            {
                "ShortTerm" => GoalScope::ShortTerm,
                "LongTerm" => GoalScope::LongTerm,
                _ => GoalScope::Session,
            };
            let origin = match object
                .get("origin")
                .and_then(|candidate| candidate.as_str())
                .unwrap_or("AgentProposed")
            {
                "UserDefined" => GoalOrigin::UserDefined,
                "SystemDefault" => GoalOrigin::SystemDefault,
                _ => GoalOrigin::AgentProposed,
            };
            let parent_goal_id = object
                .get("parent_goal_id")
                .and_then(|candidate| candidate.as_str())
                .and_then(|candidate| uuid::Uuid::parse_str(candidate).ok());

            Some(AgentGoalContent {
                goal_text,
                scope,
                origin,
                parent_goal_id,
            })
        })
}

fn active_goal_from_row(
    row: cortex_storage::queries::goal_state_queries::ActiveGoalRow,
) -> Option<ActiveGoalSummary> {
    let content = serde_json::from_str::<serde_json::Value>(&row.content).ok()?;
    let goal = parse_goal_content(&content)?;

    Some(ActiveGoalSummary {
        id: row.goal_id,
        proposal_id: row.source_proposal_id,
        agent_id: row.agent_id,
        session_id: row.session_id,
        state: row.state,
        subject_type: row.subject_type,
        subject_key: row.subject_key,
        reviewed_revision: row.reviewed_revision,
        goal_text: goal.goal_text,
        scope: format!("{:?}", goal.scope),
        origin: format!("{:?}", goal.origin),
        parent_goal_id: goal.parent_goal_id.map(|value| value.to_string()),
        content,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn fetch_goal_agent_id(
    conn: &rusqlite::Connection,
    goal_id: &str,
) -> Result<Option<String>, ApiError> {
    conn.query_row(
        "SELECT agent_id FROM goal_proposals WHERE id = ?1",
        [goal_id],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(ApiError::db_error("goal_agent_lookup", other)),
    })
}

fn write_decision_audit_entry(
    conn: &rusqlite::Connection,
    agent_id: &str,
    proposal_id: &str,
    decision: &str,
    actor: &str,
    operation_context: &OperationContext,
    idempotency_status: &IdempotencyStatus,
) {
    write_mutation_audit_entry(
        conn,
        agent_id,
        "goal_proposal_decision",
        "info",
        actor,
        decision,
        serde_json::json!({
            "proposal_id": proposal_id,
            "decision": decision,
        }),
        operation_context,
        idempotency_status,
    );
}

async fn resolve_goal_decision(
    state: Arc<AppState>,
    goal_id: String,
    decision: &'static str,
    request_body: GoalDecisionRequestBody,
    route_template: &'static str,
    claims: Option<Claims>,
    operation_context: OperationContext,
) -> Response {
    tracing::info!(goal_id = %goal_id, decision = %decision, "Goal decision requested");

    let db = state.db.write().await;
    let actor = actor_key(claims.as_ref());

    let outcome = execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        route_template,
        &serde_json::to_value(&request_body).unwrap_or(serde_json::Value::Null),
        |conn| {
            let resolved_at = chrono::Utc::now().to_rfc3339();
            let preconditions =
                cortex_storage::queries::goal_proposal_queries::HumanDecisionPreconditions {
                    expected_state: &request_body.expected_state,
                    expected_lineage_id: &request_body.expected_lineage_id,
                    expected_subject_key: &request_body.expected_subject_key,
                    expected_reviewed_revision: &request_body.expected_reviewed_revision,
                    rationale: request_body.rationale.as_deref(),
                    actor_id: actor,
                    operation_id: operation_context.operation_id.as_deref(),
                    request_id: Some(&operation_context.request_id),
                    idempotency_key: operation_context.idempotency_key.as_deref(),
                };

            match cortex_storage::queries::goal_proposal_queries::resolve_human_decision_in_transaction(
                conn,
                &goal_id,
                decision,
                &preconditions,
                &resolved_at,
            ) {
                Ok(()) => {
                    if decision == "approved" {
                        cortex_storage::queries::proposal_materialization_queries::materialize_memory_write_in_transaction(
                            conn,
                            &goal_id,
                        )
                        .map_err(|error| {
                            ApiError::internal(format!(
                                "memory proposal materialization failed: {error}"
                            ))
                        })?;
                    }

                    Ok((
                        StatusCode::OK,
                        serde_json::to_value(GoalDecisionResponse {
                            status: decision.to_string(),
                            id: goal_id.clone(),
                        })
                        .unwrap_or_else(|_| serde_json::json!({"status": decision, "id": goal_id})),
                    ))
                }
                Err(cortex_storage::queries::goal_proposal_queries::HumanDecisionError::NotFound) => {
                    Ok((
                        StatusCode::NOT_FOUND,
                        serde_json::json!({
                            "error": {
                                "code": "NOT_FOUND",
                                "message": "Proposal not found",
                                "details": { "proposal_id": goal_id }
                            }
                        }),
                    ))
                }
                Err(cortex_storage::queries::goal_proposal_queries::HumanDecisionError::StaleState {
                    expected,
                    actual,
                }) => Ok(stale_decision_response(
                    &goal_id,
                    "STALE_DECISION_STATE",
                    "Proposal state no longer matches the reviewed state",
                    serde_json::json!({
                        "expected_state": expected,
                        "actual_state": actual,
                    }),
                )),
                Err(
                    cortex_storage::queries::goal_proposal_queries::HumanDecisionError::StaleLineage {
                        expected,
                        actual,
                    },
                ) => Ok(stale_decision_response(
                    &goal_id,
                    "STALE_DECISION_LINEAGE",
                    "Proposal lineage no longer matches the reviewed lineage",
                    serde_json::json!({
                        "expected_lineage_id": expected,
                        "actual_lineage_id": actual,
                    }),
                )),
                Err(
                    cortex_storage::queries::goal_proposal_queries::HumanDecisionError::StaleSubject {
                        expected,
                        actual,
                    },
                ) => Ok(stale_decision_response(
                    &goal_id,
                    "STALE_DECISION_SUBJECT",
                    "Proposal subject no longer matches the reviewed subject",
                    serde_json::json!({
                        "expected_subject_key": expected,
                        "actual_subject_key": actual,
                    }),
                )),
                Err(
                    cortex_storage::queries::goal_proposal_queries::HumanDecisionError::StaleReviewedRevision {
                        expected,
                        actual,
                    },
                ) => Ok(stale_decision_response(
                    &goal_id,
                    "STALE_DECISION_REVIEWED_REVISION",
                    "Proposal reviewed revision no longer matches the reviewed revision",
                    serde_json::json!({
                        "expected_reviewed_revision": expected,
                        "actual_reviewed_revision": actual,
                    }),
                )),
                Err(
                    cortex_storage::queries::goal_proposal_queries::HumanDecisionError::StaleHead {
                        head_proposal_id,
                        head_state,
                    },
                ) => Ok(stale_decision_response(
                    &goal_id,
                    "STALE_DECISION_SUPERSEDED",
                    "Proposal is no longer the active lineage head",
                    serde_json::json!({
                        "head_proposal_id": head_proposal_id,
                        "head_state": head_state,
                    }),
                )),
                Err(cortex_storage::queries::goal_proposal_queries::HumanDecisionError::Storage(
                    message,
                )) => Err(ApiError::db_error("goal_resolve", message)),
            }
        },
    );

    match outcome {
        Ok(outcome) => {
            let agent_id = match fetch_goal_agent_id(&db, &goal_id) {
                Ok(agent_id) => agent_id,
                Err(error) => {
                    tracing::warn!(goal_id = %goal_id, error = %error, "failed to fetch proposal agent_id after decision");
                    None
                }
            };

            if outcome.status == StatusCode::OK {
                if let Some(agent_id) = agent_id.as_deref() {
                    write_decision_audit_entry(
                        &db,
                        agent_id,
                        &goal_id,
                        decision,
                        actor,
                        &operation_context,
                        &outcome.idempotency_status,
                    );
                }

                if outcome.idempotency_status == IdempotencyStatus::Executed {
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::ProposalDecision {
                            proposal_id: goal_id.clone(),
                            decision: decision.into(),
                            agent_id: agent_id.clone().unwrap_or_default(),
                        },
                    );
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::ProposalUpdated {
                            proposal_id: goal_id.clone(),
                            agent_id: agent_id.unwrap_or_default(),
                            status: decision.into(),
                            change: "state_changed".into(),
                            supersedes_proposal_id: None,
                        },
                    );
                }
            }

            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// Query parameters for goal/proposal listing (T-2.1.5).
#[derive(Debug, Deserialize)]
pub struct GoalQueryParams {
    /// Filter by status: "pending", "approved", "rejected", "history", or omit for all.
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

    let db = match state.db.read() {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(error = %e, "Failed to acquire DB read connection");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database connection error"})),
            )
                .into_response();
        }
    };

    // Build dynamic query based on filters.
    let mut conditions = Vec::new();
    let mut bind_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;
    let status_expr = canonical_status_sql_expr("goal_proposals");

    match params.status.as_deref() {
        Some("pending") => {
            conditions.push(format!("{status_expr} = ?{idx}"));
            bind_params.push(Box::new(PROPOSAL_STATUS_PENDING_REVIEW.to_string()));
            idx += 1;
        }
        Some("approved") => {
            conditions.push(format!("{status_expr} IN (?{idx}, ?{})", idx + 1));
            bind_params.push(Box::new(PROPOSAL_STATUS_APPROVED.to_string()));
            bind_params.push(Box::new(PROPOSAL_STATUS_AUTO_APPLIED.to_string()));
            idx += 2;
        }
        Some("rejected") => {
            conditions.push(format!("{status_expr} IN (?{idx}, ?{})", idx + 1));
            bind_params.push(Box::new(PROPOSAL_STATUS_REJECTED.to_string()));
            bind_params.push(Box::new(PROPOSAL_STATUS_AUTO_REJECTED.to_string()));
            idx += 2;
        }
        Some("history") => {
            conditions.push(format!("{status_expr} != ?{idx}"));
            bind_params.push(Box::new(PROPOSAL_STATUS_PENDING_REVIEW.to_string()));
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
            )
                .into_response();
        }
    };

    // Fetch page.
    let query = format!(
        "SELECT id, agent_id, session_id, proposer_type, operation, target_type, \
                decision, dimension_scores, flags, created_at, resolved_at, \
                {status_expr} \
         FROM goal_proposals {where_clause} \
         ORDER BY created_at DESC \
         LIMIT ?{idx} OFFSET ?{}",
        idx + 1,
        status_expr = status_expr,
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
            )
                .into_response();
        }
    };

    let mut proposals = Vec::new();
    match stmt.query_map(all_refs.as_slice(), |row| {
        let dim_scores_str: Option<String> = row.get(7)?;
        let flags_str: Option<String> = row.get(8)?;
        let status: String = row.get(11)?;
        Ok(GoalProposalSummary {
            id: row.get::<_, String>(0)?,
            agent_id: row.get::<_, String>(1)?,
            session_id: row.get::<_, String>(2)?,
            proposer_type: row.get::<_, String>(3)?,
            operation: row.get::<_, String>(4)?,
            target_type: row.get::<_, String>(5)?,
            status: status.clone(),
            decision: row.get::<_, Option<String>>(6)?,
            dimension_scores: parse_score_map(dim_scores_str.as_deref()),
            flags: parse_string_vec(flags_str.as_deref()),
            created_at: row.get::<_, String>(9)?,
            resolved_at: row.get::<_, Option<String>>(10)?,
            current_state: Some(status),
        })
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
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        Json(GoalListResponse {
            proposals,
            page,
            page_size,
            total,
        }),
    )
        .into_response()
}

pub async fn list_active_goals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GoalQueryParams>,
) -> impl IntoResponse {
    let db = match state.db.read() {
        Ok(db) => db,
        Err(error) => {
            tracing::error!(error = %error, "Failed to acquire DB read connection");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database connection error"})),
            )
                .into_response();
        }
    };

    let total = match cortex_storage::queries::goal_state_queries::count_active_goals(
        &db,
        params.agent_id.as_deref(),
    ) {
        Ok(total) => total,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("active goal count failed: {error}")})),
            )
                .into_response();
        }
    };

    let limit = params.page_size.unwrap_or(200).min(500);
    let offset = (params.page.unwrap_or(1).saturating_sub(1)) * limit;
    let rows = match cortex_storage::queries::goal_state_queries::list_active_goals(
        &db,
        params.agent_id.as_deref(),
        limit,
        offset,
    ) {
        Ok(rows) => rows,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("active goal query failed: {error}")})),
            )
                .into_response();
        }
    };

    let goals = rows.into_iter().filter_map(active_goal_from_row).collect();

    (
        StatusCode::OK,
        Json(ActiveGoalListResponse { goals, total }),
    )
        .into_response()
}

/// POST /api/goals/{id}/approve
///
/// Uses the proposal v2 transition engine inside the gateway's idempotent transaction.
pub async fn approve_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(operation_context): Extension<OperationContext>,
    claims: Option<Extension<Claims>>,
    request_body: Result<Json<GoalDecisionRequestBody>, JsonRejection>,
) -> impl IntoResponse {
    let request_body = match parse_decision_request(request_body) {
        Ok(request_body) => request_body,
        Err(error) => return error_response_with_idempotency(error),
    };

    resolve_goal_decision(
        state,
        id,
        "approved",
        request_body,
        APPROVE_ROUTE_TEMPLATE,
        claims.map(|Extension(claims)| claims),
        operation_context,
    )
    .await
}

/// POST /api/goals/{id}/reject
pub async fn reject_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(operation_context): Extension<OperationContext>,
    claims: Option<Extension<Claims>>,
    request_body: Result<Json<GoalDecisionRequestBody>, JsonRejection>,
) -> impl IntoResponse {
    let request_body = match parse_decision_request(request_body) {
        Ok(request_body) => request_body,
        Err(error) => return error_response_with_idempotency(error),
    };

    resolve_goal_decision(
        state,
        id,
        "rejected",
        request_body,
        REJECT_ROUTE_TEMPLATE,
        claims.map(|Extension(claims)| claims),
        operation_context,
    )
    .await
}

/// GET /api/goals/:id — get a single proposal with full validation breakdown (T-2.1.6).
///
/// Returns the complete proposal including 7-dimension validation scores
/// from the dimension_scores column, flags, and all metadata.
pub async fn get_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<GoalProposalDetail> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("goal_get", e))?;

    let transition_history = fetch_transition_history(&db, &id)?;
    let proposal = db
        .query_row(
            "SELECT gp.id, gp.agent_id, gp.session_id, gp.proposer_type, gp.operation, gp.target_type, \
                    gp.content, gp.cited_memory_ids, gp.decision, gp.resolved_at, gp.resolver, \
                    gp.flags, gp.dimension_scores, gp.denial_reason, gp.created_at, \
                    v2.lineage_id, v2.subject_type, v2.subject_key, v2.reviewed_revision, \
                    v2.validation_disposition, v2.supersedes_proposal_id, \
                    (SELECT to_state FROM goal_proposal_transitions t WHERE t.proposal_id = gp.id ORDER BY rowid DESC LIMIT 1) \
             FROM goal_proposals gp \
             LEFT JOIN goal_proposals_v2 v2 ON v2.id = gp.id \
             WHERE gp.id = ?1",
            [&id],
            |row| {
                let content_str: String = row.get(6)?;
                let cited_str: String = row.get(7)?;
                let flags_str: Option<String> = row.get(11)?;
                let dim_str: Option<String> = row.get(12)?;
                let decision = row.get::<_, Option<String>>(8)?;
                let resolved_at = row.get::<_, Option<String>>(9)?;
                let current_state = row.get::<_, Option<String>>(21)?;
                let status = canonical_status_from_parts(
                    current_state.as_deref(),
                    decision.as_deref(),
                    resolved_at.as_deref(),
                );

                Ok(GoalProposalDetail {
                    proposal: GoalProposalSummary {
                        id: row.get::<_, String>(0)?,
                        agent_id: row.get::<_, String>(1)?,
                        session_id: row.get::<_, String>(2)?,
                        proposer_type: row.get::<_, String>(3)?,
                        operation: row.get::<_, String>(4)?,
                        target_type: row.get::<_, String>(5)?,
                        status: status.clone(),
                        decision,
                        dimension_scores: parse_score_map(dim_str.as_deref()),
                        flags: parse_string_vec(flags_str.as_deref()),
                        created_at: row.get::<_, String>(14)?,
                        resolved_at,
                        current_state: Some(status),
                    },
                    content: serde_json::from_str::<serde_json::Value>(&content_str)
                        .unwrap_or(serde_json::Value::String(content_str)),
                    cited_memory_ids: parse_string_vec(Some(&cited_str)),
                    resolver: row.get::<_, Option<String>>(10)?,
                    denial_reason: row.get::<_, Option<String>>(13)?,
                    lineage_id: row.get::<_, Option<String>>(15)?,
                    subject_type: row.get::<_, Option<String>>(16)?,
                    subject_key: row.get::<_, Option<String>>(17)?,
                    reviewed_revision: row.get::<_, Option<String>>(18)?,
                    validation_disposition: row.get::<_, Option<String>>(19)?,
                    supersedes_proposal_id: row.get::<_, Option<String>>(20)?,
                    transition_history: transition_history.clone(),
                })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_goal_content_accepts_legacy_goal_text_only_shape() {
        let parsed = parse_goal_content(&serde_json::json!({
            "goal_text": "Ship the durable goal contract"
        }))
        .expect("legacy goal content should parse");

        assert_eq!(parsed.goal_text, "Ship the durable goal contract");
        assert_eq!(parsed.scope, GoalScope::Session);
        assert_eq!(parsed.origin, GoalOrigin::AgentProposed);
        assert_eq!(parsed.parent_goal_id, None);
    }

    #[test]
    fn canonical_status_prefers_auto_states() {
        assert_eq!(
            canonical_status_from_parts(None, Some("AutoApproved"), None),
            PROPOSAL_STATUS_AUTO_APPLIED
        );
        assert_eq!(
            canonical_status_from_parts(Some("approved"), Some("HumanReviewRequired"), None),
            PROPOSAL_STATUS_APPROVED
        );
    }
}
