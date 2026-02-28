//! Ed25519 signing.

use ed25519_dalek::Signer as _;
use serde::{Deserialize, Serialize};

use crate::keypair::SigningKey;

/// A 64-byte Ed25519 signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    inner: ed25519_dalek::Signature,
}

impl Signature {
    /// Raw 64-byte representation.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.inner.to_bytes()
    }

    /// Reconstruct from 64 bytes. Returns `None` if the slice length is wrong.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 64 {
            return None;
        }
        let mut buf = [0u8; 64];
        buf.copy_from_slice(bytes);
        Some(Self {
            inner: ed25519_dalek::Signature::from_bytes(&buf),
        })
    }

    /// Access the inner `ed25519_dalek::Signature`.
    #[inline]
    pub(crate) fn inner(&self) -> &ed25519_dalek::Signature {
        &self.inner
    }
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for Signature {}

/// Sign arbitrary bytes with an Ed25519 signing key.
///
/// Returns a deterministic 64-byte signature (Ed25519 is deterministic —
/// the same key + message always produces the same signature).
pub fn sign(data: &[u8], key: &SigningKey) -> Signature {
    Signature {
        inner: key.inner().sign(data),
    }
}
