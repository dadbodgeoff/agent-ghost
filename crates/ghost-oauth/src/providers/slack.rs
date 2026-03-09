//! Slack OAuth 2.0 provider.
//!
//! Endpoints:
//! - Auth: `https://slack.com/oauth/v2/authorize`
//! - Token: `https://slack.com/api/oauth.v2.access`
//!
//! Quirks: Slack uses `xoxb-` bot tokens. Token response has a different
//! structure (`authed_user.access_token` vs top-level `access_token`).

use std::time::Duration;

use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};

use crate::error::OAuthError;
use crate::provider::OAuthProvider;
use crate::types::{ApiRequest, ApiResponse, PkceChallenge, TokenSet};

use super::google::{execute_bearer_request, urlencod};

const AUTH_URL: &str = "https://slack.com/oauth/v2/authorize";
const TOKEN_URL: &str = "https://slack.com/api/oauth.v2.access";

pub struct SlackOAuthProvider {
    client_id: String,
    client_secret: SecretString,
    http: &'static reqwest::blocking::Client,
}

impl SlackOAuthProvider {
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

    /// Validate that a token looks like a Slack bot token.
    pub fn validate_token_prefix(token: &str) -> bool {
        token.starts_with("xoxb-") || token.starts_with("xoxp-")
    }
}

impl OAuthProvider for SlackOAuthProvider {
    fn name(&self) -> &str {
        "slack"
    }

    fn authorization_url(
        &self,
        scopes: &[String],
        state: &str,
        redirect_uri: &str,
    ) -> Result<(String, PkceChallenge), OAuthError> {
        let pkce = PkceChallenge::generate();
        let scope_str = scopes.join(",");

        let url = format!(
            "{}?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
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
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &self.client_id),
            ("client_secret", self.client_secret.expose_secret()),
            ("code_verifier", pkce_verifier),
        ];

        let resp = self
            .http
            .post(TOKEN_URL)
            .form(&params)
            .send()
            .map_err(|e| OAuthError::FlowFailed(format!("token exchange: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .map_err(|e| OAuthError::FlowFailed(format!("failed to read response body: {e}")))?;

        if !status.is_success() {
            return Err(OAuthError::FlowFailed(format!("HTTP {status}: {body}")));
        }

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| OAuthError::FlowFailed(format!("malformed JSON: {e}")))?;

        // Slack wraps errors in {"ok": false, "error": "..."}
        if json.get("ok").and_then(|v| v.as_bool()) == Some(false) {
            let err = json
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            return Err(OAuthError::FlowFailed(format!("Slack error: {err}")));
        }

        // Bot token is at top level
        let access_token = json
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OAuthError::FlowFailed("missing access_token".into()))?;

        let refresh_token = json
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| SecretString::from(s.to_string()));

        let scope_str = json.get("scope").and_then(|v| v.as_str()).unwrap_or("");
        let scopes: Vec<String> = scope_str
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Slack bot tokens don't expire by default, but rotation tokens do
        let expires_in = json
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(86400 * 365); // Default 1 year if not specified

        Ok(TokenSet {
            access_token: SecretString::from(access_token.to_string()),
            refresh_token,
            expires_at: Utc::now() + chrono::Duration::seconds(expires_in as i64),
            scopes,
        })
    }

    fn refresh_token(&self, refresh_token: &str) -> Result<TokenSet, OAuthError> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", self.client_secret.expose_secret()),
        ];

        let resp = self
            .http
            .post(TOKEN_URL)
            .form(&params)
            .send()
            .map_err(|e| OAuthError::RefreshFailed(format!("refresh: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .map_err(|e| OAuthError::RefreshFailed(format!("failed to read response body: {e}")))?;

        if !status.is_success() {
            return Err(OAuthError::RefreshFailed(format!("HTTP {status}: {body}")));
        }

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| OAuthError::RefreshFailed(format!("malformed JSON: {e}")))?;

        if json.get("ok").and_then(|v| v.as_bool()) == Some(false) {
            let err = json
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            return Err(OAuthError::RefreshFailed(format!("Slack error: {err}")));
        }

        let access_token = json
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OAuthError::RefreshFailed("missing access_token".into()))?;

        let new_refresh = json
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| SecretString::from(s.to_string()));

        let expires_in = json
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(43200);

        let scope_str = json.get("scope").and_then(|v| v.as_str()).unwrap_or("");
        let scopes: Vec<String> = scope_str
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        Ok(TokenSet {
            access_token: SecretString::from(access_token.to_string()),
            refresh_token: new_refresh,
            expires_at: Utc::now() + chrono::Duration::seconds(expires_in as i64),
            scopes,
        })
    }

    fn revoke_token(&self, token: &str) -> Result<(), OAuthError> {
        // Slack doesn't have a standard revocation endpoint for bot tokens.
        // The app can be uninstalled via api.slack.com/apps, but there's no
        // programmatic single-token revoke. We log and treat as success.
        tracing::info!(
            token_prefix = &token[..5.min(token.len())],
            "Slack token revocation requested (no-op — uninstall app to fully revoke)"
        );
        Ok(())
    }

    fn execute_api_call(
        &self,
        access_token: &str,
        request: &ApiRequest,
    ) -> Result<ApiResponse, OAuthError> {
        execute_bearer_request(&self.http, access_token, request)
    }
}
