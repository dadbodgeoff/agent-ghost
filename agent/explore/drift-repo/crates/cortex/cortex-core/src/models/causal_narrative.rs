use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A generated narrative explaining causal relationships.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CausalNarrative {
    pub sections: Vec<NarrativeSection>,
    pub summary: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NarrativeSection {
    pub title: String,
    pub content: String,
    pub memory_ids: Vec<String>,
}
