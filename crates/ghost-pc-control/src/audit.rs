//! Audit trail helper for PC control actions.
//!
//! All PC control actions — executed or blocked — are logged to the
//! `pc_control_actions` table via `cortex-storage` queries.
//!
//! Logging is best-effort: errors are traced but never block execution.

use rusqlite::Connection;
use uuid::Uuid;

/// Log a successful PC control action to the audit trail.
pub fn log_pc_action(
    db: &Connection,
    agent_id: Uuid,
    session_id: Uuid,
    skill_name: &str,
    input: &serde_json::Value,
    result: &serde_json::Value,
) {
    let id = Uuid::now_v7().to_string();
    let target_app = input.get("target_app").and_then(|v| v.as_str());
    let coordinates = format_coordinates(input);

    if let Err(e) = cortex_storage::queries::pc_control_queries::insert_action(
        db,
        &id,
        &agent_id.to_string(),
        &session_id.to_string(),
        skill_name,
        skill_name,
        &input.to_string(),
        &result.to_string(),
        target_app,
        coordinates.as_deref(),
        false,
        None,
    ) {
        tracing::warn!(
            error = %e,
            skill = skill_name,
            "failed to log pc control action"
        );
    }
}

/// Log a blocked PC control action to the audit trail.
pub fn log_blocked_action(
    db: &Connection,
    agent_id: Uuid,
    session_id: Uuid,
    skill_name: &str,
    input: &serde_json::Value,
    reason: &str,
) {
    let id = Uuid::now_v7().to_string();
    let target_app = input.get("target_app").and_then(|v| v.as_str());
    let coordinates = format_coordinates(input);

    if let Err(e) = cortex_storage::queries::pc_control_queries::insert_action(
        db,
        &id,
        &agent_id.to_string(),
        &session_id.to_string(),
        skill_name,
        skill_name,
        &input.to_string(),
        "{}",
        target_app,
        coordinates.as_deref(),
        true,
        Some(reason),
    ) {
        tracing::warn!(
            error = %e,
            skill = skill_name,
            reason = reason,
            "failed to log blocked pc control action"
        );
    }
}

/// Extract coordinates from input JSON if present.
fn format_coordinates(input: &serde_json::Value) -> Option<String> {
    let x = input.get("x").and_then(|v| v.as_i64());
    let y = input.get("y").and_then(|v| v.as_i64());
    match (x, y) {
        (Some(x), Some(y)) => Some(format!("{x},{y}")),
        _ => None,
    }
}
