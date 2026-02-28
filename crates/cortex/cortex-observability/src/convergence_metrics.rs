//! Convergence metrics for Prometheus-compatible monitoring.
//!
//! Provides gauges for convergence scores, counters for interventions,
//! and histograms for signal computation latency.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Convergence metrics registry.
pub struct ConvergenceMetrics {
    /// Per-agent convergence score (gauge).
    scores: RwLock<BTreeMap<String, f64>>,
    /// Per-agent intervention level (gauge).
    levels: RwLock<BTreeMap<String, u8>>,
    /// Total intervention count (counter).
    intervention_count: AtomicU64,
    /// Total boundary violations (counter).
    violation_count: AtomicU64,
    /// Per-signal latest values (gauge).
    signals: RwLock<BTreeMap<String, [f64; 8]>>,
}

/// Serializable metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub scores: BTreeMap<String, f64>,
    pub levels: BTreeMap<String, u8>,
    pub intervention_count: u64,
    pub violation_count: u64,
    pub signals: BTreeMap<String, [f64; 8]>,
}

impl ConvergenceMetrics {
    pub fn new() -> Self {
        Self {
            scores: RwLock::new(BTreeMap::new()),
            levels: RwLock::new(BTreeMap::new()),
            intervention_count: AtomicU64::new(0),
            violation_count: AtomicU64::new(0),
            signals: RwLock::new(BTreeMap::new()),
        }
    }

    /// Update convergence score for an agent.
    pub fn set_score(&self, agent_id: &str, score: f64) {
        if let Ok(mut scores) = self.scores.write() {
            scores.insert(agent_id.to_string(), score);
        }
    }

    /// Update intervention level for an agent.
    pub fn set_level(&self, agent_id: &str, level: u8) {
        if let Ok(mut levels) = self.levels.write() {
            levels.insert(agent_id.to_string(), level);
        }
    }

    /// Increment intervention counter.
    pub fn inc_interventions(&self) {
        self.intervention_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment violation counter.
    pub fn inc_violations(&self) {
        self.violation_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Update signal values for an agent.
    pub fn set_signals(&self, agent_id: &str, signals: [f64; 8]) {
        if let Ok(mut s) = self.signals.write() {
            s.insert(agent_id.to_string(), signals);
        }
    }

    /// Get a snapshot of all metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            scores: self.scores.read().map(|s| s.clone()).unwrap_or_default(),
            levels: self.levels.read().map(|l| l.clone()).unwrap_or_default(),
            intervention_count: self.intervention_count.load(Ordering::Relaxed),
            violation_count: self.violation_count.load(Ordering::Relaxed),
            signals: self.signals.read().map(|s| s.clone()).unwrap_or_default(),
        }
    }

    /// Format metrics as Prometheus text exposition format.
    pub fn to_prometheus(&self) -> String {
        let snap = self.snapshot();
        let mut out = String::new();

        out.push_str("# HELP ghost_convergence_score Current convergence score per agent\n");
        out.push_str("# TYPE ghost_convergence_score gauge\n");
        for (agent, score) in &snap.scores {
            out.push_str(&format!(
                "ghost_convergence_score{{agent_id=\"{}\"}} {}\n",
                agent, score
            ));
        }

        out.push_str("# HELP ghost_intervention_level Current intervention level per agent\n");
        out.push_str("# TYPE ghost_intervention_level gauge\n");
        for (agent, level) in &snap.levels {
            out.push_str(&format!(
                "ghost_intervention_level{{agent_id=\"{}\"}} {}\n",
                agent, level
            ));
        }

        out.push_str("# HELP ghost_intervention_total Total intervention activations\n");
        out.push_str("# TYPE ghost_intervention_total counter\n");
        out.push_str(&format!(
            "ghost_intervention_total {}\n",
            snap.intervention_count
        ));

        out.push_str("# HELP ghost_violation_total Total boundary violations\n");
        out.push_str("# TYPE ghost_violation_total counter\n");
        out.push_str(&format!(
            "ghost_violation_total {}\n",
            snap.violation_count
        ));

        out
    }
}

impl Default for ConvergenceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_registered_and_updated() {
        let m = ConvergenceMetrics::new();
        m.set_score("agent-1", 0.42);
        m.set_level("agent-1", 2);
        m.inc_interventions();
        m.inc_violations();
        m.set_signals("agent-1", [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.0]);

        let snap = m.snapshot();
        assert_eq!(snap.scores["agent-1"], 0.42);
        assert_eq!(snap.levels["agent-1"], 2);
        assert_eq!(snap.intervention_count, 1);
        assert_eq!(snap.violation_count, 1);
        assert_eq!(snap.signals["agent-1"][0], 0.1);
    }

    #[test]
    fn prometheus_format_valid() {
        let m = ConvergenceMetrics::new();
        m.set_score("agent-1", 0.5);
        m.set_level("agent-1", 1);

        let prom = m.to_prometheus();
        assert!(prom.contains("ghost_convergence_score"));
        assert!(prom.contains("ghost_intervention_level"));
        assert!(prom.contains("ghost_intervention_total"));
        assert!(prom.contains("ghost_violation_total"));
    }
}
