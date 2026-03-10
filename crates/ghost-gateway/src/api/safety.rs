//! Safety API endpoints: kill switch, pause, resume, quarantine (Req 14b).
//!
//! All endpoints now wire through to the real `KillSwitch` and broadcast
//! events to connected WebSocket clients.

use std::io::Write;
use std::path::Path as FsPath;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::Claims;
use crate::api::authz::{Action, AuthorizationContext, Principal, RouteId};
use crate::api::authz_policy::authorize_claims;
use crate::api::error::ApiError;
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::{IdempotencyStatus, OperationContext};
use crate::api::websocket::WsEvent;
use crate::runtime_status::{convergence_protection_summary_value, distributed_kill_status_value};
use crate::safety::kill_switch::{KillLevel, PLATFORM_KILLED};
use crate::state::AppState;
use cortex_core::safety::trigger::TriggerEvent;
use serde_json::Value;

const KILL_ALL_ROUTE_TEMPLATE: &str = "/api/safety/kill-all";
const PAUSE_AGENT_ROUTE_TEMPLATE: &str = "/api/safety/pause/:agent_id";
const RESUME_AGENT_ROUTE_TEMPLATE: &str = "/api/safety/resume/:agent_id";
const QUARANTINE_AGENT_ROUTE_TEMPLATE: &str = "/api/safety/quarantine/:agent_id";
const PERSISTED_SAFETY_STATE_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSafetyState {
    version: u32,
    updated_at: String,
    state: crate::safety::kill_switch::KillSwitchState,
    distributed_gate: Option<ghost_kill_gates::gate::PersistedGateState>,
}

#[derive(Debug, Deserialize)]
struct PersistedSafetyStateV2 {
    version: u32,
    updated_at: String,
    state: crate::safety::kill_switch::KillSwitchState,
}

#[derive(Debug, Clone)]
pub(crate) struct RestoredSafetyRuntimeState {
    pub state: crate::safety::kill_switch::KillSwitchState,
    pub distributed_gate: Option<ghost_kill_gates::gate::PersistedGateState>,
}

#[derive(Debug, Deserialize)]
struct LegacyPersistedKillAllState {
    active: Option<bool>,
    level: Option<String>,
}

pub(crate) fn persisted_safety_state_path() -> String {
    std::env::var("GHOST_KILL_STATE_PATH")
        .ok()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| crate::bootstrap::shellexpand_tilde("~/.ghost/data/kill_state.json"))
}

fn is_clear_safety_state(state: &crate::safety::kill_switch::KillSwitchState) -> bool {
    state.platform_level == KillLevel::Normal && state.per_agent.is_empty()
}

fn capture_kill_gate_state(state: &AppState) -> Option<ghost_kill_gates::gate::PersistedGateState> {
    state.kill_gate.as_ref().map(|gate| match gate.read() {
        Ok(bridge) => bridge.persisted_state(),
        Err(poisoned) => {
            tracing::error!("kill gate lock poisoned during persistence snapshot");
            poisoned.into_inner().persisted_state()
        }
    })
}

fn restore_kill_gate_state(
    state: &AppState,
    snapshot: Option<ghost_kill_gates::gate::PersistedGateState>,
) {
    let (Some(gate), Some(snapshot)) = (state.kill_gate.as_ref(), snapshot) else {
        return;
    };
    match gate.write() {
        Ok(mut bridge) => bridge.restore_persisted_state(snapshot),
        Err(poisoned) => {
            tracing::error!("kill gate lock poisoned during restore");
            poisoned.into_inner().restore_persisted_state(snapshot);
        }
    }
}

fn cleanup_persisted_safety_temp_path(path: &FsPath) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to clean up persisted safety temp file"
            );
        }
    }
}

