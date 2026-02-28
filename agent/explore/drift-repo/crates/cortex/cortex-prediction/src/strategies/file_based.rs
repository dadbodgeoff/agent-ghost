use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::traits::IMemoryStorage;

use crate::signals::FileSignals;

use super::PredictionCandidate;

/// File-based prediction strategy.
///
/// Returns memories linked to the active file and its imports.
pub struct FileBasedStrategy;

impl FileBasedStrategy {
    /// Predict memories linked to the active file and its imports.
    pub fn predict(
        signals: &FileSignals,
        storage: &dyn IMemoryStorage,
    ) -> CortexResult<Vec<PredictionCandidate>> {
        let mut candidates = Vec::new();

        // Gather all relevant file paths
        let paths = signals.relevant_paths();
        if paths.is_empty() {
            return Ok(candidates);
        }

        // For each relevant path, find memories linked to that file
        for path in &paths {
            let memories = storage.search_fts5(path, 20)?;
            for memory in memories {
                let confidence = compute_file_confidence(&memory, signals);
                candidates.push(PredictionCandidate {
                    memory_id: memory.id.clone(),
                    confidence,
                    source_strategy: "file_based".to_string(),
                    signals: vec![format!("linked_file:{}", path)],
                });
            }
        }

        // Also include memories directly linked to the active file via FileLink
        if let Some(ref active) = signals.active_file {
            let tag_results = storage.query_by_tags(std::slice::from_ref(active))?;
            for memory in tag_results {
                if !candidates.iter().any(|c| c.memory_id == memory.id) {
                    candidates.push(PredictionCandidate {
                        memory_id: memory.id.clone(),
                        confidence: 0.7,
                        source_strategy: "file_based".to_string(),
                        signals: vec![format!("tagged_file:{}", active)],
                    });
                }
            }
        }

        Ok(candidates)
    }
}

/// Compute confidence for a file-based prediction.
fn compute_file_confidence(memory: &BaseMemory, signals: &FileSignals) -> f64 {
    let mut score = 0.5;

    // Boost if memory is directly linked to the active file
    if let Some(ref active) = signals.active_file {
        if memory.linked_files.iter().any(|f| f.file_path == *active) {
            score += 0.3;
        }
    }

    // Boost for import matches
    let import_matches = memory
        .linked_files
        .iter()
        .filter(|f| signals.imports.contains(&f.file_path))
        .count();
    score += (import_matches as f64 * 0.1).min(0.2);

    score.min(1.0)
}
