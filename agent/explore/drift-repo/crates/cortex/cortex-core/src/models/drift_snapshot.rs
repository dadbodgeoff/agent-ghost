//! Drift snapshot models — periodic captures of knowledge base health metrics.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::memory::MemoryType;

/// A point-in-time capture of all drift metrics across the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftSnapshot {
    /// When this snapshot was taken.
    pub timestamp: DateTime<Utc>,
    /// Window duration in hours over which metrics were computed.
    pub window_hours: u64,
    /// Per-memory-type drift metrics.
    pub type_metrics: HashMap<MemoryType, TypeDriftMetrics>,
    /// Per-module drift metrics (keyed by module/namespace).
    pub module_metrics: HashMap<String, ModuleDriftMetrics>,
    /// Global aggregate metrics.
    pub global: GlobalDriftMetrics,
}

/// Drift metrics for a specific memory type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeDriftMetrics {
    /// Number of memories of this type.
    pub count: usize,
    /// Average confidence across memories of this type.
    pub avg_confidence: f64,
    /// Knowledge Stability Index for this type (0.0–1.0).
    pub ksi: f64,
    /// Contradiction density for this type.
    pub contradiction_density: f64,
    /// Consolidation efficiency for this type.
    pub consolidation_efficiency: f64,
    /// Evidence freshness index for this type (0.0–1.0).
    pub evidence_freshness_index: f64,
}

/// Drift metrics for a specific module/namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleDriftMetrics {
    /// Number of memories in this module.
    pub memory_count: usize,
    /// Coverage ratio (memories with evidence / total).
    pub coverage_ratio: f64,
    /// Average confidence in this module.
    pub avg_confidence: f64,
    /// Churn rate: (created + archived) / total in window.
    pub churn_rate: f64,
}

/// Global aggregate drift metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalDriftMetrics {
    /// Total memory count.
    pub total_memories: usize,
    /// Active (non-archived) memory count.
    pub active_memories: usize,
    /// Archived memory count.
    pub archived_memories: usize,
    /// Average confidence across all active memories.
    pub avg_confidence: f64,
    /// Overall KSI across all types.
    pub overall_ksi: f64,
    /// Overall contradiction density.
    pub overall_contradiction_density: f64,
    /// Overall evidence freshness index.
    pub overall_evidence_freshness: f64,
}

impl PartialEq for DriftSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
            && self.window_hours == other.window_hours
            && self.type_metrics == other.type_metrics
            && self.module_metrics == other.module_metrics
            && self.global == other.global
    }
}