fn persist_runtime_safety_state_to_path(
    path: &FsPath,
    state: &crate::safety::kill_switch::KillSwitchState,
    distributed_gate: Option<&ghost_kill_gates::gate::PersistedGateState>,
) -> Result<(), ApiError> {
    if is_clear_safety_state(state) && distributed_gate.is_none() {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(ApiError::internal(format!(
                    "remove persisted safety state: {error}"
                )));
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| ApiError::internal(format!("create persisted safety dir: {error}")))?;
    }

    let payload = PersistedSafetyState {
        version: PERSISTED_SAFETY_STATE_VERSION,
        updated_at: chrono::Utc::now().to_rfc3339(),
        state: state.clone(),
        distributed_gate: distributed_gate.cloned(),
    };
    let json_bytes = serde_json::to_vec_pretty(&payload).map_err(|error| {
        ApiError::internal(format!("serialize persisted safety state: {error}"))
    })?;

    let tmp_path = path.with_extension("json.tmp");
    let persist_result = (|| -> Result<(), ApiError> {
        let mut file = std::fs::File::create(&tmp_path).map_err(|error| {
            ApiError::internal(format!("create persisted safety temp file: {error}"))
        })?;
        file.write_all(&json_bytes).map_err(|error| {
            ApiError::internal(format!("write persisted safety temp file: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            ApiError::internal(format!("fsync persisted safety temp file: {error}"))
        })?;
        std::fs::rename(&tmp_path, path).map_err(|error| {
            ApiError::internal(format!("rename persisted safety temp file: {error}"))
        })?;
        Ok(())
    })();

    if let Err(error) = persist_result {
        cleanup_persisted_safety_temp_path(&tmp_path);
        return Err(error);
    }

    Ok(())
}

#[cfg(test)]
fn persist_safety_state_snapshot_to_path(
    path: &FsPath,
    state: &crate::safety::kill_switch::KillSwitchState,
) -> Result<(), ApiError> {
    persist_runtime_safety_state_to_path(path, state, None)
}

pub(crate) fn persist_current_safety_state(state: &AppState) -> Result<(), ApiError> {
    let snapshot = state.kill_switch.current_state();
    let path = persisted_safety_state_path();
    let gate_snapshot = capture_kill_gate_state(state);
    persist_runtime_safety_state_to_path(FsPath::new(&path), &snapshot, gate_snapshot.as_ref())
}

fn execute_persisted_safety_mutation<T>(
    state: &AppState,
    mutate: impl FnOnce() -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let previous_state = state.kill_switch.current_state();
    let previous_gate_state = capture_kill_gate_state(state);
    let result = mutate();
    match result {
        Ok(value) => {
            if let Err(error) = persist_current_safety_state(state) {
                state.kill_switch.restore_state(previous_state);
                restore_kill_gate_state(state, previous_gate_state);
                if let Err(sync_error) = state.sync_agent_access_pullbacks() {
                    tracing::error!(
                        error = %sync_error,
                        "failed to restore agent access pullbacks after persistence rollback"
                    );
                }
                return Err(error);
            }
            Ok(value)
        }
        Err(error) => {
            state.kill_switch.restore_state(previous_state);
            restore_kill_gate_state(state, previous_gate_state);
            if let Err(sync_error) = state.sync_agent_access_pullbacks() {
                tracing::error!(
                    error = %sync_error,
                    "failed to restore agent access pullbacks after safety mutation error"
                );
            }
            Err(error)
        }
    }
}

pub(crate) fn load_persisted_runtime_safety_state(
    path: &FsPath,
) -> Result<Option<RestoredSafetyRuntimeState>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("read persisted safety state: {error}"))?;
    if raw.trim().is_empty() {
        return Err("persisted safety state file is empty".into());
    }

    if let Ok(parsed) = serde_json::from_str::<PersistedSafetyState>(&raw) {
        return Ok(Some(RestoredSafetyRuntimeState {
            state: parsed.state,
            distributed_gate: parsed.distributed_gate,
        }));
    }

    if let Ok(parsed) = serde_json::from_str::<PersistedSafetyStateV2>(&raw) {
        let _ = (parsed.version, parsed.updated_at.clone());
        return Ok(Some(RestoredSafetyRuntimeState {
            state: parsed.state,
            distributed_gate: None,
        }));
    }

    if let Ok(legacy) = serde_json::from_str::<LegacyPersistedKillAllState>(&raw) {
        let is_kill_all = legacy.active == Some(true)
            || legacy
                .level
                .as_deref()
                .map(|level| level.eq_ignore_ascii_case("killall"))
                .unwrap_or(false);
        if is_kill_all {
            let restored = crate::safety::kill_switch::KillSwitchState {
                platform_level: KillLevel::KillAll,
                activated_at: Some(chrono::Utc::now()),
                trigger: Some("legacy kill_state.json found on startup".into()),
                ..Default::default()
            };
            return Ok(Some(RestoredSafetyRuntimeState {
                state: restored,
                distributed_gate: None,
            }));
        }
    }

    let fallback: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("parse persisted safety state: {error}"))?;
    if fallback["active"].as_bool() == Some(true) {
        let restored = crate::safety::kill_switch::KillSwitchState {
            platform_level: KillLevel::KillAll,
            activated_at: Some(chrono::Utc::now()),
            trigger: Some("legacy kill_state.json found on startup".into()),
            ..Default::default()
        };
        return Ok(Some(RestoredSafetyRuntimeState {
            state: restored,
            distributed_gate: None,
        }));
    }

    Err("persisted safety state did not match a supported schema".into())
}

#[cfg(test)]
pub(crate) fn load_persisted_safety_state(
    path: &FsPath,
) -> Result<Option<crate::safety::kill_switch::KillSwitchState>, String> {
    load_persisted_runtime_safety_state(path).map(|state| state.map(|state| state.state))
}

/// T-5.4.3: Look up agent ID by name, with DB fallback on registry poisoning.
///
/// When the agent registry RwLock is poisoned, falls back to querying
/// the agents table in the DB directly. Safety operations must remain
/// functional even when in-memory state is corrupted.
fn lookup_agent_id(
    state: &AppState,
    name_or_id: &str,
) -> Result<uuid::Uuid, (StatusCode, Json<serde_json::Value>)> {
    // Try UUID parse first.
    if let Ok(id) = uuid::Uuid::parse_str(name_or_id) {
        return Ok(id);
    }

    // Try in-memory registry.
    match state.agents.read() {
        Ok(guard) => {
            if let Some(a) = guard.lookup_by_name(name_or_id) {
                return Ok(a.id);
            }
        }
        Err(e) => {
            // T-5.4.3: Registry poisoned — fall back to DB lookup.
            tracing::error!(error = %e, "Agent registry RwLock poisoned — falling back to DB lookup");
            if let Ok(db) = state.db.read() {
                let result: Option<String> = db
                    .query_row(
                        "SELECT id FROM agents WHERE name = ?1 LIMIT 1",
                        rusqlite::params![name_or_id],
                        |row| row.get(0),
                    )
                    .ok();
                if let Some(id_str) = result {
                    if let Ok(id) = uuid::Uuid::parse_str(&id_str) {
                        return Ok(id);
                    }
                }
            }
        }
    }

    Err((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "agent not found", "agent_id": name_or_id})),
    ))
}

