use cortex_core::memory::BaseMemory;

/// Pattern linkage boost factor.
///
/// Memories linked to active patterns get a 1.3× boost.
/// Memories with no pattern links get 1.0× (no boost).
///
/// Range: 1.0 – 1.3.
pub fn calculate(memory: &BaseMemory, has_active_patterns: bool) -> f64 {
    if !memory.linked_patterns.is_empty() && has_active_patterns {
        1.3
    } else {
        1.0
    }
}
