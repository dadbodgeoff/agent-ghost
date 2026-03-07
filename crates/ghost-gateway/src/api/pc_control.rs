use std::path::Path;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::api::websocket::WsEvent;
use crate::config::GhostConfig;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeZone {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionBudget {
    pub max_per_minute: u32,
    pub max_per_hour: u32,
    pub used_this_minute: u32,
    pub used_this_hour: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcControlStatus {
    pub enabled: bool,
    pub action_budget: ActionBudget,
    pub allowed_apps: Vec<String>,
    pub safe_zones: Vec<SafeZone>,
    pub blocked_hotkeys: Vec<String>,
    pub circuit_breaker_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionLogEntry {
    pub id: String,
    pub action_type: String,
    pub target: String,
    pub timestamp: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PcControlActionsResponse {
    pub actions: Vec<ActionLogEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ActionLogQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePcControlStatusRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct AllowedAppsRequest {
    pub apps: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlockedHotkeysRequest {
    pub hotkeys: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SafeZonesRequest {
    pub zones: Vec<SafeZone>,
}

fn to_safe_zones(config: &ghost_pc_control::safety::PcControlConfig) -> Vec<SafeZone> {
    config
        .safe_zone
        .as_ref()
        .map(|zone| SafeZone {
            x: zone.x,
            y: zone.y,
            width: zone.width,
            height: zone.height,
            label: "Primary Safe Zone".to_string(),
        })
        .into_iter()
        .collect()
}

fn latest_safe_zone(zones: &[SafeZone]) -> Option<ghost_pc_control::safety::ScreenRegion> {
    zones
        .last()
        .map(|zone| ghost_pc_control::safety::ScreenRegion {
            x: zone.x,
            y: zone.y,
            width: zone.width,
            height: zone.height,
        })
}

fn action_budget(
    db: &rusqlite::Connection,
    config: &ghost_pc_control::safety::PcControlConfig,
) -> Result<ActionBudget, ApiError> {
    let minute_window = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
    let hour_window = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();

    let used_this_minute: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM pc_control_actions WHERE created_at >= ?1",
            [&minute_window],
            |row| row.get(0),
        )
        .map_err(|e| ApiError::db_error("pc_control.used_this_minute", e))?;

    let used_this_hour: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM pc_control_actions WHERE created_at >= ?1",
            [&hour_window],
            |row| row.get(0),
        )
        .map_err(|e| ApiError::db_error("pc_control.used_this_hour", e))?;

    let max_per_minute = config
        .circuit_breaker
        .max_actions_per_second
        .saturating_mul(60);
    let max_per_hour = max_per_minute.saturating_mul(60);

    Ok(ActionBudget {
        max_per_minute,
        max_per_hour,
        used_this_minute,
        used_this_hour,
    })
}

fn status_from_config(
    db: &rusqlite::Connection,
    config: &ghost_pc_control::safety::PcControlConfig,
) -> Result<PcControlStatus, ApiError> {
    Ok(PcControlStatus {
        enabled: config.enabled,
        action_budget: action_budget(db, config)?,
        allowed_apps: config.allowed_apps.clone(),
        safe_zones: to_safe_zones(config),
        blocked_hotkeys: config.blocked_hotkeys.clone(),
        circuit_breaker_state: "closed".to_string(),
    })
}

fn read_pc_control_config() -> Result<ghost_pc_control::safety::PcControlConfig, ApiError> {
    Ok(GhostConfig::load_default(None)
        .map_err(|e| ApiError::internal(format!("load config: {e}")))?
        .pc_control)
}

fn write_pc_control_config(
    path: &Path,
    config: &ghost_pc_control::safety::PcControlConfig,
) -> Result<(), ApiError> {
    let mut root = if path.exists() {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| ApiError::internal(format!("read config: {e}")))?;
        serde_yaml::from_str::<serde_yaml::Value>(&raw)
            .map_err(|e| ApiError::internal(format!("parse config yaml: {e}")))?
    } else {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    };

    let mapping = root
        .as_mapping_mut()
        .ok_or_else(|| ApiError::internal("config root must be a YAML mapping"))?;
    let pc_control_value = serde_yaml::to_value(config)
        .map_err(|e| ApiError::internal(format!("serialize pc_control: {e}")))?;
    mapping.insert(
        serde_yaml::Value::String("pc_control".to_string()),
        pc_control_value,
    );

    let yaml = serde_yaml::to_string(&root)
        .map_err(|e| ApiError::internal(format!("render config yaml: {e}")))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::internal(format!("create config dir: {e}")))?;
    }
    std::fs::write(path, yaml).map_err(|e| ApiError::internal(format!("write config: {e}")))?;

    Ok(())
}

async fn persist_pc_control_config(
    state: &Arc<AppState>,
    mutate: impl FnOnce(&mut ghost_pc_control::safety::PcControlConfig),
) -> Result<PcControlStatus, ApiError> {
    let path = GhostConfig::default_path(None);
    let mut full_config = GhostConfig::load_default(None)
        .map_err(|e| ApiError::internal(format!("load config: {e}")))?;
    mutate(&mut full_config.pc_control);
    write_pc_control_config(&path, &full_config.pc_control)?;

    crate::api::websocket::broadcast_event(
        state,
        WsEvent::AgentConfigChange {
            agent_id: "system".to_string(),
            changed_fields: vec!["pc_control".to_string()],
        },
    );

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("pc_control.persist_status", e))?;
    status_from_config(&db, &full_config.pc_control)
}

pub async fn get_status(State(state): State<Arc<AppState>>) -> ApiResult<PcControlStatus> {
    let config = read_pc_control_config()?;
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("pc_control.status", e))?;
    Ok(Json(status_from_config(&db, &config)?))
}

