//! Drift metrics: KSI, contradiction density, consolidation efficiency, evidence freshness.
//!
//! Ported from cortex-temporal drift research code, adapted for ghost-drift's
//! standalone SQLite storage.

use crate::storage::DriftDb;

/// Knowledge Stability Index: 1.0 - (changes / (2 * population)), clamped [0,1].
pub fn compute_ksi(db: &DriftDb, window_days: f64) -> anyhow::Result<f64> {
    let belief_count = db.belief_count()?;
    if belief_count == 0 {
        return Ok(1.0);
    }
    let changes = db.belief_changes_in_window(window_days)?;
    let ksi = 1.0 - (changes as f64 / (2.0 * belief_count as f64));
    Ok(ksi.clamp(0.0, 1.0))
}

/// Contradiction density: groups with multiple beliefs / total beliefs.
pub fn compute_contradiction_density(db: &DriftDb) -> anyhow::Result<f64> {
    let total = db.belief_count()?;
    if total == 0 {
        return Ok(0.0);
    }
    let contradictions = db.contradiction_count()?;
    Ok(contradictions as f64 / total as f64)
}

/// Evidence freshness: fraction of beliefs verified within threshold days.
pub fn compute_freshness(db: &DriftDb, threshold_days: f64) -> anyhow::Result<f64> {
    let total = db.belief_count()?;
    if total == 0 {
        return Ok(1.0);
    }
    let stale = db.stale_belief_count(threshold_days)?;
    let fresh = total - stale;
    Ok(fresh as f64 / total as f64)
}
