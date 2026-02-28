use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::memory::{Importance, MemoryType};

/// A memory compressed to one of 4 levels for token-efficient context injection.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CompressedMemory {
    pub memory_id: String,
    pub memory_type: MemoryType,
    pub importance: Importance,
    /// Compression level: 0 (IDs only) through 3 (full context).
    pub level: u8,
    /// The compressed text representation.
    pub text: String,
    /// Actual token count of the compressed text.
    pub token_count: usize,
    /// Relevance score from retrieval ranking.
    pub relevance_score: f64,
}
