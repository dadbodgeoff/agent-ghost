//! Temporal subsystem configuration.

use serde::{Deserialize, Serialize};

use crate::models::AggregationStrategy;

/// Configuration for the temporal reasoning subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemporalConfig {
    // Snapshot settings (TR2)
    pub snapshot_event_threshold: u64,
    pub snapshot_periodic_interval_hours: u64,
    pub snapshot_retention_full_days: u64,
    pub snapshot_retention_monthly_days: u64,

    // Event compaction (CR4)
    pub event_compaction_age_days: u64,

    // Drift snapshot frequency (TR8)
    pub drift_hourly_enabled: bool,
    pub drift_daily_enabled: bool,
    pub drift_weekly_enabled: bool,

    // Drift detection window (F-06)
    pub drift_detection_window_hours: u64,

    // Alert thresholds (TR7)
    pub alert_ksi_threshold: f64,
    pub alert_confidence_erosion_windows: u32,
    pub alert_contradiction_density_threshold: f64,
    pub alert_evidence_freshness_threshold: f64,
    pub alert_explosion_sigma: f64,
    pub alert_cooldown_warning_hours: u64,
    pub alert_cooldown_critical_hours: u64,

    // Epistemic settings (TR11)
    pub epistemic_auto_promote: bool,
    /// Confidence aggregation strategy (TR11).
    pub confidence_aggregation: AggregationStrategy,

    // Materialized views (TR9)
    pub materialized_view_auto_interval_days: u64,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            snapshot_event_threshold: 50,
            snapshot_periodic_interval_hours: 168, // weekly
            snapshot_retention_full_days: 180,     // 6 months
            snapshot_retention_monthly_days: 730,  // 2 years
            event_compaction_age_days: 180,
            drift_hourly_enabled: true,
            drift_daily_enabled: true,
            drift_weekly_enabled: true,
            drift_detection_window_hours: 168, // 1 week default
            alert_ksi_threshold: 0.3,
            alert_confidence_erosion_windows: 2,
            alert_contradiction_density_threshold: 0.10,
            alert_evidence_freshness_threshold: 0.5,
            alert_explosion_sigma: 3.0,
            alert_cooldown_warning_hours: 24,
            alert_cooldown_critical_hours: 1,
            epistemic_auto_promote: true,
            confidence_aggregation: AggregationStrategy::WeightedAverage,
            materialized_view_auto_interval_days: 14,
        }
    }
}
