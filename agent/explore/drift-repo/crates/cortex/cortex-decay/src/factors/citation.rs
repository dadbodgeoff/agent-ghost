use cortex_core::memory::BaseMemory;

/// Citation freshness factor.
///
/// Compares linked file content hashes to detect stale citations.
/// Range: 0.5 – 1.0.
///
/// - All citations fresh (or no citations): 1.0
/// - Some stale: proportional reduction, minimum 0.5
pub fn calculate(memory: &BaseMemory, stale_citation_ratio: f64) -> f64 {
    if memory.linked_files.is_empty() {
        return 1.0;
    }

    // Linear interpolation: 1.0 at 0% stale, 0.5 at 100% stale.
    let ratio = stale_citation_ratio.clamp(0.0, 1.0);
    1.0 - (ratio * 0.5)
}
