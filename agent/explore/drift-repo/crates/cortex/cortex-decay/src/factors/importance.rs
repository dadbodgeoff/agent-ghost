use cortex_core::memory::BaseMemory;

/// Importance anchor factor.
///
/// Multiplier based on memory importance level:
/// - Critical: 2.0×
/// - High: 1.5×
/// - Normal: 1.0×
/// - Low: 0.8×
///
/// Range: 0.8 – 2.0.
pub fn calculate(memory: &BaseMemory) -> f64 {
    memory.importance.weight()
}
