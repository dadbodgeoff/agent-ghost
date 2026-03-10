use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::autonomy::{
    AutonomyJobListResponse, AutonomyPolicyDocument, AutonomyRunListResponse,
    AutonomyStatusResponse, AutonomySuppressionsResponse,
};
use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AutonomyListParams {
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PutAutonomyPolicyRequest {
    pub policy: AutonomyPolicyDocument,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AutonomyPolicyResponse {
    pub scope_kind: String,
    pub scope_key: String,
    pub policy: AutonomyPolicyDocument,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateSuppressionRequest {
    pub scope_kind: String,
    pub scope_key: String,
    pub fingerprint: String,
    pub reason: String,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApproveRunRequest {
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApproveRunResponse {
    pub run_id: String,
    pub approval_state: String,
    pub approval_expires_at: String,
}

fn actor_from_claims(claims: Option<Extension<Claims>>) -> String {
    claims
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| "unknown".to_string())
}

fn limit_or_default(params: &AutonomyListParams) -> usize {
    params.limit.unwrap_or(50).clamp(1, 200) as usize
}

pub async fn get_status(State(state): State<Arc<AppState>>) -> Json<AutonomyStatusResponse> {
    Json(state.autonomy.status(&state).await)
}

pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AutonomyListParams>,
) -> ApiResult<AutonomyJobListResponse> {
    Ok(Json(
        state
            .autonomy
            .list_jobs(&state, limit_or_default(&params))
            .await?,
    ))
}

pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AutonomyListParams>,
) -> ApiResult<AutonomyRunListResponse> {
    Ok(Json(
        state
            .autonomy
            .list_runs(&state, limit_or_default(&params))
            .await?,
    ))
}

pub async fn get_global_policy(
    State(state): State<Arc<AppState>>,
) -> ApiResult<AutonomyPolicyResponse> {
    let policy = state
        .autonomy
        .get_policy_document(&state, "platform", "global")
        .await?;
    Ok(Json(AutonomyPolicyResponse {
        scope_kind: "platform".into(),
        scope_key: "global".into(),
        policy,
    }))
}

pub async fn put_global_policy(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Json(body): Json<PutAutonomyPolicyRequest>,
) -> ApiResult<AutonomyPolicyResponse> {
    state
        .autonomy
        .put_policy_document(
            &state,
            "platform",
            "global",
            &body.policy,
            &actor_from_claims(claims),
        )
        .await?;
    Ok(Json(AutonomyPolicyResponse {
        scope_kind: "platform".into(),
        scope_key: "global".into(),
        policy: body.policy,
    }))
}

pub async fn get_agent_policy(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> ApiResult<AutonomyPolicyResponse> {
    let policy = state
        .autonomy
        .get_policy_document(&state, "agent", &agent_id)
        .await?;
    Ok(Json(AutonomyPolicyResponse {
        scope_kind: "agent".into(),
        scope_key: agent_id,
        policy,
    }))
}

pub async fn put_agent_policy(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Path(agent_id): Path<String>,
    Json(body): Json<PutAutonomyPolicyRequest>,
) -> ApiResult<AutonomyPolicyResponse> {
    Uuid::parse_str(&agent_id).map_err(|_| ApiError::bad_request("invalid agent_id"))?;
    state
        .autonomy
        .put_policy_document(
            &state,
            "agent",
            &agent_id,
            &body.policy,
            &actor_from_claims(claims),
        )
        .await?;
    Ok(Json(AutonomyPolicyResponse {
        scope_kind: "agent".into(),
        scope_key: agent_id,
        policy: body.policy,
    }))
}

pub async fn create_suppression(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Json(body): Json<CreateSuppressionRequest>,
) -> ApiResult<AutonomySuppressionsResponse> {
    state
        .autonomy
        .create_suppression(
            &state,
            &body.scope_kind,
            &body.scope_key,
            &body.fingerprint,
            &body.reason,
            body.expires_at.as_deref(),
            &actor_from_claims(claims),
            &body.metadata,
        )
        .await?;
    Ok(Json(
        state
            .autonomy
            .list_suppressions(&state, &body.scope_kind, &body.scope_key)
            .await?,
    ))
}

pub async fn approve_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
    claims: Option<Extension<Claims>>,
    Json(body): Json<ApproveRunRequest>,
) -> Result<(StatusCode, Json<ApproveRunResponse>), ApiError> {
    let approval = state
        .autonomy
        .approve_run(
            &state,
            &run_id,
            body.ttl_seconds.unwrap_or(15 * 60),
            &actor_from_claims(claims),
        )
        .await?;
    Ok((
        StatusCode::OK,
        Json(ApproveRunResponse {
            run_id,
            approval_state: approval.0,
            approval_expires_at: approval.1,
        }),
    ))
}
