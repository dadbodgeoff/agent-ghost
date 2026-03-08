//! Skill management endpoints backed by the canonical catalog service.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::{Extension, Json};

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::websocket::WsEvent;
use crate::skill_catalog::{SkillCatalogError, SkillListResponseDto, SkillSummaryDto};
use crate::state::AppState;

fn skill_actor(claims: Option<&Claims>) -> &str {
    claims.map(|claims| claims.sub.as_str()).unwrap_or("system")
}

fn map_catalog_error(error: SkillCatalogError) -> ApiError {
    match error {
        SkillCatalogError::SkillNotFound(name) => {
            ApiError::not_found(format!("Skill '{name}' not found"))
        }
        SkillCatalogError::AlreadyInstalled(name) => {
            ApiError::conflict(format!("Skill '{name}' is already installed"))
        }
        SkillCatalogError::NotInstallable(name) => {
            ApiError::conflict(format!("Skill '{name}' cannot be installed"))
        }
        SkillCatalogError::NotRemovable(name) => {
            ApiError::conflict(format!("Skill '{name}' cannot be uninstalled"))
        }
        SkillCatalogError::SkillDisabled(name) => {
            ApiError::conflict(format!("Skill '{name}' is disabled"))
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
        SkillCatalogError::DbPool(message) | SkillCatalogError::Storage(message) => {
            ApiError::db_error("skill catalog", message)
        }
    }
}

/// GET /api/skills — list installed and available compiled skills.
pub async fn list_skills(State(state): State<Arc<AppState>>) -> ApiResult<SkillListResponseDto> {
    state
        .skill_catalog
        .list_skills()
        .map(Json)
        .map_err(map_catalog_error)
}

/// POST /api/skills/:id/install — install a compiled skill.
pub async fn install_skill(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Path(skill_name): Path<String>,
) -> ApiResult<SkillSummaryDto> {
    let actor = skill_actor(claims.as_ref().map(|Extension(claims)| claims));
    let summary = state
        .skill_catalog
        .install(&skill_name, Some(actor))
        .await
        .map_err(map_catalog_error)?;

    crate::api::websocket::broadcast_event(
        &state,
        WsEvent::SkillChange {
            skill_name,
            action: "installed".into(),
        },
    );

    Ok(Json(summary))
}

/// POST /api/skills/:id/uninstall — disable a removable compiled skill.
pub async fn uninstall_skill(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Path(skill_name): Path<String>,
) -> ApiResult<SkillSummaryDto> {
    let actor = skill_actor(claims.as_ref().map(|Extension(claims)| claims));
    let summary = state
        .skill_catalog
        .uninstall(&skill_name, Some(actor))
        .await
        .map_err(map_catalog_error)?;

    crate::api::websocket::broadcast_event(
        &state,
        WsEvent::SkillChange {
            skill_name,
            action: "uninstalled".into(),
        },
    );

    Ok(Json(summary))
}
