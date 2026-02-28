//! `VaultProvider` — HashiCorp Vault KV v2 HTTP API backend.
//!
//! Feature-gated: `#[cfg(feature = "vault")]`.
//! Uses `reqwest::blocking::Client` with 5s timeout.
//! Vault token stored as `SecretString`, zeroized after use.

use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

use crate::error::SecretsError;
use crate::provider::SecretProvider;

/// HashiCorp Vault KV v2 secret provider.
pub struct VaultProvider {
    endpoint: String,
    mount: String,
    token: SecretString,
    client: reqwest::blocking::Client,
}

impl VaultProvider {
    /// Create a new `VaultProvider`.
    ///
    /// - `endpoint`: Vault server URL (e.g. `https://vault.example.com`)
    /// - `mount`: KV v2 mount path (default `"secret"`)
    /// - `token`: Vault authentication token
    ///
    /// Token renewal: in production, call `renew_token()` periodically
    /// before the lease expires. For initial implementation, token renewal
    /// is the caller's responsibility (e.g. via a background tokio task
    /// in ghost-gateway).
    pub fn new(endpoint: &str, mount: &str, token: SecretString) -> Result<Self, SecretsError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| SecretsError::ProviderError(format!("HTTP client init failed: {e}")))?;

        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            mount: mount.to_string(),
            token,
            client,
        })
    }

    /// Construct the KV v2 data URL for a given key.
    fn data_url(&self, key: &str) -> String {
        let sanitized = Self::sanitize_key(key);
        format!("{}/v1/{}/data/ghost/{}", self.endpoint, self.mount, sanitized)
    }

    /// Construct the KV v2 metadata URL for deletion.
    fn metadata_url(&self, key: &str) -> String {
        let sanitized = Self::sanitize_key(key);
        format!(
            "{}/v1/{}/metadata/ghost/{}",
            self.endpoint, self.mount, sanitized
        )
    }

    /// Sanitize key to prevent path traversal.
    fn sanitize_key(key: &str) -> String {
        key.replace("..", "")
            .replace('/', "")
            .replace('\\', "")
    }

    /// Validate key before use.
    fn validate_key(key: &str) -> Result<(), SecretsError> {
        if key.is_empty() {
            return Err(SecretsError::InvalidKey("key must not be empty".into()));
        }
        if key.contains('\0') {
            return Err(SecretsError::InvalidKey(
                "key must not contain null bytes".into(),
            ));
        }
        Ok(())
    }

    /// Parse a Vault KV v2 JSON response to extract the secret value.
    pub fn parse_kv2_response(body: &str) -> Result<String, SecretsError> {
        let parsed: Value = serde_json::from_str(body)
            .map_err(|e| SecretsError::ProviderError(format!("malformed JSON from Vault: {e}")))?;

        parsed
            .get("data")
            .and_then(|d| d.get("data"))
            .and_then(|d| d.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                SecretsError::ProviderError(
                    "Vault response missing .data.data.value field".into(),
                )
            })
    }

    /// Renew the Vault token lease.
    ///
    /// Sends `POST /v1/auth/token/renew-self` with the current token.
    /// Should be called periodically before the token lease expires
    /// (e.g. from a background `tokio::task` in ghost-gateway).
    pub fn renew_token(&self) -> Result<(), SecretsError> {
        let url = format!("{}/v1/auth/token/renew-self", self.endpoint);
        let resp = self
            .client
            .post(&url)
            .header("X-Vault-Token", self.token.expose_secret())
            .send()
            .map_err(|e: reqwest::Error| {
                if e.is_timeout() {
                    SecretsError::StorageUnavailable(format!("Vault timeout during token renewal: {e}"))
                } else {
                    SecretsError::StorageUnavailable(format!("Vault token renewal failed: {e}"))
                }
            })?;

        match resp.status().as_u16() {
            200 => {
                tracing::debug!("Vault token renewed successfully");
                Ok(())
            }
            403 => Err(SecretsError::ProviderError(
                "Vault token renewal failed (403 Forbidden — token may be expired)".into(),
            )),
            status => Err(SecretsError::ProviderError(format!(
                "Vault token renewal returned HTTP {status}"
            ))),
        }
    }
}

impl SecretProvider for VaultProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString, SecretsError> {
        Self::validate_key(key)?;
        let url = self.data_url(key);

        let resp = self
            .client
            .get(&url)
            .header("X-Vault-Token", self.token.expose_secret())
            .send()
            .map_err(|e: reqwest::Error| {
                if e.is_timeout() {
                    SecretsError::StorageUnavailable(format!("Vault timeout: {e}"))
                } else {
                    SecretsError::StorageUnavailable(format!("Vault request failed: {e}"))
                }
            })?;

        match resp.status().as_u16() {
            200 => {
                let body = resp.text().map_err(|e| {
                    SecretsError::ProviderError(format!("failed to read Vault response: {e}"))
                })?;
                let value = Self::parse_kv2_response(&body)?;
                Ok(SecretString::from(value))
            }
            404 => Err(SecretsError::NotFound(key.to_string())),
            403 => Err(SecretsError::ProviderError(
                "Vault authentication failed (403 Forbidden)".into(),
            )),
            status => Err(SecretsError::ProviderError(format!(
                "Vault returned HTTP {status}"
            ))),
        }
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretsError> {
        Self::validate_key(key)?;
        let url = self.data_url(key);

        let body = serde_json::json!({
            "data": {
                "value": value
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("X-Vault-Token", self.token.expose_secret())
            .json(&body)
            .send()
            .map_err(|e: reqwest::Error| {
                if e.is_timeout() {
                    SecretsError::StorageUnavailable(format!("Vault timeout: {e}"))
                } else {
                    SecretsError::StorageUnavailable(format!("Vault request failed: {e}"))
                }
            })?;

        match resp.status().as_u16() {
            200 | 204 => Ok(()),
            403 => Err(SecretsError::ProviderError(
                "Vault authentication failed (403 Forbidden)".into(),
            )),
            status => Err(SecretsError::ProviderError(format!(
                "Vault returned HTTP {status}"
            ))),
        }
    }

    fn delete_secret(&self, key: &str) -> Result<(), SecretsError> {
        Self::validate_key(key)?;
        let url = self.metadata_url(key);

        let resp = self
            .client
            .delete(&url)
            .header("X-Vault-Token", self.token.expose_secret())
            .send()
            .map_err(|e: reqwest::Error| {
                if e.is_timeout() {
                    SecretsError::StorageUnavailable(format!("Vault timeout: {e}"))
                } else {
                    SecretsError::StorageUnavailable(format!("Vault request failed: {e}"))
                }
            })?;

        match resp.status().as_u16() {
            200 | 204 => Ok(()),
            404 => Err(SecretsError::NotFound(key.to_string())),
            403 => Err(SecretsError::ProviderError(
                "Vault authentication failed (403 Forbidden)".into(),
            )),
            status => Err(SecretsError::ProviderError(format!(
                "Vault returned HTTP {status}"
            ))),
        }
    }

    fn has_secret(&self, key: &str) -> bool {
        if Self::validate_key(key).is_err() {
            return false;
        }
        matches!(self.get_secret(key), Ok(_))
    }
}
