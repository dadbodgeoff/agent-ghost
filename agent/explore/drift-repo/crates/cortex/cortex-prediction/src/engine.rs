//! PredictionEngine — coordinates strategies, deduplicates, manages cache.
//!
//! Implements `IPredictor` from cortex-core.

use cortex_core::errors::CortexResult;
use cortex_core::models::PredictionResult;
use cortex_core::traits::{IMemoryStorage, IPredictor, PredictionSignals};

use crate::cache::PredictionCache;
use crate::signals::AggregatedSignals;
use crate::strategies::{
    self, BehavioralStrategy, FileBasedStrategy, PatternBasedStrategy, PredictionCandidate,
    TemporalStrategy,
};

/// Prediction engine coordinating all 4 strategies with caching and dedup.
pub struct PredictionEngine<S: IMemoryStorage> {
    storage: S,
    cache: PredictionCache,
}

impl<S: IMemoryStorage> PredictionEngine<S> {
    /// Create a new PredictionEngine.
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            cache: PredictionCache::new(),
        }
    }

    /// Get a reference to the prediction cache.
    pub fn cache(&self) -> &PredictionCache {
        &self.cache
    }

    /// Run all 4 prediction strategies with full aggregated signals.
    pub fn predict_with_signals(
        &self,
        signals: &AggregatedSignals,
    ) -> CortexResult<Vec<PredictionCandidate>> {
        // F-07: Build a cache key that includes both the active file AND query context
        // to avoid collisions when the same file (or no file) is used with different queries.
        let file_part = signals.file.active_file.as_deref().unwrap_or("__no_active_file__");
        let imports_hash = signals.file.imports.len();
        let cache_key = format!("{file_part}:{imports_hash}");
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(cached);
        }

        let mut all_candidates: Vec<PredictionCandidate> = Vec::new();

        // Strategy 1: File-based
        let file_candidates = FileBasedStrategy::predict(&signals.file, &self.storage)?;
        all_candidates.extend(file_candidates);

        // Strategy 2: Pattern-based
        let pattern_candidates = PatternBasedStrategy::predict(&signals.file, &self.storage)?;
        all_candidates.extend(pattern_candidates);

        // Strategy 3: Temporal
        let temporal_candidates = TemporalStrategy::predict(&signals.temporal, &self.storage)?;
        all_candidates.extend(temporal_candidates);

        // Strategy 4: Behavioral
        let behavioral_candidates =
            BehavioralStrategy::predict(&signals.behavioral, &self.storage)?;
        all_candidates.extend(behavioral_candidates);

        // Deduplicate across strategies
        let deduped = strategies::deduplicate(all_candidates);

        // Cache the results
        self.cache
            .insert(cache_key.to_string(), deduped.clone(), 0.0);

        Ok(deduped)
    }

    /// Invalidate cache for a file change.
    pub fn on_file_changed(&self, file_path: &str) {
        self.cache.invalidate_file(file_path);
    }

    /// Invalidate all cache entries (e.g., new session).
    pub fn on_new_session(&self) {
        self.cache.invalidate_all();
    }
}

impl<S: IMemoryStorage> IPredictor for PredictionEngine<S> {
    fn predict(&self, signals: &PredictionSignals) -> CortexResult<PredictionResult> {
        // Convert PredictionSignals → AggregatedSignals
        let aggregated = AggregatedSignals {
            file: crate::signals::FileSignals {
                active_file: signals.active_files.first().cloned(),
                imports: signals.active_files.get(1..).unwrap_or_default().to_vec(),
                symbols: vec![],
                directory: signals.active_files.first().and_then(|f| {
                    std::path::Path::new(f)
                        .parent()
                        .map(|p| p.to_string_lossy().into_owned())
                }),
            },
            behavioral: crate::signals::BehavioralSignals {
                recent_queries: signals.recent_queries.clone(),
                recent_intents: signals
                    .current_intent
                    .as_ref()
                    .map(|i| vec![i.clone()])
                    .unwrap_or_default(),
                frequent_memory_ids: vec![],
            },
            ..Default::default()
        };

        let candidates = self.predict_with_signals(&aggregated)?;

        Ok(PredictionResult {
            memory_ids: candidates.iter().map(|c| c.memory_id.clone()).collect(),
            signals: candidates.iter().flat_map(|c| c.signals.clone()).collect(),
            confidence: candidates.first().map(|c| c.confidence).unwrap_or(0.0),
        })
    }
}
