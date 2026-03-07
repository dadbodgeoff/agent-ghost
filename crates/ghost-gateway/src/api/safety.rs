//! Safety API endpoints: kill switch, pause, resume, quarantine (Req 14b).
//!
//! All endpoints now wire through to the real `KillSwitch` and broadcast
//! events to connected WebSocket clients.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::api::websocket::WsEvent;
use crate::safety::kill_switch::{KillLevel, PLATFORM_KILLED};
use crate::state::AppState;
use cortex_core::safety::trigger::TriggerEvent;

/// T-5.4.3: Look up agent ID by name, with DB fallback on registry poisoning.
///
/// When the agent registry RwLock is poisoned, falls back to querying
/// the agents table in the DB directly. Safety operations must remain
/// functional even when in-memory state is corrupted.
fn lookup_agent_id(state: &AppState, name_or_id: &str) -> Result<uuid::Uuid, (StatusCode, Json<serde_json::Value>)> {
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

/// Write a safety action to the audit_log table.
async fn write_audit_entry(state: &AppState, event_type: &str, severity: &str, agent_id: Option<&str>, details: &str) {
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
    };
    if let Err(e) = engine.insert(&entry) {
        tracing::error!(error = %e, "failed to write safety audit entry");
    }
}

/// T-5.11.2: Check safety cooldown for the given actor, returning 429 if in cooldown.
fn check_safety_cooldown(state: &AppState, actor: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
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

/// POST /api/safety/kill-all — activate KILL_ALL (Req 14b AC5).
pub async fn kill_all(
    State(state): State<Arc<AppState>>,
    Json(body): Json<KillAllRequest>,
) -> impl IntoResponse {
    // T-5.11.2: Safety action cooldown.
    if let Err(resp) = check_safety_cooldown(&state, &body.initiated_by) {
        return resp;
    }

    tracing::error!(
        reason = %body.reason,
        initiated_by = %body.initiated_by,
        "KILL_ALL requested via API"
    );

    // INVARIANT: File write (with fsync) MUST complete before atomic bool is set.
    // This ensures crash recovery always finds the kill state on disk.
    let kill_state = serde_json::json!({
        "active": true,
        "level": "KillAll",
        "reason": body.reason,
        "initiated_by": body.initiated_by,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let kill_path = crate::bootstrap::shellexpand_tilde("~/.ghost/data/kill_state.json");
    if let Some(parent) = std::path::Path::new(&kill_path).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::error!(error = %e, "Failed to create kill_state directory");
        }
    }
    // Atomic write: temp file → fsync → rename (prevents partial writes on crash).
    let tmp_path = format!("{}.tmp", kill_path);
    match std::fs::File::create(&tmp_path) {
        Ok(mut file) => {
            use std::io::Write;
            let json_bytes = serde_json::to_string_pretty(&kill_state).unwrap_or_default();
            if let Err(e) = file.write_all(json_bytes.as_bytes()) {
                tracing::error!(error = %e, "Failed to write kill_state.json.tmp");
            } else if let Err(e) = file.sync_all() {
                tracing::error!(error = %e, "Failed to fsync kill_state.json.tmp");
            } else if let Err(e) = std::fs::rename(&tmp_path, &kill_path) {
                tracing::error!(error = %e, "Failed to rename kill_state.json.tmp to kill_state.json");
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to create kill_state.json.tmp");
        }
    }

    // THEN set atomic bool — file is durable on disk before in-memory state changes.
    let trigger = TriggerEvent::ManualKillAll {
        reason: body.reason.clone(),
        initiated_by: body.initiated_by.clone(),
    };
    state.kill_switch.activate_kill_all(&trigger);

    // Broadcast to WebSocket clients.
    crate::api::websocket::broadcast_event(&state, WsEvent::KillSwitchActivation {
        level: "KILL_ALL".into(),
        agent_id: None,
        reason: body.reason.clone(),
    });

    // Propagate through distributed kill gate if available.
    // Scoped block ensures RwLockWriteGuard is dropped before any .await (Send requirement).
    let gate_poisoned = if let Some(ref gate) = state.kill_gate {
        match gate.write() {
            Ok(mut bridge) => {
                bridge.close_and_propagate(body.reason.clone());
                tracing::info!("KILL_ALL propagated through distributed kill gate");
                None
            }
            Err(e) => {
                // T-5.4.2: RwLock poisoned — fall back to HTTP fanout.
                // Kill signal MUST reach peers; silent failure causes split-brain.
                let err_msg = format!("Kill gate RwLock poisoned during KILL_ALL. Error: {e}. Falling back to HTTP fanout.");
                tracing::error!("CRITICAL: {}", err_msg);
                Some(err_msg)
            }
        }
    } else {
        None
    };
    if let Some(ref err_msg) = gate_poisoned {
        write_audit_entry(
            &state, "kill_gate_poison", "critical", None, err_msg,
        ).await;
    }

    // HTTP fanout to mesh peers (T-X.25).
    crate::api::kill_fanout::propagate_kill(&state, "KILL_ALL", &body.reason, None);

    // Write to audit log.
    write_audit_entry(
        &state, "kill_all", "critical", None,
        &format!("KILL_ALL activated by {}. Reason: {}", body.initiated_by, body.reason),
    ).await;

    // Fire webhooks for kill_switch event (T-4.3.1).
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
            crate::api::webhooks::fire_webhooks(&db, &state_ref, "kill_switch", payload).await;
        });
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "kill_all_activated",
            "reason": body.reason,
            "initiated_by": body.initiated_by,
        })),
    )
}

