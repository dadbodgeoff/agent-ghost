use crate::errors::CortexResult;
use crate::memory::BaseMemory;

/// Confidence decay calculation.
pub trait IDecayEngine: Send + Sync {
    /// Calculate the new confidence for a memory after decay.
    /// Returns a value in [0.0, 1.0].
    fn calculate(&self, memory: &BaseMemory) -> CortexResult<f64>;
}
