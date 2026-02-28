//! Late-arriving fact handling.
//!
//! When we learn about something that was true in the past, we record:
//! - transaction_time = now (when we learned it)
//! - valid_time = past (when it was actually true)

use chrono::Utc;

use cortex_core::errors::{CortexError, CortexResult};
use cortex_core::memory::BaseMemory;

/// Handle a late-arriving fact.
///
/// Sets transaction_time = now (when we learned it) and valid_time = actual_valid_time
/// (when it was actually true).
///
/// Validates that valid_time < transaction_time (late discovery quadrant).
pub fn handle_late_arriving_fact(
    mut memory: BaseMemory,
    actual_valid_time: chrono::DateTime<chrono::Utc>,
) -> CortexResult<BaseMemory> {
    let now = Utc::now();

    // Validate late arrival: valid_time must be in the past
    if actual_valid_time >= now {
        return Err(CortexError::TemporalError(
            cortex_core::errors::TemporalError::InvalidTemporalBounds(format!(
                "Late-arriving fact must have valid_time < transaction_time (got valid_time={}, now={})",
                actual_valid_time.to_rfc3339(),
                now.to_rfc3339()
            )),
        ));
    }

    memory.transaction_time = now;
    memory.valid_time = actual_valid_time;

    Ok(memory)
}
