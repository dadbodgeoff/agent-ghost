//! Canonical skill execution endpoint.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::{Extension, Json};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::runtime_safety::{RuntimeSafetyBuilder, API_SYNTHETIC_AGENT_NAME};
use crate::skill_catalog::{
    ExecuteSkillRequestDto, ExecuteSkillResponseDto, SkillCatalogExecutionError,
    SkillCatalogExecutor,
};
use crate::state::AppState;

fn map_execution_error(error: SkillCatalogExecutionError) -> ApiError {
    match error {
        SkillCatalogExecutionError::Catalog(error) => match error {
            crate::skill_catalog::SkillCatalogError::SkillNotFound(name) => {
                ApiError::not_found(format!("Skill '{name}' not found"))
            }
            crate::skill_catalog::SkillCatalogError::SkillDisabled(name) => {
                ApiError::conflict(format!("Skill '{name}' is disabled and cannot be executed"))
            }
            crate::skill_catalog::SkillCatalogError::NotInstallable(name) => {
                ApiError::conflict(format!("Skill '{name}' cannot be installed"))
            }
            crate::skill_catalog::SkillCatalogError::AlreadyInstalled(name) => {
                ApiError::conflict(format!("Skill '{name}' is already installed"))
            }
            crate::skill_catalog::SkillCatalogError::NotInstalled(name) => {
                ApiError::conflict(format!("Skill '{name}' is not installed"))
            }
            crate::skill_catalog::SkillCatalogError::NotRemovable(name) => {
                ApiError::conflict(format!("Skill '{name}' cannot be uninstalled"))
            }
            crate::skill_catalog::SkillCatalogError::NotEnabledForAgent {
                skill_name,
                agent_name,
            } => ApiError::Forbidden(format!(
                "skill '{skill_name}' is not enabled for agent '{agent_name}'"
            )),
            crate::skill_catalog::SkillCatalogError::DbPool(message)
            | crate::skill_catalog::SkillCatalogError::Storage(message) => {
                ApiError::db_error("execute skill", message)
            }
        },
        SkillCatalogExecutionError::DbPool(message) => ApiError::db_error("execute skill", message),
        SkillCatalogExecutionError::DbLockPoisoned => ApiError::internal("db lock poisoned"),
        SkillCatalogExecutionError::PolicyDenied(message) => ApiError::Forbidden(message),
        SkillCatalogExecutionError::PolicyEscalation(message) => ApiError::Forbidden(message),
        SkillCatalogExecutionError::Skill(error) => {
            use ghost_skills::skill::SkillError;
            match error {
                SkillError::InvalidInput(message) => ApiError::bad_request(message),
                SkillError::ConvergenceTooHigh { .. } => ApiError::with_details(
                    axum::http::StatusCode::FORBIDDEN,
                    "CONVERGENCE_TOO_HIGH",
                    error.to_string(),
                    serde_json::json!({ "code": error.code() }),
                ),
                SkillError::BudgetExhausted { .. } => ApiError::with_details(
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    "BUDGET_EXHAUSTED",
                    error.to_string(),
                    serde_json::json!({ "code": error.code() }),
                ),
                SkillError::ReflectionConstraint(_) => ApiError::with_details(
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    "REFLECTION_CONSTRAINT",
                    error.to_string(),
                    serde_json::json!({ "code": error.code() }),
                ),
                SkillError::PcControlBlocked(_) => ApiError::with_details(
                    axum::http::StatusCode::FORBIDDEN,
                    "PC_CONTROL_BLOCKED",
                    error.to_string(),
                    serde_json::json!({ "code": error.code() }),
                ),
                SkillError::CircuitBreakerOpen(_) => ApiError::with_details(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "CIRCUIT_BREAKER_OPEN",
                    error.to_string(),
                    serde_json::json!({ "code": error.code() }),
                ),
                SkillError::DelegationFailed(_) | SkillError::AuthorizationDenied(_) => {
                    ApiError::Forbidden(error.to_string())
                }
                _ => ApiError::internal(error.to_string()),
            }
        }
    }
}

/// POST /api/skills/:name/execute — execute a catalog-resolved skill.
pub async fn execute_skill(
    State(state): State<Arc<AppState>>,
    _claims: Option<Extension<Claims>>,
    Path(skill_name): Path<String>,
    Json(body): Json<ExecuteSkillRequestDto>,
) -> ApiResult<ExecuteSkillResponseDto> {
    let agent = RuntimeSafetyBuilder::new(&state)
        .resolve_agent_by_id_or_synthetic(body.agent_id, API_SYNTHETIC_AGENT_NAME)
        .map_err(|error| ApiError::internal(error.to_string()))?;

    let executor = SkillCatalogExecutor::new(
        Arc::clone(&state.skill_catalog),
        Arc::clone(&state.db),
        state.convergence_profile.clone(),
    );

    executor
        .execute(&skill_name, &agent, body.session_id, &body.input)
        .map(Json)
        .map_err(map_execution_error)
}
