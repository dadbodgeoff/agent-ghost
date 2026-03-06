//! Git anchor — Merkle root attestation via git notes (Phase 3, T-6.8.1).
//!
//! Writes the hex-encoded Merkle root as a git note on HEAD using
//! `refs/notes/ghost-anchors`. Does NOT create commits or modify the working tree.
//!
//! Verification: `git notes --ref=ghost-anchors show <commit>` returns the root.

use serde::{Deserialize, Serialize};

/// A record of a Merkle root anchored to a git commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorRecord {
    pub merkle_root: [u8; 32],
    pub git_commit_hash: Option<String>,
    pub anchored_at: String,
    pub event_count: usize,
}

/// Git anchor — attaches Merkle roots to git commits via notes.
pub struct GitAnchor;

impl GitAnchor {
    pub fn new() -> Self {
        Self
    }

    /// Anchor a Merkle root to the current HEAD commit via git notes.
    ///
    /// Uses `refs/notes/ghost-anchors` as the notes ref to avoid colliding
    /// with user notes. The note content is the hex-encoded Merkle root.
    ///
    /// Returns an error if no git repository is found or HEAD is unborn.
    #[cfg(feature = "git-anchor")]
    pub fn anchor(
        &self,
        merkle_root: &[u8; 32],
        event_count: usize,
    ) -> Result<AnchorRecord, AnchorError> {
        // Open the repository at the current directory or any parent.
        let repo = git2::Repository::discover(".").map_err(|e| {
            AnchorError::NoRepository(format!("failed to discover git repo: {e}"))
        })?;

        // Resolve HEAD to a commit.
        let head = repo.head().map_err(|e| {
            AnchorError::NoHead(format!("failed to resolve HEAD: {e}"))
        })?;
        let commit = head.peel_to_commit().map_err(|e| {
            AnchorError::NoHead(format!("failed to peel HEAD to commit: {e}"))
        })?;
        let commit_hash = commit.id().to_string();

        // Write the Merkle root as a git note on the commit.
        let root_hex = hex_encode(merkle_root);
        let sig = repo.signature().unwrap_or_else(|_| {
            git2::Signature::now("ghost-anchor", "ghost@local").expect("valid signature")
        });

        // Create or overwrite the note on refs/notes/ghost-anchors.
        repo.note(
            &sig,
            &sig,
            Some("refs/notes/ghost-anchors"),
            commit.id(),
            &root_hex,
            true, // force — overwrite existing note for this commit
        )
        .map_err(|e| {
            AnchorError::WriteFailed(format!("failed to write git note: {e}"))
        })?;

        Ok(AnchorRecord {
            merkle_root: *merkle_root,
            git_commit_hash: Some(commit_hash),
            anchored_at: chrono::Utc::now().to_rfc3339(),
            event_count,
        })
    }

    /// Stub anchor when the `git-anchor` feature is not enabled.
    #[cfg(not(feature = "git-anchor"))]
    pub fn anchor(
        &self,
        merkle_root: &[u8; 32],
        event_count: usize,
    ) -> Result<AnchorRecord, AnchorError> {
        let _ = (merkle_root, event_count);
        Err(AnchorError::NotAvailable(
            "git-anchor feature not enabled — compile with --features git-anchor".into(),
        ))
    }
}

impl Default for GitAnchor {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors from git anchoring.
#[derive(Debug, thiserror::Error)]
pub enum AnchorError {
    #[error("no git repository found: {0}")]
    NoRepository(String),
    #[error("HEAD is unborn or detached: {0}")]
    NoHead(String),
    #[error("failed to write git note: {0}")]
    WriteFailed(String),
    #[error("git anchor not available: {0}")]
    NotAvailable(String),
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
