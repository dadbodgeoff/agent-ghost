//! Pre-compute hybrid search results for predicted queries.
//!
//! Triggered on file change events. Pre-computes search results so
//! retrieval is near-instant when the prediction is consumed.

use cortex_core::errors::CortexResult;
use cortex_core::traits::IMemoryStorage;

use crate::cache::PredictionCache;
use crate::signals::FileSignals;
use crate::strategies::{self, FileBasedStrategy, PatternBasedStrategy, PredictionCandidate};

/// Pre-compute predictions for a file change event.
///
/// Runs file-based and pattern-based strategies for the changed file
/// and stores results in the prediction cache.
pub fn precompute_for_file_change(
    file_path: &str,
    imports: Vec<String>,
    symbols: Vec<String>,
    storage: &dyn IMemoryStorage,
    cache: &PredictionCache,
) -> CortexResult<usize> {
    // Invalidate stale cache for this file
    cache.invalidate_file(file_path);

    let signals = FileSignals::gather(Some(file_path), imports, symbols);

    let mut all_candidates: Vec<PredictionCandidate> = Vec::new();

    // Run file-based strategy
    let file_candidates = FileBasedStrategy::predict(&signals, storage)?;
    all_candidates.extend(file_candidates);

    // Run pattern-based strategy
    let pattern_candidates = PatternBasedStrategy::predict(&signals, storage)?;
    all_candidates.extend(pattern_candidates);

    // Deduplicate across strategies
    let deduped = strategies::deduplicate(all_candidates);
    let count = deduped.len();

    // Cache the results
    cache.insert(file_path.to_string(), deduped, 0.0);

    Ok(count)
}
