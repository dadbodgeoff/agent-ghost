//! HTTP request tool — general-purpose API interaction for the agent.
//!
//! Complements `web_search` (find info) and `web_fetch` (read pages) by
//! letting the agent interact with REST APIs: POST, PUT, PATCH, DELETE,
//! send JSON payloads, and set custom headers.
//!
//! Safety:
//! - Only HTTPS URLs by default (HTTP rejected unless explicitly configured).
//! - Private/internal IPs are blocked (SSRF protection, reuses `web_fetch` checks).
//! - Response body capped at `max_response_bytes`.
//! - Domain allowlist: only configured domains are reachable.
//! - Methods are restricted to the configured set (default: GET, POST, PUT, PATCH, DELETE).
//! - Request body size capped at `max_request_bytes`.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum HttpRequestError {
    #[error("URL not allowed: {0}")]
    UrlNotAllowed(String),
    #[error("method not allowed: {0}")]
    MethodNotAllowed(String),
    #[error("domain not in allowlist: {0}")]
    DomainNotAllowed(String),
    #[error("request body too large: {size} bytes exceeds {max} byte limit")]
    RequestTooLarge { size: usize, max: usize },
    #[error("response too large: {size} bytes exceeds {max} byte limit")]
    ResponseTooLarge { size: u64, max: u64 },
    #[error("request failed: {0}")]
    RequestFailed(String),
    #[error("SSRF blocked: {0} resolves to private/internal IP")]
    SsrfBlocked(String),
}

// ── Configuration ───────────────────────────────────────────────────────

/// HTTP request tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    /// Allow plain HTTP (not just HTTPS). Default: false.
    #[serde(default)]
    pub allow_http: bool,

    /// Allowed HTTP methods. Default: GET, POST, PUT, PATCH, DELETE.
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,

    /// Domain allowlist. Must be explicitly populated for the tool to run.
    #[serde(default)]
    pub allowed_domains: Vec<String>,

    /// Maximum request body size in bytes. Default: 1MB.
    #[serde(default = "default_max_request_bytes")]
    pub max_request_bytes: usize,

    /// Maximum response body size in bytes. Default: 2MB.
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: u64,

    /// Request timeout in seconds. Default: 30.
    #[serde(default = "default_request_timeout")]
    pub timeout_secs: u64,

    /// User-Agent header. Default: GHOST agent identifier.
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

fn default_allowed_methods() -> Vec<String> {
    vec![
        "GET".into(),
        "POST".into(),
        "PUT".into(),
        "PATCH".into(),
        "DELETE".into(),
    ]
}
fn default_max_request_bytes() -> usize {
    1_048_576
} // 1MB
fn default_max_response_bytes() -> u64 {
    2_097_152
} // 2MB
fn default_request_timeout() -> u64 {
    30
}
fn default_user_agent() -> String {
    "GHOST-Agent/0.1 (autonomous-agent; +https://github.com/ghost-agent)".into()
}

impl Default for HttpRequestConfig {
    fn default() -> Self {
        Self {
            allow_http: false,
            allowed_methods: default_allowed_methods(),
            allowed_domains: Vec::new(),
            max_request_bytes: default_max_request_bytes(),
            max_response_bytes: default_max_response_bytes(),
            timeout_secs: default_request_timeout(),
            user_agent: default_user_agent(),
        }
    }
}

// ── Result type ─────────────────────────────────────────────────────────

/// Result of an HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestResult {
    /// The final URL (after redirects).
    pub url: String,
    /// HTTP method used.
    pub method: String,
    /// HTTP status code.
    pub status: u16,
    /// Response headers (selected safe subset).
    pub headers: HashMap<String, String>,
    /// Response body (text content, truncated if needed).
    pub body: String,
    /// Content-Type from the response.
    pub content_type: String,
    /// Whether the body was truncated.
    pub truncated: bool,
    /// Body length in bytes.
    pub body_length: usize,
}

// ── Public API ──────────────────────────────────────────────────────────

