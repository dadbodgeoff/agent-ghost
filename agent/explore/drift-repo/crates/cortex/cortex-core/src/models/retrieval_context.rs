use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::intent::Intent;

/// Context for a retrieval request.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RetrievalContext {
    /// The user's query or focus area.
    pub focus: String,
    /// Detected intent.
    pub intent: Option<Intent>,
    /// Active files in the editor.
    pub active_files: Vec<String>,
    /// Token budget for the response.
    pub budget: usize,
    /// Memory IDs already sent in this session (for dedup).
    pub sent_ids: Vec<String>,
}
