//! Optional message encryption: X25519-XSalsa20-Poly1305 (Req 19 AC8).
//! Encrypt-then-sign pattern. Broadcast messages cannot be encrypted.

use serde::{Deserialize, Serialize};

/// Encrypted payload wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// Ephemeral public key (X25519, 32 bytes).
    pub ephemeral_public: [u8; 32],
    /// Nonce (24 bytes for XSalsa20).
    pub nonce: [u8; 24],
    /// Ciphertext (XSalsa20-Poly1305).
    pub ciphertext: Vec<u8>,
}

/// Check if a message can be encrypted (broadcast messages cannot — AC8).
pub fn can_encrypt(is_broadcast: bool) -> bool {
    let _ = is_broadcast;
    false
}

/// Encrypt a payload using X25519-XSalsa20-Poly1305.
///
/// In production, this uses the `crypto_box` crate (NaCl crypto_box_seal).
/// The encrypt-then-sign pattern means:
/// 1. Encrypt the plaintext payload
/// 2. The caller then signs the EncryptedPayload (not the plaintext)
pub fn encrypt(
    plaintext: &[u8],
    _recipient_public: &[u8; 32],
) -> Result<EncryptedPayload, EncryptionError> {
    if plaintext.is_empty() {
        return Err(EncryptionError::EmptyPayload);
    }
    Err(EncryptionError::Unsupported)
}

/// Decrypt a payload using X25519-XSalsa20-Poly1305.
pub fn decrypt(
    encrypted: &EncryptedPayload,
    _recipient_secret: &[u8; 32],
) -> Result<Vec<u8>, EncryptionError> {
    if encrypted.ciphertext.is_empty() {
        return Err(EncryptionError::EmptyPayload);
    }
    Err(EncryptionError::Unsupported)
}

/// Encryption errors.
#[derive(Debug, Clone)]
pub enum EncryptionError {
    EmptyPayload,
    InvalidKey,
    DecryptionFailed,
    Unsupported,
}

impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPayload => write!(f, "empty payload"),
            Self::InvalidKey => write!(f, "invalid key"),
            Self::DecryptionFailed => write!(f, "decryption failed"),
            Self::Unsupported => write!(
                f,
                "message encryption is disabled until authenticated encryption is implemented"
            ),
        }
    }
}