/// POST /api/safety/pause/{agent_id} — pause a specific agent (Req 14b AC3).
pub async fn pause_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<PauseRequest>,
) -> impl IntoResponse {
    // T-5.11.2: Safety action cooldown.
    if let Err(resp) = check_safety_cooldown(&state, "api") {
        return resp;
    }

    // T-5.4.3: Use helper with DB fallback on registry poisoning.
    let agent_id = match lookup_agent_id(&state, &agent_id_str) {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    tracing::warn!(
        agent_id = %agent_id,
        reason = %body.reason,
        "Agent pause requested via API"
    );

    let trigger = TriggerEvent::ManualPause {
        agent_id,
        reason: body.reason.clone(),
        initiated_by: "api".into(),
    };
    state.kill_switch.activate_agent(agent_id, KillLevel::Pause, &trigger);

    // Broadcast to WebSocket clients.
    crate::api::websocket::broadcast_event(&state, WsEvent::KillSwitchActivation {
        level: "PAUSE".into(),
        agent_id: Some(agent_id.to_string()),
        reason: body.reason.clone(),
    });

    // Write to audit log.
    write_audit_entry(
        &state, "pause_agent", "high", Some(&agent_id.to_string()),
        &format!("Agent paused. Reason: {}", body.reason),
    ).await;

    // Fire webhooks for intervention_change event (T-4.3.1).
    {
        let db = Arc::clone(&state.db);
        let state_ref = Arc::clone(&state);
        let payload = serde_json::json!({
            "event": "intervention_change",
            "action": "pause",
            "agent_id": agent_id.to_string(),
            "reason": body.reason,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        tokio::spawn(async move {
            crate::api::webhooks::fire_webhooks(&db, &state_ref, "intervention_change", payload).await;
        });
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "paused",
            "agent_id": agent_id.to_string(),
            "reason": body.reason,
        })),
    )
}

