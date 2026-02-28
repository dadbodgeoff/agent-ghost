use crate::errors::CortexResult;
use crate::models::{CompressedMemory, RetrievalContext};

/// Context-aware memory retrieval.
pub trait IRetriever: Send + Sync {
    /// Retrieve memories relevant to the given context, compressed to fit within budget.
    fn retrieve(
        &self,
        context: &RetrievalContext,
        budget: usize,
    ) -> CortexResult<Vec<CompressedMemory>>;
}
