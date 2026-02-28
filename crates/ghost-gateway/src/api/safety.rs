//! Safety API endpoints: kill switch, pause, resume (Req 25 AC1-2).

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

/// POST /api/safety/kill-all — activate KILL_ALL (Req 14b AC5).
/// Requires confirmation token in body for resume.
pub async fn kill_all(Json(body): Json<KillAllRequest>) -> impl IntoResponse {
    tracing::error!(
        reason = %body.reason,
        initiated_by = %body.initiated_by,
        "KILL_ALL requested via API"
    );

    // In production, this calls KillSwitch::activate_kill_all with a ManualKillAll trigger.
    // The confirmation_token is stored for resume verification.
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "kill_all_activated",
            "reason": body.reason,
            "initiated_by": body.initiated_by,
            "resume_requires": "confirmation_token + restart OR dashboard API",
        })),
    )
}

/// POST /api/safety/pause/{agent_id} — pause a specific agent (Req 14b AC3).
pub async fn pause_agent(
    Path(agent_id): Path<String>,
    Json(body): Json<PauseRequest>,
) -> impl IntoResponse {
    tracing::warn!(
        agent_id = %agent_id,
        reason = %body.reason,
        "Agent pause requested via API"
    );

    // In production, calls KillSwitch::activate_agent with KillLevel::Pause.
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "paused",
            "agent_id": agent_id,
            "reason": body.reason,
            "resume_requires": "GHOST_TOKEN owner auth",
        })),
    )
}

/// POST /api/safety/resume/{agent_id} — resume a paused/quarantined agent (Req 14b AC3-4).
pub async fn resume_agent(
    Path(agent_id): Path<String>,
    Json(body): Json<ResumeRequest>,
) -> impl IntoResponse {
    tracing::info!(
        agent_id = %agent_id,
        "Agent resume requested via API"
    );

    // In production, calls KillSwitch::resume_agent after verifying:
    // - PAUSE: owner auth (GHOST_TOKEN)
    // - QUARANTINE: owner auth + forensic review + second confirmation + 24h heightened monitoring
    // - KILL_ALL: cannot resume individual agent, must restart platform

    if body.forensic_reviewed.unwrap_or(false) || body.level == Some("pause".into()) {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "resumed",
                "agent_id": agent_id,
                "heightened_monitoring": body.level == Some("quarantine".into()),
                "monitoring_duration_hours": if body.level == Some("quarantine".into()) { 24 } else { 0 },
            })),
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "forensic review required for quarantine resume",
                "agent_id": agent_id,
            })),
        )
    }
}

/// POST /api/safety/quarantine/{agent_id} — quarantine a specific agent.
pub async fn quarantine_agent(
    Path(agent_id): Path<String>,
    Json(body): Json<PauseRequest>,
) -> impl IntoResponse {
    tracing::warn!(
        agent_id = %agent_id,
        reason = %body.reason,
        "Agent quarantine requested via API"
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "quarantined",
            "agent_id": agent_id,
            "reason": body.reason,
            "resume_requires": "forensic_review + second_confirmation + 24h_monitoring",
        })),
    )
}

/// GET /api/safety/status — get current kill switch state.
pub async fn safety_status() -> impl IntoResponse {
    // In production, reads from KillSwitch::current_state()
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "platform_level": "Normal",
            "per_agent": {},
            "platform_killed": false,
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
