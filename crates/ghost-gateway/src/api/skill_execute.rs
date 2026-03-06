//! Skill execution endpoint (Phase 5).
//!
//! `POST /api/skills/:name/execute` — execute a safety skill by name.
//!
//! Safety skills are platform-managed and always available. They cannot
//! be uninstalled or disabled. This endpoint exposes them to agents and
//! internal subsystems via the REST API.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ghost_skills::skill::SkillContext;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

// ── Request / Response types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExecuteSkillRequest {
    /// Agent ID requesting execution.
    pub agent_id: Uuid,
    /// Session ID for context scoping.
    pub session_id: Uuid,
    /// Skill-specific input payload.
    #[serde(default)]
    pub input: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ExecuteSkillResponse {
    /// Name of the executed skill.
    pub skill: String,
    /// Skill execution output.
    pub result: serde_json::Value,
}

// ── Handler ──────────────────────────────────────────────────────

/// POST /api/skills/:name/execute — execute a safety skill.
///
/// The skill is looked up by name in the safety_skills registry.
/// A `SkillContext` is built from the request body (agent_id, session_id)
/// and the DB connection from AppState.
pub async fn execute_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_name): Path<String>,
    Json(body): Json<ExecuteSkillRequest>,
) -> ApiResult<ExecuteSkillResponse> {
    // Look up the skill.
    let skill = state
        .safety_skills
        .get(&skill_name)
        .ok_or_else(|| ApiError::not_found(format!("Safety skill '{skill_name}' not found")))?;

    // Acquire DB lock.
    let db = state
        .db
        .lock()
        .map_err(|_| ApiError::lock_poisoned("db"))?;

    // Build skill context.
    let ctx = SkillContext {
        db: &db,
        agent_id: body.agent_id,
        session_id: body.session_id,
        convergence_profile: &state.convergence_profile,
    };

    // Execute.
    let result = skill.execute(&ctx, &body.input).map_err(|e| {
        // Map SkillError variants to appropriate HTTP errors.
        use ghost_skills::skill::SkillError;
        match &e {
            SkillError::InvalidInput(_) => ApiError::bad_request(e.to_string()),
            SkillError::ConvergenceTooHigh { .. } => ApiError {
                status: axum::http::StatusCode::FORBIDDEN,
                body: crate::api::error::ErrorResponse::with_details(
                    "CONVERGENCE_TOO_HIGH",
                    e.to_string(),
                    serde_json::json!({ "code": e.code() }),
                ),
            },
            SkillError::BudgetExhausted { .. } => ApiError {
                status: axum::http::StatusCode::TOO_MANY_REQUESTS,
                body: crate::api::error::ErrorResponse::with_details(
                    "BUDGET_EXHAUSTED",
                    e.to_string(),
                    serde_json::json!({ "code": e.code() }),
                ),
            },
            SkillError::ReflectionConstraint(_) => ApiError {
                status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                body: crate::api::error::ErrorResponse::with_details(
                    "REFLECTION_CONSTRAINT",
                    e.to_string(),
                    serde_json::json!({ "code": e.code() }),
                ),
            },
            SkillError::PcControlBlocked(_) => ApiError {
                status: axum::http::StatusCode::FORBIDDEN,
                body: crate::api::error::ErrorResponse::with_details(
                    "PC_CONTROL_BLOCKED",
                    e.to_string(),
                    serde_json::json!({ "code": e.code() }),
                ),
            },
            SkillError::CircuitBreakerOpen(_) => ApiError {
                status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
                body: crate::api::error::ErrorResponse::with_details(
                    "CIRCUIT_BREAKER_OPEN",
                    e.to_string(),
                    serde_json::json!({ "code": e.code() }),
                ),
            },
            SkillError::DelegationFailed(_) => ApiError {
                status: axum::http::StatusCode::CONFLICT,
                body: crate::api::error::ErrorResponse::with_details(
                    "DELEGATION_FAILED",
                    e.to_string(),
                    serde_json::json!({ "code": e.code() }),
                ),
            },
            _ => ApiError::internal(e.to_string()),
        }
    })?;

    Ok(Json(ExecuteSkillResponse {
        skill: skill_name,
        result,
    }))
}