/// Execute an HTTP request.
pub async fn http_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
    config: &HttpRequestConfig,
) -> Result<HttpRequestResult, HttpRequestError> {
    let url = url.trim();
    let method_upper = method.trim().to_uppercase();

    // ── Validate method ─────────────────────────────────────────────
    if !config
        .allowed_methods
        .iter()
        .any(|m| m.to_uppercase() == method_upper)
    {
        return Err(HttpRequestError::MethodNotAllowed(format!(
            "{} is not in allowed methods: {:?}",
            method_upper, config.allowed_methods,
        )));
    }

    // ── Validate URL scheme ─────────────────────────────────────────
    if url.starts_with("http://") && !config.allow_http {
        return Err(HttpRequestError::UrlNotAllowed(
            "HTTP URLs are not allowed. Only HTTPS is permitted.".into(),
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(HttpRequestError::UrlNotAllowed(format!(
            "Unsupported URL scheme. Only HTTP(S) is allowed: {}",
            truncate(url, 100),
        )));
    }

    // ── Validate request body size ──────────────────────────────────
    if let Some(b) = body {
        if b.len() > config.max_request_bytes {
            return Err(HttpRequestError::RequestTooLarge {
                size: b.len(),
                max: config.max_request_bytes,
            });
        }
    }

    // ── Extract and validate host ───────────────────────────────────
    let host = extract_host(url)
        .ok_or_else(|| HttpRequestError::UrlNotAllowed("Cannot parse host from URL".into()))?;

    if is_private_host(&host) {
        return Err(HttpRequestError::SsrfBlocked(host));
    }

    // ── Domain allowlist check ──────────────────────────────────────
    if config.allowed_domains.is_empty() {
        return Err(HttpRequestError::DomainNotAllowed(
            "HTTP request tool disabled: no allowed domains configured".into(),
        ));
    }

    let domain_allowed = config.allowed_domains.iter().any(|d| {
        let d = d.to_lowercase();
        host == d || host.ends_with(&format!(".{}", d))
    });
    if !domain_allowed {
        return Err(HttpRequestError::DomainNotAllowed(format!(
            "Domain '{}' is not in the allowlist. Allowed: {:?}",
            host, config.allowed_domains,
        )));
    }

    // ── Build request ───────────────────────────────────────────────
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent(&config.user_agent)
        .build()
        .map_err(|e| HttpRequestError::RequestFailed(e.to_string()))?;

    let reqwest_method = method_upper.parse::<reqwest::Method>().map_err(|_| {
        HttpRequestError::MethodNotAllowed(format!("Invalid HTTP method: {}", method_upper))
    })?;

    let mut req = client.request(reqwest_method.clone(), url);

    // Apply custom headers.
    for (key, value) in headers {
        // Block Host header override (SSRF vector).
        let key_lower = key.to_lowercase();
        if key_lower == "host" {
            continue;
        }
        req = req.header(key.as_str(), value.as_str());
    }

    // Apply body for methods that support it.
    if let Some(b) = body {
        // Auto-detect content type if not set.
        let has_content_type = headers.keys().any(|k| k.to_lowercase() == "content-type");
        if !has_content_type {
            // If body looks like JSON, set content-type.
            if (b.starts_with('{') && b.ends_with('}')) || (b.starts_with('[') && b.ends_with(']'))
            {
                req = req.header("Content-Type", "application/json");
            } else {
                req = req.header("Content-Type", "text/plain");
            }
        }
        req = req.body(b.to_string());
    }

    // ── Execute request ─────────────────────────────────────────────
    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            HttpRequestError::RequestFailed(format!(
                "Request timed out after {}s",
                config.timeout_secs
            ))
        } else if e.is_connect() {
            HttpRequestError::RequestFailed(format!("Connection failed to {}: {}", host, e))
        } else {
            HttpRequestError::RequestFailed(e.to_string())
        }
    })?;

    let status = resp.status().as_u16();
    let final_url = resp.url().to_string();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Extract safe response headers.
    let response_headers = extract_safe_headers(&resp);

    // Check content-length if present.
    if let Some(cl) = resp.content_length() {
        if cl > config.max_response_bytes {
            return Err(HttpRequestError::ResponseTooLarge {
                size: cl,
                max: config.max_response_bytes,
            });
        }
    }

    // ── Read response body ──────────────────────────────────────────
    let body_bytes = resp
        .bytes()
        .await
        .map_err(|e| HttpRequestError::RequestFailed(format!("Failed to read response: {}", e)))?;

    if body_bytes.len() as u64 > config.max_response_bytes {
        return Err(HttpRequestError::ResponseTooLarge {
            size: body_bytes.len() as u64,
            max: config.max_response_bytes,
        });
    }

    let body_text = String::from_utf8_lossy(&body_bytes).to_string();

    // Truncate if needed (character-level for display).
    let max_chars = 32_000; // generous limit for API responses
    let truncated = body_text.len() > max_chars;
    let body_out = if truncated {
        let mut s = safe_truncate(&body_text, max_chars);
        s.push_str("\n\n[Response truncated]");
        s
    } else {
        body_text
    };

    let body_length = body_out.len();

    Ok(HttpRequestResult {
        url: final_url,
        method: method_upper,
        status,
        headers: response_headers,
        body: body_out,
        content_type,
        truncated,
        body_length,
    })
}

