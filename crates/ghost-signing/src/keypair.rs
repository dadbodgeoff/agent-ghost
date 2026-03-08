//! Ed25519 keypair generation with zeroize-on-drop semantics.

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// Ed25519 signing key. Wraps `ed25519_dalek::SigningKey`.
///
/// Private key material is zeroized on drop. With the `zeroize` feature
/// enabled on `ed25519-dalek`, the inner `SigningKey` implements
/// `ZeroizeOnDrop` — its `Drop` impl overwrites the 32-byte secret seed
/// with zeros before deallocation. Our wrapper inherits this guarantee:
/// when `SigningKey` is dropped, the inner field is dropped, triggering
/// the zeroize.
pub struct SigningKey {
    inner: ed25519_dalek::SigningKey,
}

impl SigningKey {
    /// Access the inner `ed25519_dalek::SigningKey`.
    #[inline]
    pub(crate) fn inner(&self) -> &ed25519_dalek::SigningKey {
        &self.inner
    }

    /// Serialize to the 32-byte Ed25519 secret seed.
    ///
    /// This is intentionally explicit rather than implementing serde for the
    /// signing key. Callers that persist this value should do so through an
    /// encrypted-at-rest secret store.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Reconstruct a signing key from a 32-byte Ed25519 secret seed.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::from_bytes(bytes),
        }
    }

    /// Derive the corresponding verifying (public) key.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            inner: self.inner.verifying_key(),
        }
    }
}

// No manual Drop needed — ed25519_dalek::SigningKey (with `zeroize` feature)
// implements ZeroizeOnDrop, which zeroizes the secret_key bytes in its own
// Drop impl. When our wrapper is dropped, the inner field is dropped,
// triggering the zeroize automatically.

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningKey")
            .field("public", &self.inner.verifying_key())
            .finish_non_exhaustive()
    }
}

/// Ed25519 verifying (public) key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyingKey {
    inner: ed25519_dalek::VerifyingKey,
}

impl VerifyingKey {
    /// Access the inner `ed25519_dalek::VerifyingKey`.
    #[inline]
    pub(crate) fn inner(&self) -> &ed25519_dalek::VerifyingKey {
        &self.inner
    }

    /// Serialize to 32-byte compressed Edwards Y representation.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Deserialize from 32 bytes. Returns `None` if the bytes are not a valid
    /// compressed Edwards Y point.
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        ed25519_dalek::VerifyingKey::from_bytes(bytes)
            .ok()
            .map(|inner| Self { inner })
    }
}

impl PartialEq for VerifyingKey {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for VerifyingKey {}

/// Generate a fresh Ed25519 keypair using OS-provided entropy.
///
/// The signing key wraps `ed25519_dalek::SigningKey` and zeroizes on drop.
/// The verifying key is safe to share publicly.
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let inner = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let verifying = VerifyingKey {
        inner: inner.verifying_key(),
    };
    let signing = SigningKey { inner };
    (signing, verifying)
}
