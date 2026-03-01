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
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
) -> ApiResult<SkillListResponse> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Load installed skills from DB.
    let installed: Vec<SkillInfo> = db
        .prepare(
            "SELECT id, skill_name, version, description, capabilities, source, state \
             FROM installed_skills ORDER BY skill_name",
        )
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| {
                let caps_json: String = row.get::<_, String>(4).unwrap_or_else(|_| "[]".into());
                Ok(SkillInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    version: row.get(2)?,
                    description: row.get(3)?,
                    capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
                    source: row.get(5)?,
                    state: row.get(6)?,
                })
            })?;
            Ok(rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    // Build list of available (not yet installed) skills.
    // In a full implementation, this would query a skill registry or marketplace.
    // For now, return bundled skill metadata.
    let installed_names: std::collections::HashSet<&str> =
        installed.iter().map(|s| s.name.as_str()).collect();

    let available = get_bundled_skills()
        .into_iter()
        .filter(|s| !installed_names.contains(s.name.as_str()))
        .collect();

    Ok(Json(SkillListResponse {
        installed,
        available,
    }))
}

/// POST /api/skills/:id/install — install a skill.
pub async fn install_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_name): Path<String>,
) -> ApiResult<SkillInfo> {
    // Find the skill in the available list.
    let available = get_bundled_skills();
    let skill = available
        .iter()
        .find(|s| s.name == skill_name || s.id == skill_name)
        .ok_or_else(|| ApiError::not_found(format!("Skill '{skill_name}' not found")))?;

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Check not already installed.
    let exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM installed_skills WHERE skill_name = ?1",
            rusqlite::params![skill.name],
            |row| row.get::<_, i64>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    if exists {
        return Err(ApiError::conflict(format!(
            "Skill '{}' is already installed",
            skill.name
        )));
    }

    let caps_json = serde_json::to_string(&skill.capabilities)
        .map_err(|e| ApiError::internal(format!("serialize capabilities: {e}")))?;

    let id = uuid::Uuid::now_v7().to_string();
    db.execute(
        "INSERT INTO installed_skills (id, skill_name, version, description, capabilities, source) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, skill.name, skill.version, skill.description, caps_json, skill.source],
    )
    .map_err(|e| ApiError::db_error("install skill", e))?;

    // Broadcast event.
    let _ = state.event_tx.send(WsEvent::SkillChange {
        skill_name: skill.name.clone(),
        action: "installed".into(),
    });

    Ok(Json(SkillInfo {
        id,
        name: skill.name.clone(),
        version: skill.version.clone(),
        description: skill.description.clone(),
        capabilities: skill.capabilities.clone(),
        source: skill.source.clone(),
        state: "active".into(),
    }))
}

/// POST /api/skills/:id/uninstall — uninstall a skill.
pub async fn uninstall_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_name): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

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
    let _ = state.event_tx.send(WsEvent::SkillChange {
        skill_name: skill_name.clone(),
        action: "uninstalled".into(),
    });

    Ok(Json(
        serde_json::json!({ "uninstalled": skill_name }),
    ))
}

// ── Bundled skill catalog ──────────────────────────────────────────

fn get_bundled_skills() -> Vec<SkillInfo> {
    vec![
        SkillInfo {
            id: "web-search".into(),
            name: "web-search".into(),
            version: "1.0.0".into(),
            description: "Search the web and return relevant results".into(),
            capabilities: vec!["web_search".into(), "api_calls".into()],
            source: "bundled".into(),
            state: "available".into(),
        },
        SkillInfo {
            id: "code-executor".into(),
            name: "code-executor".into(),
            version: "1.0.0".into(),
            description: "Execute code in a WASM sandbox".into(),
            capabilities: vec!["code_execution".into()],
            source: "bundled".into(),
            state: "available".into(),
        },
        SkillInfo {
            id: "file-reader".into(),
            name: "file-reader".into(),
            version: "1.0.0".into(),
            description: "Read files from the workspace".into(),
            capabilities: vec!["file_read".into()],
            source: "bundled".into(),
            state: "available".into(),
        },
        SkillInfo {
            id: "data-analysis".into(),
            name: "data-analysis".into(),
            version: "1.0.0".into(),
            description: "Analyze structured data and generate summaries".into(),
            capabilities: vec!["data_analysis".into(), "file_read".into()],
            source: "bundled".into(),
            state: "available".into(),
        },
        SkillInfo {
            id: "memory-writer".into(),
            name: "memory-writer".into(),
            version: "1.0.0".into(),
            description: "Write and manage agent memory entries".into(),
            capabilities: vec!["memory_write".into()],
            source: "bundled".into(),
            state: "available".into(),
        },
    ]
}
