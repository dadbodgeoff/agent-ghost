use cortex_core::errors::CortexResult;
use cortex_core::traits::IMemoryStorage;

use crate::signals::BehavioralSignals;

use super::PredictionCandidate;

/// Behavioral prediction strategy.
///
/// Predicts memories based on recent queries, intents, and frequently accessed memories.
pub struct BehavioralStrategy;

impl BehavioralStrategy {
    /// Predict memories based on recent user behavior.
    pub fn predict(
        signals: &BehavioralSignals,
        storage: &dyn IMemoryStorage,
    ) -> CortexResult<Vec<PredictionCandidate>> {
        let mut candidates = Vec::new();

        if !signals.has_signals() {
            return Ok(candidates);
        }

        // Directly include frequently accessed memories
        if !signals.frequent_memory_ids.is_empty() {
            let memories = storage.get_bulk(&signals.frequent_memory_ids)?;
            for memory in &memories {
                candidates.push(PredictionCandidate {
                    memory_id: memory.id.clone(),
                    confidence: 0.8,
                    source_strategy: "behavioral".to_string(),
                    signals: vec!["frequent_access".to_string()],
                });
            }
        }

        // Search for memories matching recent queries
        for query in signals.recent_queries.iter().take(5) {
            let results = storage.search_fts5(query, 10)?;
            for memory in results {
                if !candidates.iter().any(|c| c.memory_id == memory.id) {
                    candidates.push(PredictionCandidate {
                        memory_id: memory.id.clone(),
                        confidence: 0.6,
                        source_strategy: "behavioral".to_string(),
                        signals: vec![format!("recent_query:{}", query)],
                    });
                }
            }
        }

        // Search for memories matching recent intents
        for intent in signals.recent_intents.iter().take(3) {
            let results = storage.search_fts5(intent, 5)?;
            for memory in results {
                if !candidates.iter().any(|c| c.memory_id == memory.id) {
                    candidates.push(PredictionCandidate {
                        memory_id: memory.id.clone(),
                        confidence: 0.5,
                        source_strategy: "behavioral".to_string(),
                        signals: vec![format!("recent_intent:{}", intent)],
                    });
                }
            }
        }

        Ok(candidates)
    }
}