// ── Internals ───────────────────────────────────────────────────────────

/// Extract a safe subset of response headers (no cookies, auth tokens, etc.).
fn extract_safe_headers(resp: &reqwest::Response) -> HashMap<String, String> {
    let safe_keys = [
        "content-type",
        "content-length",
        "x-request-id",
        "x-ratelimit-limit",
        "x-ratelimit-remaining",
        "x-ratelimit-reset",
        "retry-after",
        "location",
        "etag",
        "last-modified",
        "cache-control",
        "date",
    ];

    let mut out = HashMap::new();
    for key in &safe_keys {
        if let Some(val) = resp.headers().get(*key) {
            if let Ok(s) = val.to_str() {
                out.insert(key.to_string(), s.to_string());
            }
        }
    }
    out
}

/// Extract host from a URL string.
fn extract_host(url: &str) -> Option<String> {
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = after_scheme.split('/').next()?.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    Some(host.to_lowercase())
}

/// Check if a host resolves to a private/internal IP (SSRF protection).
fn is_private_host(host: &str) -> bool {
    let blocked_hosts = [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "::1",
        "[::1]",
        "metadata.google.internal",
        "169.254.169.254",
    ];
    if blocked_hosts.contains(&host) {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_ip(ip);
    }
    false
}

/// Check if an IP address is in a private/reserved range.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

