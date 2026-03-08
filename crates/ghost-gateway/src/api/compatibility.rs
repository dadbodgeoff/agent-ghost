use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::api::error::ApiError;
use crate::api::operation_context::{
    IDEMPOTENCY_KEY_HEADER, OPERATION_ID_HEADER, REQUEST_ID_HEADER,
};

pub const CLIENT_NAME_HEADER: &str = "x-ghost-client-name";
pub const CLIENT_VERSION_HEADER: &str = "x-ghost-client-version";
pub const COMPATIBILITY_CONTRACT_VERSION: u32 = 1;

const SUPPORTED_CLIENTS: &[SupportedClientPolicy] = &[
    SupportedClientPolicy::new("dashboard", "0.1.0", "0.2.0"),
    SupportedClientPolicy::new("desktop", "0.1.0", "0.2.0"),
    SupportedClientPolicy::new("cli", "0.1.0", "0.2.0"),
    SupportedClientPolicy::new("sdk", "0.1.0", "0.2.0"),
];

#[derive(Clone, Copy, Debug)]
struct SupportedClientPolicy {
    client_name: &'static str,
    minimum_version: &'static str,
    maximum_version_exclusive: &'static str,
}

impl SupportedClientPolicy {
    const fn new(
        client_name: &'static str,
        minimum_version: &'static str,
        maximum_version_exclusive: &'static str,
    ) -> Self {
        Self {
            client_name,
            minimum_version,
            maximum_version_exclusive,
        }
    }

