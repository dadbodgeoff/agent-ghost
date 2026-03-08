//! HTTP client for CLI → gateway communication (Task 6.6 — §5.4, E.12, E.14).

use reqwest::{Response, StatusCode};
use serde::{Deserialize, Serialize};

use super::error::CliError;

const REQUEST_ID_HEADER: &str = "x-request-id";
const OPERATION_ID_HEADER: &str = "x-ghost-operation-id";
const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
const CLIENT_NAME_HEADER: &str = crate::api::compatibility::CLIENT_NAME_HEADER;
const CLIENT_VERSION_HEADER: &str = crate::api::compatibility::CLIENT_VERSION_HEADER;
const CLI_CLIENT_NAME: &str = "cli";
const CLI_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
struct RetryOperationHeaders {
    operation_id: String,
    idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GatewayCompatibilityStatus {
    gateway_version: String,
    supported_clients: Vec<GatewayCompatibilityRange>,
}

#[derive(Clone, Debug, Deserialize)]
struct GatewayCompatibilityRange {
    client_name: String,
    minimum_version: String,
    maximum_version_exclusive: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl ParsedVersion {
    fn parse(input: &str) -> Result<Self, CliError> {
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
            .ok_or_else(|| CliError::Usage(format!("invalid semantic version: {input}")))?
            .parse::<u64>()
            .map_err(|_| CliError::Usage(format!("invalid semantic version: {input}")))?;
        let minor = parts
            .next()
            .ok_or_else(|| CliError::Usage(format!("invalid semantic version: {input}")))?
            .parse::<u64>()
            .map_err(|_| CliError::Usage(format!("invalid semantic version: {input}")))?;
        let patch = parts
            .next()
            .ok_or_else(|| CliError::Usage(format!("invalid semantic version: {input}")))?
            .parse::<u64>()
            .map_err(|_| CliError::Usage(format!("invalid semantic version: {input}")))?;

        if parts.next().is_some() {
            return Err(CliError::Usage(format!(
                "invalid semantic version: {input}"
            )));
        }

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

/// HTTP client that talks to the ghost gateway API.
///
/// Reuses a single `reqwest::Client` instance across all requests (E.14).
pub struct GhostHttpClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl GhostHttpClient {
    /// Create a new client targeting the given gateway base URL.
    pub fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            token,
        }
    }

    /// GET request to an API path.
    pub async fn get(&self, path: &str) -> Result<Response, CliError> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }
        self.send_with_retry(req).await
    }