/// POST /api/safety/resume/{agent_id} — resume a paused/quarantined agent (Req 14b AC3-4).
///
/// T-5.1.2: Quarantine resume requires `admin` or `security_reviewer` role.
/// Forensic review is persisted as an audit entry with reviewer identity
/// BEFORE the resume is allowed.
pub async fn resume_agent(
    State(state): State<Arc<AppState>>,
    claims: Option<axum::Extension<crate::api::auth::Claims>>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<ResumeRequest>,
) -> impl IntoResponse {
    // T-5.4.3: Use helper with DB fallback on registry poisoning.
    let agent_id = match lookup_agent_id(&state, &agent_id_str) {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    // Extract actor identity from JWT claims (T-5.1.2).
    let actor_id = claims
        .as_ref()
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "unknown".into());
    let actor_role = claims
        .as_ref()
        .map(|c| c.role.clone())
        .unwrap_or_default();

    // Check current kill state for this agent.
    let current = state.kill_switch.current_state();
    let agent_state = current.per_agent.get(&agent_id);

    match agent_state.map(|s| s.level) {
        Some(KillLevel::Quarantine) => {
            // T-5.1.2: Quarantine resume requires admin or security_reviewer role.
            if actor_role != "admin" && actor_role != "security_reviewer" {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "Quarantine resume requires 'admin' or 'security_reviewer' role",
                        "agent_id": agent_id.to_string(),
                        "current_role": actor_role,
                    })),
                );
            }

            // Quarantine resume requires forensic review + second confirmation.
            if !body.forensic_reviewed.unwrap_or(false) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "forensic review required for quarantine resume",
                        "agent_id": agent_id.to_string(),
                    })),
                );
            }

            // T-5.1.2: Persist forensic review as audit entry BEFORE allowing resume.
            write_audit_entry(
                &state, "forensic_review", "critical", Some(&agent_id.to_string()),
                &format!("Forensic review completed by {actor_id} (role: {actor_role}) for quarantine resume"),
            ).await;

            if !body.second_confirmation.unwrap_or(false) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "second confirmation required for quarantine resume",
                        "agent_id": agent_id.to_string(),
                    })),
                );
            }
        }
        Some(KillLevel::KillAll) => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "cannot resume from KILL_ALL via agent resume — use platform resume",
                    "agent_id": agent_id.to_string(),
                })),
            );
        }
        None | Some(KillLevel::Normal) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "agent is not paused or quarantined",
                    "agent_id": agent_id.to_string(),
                })),
            );
        }
        _ => {}
    }

    // Pass expected_level to prevent TOCTOU: if the level was escalated
    // between our check above and the actual resume inside the write lock,
    // the resume will be rejected.
    let expected = agent_state.map(|s| s.level);
    match state.kill_switch.resume_agent(agent_id, expected) {
        Ok(()) => {
            tracing::info!(agent_id = %agent_id, "Agent resumed via API");
            let is_quarantine = agent_state.map(|s| s.level) == Some(KillLevel::Quarantine);

            // Broadcast state change to WebSocket clients.
            crate::api::websocket::broadcast_event(&state, WsEvent::AgentStateChange {
                agent_id: agent_id.to_string(),
                new_state: "resumed".into(),
            });

            // Write to audit log.
            let severity = if is_quarantine { "critical" } else { "high" };
            write_audit_entry(
                &state, "resume_agent", severity, Some(&agent_id.to_string()),
                &format!("Agent resumed (from {})", if is_quarantine { "quarantine" } else { "pause" }),
            ).await;

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "resumed",
                    "agent_id": agent_id.to_string(),
                    "heightened_monitoring": is_quarantine,
                    "monitoring_duration_hours": if is_quarantine { 24 } else { 0 },
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e, "agent_id": agent_id.to_string()})),
        ),
    }
}

