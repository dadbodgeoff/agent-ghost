//! CLI authentication — token storage and resolution (Task 6.6 — §5.2, F.1).

use ghost_secrets::SecretProvider;
use secrecy::ExposeSecret;

use super::error::CliError;

const CLI_TOKEN_KEY: &str = "ghost_cli_token";

/// Store a CLI authentication token via the secret provider.
pub fn store_token(provider: &dyn SecretProvider, token: &str) -> Result<(), CliError> {
    provider
        .set_secret(CLI_TOKEN_KEY, token)
        .map_err(|e| CliError::Auth(format!("failed to store token: {e}")))
}

/// Load a previously stored CLI token, if any.
pub fn load_token(provider: &dyn SecretProvider) -> Option<String> {
    provider
        .get_secret(CLI_TOKEN_KEY)
        .ok()
        .map(|s| s.expose_secret().to_string())
}

/// Delete the stored CLI token.
pub fn clear_token(provider: &dyn SecretProvider) -> Result<(), CliError> {
    provider
        .delete_secret(CLI_TOKEN_KEY)
        .map_err(|e| CliError::Auth(format!("failed to clear token: {e}")))
}

/// Resolve the active token: `GHOST_TOKEN` env var first, then stored token.
pub fn resolve_token(provider: &dyn SecretProvider) -> Option<String> {
    if let Ok(token) = std::env::var("GHOST_TOKEN") {
        if !token.is_empty() {
            return Some(token);
        }
    }
    load_token(provider)
}
