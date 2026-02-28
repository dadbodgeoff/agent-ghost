//! `OAuthProvider` trait — the abstraction all OAuth provider backends implement.
//!
//! Each provider (Google, GitHub, Slack, Microsoft) implements this trait
//! to handle provider-specific quirks (non-standard token exchange, bot tokens, etc.).

use crate::error::OAuthError;
use crate::types::{ApiRequest, ApiResponse, PkceChallenge, TokenSet};

/// Unified interface for OAuth 2.0 provider backends.
///
/// Implementations handle provider-specific authorization URLs, token exchange,
/// refresh, revocation, and API call execution with Bearer token injection.
pub trait OAuthProvider: Send + Sync {
    /// Provider name (e.g. "google", "github", "slack", "microsoft").
    fn name(&self) -> &str;

    /// Generate an authorization URL with PKCE challenge.
    ///
    /// Returns `(authorization_url, pkce_challenge)`. The caller must store
    /// the `PkceChallenge.code_verifier` for the callback exchange.
    fn authorization_url(
        &self,
        scopes: &[String],
        state: &str,
        redirect_uri: &str,
    ) -> Result<(String, PkceChallenge), OAuthError>;

    /// Exchange an authorization code for tokens using the PKCE verifier.
    fn exchange_code(
        &self,
        code: &str,
        pkce_verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, OAuthError>;

    /// Refresh an expired access token using the refresh token.
    ///
    /// Returns a new `TokenSet`. Not all providers support refresh tokens
    /// (e.g. GitHub uses long-lived tokens).
    fn refresh_token(&self, refresh_token: &str) -> Result<TokenSet, OAuthError>;

    /// Revoke a token at the provider.
    fn revoke_token(&self, token: &str) -> Result<(), OAuthError>;

    /// Execute an API call with Bearer token injection.
    ///
    /// The broker calls this after decrypting the token. The token is injected
    /// into the `Authorization: Bearer {token}` header automatically.
    fn execute_api_call(
        &self,
        access_token: &str,
        request: &ApiRequest,
    ) -> Result<ApiResponse, OAuthError>;
}