pub async fn list_actions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActionLogQuery>,
) -> ApiResult<PcControlActionsResponse> {
    let limit = query.limit.unwrap_or(100).min(500);
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("pc_control.actions", e))?;

    let mut stmt = db
        .prepare(
            "SELECT id, action_type, skill_name, target_app, coordinates, blocked, block_reason, created_at
             FROM pc_control_actions
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .map_err(|e| ApiError::db_error("pc_control.actions_prepare", e))?;

    let actions = stmt
        .query_map([limit], |row| {
            let target_app: Option<String> = row.get(3)?;
            let coordinates: Option<String> = row.get(4)?;
            let blocked = row.get::<_, i32>(5)? != 0;
            let block_reason: Option<String> = row.get(6)?;
            let skill_name: String = row.get(2)?;

            Ok(ActionLogEntry {
                id: row.get(0)?,
                action_type: row.get(1)?,
                target: target_app.or(coordinates).unwrap_or(skill_name),
                timestamp: row.get(7)?,
                result: if blocked {
                    block_reason.unwrap_or_else(|| "blocked".to_string())
                } else {
                    "ok".to_string()
                },
            })
        })
        .map_err(|e| ApiError::db_error("pc_control.actions_query", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::db_error("pc_control.actions_collect", e))?;

    Ok(Json(PcControlActionsResponse { actions }))
}

pub async fn update_status(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdatePcControlStatusRequest>,
) -> ApiResult<PcControlStatus> {
    Ok(Json(
        persist_pc_control_config(&state, |config| {
            config.enabled = body.enabled;
        })
        .await?,
    ))
}

pub async fn update_allowed_apps(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AllowedAppsRequest>,
) -> ApiResult<PcControlStatus> {
    Ok(Json(
        persist_pc_control_config(&state, |config| {
            config.allowed_apps = body.apps;
        })
        .await?,
    ))
}

pub async fn update_blocked_hotkeys(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BlockedHotkeysRequest>,
) -> ApiResult<PcControlStatus> {
    Ok(Json(
        persist_pc_control_config(&state, |config| {
            config.blocked_hotkeys = body.hotkeys;
        })
        .await?,
    ))
}

pub async fn update_safe_zones(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SafeZonesRequest>,
) -> ApiResult<PcControlStatus> {
    Ok(Json(
        persist_pc_control_config(&state, |config| {
            config.safe_zone = latest_safe_zone(&body.zones);
        })
        .await?,
    ))
}
