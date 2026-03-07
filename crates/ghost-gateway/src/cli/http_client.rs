//! HTTP client for CLI → gateway communication (Task 6.6 — §5.4, E.12, E.14).

use reqwest::{Response, StatusCode};
use serde::Serialize;

use super::error::CliError;

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

        let max_retries = 3u32;
        let mut last_err = None;

        for attempt in 0..=max_retries {
            let req = if attempt == 0 {
                request
                    .try_clone()
                    .unwrap_or_else(|| request.try_clone().unwrap())
            } else {
                match request.try_clone() {
                    Some(r) => r,
                    None => break, // Can't retry if body isn't cloneable
                }
            };

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
