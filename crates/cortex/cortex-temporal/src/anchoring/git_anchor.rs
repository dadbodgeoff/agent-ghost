//! Git anchor stub (Phase 1 stub, full implementation in Phase 3).

use serde::{Deserialize, Serialize};

/// A record of a Merkle root anchored to a git commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorRecord {
    pub merkle_root: [u8; 32],
    pub git_commit_hash: Option<String>,
    pub anchored_at: String,
    pub event_count: usize,
}

/// Git anchor — stub for Phase 1.
pub struct GitAnchor;

impl GitAnchor {
    pub fn new() -> Self {
        Self
    }

    /// Anchor a Merkle root to a git commit (stub).
    pub fn anchor(&self, _merkle_root: &[u8; 32]) -> AnchorRecord {
        AnchorRecord {
            merkle_root: [0u8; 32],
            git_commit_hash: None,
            anchored_at: chrono::Utc::now().to_rfc3339(),
            event_count: 0,
        }
    }
}

impl Default for GitAnchor {
    fn default() -> Self {
        Self::new()
    }
}
