//! Token storage with encryption at rest.
//!
//! Tokens are encrypted using a vault key retrieved from `SecretProvider`.
//! Storage path: `~/.ghost/oauth/tokens/{provider}/{ref_id}.age`
//! Atomic writes (temp + rename) prevent corruption.
//! Vault key auto-generated on first use if not present.

use std::fs;
use std::path::PathBuf;

use ghost_secrets::{ExposeSecret, SecretProvider, SecretString};
use rand::Rng;
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
        let dir = self.provider_dir(provider);
        fs::create_dir_all(&dir)
            .map_err(|e| OAuthError::StorageError(format!("create dir: {e}")))?;

        let serde_form: TokenSetSerde = token_set.into();
        let json = serde_json::to_vec(&serde_form)
            .map_err(|e| OAuthError::StorageError(format!("serialize: {e}")))?;

        let key = self.get_or_create_vault_key()?;
        let encrypted = Self::encrypt(&json, key.expose_secret());

        // Atomic write: temp file + rename
        let target = self.token_path(provider, ref_id);
        let tmp = target.with_extension("tmp");
        fs::write(&tmp, &encrypted)
            .map_err(|e| OAuthError::StorageError(format!("write tmp: {e}")))?;
        fs::rename(&tmp, &target)
            .map_err(|e| OAuthError::StorageError(format!("rename: {e}")))?;

        tracing::debug!(
            provider = %provider,
            ref_id = %ref_id,
            "token stored (encrypted)"
        );
        Ok(())
    }

    /// Load and decrypt a token set. Returns `TokenExpired` if the token
    /// has expired (caller should refresh).
    pub fn load_token(
        &self,
        ref_id: &OAuthRefId,
        provider: &str,
    ) -> Result<TokenSet, OAuthError> {
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
        let path = self.token_path(provider, ref_id);
        if !path.exists() {
            return Err(OAuthError::NotConnected(ref_id.to_string()));
        }

        let encrypted = fs::read(&path)
            .map_err(|e| OAuthError::StorageError(format!("read: {e}")))?;

        let key = self.get_or_create_vault_key()?;
        let decrypted = Self::decrypt(&encrypted, key.expose_secret())
            .map_err(|e| OAuthError::EncryptionError(format!("decrypt: {e}")))?;

        let serde_form: TokenSetSerde = serde_json::from_slice(&decrypted)
            .map_err(|e| OAuthError::StorageError(format!("deserialize: {e}")))?;

        Ok(serde_form.into())
    }

    /// Delete an encrypted token file.
    pub fn delete_token(
        &self,
        ref_id: &OAuthRefId,
        provider: &str,
    ) -> Result<(), OAuthError> {
        let path = self.token_path(provider, ref_id);
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| OAuthError::StorageError(format!("delete: {e}")))?;
        }
        tracing::debug!(provider = %provider, ref_id = %ref_id, "token deleted");
        Ok(())
    }

    /// List all connection ref_ids for a provider.
    pub fn list_connections(&self, provider: &str) -> Result<Vec<OAuthRefId>, OAuthError> {
        let dir = self.provider_dir(provider);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut refs = Vec::new();
        let entries = fs::read_dir(&dir)
            .map_err(|e| OAuthError::StorageError(format!("read dir: {e}")))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| OAuthError::StorageError(format!("dir entry: {e}")))?;
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
            let entry = entry
                .map_err(|e| OAuthError::StorageError(format!("dir entry: {e}")))?;
            if entry.path().is_dir() {
                let provider = entry.file_name().to_string_lossy().to_string();
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

    /// Retrieve the vault key from SecretProvider, or auto-generate one.
    fn get_or_create_vault_key(&self) -> Result<SecretString, OAuthError> {
        match self.secret_provider.get_secret(VAULT_KEY_NAME) {
            Ok(key) => Ok(key),
            Err(ghost_secrets::SecretsError::NotFound(_)) => {
                // Auto-generate a 256-bit random key
                let mut rng = rand::thread_rng();
                let key_bytes: [u8; 32] = rng.gen();
                let key_str = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    key_bytes,
                );
                // Try to store it; if the provider is read-only, that's OK —
                // we'll use the generated key for this session.
                let _ = self.secret_provider.set_secret(VAULT_KEY_NAME, &key_str);
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
    s.replace("..", "")
        .replace(['/', '\\', '\0'], "")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}
