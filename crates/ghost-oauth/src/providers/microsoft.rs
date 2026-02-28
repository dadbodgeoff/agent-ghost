//! Microsoft OAuth 2.0 provider (Azure AD / Microsoft Identity Platform).
//!
//! Endpoints (with configurable tenant):
//! - Auth: `https://login.microsoftonline.com/{tenant}/oauth2/v2/authorize`
//! - Token: `https://login.microsoftonline.com/{tenant}/oauth2/v2/token`
//!
//! Default scopes: Mail.Read, Calendars.Read, User.Read.
//! Multi-tenant support via configurable tenant ID.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};

use crate::error::OAuthError;
use crate::provider::OAuthProvider;
use crate::types::{ApiRequest, ApiResponse, PkceChallenge, TokenSet};

use super::google::{execute_bearer_request, parse_token_response, urlencod};

pub struct MicrosoftOAuthProvider {
    client_id: String,
    client_secret: SecretString,
    tenant: String,
    http: reqwest::blocking::Client,
}

impl MicrosoftOAuthProvider {
    pub fn new(
        client_id: String,
        client_secret: SecretString,
        tenant: String,
    ) -> Result<Self, OAuthError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| OAuthError::ProviderError(format!("HTTP client init: {e}")))?;
        Ok(Self { client_id, client_secret, tenant, http })
    }

    fn auth_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2/authorize",
            self.tenant
        )
    }

    fn token_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2/token",
            self.tenant
        )
    }
}

impl OAuthProvider for MicrosoftOAuthProvider {
    fn name(&self) -> &str { "microsoft" }

    fn authorization_url(
        &self,
        scopes: &[String],
        state: &str,
        redirect_uri: &str,
    ) -> Result<(String, PkceChallenge), OAuthError> {
        let pkce = PkceChallenge::generate();
        let scope_str = if scopes.is_empty() {
            "openid profile".to_string()
        } else {
            scopes.join(" ")
        };

        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256&response_mode=query",
            self.auth_url(),
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
            ("client_secret", self.client_secret.expose_secret()),
            ("code_verifier", pkce_verifier),
        ];

        let resp = self.http.post(self.token_url()).form(&params).send()
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

        let resp = self.http.post(self.token_url()).form(&params).send()
            .map_err(|e| OAuthError::RefreshFailed(format!("refresh: {e}")))?;

        parse_token_response(resp)
    }

    fn revoke_token(&self, _token: &str) -> Result<(), OAuthError> {
        // Microsoft Identity Platform doesn't have a standard token revocation
        // endpoint for v2.0. Tokens expire naturally. For immediate revocation,
        // the admin must revoke refresh tokens via Microsoft Graph API.
        tracing::info!("Microsoft token revocation requested (tokens expire naturally)");
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
