use cortex_core::memory::BaseMemory;

/// Usage frequency boost factor.
///
/// Formula: `min(1.5, 1 + log10(accessCount + 1) × 0.2)`
/// Range: 1.0 – 1.5 (capped).
///
/// Frequently accessed memories decay slower.
pub fn calculate(memory: &BaseMemory) -> f64 {
    let boost = 1.0 + ((memory.access_count as f64 + 1.0).log10() * 0.2);
    boost.min(1.5)
}
