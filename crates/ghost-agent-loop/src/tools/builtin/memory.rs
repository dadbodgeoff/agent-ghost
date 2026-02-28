//! Memory tool — Cortex read/write via proposals.
//!
//! Read operations query the read-only pipeline snapshot.
//! Write operations generate proposals routed through ProposalValidator.
//! The agent cannot directly mutate memory — all writes go through
//! the proposal system.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryToolError {
    #[error("memory read failed: {0}")]
    ReadFailed(String),
    #[error("memory write requires proposal: {0}")]
    WriteRequiresProposal(String),
}

/// Memory tool operations.
#[derive(Debug, Clone)]
pub enum MemoryOperation {
    /// Read memories matching a query.
    Read { query: String, limit: usize },
    /// Propose a memory write (routed through ProposalValidator).
    Write {
        content: String,
        memory_type: String,
    },
}

/// Result of a memory read operation.
#[derive(Debug, Clone)]
pub struct MemoryReadResult {
    pub memories: Vec<serde_json::Value>,
    pub total_count: usize,
}

/// Execute a memory read against the snapshot.
///
/// In production, this queries the AgentSnapshot's memory index.
pub fn read_memories(
    query: &str,
    limit: usize,
    snapshot_memories: &[serde_json::Value],
) -> MemoryReadResult {
    // Simple substring match against snapshot memories
    let matched: Vec<serde_json::Value> = snapshot_memories
        .iter()
        .filter(|m| {
            m.to_string()
                .to_lowercase()
                .contains(&query.to_lowercase())
        })
        .take(limit)
        .cloned()
        .collect();

    let total_count = matched.len();
    MemoryReadResult {
        memories: matched,
        total_count,
    }
}
