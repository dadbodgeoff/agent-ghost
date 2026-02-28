//! Drift detection module — metrics, alerting, evidence freshness,
//! snapshot storage, and evolution pattern detection.

pub mod alerting;
pub mod evidence_freshness;
pub mod metrics;
pub mod patterns;
pub mod snapshots;

pub use alerting::evaluate_drift_alerts;
pub use evidence_freshness::{compute_evidence_freshness, compute_evidence_freshness_index};
pub use metrics::{
    compute_all_metrics, compute_confidence_trajectory, compute_consolidation_efficiency,
    compute_contradiction_density, compute_ksi,
};
pub use patterns::{
    detect_conflict_wave, detect_crystallization, detect_erosion, detect_explosion,
    ConflictWavePattern, CrystallizationPattern, ErosionPattern, ExplosionPattern,
};
pub use snapshots::{get_drift_snapshots, get_latest_drift_snapshot, store_drift_snapshot};
