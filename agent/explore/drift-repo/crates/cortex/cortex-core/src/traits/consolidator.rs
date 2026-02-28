use crate::errors::CortexResult;
use crate::memory::BaseMemory;
use crate::models::ConsolidationResult;

/// Memory consolidation — merging episodic memories into semantic knowledge.
pub trait IConsolidator: Send + Sync {
    /// Consolidate a set of candidate memories.
    fn consolidate(&self, candidates: &[BaseMemory]) -> CortexResult<ConsolidationResult>;
}
