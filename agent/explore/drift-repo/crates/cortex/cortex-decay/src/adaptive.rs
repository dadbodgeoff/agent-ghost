use cortex_core::memory::{half_life_days, BaseMemory};

/// Per-memory adaptive half-life computation.
///
/// Instead of fixed type-based half-lives, each memory gets a personalized
/// half-life based on its usage patterns:
///
/// ```text
/// adaptiveHalfLife = baseHalfLife × accessFrequencyFactor × validationFactor × linkageFactor
/// ```
pub fn adaptive_half_life(memory: &BaseMemory) -> Option<f64> {
    let base = half_life_days(memory.memory_type)? as f64;

    let access_factor = access_frequency_factor(memory);
    let validation_factor = validation_factor(memory);
    let linkage_factor = linkage_factor(memory);

    Some(base * access_factor * validation_factor * linkage_factor)
}

/// Access frequency factor: 1.0 – 2.0×.
/// Frequently accessed memories decay slower.
///
/// Uses log scale of access count to determine the factor.
fn access_frequency_factor(memory: &BaseMemory) -> f64 {
    // Scale: 0 accesses = 1.0, 100+ accesses = 2.0
    let factor = 1.0 + (memory.access_count as f64 + 1.0).log10() / 2.0;
    factor.clamp(1.0, 2.0)
}

/// Validation factor: 1.0 – 1.5×.
/// Recently validated memories (high confidence) decay slower.
fn validation_factor(memory: &BaseMemory) -> f64 {
    // Higher confidence = more recently validated = slower decay.
    let conf = memory.confidence.value();
    1.0 + (conf * 0.5) // conf=1.0 → 1.5, conf=0.0 → 1.0
}

/// Linkage factor: 1.0 – 1.3×.
/// Memories linked to active patterns/files decay slower.
fn linkage_factor(memory: &BaseMemory) -> f64 {
    let link_count = memory.linked_patterns.len()
        + memory.linked_files.len()
        + memory.linked_functions.len()
        + memory.linked_constraints.len();

    if link_count == 0 {
        return 1.0;
    }

    // Scale: 1 link = 1.06, 5+ links = 1.3
    let factor = 1.0 + (link_count as f64).min(5.0) * 0.06;
    factor.min(1.3)
}
