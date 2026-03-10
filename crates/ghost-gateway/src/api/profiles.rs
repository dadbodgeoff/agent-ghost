//! Convergence profile CRUD endpoints (T-3.3.1).
//!
//! Manages convergence profiles (weight configurations for the 8-signal scorer).

use std::sync::Arc;

use axum::extract::{Path, State};
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
use crate::state::AppState;

const CREATE_PROFILE_ROUTE_TEMPLATE: &str = "/api/profiles";
const UPDATE_PROFILE_ROUTE_TEMPLATE: &str = "/api/profiles/:name";
const DELETE_PROFILE_ROUTE_TEMPLATE: &str = "/api/profiles/:name";
const ASSIGN_PROFILE_ROUTE_TEMPLATE: &str = "/api/agents/:id/profile";

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProfileSummary {
    pub name: String,
    pub description: String,
    pub is_preset: bool,
    pub weights: [f64; 8],
    pub thresholds: [f64; 4],
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateProfileRequest {
    pub name: String,
    pub description: Option<String>,
    pub weights: [f64; 8],
    pub thresholds: [f64; 4],
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct UpdateProfileRequest {
    pub weights: Option<[f64; 8]>,
    pub thresholds: Option<[f64; 4]>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProfileListResponse {
    pub profiles: Vec<ProfileSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct AssignProfileRequest {
    pub profile_name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AssignProfileResponse {
    pub agent_id: String,
    pub profile_name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeleteProfileResponse {
    pub deleted: String,
}

fn profile_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

fn validate_weights(weights: &[f64; 8]) -> Result<(), ApiError> {
    let sum: f64 = weights.iter().sum();
    if (sum - 1.0).abs() > 0.01 {
        return Err(ApiError::bad_request(format!(
            "Weights must sum to 1.0 (got {sum:.3})"
        )));
    }

    if weights.iter().any(|&w| w < 0.0) {
        return Err(ApiError::bad_request("Weights must be non-negative"));
    }

    Ok(())
}

fn validate_thresholds(thresholds: &[f64; 4]) -> Result<(), ApiError> {
    for (i, &t) in thresholds.iter().enumerate() {
        if !(0.0..=1.0).contains(&t) {
            return Err(ApiError::bad_request(format!(
                "Threshold[{i}] = {t} is out of range [0.0, 1.0]"
            )));
        }
        if i > 0 && t <= thresholds[i - 1] {
            return Err(ApiError::bad_request(format!(
                "Thresholds must be monotonically increasing: threshold[{i}]={t} <= threshold[{}]={}",
                i - 1,
                thresholds[i - 1]
            )));
        }
    }
    Ok(())
}

// ── Presets ─────────────────────────────────────────────────────────

fn preset_profiles() -> Vec<ProfileSummary> {
    vec![
        ProfileSummary {
            name: "standard".into(),
            description: "Balanced scoring across all signals".into(),
            is_preset: true,
            weights: [0.125; 8],
            thresholds: [0.3, 0.5, 0.7, 0.85],
        },
        ProfileSummary {
            name: "research".into(),
            description: "Higher thresholds — more permissive for research agents".into(),
            is_preset: true,
            weights: [0.10, 0.10, 0.10, 0.15, 0.15, 0.15, 0.10, 0.15],
            thresholds: [0.4, 0.6, 0.8, 0.9],
        },
        ProfileSummary {
            name: "companion".into(),
            description: "Lower thresholds — more sensitive to convergence patterns".into(),
            is_preset: true,
            weights: [0.15, 0.15, 0.10, 0.15, 0.15, 0.10, 0.10, 0.10],
            thresholds: [0.2, 0.4, 0.6, 0.75],
        },
        ProfileSummary {
            name: "productivity".into(),
            description: "Task-focused — prioritizes goal boundary and initiative signals".into(),
            is_preset: true,
            weights: [0.05, 0.05, 0.10, 0.10, 0.25, 0.25, 0.10, 0.10],
            thresholds: [0.3, 0.5, 0.7, 0.85],
        },
    ]
}

// ── Handlers ────────────────────────────────────────────────────────

/// GET /api/profiles — list all profiles (presets + custom from DB).
pub async fn list_profiles(State(state): State<Arc<AppState>>) -> ApiResult<ProfileListResponse> {
    let mut profiles = preset_profiles();

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_profiles", e))?;

    let custom: Vec<ProfileSummary> = db
        .prepare(
            "SELECT name, description, weights, thresholds FROM convergence_profiles \
             ORDER BY name",
        )
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let description: String = row.get::<_, String>(1).unwrap_or_default();
                let weights_json: String = row.get::<_, String>(2).unwrap_or_default();
                let thresholds_json: String = row.get::<_, String>(3).unwrap_or_default();
                Ok((name, description, weights_json, thresholds_json))
            })?;
            Ok(rows
                .filter_map(|r| r.ok())
                .filter_map(|(name, desc, w, t)| {
                    let weights: [f64; 8] = serde_json::from_str(&w).ok()?;
                    let thresholds: [f64; 4] = serde_json::from_str(&t).ok()?;
                    Some(ProfileSummary {
                        name,
                        description: desc,
                        is_preset: false,
                        weights,
                        thresholds,
                    })
                })
                .collect())
        })
        .unwrap_or_default();

    profiles.extend(custom);
    Ok(Json(ProfileListResponse { profiles }))
}

/// POST /api/profiles — create a custom profile.
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(req): Json<CreateProfileRequest>,
) -> Response {
    if ["standard", "research", "companion", "productivity"].contains(&req.name.as_str()) {
        return error_response_with_idempotency(ApiError::conflict(
            "Cannot create profile with preset name",
        ));
    }

    if let Err(error) = validate_weights(&req.weights) {
        return error_response_with_idempotency(error);
    }
    if let Err(error) = validate_thresholds(&req.thresholds) {
        return error_response_with_idempotency(error);
    }

    let actor = profile_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::to_value(&req).unwrap_or(serde_json::Value::Null);
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_PROFILE_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let weights_json = serde_json::to_string(&req.weights)
                .map_err(|e| ApiError::internal(format!("serialize weights: {e}")))?;
            let thresholds_json = serde_json::to_string(&req.thresholds)
                .map_err(|e| ApiError::internal(format!("serialize thresholds: {e}")))?;

            conn.execute(
                "INSERT INTO convergence_profiles (name, description, weights, thresholds) \
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    req.name,
                    req.description.as_deref().unwrap_or(""),
                    weights_json,
                    thresholds_json,
                ],
            )
            .map_err(|e| ApiError::db_error("insert profile", e))?;

            Ok((
                StatusCode::CREATED,
                serde_json::to_value(ProfileSummary {
                    name: req.name.clone(),
                    description: req.description.clone().unwrap_or_default(),
                    is_preset: false,
                    weights: req.weights,
                    thresholds: req.thresholds,
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &req.name,
                "create_profile",
                "info",
                actor,
                "created",
                serde_json::json!({ "profile_name": req.name }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// PUT /api/profiles/:name — update a custom profile's weights and thresholds.
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(name): Path<String>,
    Json(req): Json<UpdateProfileRequest>,
) -> Response {
    if ["standard", "research", "companion", "productivity"].contains(&name.as_str()) {
        return error_response_with_idempotency(ApiError::bad_request(
            "Cannot modify preset profiles",
        ));
    }

    if let Some(weights) = &req.weights {
        if let Err(error) = validate_weights(weights) {
            return error_response_with_idempotency(error);
        }
    }
    if let Some(thresholds) = &req.thresholds {
        if let Err(error) = validate_thresholds(thresholds) {
            return error_response_with_idempotency(error);
        }
    }

    let actor = profile_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "profile_name": name,
        "body": req,
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "PUT",
        UPDATE_PROFILE_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let (cur_desc, cur_weights, cur_thresholds): (String, String, String) = conn
                .query_row(
                    "SELECT description, weights, thresholds FROM convergence_profiles WHERE name = ?1",
                    rusqlite::params![name],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|_| ApiError::not_found(format!("Profile '{name}' not found")))?;

            let weights: [f64; 8] = match &req.weights {
                Some(w) => *w,
                None => serde_json::from_str(&cur_weights).unwrap_or([0.125; 8]),
            };
            let thresholds: [f64; 4] = match &req.thresholds {
                Some(t) => *t,
                None => serde_json::from_str(&cur_thresholds).unwrap_or([0.3, 0.5, 0.7, 0.85]),
            };
            let description = req.description.clone().unwrap_or(cur_desc);

            let weights_json = serde_json::to_string(&weights)
                .map_err(|e| ApiError::internal(format!("serialize weights: {e}")))?;
            let thresholds_json = serde_json::to_string(&thresholds)
                .map_err(|e| ApiError::internal(format!("serialize thresholds: {e}")))?;

            conn.execute(
                "UPDATE convergence_profiles SET description = ?1, weights = ?2, thresholds = ?3 WHERE name = ?4",
                rusqlite::params![description, weights_json, thresholds_json, name],
            )
            .map_err(|e| ApiError::db_error("update profile", e))?;

            Ok((
                StatusCode::OK,
                serde_json::to_value(ProfileSummary {
                    name: name.clone(),
                    description,
                    is_preset: false,
                    weights,
                    thresholds,
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &name,
                "update_profile",
                "info",
                actor,
                "updated",
                serde_json::json!({ "profile_name": name }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// DELETE /api/profiles/:name — delete a custom profile.
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(name): Path<String>,
) -> Response {
    if ["standard", "research", "companion", "productivity"].contains(&name.as_str()) {
        return error_response_with_idempotency(ApiError::bad_request(
            "Cannot delete preset profiles",
        ));
    }

    let actor = profile_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "profile_name": name });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "DELETE",
        DELETE_PROFILE_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let affected = conn
                .execute(
                    "DELETE FROM convergence_profiles WHERE name = ?1",
                    rusqlite::params![name],
                )
                .map_err(|e| ApiError::db_error("delete profile", e))?;

            if affected == 0 {
                return Err(ApiError::not_found(format!("Profile '{name}' not found")));
            }

            Ok((
                StatusCode::OK,
                serde_json::to_value(DeleteProfileResponse {
                    deleted: name.clone(),
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &name,
                "delete_profile",
                "high",
                actor,
                "deleted",
                serde_json::json!({ "profile_name": name }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/agents/:id/profile — assign a profile to an agent.
pub async fn assign_profile(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(agent_id): Path<String>,
    Json(req): Json<AssignProfileRequest>,
) -> Response {
    let actor = profile_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "agent_id": agent_id,
        "profile_name": req.profile_name,
    });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        ASSIGN_PROFILE_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let profile_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM convergence_profiles WHERE name = ?1",
                    rusqlite::params![req.profile_name],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .unwrap_or(false);

            if !profile_exists {
                let is_preset = ["standard", "research", "companion", "productivity"]
                    .contains(&req.profile_name.as_str());
                if !is_preset {
                    return Err(ApiError::bad_request(format!(
                        "Profile '{}' does not exist",
                        req.profile_name
                    )));
                }
            }

            conn.execute(
                "INSERT INTO agent_profile_assignments (agent_id, profile_name, updated_at, updated_by)
                 VALUES (?1, ?2, datetime('now'), ?3)
                 ON CONFLICT(agent_id) DO UPDATE SET
                    profile_name = excluded.profile_name,
                    updated_at = excluded.updated_at,
                    updated_by = excluded.updated_by",
                rusqlite::params![agent_id, req.profile_name, actor],
            )
            .map_err(|e| ApiError::db_error("assign profile", e))?;

            Ok((
                StatusCode::OK,
                serde_json::to_value(AssignProfileResponse {
                    agent_id: agent_id.clone(),
                    profile_name: req.profile_name.clone(),
                })
                .unwrap_or(serde_json::Value::Null),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &agent_id,
                "assign_profile",
                "info",
                actor,
                "assigned",
                serde_json::json!({
                    "agent_id": agent_id,
                    "profile_name": req.profile_name,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::to_bytes;
    use axum::extract::State;
    use axum::Extension;
    use std::sync::{Arc, RwLock};

    async fn test_state(db_path: &std::path::Path) -> Arc<AppState> {
        let db = crate::db_pool::create_pool(db_path.to_path_buf()).unwrap();
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }

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
            gateway: Arc::new(crate::gateway::GatewaySharedState::new()),
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
            convergence_profile: "standard".to_string(),
            model_providers: Vec::new(),
            default_model_provider: None,
            pc_control_circuit_breaker: ghost_pc_control::safety::PcControlConfig::default()
                .circuit_breaker(),
            websocket_auth_tickets: Arc::new(dashmap::DashMap::new()),
            ws_ticket_auth_only: false,
            tools_config: crate::config::ToolsConfig::default(),
            custom_safety_checks: Arc::new(RwLock::new(Vec::new())),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            background_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            live_execution_controls: Arc::new(dashmap::DashMap::new()),
            safety_cooldown: Arc::new(crate::api::rate_limit::SafetyCooldown::new()),
            monitor_address: "127.0.0.1:18790".to_string(),
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

    #[tokio::test]
    async fn assign_profile_updates_mutable_table_without_mutating_convergence_scores() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("profiles.db");
        let state = test_state(&db_path).await;

        {
            let writer = state.db.write().await;
            writer
                .execute(
                    "INSERT INTO convergence_scores (
                        id, agent_id, session_id, composite_score, signal_scores, level,
                        profile, computed_at, event_hash, previous_hash
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        "score-1",
                        "agent-1",
                        "session-1",
                        0.42f64,
                        "[]",
                        2i32,
                        "standard",
                        "2026-03-01T00:00:00Z",
                        vec![1u8; 32],
                        vec![0u8; 32],
                    ],
                )
                .unwrap();
        }

        let response = assign_profile(
            State(Arc::clone(&state)),
            Some(Extension(crate::api::auth::Claims {
                sub: "operator-1".to_string(),
                role: "admin".to_string(),
                capabilities: Vec::new(),
                authz_v: None,
                exp: u64::MAX,
                iat: 0,
                jti: "jti-1".to_string(),
                iss: None,
            })),
            Extension(crate::api::operation_context::OperationContext {
                request_id: "req-1".to_string(),
                operation_id: Some("op-1".to_string()),
                idempotency_key: Some("idem-1".to_string()),
                idempotency_status: None,
                is_mutating: true,
                client_supplied_operation_id: true,
                client_supplied_idempotency_key: true,
            }),
            Path("agent-1".to_string()),
            Json(AssignProfileRequest {
                profile_name: "research".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["profile_name"], "research");

        let db = state.db.read().unwrap();
        let assigned: (String, String) = db
            .query_row(
                "SELECT agent_id, profile_name FROM agent_profile_assignments WHERE agent_id = 'agent-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(assigned.0, "agent-1");
        assert_eq!(assigned.1, "research");

        let score_profile: String = db
            .query_row(
                "SELECT profile FROM convergence_scores WHERE id = 'score-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(score_profile, "standard");
    }
}
