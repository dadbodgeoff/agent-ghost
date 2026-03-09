use std::path::Path;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::OperationContext;
use crate::api::websocket::WsEvent;
use crate::config::GhostConfig;
use crate::state::AppState;

const UPDATE_PC_CONTROL_STATUS_ROUTE_TEMPLATE: &str = "/api/pc-control/status";
const UPDATE_PC_CONTROL_ALLOWED_APPS_ROUTE_TEMPLATE: &str = "/api/pc-control/allowed-apps";
const UPDATE_PC_CONTROL_BLOCKED_HOTKEYS_ROUTE_TEMPLATE: &str = "/api/pc-control/blocked-hotkeys";
const UPDATE_PC_CONTROL_SAFE_ZONES_ROUTE_TEMPLATE: &str = "/api/pc-control/safe-zones";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct SafeZone {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionBudget {
    pub max_per_minute: u32,
    pub max_per_hour: u32,
    pub used_this_minute: u32,
    pub used_this_hour: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PcControlStatus {
    pub enabled: bool,
    pub action_budget: ActionBudget,
    pub allowed_apps: Vec<String>,
    pub safe_zone: Option<SafeZone>,
    pub safe_zones: Vec<SafeZone>,
    pub blocked_hotkeys: Vec<String>,
    pub circuit_breaker_state: String,
    pub persisted: PcControlPersistedState,
    pub runtime: PcControlRuntimeState,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PcControlPersistedState {
    pub enabled: bool,
    pub allowed_apps: Vec<String>,
    pub safe_zone: Option<SafeZone>,
    pub blocked_hotkeys: Vec<String>,
    pub action_budget: ActionBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PcControlRuntimeState {
    pub circuit_breaker_state: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActionLogEntry {
    pub id: String,
    pub action_type: String,
    pub target: String,
    pub timestamp: String,
    pub result: String,
    pub input_json: String,
    pub result_json: String,
    pub target_app: Option<String>,
    pub coordinates: Option<String>,
    pub blocked: bool,
    pub block_reason: Option<String>,
    pub agent_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PcControlActionsResponse {
    pub actions: Vec<ActionLogEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ActionLogQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct UpdatePcControlStatusRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct AllowedAppsRequest {
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct BlockedHotkeysRequest {
    pub hotkeys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SafeZonesRequest {
    #[serde(default)]
    pub zones: Option<Vec<SafeZone>>,
    #[serde(default)]
    #[schema(value_type = Option<SafeZone>)]
    pub safe_zone: Option<Option<SafeZone>>,
}

impl SafeZonesRequest {
    fn into_safe_zone(self) -> Result<Option<ghost_pc_control::safety::ScreenRegion>, ApiError> {
        match (self.zones, self.safe_zone) {
            (Some(_), Some(_)) => Err(pc_control_bad_request(
                "provide either safe_zone or zones, not both",
            )),
            (Some(zones), None) => match zones.len() {
                0 => Ok(None),
                1 => zones.into_iter().next().map(validate_safe_zone).transpose(),
                _ => Err(pc_control_bad_request(
                    "only one safe zone is supported; send safe_zone or exactly one zones entry",
                )),
            },
            (None, Some(zone)) => zone.map(validate_safe_zone).transpose(),
            (None, None) => Err(pc_control_bad_request(
                "request must include safe_zone or zones",
            )),
        }
    }
}

fn pc_control_bad_request(message: impl Into<String>) -> ApiError {
    ApiError::custom(
        StatusCode::BAD_REQUEST,
        "INVALID_PC_CONTROL_REQUEST",
        message,
    )
}

fn to_safe_zone(zone: &ghost_pc_control::safety::ScreenRegion) -> SafeZone {
    SafeZone {
        x: zone.x,
        y: zone.y,
        width: zone.width,
        height: zone.height,
        label: "Primary Safe Zone".to_string(),
    }
}

fn validate_safe_zone(zone: SafeZone) -> Result<ghost_pc_control::safety::ScreenRegion, ApiError> {
    if zone.width == 0 || zone.height == 0 {
        return Err(pc_control_bad_request(
            "safe_zone width and height must both be greater than zero",
        ));
    }

    let right = i64::from(zone.x) + i64::from(zone.width);
    let bottom = i64::from(zone.y) + i64::from(zone.height);
    if right > i64::from(i32::MAX) || bottom > i64::from(i32::MAX) {
        return Err(pc_control_bad_request(
            "safe_zone rectangle exceeds supported coordinate bounds",
        ));
    }

    Ok(ghost_pc_control::safety::ScreenRegion {
        x: zone.x,
        y: zone.y,
        width: zone.width,
        height: zone.height,
    })
}

fn runtime_circuit_breaker_state(state: &AppState) -> Result<String, ApiError> {
    let breaker = state
        .pc_control_circuit_breaker
        .lock()
        .map_err(|e| ApiError::internal(format!("pc control breaker lock poisoned: {e}")))?;
    Ok(match breaker.state() {
        ghost_pc_control::safety::circuit_breaker::CircuitState::Closed => "closed",
        ghost_pc_control::safety::circuit_breaker::CircuitState::Open => "open",
        ghost_pc_control::safety::circuit_breaker::CircuitState::HalfOpen => "half_open",
    }
    .to_string())
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
    state: &AppState,
    config: &ghost_pc_control::safety::PcControlConfig,
) -> Result<PcControlStatus, ApiError> {
    let budget = action_budget(db, config)?;
    let safe_zone = config.safe_zone.as_ref().map(to_safe_zone);
    let runtime_state = runtime_circuit_breaker_state(state)?;

    Ok(PcControlStatus {
        enabled: config.enabled,
        action_budget: budget.clone(),
        allowed_apps: config.allowed_apps.clone(),
        safe_zone: safe_zone.clone(),
        safe_zones: safe_zone.iter().cloned().collect(),
        blocked_hotkeys: config.blocked_hotkeys.clone(),
        circuit_breaker_state: runtime_state.clone(),
        persisted: PcControlPersistedState {
            enabled: config.enabled,
            allowed_apps: config.allowed_apps.clone(),
            safe_zone,
            blocked_hotkeys: config.blocked_hotkeys.clone(),
            action_budget: budget,
        },
        runtime: PcControlRuntimeState {
            circuit_breaker_state: runtime_state,
        },
    })
}

fn read_pc_control_config(state: &AppState) -> Result<ghost_pc_control::safety::PcControlConfig, ApiError> {
    Ok(GhostConfig::load(&state.config_path)
        .map_err(|e| ApiError::internal(format!("load config: {e}")))?
        .pc_control)
}

fn cleanup_pc_control_temp_path(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to clean up pc-control config temp file"
            );
        }
    }
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
    let tmp_path = path.with_extension("yml.tmp");
    let write_result = (|| -> Result<(), ApiError> {
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|e| ApiError::internal(format!("create config temp file: {e}")))?;
        use std::io::Write;
        file.write_all(yaml.as_bytes())
            .map_err(|e| ApiError::internal(format!("write config temp file: {e}")))?;
        file.sync_all()
            .map_err(|e| ApiError::internal(format!("fsync config temp file: {e}")))?;
        std::fs::rename(&tmp_path, path)
            .map_err(|e| ApiError::internal(format!("rename config temp file: {e}")))?;
        Ok(())
    })();

    if let Err(error) = write_result {
        cleanup_pc_control_temp_path(&tmp_path);
        return Err(error);
    }

    Ok(())
}

fn persist_pc_control_config(
    state: &Arc<AppState>,
    mutate: impl FnOnce(&mut ghost_pc_control::safety::PcControlConfig),
) -> Result<PcControlStatus, ApiError> {
    let mut full_config = GhostConfig::load(&state.config_path)
        .map_err(|e| ApiError::internal(format!("load config: {e}")))?;
    mutate(&mut full_config.pc_control);
    write_pc_control_config(&state.config_path, &full_config.pc_control)?;

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
    status_from_config(&db, state, &full_config.pc_control)
}

fn pc_control_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

pub async fn get_status(State(state): State<Arc<AppState>>) -> ApiResult<PcControlStatus> {
    let config = read_pc_control_config(&state)?;
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("pc_control.status", e))?;
    Ok(Json(status_from_config(&db, &state, &config)?))
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
            "SELECT id, action_type, skill_name, target_app, coordinates, blocked, block_reason,
                    created_at, input_json, result_json, agent_id, session_id
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
                input_json: row.get(8)?,
                result_json: row.get(9)?,
                target_app: row.get(3)?,
                coordinates: row.get(4)?,
                blocked,
                block_reason: row.get(6)?,
                agent_id: row.get(10)?,
                session_id: row.get(11)?,
            })
        })
        .map_err(|e| ApiError::db_error("pc_control.actions_query", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::db_error("pc_control.actions_collect", e))?;

    Ok(Json(PcControlActionsResponse { actions }))
}

pub async fn update_status(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<UpdatePcControlStatusRequest>,
) -> Response {
    let actor = pc_control_actor(claims.as_ref().map(|claims| &claims.0));
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "PUT",
        UPDATE_PC_CONTROL_STATUS_ROUTE_TEMPLATE,
        &serde_json::to_value(&body).unwrap_or(serde_json::Value::Null),
        |_| {
            let status = persist_pc_control_config(&state, |config| {
                config.enabled = body.enabled;
            })?;
            Ok((
                StatusCode::OK,
                serde_json::to_value(&status).map_err(|e| ApiError::internal(e.to_string()))?,
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "pc_control_status_update",
                "high",
                actor,
                "updated",
                serde_json::json!({ "enabled": body.enabled }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

pub async fn update_allowed_apps(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<AllowedAppsRequest>,
) -> Response {
    let actor = pc_control_actor(claims.as_ref().map(|claims| &claims.0));
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "PUT",
        UPDATE_PC_CONTROL_ALLOWED_APPS_ROUTE_TEMPLATE,
        &serde_json::to_value(&body).unwrap_or(serde_json::Value::Null),
        |_| {
            let status = persist_pc_control_config(&state, |config| {
                config.allowed_apps = body.apps.clone();
            })?;
            Ok((
                StatusCode::OK,
                serde_json::to_value(&status).map_err(|e| ApiError::internal(e.to_string()))?,
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "pc_control_allowed_apps_update",
                "high",
                actor,
                "updated",
                serde_json::json!({ "apps": body.apps }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

pub async fn update_blocked_hotkeys(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<BlockedHotkeysRequest>,
) -> Response {
    let actor = pc_control_actor(claims.as_ref().map(|claims| &claims.0));
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "PUT",
        UPDATE_PC_CONTROL_BLOCKED_HOTKEYS_ROUTE_TEMPLATE,
        &serde_json::to_value(&body).unwrap_or(serde_json::Value::Null),
        |_| {
            let status = persist_pc_control_config(&state, |config| {
                config.blocked_hotkeys = body.hotkeys.clone();
            })?;
            Ok((
                StatusCode::OK,
                serde_json::to_value(&status).map_err(|e| ApiError::internal(e.to_string()))?,
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "pc_control_blocked_hotkeys_update",
                "high",
                actor,
                "updated",
                serde_json::json!({ "hotkeys": body.hotkeys }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

pub async fn update_safe_zones(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<SafeZonesRequest>,
) -> Response {
    let safe_zone = match body.into_safe_zone() {
        Ok(safe_zone) => safe_zone,
        Err(error) => return error_response_with_idempotency(error),
    };
    let actor = pc_control_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "safe_zone": safe_zone.as_ref().map(to_safe_zone),
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "PUT",
        UPDATE_PC_CONTROL_SAFE_ZONES_ROUTE_TEMPLATE,
        &request_body,
        |_| {
            let status = persist_pc_control_config(&state, |config| {
                config.safe_zone = safe_zone.clone();
            })?;
            Ok((
                StatusCode::OK,
                serde_json::to_value(&status).map_err(|e| ApiError::internal(e.to_string()))?,
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "platform",
                "pc_control_safe_zone_update",
                "high",
                actor,
                "updated",
                request_body,
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;

    use std::sync::{Mutex, OnceLock, RwLock};

    use axum::body::to_bytes;
    use axum::extract::{Query, State};
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

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

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
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

    async fn response_json(response: Response) -> serde_json::Value {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    fn write_test_config(path: &Path) {
        write_test_config_with(path, crate::config::GhostConfig::default());
    }

    fn write_test_config_with(path: &Path, config: crate::config::GhostConfig) {
        let yaml = serde_yaml::to_string(&config).unwrap();
        std::fs::write(path, yaml).unwrap();
    }

    async fn test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("ghost.yml");
        write_test_config(&config_path);
        let db_path = temp_dir.path().join("ghost.db");
        let db = crate::db_pool::create_pool(db_path).unwrap();
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

        let state = Arc::new(AppState {
            gateway: shared_state,
            config_path: config_path.clone(),
            agents: Arc::new(RwLock::new(crate::agents::registry::AgentRegistry::new())),
            kill_switch: Arc::new(crate::safety::kill_switch::KillSwitch::new()),
            quarantine: Arc::new(RwLock::new(
                crate::safety::quarantine::QuarantineManager::new(),
            )),
            db: Arc::clone(&db),
            event_tx,
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
        });

        (state, temp_dir)
    }

    #[tokio::test]
    async fn status_reports_runtime_breaker_state_separately() {
        let (state, _temp_dir) = test_state().await;

        {
            let mut breaker = state.pc_control_circuit_breaker.lock().unwrap();
            breaker.record_failure();
            breaker.record_failure();
            breaker.record_failure();
        }

        let Json(status) = get_status(State(state)).await.unwrap();
        assert_eq!(status.runtime.circuit_breaker_state, "open");
        assert_eq!(status.circuit_breaker_state, "open");
        assert_eq!(status.persisted.safe_zone, None);
    }

    #[test]
    fn write_pc_control_config_cleans_up_temp_file_on_rename_failure() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ghost.yml");
        std::fs::create_dir_all(&config_path).unwrap();

        let error = write_pc_control_config(
            &config_path,
            &ghost_pc_control::safety::PcControlConfig::default(),
        )
        .unwrap_err();
        assert!(matches!(error, ApiError::Internal(_)));
        assert!(!config_path.with_extension("yml.tmp").exists());
        assert!(config_path.is_dir());
    }

    #[tokio::test]
    async fn update_safe_zones_rejects_multiple_entries() {
        let (state, _temp_dir) = test_state().await;

        let response = update_safe_zones(
            State(state),
            None,
            Extension(operation_context("req-multi", "op-multi", "idem-multi")),
            Json(SafeZonesRequest {
                zones: Some(vec![
                    SafeZone {
                        x: 0,
                        y: 0,
                        width: 100,
                        height: 100,
                        label: "one".into(),
                    },
                    SafeZone {
                        x: 10,
                        y: 10,
                        width: 50,
                        height: 50,
                        label: "two".into(),
                    },
                ]),
                safe_zone: None,
            }),
        )
        .await;

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_safe_zones_rejects_zero_area_and_overflow() {
        let (state, _temp_dir) = test_state().await;

        let zero_area = update_safe_zones(
            State(Arc::clone(&state)),
            None,
            Extension(operation_context("req-zero", "op-zero", "idem-zero")),
            Json(SafeZonesRequest {
                zones: Some(vec![SafeZone {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 10,
                    label: "bad".into(),
                }]),
                safe_zone: None,
            }),
        )
        .await;
        assert_eq!(zero_area.status(), axum::http::StatusCode::BAD_REQUEST);

        let overflow = update_safe_zones(
            State(state),
            None,
            Extension(operation_context(
                "req-overflow",
                "op-overflow",
                "idem-overflow",
            )),
            Json(SafeZonesRequest {
                zones: Some(vec![SafeZone {
                    x: i32::MAX,
                    y: 0,
                    width: 1,
                    height: 10,
                    label: "overflow".into(),
                }]),
                safe_zone: None,
            }),
        )
        .await;
        assert_eq!(overflow.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_safe_zones_replays_committed_response_with_audit_provenance() {
        let (state, _temp_dir) = test_state().await;

        let request = SafeZonesRequest {
            zones: Some(vec![SafeZone {
                x: 10,
                y: 20,
                width: 30,
                height: 40,
                label: "primary".into(),
            }]),
            safe_zone: None,
        };

        let first = update_safe_zones(
            State(Arc::clone(&state)),
            None,
            Extension(operation_context(
                "req-safe-zone-1",
                "op-safe-zone",
                "idem-safe-zone",
            )),
            Json(request.clone()),
        )
        .await;

        assert_eq!(first.status(), axum::http::StatusCode::OK);
        assert_eq!(
            first
                .headers()
                .get(crate::api::operation_context::IDEMPOTENCY_STATUS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("executed")
        );
        let body = response_json(first).await;
        assert_eq!(body["persisted"]["safe_zone"]["x"], 10);

        let replay = update_safe_zones(
            State(Arc::clone(&state)),
            None,
            Extension(operation_context(
                "req-safe-zone-2",
                "op-safe-zone",
                "idem-safe-zone",
            )),
            Json(request),
        )
        .await;

        assert_eq!(replay.status(), axum::http::StatusCode::OK);
        assert_eq!(
            replay
                .headers()
                .get(crate::api::operation_context::IDEMPOTENCY_STATUS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("replayed")
        );

        let db = state.db.write().await;
        let audit_rows: Vec<(Option<String>, Option<String>, Option<String>)> = db
            .prepare(
                "SELECT request_id, idempotency_key, idempotency_status
                 FROM audit_log
                 WHERE operation_id = ?1 AND event_type = 'pc_control_safe_zone_update'
                 ORDER BY rowid ASC",
            )
            .unwrap()
            .query_map(["op-safe-zone"], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(audit_rows.len(), 2);
        assert_eq!(audit_rows[0].2.as_deref(), Some("executed"));
        assert_eq!(audit_rows[1].0.as_deref(), Some("req-safe-zone-2"));
        assert_eq!(audit_rows[1].2.as_deref(), Some("replayed"));
    }

    #[tokio::test]
    async fn update_safe_zones_rejects_idempotency_key_reuse_with_different_payload() {
        let (state, _temp_dir) = test_state().await;

        let first = update_safe_zones(
            State(Arc::clone(&state)),
            None,
            Extension(operation_context(
                "req-safe-zone-first",
                "op-safe-zone-first",
                "idem-safe-zone-shared",
            )),
            Json(SafeZonesRequest {
                zones: Some(vec![SafeZone {
                    x: 1,
                    y: 2,
                    width: 30,
                    height: 40,
                    label: "first".into(),
                }]),
                safe_zone: None,
            }),
        )
        .await;
        assert_eq!(first.status(), axum::http::StatusCode::OK);

        let conflict = update_safe_zones(
            State(state),
            None,
            Extension(operation_context(
                "req-safe-zone-conflict",
                "op-safe-zone-conflict",
                "idem-safe-zone-shared",
            )),
            Json(SafeZonesRequest {
                zones: Some(vec![SafeZone {
                    x: 99,
                    y: 2,
                    width: 30,
                    height: 40,
                    label: "conflict".into(),
                }]),
                safe_zone: None,
            }),
        )
        .await;
        assert_eq!(conflict.status(), axum::http::StatusCode::CONFLICT);
        assert_eq!(
            conflict
                .headers()
                .get(crate::api::operation_context::IDEMPOTENCY_STATUS_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("mismatch")
        );
        let body = response_json(conflict).await;
        assert_eq!(body["error"]["code"], "IDEMPOTENCY_KEY_REUSED");
    }

    #[tokio::test]
    async fn pc_control_reads_state_config_path_even_when_env_points_elsewhere() {
        let _guard = env_lock().lock().unwrap();
        let (state, temp_dir) = test_state().await;

        let mut primary = crate::config::GhostConfig::default();
        primary.pc_control.enabled = true;
        primary.pc_control.allowed_apps = vec!["GhostApp".into()];
        write_test_config_with(&state.config_path, primary);

        let decoy_path = temp_dir.path().join("decoy.yml");
        let mut decoy = crate::config::GhostConfig::default();
        decoy.pc_control.enabled = false;
        decoy.pc_control.allowed_apps = vec!["WrongApp".into()];
        write_test_config_with(&decoy_path, decoy);
        let _env = EnvVarGuard::set("GHOST_CONFIG", decoy_path.to_str().unwrap());

        let Json(status) = get_status(State(state)).await.unwrap();

        assert!(status.enabled);
        assert_eq!(status.allowed_apps, vec!["GhostApp"]);
    }

    #[tokio::test]
    async fn list_actions_preserves_forensic_fields() {
        let (state, _temp_dir) = test_state().await;
        {
            let db = state.db.write().await;
            cortex_storage::queries::pc_control_queries::insert_action(
                &db,
                "action-1",
                "agent-1",
                "session-1",
                "mouse_click",
                "mouse_click",
                r#"{"x":10,"y":20}"#,
                r#"{"status":"ok"}"#,
                Some("Firefox"),
                Some("10,20"),
                true,
                Some("blocked by test"),
            )
            .unwrap();
        }

        let Json(response) = list_actions(State(state), Query(ActionLogQuery { limit: Some(10) }))
            .await
            .unwrap();

        assert_eq!(response.actions.len(), 1);
        let entry = &response.actions[0];
        assert_eq!(entry.input_json, r#"{"x":10,"y":20}"#);
        assert_eq!(entry.result_json, r#"{"status":"ok"}"#);
        assert_eq!(entry.target_app.as_deref(), Some("Firefox"));
        assert_eq!(entry.coordinates.as_deref(), Some("10,20"));
        assert!(entry.blocked);
        assert_eq!(entry.block_reason.as_deref(), Some("blocked by test"));
        assert_eq!(entry.agent_id, "agent-1");
        assert_eq!(entry.session_id, "session-1");
    }
}
