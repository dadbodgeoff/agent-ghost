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

/// Write a safety action to the audit_log table.
fn write_audit_entry(state: &AppState, event_type: &str, severity: &str, agent_id: Option<&str>, details: &str) {
    let db = match state.db.lock() {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(error = %e, event_type = %event_type, "DB Mutex poisoned — safety audit entry LOST");
            return;
        }
    };
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

/// POST /api/safety/kill-all — activate KILL_ALL (Req 14b AC5).
pub async fn kill_all(
    State(state): State<Arc<AppState>>,
    Json(body): Json<KillAllRequest>,
) -> impl IntoResponse {
    tracing::error!(
        reason = %body.reason,
        initiated_by = %body.initiated_by,
        "KILL_ALL requested via API"
    );

    // Activate through the real KillSwitch.
    let trigger = TriggerEvent::ManualKillAll {
        reason: body.reason.clone(),
        initiated_by: body.initiated_by.clone(),
    };
    state.kill_switch.activate_kill_all(&trigger);

    // Persist kill_state.json for crash recovery.
    let kill_state = serde_json::json!({
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
    if let Err(e) = std::fs::write(&kill_path, kill_state.to_string()) {
        tracing::error!(error = %e, "Failed to persist kill_state.json");
    }

    // Broadcast to WebSocket clients.
    if let Err(e) = state.event_tx.send(WsEvent::KillSwitchActivation {
        level: "KILL_ALL".into(),
        agent_id: None,
        reason: body.reason.clone(),
    }) {
        tracing::error!(error = %e, "Failed to broadcast KILL_ALL event to WebSocket clients");
    }

    // Propagate through distributed kill gate if available.
    if let Some(ref gate) = state.kill_gate {
        match gate.write() {
            Ok(mut bridge) => {
                bridge.close_and_propagate(body.reason.clone());
                tracing::info!("KILL_ALL propagated through distributed kill gate");
            }
            Err(e) => {
                tracing::error!(error = %e, "FAILED to propagate KILL_ALL through kill gate — SPLIT BRAIN RISK");
            }
        }
    }

    // Write to audit log.
    write_audit_entry(
        &state, "kill_all", "critical", None,
        &format!("KILL_ALL activated by {}. Reason: {}", body.initiated_by, body.reason),
    );

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
    let agent_id = match uuid::Uuid::parse_str(&agent_id_str) {
        Ok(id) => id,
        Err(_) => {
            // Try looking up by name in the registry.
            let agents = match state.agents.read() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!(error = %e, "Agent registry RwLock poisoned in pause_agent");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal server error"})),
                    );
                }
            };
            match agents.lookup_by_name(&agent_id_str) {
                Some(a) => a.id,
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error": "agent not found", "agent_id": agent_id_str})),
                    );
                }
            }
        }
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
    if let Err(e) = state.event_tx.send(WsEvent::KillSwitchActivation {
        level: "PAUSE".into(),
        agent_id: Some(agent_id.to_string()),
        reason: body.reason.clone(),
    }) {
        tracing::warn!(error = %e, "Failed to broadcast PAUSE event to WebSocket clients");
    }

    // Write to audit log.
    write_audit_entry(
        &state, "pause_agent", "high", Some(&agent_id.to_string()),
        &format!("Agent paused. Reason: {}", body.reason),
    );

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
pub async fn resume_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id_str): Path<String>,
    Json(body): Json<ResumeRequest>,
) -> impl IntoResponse {
    let agent_id = match uuid::Uuid::parse_str(&agent_id_str) {
        Ok(id) => id,
        Err(_) => {
            let agents = match state.agents.read() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!(error = %e, "Agent registry RwLock poisoned in resume_agent");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal server error"})),
                    );
                }
            };
            match agents.lookup_by_name(&agent_id_str) {
                Some(a) => a.id,
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error": "agent not found", "agent_id": agent_id_str})),
                    );
                }
            }
        }
    };

    // Check current kill state for this agent.
    let current = state.kill_switch.current_state();
    let agent_state = current.per_agent.get(&agent_id);

    match agent_state.map(|s| s.level) {
        Some(KillLevel::Quarantine) => {
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

    match state.kill_switch.resume_agent(agent_id) {
        Ok(()) => {
            tracing::info!(agent_id = %agent_id, "Agent resumed via API");
            let is_quarantine = agent_state.map(|s| s.level) == Some(KillLevel::Quarantine);

            // Broadcast state change to WebSocket clients.
            if let Err(e) = state.event_tx.send(WsEvent::AgentStateChange {
                agent_id: agent_id.to_string(),
                new_state: "resumed".into(),
            }) {
                tracing::warn!(error = %e, "Failed to broadcast resume event to WebSocket clients");
            }

            // Write to audit log.
            let severity = if is_quarantine { "critical" } else { "high" };
            write_audit_entry(
                &state, "resume_agent", severity, Some(&agent_id.to_string()),
                &format!("Agent resumed (from {})", if is_quarantine { "quarantine" } else { "pause" }),
            );

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
    let agent_id = match uuid::Uuid::parse_str(&agent_id_str) {
        Ok(id) => id,
        Err(_) => {
            let agents = match state.agents.read() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!(error = %e, "Agent registry RwLock poisoned in quarantine_agent");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal server error"})),
                    );
                }
            };
            match agents.lookup_by_name(&agent_id_str) {
                Some(a) => a.id,
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error": "agent not found", "agent_id": agent_id_str})),
                    );
                }
            }
        }
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
    if let Err(e) = state.event_tx.send(WsEvent::KillSwitchActivation {
        level: "QUARANTINE".into(),
        agent_id: Some(agent_id.to_string()),
        reason: body.reason.clone(),
    }) {
        tracing::warn!(error = %e, "Failed to broadcast QUARANTINE event to WebSocket clients");
    }

    // Write to audit log.
    write_audit_entry(
        &state, "quarantine_agent", "critical", Some(&agent_id.to_string()),
        &format!("Agent quarantined. Reason: {}", body.reason),
    );

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
pub async fn safety_status(
    State(state): State<Arc<AppState>>,
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

    // Include distributed gate state if available.
    let gate_state = state.kill_gate.as_ref().and_then(|gate| {
        let bridge = match gate.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = %e, "Kill gate RwLock poisoned in safety_status");
                return None;
            }
        };
        let snapshot = bridge.gate.snapshot();
        Some(serde_json::json!({
            "state": format!("{:?}", snapshot.state),
            "node_id": snapshot.node_id.to_string(),
            "closed_at": snapshot.closed_at.map(|t| t.to_rfc3339()),
            "close_reason": snapshot.close_reason,
            "acked_nodes": snapshot.acked_nodes.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            "chain_length": snapshot.chain_length,
        }))
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
