use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::traits::IMemoryStorage;

use crate::signals::TemporalSignals;

use super::PredictionCandidate;

/// Temporal prediction strategy.
///
/// Predicts memories based on time-of-day and day-of-week usage patterns.
/// Memories frequently accessed at similar times are predicted.
pub struct TemporalStrategy;

impl TemporalStrategy {
    /// Predict memories based on temporal patterns.
    ///
    /// Uses the time bucket (morning/afternoon/evening/night) as a search tag
    /// and boosts recently accessed memories.
    pub fn predict(
        signals: &TemporalSignals,
        storage: &dyn IMemoryStorage,
    ) -> CortexResult<Vec<PredictionCandidate>> {
        let mut candidates = Vec::new();

        let bucket = signals.time_bucket();

        // Search for memories tagged with the current time bucket
        let tagged = storage.query_by_tags(&[bucket.to_string()])?;
        for memory in &tagged {
            candidates.push(PredictionCandidate {
                memory_id: memory.id.clone(),
                confidence: 0.5,
                source_strategy: "temporal".to_string(),
                signals: vec![format!("time_bucket:{}", bucket)],
            });
        }

        // Also boost frequently accessed memories (high access count = likely needed again)
        let frequent = storage.query_by_importance(cortex_core::memory::Importance::Normal)?;
        for memory in frequent.iter().filter(|m| is_temporally_relevant(m)) {
            if !candidates.iter().any(|c| c.memory_id == memory.id) {
                candidates.push(PredictionCandidate {
                    memory_id: memory.id.clone(),
                    confidence: 0.4,
                    source_strategy: "temporal".to_string(),
                    signals: vec![format!("frequent_access:{}", memory.access_count)],
                });
            }
        }

        Ok(candidates)
    }
}

/// A memory is temporally relevant if it has been accessed recently and frequently.
fn is_temporally_relevant(memory: &BaseMemory) -> bool {
    let days_since_access = (chrono::Utc::now() - memory.last_accessed).num_days();
    memory.access_count >= 5 && days_since_access <= 7
}
