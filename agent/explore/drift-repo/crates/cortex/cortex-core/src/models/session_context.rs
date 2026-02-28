use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use ts_rs::TS;

/// Session state for deduplication and token tracking.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionContext {
    pub session_id: String,
    /// Memory IDs already sent in this session.
    pub sent_memory_ids: HashSet<String>,
    /// Total tokens used in this session.
    pub tokens_used: usize,
    /// Token budget for this session.
    pub token_budget: usize,
}