    fn contains_version(&self, version: &ParsedVersion) -> Result<bool, VersionParseError> {
        let minimum = ParsedVersion::parse(self.minimum_version)?;
        let maximum = ParsedVersion::parse(self.maximum_version_exclusive)?;
        Ok(version >= &minimum && version < &maximum)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CompatibilityClientRange {
    pub client_name: &'static str,
    pub minimum_version: &'static str,
    pub maximum_version_exclusive: &'static str,
    pub enforcement: &'static str,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CompatibilityStatus {
    pub gateway_version: &'static str,
    pub compatibility_contract_version: u32,
    pub policy_a_writes_require_explicit_client_identity: bool,
    pub required_mutation_headers: Vec<&'static str>,
    pub supported_clients: Vec<CompatibilityClientRange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CompatibleClient {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum VersionParseError {
    InvalidFormat,
    InvalidComponent,
}

impl ParsedVersion {
    fn parse(input: &str) -> Result<Self, VersionParseError> {
        let normalized = input
            .split_once('+')
            .map(|(prefix, _)| prefix)
            .unwrap_or(input);
        let normalized = normalized
            .split_once('-')
            .map(|(prefix, _)| prefix)
            .unwrap_or(normalized);

        let mut parts = normalized.split('.');
        let major = parts
            .next()
            .ok_or(VersionParseError::InvalidFormat)?
            .parse::<u64>()
            .map_err(|_| VersionParseError::InvalidComponent)?;
        let minor = parts
            .next()
            .ok_or(VersionParseError::InvalidFormat)?
            .parse::<u64>()
            .map_err(|_| VersionParseError::InvalidComponent)?;
        let patch = parts
            .next()
            .ok_or(VersionParseError::InvalidFormat)?
            .parse::<u64>()
            .map_err(|_| VersionParseError::InvalidComponent)?;

        if parts.next().is_some() {
            return Err(VersionParseError::InvalidFormat);
        }

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

fn is_mutating_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn read_header(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn supported_client_ranges() -> Vec<CompatibilityClientRange> {
    SUPPORTED_CLIENTS
        .iter()
        .map(|policy| CompatibilityClientRange {
            client_name: policy.client_name,
            minimum_version: policy.minimum_version,
            maximum_version_exclusive: policy.maximum_version_exclusive,
            enforcement: "policy_a_writes",
        })
        .collect()
}

fn compatibility_status() -> CompatibilityStatus {
    CompatibilityStatus {
        gateway_version: env!("CARGO_PKG_VERSION"),
        compatibility_contract_version: COMPATIBILITY_CONTRACT_VERSION,
        policy_a_writes_require_explicit_client_identity: true,
        required_mutation_headers: vec![
            REQUEST_ID_HEADER,
            OPERATION_ID_HEADER,
            IDEMPOTENCY_KEY_HEADER,
            CLIENT_NAME_HEADER,
            CLIENT_VERSION_HEADER,
        ],
        supported_clients: supported_client_ranges(),
    }
}

fn find_supported_client(name: &str) -> Option<&'static SupportedClientPolicy> {
    SUPPORTED_CLIENTS
        .iter()
        .find(|policy| policy.client_name.eq_ignore_ascii_case(name))
}

fn validate_client_identity(
    name: Option<String>,
    version: Option<String>,
) -> Result<CompatibleClient, ApiError> {
    let client_name = name.ok_or_else(|| {
        ApiError::with_details(
            StatusCode::UPGRADE_REQUIRED,
            "CLIENT_COMPATIBILITY_REQUIRED",
            "Mutating requests require explicit client compatibility headers.",
            serde_json::json!({
                "missing_header": CLIENT_NAME_HEADER,
                "required_headers": compatibility_status().required_mutation_headers,
            }),
        )
    })?;

    let client_version = version.ok_or_else(|| {
        ApiError::with_details(
            StatusCode::UPGRADE_REQUIRED,
            "CLIENT_COMPATIBILITY_REQUIRED",
            "Mutating requests require explicit client compatibility headers.",
            serde_json::json!({
                "missing_header": CLIENT_VERSION_HEADER,
                "required_headers": compatibility_status().required_mutation_headers,
            }),
        )
    })?;

    let policy = find_supported_client(&client_name).ok_or_else(|| {
        ApiError::with_details(
            StatusCode::UPGRADE_REQUIRED,
            "CLIENT_COMPATIBILITY_UNKNOWN",
            "This gateway does not recognize the supplied client identity for policy-A writes.",
            serde_json::json!({
                "client_name": client_name,
                "supported_clients": supported_client_ranges(),
            }),
        )
    })?;

    let parsed_version = ParsedVersion::parse(&client_version).map_err(|_| {
        ApiError::with_details(
            StatusCode::UPGRADE_REQUIRED,
            "CLIENT_VERSION_INVALID",
            "Client version is not a valid semantic version.",
            serde_json::json!({
                "client_name": policy.client_name,
                "client_version": client_version,
            }),
        )
    })?;

    let version_supported = policy.contains_version(&parsed_version).map_err(|_| {
        ApiError::internal("gateway compatibility policy contains an invalid version range")
    })?;

    if !version_supported {
        return Err(ApiError::with_details(
            StatusCode::UPGRADE_REQUIRED,
            "CLIENT_VERSION_UNSUPPORTED",
            "Client version is outside the supported compatibility window for policy-A writes.",
            serde_json::json!({
                "client_name": policy.client_name,
                "client_version": client_version,
                "minimum_version": policy.minimum_version,
                "maximum_version_exclusive": policy.maximum_version_exclusive,
                "gateway_version": env!("CARGO_PKG_VERSION"),
            }),
        ));
    }

    Ok(CompatibleClient {
        name: policy.client_name.to_string(),
        version: client_version,
    })
}

pub async fn compatibility_handler() -> Json<CompatibilityStatus> {
    Json(compatibility_status())
}

pub async fn enforce_client_compatibility_middleware(
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if !is_mutating_method(request.method()) {
        return next.run(request).await;
    }

    let client_name = read_header(request.headers(), CLIENT_NAME_HEADER);
    let client_version = read_header(request.headers(), CLIENT_VERSION_HEADER);

    match validate_client_identity(client_name, client_version) {
        Ok(client) => {
            request.extensions_mut().insert(client);
            next.run(request).await
        }
        Err(error) => error.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Extension;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use tower::ServiceExt;

    async fn current_client(
        Extension(client): Extension<CompatibleClient>,
    ) -> Json<CompatibleClient> {
        Json(client)
    }

    #[test]
    fn supported_versions_accept_current_release_window() {
        let policy = find_supported_client("dashboard").unwrap();
        assert!(policy
            .contains_version(&ParsedVersion::parse("0.1.0").unwrap())
            .unwrap());
        assert!(policy
            .contains_version(&ParsedVersion::parse("0.1.9").unwrap())
            .unwrap());
        assert!(!policy
            .contains_version(&ParsedVersion::parse("0.0.99").unwrap())
            .unwrap());
        assert!(!policy
            .contains_version(&ParsedVersion::parse("0.2.0").unwrap())
            .unwrap());
    }

    #[tokio::test]
    async fn middleware_rejects_missing_client_headers_on_mutations() {
        let app =
            Router::new()
                .route("/mutate", post(current_client))
                .layer(axum::middleware::from_fn(
                    enforce_client_compatibility_middleware,
                ));

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

        assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    }

    #[tokio::test]
    async fn middleware_accepts_supported_clients_and_skips_reads() {
        let app = Router::new()
            .route("/mutate", post(current_client))
            .route("/read", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(
                enforce_client_compatibility_middleware,
            ));

        let mutating = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mutate")
                    .header(CLIENT_NAME_HEADER, "dashboard")
                    .header(CLIENT_VERSION_HEADER, "0.1.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(mutating.status(), StatusCode::OK);

        let read = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/read")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(read.status(), StatusCode::OK);
    }
}
