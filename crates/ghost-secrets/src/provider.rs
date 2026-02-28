//! Core `SecretProvider` trait — the abstraction all backends implement.

use secrecy::SecretString;

use crate::error::SecretsError;

/// Unified interface for credential storage backends.
///
/// Implementations: [`EnvProvider`](crate::env_provider::EnvProvider),
/// [`KeychainProvider`](crate::keychain_provider::KeychainProvider) (feature `keychain`),
/// [`VaultProvider`](crate::vault_provider::VaultProvider) (feature `vault`).
pub trait SecretProvider: Send + Sync {
    /// Retrieve a secret by key. Returns `SecretString` (zeroized on drop).
    fn get_secret(&self, key: &str) -> Result<SecretString, SecretsError>;

    /// Store a secret. Some backends (e.g. env) are read-only.
    fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretsError>;

    /// Delete a secret. Some backends (e.g. env) are read-only.
    fn delete_secret(&self, key: &str) -> Result<(), SecretsError>;

    /// Check if a secret exists without retrieving its value.
    fn has_secret(&self, key: &str) -> bool;
}

/// Configuration for selecting a secret provider backend.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum ProviderConfig {
    /// Read secrets from environment variables (default, read-only).
    #[default]
    Env,
    /// OS-native credential storage (macOS Keychain, Windows Credential Manager, Linux Secret Service).
    Keychain {
        #[serde(default = "default_service_name")]
        service_name: String,
    },
    /// HashiCorp Vault KV v2 API.
    Vault {
        endpoint: String,
        #[serde(default = "default_mount")]
        mount: String,
        /// Environment variable name containing the Vault token (bootstrap).
        #[serde(default = "default_token_env")]
        token_env: String,
    },
}

fn default_service_name() -> String {
    "ghost-platform".into()
}

fn default_mount() -> String {
    "secret".into()
}

fn default_token_env() -> String {
    "VAULT_TOKEN".into()
}
