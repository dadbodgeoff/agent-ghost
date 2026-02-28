//! Signed delta wrapper and sign/verify functions.
//!
//! Uses `ed25519-dalek` directly (NOT `ghost-signing`) per architectural constraint.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, Verifier};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A CRDT delta wrapped with an Ed25519 signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedDelta<T: Serialize> {
    /// The actual delta payload.
    pub delta: T,
    /// Author agent ID.
    pub author: Uuid,
    /// Ed25519 signature over the canonical bytes of the delta.
    #[serde(with = "signature_serde")]
    pub signature: ed25519_dalek::Signature,
    /// Timestamp of signing.
    pub timestamp: DateTime<Utc>,
}

impl<T: Serialize> SignedDelta<T> {
    /// Compute canonical bytes for signing.
    ///
    /// Format: `delta_json || "|" || author_uuid_bytes || "|" || rfc3339_timestamp`
    ///
    /// # Determinism
    /// `serde_json::to_vec` is deterministic for struct types (field order is
    /// fixed by the derive). Callers MUST NOT use `HashMap`-based delta types
    /// as the JSON key ordering would be non-deterministic. Use `BTreeMap` if
    /// map-like deltas are needed.
    fn canonical_bytes(delta: &T, author: Uuid, timestamp: DateTime<Utc>) -> Vec<u8> {
        let mut buf = Vec::new();
        // Delta JSON (deterministic via serde_json)
        let delta_json = serde_json::to_vec(delta).unwrap_or_default();
        buf.extend_from_slice(&delta_json);
        buf.push(b'|');
        buf.extend_from_slice(author.as_bytes());
        buf.push(b'|');
        buf.extend_from_slice(timestamp.to_rfc3339().as_bytes());
        buf
    }
}

/// Sign a delta with an Ed25519 signing key.
pub fn sign_delta<T: Serialize>(
    delta: T,
    author: Uuid,
    key: &ed25519_dalek::SigningKey,
) -> SignedDelta<T> {
    let timestamp = Utc::now();
    let canonical = SignedDelta::<T>::canonical_bytes(&delta, author, timestamp);
    let signature = key.sign(&canonical);
    SignedDelta {
        delta,
        author,
        signature,
        timestamp,
    }
}

/// Verify a signed delta against a public key. Returns `true` if valid.
pub fn verify_delta<T: Serialize>(
    signed: &SignedDelta<T>,
    key: &ed25519_dalek::VerifyingKey,
) -> bool {
    let canonical =
        SignedDelta::<T>::canonical_bytes(&signed.delta, signed.author, signed.timestamp);
    key.verify(&canonical, &signed.signature).is_ok()
}

/// Serde support for ed25519_dalek::Signature (64-byte array).
mod signature_serde {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        sig: &ed25519_dalek::Signature,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let bytes = sig.to_bytes();
        serializer.serialize_bytes(&bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ed25519_dalek::Signature, D::Error> {
        use serde::de::{self, Visitor};

        struct SigVisitor;

        impl<'de> Visitor<'de> for SigVisitor {
            type Value = ed25519_dalek::Signature;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("64 bytes for an Ed25519 signature")
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                let arr: [u8; 64] = v
                    .try_into()
                    .map_err(|_| E::invalid_length(v.len(), &"64 bytes"))?;
                Ok(ed25519_dalek::Signature::from_bytes(&arr))
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut bytes = [0u8; 64];
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(i, &"64 bytes"))?;
                }
                Ok(ed25519_dalek::Signature::from_bytes(&bytes))
            }
        }

        deserializer.deserialize_bytes(SigVisitor)
    }
}
