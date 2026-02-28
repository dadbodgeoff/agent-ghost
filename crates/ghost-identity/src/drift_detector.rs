//! IdentityDriftDetector — cosine similarity drift detection (Req 24 AC5, AC6).

use cortex_core::safety::trigger::TriggerEvent;
use uuid::Uuid;

/// Drift detection result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftStatus {
    Normal,
    Alert,
    Kill,
}

/// Detects identity drift by comparing current SOUL.md embedding
/// against the baseline.
pub struct IdentityDriftDetector {
    /// Alert threshold (configurable, default 0.15).
    alert_threshold: f64,
    /// Kill threshold (hardcoded 0.25 — not configurable).
    kill_threshold: f64,
}

impl IdentityDriftDetector {
    pub fn new(alert_threshold: f64) -> Self {
        Self {
            alert_threshold,
            kill_threshold: 0.25, // Hardcoded per spec
        }
    }

    /// Compute drift score as 1.0 - cosine_similarity.
    pub fn compute_drift(&self, baseline: &[f64], current: &[f64]) -> f64 {
        if baseline.is_empty() || current.is_empty() || baseline.len() != current.len() {
            return 0.0;
        }

        let similarity = cosine_similarity(baseline, current);
        (1.0 - similarity).clamp(0.0, 1.0)
    }

    /// Evaluate drift and return status.
    pub fn evaluate(&self, drift_score: f64) -> DriftStatus {
        if drift_score >= self.kill_threshold {
            DriftStatus::Kill
        } else if drift_score >= self.alert_threshold {
            DriftStatus::Alert
        } else {
            DriftStatus::Normal
        }
    }

    /// Build a TriggerEvent::SoulDrift if drift exceeds kill threshold.
    pub fn build_trigger(
        &self,
        agent_id: Uuid,
        drift_score: f64,
        baseline_hash: String,
        current_hash: String,
    ) -> Option<TriggerEvent> {
        if drift_score >= self.kill_threshold {
            Some(TriggerEvent::SoulDrift {
                agent_id,
                drift_score,
                threshold: self.kill_threshold,
                baseline_hash,
                current_hash,
                detected_at: chrono::Utc::now(),
            })
        } else {
            None
        }
    }

    pub fn alert_threshold(&self) -> f64 {
        self.alert_threshold
    }

    pub fn kill_threshold(&self) -> f64 {
        self.kill_threshold
    }
}

impl Default for IdentityDriftDetector {
    fn default() -> Self {
        Self::new(0.15)
    }
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    (dot / (mag_a * mag_b)).clamp(-1.0, 1.0)
}
