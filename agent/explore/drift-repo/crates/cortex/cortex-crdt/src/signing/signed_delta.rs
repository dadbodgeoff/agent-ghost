//! Signed delta wrapper — a MemoryDelta with an Ed25519 signature.

use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

use crate::memory::merge_engine::MemoryDelta;

/// A MemoryDelta with a cryptographic signature from the authoring agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedDelta {
    pub delta: MemoryDelta,
    /// Ed25519 signature over blake3(delta serialized bytes).
    pub signature: Vec<u8>,
    /// blake3 hash of the serialized delta (the signed payload).
    pub content_hash: String,
}

impl SignedDelta {
    /// Create a signed delta from a delta and signing key.
    pub fn sign(delta: MemoryDelta, signing_key: &SigningKey) -> Self {
        let serialized = serde_json::to_vec(&delta).expect("delta serialization");
        let content_hash = blake3::hash(&serialized).to_hex().to_string();
        let signature = signing_key.sign(&serialized);
        Self {
            delta,
            signature: signature.to_bytes().to_vec(),
            content_hash,
        }
    }

    /// Extract the raw bytes that were signed (for verification).
    pub fn signed_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.delta).expect("delta serialization")
    }
}
