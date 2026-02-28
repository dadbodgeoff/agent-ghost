use crate::errors::CortexResult;
use crate::memory::BaseMemory;
use crate::models::CompressedMemory;

/// Hierarchical memory compression (4 levels).
pub trait ICompressor: Send + Sync {
    /// Compress a memory to the specified level (0–3).
    fn compress(&self, memory: &BaseMemory, level: u8) -> CortexResult<CompressedMemory>;

    /// Compress a memory to fit within a token budget.
    fn compress_to_fit(
        &self,
        memory: &BaseMemory,
        max_tokens: usize,
    ) -> CortexResult<CompressedMemory>;

    /// Compress a batch of memories to fit within a total token budget.
    fn compress_batch_to_fit(
        &self,
        memories: &[BaseMemory],
        budget: usize,
    ) -> CortexResult<Vec<CompressedMemory>>;
}