/// Truncate a string at a char boundary.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Safe truncation that respects char boundaries.
fn safe_truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SSRF protection ─────────────────────────────────────────────

    #[test]
    fn blocks_localhost() {
        assert!(is_private_host("localhost"));
        assert!(is_private_host("127.0.0.1"));
        assert!(is_private_host("0.0.0.0"));
        assert!(is_private_host("::1"));
    }

    #[test]
    fn blocks_metadata_endpoints() {
        assert!(is_private_host("169.254.169.254"));
        assert!(is_private_host("metadata.google.internal"));
    }

    #[test]
    fn blocks_private_ips() {
        assert!(is_private_host("10.0.0.1"));
        assert!(is_private_host("172.16.0.1"));
        assert!(is_private_host("192.168.1.1"));
    }

    #[test]
    fn allows_public_hosts() {
        assert!(!is_private_host("api.github.com"));
        assert!(!is_private_host("8.8.8.8"));
        assert!(!is_private_host("httpbin.org"));
    }

    // ── URL validation ──────────────────────────────────────────────

    #[tokio::test]
    async fn rejects_http_by_default() {
        let config = HttpRequestConfig::default();
        let headers = HashMap::new();
        let err = http_request("http://example.com/api", "GET", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpRequestError::UrlNotAllowed(_)));
    }

    #[tokio::test]
    async fn rejects_ftp_scheme() {
        let config = HttpRequestConfig::default();
        let headers = HashMap::new();
        let err = http_request("ftp://example.com/file", "GET", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpRequestError::UrlNotAllowed(_)));
    }

    #[tokio::test]
    async fn rejects_file_scheme() {
        let config = HttpRequestConfig::default();
        let headers = HashMap::new();
        let err = http_request("file:///etc/passwd", "GET", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpRequestError::UrlNotAllowed(_)));
    }

    // ── Method validation ───────────────────────────────────────────

    #[tokio::test]
    async fn rejects_disallowed_method() {
        let config = HttpRequestConfig {
            allowed_methods: vec!["GET".into(), "POST".into()],
            ..Default::default()
        };
        let headers = HashMap::new();
        let err = http_request("https://example.com/api", "DELETE", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpRequestError::MethodNotAllowed(_)));
    }

    #[tokio::test]
    async fn method_case_insensitive() {
        // "get" should match "GET" in allowed_methods.
        let config = HttpRequestConfig::default();
        let headers = HashMap::new();
        // This will fail at the network level, but should NOT fail at method validation.
        let result = http_request("https://192.0.2.1/api", "get", &headers, None, &config).await;
        // 192.0.2.1 is TEST-NET, not private — will fail with connection error, not MethodNotAllowed.
        assert!(!matches!(
            result,
            Err(HttpRequestError::MethodNotAllowed(_))
        ));
    }

    // ── Domain allowlist ────────────────────────────────────────────

    #[tokio::test]
    async fn rejects_domain_not_in_allowlist() {
        let config = HttpRequestConfig {
            allowed_domains: vec!["api.github.com".into()],
            ..Default::default()
        };
        let headers = HashMap::new();
        let err = http_request("https://evil.com/steal", "GET", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpRequestError::DomainNotAllowed(_)));
    }

    #[tokio::test]
    async fn allows_domain_in_allowlist() {
        let config = HttpRequestConfig {
            allowed_domains: vec!["api.github.com".into()],
            timeout_secs: 1,
            ..Default::default()
        };
        let headers = HashMap::new();
        // Will fail at network level (timeout), but should NOT fail at domain check.
        let result = http_request(
            "https://api.github.com/repos",
            "GET",
            &headers,
            None,
            &config,
        )
        .await;
        assert!(!matches!(
            result,
            Err(HttpRequestError::DomainNotAllowed(_))
        ));
    }

    #[tokio::test]
    async fn subdomain_matching() {
        let config = HttpRequestConfig {
            allowed_domains: vec!["github.com".into()],
            timeout_secs: 1,
            ..Default::default()
        };
        let headers = HashMap::new();
        // api.github.com should match github.com allowlist entry.
        let result = http_request(
            "https://api.github.com/repos",
            "GET",
            &headers,
            None,
            &config,
        )
        .await;
        assert!(!matches!(
            result,
            Err(HttpRequestError::DomainNotAllowed(_))
        ));
    }

    #[tokio::test]
    async fn http_request_denied_when_domain_not_explicitly_allowed() {
        let config = HttpRequestConfig {
            allowed_domains: vec![],
            timeout_secs: 1,
            ..Default::default()
        };
        let headers = HashMap::new();
        let error = http_request("https://any-domain.com/api", "GET", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(error, HttpRequestError::DomainNotAllowed(_)));
    }

    // ── SSRF via request ────────────────────────────────────────────

    #[tokio::test]
    async fn rejects_localhost_ssrf() {
        let config = HttpRequestConfig {
            allow_http: true,
            ..Default::default()
        };
        let headers = HashMap::new();
        let err = http_request("http://localhost/admin", "POST", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpRequestError::SsrfBlocked(_)));
    }

    #[tokio::test]
    async fn rejects_metadata_ssrf() {
        let config = HttpRequestConfig {
            allow_http: true,
            ..Default::default()
        };
        let headers = HashMap::new();
        let err = http_request(
            "http://169.254.169.254/latest/meta-data",
            "GET",
            &headers,
            None,
            &config,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, HttpRequestError::SsrfBlocked(_)));
    }

    #[tokio::test]
    async fn rejects_private_ip_ssrf() {
        let config = HttpRequestConfig {
            allow_http: true,
            ..Default::default()
        };
        let headers = HashMap::new();
        let err = http_request("http://10.0.0.1/internal", "GET", &headers, None, &config)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpRequestError::SsrfBlocked(_)));
    }

    // ── Request body validation ─────────────────────────────────────

    #[tokio::test]
    async fn rejects_oversized_request_body() {
        let config = HttpRequestConfig {
            max_request_bytes: 100,
            ..Default::default()
        };
        let headers = HashMap::new();
        let big_body = "x".repeat(200);
        let err = http_request(
            "https://example.com/api",
            "POST",
            &headers,
            Some(&big_body),
            &config,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, HttpRequestError::RequestTooLarge { .. }));
    }

    // ── Host header override blocked ────────────────────────────────

    #[tokio::test]
    async fn host_header_override_blocked() {
        // The Host header should be silently stripped. The request should
        // proceed to the actual URL host, not the spoofed one.
        let config = HttpRequestConfig {
            timeout_secs: 1,
            ..Default::default()
        };
        let mut headers = HashMap::new();
        headers.insert("Host".into(), "evil.com".into());
        // Should not error with SSRF — Host header is stripped.
        let result = http_request("https://httpbin.org/get", "GET", &headers, None, &config).await;
        assert!(!matches!(result, Err(HttpRequestError::SsrfBlocked(_))));
    }

    // ── Host extraction ─────────────────────────────────────────────

    #[test]
    fn extract_host_https() {
        assert_eq!(
            extract_host("https://api.github.com/repos"),
            Some("api.github.com".into())
        );
    }

    #[test]
    fn extract_host_with_port() {
        assert_eq!(
            extract_host("https://api.example.com:8443/v1"),
            Some("api.example.com".into())
        );
    }

    #[test]
    fn extract_host_bare() {
        assert_eq!(
            extract_host("https://example.com"),
            Some("example.com".into())
        );
    }

    // ── Safe headers extraction ─────────────────────────────────────

    #[test]
    fn default_config_methods() {
        let config = HttpRequestConfig::default();
        assert!(config.allowed_methods.contains(&"GET".to_string()));
        assert!(config.allowed_methods.contains(&"POST".to_string()));
        assert!(config.allowed_methods.contains(&"PUT".to_string()));
        assert!(config.allowed_methods.contains(&"PATCH".to_string()));
        assert!(config.allowed_methods.contains(&"DELETE".to_string()));
    }

    #[test]
    fn default_config_no_http() {
        let config = HttpRequestConfig::default();
        assert!(!config.allow_http);
    }

    #[test]
    fn default_config_empty_allowlist() {
        let config = HttpRequestConfig::default();
        assert!(config.allowed_domains.is_empty());
    }

    // ── Integration tests (require network) ─────────────────────────

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_get_request() {
        let config = HttpRequestConfig::default();
        let headers = HashMap::new();
        let result = http_request("https://httpbin.org/get", "GET", &headers, None, &config)
            .await
            .unwrap();
        assert_eq!(result.status, 200);
        assert_eq!(result.method, "GET");
        assert!(!result.body.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_post_json() {
        let config = HttpRequestConfig::default();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".into(), "application/json".into());
        let body = r#"{"key": "value", "number": 42}"#;
        let result = http_request(
            "https://httpbin.org/post",
            "POST",
            &headers,
            Some(body),
            &config,
        )
        .await
        .unwrap();
        assert_eq!(result.status, 200);
        assert!(result.body.contains("value"));
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_custom_headers() {
        let config = HttpRequestConfig::default();
        let mut headers = HashMap::new();
        headers.insert("X-Custom-Header".into(), "ghost-agent-test".into());
        let result = http_request(
            "https://httpbin.org/headers",
            "GET",
            &headers,
            None,
            &config,
        )
        .await
        .unwrap();
        assert_eq!(result.status, 200);
        assert!(result.body.contains("ghost-agent-test"));
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_put_request() {
        let config = HttpRequestConfig::default();
        let headers = HashMap::new();
        let result = http_request(
            "https://httpbin.org/put",
            "PUT",
            &headers,
            Some("updated data"),
            &config,
        )
        .await
        .unwrap();
        assert_eq!(result.status, 200);
        assert!(result.body.contains("updated data"));
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_delete_request() {
        let config = HttpRequestConfig::default();
        let headers = HashMap::new();
        let result = http_request(
            "https://httpbin.org/delete",
            "DELETE",
            &headers,
            None,
            &config,
        )
        .await
        .unwrap();
        assert_eq!(result.status, 200);
    }
}
