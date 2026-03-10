//! Route authorization middleware backed by the typed authz policy registry.

use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    middleware::Next,
    response::Response,
};

use crate::api::auth::Claims;
use crate::api::authz::{Action, AuthorizationContext, ResourceContext, RouteId};
use crate::api::authz_policy::{authorize_claims, RouteAuthorizationKind, RouteAuthorizationSpec};
use crate::api::error::ApiError;
use crate::state::AppState;

pub async fn require_route(
    spec: RouteAuthorizationSpec,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let claims = req.extensions().get::<Claims>();
    let context = AuthorizationContext::new(spec.action, spec.route_id);

    match spec.authorization_kind {
        RouteAuthorizationKind::MinimumRole(_) | RouteAuthorizationKind::SafetyReview => {
            authorize_claims(claims, &context)?;
        }
        RouteAuthorizationKind::OwnerOrAdmin => {
            return Err(ApiError::internal(
                "owner-aware routes must use dedicated authz middleware",
            ));
        }
    }

    Ok(next.run(req).await)
}

async fn require_live_execution_route(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    mut req: Request,
    next: Next,
    action: Action,
    route_id: RouteId,
) -> Result<Response, ApiError> {
    let claims = req.extensions().get::<Claims>();
    let db = state
        .db
        .read()
        .map_err(|error| ApiError::db_error("require_live_execution_route", error))?;
    let Some(record) =
        cortex_storage::queries::live_execution_queries::get_by_id(&db, &execution_id)
            .map_err(|error| ApiError::db_error("require_live_execution_route", error))?
    else {
        return Err(ApiError::not_found(format!(
            "live execution {execution_id} not found"
        )));
    };

    let context =
        AuthorizationContext::new(action, route_id).with_resource(ResourceContext::LiveExecution {
            execution_id: &execution_id,
            owner_subject: Some(record.actor_key.as_str()),
        });
    authorize_claims(claims, &context)
        .map_err(|_| ApiError::not_found(format!("live execution {execution_id} not found")))?;

    req.extensions_mut()
        .insert(crate::api::live_executions::AuthorizedLiveExecutionRecord(
            record,
        ));
    Ok(next.run(req).await)
}

pub async fn require_live_execution_read_route(
    state: State<Arc<AppState>>,
    path: Path<String>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    require_live_execution_route(
        state,
        path,
        req,
        next,
        Action::LiveExecutionRead,
        RouteId::LiveExecutionById,
    )
    .await
}

pub async fn require_live_execution_cancel_route(
    state: State<Arc<AppState>>,
    path: Path<String>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    require_live_execution_route(
        state,
        path,
        req,
        next,
        Action::LiveExecutionCancel,
        RouteId::LiveExecutionCancelById,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::require_route;

    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use axum::middleware::from_fn;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    use crate::api::auth::Claims;
    use crate::api::authz::{RouteId, AUTHZ_CLAIMS_VERSION_V1, INTERNAL_JWT_ISSUER};
    use crate::api::authz_policy::route_spec_for;

    fn typed_claims(role: &str, capabilities: &[&str]) -> Claims {
        Claims {
            sub: format!("{role}-subject"),
            role: role.into(),
            capabilities: capabilities
                .iter()
                .map(|capability| (*capability).into())
                .collect(),
            authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
            exp: 42,
            iat: 21,
            jti: format!("{role}-jwt"),
            iss: Some(INTERNAL_JWT_ISSUER.into()),
        }
    }

    fn legacy_claims(role: &str) -> Claims {
        Claims {
            sub: format!("{role}-subject"),
            role: role.into(),
            capabilities: Vec::new(),
            authz_v: None,
            exp: 42,
            iat: 21,
            jti: format!("{role}-legacy"),
            iss: None,
        }
    }

    #[tokio::test]
    async fn route_bound_admin_backup_allows_admin_claims() {
        let spec = route_spec_for(RouteId::AdminBackupCreate, &Method::POST).expect("route spec");
        let router = Router::new()
            .route("/api/admin/backup", post(|| async { StatusCode::OK }))
            .route_layer(from_fn(move |req, next| require_route(spec, req, next)));

        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/admin/backup")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(typed_claims("admin", &[]));

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn route_bound_admin_backup_denies_operator_claims() {
        let spec = route_spec_for(RouteId::AdminBackupCreate, &Method::POST).expect("route spec");
        let router = Router::new()
            .route("/api/admin/backup", post(|| async { StatusCode::OK }))
            .route_layer(from_fn(move |req, next| require_route(spec, req, next)));

        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/admin/backup")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(legacy_claims("operator"));

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn route_bound_safety_resume_allows_operator_with_capability() {
        let spec = route_spec_for(RouteId::SafetyResumeAgent, &Method::POST).expect("route spec");
        let router = Router::new()
            .route(
                "/api/safety/resume/agent-1",
                post(|| async { StatusCode::OK }),
            )
            .route_layer(from_fn(move |req, next| require_route(spec, req, next)));

        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/safety/resume/agent-1")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(typed_claims("operator", &["safety_review"]));

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn route_bound_safety_resume_denies_plain_operator() {
        let spec = route_spec_for(RouteId::SafetyResumeAgent, &Method::POST).expect("route spec");
        let router = Router::new()
            .route(
                "/api/safety/resume/agent-1",
                post(|| async { StatusCode::OK }),
            )
            .route_layer(from_fn(move |req, next| require_route(spec, req, next)));

        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/safety/resume/agent-1")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(typed_claims("operator", &[]));

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn route_bound_safety_resume_allows_legacy_security_reviewer() {
        let spec = route_spec_for(RouteId::SafetyResumeAgent, &Method::POST).expect("route spec");
        let router = Router::new()
            .route(
                "/api/safety/resume/agent-1",
                post(|| async { StatusCode::OK }),
            )
            .route_layer(from_fn(move |req, next| require_route(spec, req, next)));

        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/api/safety/resume/agent-1")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(legacy_claims("security_reviewer"));

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
