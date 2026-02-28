use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::compressed_memory::CompressedMemory;

/// Context assembled for LLM generation with budget allocation and provenance.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GenerationContext {
    /// Memories organized by allocation category.
    pub allocations: Vec<BudgetAllocation>,
    /// Total tokens used.
    pub total_tokens: usize,
    /// Total budget available.
    pub total_budget: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BudgetAllocation {
    pub category: String,
    /// Percentage of budget allocated to this category.
    pub percentage: f64,
    pub memories: Vec<CompressedMemory>,
    pub tokens_used: usize,
}
