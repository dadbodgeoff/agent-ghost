//! AgentKeypairManager — generate, store, load, rotate Ed25519 keypairs (Req 24 AC4).
//!
//! Uses ghost-signing for generation and verification.
//! Persists verifying keys to disk. Signing keys are held in memory only
//! (in production, encrypted at rest via OS keychain).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeypairError {
    #[error("keypair directory not found: {0}")]
    DirNotFound(String),
    #[error("failed to read keypair: {0}")]
    ReadError(String),
    #[error("failed to write keypair: {0}")]
    WriteError(String),
    #[error("no keypair loaded")]
    NoKeypair,
}

/// A managed keypair with rotation support.
pub struct ManagedKeypair {
    pub signing_key: ghost_signing::SigningKey,
    pub verifying_key: ghost_signing::VerifyingKey,
    pub created_at: Instant,
}

/// A retired keypair kept during grace period.
struct RetiredKeypair {
    verifying_key: ghost_signing::VerifyingKey,
    /// When this key was rotated out (NOT when it was created).
    rotated_at: Instant,
}

/// Manages agent keypairs at `~/.ghost/agents/{name}/keys/`.
pub struct AgentKeypairManager {
    keys_dir: PathBuf,
    current: Option<ManagedKeypair>,
    /// Previous key kept during grace period for rotation.
    previous: Option<RetiredKeypair>,
    /// Grace period for old keys (default 1 hour).
    grace_period: Duration,
}

impl AgentKeypairManager {
    pub fn new(keys_dir: PathBuf) -> Self {
        Self {
            keys_dir,
            current: None,
            previous: None,
            grace_period: Duration::from_secs(3600),
        }
    }

    /// Generate a new keypair and store the public key to disk.
    pub fn generate(&mut self) -> Result<&ghost_signing::VerifyingKey, KeypairError> {
        // Ensure directory exists
        std::fs::create_dir_all(&self.keys_dir)
            .map_err(|e| KeypairError::WriteError(e.to_string()))?;

        let (signing_key, verifying_key) = ghost_signing::generate_keypair();

        // Write public key to disk
        let pub_path = self.keys_dir.join("agent.pub");
        let pub_bytes = verifying_key.to_bytes();
        std::fs::write(&pub_path, pub_bytes)
            .map_err(|e| KeypairError::WriteError(e.to_string()))?;

        self.current = Some(ManagedKeypair {
            signing_key,
            verifying_key,
            created_at: Instant::now(),
        });

        // SAFETY: we just assigned `Some` to self.current above
        Ok(&self
            .current
            .as_ref()
            .expect("current was just set")
            .verifying_key)
    }

    /// Load the public key from disk. Signing key must be regenerated
    /// or loaded from secure storage separately.
    pub fn load_verifying_key(&self) -> Result<ghost_signing::VerifyingKey, KeypairError> {
        let pub_path = self.keys_dir.join("agent.pub");

        if !pub_path.exists() {
            return Err(KeypairError::DirNotFound(
                self.keys_dir.display().to_string(),
            ));
        }

        let pub_bytes =
            std::fs::read(&pub_path).map_err(|e| KeypairError::ReadError(e.to_string()))?;

        if pub_bytes.len() != 32 {
            return Err(KeypairError::ReadError("invalid public key length".into()));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&pub_bytes);

        ghost_signing::VerifyingKey::from_bytes(&arr)
            .ok_or_else(|| KeypairError::ReadError("invalid public key bytes".into()))
    }

    /// Rotate the keypair: generate new, keep old during grace period.
    pub fn rotate(&mut self) -> Result<&ghost_signing::VerifyingKey, KeypairError> {
        // Move current to previous as a retired keypair
        if let Some(current) = self.current.take() {
            self.previous = Some(RetiredKeypair {
                verifying_key: current.verifying_key,
                rotated_at: Instant::now(),
            });
        }
        // Generate new
        self.generate()
    }

    /// Verify a signature against current key, falling back to previous
    /// during the grace period.
    pub fn verify(&self, data: &[u8], signature: &ghost_signing::Signature) -> bool {
        // Try current key
        if let Some(ref current) = self.current {
            if ghost_signing::verify(data, signature, &current.verifying_key) {
                return true;
            }
        }

        // Try previous key during grace period (check time since rotation, not creation)
        if let Some(ref previous) = self.previous {
            if previous.rotated_at.elapsed() < self.grace_period {
                return ghost_signing::verify(data, signature, &previous.verifying_key);
            }
        }

        false
    }

    /// Get the current signing key.
    pub fn signing_key(&self) -> Option<&ghost_signing::SigningKey> {
        self.current.as_ref().map(|k| &k.signing_key)
    }

    /// Get the current verifying key.
    pub fn verifying_key(&self) -> Option<&ghost_signing::VerifyingKey> {
        self.current.as_ref().map(|k| &k.verifying_key)
    }

    /// Check if the previous key's grace period has expired.
    pub fn is_grace_period_expired(&self) -> bool {
        self.previous
            .as_ref()
            .map_or(true, |p| p.rotated_at.elapsed() >= self.grace_period)
    }
}
