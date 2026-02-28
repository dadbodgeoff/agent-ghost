//! `AuthProfileManager` — credential retrieval via `SecretProvider`.
//!
//! Migrated from direct env var reads to the `ghost-secrets` abstraction.
//! Backward compatible: defaults to `EnvProvider` when no secrets config is set.
//! `SecretString` is retrieved just-in-time per request and never stored
//! in long-lived structs. Never logged via tracing.

use ghost_secrets::{EnvProvider, SecretProvider, SecretString, SecretsError};

use crate::provider::LLMError;

/// Key naming convention for LLM provider credentials.
/// Primary key: `{provider}-api-key`
/// Rotation keys: `{provider}-api-key-2`, `{provider}-api-key-3`, etc.
fn credential_key(provider_name: &str, index: usize) -> String {
    if index == 0 {
        format!("{provider_name}-api-key")
    } else {
        format!("{provider_name}-api-key-{}", index + 1)
    }
}

/// Also supports the legacy env var naming convention (uppercase, underscores).
/// e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`
fn legacy_env_key(provider_name: &str, index: usize) -> String {
    let upper = provider_name.to_uppercase().replace('-', "_");
    if index == 0 {
        format!("{upper}_API_KEY")
    } else {
        format!("{upper}_API_KEY_{}", index + 1)
    }
}

/// Manages credential retrieval for LLM providers via `SecretProvider`.
///
/// Supports credential rotation: on 401/429, the caller advances the
/// profile index and retrieves the next key.
pub struct AuthProfileManager {
    provider: Box<dyn SecretProvider>,
    /// LLM provider name (e.g. "anthropic", "openai").
    provider_name: String,
    /// Current profile index for rotation.
    current_index: usize,
    /// Maximum number of profiles to try before giving up.
    max_profiles: usize,
}

impl AuthProfileManager {
    /// Create a new `AuthProfileManager` with a custom `SecretProvider`.
    pub fn new(
        secret_provider: Box<dyn SecretProvider>,
        provider_name: &str,
        max_profiles: usize,
    ) -> Self {
        Self {
            provider: secret_provider,
            provider_name: provider_name.to_string(),
            current_index: 0,
            max_profiles,
        }
    }

    /// Create with the default `EnvProvider` (backward compatibility).
    pub fn with_env(provider_name: &str) -> Self {
        Self::new(Box::new(EnvProvider), provider_name, 3)
    }

    /// Retrieve the current credential as a `SecretString`.
    ///
    /// Tries the new naming convention first (`{provider}-api-key`),
    /// then falls back to the legacy env var convention (`PROVIDER_API_KEY`).
    /// The returned `SecretString` is zeroized on drop.
    pub fn get_credential(&self) -> Result<SecretString, LLMError> {
        let key = credential_key(&self.provider_name, self.current_index);

        match self.provider.get_secret(&key) {
            Ok(secret) => {
                tracing::debug!(
                    provider = %self.provider_name,
                    profile_index = self.current_index,
                    "credential retrieved (value redacted)"
                );
                Ok(secret)
            }
            Err(SecretsError::NotFound(_)) => {
                // Fallback to legacy env var naming
                let legacy = legacy_env_key(&self.provider_name, self.current_index);
                match self.provider.get_secret(&legacy) {
                    Ok(secret) => {
                        tracing::debug!(
                            provider = %self.provider_name,
                            profile_index = self.current_index,
                            "credential retrieved via legacy key (value redacted)"
                        );
                        Ok(secret)
                    }
                    Err(_) => Err(LLMError::AuthFailed(format!(
                        "no credential found for '{}' (tried '{}' and '{}')",
                        self.provider_name, key, legacy
                    ))),
                }
            }
            Err(e) => Err(LLMError::AuthFailed(format!(
                "secret provider error for '{}': {e}",
                self.provider_name
            ))),
        }
    }

    /// Advance to the next credential profile (for rotation on 401/429).
    /// Returns `true` if there are more profiles to try, `false` if exhausted.
    pub fn rotate(&mut self) -> bool {
        if self.current_index + 1 < self.max_profiles {
            self.current_index += 1;
            tracing::info!(
                provider = %self.provider_name,
                new_index = self.current_index,
                "rotating to next auth profile"
            );
            true
        } else {
            tracing::warn!(
                provider = %self.provider_name,
                "all auth profiles exhausted"
            );
            false
        }
    }

    /// Reset to the first profile.
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// Current profile index.
    pub fn current_index(&self) -> usize {
        self.current_index
    }
}
