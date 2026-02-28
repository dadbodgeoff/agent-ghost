//! Bitemporal validation rules.

use chrono::{DateTime, Utc};

use cortex_core::errors::{CortexError, CortexResult};
use cortex_core::memory::BaseMemory;

/// Validate that transaction_time is not being modified.
///
/// transaction_time is immutable once written — it represents when we learned
/// about something, which cannot change retroactively.
///
/// Returns error if old and new transaction_time differ.
pub fn validate_transaction_time_immutability(
    old_transaction_time: DateTime<Utc>,
    new_transaction_time: DateTime<Utc>,
) -> CortexResult<()> {
    if old_transaction_time != new_transaction_time {
        return Err(CortexError::TemporalError(
            cortex_core::errors::TemporalError::ImmutableFieldViolation(format!(
                "transaction_time cannot be modified (was {}, attempted {})",
                old_transaction_time.to_rfc3339(),
                new_transaction_time.to_rfc3339()
            )),
        ));
    }
    Ok(())
}

/// Validate temporal bounds: valid_time <= valid_until.
///
/// A memory cannot end being valid before it started being valid.
pub fn validate_temporal_bounds(memory: &BaseMemory) -> CortexResult<()> {
    if let Some(valid_until) = memory.valid_until {
        if memory.valid_time > valid_until {
            return Err(CortexError::TemporalError(
                cortex_core::errors::TemporalError::InvalidTemporalBounds(format!(
                    "valid_time ({}) must be <= valid_until ({})",
                    memory.valid_time.to_rfc3339(),
                    valid_until.to_rfc3339()
                )),
            ));
        }
    }
    Ok(())
}
