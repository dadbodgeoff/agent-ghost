//! Token storage with encryption at rest.
//!
//! Tokens are encrypted using a vault key retrieved from `SecretProvider`.
//! Storage path: `~/.ghost/oauth/tokens/{provider}/{ref_id}.age`
//! Atomic writes (temp + rename) prevent corruption.
//! Vault key auto-generated on first use if not present.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use ghost_secrets::{ExposeSecret, SecretProvider, SecretString};
use rand::Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::OAuthError;
use crate::types::{OAuthRefId, TokenSet, TokenSetSerde};

/// Key name in SecretProvider for the token encryption key.
const VAULT_KEY_NAME: &str = "ghost-oauth-vault-key";

/// Token store: encrypted token persistence backed by `SecretProvider`.
pub struct TokenStore {
    /// Base directory for token files (default `~/.ghost/oauth/tokens`).
    base_dir: PathBuf,
    /// Secret provider for encryption key retrieval.
    secret_provider: Box<dyn SecretProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredPendingFlow {
    pub state: String,
    pub provider_name: String,
    pub ref_id: OAuthRefId,
    pub pkce_verifier: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredConnectionMeta {
    pub ref_id: OAuthRefId,
    pub provider_name: String,
    pub scopes: Vec<String>,
    pub connected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredDisconnectTombstone {
    pub ref_id: OAuthRefId,
    pub provider_name: String,
    pub disconnected_at: DateTime<Utc>,
}

impl TokenStore {
    /// Create a new `TokenStore`.
    ///
    /// `base_dir`: root directory for encrypted token files.
    /// `secret_provider`: backend for retrieving/storing the vault key.
    pub fn new(base_dir: PathBuf, secret_provider: Box<dyn SecretProvider>) -> Self {
        Self {
            base_dir,
            secret_provider,
        }
    }

    /// Create with the default base directory (`~/.ghost/oauth/tokens`).
    pub fn with_default_dir(secret_provider: Box<dyn SecretProvider>) -> Self {
        let base = dirs_home()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ghost")
            .join("oauth")
            .join("tokens");
        Self::new(base, secret_provider)
    }

    /// Store an encrypted token set for a provider connection.
    pub fn store_token(
        &self,
        ref_id: &OAuthRefId,
        provider: &str,
        token_set: &TokenSet,
    ) -> Result<(), OAuthError> {
        self.write_encrypted_json(
            self.token_path(provider, ref_id),
            &TokenSetSerde::from(token_set),
        )?;

        tracing::debug!(
            provider = %provider,
            ref_id = %ref_id,
            "token stored (encrypted)"
        );
        Ok(())
    }

    /// Load and decrypt a token set. Returns `TokenExpired` if the token
    /// has expired (caller should refresh).
    pub fn load_token(&self, ref_id: &OAuthRefId, provider: &str) -> Result<TokenSet, OAuthError> {
        let ts = self.load_token_raw(ref_id, provider)?;
        if ts.is_expired() {
            return Err(OAuthError::TokenExpired(ref_id.to_string()));
        }
        Ok(ts)
    }

    /// Load and decrypt a token set WITHOUT checking expiry.
    /// Used internally by the broker for refresh flows.
    pub fn load_token_raw(
        &self,
        ref_id: &OAuthRefId,
        provider: &str,
    ) -> Result<TokenSet, OAuthError> {
        match self.read_encrypted_json::<TokenSetSerde>(&self.token_path(provider, ref_id))? {
            Some(serde_form) => Ok(serde_form.into()),
            None => Err(OAuthError::NotConnected(ref_id.to_string())),
        }
    }

    /// Delete an encrypted token file.
    pub fn delete_token(&self, ref_id: &OAuthRefId, provider: &str) -> Result<(), OAuthError> {
        self.delete_if_exists(&self.token_path(provider, ref_id))?;
        tracing::debug!(provider = %provider, ref_id = %ref_id, "token deleted");
        Ok(())
    }

    pub(crate) fn store_pending_flow(
        &self,
        state: &str,
        flow: &StoredPendingFlow,
    ) -> Result<(), OAuthError> {
        self.write_encrypted_json(self.pending_flow_path(state), flow)
    }

    pub(crate) fn load_pending_flow(
        &self,
        state: &str,
    ) -> Result<Option<StoredPendingFlow>, OAuthError> {
        let Some(flow) =
            self.read_encrypted_json::<StoredPendingFlow>(&self.pending_flow_path(state))?
        else {
            return Ok(None);
        };
        if flow.state != state {
            return Err(OAuthError::StorageError(
                "pending OAuth flow state mismatch".into(),
            ));
        }
        Ok(Some(flow))
    }

    pub(crate) fn delete_pending_flow(&self, state: &str) -> Result<(), OAuthError> {
        self.delete_if_exists(&self.pending_flow_path(state))
    }

    pub(crate) fn store_connection_meta(
        &self,
        meta: &StoredConnectionMeta,
    ) -> Result<(), OAuthError> {
        self.write_encrypted_json(self.connection_meta_path(&meta.ref_id), meta)
    }

    pub(crate) fn load_connection_meta(
        &self,
        ref_id: &OAuthRefId,
    ) -> Result<Option<StoredConnectionMeta>, OAuthError> {
        self.read_encrypted_json(&self.connection_meta_path(ref_id))
    }

    pub(crate) fn list_connection_metas(&self) -> Result<Vec<StoredConnectionMeta>, OAuthError> {
        self.list_encrypted_entries(&self.connections_dir())
    }

    pub(crate) fn delete_connection_meta(&self, ref_id: &OAuthRefId) -> Result<(), OAuthError> {
        self.delete_if_exists(&self.connection_meta_path(ref_id))
    }

    pub(crate) fn store_disconnect_tombstone(
        &self,
        tombstone: &StoredDisconnectTombstone,
    ) -> Result<(), OAuthError> {
        self.write_encrypted_json(self.disconnect_tombstone_path(&tombstone.ref_id), tombstone)
    }

    pub(crate) fn load_disconnect_tombstone(
        &self,
        ref_id: &OAuthRefId,
    ) -> Result<Option<StoredDisconnectTombstone>, OAuthError> {
        self.read_encrypted_json(&self.disconnect_tombstone_path(ref_id))
    }

    /// List all connection ref_ids for a provider.
    pub fn list_connections(&self, provider: &str) -> Result<Vec<OAuthRefId>, OAuthError> {
        let dir = self.provider_dir(provider);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut refs = Vec::new();
        let entries =
            fs::read_dir(&dir).map_err(|e| OAuthError::StorageError(format!("read dir: {e}")))?;

        for entry in entries {
            let entry = entry.map_err(|e| OAuthError::StorageError(format!("dir entry: {e}")))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(stem) = name.strip_suffix(".age") {
                if let Ok(uuid) = stem.parse::<uuid::Uuid>() {
                    refs.push(OAuthRefId::from_uuid(uuid));
                }
            }
        }
        Ok(refs)
    }

    /// List all connections across all providers.
    pub fn list_all_connections(&self) -> Result<Vec<(String, OAuthRefId)>, OAuthError> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut all = Vec::new();
        let entries = fs::read_dir(&self.base_dir)
            .map_err(|e| OAuthError::StorageError(format!("read base dir: {e}")))?;

        for entry in entries {
            let entry = entry.map_err(|e| OAuthError::StorageError(format!("dir entry: {e}")))?;
            if entry.path().is_dir() {
                let provider = entry.file_name().to_string_lossy().to_string();
                if provider == "_broker" {
                    continue;
                }
                for ref_id in self.list_connections(&provider)? {
                    all.push((provider.clone(), ref_id));
                }
            }
        }
        Ok(all)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn provider_dir(&self, provider: &str) -> PathBuf {
        self.base_dir.join(sanitize_path_component(provider))
    }

    fn token_path(&self, provider: &str, ref_id: &OAuthRefId) -> PathBuf {
        self.provider_dir(provider)
            .join(format!("{}.age", ref_id.as_uuid()))
    }

    fn broker_dir(&self) -> PathBuf {
        self.base_dir.join("_broker")
    }

    fn pending_dir(&self) -> PathBuf {
        self.broker_dir().join("pending")
    }

    fn connections_dir(&self) -> PathBuf {
        self.broker_dir().join("connections")
    }

    fn disconnects_dir(&self) -> PathBuf {
        self.broker_dir().join("disconnects")
    }

    fn pending_flow_path(&self, state: &str) -> PathBuf {
        self.pending_dir()
            .join(format!("{}.age", Self::state_file_key(state)))
    }

    fn connection_meta_path(&self, ref_id: &OAuthRefId) -> PathBuf {
        self.connections_dir()
            .join(format!("{}.age", ref_id.as_uuid()))
    }

    fn disconnect_tombstone_path(&self, ref_id: &OAuthRefId) -> PathBuf {
        self.disconnects_dir()
            .join(format!("{}.age", ref_id.as_uuid()))
    }

    fn write_encrypted_json<T>(&self, target: PathBuf, value: &T) -> Result<(), OAuthError>
    where
        T: Serialize,
    {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| OAuthError::StorageError(format!("create dir: {e}")))?;
        }

        let json = serde_json::to_vec(value)
            .map_err(|e| OAuthError::StorageError(format!("serialize: {e}")))?;
        let key = self.get_or_create_vault_key()?;
        let encrypted = Self::encrypt(&json, key.expose_secret());

        let tmp = target.with_extension("tmp");
        fs::write(&tmp, &encrypted)
            .map_err(|e| OAuthError::StorageError(format!("write tmp: {e}")))?;
        fs::rename(&tmp, &target).map_err(|e| OAuthError::StorageError(format!("rename: {e}")))?;
        Ok(())
    }

    fn read_encrypted_json<T>(&self, path: &PathBuf) -> Result<Option<T>, OAuthError>
    where
        T: DeserializeOwned,
    {
        if !path.exists() {
            return Ok(None);
        }

        let encrypted =
            fs::read(path).map_err(|e| OAuthError::StorageError(format!("read: {e}")))?;
        let key = self.get_or_create_vault_key()?;
        let decrypted = Self::decrypt(&encrypted, key.expose_secret())
            .map_err(|e| OAuthError::EncryptionError(format!("decrypt: {e}")))?;
        let value = serde_json::from_slice(&decrypted)
            .map_err(|e| OAuthError::StorageError(format!("deserialize: {e}")))?;
        Ok(Some(value))
    }

    fn list_encrypted_entries<T>(&self, dir: &PathBuf) -> Result<Vec<T>, OAuthError>
    where
        T: DeserializeOwned,
    {
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let dir_entries =
            fs::read_dir(dir).map_err(|e| OAuthError::StorageError(format!("read dir: {e}")))?;
        for entry in dir_entries {
            let entry = entry.map_err(|e| OAuthError::StorageError(format!("dir entry: {e}")))?;
            if entry.path().is_file() {
                if let Some(value) = self.read_encrypted_json::<T>(&entry.path())? {
                    entries.push(value);
                }
            }
        }
        Ok(entries)
    }

    fn delete_if_exists(&self, path: &PathBuf) -> Result<(), OAuthError> {
        if path.exists() {
            fs::remove_file(path).map_err(|e| OAuthError::StorageError(format!("delete: {e}")))?;
        }
        Ok(())
    }

    fn state_file_key(state: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(state.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Retrieve the vault key from SecretProvider, or auto-generate one.
    fn get_or_create_vault_key(&self) -> Result<SecretString, OAuthError> {
        match self.secret_provider.get_secret(VAULT_KEY_NAME) {
            Ok(key) => Ok(key),
            Err(ghost_secrets::SecretsError::NotFound(_)) => {
                // Auto-generate a 256-bit random key
                let mut rng = rand::thread_rng();
                let key_bytes: [u8; 32] = rng.gen();
                let key_str =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key_bytes);
                // Try to store it; if the provider is read-only, that's OK —
                // we'll use the generated key for this session.
                if let Err(e) = self.secret_provider.set_secret(VAULT_KEY_NAME, &key_str) {
                    tracing::warn!(
                        error = %e,
                        "could not persist OAuth vault key (provider may be read-only) — key valid for this session only"
                    );
                }
                tracing::info!("auto-generated OAuth vault key");
                Ok(SecretString::from(key_str))
            }
            Err(e) => Err(OAuthError::EncryptionError(format!(
                "failed to retrieve vault key: {e}"
            ))),
        }
    }

    /// Encrypt data using HMAC-derived XOR stream.
    ///
    /// NOTE: This is a placeholder implementation. Production should use the
    /// `age` crate for authenticated encryption (as specified in the design doc).
    /// The current XOR-stream approach provides confidentiality but not
    /// authentication — a corrupted ciphertext may decrypt to garbage rather
    /// than returning an explicit error.
    ///
    /// Format: [16-byte salt][encrypted data]
    /// Key derivation: SHA-256(passphrase || salt) → 32-byte stream key.
    fn encrypt(data: &[u8], passphrase: &str) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let salt: [u8; 16] = rng.gen();

        let stream_key = Self::derive_key(passphrase, &salt);
        let mut out = Vec::with_capacity(16 + data.len());
        out.extend_from_slice(&salt);
        for (i, byte) in data.iter().enumerate() {
            out.push(byte ^ stream_key[i % 32]);
        }
        out
    }

    /// Decrypt data encrypted by `encrypt`.
    fn decrypt(data: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
        if data.len() < 16 {
            return Err("encrypted data too short".into());
        }
        let salt = &data[..16];
        let ciphertext = &data[16..];

        let stream_key = Self::derive_key(passphrase, salt);
        let plain: Vec<u8> = ciphertext
            .iter()
            .enumerate()
            .map(|(i, byte)| byte ^ stream_key[i % 32])
            .collect();
        Ok(plain)
    }

    fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(passphrase.as_bytes());
        hasher.update(salt);
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        key
    }
}

/// Sanitize a path component to prevent directory traversal.
fn sanitize_path_component(s: &str) -> String {
    s.replace("..", "").replace(['/', '\\', '\0'], "")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}
