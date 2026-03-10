//! Google OAuth 2.0 provider.
//!
//! Endpoints:
//! - Auth: `https://accounts.google.com/o/oauth2/v2/auth`
//! - Token: `https://oauth2.googleapis.com/token`
//! - Revoke: `https://oauth2.googleapis.com/revoke`
//!
//! Default scopes: gmail.readonly, calendar, drive.readonly.
//! Supports refresh tokens (offline access).

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Utc;
use secrecy::SecretString;

use crate::error::OAuthError;
use crate::provider::OAuthProvider;
use crate::types::{ApiRequest, ApiResponse, PkceChallenge, TokenSet};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";

pub struct GoogleOAuthProvider {
    client_id: String,
    client_secret: SecretString,
    http: &'static reqwest::blocking::Client,
}

impl GoogleOAuthProvider {
    pub fn new(client_id: String, client_secret: SecretString) -> Result<Self, OAuthError> {
        let http = Box::leak(Box::new(
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|e| OAuthError::ProviderError(format!("HTTP client init: {e}")))?,
        ));
        Ok(Self {
            client_id,
            client_secret,
            http,
        })
    }
}

impl OAuthProvider for GoogleOAuthProvider {
    fn name(&self) -> &str {
        "google"
    }

    fn authorization_url(
        &self,
        scopes: &[String],
        state: &str,
        redirect_uri: &str,
    ) -> Result<(String, PkceChallenge), OAuthError> {
        let pkce = PkceChallenge::generate();
        let scope_str = if scopes.is_empty() {
            "openid".to_string()
        } else {
            scopes.join(" ")
        };

        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent",
            AUTH_URL,
            urlencod(&self.client_id),
            urlencod(redirect_uri),
            urlencod(&scope_str),
            urlencod(state),
            urlencod(&pkce.code_challenge),
        );
        Ok((url, pkce))
    }

    fn exchange_code(
        &self,
        code: &str,
        pkce_verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, OAuthError> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &self.client_id),
            (
                "client_secret",
                secrecy::ExposeSecret::expose_secret(&self.client_secret),
            ),
            ("code_verifier", pkce_verifier),
        ];

        let resp = self
            .http
            .post(TOKEN_URL)
            .form(&params)
            .send()
            .map_err(|e| OAuthError::FlowFailed(format!("token exchange: {e}")))?;

        parse_token_response(resp)
    }

    fn refresh_token(&self, refresh_token: &str) -> Result<TokenSet, OAuthError> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
            (
                "client_secret",
                secrecy::ExposeSecret::expose_secret(&self.client_secret),
            ),
        ];

        let resp = self
            .http
            .post(TOKEN_URL)
            .form(&params)
            .send()
            .map_err(|e| OAuthError::RefreshFailed(format!("refresh: {e}")))?;

        parse_token_response(resp)
    }

    fn revoke_token(&self, token: &str) -> Result<(), OAuthError> {
        let resp = self
            .http
            .post(REVOKE_URL)
            .form(&[("token", token)])
            .send()
            .map_err(|e| OAuthError::ProviderError(format!("revoke: {e}")))?;

        if resp.status().is_success() || resp.status().as_u16() == 400 {
            // Google returns 400 if token already revoked — treat as success
            Ok(())
        } else {
            Err(OAuthError::ProviderError(format!(
                "revoke returned HTTP {}",
                resp.status()
            )))
        }
    }

    fn execute_api_call(
        &self,
        access_token: &str,
        request: &ApiRequest,
    ) -> Result<ApiResponse, OAuthError> {
        execute_bearer_request(self.http, access_token, request)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers (used by all providers in this module)
// ---------------------------------------------------------------------------

/// Minimal URL encoding for query parameters.
pub(crate) fn urlencod(s: &str) -> String {
    s.replace('&', "%26")
        .replace('=', "%3D")
        .replace(' ', "%20")
        .replace('+', "%2B")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('/', "%2F")
}

/// Parse a standard OAuth 2.0 token response JSON.
pub(crate) fn parse_token_response(
    resp: reqwest::blocking::Response,
) -> Result<TokenSet, OAuthError> {
    let status = resp.status();
    let body = resp
        .text()
        .map_err(|e| OAuthError::FlowFailed(format!("failed to read response body: {e}")))?;

    if !status.is_success() {
        return Err(OAuthError::FlowFailed(format!("HTTP {status}: {body}")));
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| OAuthError::FlowFailed(format!("malformed JSON: {e}")))?;

    let access_token = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OAuthError::FlowFailed("missing access_token".into()))?;

    let refresh_token = json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| SecretString::from(s.to_string()));

    let expires_in = json
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);

    let scope_str = json.get("scope").and_then(|v| v.as_str()).unwrap_or("");
    let scopes: Vec<String> = scope_str
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    Ok(TokenSet {
        access_token: SecretString::from(access_token.to_string()),
        refresh_token,
        expires_at: Utc::now() + chrono::Duration::seconds(expires_in as i64),
        scopes,
    })
}

/// Execute an HTTP request with Bearer token injection.
pub(crate) fn execute_bearer_request(
    http: &reqwest::blocking::Client,
    access_token: &str,
    request: &ApiRequest,
) -> Result<ApiResponse, OAuthError> {
    let method = request.method.to_uppercase();
    let mut builder = match method.as_str() {
        "GET" => http.get(&request.url),
        "POST" => http.post(&request.url),
        "PUT" => http.put(&request.url),
        "DELETE" => http.delete(&request.url),
        "PATCH" => http.patch(&request.url),
        other => {
            return Err(OAuthError::ProviderError(format!(
                "unsupported method: {other}"
            )))
        }
    };

    builder = builder.header("Authorization", format!("Bearer {access_token}"));

    for (k, v) in &request.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }

    if let Some(ref body) = request.body {
        builder = builder.body(body.clone());
    }

    let resp = builder
        .send()
        .map_err(|e| OAuthError::ProviderError(format!("API call failed: {e}")))?;

    let status = resp.status().as_u16();
    let headers: BTreeMap<String, String> = resp
        .headers()
        .iter()
        .map(
            |(k, v): (&reqwest::header::HeaderName, &reqwest::header::HeaderValue)| {
                (k.to_string(), v.to_str().unwrap_or("").to_string())
            },
        )
        .collect();
    let body = resp
        .text()
        .map_err(|e| OAuthError::ProviderError(format!("failed to read API response body: {e}")))?;

    Ok(ApiResponse {
        status,
        headers,
        body,
    })
}
