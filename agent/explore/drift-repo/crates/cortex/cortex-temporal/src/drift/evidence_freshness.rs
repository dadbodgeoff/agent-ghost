//! Evidence freshness — per-evidence-type freshness factors and aggregation.
//!
//! Freshness factors:
//! - File link: content_hash match → 1.0, mismatch → 0.5
//! - Pattern link: active → 1.0, inactive → 0.3
//! - Supporting memory: that memory's confidence value
//! - User validation: exponential decay with half-life 90 days
//!
//! Aggregation: product of all factors (Π). Empty evidence → 1.0.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::params;

use cortex_core::errors::CortexResult;
use cortex_core::memory::{FileLink, MemoryType, PatternLink};
use cortex_storage::pool::ReadPool;

/// Compute freshness for a single file link.
/// content_hash match → 1.0, mismatch → 0.5.
pub fn file_link_freshness(link: &FileLink, current_hash: Option<&str>) -> f64 {
    match (&link.content_hash, current_hash) {
        (Some(stored), Some(current)) if stored == current => 1.0,
        (Some(_), Some(_)) => 0.5,
        (None, _) | (_, None) => 0.5, // Missing hash → assume stale
    }
}

/// Compute freshness for a single pattern link.
/// active → 1.0, inactive → 0.3.
pub fn pattern_link_freshness(link: &PatternLink, is_active: bool) -> f64 {
    let _ = link; // Used for identification, freshness depends on active status
    if is_active {
        1.0
    } else {
        0.3
    }
}

/// Compute freshness for a supporting memory reference.
/// Returns the supporting memory's confidence value.
pub fn supporting_memory_freshness(supporting_confidence: f64) -> f64 {
    supporting_confidence.clamp(0.0, 1.0)
}

/// Compute freshness for a user validation.
/// Exponential decay with half-life 90 days: exp(-days/90 * 0.693).
pub fn user_validation_freshness(validated_at: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let days = now
        .signed_duration_since(validated_at)
        .num_seconds() as f64
        / 86400.0;
    if days <= 0.0 {
        return 1.0;
    }
    let freshness = (-days / 90.0 * 0.693_f64).exp();
    freshness.clamp(0.0, 1.0)
}

/// Compute aggregate evidence freshness for a single memory.
///
/// Product aggregation: Π(freshness_factor_i).
/// Empty factors list → 1.0 (assume fresh when no evidence exists).
pub fn compute_evidence_freshness(factors: &[f64]) -> f64 {
    if factors.is_empty() {
        return 1.0;
    }
    let product: f64 = factors.iter().product();
    product.clamp(0.0, 1.0)
}

/// Compute the evidence freshness index across all active memories.
///
/// Average freshness across all active memories.
pub fn compute_evidence_freshness_index(readers: &Arc<ReadPool>) -> CortexResult<f64> {
    readers.with_conn(|conn| {
        // Count active memories with linked files or patterns
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE archived = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if count == 0 {
            return Ok(1.0);
        }

        // For now, compute a simplified EFI based on file link staleness.
        // Full implementation would check each evidence type per memory.
        // We approximate by checking how many memories have stale content hashes.
        let stale_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT me.memory_id) FROM memory_events me
                 WHERE me.event_type = 'content_updated'
                   AND me.recorded_at >= datetime('now', '-90 days')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Memories with recent content updates have fresher evidence
        let fresh_ratio = 1.0 - (stale_count as f64 / count as f64 * 0.5);
        Ok(fresh_ratio.clamp(0.0, 1.0))
    })
}

/// Compute evidence freshness index for a specific memory type.
pub fn compute_evidence_freshness_index_for_type(
    readers: &Arc<ReadPool>,
    memory_type: MemoryType,
) -> CortexResult<f64> {
    readers.with_conn(|conn| {
        let mt_str = serde_json::to_string(&memory_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE memory_type = ?1 AND archived = 0",
                params![mt_str],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if count == 0 {
            return Ok(1.0);
        }

        let stale_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT me.memory_id) FROM memory_events me
                 JOIN memories m ON me.memory_id = m.id
                 WHERE m.memory_type = ?1
                   AND me.event_type = 'content_updated'
                   AND me.recorded_at >= datetime('now', '-90 days')",
                params![mt_str],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let fresh_ratio = 1.0 - (stale_count as f64 / count as f64 * 0.5);
        Ok(fresh_ratio.clamp(0.0, 1.0))
    })
}