fn lookup_agent_id_api_error(state: &AppState, name_or_id: &str) -> Result<uuid::Uuid, ApiError> {
    lookup_agent_id(state, name_or_id).map_err(|(status, Json(body))| {
        if status == StatusCode::NOT_FOUND {
            ApiError::not_found(format!("agent {name_or_id} not found"))
        } else {
            let code = body["error"]["code"]
                .as_str()
                .unwrap_or("SAFETY_AGENT_LOOKUP_FAILED")
                .to_string();
            let message = body["error"]["message"]
                .as_str()
                .unwrap_or("agent lookup failed")
                .to_string();
            ApiError::with_details(status, code, message, body)
        }
    })
}

/// Write a safety action to the audit_log table.
async fn write_audit_entry(
    state: &AppState,
    event_type: &str,
    severity: &str,
    agent_id: Option<&str>,
    details: &str,
) {
    let db = state.db.write().await;
    let engine = ghost_audit::AuditQueryEngine::new(&db);
    let entry = ghost_audit::AuditEntry {
        id: uuid::Uuid::now_v7().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.unwrap_or("platform").to_string(),
        event_type: event_type.to_string(),
        severity: severity.to_string(),
        tool_name: None,
        details: details.to_string(),
        session_id: None,
        actor_id: None,
        operation_id: None,
        request_id: None,
        idempotency_key: None,
        idempotency_status: None,
    };
    if let Err(e) = engine.insert(&entry) {
        tracing::error!(error = %e, "failed to write safety audit entry");
    }
}

fn safety_actor<'a>(claims: Option<&'a crate::api::auth::Claims>, fallback: &'a str) -> &'a str {
    claims.map(|claims| claims.sub.as_str()).unwrap_or(fallback)
}

fn principal_summary_value(principal: Option<&Principal>) -> serde_json::Value {
    principal.map_or_else(
        || serde_json::json!({"role": "unknown", "capabilities": [], "authz_version": null}),
        |principal| {
            serde_json::json!({
                "role": principal.base_role.as_str(),
                "capabilities": principal.canonical_capability_names(),
                "authz_version": principal.authz_version,
            })
        },
    )
}

fn authorize_quarantine_resume_principal(claims: Option<&Claims>) -> Result<Principal, ApiError> {
    let context = AuthorizationContext::new(Action::SafetyResumeAgent, RouteId::SafetyResumeAgent);
    let (principal, _) = authorize_claims(claims, &context)?;
    Ok(principal)
}

/// T-5.11.2: Check safety cooldown for the given actor, returning 429 if in cooldown.
fn check_safety_cooldown(
    state: &AppState,
    actor: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Err(remaining_secs) = state.safety_cooldown.check_and_record(actor) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": {
                    "code": "SAFETY_COOLDOWN",
                    "message": format!(
                        "Too many safety actions. Cooldown active — retry after {remaining_secs}s."
                    ),
                    "retry_after": remaining_secs,
                }
            })),
        ));
    }
    Ok(())
}

fn check_safety_cooldown_api_error(state: &AppState, actor: &str) -> Result<(), ApiError> {
    check_safety_cooldown(state, actor).map_err(|(status, Json(body))| {
        let code = body["error"]["code"]
            .as_str()
            .unwrap_or("SAFETY_COOLDOWN")
            .to_string();
        let message = body["error"]["message"]
            .as_str()
            .unwrap_or("Too many safety actions")
            .to_string();
        let details = body["error"].clone();
        ApiError::with_details(status, code, message, details)
    })
}

