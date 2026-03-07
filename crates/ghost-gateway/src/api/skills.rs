//! Skill management endpoints (T-4.2.1).
//!
//! List available/installed skills, install, and uninstall.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::api::error::{ApiError, ApiResult};
use crate::api::websocket::WsEvent;
use crate::state::AppState;

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub source: String,
    pub state: String,
}

#[derive(Debug, Serialize)]
pub struct SkillListResponse {
    pub installed: Vec<SkillInfo>,
    pub available: Vec<SkillInfo>,
}

// ── Handlers ───────────────────────────────────────────────────────

/// GET /api/skills — list installed and available skills.
///
/// Returns all registered skills from AppState.safety_skills as "installed",
/// and an empty "available" list (marketplace not yet implemented).
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
) -> ApiResult<SkillListResponse> {
    // Build installed list from actual registered skills in AppState.
    let mut installed: Vec<SkillInfo> = state.safety_skills.iter().map(|(name, skill)| {
        let source = match skill.source() {
            ghost_skills::registry::SkillSource::Bundled => "bundled",
            ghost_skills::registry::SkillSource::User => "user",
            ghost_skills::registry::SkillSource::Workspace => "workspace",
        };
        SkillInfo {
            id: name.clone(),
            name: name.clone(),
            version: "1.0.0".into(),
            description: skill.description().to_string(),
            capabilities: vec![format!("skill:{name}")],
            source: source.into(),
            state: "active".into(),
        }
    }).collect();

    // Sort by name for stable ordering.
    installed.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(SkillListResponse {
        installed,
        available: Vec::new(),
    }))
}

/// POST /api/skills/:id/install — install a skill.
pub async fn install_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_name): Path<String>,
) -> ApiResult<SkillInfo> {
    // Check if the skill exists in registered skills.
    let skill = state.safety_skills.get(&skill_name)
        .ok_or_else(|| ApiError::not_found(format!("Skill '{skill_name}' not found")))?;

    let db = state.db.write().await;

    // Check not already installed.
    let exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM installed_skills WHERE skill_name = ?1",
            rusqlite::params![skill_name],
            |row| row.get::<_, i64>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    if exists {
        return Err(ApiError::conflict(format!(
            "Skill '{}' is already installed",
            skill_name
        )));
    }

    let source = match skill.source() {
        ghost_skills::registry::SkillSource::Bundled => "bundled",
        ghost_skills::registry::SkillSource::User => "user",
        ghost_skills::registry::SkillSource::Workspace => "workspace",
    };

    let caps_json = serde_json::json!([format!("skill:{skill_name}")]).to_string();
    let id = uuid::Uuid::now_v7().to_string();
    db.execute(
        "INSERT INTO installed_skills (id, skill_name, version, description, capabilities, source) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, skill_name, "1.0.0", skill.description(), caps_json, source],
    )
    .map_err(|e| ApiError::db_error("install skill", e))?;

    // Broadcast event.
    crate::api::websocket::broadcast_event(&state, WsEvent::SkillChange {
        skill_name: skill_name.clone(),
        action: "installed".into(),
    });

    Ok(Json(SkillInfo {
        id,
        name: skill_name,
        version: "1.0.0".into(),
        description: skill.description().to_string(),
        capabilities: vec![format!("skill:{}", skill.name())],
        source: source.into(),
        state: "active".into(),
    }))
}

/// POST /api/skills/:id/uninstall — uninstall a skill.
pub async fn uninstall_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_name): Path<String>,
) -> ApiResult<serde_json::Value> {
    // Check if the skill is removable.
    if let Some(skill) = state.safety_skills.get(&skill_name) {
        if !skill.removable() {
            return Err(ApiError::conflict(format!(
                "Skill '{}' is a platform-managed safety skill and cannot be uninstalled",
                skill_name
            )));
        }
    }

    let db = state.db.write().await;

    let affected = db
        .execute(
            "DELETE FROM installed_skills WHERE skill_name = ?1 OR id = ?1",
            rusqlite::params![skill_name],
        )
        .map_err(|e| ApiError::db_error("uninstall skill", e))?;

    if affected == 0 {
        return Err(ApiError::not_found(format!(
            "Skill '{skill_name}' is not installed"
        )));
    }

    // Broadcast event.
    crate::api::websocket::broadcast_event(&state, WsEvent::SkillChange {
        skill_name: skill_name.clone(),
        action: "uninstalled".into(),
    });

    Ok(Json(
        serde_json::json!({ "uninstalled": skill_name }),
    ))
}
