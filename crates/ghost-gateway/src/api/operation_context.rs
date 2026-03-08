use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, Request};
use axum::middleware::Next;
use axum::response::Response;
use serde::{Deserialize, Serialize};

pub const REQUEST_ID_HEADER: &str = "x-request-id";
pub const OPERATION_ID_HEADER: &str = "x-ghost-operation-id";
pub const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
pub const IDEMPOTENCY_STATUS_HEADER: &str = "x-ghost-idempotency-status";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyStatus {
    Executed,
    Replayed,
    InProgress,
    Mismatch,
}

impl IdempotencyStatus {
    pub fn as_header_value(&self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::Replayed => "replayed",
            Self::InProgress => "in_progress",
            Self::Mismatch => "mismatch",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationContext {
    pub request_id: String,
    pub operation_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub idempotency_status: Option<IdempotencyStatus>,
    pub is_mutating: bool,
    pub client_supplied_operation_id: bool,
    pub client_supplied_idempotency_key: bool,
}

#[derive(Clone, Debug)]
pub struct RequestId(pub String);

fn is_mutating_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn read_header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
}

fn set_header(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let Ok(header_value) = HeaderValue::from_str(value) {
        headers.insert(name, header_value);
    }
}

pub async fn operation_context_middleware(mut request: Request<Body>, next: Next) -> Response {
    let is_mutating = is_mutating_method(request.method());

    let client_operation_id = read_header(request.headers(), OPERATION_ID_HEADER);
    let client_idempotency_key = read_header(request.headers(), IDEMPOTENCY_KEY_HEADER);
    let operation_id = client_operation_id
        .clone()
        .or_else(|| is_mutating.then(|| uuid::Uuid::now_v7().to_string()));
    let idempotency_key = client_idempotency_key
        .clone()
        .or_else(|| is_mutating.then(|| operation_id.clone()).flatten());
    let request_id = read_header(request.headers(), REQUEST_ID_HEADER)
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    {
        let headers = request.headers_mut();
        set_header(headers, REQUEST_ID_HEADER, &request_id);
        if let Some(operation_id) = &operation_id {
            set_header(headers, OPERATION_ID_HEADER, operation_id);
        }
        if let Some(idempotency_key) = &idempotency_key {
            set_header(headers, IDEMPOTENCY_KEY_HEADER, idempotency_key);
        }
    }

    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    request.extensions_mut().insert(OperationContext {
        request_id: request_id.clone(),
        operation_id: operation_id.clone(),
        idempotency_key: idempotency_key.clone(),
        idempotency_status: None,
        is_mutating,
        client_supplied_operation_id: client_operation_id.is_some(),
        client_supplied_idempotency_key: client_idempotency_key.is_some(),
    });

    let mut response = next.run(request).await;
    let response_headers = response.headers_mut();
    set_header(response_headers, REQUEST_ID_HEADER, &request_id);
    if let Some(operation_id) = &operation_id {
        set_header(response_headers, OPERATION_ID_HEADER, operation_id);
    }
    if let Some(idempotency_key) = &idempotency_key {
        set_header(response_headers, IDEMPOTENCY_KEY_HEADER, idempotency_key);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::extract::Extension;
    use axum::http::{Request, StatusCode};
    use axum::response::Json;
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    async fn context_handler(
        Extension(context): Extension<OperationContext>,
    ) -> Json<OperationContext> {
        Json(context)
    }

    #[tokio::test]
    async fn mutating_requests_get_full_operation_envelope() {
        let app = Router::new()
            .route("/mutate", post(context_handler))
            .layer(axum::middleware::from_fn(operation_context_middleware));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mutate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let context: OperationContext = serde_json::from_slice(&body).unwrap();
        assert!(!context.client_supplied_operation_id);
        assert!(!context.client_supplied_idempotency_key);
    }

    #[tokio::test]
    async fn caller_supplied_ids_are_tracked_explicitly() {
        let app = Router::new()
            .route("/mutate", post(context_handler))
            .layer(axum::middleware::from_fn(operation_context_middleware));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mutate")
                    .header(OPERATION_ID_HEADER, "op-123")
                    .header(IDEMPOTENCY_KEY_HEADER, "idem-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        let operation_id = headers
            .get(OPERATION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap();
        let idempotency_key = headers
            .get(IDEMPOTENCY_KEY_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap();

        assert_eq!(operation_id, "op-123");
        assert_eq!(idempotency_key, "idem-123");
        assert!(headers.get(REQUEST_ID_HEADER).is_some());
        assert_eq!(
            headers
                .get(IDEMPOTENCY_STATUS_HEADER)
                .and_then(|value| value.to_str().ok()),
            None
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let context: OperationContext = serde_json::from_slice(&body).unwrap();
        assert!(context.client_supplied_operation_id);
        assert!(context.client_supplied_idempotency_key);
    }

    #[tokio::test]
    async fn get_requests_keep_request_id_only_by_default() {
        let app = Router::new()
            .route("/read", get(context_handler))
            .layer(axum::middleware::from_fn(operation_context_middleware));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/read")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert!(headers.get(REQUEST_ID_HEADER).is_some());
        assert!(headers.get(OPERATION_ID_HEADER).is_none());
        assert!(headers.get(IDEMPOTENCY_KEY_HEADER).is_none());
        assert!(headers.get(IDEMPOTENCY_STATUS_HEADER).is_none());
    }
}
