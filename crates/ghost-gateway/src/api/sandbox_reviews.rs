use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Extension;
use axum::Json;

use crate::api::auth::Claims;
use crate::sandbox_reviews::{SandboxReviewDecisionRequest, SandboxReviewListParams};
use crate::state::AppState;

fn sandbox_review_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

pub async fn list_sandbox_reviews(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SandboxReviewListParams>,
) -> impl IntoResponse {
    match state.sandbox_reviews.list_reviews(&params).await {
        Ok(reviews) => (
            StatusCode::OK,
            Json(serde_json::json!({ "reviews": reviews })),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub async fn approve_sandbox_review(
    State(state): State<Arc<AppState>>,
    Path(review_id): Path<String>,
    claims: Option<Extension<Claims>>,
    Json(body): Json<SandboxReviewDecisionRequest>,
) -> impl IntoResponse {
    let actor = sandbox_review_actor(claims.as_ref().map(|claims| &claims.0));
    match state
        .sandbox_reviews
        .resolve(
            &review_id,
            ghost_agent_loop::tools::executor::SandboxReviewDecision::Approved,
            actor,
            body.note,
        )
        .await
    {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({ "review_id": review_id, "status": "approved" })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "pending sandbox review not found" })),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}

pub async fn reject_sandbox_review(
    State(state): State<Arc<AppState>>,
    Path(review_id): Path<String>,
    claims: Option<Extension<Claims>>,
    Json(body): Json<SandboxReviewDecisionRequest>,
) -> impl IntoResponse {
    let actor = sandbox_review_actor(claims.as_ref().map(|claims| &claims.0));
    match state
        .sandbox_reviews
        .resolve(
            &review_id,
            ghost_agent_loop::tools::executor::SandboxReviewDecision::Rejected,
            actor,
            body.note,
        )
        .await
    {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({ "review_id": review_id, "status": "rejected" })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "pending sandbox review not found" })),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        ),
    }
}
