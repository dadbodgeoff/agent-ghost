use crate::errors::CortexResult;
use crate::memory::BaseMemory;
use crate::models::ValidationResult;

/// 4-dimension memory validation.
pub trait IValidator: Send + Sync {
    /// Validate a memory across all dimensions, returning scores and healing actions.
    fn validate(&self, memory: &BaseMemory) -> CortexResult<ValidationResult>;
}