/// POST /api/safety/quarantine/{agent_id} — quarantine a specific agent.
pub async fn quarantine_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<PauseRequest>,
) -> impl IntoResponse {
    // T-5.11.2: Safety action cooldown.
    if let Err(resp) = check_safety_cooldown(&state, "api") {
        return resp;
    }

    // T-5.4.3: Use helper with DB fallback on registry poisoning.
    let agent_id = match lookup_agent_id(&state, &agent_id_str) {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    tracing::warn!(
        agent_id = %agent_id,
        reason = %body.reason,
        "Agent quarantine requested via API"
    );

    let trigger = TriggerEvent::ManualQuarantine {
        agent_id,
        reason: body.reason.clone(),
        initiated_by: "api".into(),
    };
    state.kill_switch.activate_agent(agent_id, KillLevel::Quarantine, &trigger);

    // Broadcast to WebSocket clients.
    crate::api::websocket::broadcast_event(&state, WsEvent::KillSwitchActivation {
        level: "QUARANTINE".into(),
        agent_id: Some(agent_id.to_string()),
        reason: body.reason.clone(),
    });

    // Write to audit log.
    write_audit_entry(
        &state, "quarantine_agent", "critical", Some(&agent_id.to_string()),
        &format!("Agent quarantined. Reason: {}", body.reason),
    ).await;

    // Fire webhooks for intervention_change event (T-4.3.1).
    {
        let db = Arc::clone(&state.db);
        let state_ref = Arc::clone(&state);
        let payload = serde_json::json!({
            "event": "intervention_change",
            "action": "quarantine",
            "agent_id": agent_id.to_string(),
            "reason": body.reason,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        tokio::spawn(async move {
            crate::api::webhooks::fire_webhooks(&db, &state_ref, "intervention_change", payload).await;
        });
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "quarantined",
            "agent_id": agent_id.to_string(),
            "reason": body.reason,
            "resume_requires": "forensic_review + second_confirmation + 24h_monitoring",
        })),
    )
}

/// GET /api/safety/status — get current kill switch state.
///
/// T-5.1.4: `viewer` role sees only `{platform_killed, state}`.
/// `operator`+ sees the full breakdown including per-agent details and gate topology.
pub async fn safety_status(
    State(state): State<Arc<AppState>>,
    claims: Option<axum::Extension<crate::api::auth::Claims>>,
) -> impl IntoResponse {
    let ks_state = state.kill_switch.current_state();
    let platform_killed = PLATFORM_KILLED.load(std::sync::atomic::Ordering::SeqCst);

    // T-5.1.4: Determine if caller has elevated access.
    let role = claims.as_ref().map(|c| c.role.as_str()).unwrap_or("viewer");
    let is_elevated = matches!(role, "admin" | "operator" | "security_reviewer");

    // T-5.1.4: Viewer only sees minimal state.
    if !is_elevated {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "platform_killed": platform_killed,
                "state": format!("{:?}", ks_state.platform_level),
            })),
        );
    }

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

    // Include distributed gate state if available.
    // T-5.4.2: On RwLock poisoning, return degraded state info rather than hiding it.
    let gate_state = state.kill_gate.as_ref().map(|gate| {
        match gate.read() {
            Ok(guard) => {
                let snapshot = guard.gate.snapshot();
                serde_json::json!({
                    "state": format!("{:?}", snapshot.state),
                    "node_id": snapshot.node_id.to_string(),
                    "closed_at": snapshot.closed_at.map(|t| t.to_rfc3339()),
                    "close_reason": snapshot.close_reason,
                    "acked_nodes": snapshot.acked_nodes.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "chain_length": snapshot.chain_length,
                })
            }
            Err(e) => {
                tracing::error!(error = %e, "CRITICAL: Kill gate RwLock poisoned in safety_status");
                serde_json::json!({
                    "state": "POISONED",
                    "error": "Kill gate lock poisoned — gate state unreliable. Restart required.",
                })
            }
        }
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "platform_level": format!("{:?}", ks_state.platform_level),
            "platform_killed": platform_killed,
            "per_agent": per_agent,
            "activated_at": ks_state.activated_at.map(|t| t.to_rfc3339()),
            "trigger": ks_state.trigger,
            "distributed_gate": gate_state,
        })),
    )
}

#[derive(Debug, Deserialize)]
pub struct KillAllRequest {
    pub reason: String,
    pub initiated_by: String,
}

#[derive(Debug, Deserialize)]
pub struct PauseRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct ResumeRequest {
    pub level: Option<String>,
    pub forensic_reviewed: Option<bool>,
    pub second_confirmation: Option<bool>,
}
