use serde::{Deserialize, Serialize};

/// Signals derived from recent user behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BehavioralSignals {
    /// Recent query strings (most recent first).
    pub recent_queries: Vec<String>,
    /// Recent intent classifications.
    pub recent_intents: Vec<String>,
    /// Memory IDs that have been frequently accessed.
    pub frequent_memory_ids: Vec<String>,
}

impl BehavioralSignals {
    /// Gather behavioral signals from recent activity.
    pub fn gather(
        recent_queries: Vec<String>,
        recent_intents: Vec<String>,
        frequent_memory_ids: Vec<String>,
    ) -> Self {
        Self {
            recent_queries,
            recent_intents,
            frequent_memory_ids,
        }
    }

    /// Returns true if there is any behavioral signal data.
    pub fn has_signals(&self) -> bool {
        !self.recent_queries.is_empty()
            || !self.recent_intents.is_empty()
            || !self.frequent_memory_ids.is_empty()
    }
}
