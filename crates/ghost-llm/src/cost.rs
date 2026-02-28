//! Cost calculation with pre/post estimation (Req 21 AC5).

use crate::provider::{TokenPricing, UsageStats};

/// Pre-call cost estimate and post-call actual cost.
#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub estimated_input_cost: f64,
    pub estimated_output_cost: f64,
    pub estimated_total: f64,
}

#[derive(Debug, Clone)]
pub struct CostActual {
    pub input_cost: f64,
    pub output_cost: f64,
    pub total: f64,
}

/// Calculates LLM call costs.
pub struct CostCalculator;

impl CostCalculator {
    /// Estimate cost before a call.
    pub fn estimate(
        input_tokens: usize,
        estimated_output_tokens: usize,
        pricing: &TokenPricing,
    ) -> CostEstimate {
        let input_cost = (input_tokens as f64 / 1000.0) * pricing.input_per_1k;
        let output_cost = (estimated_output_tokens as f64 / 1000.0) * pricing.output_per_1k;
        CostEstimate {
            estimated_input_cost: input_cost,
            estimated_output_cost: output_cost,
            estimated_total: input_cost + output_cost,
        }
    }

    /// Calculate actual cost after a call.
    pub fn actual(usage: &UsageStats, pricing: &TokenPricing) -> CostActual {
        let input_cost = (usage.prompt_tokens as f64 / 1000.0) * pricing.input_per_1k;
        let output_cost = (usage.completion_tokens as f64 / 1000.0) * pricing.output_per_1k;
        CostActual {
            input_cost,
            output_cost,
            total: input_cost + output_cost,
        }
    }
}
