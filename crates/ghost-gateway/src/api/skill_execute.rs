//! Canonical skill execution endpoint.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};

use crate::api::auth::Claims;
use crate::api::error::ApiError;
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation, prepare_json_operation,
    PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, insert_mutation_audit_entry, json_response_with_idempotency,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::runtime_safety::{RuntimeSafetyBuilder, API_SYNTHETIC_AGENT_NAME};
use crate::skill_catalog::{
    ExecuteSkillRequestDto, ExecuteSkillResponseDto, SkillCatalogExecutionError,
    SkillCatalogExecutor, SkillMutationKind,
};
use crate::state::AppState;

const EXECUTE_SKILL_ROUTE_TEMPLATE: &str = "/api/skills/:name/execute";

fn map_execution_error(error: SkillCatalogExecutionError) -> ApiError {
    match error {
        SkillCatalogExecutionError::Catalog(error) => match error {
            crate::skill_catalog::SkillCatalogError::SkillNotFound(name) => {
                ApiError::not_found(format!("Skill '{name}' not found"))
            }
            crate::skill_catalog::SkillCatalogError::AmbiguousSkillIdentifier(id) => {
                ApiError::conflict(format!(
                    "Skill identifier '{id}' is ambiguous; use the catalog id"
                ))
            }
            crate::skill_catalog::SkillCatalogError::SkillDisabled(name) => {
                ApiError::conflict(format!("Skill '{name}' is disabled and cannot be executed"))
            }
            crate::skill_catalog::SkillCatalogError::ExecutionUnavailable(name) => {
                ApiError::conflict(format!(
                    "Skill '{name}' is verified but runtime execution is still gated off"
                ))
            }
            crate::skill_catalog::SkillCatalogError::VerificationFailed(name) => {
                ApiError::conflict(format!(
                    "Skill '{name}' failed verification and cannot be installed or executed"
                ))
            }
            crate::skill_catalog::SkillCatalogError::SkillQuarantined { skill_id, reason } => {
                ApiError::conflict(format!("Skill '{skill_id}' is quarantined: {reason}"))
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
            crate::skill_catalog::SkillCatalogError::NotExternalSkill(name) => {
                ApiError::conflict(format!("Skill '{name}' is not an external artifact"))
            }
            crate::skill_catalog::SkillCatalogError::NotEnabledForAgent {
                skill_name,
                agent_name,
            } => ApiError::Forbidden(format!(
                "skill '{skill_name}' is not enabled for agent '{agent_name}'"
            )),
            crate::skill_catalog::SkillCatalogError::NotQuarantined(name) => {
                ApiError::conflict(format!("Skill '{name}' is not quarantined"))
            }
            crate::skill_catalog::SkillCatalogError::StaleQuarantineRevision {
                skill_id,
                expected_revision,
                actual_revision,
            } => ApiError::with_details(
                StatusCode::CONFLICT,
                "STALE_QUARANTINE_REVISION",
                format!(
                    "Skill '{skill_id}' quarantine revision is stale; expected {expected_revision}, actual {actual_revision}"
                ),
                serde_json::json!({
                    "skill_id": skill_id,
                    "expected_revision": expected_revision,
                    "actual_revision": actual_revision,
                }),
            ),
            crate::skill_catalog::SkillCatalogError::DbPool(message)
            | crate::skill_catalog::SkillCatalogError::Storage(message)
            | crate::skill_catalog::SkillCatalogError::Ingest(message) => {
                ApiError::db_error("execute skill", message)
            }
        },
        SkillCatalogExecutionError::DbPool(message) => ApiError::db_error("execute skill", message),
        SkillCatalogExecutionError::DbLockPoisoned => ApiError::internal("db lock poisoned"),
        SkillCatalogExecutionError::PolicyDenied(message) => ApiError::Forbidden(message),
        SkillCatalogExecutionError::PolicyEscalation(message) => ApiError::Forbidden(message),
        SkillCatalogExecutionError::NativeSandbox(message) => ApiError::Forbidden(message),
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
                SkillError::ExecutionTimedOut(_) => ApiError::with_details(
                    axum::http::StatusCode::REQUEST_TIMEOUT,
                    "SKILL_TIMED_OUT",
                    error.to_string(),
                    serde_json::json!({ "code": error.code() }),
                ),
                SkillError::ResourceExhausted(_) => ApiError::with_details(
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    "SKILL_RESOURCE_EXHAUSTED",
                    error.to_string(),
                    serde_json::json!({ "code": error.code() }),
                ),
                SkillError::SandboxViolation(_) => ApiError::with_details(
                    axum::http::StatusCode::FORBIDDEN,
                    "SKILL_SANDBOX_VIOLATION",
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

fn skill_execution_actor(claims: Option<&Claims>, agent_id: uuid::Uuid) -> String {
    claims
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| format!("agent:{agent_id}"))
}

fn require_explicit_skill_idempotency(context: &OperationContext) -> Result<(), ApiError> {
    if context.client_supplied_idempotency_key {
        return Ok(());
    }
    Err(ApiError::custom(
        StatusCode::PRECONDITION_REQUIRED,
        "EXPLICIT_IDEMPOTENCY_KEY_REQUIRED",
        "Mutating skill routes require a caller-supplied Idempotency-Key",
    ))
}

fn execution_request_body(
    skill_name: &str,
    body: &ExecuteSkillRequestDto,
    mutation_kind: SkillMutationKind,
) -> serde_json::Value {
    serde_json::json!({
        "skill_name": skill_name,
        "agent_id": body.agent_id,
        "session_id": body.session_id,
        "input": body.input,
        "mutation_kind": mutation_kind,
    })
}

fn execution_audit_details(
    skill_name: &str,
    body: &ExecuteSkillRequestDto,
    mutation_kind: SkillMutationKind,
    result: &ExecuteSkillResponseDto,
) -> serde_json::Value {
    serde_json::json!({
        "skill_name": skill_name,
        "agent_id": body.agent_id,
        "session_id": body.session_id,
        "mutation_kind": mutation_kind,
        "input": body.input,
        "result": result.result,
    })
}

/// POST /api/skills/:name/execute — execute a catalog-resolved skill.
pub async fn execute_skill(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(skill_name): Path<String>,
    Json(body): Json<ExecuteSkillRequestDto>,
) -> Response {
    let agent = RuntimeSafetyBuilder::new(&state)
        .resolve_agent_by_id_or_synthetic(body.agent_id, API_SYNTHETIC_AGENT_NAME)
        .map_err(|error| ApiError::internal(error.to_string()))
        .map_err(error_response_with_idempotency);
    let agent = match agent {
        Ok(agent) => agent,
        Err(response) => return response,
    };
    let actor = skill_execution_actor(claims.as_ref().map(|Extension(claims)| claims), agent.id);
    let resolved = match state.skill_catalog.resolve_for_execute(&skill_name, &agent) {
        Ok(resolved) => resolved,
        Err(error) => return error_response_with_idempotency(map_execution_error(error.into())),
    };

    let executor = SkillCatalogExecutor::new(
        Arc::clone(&state.skill_catalog),
        Arc::clone(&state.db),
        state.convergence_profile.clone(),
    );

    match resolved.metadata.mutation_kind {
        SkillMutationKind::ReadOnly => {
            match executor.execute(&skill_name, &agent, body.session_id, &body.input) {
                Ok(result) => (StatusCode::OK, Json(result)).into_response(),
                Err(error) => error_response_with_idempotency(map_execution_error(error)),
            }
        }
        SkillMutationKind::Transactional => {
            if let Err(error) = require_explicit_skill_idempotency(&operation_context) {
                return error_response_with_idempotency(error);
            }
            let request_body =
                execution_request_body(&skill_name, &body, resolved.metadata.mutation_kind);
            let db = state.db.write().await;
            match prepare_json_operation(
                &db,
                &operation_context,
                &actor,
                "POST",
                EXECUTE_SKILL_ROUTE_TEMPLATE,
                &request_body,
            ) {
                Ok(PreparedOperation::Replay(stored)) => json_response_with_idempotency(
                    stored.status,
                    stored.body,
                    IdempotencyStatus::Replayed,
                ),
                Ok(PreparedOperation::Mismatch) => {
                    error_response_with_idempotency(ApiError::with_details(
                        StatusCode::CONFLICT,
                        "IDEMPOTENCY_KEY_REUSED",
                        "Idempotency key was reused with a different request payload",
                        serde_json::json!({
                            "route_template": EXECUTE_SKILL_ROUTE_TEMPLATE,
                            "method": "POST",
                        }),
                    ))
                }
                Ok(PreparedOperation::InProgress) => {
                    error_response_with_idempotency(ApiError::custom(
                        StatusCode::CONFLICT,
                        "IDEMPOTENCY_IN_PROGRESS",
                        "An equivalent request is already in progress",
                    ))
                }
                Ok(PreparedOperation::Acquired { lease }) => {
                    let result = match executor.execute_with_connection(
                        &db,
                        &skill_name,
                        &agent,
                        body.session_id,
                        &body.input,
                    ) {
                        Ok(result) => result,
                        Err(error) => {
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(map_execution_error(error));
                        }
                    };
                    let response_body = match serde_json::to_value(&result) {
                        Ok(body) => body,
                        Err(error) => {
                            let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                            return error_response_with_idempotency(ApiError::internal(
                                error.to_string(),
                            ));
                        }
                    };
                    if let Err(error) = insert_mutation_audit_entry(
                        &db,
                        &agent.id.to_string(),
                        "execute_skill",
                        "high",
                        &actor,
                        "executed",
                        execution_audit_details(
                            &skill_name,
                            &body,
                            resolved.metadata.mutation_kind,
                            &result,
                        ),
                        &operation_context,
                        &IdempotencyStatus::Executed,
                    ) {
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(error);
                    }
                    match commit_prepared_json_operation(
                        &db,
                        &operation_context,
                        &lease,
                        StatusCode::OK,
                        &response_body,
                    ) {
                        Ok(outcome) => json_response_with_idempotency(
                            outcome.status,
                            outcome.body,
                            outcome.idempotency_status,
                        ),
                        Err(error) => error_response_with_idempotency(error),
                    }
                }
                Err(error) => error_response_with_idempotency(error),
            }
        }
        SkillMutationKind::ExternalSideEffect => {
            error_response_with_idempotency(ApiError::custom(
                StatusCode::CONFLICT,
                "NON_IDEMPOTENT_SKILL_UNSUPPORTED",
                format!(
                    "Skill '{skill_name}' performs external side effects and is disabled on the canonical execute route until durable exactly-once execution exists"
                ),
            ))
        }
    }
}
