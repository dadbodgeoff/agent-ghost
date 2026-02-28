use cortex_core::errors::CortexResult;
use cortex_core::traits::IMemoryStorage;

use crate::signals::FileSignals;

use super::PredictionCandidate;

/// Pattern-based prediction strategy.
///
/// Returns memories linked to detected patterns in the active file.
pub struct PatternBasedStrategy;

impl PatternBasedStrategy {
    /// Predict memories linked to patterns found in the active file's symbols.
    pub fn predict(
        signals: &FileSignals,
        storage: &dyn IMemoryStorage,
    ) -> CortexResult<Vec<PredictionCandidate>> {
        let mut candidates = Vec::new();

        if signals.symbols.is_empty() {
            return Ok(candidates);
        }

        // Search for memories related to detected symbols/patterns
        for symbol in &signals.symbols {
            let memories = storage.search_fts5(symbol, 10)?;
            for memory in memories {
                // Only include if the memory has pattern links
                let has_pattern_link = !memory.linked_patterns.is_empty();
                let confidence = if has_pattern_link { 0.7 } else { 0.4 };

                if !candidates.iter().any(|c| c.memory_id == memory.id) {
                    candidates.push(PredictionCandidate {
                        memory_id: memory.id.clone(),
                        confidence,
                        source_strategy: "pattern_based".to_string(),
                        signals: vec![format!("symbol:{}", symbol)],
                    });
                }
            }
        }

        Ok(candidates)
    }
}
