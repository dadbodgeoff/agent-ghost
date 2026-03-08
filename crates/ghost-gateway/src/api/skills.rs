//! Skill management endpoints backed by the canonical catalog service.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::{Extension, Json};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::{
    abort_prepared_json_operation, commit_prepared_json_operation, prepare_json_operation,
    PreparedOperation,
};
use crate::api::mutation::{
    error_response_with_idempotency, insert_mutation_audit_entry, json_response_with_idempotency,
    write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::websocket::WsEvent;
use crate::skill_catalog::{
    SkillCatalogError, SkillListResponseDto, SkillQuarantineRequestDto,
    SkillQuarantineResolutionRequestDto, SkillSummaryDto,
};
use crate::state::AppState;

fn skill_actor(claims: Option<&Claims>) -> &str {
    claims.map(|claims| claims.sub.as_str()).unwrap_or("system")
}

const INSTALL_SKILL_ROUTE_TEMPLATE: &str = "/api/skills/:id/install";
const UNINSTALL_SKILL_ROUTE_TEMPLATE: &str = "/api/skills/:id/uninstall";
const QUARANTINE_SKILL_ROUTE_TEMPLATE: &str = "/api/skills/:id/quarantine";
const RESOLVE_QUARANTINE_SKILL_ROUTE_TEMPLATE: &str = "/api/skills/:id/quarantine/resolve";
const REVERIFY_SKILL_ROUTE_TEMPLATE: &str = "/api/skills/:id/reverify";

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

fn skill_change_audit_details(skill_name: &str, summary: &SkillSummaryDto) -> serde_json::Value {
    serde_json::json!({
        "skill_name": skill_name,
        "skill_id": summary.id,
        "state": summary.state,
        "install_state": summary.install_state,
        "verification_status": summary.verification_status,
        "quarantine_state": summary.quarantine_state,
        "runtime_visible": summary.runtime_visible,
        "mutation_kind": summary.mutation_kind,
        "policy_capability": summary.policy_capability,
    })
}

fn map_catalog_error(error: SkillCatalogError) -> ApiError {
    match error {
        SkillCatalogError::SkillNotFound(name) => {
            ApiError::not_found(format!("Skill '{name}' not found"))
        }
        SkillCatalogError::AmbiguousSkillIdentifier(id) => ApiError::conflict(format!(
            "Skill identifier '{id}' is ambiguous; use the catalog id"
        )),
        SkillCatalogError::AlreadyInstalled(name) => {
            ApiError::conflict(format!("Skill '{name}' is already installed"))
        }
        SkillCatalogError::NotInstallable(name) => {
            ApiError::conflict(format!("Skill '{name}' cannot be installed"))
        }
        SkillCatalogError::NotRemovable(name) => {
            ApiError::conflict(format!("Skill '{name}' cannot be uninstalled"))
        }
        SkillCatalogError::NotExternalSkill(name) => {
            ApiError::conflict(format!("Skill '{name}' is not an external artifact"))
        }
        SkillCatalogError::SkillDisabled(name) => {
            ApiError::conflict(format!("Skill '{name}' is disabled"))
        }
        SkillCatalogError::ExecutionUnavailable(name) => ApiError::conflict(format!(
            "Skill '{name}' is verified but runtime execution is still gated off"
        )),
        SkillCatalogError::VerificationFailed(name) => ApiError::conflict(format!(
            "Skill '{name}' failed verification and cannot be installed or executed"
        )),
        SkillCatalogError::NotQuarantined(name) => {
            ApiError::conflict(format!("Skill '{name}' is not quarantined"))
        }
        SkillCatalogError::StaleQuarantineRevision {
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
        SkillCatalogError::SkillQuarantined { skill_id, reason } => {
            ApiError::conflict(format!("Skill '{skill_id}' is quarantined: {reason}"))
        }
        SkillCatalogError::NotInstalled(name) => {
            ApiError::not_found(format!("Skill '{name}' is not installed"))
        }
        SkillCatalogError::NotEnabledForAgent {
            skill_name,
            agent_name,
        } => ApiError::Forbidden(format!(
            "skill '{skill_name}' is not enabled for agent '{agent_name}'"
        )),
        SkillCatalogError::DbPool(message)
        | SkillCatalogError::Storage(message)
        | SkillCatalogError::Ingest(message) => ApiError::db_error("skill catalog", message),
    }
}

/// GET /api/skills — list installed and available skills from the mixed-source catalog.
pub async fn list_skills(State(state): State<Arc<AppState>>) -> ApiResult<SkillListResponseDto> {
    state
        .skill_catalog
        .list_skills()
        .map(Json)
        .map_err(map_catalog_error)
}

/// POST /api/skills/:id/install — install a catalog skill by id or compiled name.
pub async fn install_skill(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(skill_name): Path<String>,
) -> Response {
    if let Err(error) = require_explicit_skill_idempotency(&operation_context) {
        return error_response_with_idempotency(error);
    }
    let actor = skill_actor(claims.as_ref().map(|Extension(claims)| claims));
    let request_body = serde_json::json!({ "skill_name": skill_name });
    let db = state.db.write().await;
    match prepare_json_operation(
        &db,
        &operation_context,
        actor,
        "POST",
        INSTALL_SKILL_ROUTE_TEMPLATE,
        &request_body,
    ) {
        Ok(PreparedOperation::Replay(stored)) => {
            write_mutation_audit_entry(
                &db,
                &skill_name,
                "install_skill",
                "high",
                actor,
                "replayed",
                serde_json::json!({ "skill_name": skill_name }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": INSTALL_SKILL_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            let summary = match state
                .skill_catalog
                .install_with_conn(&db, &skill_name, Some(actor))
            {
                Ok(summary) => summary,
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(map_catalog_error(error));
                }
            };
            let body = match serde_json::to_value(&summary) {
                Ok(body) => body,
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(ApiError::internal(error.to_string()));
                }
            };
            if let Err(error) = insert_mutation_audit_entry(
                &db,
                &skill_name,
                "install_skill",
                "high",
                actor,
                "installed",
                skill_change_audit_details(&skill_name, &summary),
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
                &body,
            ) {
                Ok(outcome) => {
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::SkillChange {
                            skill_name: skill_name.clone(),
                            action: "installed".into(),
                        },
                    );
                    json_response_with_idempotency(
                        outcome.status,
                        outcome.body,
                        outcome.idempotency_status,
                    )
                }
                Err(error) => error_response_with_idempotency(error),
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/skills/:id/uninstall — disable an installed catalog skill by id or compiled name.
pub async fn uninstall_skill(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(skill_name): Path<String>,
) -> Response {
    if let Err(error) = require_explicit_skill_idempotency(&operation_context) {
        return error_response_with_idempotency(error);
    }
    let actor = skill_actor(claims.as_ref().map(|Extension(claims)| claims));
    let request_body = serde_json::json!({ "skill_name": skill_name });
    let db = state.db.write().await;
    match prepare_json_operation(
        &db,
        &operation_context,
        actor,
        "POST",
        UNINSTALL_SKILL_ROUTE_TEMPLATE,
        &request_body,
    ) {
        Ok(PreparedOperation::Replay(stored)) => {
            write_mutation_audit_entry(
                &db,
                &skill_name,
                "uninstall_skill",
                "high",
                actor,
                "replayed",
                serde_json::json!({ "skill_name": skill_name }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": UNINSTALL_SKILL_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            let summary =
                match state
                    .skill_catalog
                    .uninstall_with_conn(&db, &skill_name, Some(actor))
                {
                    Ok(summary) => summary,
                    Err(error) => {
                        let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                        return error_response_with_idempotency(map_catalog_error(error));
                    }
                };
            let body = match serde_json::to_value(&summary) {
                Ok(body) => body,
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(ApiError::internal(error.to_string()));
                }
            };
            if let Err(error) = insert_mutation_audit_entry(
                &db,
                &skill_name,
                "uninstall_skill",
                "high",
                actor,
                "uninstalled",
                skill_change_audit_details(&skill_name, &summary),
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
                &body,
            ) {
                Ok(outcome) => {
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::SkillChange {
                            skill_name: skill_name.clone(),
                            action: "uninstalled".into(),
                        },
                    );
                    json_response_with_idempotency(
                        outcome.status,
                        outcome.body,
                        outcome.idempotency_status,
                    )
                }
                Err(error) => error_response_with_idempotency(error),
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/skills/:id/quarantine — manually quarantine an external skill artifact.
pub async fn quarantine_skill(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(skill_name): Path<String>,
    Json(body): Json<SkillQuarantineRequestDto>,
) -> Response {
    if let Err(error) = require_explicit_skill_idempotency(&operation_context) {
        return error_response_with_idempotency(error);
    }
    if body.reason.trim().is_empty() {
        return error_response_with_idempotency(ApiError::bad_request(
            "Quarantine reason is required",
        ));
    }

    let actor = skill_actor(claims.as_ref().map(|Extension(claims)| claims));
    let request_body = serde_json::json!({
        "skill_name": skill_name,
        "reason": body.reason,
    });
    let db = state.db.write().await;
    match prepare_json_operation(
        &db,
        &operation_context,
        actor,
        "POST",
        QUARANTINE_SKILL_ROUTE_TEMPLATE,
        &request_body,
    ) {
        Ok(PreparedOperation::Replay(stored)) => {
            write_mutation_audit_entry(
                &db,
                &skill_name,
                "quarantine_skill",
                "high",
                actor,
                "replayed",
                serde_json::json!({ "skill_name": skill_name, "reason": body.reason }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": QUARANTINE_SKILL_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            let summary = match state.skill_catalog.quarantine_with_conn(
                &db,
                &skill_name,
                &body.reason,
                Some(actor),
            ) {
                Ok(summary) => summary,
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(map_catalog_error(error));
                }
            };
            let response_body = match serde_json::to_value(&summary) {
                Ok(body) => body,
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(ApiError::internal(error.to_string()));
                }
            };
            if let Err(error) = insert_mutation_audit_entry(
                &db,
                &skill_name,
                "quarantine_skill",
                "high",
                actor,
                "quarantined",
                serde_json::json!({
                    "reason": body.reason,
                    "summary": skill_change_audit_details(&skill_name, &summary),
                }),
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
                Ok(outcome) => {
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::SkillChange {
                            skill_name: skill_name.clone(),
                            action: "quarantined".into(),
                        },
                    );
                    json_response_with_idempotency(
                        outcome.status,
                        outcome.body,
                        outcome.idempotency_status,
                    )
                }
                Err(error) => error_response_with_idempotency(error),
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/skills/:id/quarantine/resolve — clear a quarantined external skill after explicit review.
pub async fn resolve_skill_quarantine(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(skill_name): Path<String>,
    Json(body): Json<SkillQuarantineResolutionRequestDto>,
) -> Response {
    if let Err(error) = require_explicit_skill_idempotency(&operation_context) {
        return error_response_with_idempotency(error);
    }
    if body.expected_quarantine_revision < 0 {
        return error_response_with_idempotency(ApiError::bad_request(
            "expected_quarantine_revision must be non-negative",
        ));
    }

    let actor = skill_actor(claims.as_ref().map(|Extension(claims)| claims));
    let request_body = serde_json::json!({
        "skill_name": skill_name,
        "expected_quarantine_revision": body.expected_quarantine_revision,
    });
    let db = state.db.write().await;
    match prepare_json_operation(
        &db,
        &operation_context,
        actor,
        "POST",
        RESOLVE_QUARANTINE_SKILL_ROUTE_TEMPLATE,
        &request_body,
    ) {
        Ok(PreparedOperation::Replay(stored)) => {
            write_mutation_audit_entry(
                &db,
                &skill_name,
                "resolve_skill_quarantine",
                "high",
                actor,
                "replayed",
                request_body,
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": RESOLVE_QUARANTINE_SKILL_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            let summary = match state.skill_catalog.resolve_quarantine_with_conn(
                &db,
                &skill_name,
                body.expected_quarantine_revision,
                Some(actor),
            ) {
                Ok(summary) => summary,
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(map_catalog_error(error));
                }
            };
            let response_body = match serde_json::to_value(&summary) {
                Ok(body) => body,
                Err(error) => {
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(ApiError::internal(error.to_string()));
                }
            };
            if let Err(error) = insert_mutation_audit_entry(
                &db,
                &skill_name,
                "resolve_skill_quarantine",
                "high",
                actor,
                "quarantine_resolved",
                skill_change_audit_details(&skill_name, &summary),
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
                Ok(outcome) => {
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::SkillChange {
                            skill_name: skill_name.clone(),
                            action: "quarantine_resolved".into(),
                        },
                    );
                    json_response_with_idempotency(
                        outcome.status,
                        outcome.body,
                        outcome.idempotency_status,
                    )
                }
                Err(error) => error_response_with_idempotency(error),
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/skills/:id/reverify — rerun verification against the gateway-managed artifact.
pub async fn reverify_skill(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(skill_name): Path<String>,
) -> Response {
    if let Err(error) = require_explicit_skill_idempotency(&operation_context) {
        return error_response_with_idempotency(error);
    }

    let actor = skill_actor(claims.as_ref().map(|Extension(claims)| claims));
    let request_body = serde_json::json!({ "skill_name": skill_name });
    let db = state.db.write().await;
    match prepare_json_operation(
        &db,
        &operation_context,
        actor,
        "POST",
        REVERIFY_SKILL_ROUTE_TEMPLATE,
        &request_body,
    ) {
        Ok(PreparedOperation::Replay(stored)) => {
            write_mutation_audit_entry(
                &db,
                &skill_name,
                "reverify_skill",
                "high",
                actor,
                "replayed",
                serde_json::json!({ "skill_name": skill_name }),
                &operation_context,
                &IdempotencyStatus::Replayed,
            );
            json_response_with_idempotency(stored.status, stored.body, IdempotencyStatus::Replayed)
        }
        Ok(PreparedOperation::Mismatch) => error_response_with_idempotency(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was reused with a different request payload",
            serde_json::json!({
                "route_template": REVERIFY_SKILL_ROUTE_TEMPLATE,
                "method": "POST",
            }),
        )),
        Ok(PreparedOperation::InProgress) => error_response_with_idempotency(ApiError::custom(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_IN_PROGRESS",
            "An equivalent request is already in progress",
        )),
        Ok(PreparedOperation::Acquired { lease }) => {
            drop(db);
            let summary = match state
                .skill_catalog
                .reverify_external_skill(&skill_name, actor)
                .await
            {
                Ok(summary) => summary,
                Err(error) => {
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(map_catalog_error(error));
                }
            };
            let response_body = match serde_json::to_value(&summary) {
                Ok(body) => body,
                Err(error) => {
                    let db = state.db.write().await;
                    let _ = abort_prepared_json_operation(&db, &operation_context, &lease);
                    return error_response_with_idempotency(ApiError::internal(error.to_string()));
                }
            };
            let db = state.db.write().await;
            if let Err(error) = insert_mutation_audit_entry(
                &db,
                &skill_name,
                "reverify_skill",
                "high",
                actor,
                "reverified",
                skill_change_audit_details(&skill_name, &summary),
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
                Ok(outcome) => {
                    crate::api::websocket::broadcast_event(
                        &state,
                        WsEvent::SkillChange {
                            skill_name: skill_name.clone(),
                            action: "reverified".into(),
                        },
                    );
                    json_response_with_idempotency(
                        outcome.status,
                        outcome.body,
                        outcome.idempotency_status,
                    )
                }
                Err(error) => error_response_with_idempotency(error),
            }
        }
        Err(error) => error_response_with_idempotency(error),
    }
}
