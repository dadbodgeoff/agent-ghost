use cortex_core::memory::{half_life_days, BaseMemory};

/// Temporal decay factor: `e^(-daysSinceAccess / halfLife)`.
///
/// Range: 0.0 – 1.0.
/// Memories with infinite half-life (Core) return 1.0 (no temporal decay).
pub fn calculate(memory: &BaseMemory, now: chrono::DateTime<chrono::Utc>) -> f64 {
    let half_life = match half_life_days(memory.memory_type) {
        Some(days) => days as f64,
        None => return 1.0, // Infinite half-life — no decay.
    };

    let days_since_access = (now - memory.last_accessed).num_seconds().max(0) as f64 / 86400.0;

    (-days_since_access / half_life).exp()
}
