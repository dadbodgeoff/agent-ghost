//! Configurable OAuth 2.0 provider for standard authorization-code flows.
//!
//! This is primarily used by the gateway when providers are declared in
//! `ghost.yml`, and by local live-audit mocks that expose OAuth-compatible
//! authorize/token/revoke endpoints.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};

use crate::error::OAuthError;
use crate::provider::OAuthProvider;
use crate::types::{ApiRequest, ApiResponse, PkceChallenge, TokenSet};

use super::google::{execute_bearer_request, parse_token_response, urlencod};

pub struct ConfigurableOAuthProvider {
    name: String,
    client_id: String,
    client_secret: SecretString,
    auth_url: String,
    token_url: String,
    revoke_url: Option<String>,
    http: &'static reqwest::blocking::Client,
}

impl ConfigurableOAuthProvider {
    pub fn new(
        name: String,
        client_id: String,
        client_secret: SecretString,
        auth_url: String,
        token_url: String,
        revoke_url: Option<String>,
    ) -> Result<Self, OAuthError> {
        // `reqwest::blocking::Client` owns a Tokio runtime internally. Leaking the
        // client avoids dropping that runtime inside the gateway's async shutdown.
        let http = Box::leak(Box::new(
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|e| OAuthError::ProviderError(format!("HTTP client init: {e}")))?,
        ));
        Ok(Self {
            name,
            client_id,
            client_secret,
            auth_url,
            token_url,
            revoke_url,
            http,
        })
    }
}

impl OAuthProvider for ConfigurableOAuthProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn authorization_url(
        &self,
        scopes: &[String],
        state: &str,
        redirect_uri: &str,
    ) -> Result<(String, PkceChallenge), OAuthError> {
        let pkce = PkceChallenge::generate();
        let scope_str = scopes.join(" ");
        let scope_query = if scope_str.is_empty() {
            String::new()
        } else {
            format!("&scope={}", urlencod(&scope_str))
        };

        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code{}&state={}&code_challenge={}&code_challenge_method=S256",
            self.auth_url,
            urlencod(&self.client_id),
            urlencod(redirect_uri),
            scope_query,
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
            ("client_secret", self.client_secret.expose_secret()),
            ("code_verifier", pkce_verifier),
        ];

        let resp = self
            .http
            .post(&self.token_url)
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
            ("client_secret", self.client_secret.expose_secret()),
        ];

        let resp = self
            .http
            .post(&self.token_url)
            .form(&params)
            .send()
            .map_err(|e| OAuthError::RefreshFailed(format!("refresh: {e}")))?;

        parse_token_response(resp)
    }

    fn revoke_token(&self, token: &str) -> Result<(), OAuthError> {
        let Some(revoke_url) = &self.revoke_url else {
            return Ok(());
        };

        let resp = self
            .http
            .post(revoke_url)
            .form(&[("token", token)])
            .send()
            .map_err(|e| OAuthError::ProviderError(format!("revoke: {e}")))?;

        if resp.status().is_success() {
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