    /// POST request with a JSON body.
    pub async fn post<T: Serialize>(&self, path: &str, body: &T) -> Result<Response, CliError> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.post(&url).json(body);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }
        self.send_with_retry(req).await
    }

    /// PATCH request with a JSON body.
    pub async fn patch<T: Serialize>(&self, path: &str, body: &T) -> Result<Response, CliError> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.patch(&url).json(body);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }
        self.send_with_retry(req).await
    }

    /// DELETE request to an API path.
    pub async fn delete(&self, path: &str) -> Result<Response, CliError> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.delete(&url);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }
        self.send_with_retry(req).await
    }

    /// Health probe with a short timeout. Returns `true` if the gateway is reachable.
    pub async fn health_check(base_url: &str) -> bool {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap_or_default();
        client
            .get(format!("{base_url}/api/health"))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Verify that this CLI build is inside the gateway's supported version range.
    pub async fn assert_compatible(&self) -> Result<(), CliError> {
        let url = format!("{}/api/compatibility", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }

        let response = req
            .send()
            .await
            .map_err(|e| CliError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(CliError::Usage(format!(
                "gateway compatibility check failed with HTTP {}",
                response.status()
            )));
        }

        let status: GatewayCompatibilityStatus = response
            .json()
            .await
            .map_err(|e| CliError::Http(e.to_string()))?;

        let range = status
            .supported_clients
            .iter()
            .find(|range| range.client_name.eq_ignore_ascii_case(CLI_CLIENT_NAME))
            .ok_or_else(|| {
                CliError::Usage(format!(
                    "gateway {} does not advertise compatibility for cli clients",
                    status.gateway_version
                ))
            })?;

        let current = ParsedVersion::parse(CLI_CLIENT_VERSION)?;
        let minimum = ParsedVersion::parse(&range.minimum_version)?;
        let maximum = ParsedVersion::parse(&range.maximum_version_exclusive)?;

        if current < minimum || current >= maximum {
            return Err(CliError::Usage(format!(
                "cli {} is incompatible with gateway {} (supported range: {} <= cli < {})",
                CLI_CLIENT_VERSION,
                status.gateway_version,
                range.minimum_version,
                range.maximum_version_exclusive
            )));
        }

        Ok(())
    }

    /// Send a request with retry logic for transient errors (E.12).
    ///
    /// Retries up to 3 times on 429/502/503/504 with exponential backoff.
    /// Respects the `Retry-After` header when present.
    async fn send_with_retry(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<Response, CliError> {
        // Build the request once to inspect. For retries we need to clone,
        // but reqwest::RequestBuilder isn't Clone. So we try_clone the built Request.
        let request = request_builder
            .build()
            .map_err(|e| CliError::Http(e.to_string()))?;
        let retry_headers = Self::retry_operation_headers(request.method());

        let max_retries = 3u32;
        let mut last_err = None;

        for attempt in 0..=max_retries {
            let mut req = if attempt == 0 {
                request
                    .try_clone()
                    .unwrap_or_else(|| request.try_clone().unwrap())
            } else {
                match request.try_clone() {
                    Some(r) => r,
                    None => break, // Can't retry if body isn't cloneable
                }
            };

            Self::apply_request_identity(&mut req, retry_headers.as_ref())?;

            match self.client.execute(req).await {
                Ok(resp) => {
                    let status = resp.status();
                    if Self::is_retryable(status) && attempt < max_retries {
                        let delay = Self::retry_delay(&resp, attempt);
                        tokio::time::sleep(delay).await;
                        last_err = Some(format!("HTTP {status}"));
                        continue;
                    }
                    return Self::map_response(resp).await;
                }
                Err(e) => {
                    if attempt < max_retries && (e.is_connect() || e.is_timeout()) {
                        let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt));
                        tokio::time::sleep(delay).await;
                        last_err = Some(e.to_string());
                        continue;
                    }
                    return Err(CliError::Http(e.to_string()));
                }
            }
        }

        Err(CliError::Http(format!(
            "max retries exceeded: {}",
            last_err.unwrap_or_default()
        )))
    }

    fn retry_operation_headers(method: &reqwest::Method) -> Option<RetryOperationHeaders> {
        if !matches!(
            *method,
            reqwest::Method::POST
                | reqwest::Method::PUT
                | reqwest::Method::PATCH
                | reqwest::Method::DELETE
        ) {
            return None;
        }

        let operation_id = uuid::Uuid::now_v7().to_string();
        Some(RetryOperationHeaders {
            idempotency_key: operation_id.clone(),
            operation_id,
        })
    }

    fn apply_request_identity(
        request: &mut reqwest::Request,
        retry_headers: Option<&RetryOperationHeaders>,
    ) -> Result<(), CliError> {
        let request_id = uuid::Uuid::now_v7().to_string();
        let request_id = reqwest::header::HeaderValue::from_str(&request_id)
            .map_err(|e| CliError::Http(e.to_string()))?;
        request.headers_mut().insert(REQUEST_ID_HEADER, request_id);

        let client_name = reqwest::header::HeaderValue::from_static(CLI_CLIENT_NAME);
        request
            .headers_mut()
            .insert(CLIENT_NAME_HEADER, client_name);

        let client_version = reqwest::header::HeaderValue::from_static(CLI_CLIENT_VERSION);
        request
            .headers_mut()
            .insert(CLIENT_VERSION_HEADER, client_version);

        if let Some(retry_headers) = retry_headers {
            let operation_id = reqwest::header::HeaderValue::from_str(&retry_headers.operation_id)
                .map_err(|e| CliError::Http(e.to_string()))?;
            request
                .headers_mut()
                .insert(OPERATION_ID_HEADER, operation_id);

            let idempotency_key =
                reqwest::header::HeaderValue::from_str(&retry_headers.idempotency_key)
                    .map_err(|e| CliError::Http(e.to_string()))?;
            request
                .headers_mut()
                .insert(IDEMPOTENCY_KEY_HEADER, idempotency_key);
        }

        Ok(())
    }

    fn is_retryable(status: StatusCode) -> bool {
        matches!(
            status,
            StatusCode::TOO_MANY_REQUESTS
                | StatusCode::BAD_GATEWAY
                | StatusCode::SERVICE_UNAVAILABLE
                | StatusCode::GATEWAY_TIMEOUT
        )
    }

    fn retry_delay(resp: &Response, attempt: u32) -> std::time::Duration {
        // Respect Retry-After header if present.
        if let Some(retry_after) = resp.headers().get("retry-after") {
            if let Ok(secs) = retry_after.to_str().unwrap_or("").parse::<u64>() {
                return std::time::Duration::from_secs(secs);
            }
        }
        std::time::Duration::from_millis(500 * 2u64.pow(attempt))
    }

    async fn map_response(resp: Response) -> Result<Response, CliError> {
        let status = resp.status();
        if status.is_success() || status == StatusCode::ACCEPTED {
            return Ok(resp);
        }

        let request_id = resp
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| format!(" (request_id={s})"))
            .unwrap_or_default();

        match status {
            StatusCode::UNAUTHORIZED => Err(CliError::AuthRequired),
            StatusCode::NOT_FOUND => Err(CliError::NotFound(format!(
                "resource not found{request_id}"
            ))),
            StatusCode::CONFLICT => {
                let body = resp.text().await.unwrap_or_default();
                Err(CliError::Conflict(format!("{body}{request_id}")))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                Err(CliError::Http(format!("rate limited{request_id}")))
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(CliError::Internal(format!("server error{request_id}")))
            }
            _ => {
                let body = resp.text().await.unwrap_or_default();
                Err(CliError::Http(format!("HTTP {status}: {body}{request_id}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_value(request: &reqwest::Request, name: &str) -> Option<String> {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string())
    }

    #[test]
    fn mutating_requests_keep_operation_identity_across_attempts() {
        let base = reqwest::Client::new()
            .post("http://example.test/api/goals/123/approve")
            .body("{}")
            .build()
            .unwrap();
        let retry_headers = GhostHttpClient::retry_operation_headers(base.method()).unwrap();

        let mut first = base.try_clone().unwrap();
        GhostHttpClient::apply_request_identity(&mut first, Some(&retry_headers)).unwrap();

        let mut second = base.try_clone().unwrap();
        GhostHttpClient::apply_request_identity(&mut second, Some(&retry_headers)).unwrap();

        assert_ne!(
            header_value(&first, REQUEST_ID_HEADER),
            header_value(&second, REQUEST_ID_HEADER)
        );
        assert_eq!(
            header_value(&first, OPERATION_ID_HEADER),
            header_value(&second, OPERATION_ID_HEADER)
        );
        assert_eq!(
            header_value(&first, IDEMPOTENCY_KEY_HEADER),
            header_value(&second, IDEMPOTENCY_KEY_HEADER)
        );
    }

    #[test]
    fn read_requests_only_rotate_request_id() {
        let base = reqwest::Client::new()
            .get("http://example.test/api/health")
            .build()
            .unwrap();

        let mut request = base.try_clone().unwrap();
        GhostHttpClient::apply_request_identity(&mut request, None).unwrap();

        assert!(header_value(&request, REQUEST_ID_HEADER).is_some());
        assert_eq!(
            header_value(&request, CLIENT_NAME_HEADER),
            Some(CLI_CLIENT_NAME.to_string())
        );
        assert_eq!(
            header_value(&request, CLIENT_VERSION_HEADER),
            Some(CLI_CLIENT_VERSION.to_string())
        );
        assert!(header_value(&request, OPERATION_ID_HEADER).is_none());
        assert!(header_value(&request, IDEMPOTENCY_KEY_HEADER).is_none());
    }
}
