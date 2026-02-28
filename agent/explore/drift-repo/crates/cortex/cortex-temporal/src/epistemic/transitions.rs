//! Epistemic status transitions — strict state machine with validation.
//!
//! Valid transitions:
//! - Conjecture → Provisional (on validation pass with evidence)
//! - Provisional → Verified (on user confirmation or multi-agent corroboration)
//! - Verified → Stale (on evidence freshness drop below threshold)
//!
//! Invalid transitions (all return InvalidEpistemicTransition):
//! - Conjecture → Verified (no skipping steps)
//! - Verified → Provisional (no backward transitions)
//! - Provisional → Stale (Stale only comes from Verified)
//! - Stale → Verified (must re-promote through the full path)

use chrono::Utc;

use cortex_core::errors::TemporalError;
use cortex_core::models::EpistemicStatus;
use cortex_core::CortexError;

/// Promote from Conjecture to Provisional.
///
/// Only valid from Conjecture. Any other source state returns an error.
pub fn promote_to_provisional(
    current: &EpistemicStatus,
    evidence_count: u32,
) -> Result<EpistemicStatus, CortexError> {
    match current {
        EpistemicStatus::Conjecture { .. } => Ok(EpistemicStatus::Provisional {
            evidence_count,
            last_validated: Utc::now(),
        }),
        _ => Err(CortexError::TemporalError(
            TemporalError::InvalidEpistemicTransition {
                from: current.variant_name().to_string(),
                to: "provisional".to_string(),
            },
        )),
    }
}

/// Promote from Provisional to Verified.
///
/// Only valid from Provisional. Conjecture→Verified is REJECTED (no skipping).
/// Stale→Verified is REJECTED.
pub fn promote_to_verified(
    current: &EpistemicStatus,
    verified_by: Vec<String>,
    evidence_refs: Vec<String>,
) -> Result<EpistemicStatus, CortexError> {
    match current {
        EpistemicStatus::Provisional { .. } => Ok(EpistemicStatus::Verified {
            verified_by,
            verified_at: Utc::now(),
            evidence_refs,
        }),
        _ => Err(CortexError::TemporalError(
            TemporalError::InvalidEpistemicTransition {
                from: current.variant_name().to_string(),
                to: "verified".to_string(),
            },
        )),
    }
}

/// Demote from Verified to Stale.
///
/// Only valid from Verified. Conjecture→Stale and Provisional→Stale are REJECTED.
pub fn demote_to_stale(
    current: &EpistemicStatus,
    reason: String,
) -> Result<EpistemicStatus, CortexError> {
    match current {
        EpistemicStatus::Verified { verified_at, .. } => Ok(EpistemicStatus::Stale {
            was_verified_at: *verified_at,
            staleness_detected_at: Utc::now(),
            reason,
        }),
        _ => Err(CortexError::TemporalError(
            TemporalError::InvalidEpistemicTransition {
                from: current.variant_name().to_string(),
                to: "stale".to_string(),
            },
        )),
    }
}
