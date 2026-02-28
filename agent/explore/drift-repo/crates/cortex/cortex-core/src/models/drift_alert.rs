//! Drift alert models — threshold-based alerts for knowledge base health.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A drift alert fired when a metric crosses a configured threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftAlert {
    /// Severity of the alert.
    pub severity: AlertSeverity,
    /// Category of drift detected.
    pub category: DriftAlertCategory,
    /// Human-readable description.
    pub message: String,
    /// Memory IDs affected by this alert.
    pub affected_memories: Vec<String>,
    /// Recommended action to address the drift.
    pub recommended_action: String,
    /// When the alert was detected.
    pub detected_at: DateTime<Utc>,
}

/// Alert severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Categories of drift that can trigger alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftAlertCategory {
    /// High churn rate — too many memories being created/archived.
    KnowledgeChurn,
    /// Confidence declining over consecutive windows.
    ConfidenceErosion,
    /// Contradiction density exceeds threshold.
    ContradictionSpike,
    /// Evidence links going stale for important memories.
    StaleEvidence,
    /// Memory creation rate spiking above baseline.
    KnowledgeExplosion,
    /// Modules or types with insufficient coverage.
    CoverageGap,
}