/// POST /api/safety/kill-all — activate KILL_ALL (Req 14b AC5).
pub async fn kill_all(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<crate::api::auth::Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<KillAllRequest>,
) -> Response {
    let actor = safety_actor(claims.as_ref().map(|claims| &claims.0), &body.initiated_by);
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        KILL_ALL_ROUTE_TEMPLATE,
        &serde_json::to_value(&body).unwrap_or(serde_json::Value::Null),
        |_| {
            check_safety_cooldown_api_error(&state, actor)?;
            tracing::error!(
                reason = %body.reason,
                initiated_by = %body.initiated_by,
                "KILL_ALL requested via API"
            );

            let trigger = TriggerEvent::ManualKillAll {
                reason: body.reason.clone(),
                initiated_by: body.initiated_by.clone(),
            };
            let mut gate_poisoned = None::<String>;
            execute_persisted_safety_mutation(&state, || {
                state.kill_switch.activate_kill_all(&trigger);
                if let Some(ref gate) = state.kill_gate {
                    match gate.write() {
                        Ok(mut bridge) => {
                            bridge.close_and_propagate(body.reason.clone());
                            tracing::info!(
                                node_id = %bridge.node_id(),
                                "KILL_ALL propagated through distributed kill gate"
                            );
                        }
                        Err(error) => {
                            let err_msg = format!(
                                "Kill gate RwLock poisoned during KILL_ALL. Error: {error}. Falling back to HTTP fanout."
                            );
                            tracing::error!("CRITICAL: {}", err_msg);
                            gate_poisoned = Some(err_msg);
                        }
                    }
                }
                Ok(())
            })?;

            Ok((
                StatusCode::OK,
                serde_json::json!({
                    "status": "kill_all_activated",
                    "reason": body.reason,
                    "initiated_by": body.initiated_by,
                    "gate_poisoned": gate_poisoned,
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            if outcome.idempotency_status == IdempotencyStatus::Executed {
                crate::api::websocket::broadcast_event(
                    &state,
                    WsEvent::KillSwitchActivation {
                        level: "KILL_ALL".into(),
                        agent_id: None,
                        reason: body.reason.clone(),
                    },
                );

                let gate_poisoned = outcome
                    .body
                    .get("gate_poisoned")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                if let Some(ref err_msg) = gate_poisoned {
                    write_audit_entry(&state, "kill_gate_poison", "critical", None, err_msg).await;
                }

                crate::api::kill_fanout::propagate_kill(&state, "KILL_ALL", &body.reason, None);

                {
                    let db = Arc::clone(&state.db);
                    let state_ref = Arc::clone(&state);
                    let payload = serde_json::json!({
                        "event": "kill_switch",
                        "level": "KILL_ALL",
                        "reason": body.reason,
                        "initiated_by": body.initiated_by,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });
                    tokio::spawn(async move {
                        crate::api::webhooks::fire_webhooks(
                            &db,
                            &state_ref,
                            "kill_switch",
                            payload,
                        )
                        .await;
                    });
                }
            }

            let db = state.db.write().await;
            write_mutation_audit_entry(
                &db,
                "platform",
                "kill_all",
                "critical",
                actor,
                "kill_all_activated",
                serde_json::json!({
                    "reason": body.reason,
                    "initiated_by": body.initiated_by,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/safety/pause/{agent_id} — pause a specific agent (Req 14b AC3).
pub async fn pause_agent(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<crate::api::auth::Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<PauseRequest>,
) -> Response {
    let actor = safety_actor(claims.as_ref().map(|claims| &claims.0), "api");
    let request_body = serde_json::json!({
        "agent_id": agent_id_str.clone(),
        "reason": body.reason.clone(),
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        PAUSE_AGENT_ROUTE_TEMPLATE,
        &request_body,
        |_| {
            check_safety_cooldown_api_error(&state, actor)?;
            let agent_id = lookup_agent_id_api_error(&state, &agent_id_str)?;

            tracing::warn!(
                agent_id = %agent_id,
                reason = %body.reason,
                "Agent pause requested via API"
            );

            let trigger = TriggerEvent::ManualPause {
                agent_id,
                reason: body.reason.clone(),
                initiated_by: actor.to_string(),
            };
            execute_persisted_safety_mutation(&state, || {
                state
                    .kill_switch
                    .activate_agent(agent_id, KillLevel::Pause, &trigger);
                Ok(())
            })?;

            Ok((
                StatusCode::OK,
                serde_json::json!({
                    "status": "paused",
                    "agent_id": agent_id.to_string(),
                    "reason": body.reason,
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            let resolved_agent_id = outcome.body["agent_id"]
                .as_str()
                .unwrap_or(agent_id_str.as_str())
                .to_string();

            if outcome.idempotency_status == IdempotencyStatus::Executed {
                crate::api::websocket::broadcast_event(
                    &state,
                    WsEvent::KillSwitchActivation {
                        level: "PAUSE".into(),
                        agent_id: Some(resolved_agent_id.clone()),
                        reason: body.reason.clone(),
                    },
                );

                let db = Arc::clone(&state.db);
                let state_ref = Arc::clone(&state);
                let payload = serde_json::json!({
                    "event": "intervention_change",
                    "action": "pause",
                    "agent_id": resolved_agent_id,
                    "reason": body.reason,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                tokio::spawn(async move {
                    crate::api::webhooks::fire_webhooks(
                        &db,
                        &state_ref,
                        "intervention_change",
                        payload,
                    )
                    .await;
                });
            }

            write_mutation_audit_entry(
                &db,
                outcome.body["agent_id"].as_str().unwrap_or("platform"),
                "pause_agent",
                "high",
                actor,
                "paused",
                serde_json::json!({
                    "reason": body.reason,
                    "agent_id": outcome.body["agent_id"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );

            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/safety/resume/{agent_id} — resume a paused/quarantined agent (Req 14b AC3-4).
///
/// T-5.1.2: Quarantine resume requires `admin` or `operator + safety_review`.
/// Legacy `security_reviewer` claims remain accepted during compatibility mode.
/// Forensic review is persisted as an audit entry with reviewer identity
/// BEFORE the resume is allowed.
pub async fn resume_agent(
    State(state): State<Arc<AppState>>,
    claims: Option<axum::Extension<crate::api::auth::Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<ResumeRequest>,
) -> Response {
    let claims = claims.as_ref().map(|claims| &claims.0);
    let actor_id = claims
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| "unknown".into());
    let request_body = serde_json::json!({
        "agent_id": agent_id_str.clone(),
        "level": body.level,
        "forensic_reviewed": body.forensic_reviewed,
        "second_confirmation": body.second_confirmation,
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        &actor_id,
        "POST",
        RESUME_AGENT_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let agent_id = lookup_agent_id_api_error(&state, &agent_id_str)?;
            let current = state.kill_switch.current_state();
            let agent_state = current.per_agent.get(&agent_id);

            match agent_state.map(|s| s.level) {
                Some(KillLevel::Quarantine) => {
                    let quarantine_resume_principal =
                        authorize_quarantine_resume_principal(claims)?;

                    if !body.forensic_reviewed.unwrap_or(false) {
                        return Err(ApiError::bad_request(
                            "forensic review required for quarantine resume",
                        ));
                    }

                    write_mutation_audit_entry(
                        conn,
                        &agent_id.to_string(),
                        "forensic_review",
                        "critical",
                        &actor_id,
                        "reviewed",
                        serde_json::json!({
                            "principal": principal_summary_value(
                                Some(&quarantine_resume_principal)
                            ),
                            "forensic_reviewed": true,
                        }),
                        &operation_context,
                        &IdempotencyStatus::Executed,
                    );

                    if !body.second_confirmation.unwrap_or(false) {
                        return Err(ApiError::bad_request(
                            "second confirmation required for quarantine resume",
                        ));
                    }
                }
                Some(KillLevel::KillAll) => {
                    return Err(ApiError::conflict(
                        "cannot resume from KILL_ALL via agent resume — use platform resume",
                    ));
                }
                None | Some(KillLevel::Normal) => {
                    return Err(ApiError::bad_request("agent is not paused or quarantined"));
                }
                _ => {}
            }

            let expected = agent_state.map(|s| s.level);
            execute_persisted_safety_mutation(&state, || {
                state
                    .kill_switch
                    .resume_agent(agent_id, expected)
                    .map_err(ApiError::internal)?;
                state
                    .sync_agent_access_pullbacks()
                    .map_err(ApiError::internal)?;
                Ok(())
            })?;
            tracing::info!(agent_id = %agent_id, "Agent resumed via API");
            let is_quarantine = agent_state.map(|s| s.level) == Some(KillLevel::Quarantine);

            Ok((
                StatusCode::OK,
                serde_json::json!({
                    "status": "resumed",
                    "agent_id": agent_id.to_string(),
                    "heightened_monitoring": is_quarantine,
                    "monitoring_duration_hours": if is_quarantine { 24 } else { 0 },
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            if outcome.idempotency_status == IdempotencyStatus::Executed {
                crate::api::websocket::broadcast_event(
                    &state,
                    WsEvent::AgentStateChange {
                        agent_id: outcome.body["agent_id"]
                            .as_str()
                            .unwrap_or(agent_id_str.as_str())
                            .to_string(),
                        new_state: "resumed".into(),
                    },
                );
            }

            write_mutation_audit_entry(
                &db,
                outcome.body["agent_id"].as_str().unwrap_or("platform"),
                "resume_agent",
                if outcome.body["heightened_monitoring"].as_bool() == Some(true) {
                    "critical"
                } else {
                    "high"
                },
                &actor_id,
                "resumed",
                serde_json::json!({
                    "heightened_monitoring": outcome.body["heightened_monitoring"],
                    "monitoring_duration_hours": outcome.body["monitoring_duration_hours"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );

            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/safety/quarantine/{agent_id} — quarantine a specific agent.
pub async fn quarantine_agent(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<crate::api::auth::Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<PauseRequest>,
) -> Response {
    let actor = safety_actor(claims.as_ref().map(|claims| &claims.0), "api");
    let request_body = serde_json::json!({
        "agent_id": agent_id_str.clone(),
        "reason": body.reason.clone(),
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        QUARANTINE_AGENT_ROUTE_TEMPLATE,
        &request_body,
        |_| {
            check_safety_cooldown_api_error(&state, actor)?;
            let agent_id = lookup_agent_id_api_error(&state, &agent_id_str)?;

            tracing::warn!(
                agent_id = %agent_id,
                reason = %body.reason,
                "Agent quarantine requested via API"
            );

            let trigger = TriggerEvent::ManualQuarantine {
                agent_id,
                reason: body.reason.clone(),
                initiated_by: actor.to_string(),
            };
            execute_persisted_safety_mutation(&state, || {
                state
                    .kill_switch
                    .activate_agent(agent_id, KillLevel::Quarantine, &trigger);
                state
                    .sync_agent_access_pullbacks()
                    .map_err(ApiError::internal)?;
                Ok(())
            })?;

            Ok((
                StatusCode::OK,
                serde_json::json!({
                    "status": "quarantined",
                    "agent_id": agent_id.to_string(),
                    "reason": body.reason,
                    "resume_requires": "forensic_review + second_confirmation + 24h_monitoring",
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            let resolved_agent_id = outcome.body["agent_id"]
                .as_str()
                .unwrap_or(agent_id_str.as_str())
                .to_string();

            if outcome.idempotency_status == IdempotencyStatus::Executed {
                crate::api::websocket::broadcast_event(
                    &state,
                    WsEvent::KillSwitchActivation {
                        level: "QUARANTINE".into(),
                        agent_id: Some(resolved_agent_id.clone()),
                        reason: body.reason.clone(),
                    },
                );

                let db = Arc::clone(&state.db);
                let state_ref = Arc::clone(&state);
                let payload = serde_json::json!({
                    "event": "intervention_change",
                    "action": "quarantine",
                    "agent_id": resolved_agent_id,
                    "reason": body.reason,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                tokio::spawn(async move {
                    crate::api::webhooks::fire_webhooks(
                        &db,
                        &state_ref,
                        "intervention_change",
                        payload,
                    )
                    .await;
                });
            }

            write_mutation_audit_entry(
                &db,
                outcome.body["agent_id"].as_str().unwrap_or("platform"),
                "quarantine_agent",
                "critical",
                actor,
                "quarantined",
                serde_json::json!({
                    "reason": body.reason,
                    "agent_id": outcome.body["agent_id"],
                }),
                &operation_context,
                &outcome.idempotency_status,
            );

            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/safety/status — get current kill switch state.
///
/// Route middleware restricts this endpoint to `operator` and higher.
pub async fn safety_status(
    State(state): State<Arc<AppState>>,
    _claims: Option<axum::Extension<crate::api::auth::Claims>>,
) -> impl IntoResponse {
    let ks_state = state.kill_switch.current_state();
    let platform_killed = PLATFORM_KILLED.load(std::sync::atomic::Ordering::SeqCst);

    let per_agent: serde_json::Map<String, serde_json::Value> = ks_state
        .per_agent
        .iter()
        .map(|(id, agent_state)| {
            (
                id.to_string(),
                serde_json::json!({
                    "level": format!("{:?}", agent_state.level),
                    "activated_at": agent_state.activated_at.map(|t| t.to_rfc3339()),
                    "trigger": agent_state.trigger,
                }),
            )
        })
        .collect();

    let agent_ids = state
        .agents
        .read()
        .map(|agents| {
            agents
                .all_agents()
                .iter()
                .map(|agent| agent.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let convergence_protection = convergence_protection_summary_value(
        agent_ids,
        state.monitor_enabled,
        state.monitor_block_on_degraded,
        state.convergence_state_stale_after,
    );
    let distributed_kill =
        distributed_kill_status_value(state.distributed_kill_enabled, state.kill_gate.as_ref());

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "platform_level": format!("{:?}", ks_state.platform_level),
            "platform_killed": platform_killed,
            "per_agent": per_agent,
            "activated_at": ks_state.activated_at.map(|t| t.to_rfc3339()),
            "trigger": ks_state.trigger,
            "convergence_protection": convergence_protection,
            "distributed_kill": distributed_kill,
        })),
    )
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KillAllRequest {
    pub reason: String,
    pub initiated_by: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PauseRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResumeRequest {
    pub level: Option<String>,
    pub forensic_reviewed: Option<bool>,
    pub second_confirmation: Option<bool>,
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;

    use std::sync::{Mutex, OnceLock, RwLock};

    use crate::api::auth::Claims;
    use axum::body::to_bytes;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(ref value) = self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    async fn test_state_with_db_path(db_path: &FsPath) -> Arc<AppState> {
        let db = crate::db_pool::create_pool(db_path.to_path_buf()).unwrap();
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }

        let shared_state = Arc::new(crate::gateway::GatewaySharedState::new());
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        let token_store =
            ghost_oauth::TokenStore::with_default_dir(Box::new(ghost_secrets::EnvProvider));
        let oauth_broker = Arc::new(ghost_oauth::OAuthBroker::new(
            std::collections::BTreeMap::new(),
            token_store,
        ));
        let embedding_engine =
            cortex_embeddings::EmbeddingEngine::new(cortex_embeddings::EmbeddingConfig::default());

        Arc::new(AppState {
            gateway: shared_state,
            config_path: std::path::PathBuf::from("ghost.yml"),
            agents: Arc::new(RwLock::new(crate::agents::registry::AgentRegistry::new())),
            kill_switch: Arc::new(crate::safety::kill_switch::KillSwitch::new()),
            quarantine: Arc::new(RwLock::new(
                crate::safety::quarantine::QuarantineManager::new(),
            )),
            db: Arc::clone(&db),
            event_tx,
            trigger_sender:
                tokio::sync::mpsc::channel::<cortex_core::safety::trigger::TriggerEvent>(16).0,
            replay_buffer: Arc::new(crate::api::websocket::EventReplayBuffer::new(16)),
            cost_tracker: Arc::new(crate::cost::tracker::CostTracker::new()),
            kill_gate: None,
            secret_provider: Arc::new(ghost_secrets::EnvProvider),
            oauth_broker,
            mesh_signing_key: None,
            soul_drift_threshold: 0.15,
            convergence_profile: "standard".into(),
            model_providers: Vec::new(),
            default_model_provider: None,
            pc_control_circuit_breaker: ghost_pc_control::safety::PcControlConfig::default()
                .circuit_breaker(),
            websocket_auth_tickets: Arc::new(dashmap::DashMap::new()),
            ws_ticket_auth_only: false,
            tools_config: crate::config::ToolsConfig::default(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            live_execution_controls: Arc::new(dashmap::DashMap::new()),
            safety_cooldown: Arc::new(crate::api::rate_limit::SafetyCooldown::new()),
            monitor_address: "127.0.0.1:0".into(),
            monitor_enabled: false,
            monitor_block_on_degraded: false,
            convergence_state_stale_after: std::time::Duration::from_secs(300),
            monitor_healthy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            distributed_kill_enabled: false,
            embedding_engine: Arc::new(tokio::sync::Mutex::new(embedding_engine)),
            skill_catalog: Arc::new(crate::skill_catalog::SkillCatalogService::empty_for_tests(
                Arc::clone(&db),
            )),
            client_heartbeats: Arc::new(dashmap::DashMap::new()),
            session_ttl_days: 90,
            autonomy: Arc::new(crate::autonomy::AutonomyService::default()),
        })
    }

    fn operation_context(
        request_id: &str,
        operation_id: &str,
        idempotency_key: &str,
    ) -> OperationContext {
        OperationContext {
            request_id: request_id.into(),
            operation_id: Some(operation_id.into()),
            idempotency_key: Some(idempotency_key.into()),
            idempotency_status: None,
            is_mutating: true,
            client_supplied_operation_id: true,
            client_supplied_idempotency_key: true,
        }
    }

    async fn response_json(response: Response) -> Value {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn persisted_safety_state_round_trip_preserves_per_agent_levels() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let state_path = temp_dir.path().join("kill_state.json");

        let mut snapshot = crate::safety::kill_switch::KillSwitchState::default();
        let paused_agent = uuid::Uuid::now_v7();
        let quarantined_agent = uuid::Uuid::now_v7();
        snapshot.per_agent.insert(
            paused_agent,
            crate::safety::kill_switch::AgentKillState {
                agent_id: paused_agent,
                level: KillLevel::Pause,
                activated_at: Some(chrono::Utc::now()),
                trigger: Some("pause".into()),
            },
        );
        snapshot.per_agent.insert(
            quarantined_agent,
            crate::safety::kill_switch::AgentKillState {
                agent_id: quarantined_agent,
                level: KillLevel::Quarantine,
                activated_at: Some(chrono::Utc::now()),
                trigger: Some("quarantine".into()),
            },
        );

        persist_safety_state_snapshot_to_path(&state_path, &snapshot).unwrap();
        let restored = load_persisted_safety_state(&state_path).unwrap().unwrap();

        assert_eq!(restored.per_agent.len(), 2);
        assert_eq!(restored.per_agent[&paused_agent].level, KillLevel::Pause);
        assert_eq!(
            restored.per_agent[&quarantined_agent].level,
            KillLevel::Quarantine
        );
    }

    #[test]
    fn persisted_safety_state_round_trip_preserves_distributed_gate_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let state_path = temp_dir.path().join("kill_state.json");
        let kill_switch = Arc::new(crate::safety::kill_switch::KillSwitch::new());
        let node_id = uuid::Uuid::now_v7();
        let mut bridge = crate::safety::kill_gate_bridge::KillGateBridge::new(
            node_id,
            Arc::clone(&kill_switch),
            ghost_kill_gates::config::KillGateConfig::default(),
        );
        bridge.close_and_propagate("persisted".into());

        let snapshot = kill_switch.current_state();
        let gate_snapshot = bridge.persisted_state();
        persist_runtime_safety_state_to_path(&state_path, &snapshot, Some(&gate_snapshot)).unwrap();

        let restored = load_persisted_runtime_safety_state(&state_path)
            .unwrap()
            .unwrap();
        let restored_gate = restored.distributed_gate.expect("persisted gate");
        assert_eq!(restored_gate.node_id, node_id);
        assert_eq!(
            restored_gate.state,
            ghost_kill_gates::gate::GateState::Propagating
        );
        assert_eq!(restored_gate.close_reason.as_deref(), Some("persisted"));
    }

    #[tokio::test]
    async fn pause_agent_replays_after_restart_with_restored_kill_state() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let kill_state_path = temp_dir.path().join("kill_state.json");
        let _kill_state_env =
            EnvVarGuard::set("GHOST_KILL_STATE_PATH", kill_state_path.to_str().unwrap());
        let db_path = temp_dir.path().join("gateway.db");
        let agent_id = uuid::Uuid::now_v7();
        let actor = Claims::admin_fallback();

        let state = test_state_with_db_path(&db_path).await;
        let first = pause_agent(
            State(Arc::clone(&state)),
            Some(Extension(actor.clone())),
            Extension(operation_context("req-1", "op-pause", "idem-pause")),
            Path(agent_id.to_string()),
            Json(PauseRequest {
                reason: "tripwire".into(),
            }),
        )
        .await;

        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(
            first
                .headers()
                .get(crate::api::operation_context::IDEMPOTENCY_STATUS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("executed")
        );
        let body = response_json(first).await;
        assert_eq!(body["status"], "paused");
        assert_eq!(
            state.kill_switch.check(agent_id),
            crate::safety::kill_switch::KillCheckResult::AgentPaused(agent_id)
        );

        let restored_snapshot = load_persisted_safety_state(&kill_state_path)
            .unwrap()
            .unwrap();
        let restarted_state = test_state_with_db_path(&db_path).await;
        restarted_state.kill_switch.restore_state(restored_snapshot);
        assert_eq!(
            restarted_state.kill_switch.check(agent_id),
            crate::safety::kill_switch::KillCheckResult::AgentPaused(agent_id)
        );

        let replay = pause_agent(
            State(Arc::clone(&restarted_state)),
            Some(Extension(actor)),
            Extension(operation_context("req-2", "op-pause", "idem-pause")),
            Path(agent_id.to_string()),
            Json(PauseRequest {
                reason: "tripwire".into(),
            }),
        )
        .await;

        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(
            replay
                .headers()
                .get(crate::api::operation_context::IDEMPOTENCY_STATUS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("replayed")
        );
        assert_eq!(response_json(replay).await["status"], "paused");

        let db = restarted_state.db.read().unwrap();
        let audit_rows: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE operation_id = ?1 AND event_type = 'pause_agent'",
                ["op-pause"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_rows, 2);
    }

    #[tokio::test]
    async fn superadmin_can_resume_quarantined_agent() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("gateway.db");
        let agent_id = uuid::Uuid::now_v7();
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = Claims {
            sub: "root".into(),
            role: "superadmin".into(),
            capabilities: Vec::new(),
            authz_v: None,
            exp: now + 3600,
            iat: now,
            jti: "resume-superadmin".into(),
            iss: None,
        };

        let state = test_state_with_db_path(&db_path).await;
        state.kill_switch.activate_agent(
            agent_id,
            KillLevel::Quarantine,
            &TriggerEvent::ManualQuarantine {
                agent_id,
                reason: "manual review".into(),
                initiated_by: "tester".into(),
            },
        );

        let response = resume_agent(
            State(Arc::clone(&state)),
            Some(Extension(claims)),
            Extension(operation_context(
                "req-superadmin-resume",
                "op-superadmin-resume",
                "idem-superadmin-resume",
            )),
            Path(agent_id.to_string()),
            Json(ResumeRequest {
                level: Some("Quarantine".into()),
                forensic_reviewed: Some(true),
                second_confirmation: Some(true),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            state.kill_switch.check(agent_id),
            crate::safety::kill_switch::KillCheckResult::Ok
        );
    }

    #[tokio::test]
    async fn operator_with_safety_review_can_resume_quarantined_agent() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("gateway.db");
        let agent_id = uuid::Uuid::now_v7();
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = Claims {
            sub: "reviewer".into(),
            role: "operator".into(),
            capabilities: vec!["safety_review".into()],
            authz_v: Some(crate::api::authz::AUTHZ_CLAIMS_VERSION_V1),
            exp: now + 3600,
            iat: now,
            jti: "resume-reviewer".into(),
            iss: Some(crate::api::authz::INTERNAL_JWT_ISSUER.into()),
        };

        let state = test_state_with_db_path(&db_path).await;
        state.kill_switch.activate_agent(
            agent_id,
            KillLevel::Quarantine,
            &TriggerEvent::ManualQuarantine {
                agent_id,
                reason: "manual review".into(),
                initiated_by: "tester".into(),
            },
        );

        let response = resume_agent(
            State(Arc::clone(&state)),
            Some(Extension(claims)),
            Extension(operation_context(
                "req-reviewer-resume",
                "op-reviewer-resume",
                "idem-reviewer-resume",
            )),
            Path(agent_id.to_string()),
            Json(ResumeRequest {
                level: Some("Quarantine".into()),
                forensic_reviewed: Some(true),
                second_confirmation: Some(true),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            state.kill_switch.check(agent_id),
            crate::safety::kill_switch::KillCheckResult::Ok
        );
    }

    #[tokio::test]
    async fn plain_operator_cannot_resume_quarantined_agent() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("gateway.db");
        let agent_id = uuid::Uuid::now_v7();
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = Claims {
            sub: "operator".into(),
            role: "operator".into(),
            capabilities: Vec::new(),
            authz_v: Some(crate::api::authz::AUTHZ_CLAIMS_VERSION_V1),
            exp: now + 3600,
            iat: now,
            jti: "resume-plain-operator".into(),
            iss: Some(crate::api::authz::INTERNAL_JWT_ISSUER.into()),
        };

        let state = test_state_with_db_path(&db_path).await;
        state.kill_switch.activate_agent(
            agent_id,
            KillLevel::Quarantine,
            &TriggerEvent::ManualQuarantine {
                agent_id,
                reason: "manual review".into(),
                initiated_by: "tester".into(),
            },
        );

        let response = resume_agent(
            State(Arc::clone(&state)),
            Some(Extension(claims)),
            Extension(operation_context(
                "req-plain-operator-resume",
                "op-plain-operator-resume",
                "idem-plain-operator-resume",
            )),
            Path(agent_id.to_string()),
            Json(ResumeRequest {
                level: Some("Quarantine".into()),
                forensic_reviewed: Some(true),
                second_confirmation: Some(true),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            state.kill_switch.check(agent_id),
            crate::safety::kill_switch::KillCheckResult::AgentQuarantined(agent_id)
        );
    }

    #[tokio::test]
    async fn safety_status_returns_full_breakdown_for_superadmin() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("gateway.db");
        let agent_id = uuid::Uuid::now_v7();
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = Claims {
            sub: "root".into(),
            role: "superadmin".into(),
            capabilities: Vec::new(),
            authz_v: None,
            exp: now + 3600,
            iat: now,
            jti: "status-superadmin".into(),
            iss: None,
        };

        let state = test_state_with_db_path(&db_path).await;
        state.kill_switch.activate_agent(
            agent_id,
            KillLevel::Pause,
            &TriggerEvent::ManualPause {
                agent_id,
                reason: "operator intervention".into(),
                initiated_by: "tester".into(),
            },
        );

        let response = safety_status(State(Arc::clone(&state)), Some(Extension(claims)))
            .await
            .into_response();
        let body = response_json(response).await;

        assert!(
            body.get("per_agent").is_some(),
            "missing full safety breakdown"
        );
        assert!(body["per_agent"].get(agent_id.to_string()).is_some());
    }

    #[test]
    fn legacy_kill_all_state_still_restores() {
        let temp_dir = TempDir::new().unwrap();
        let state_path = temp_dir.path().join("kill_state.json");
        std::fs::write(
            &state_path,
            serde_json::json!({
                "active": true,
                "level": "KillAll",
                "reason": "legacy",
            })
            .to_string(),
        )
        .unwrap();

        let restored = load_persisted_safety_state(&state_path).unwrap().unwrap();
        assert_eq!(restored.platform_level, KillLevel::KillAll);
    }

    #[test]
    fn persist_safety_state_snapshot_cleans_up_temp_file_on_rename_failure() {
        let temp_dir = TempDir::new().unwrap();
        let state_path = temp_dir.path().join("kill_state.json");
        std::fs::create_dir_all(&state_path).unwrap();

        let state = crate::safety::kill_switch::KillSwitchState {
            platform_level: KillLevel::Pause,
            ..Default::default()
        };

        let error = persist_safety_state_snapshot_to_path(&state_path, &state).unwrap_err();
        assert!(matches!(error, ApiError::Internal(_)));
        assert!(!state_path.with_extension("json.tmp").exists());
        assert!(state_path.is_dir());
    }
}
