//! Privacy levels and SHA-256 content hashing (Req 4 AC2).
//!
//! Uses SHA-256 (sha2 crate) for content hashing — NOT blake3.
//! blake3 is used only for hash chains in cortex-temporal.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Privacy level for ITP events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrivacyLevel {
    /// Hash all content fields. No plaintext.
    Minimal,
    /// Include plaintext for vocabulary analysis.
    Standard,
    /// Full plaintext for all fields.
    Full,
    /// Full plaintext + additional research metadata.
    Research,
}

/// Hash content using SHA-256 for privacy protection.
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Simple hex encoding (no external dep needed).
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

/// Apply privacy level to content: returns (hash, optional_plaintext).
pub fn apply_privacy(content: &str, level: PrivacyLevel) -> (String, Option<String>) {
    let hash = hash_content(content);
    let plaintext = match level {
        PrivacyLevel::Minimal => None,
        PrivacyLevel::Standard | PrivacyLevel::Full | PrivacyLevel::Research => {
            Some(content.to_string())
        }
    };
    (hash, plaintext)
}
