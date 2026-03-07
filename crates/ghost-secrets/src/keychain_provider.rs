//! `KeychainProvider` — OS-native credential storage via the `keyring` crate.
//!
//! Backends: macOS Security Framework, Windows Credential Manager,
//! Linux Secret Service (D-Bus) or kernel keyutils.
//!
//! Feature-gated: `#[cfg(feature = "keychain")]`.
//! Keychain calls are synchronous — wrap in `tokio::task::spawn_blocking()`
//! when called from async context.

use secrecy::SecretString;

use crate::error::SecretsError;
use crate::provider::SecretProvider;

/// OS-native credential storage. Each secret is stored as a keyring entry
/// under `(service_name, key)`.
pub struct KeychainProvider {
    service_name: String,
}

impl KeychainProvider {
    /// Create a new `KeychainProvider` with the given service name.
    /// Default service name is `"ghost-platform"`.
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
        }
    }

    /// Returns the configured service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, SecretsError> {
        keyring::Entry::new(&self.service_name, key)
            .map_err(|e| SecretsError::ProviderError(format!("keyring entry creation failed: {e}")))
    }
}

impl SecretProvider for KeychainProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString, SecretsError> {
        if key.is_empty() {
            return Err(SecretsError::InvalidKey("key must not be empty".into()));
        }
        let entry = self.entry(key)?;
        match entry.get_password() {
            Ok(password) => Ok(SecretString::from(password)),
            Err(keyring::Error::NoEntry) => Err(SecretsError::NotFound(key.to_string())),
            Err(e) => Err(SecretsError::ProviderError(format!(
                "keyring get failed: {e}"
            ))),
        }
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretsError> {
        if key.is_empty() {
            return Err(SecretsError::InvalidKey("key must not be empty".into()));
        }
        let entry = self.entry(key)?;
        entry
            .set_password(value)
            .map_err(|e| SecretsError::ProviderError(format!("keyring set failed: {e}")))
    }

    fn delete_secret(&self, key: &str) -> Result<(), SecretsError> {
        if key.is_empty() {
            return Err(SecretsError::InvalidKey("key must not be empty".into()));
        }
        let entry = self.entry(key)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Err(SecretsError::NotFound(key.to_string())),
            Err(e) => Err(SecretsError::ProviderError(format!(
                "keyring delete failed: {e}"
            ))),
        }
    }

    fn has_secret(&self, key: &str) -> bool {
        if key.is_empty() {
            return false;
        }
        match self.entry(key) {
            Ok(entry) => entry.get_password().is_ok(),
            Err(_) => false,
        }
    }
}
