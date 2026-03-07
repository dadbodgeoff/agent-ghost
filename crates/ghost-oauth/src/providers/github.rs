//! GitHub OAuth 2.0 provider.
//!
//! Endpoints:
//! - Auth: `https://github.com/login/oauth/authorize`
//! - Token: `https://github.com/login/oauth/access_token`
//! - Revoke: `https://api.github.com/applications/{client_id}/token`
//!
//! Quirks: GitHub uses non-standard token exchange (requires `Accept: application/json`).
//! GitHub tokens are long-lived — no refresh tokens.

use std::time::Duration;

use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};

use crate::error::OAuthError;
use crate::provider::OAuthProvider;
use crate::types::{ApiRequest, ApiResponse, PkceChallenge, TokenSet};

use super::google::{execute_bearer_request, urlencod};

const AUTH_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

pub struct GitHubOAuthProvider {
    client_id: String,
    client_secret: SecretString,
    http: reqwest::blocking::Client,
}

impl GitHubOAuthProvider {
    pub fn new(client_id: String, client_secret: SecretString) -> Result<Self, OAuthError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| OAuthError::ProviderError(format!("HTTP client init: {e}")))?;
        Ok(Self {
            client_id,
            client_secret,
            http,
        })
    }
}

impl OAuthProvider for GitHubOAuthProvider {
    fn name(&self) -> &str {
        "github"
    }

    fn authorization_url(
        &self,
        scopes: &[String],
        state: &str,
        redirect_uri: &str,
    ) -> Result<(String, PkceChallenge), OAuthError> {
        let pkce = PkceChallenge::generate();
        let scope_str = scopes.join(" ");

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
        // GitHub requires Accept: application/json for JSON response
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &self.client_id),
            ("client_secret", self.client_secret.expose_secret()),
            ("code_verifier", pkce_verifier),
        ];

        let resp = self
            .http
            .post(TOKEN_URL)
            .header("Accept", "application/json")
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

        // GitHub may return error in JSON body even with 200
        if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
            let desc = json
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return Err(OAuthError::FlowFailed(format!("{err}: {desc}")));
        }

        let access_token = json
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OAuthError::FlowFailed("missing access_token".into()))?;

        let scope_str = json.get("scope").and_then(|v| v.as_str()).unwrap_or("");
        let scopes: Vec<String> = scope_str
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // GitHub tokens are long-lived — set a far-future expiry
        Ok(TokenSet {
            access_token: SecretString::from(access_token.to_string()),
            refresh_token: None, // GitHub doesn't use refresh tokens
            expires_at: Utc::now() + chrono::Duration::days(365),
            scopes,
        })
    }

    fn refresh_token(&self, _refresh_token: &str) -> Result<TokenSet, OAuthError> {
        // GitHub tokens are long-lived — no refresh flow
        Err(OAuthError::RefreshFailed(
            "GitHub does not support token refresh — tokens are long-lived".into(),
        ))
    }

    fn revoke_token(&self, token: &str) -> Result<(), OAuthError> {
        let url = format!(
            "https://api.github.com/applications/{}/token",
            self.client_id
        );

        let body = serde_json::json!({ "access_token": token });

        let resp = self
            .http
            .delete(&url)
            .basic_auth(&self.client_id, Some(self.client_secret.expose_secret()))
            .json(&body)
            .send()
            .map_err(|e| OAuthError::ProviderError(format!("revoke: {e}")))?;

        if resp.status().is_success() || resp.status().as_u16() == 404 {
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
        execute_bearer_request(&self.http, access_token, request)
    }
}
