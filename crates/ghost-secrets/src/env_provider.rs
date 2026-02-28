//! `EnvProvider` — reads secrets from environment variables (read-only).

use secrecy::SecretString;

use crate::error::SecretsError;
use crate::provider::SecretProvider;

/// Reads secrets from environment variables. Write operations return
/// `StorageUnavailable` because env vars are read-only at runtime.
pub struct EnvProvider;

impl EnvProvider {
    /// Validate that a key is safe for env var lookup.
    fn validate_key(key: &str) -> Result<(), SecretsError> {
        if key.is_empty() {
            return Err(SecretsError::InvalidKey("key must not be empty".into()));
        }
        if key.contains('\0') {
            return Err(SecretsError::InvalidKey(
                "key must not contain null bytes".into(),
            ));
        }
        if key.contains('=') {
            return Err(SecretsError::InvalidKey(
                "key must not contain '=' character".into(),
            ));
        }
        Ok(())
    }
}

impl SecretProvider for EnvProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString, SecretsError> {
        Self::validate_key(key)?;
        match std::env::var(key) {
            Ok(val) => Ok(SecretString::from(val)),
            Err(std::env::VarError::NotPresent) => {
                Err(SecretsError::NotFound(key.to_string()))
            }
            Err(std::env::VarError::NotUnicode(_)) => Err(SecretsError::ProviderError(
                format!("env var '{key}' contains non-UTF-8 data"),
            )),
        }
    }

    fn set_secret(&self, _key: &str, _value: &str) -> Result<(), SecretsError> {
        Err(SecretsError::StorageUnavailable(
            "environment variables are read-only at runtime".into(),
        ))
    }

    fn delete_secret(&self, _key: &str) -> Result<(), SecretsError> {
        Err(SecretsError::StorageUnavailable(
            "environment variables are read-only at runtime".into(),
        ))
    }

    fn has_secret(&self, key: &str) -> bool {
        if Self::validate_key(key).is_err() {
            return false;
        }
        std::env::var(key).is_ok()
    }
}
