//! Temporal diff types for comparing knowledge states.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::memory::BaseMemory;
use crate::models::Contradiction;

/// Result of comparing two knowledge states at different times.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiff {
    /// Memories that exist at time_b but not time_a
    pub created: Vec<BaseMemory>,
    /// Memories that exist at time_a but not time_b
    pub archived: Vec<BaseMemory>,
    /// Memories that exist at both times but changed
    pub modified: Vec<MemoryModification>,
    /// Significant confidence changes (delta > 0.2)
    pub confidence_shifts: Vec<ConfidenceShift>,
    /// New contradictions detected between time_a and time_b
    pub new_contradictions: Vec<Contradiction>,
    /// Contradictions resolved between time_a and time_b
    pub resolved_contradictions: Vec<Contradiction>,
    /// Memory type reclassifications
    pub reclassifications: Vec<Reclassification>,
    /// Summary statistics
    pub stats: DiffStats,
}

/// A modification to a specific field of a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryModification {
    /// Memory that was modified
    pub memory_id: String,
    /// Field that changed
    pub field: String,
    /// Old value
    pub old_value: Value,
    /// New value
    pub new_value: Value,
    /// When the modification occurred
    pub modified_at: DateTime<Utc>,
}

/// A significant confidence change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceShift {
    /// Memory with confidence change
    pub memory_id: String,
    /// Old confidence
    pub old_confidence: f64,
    /// New confidence
    pub new_confidence: f64,
    /// Delta (new - old)
    pub delta: f64,
}

/// A memory type reclassification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reclassification {
    /// Memory that was reclassified
    pub memory_id: String,
    /// Old type
    pub old_type: String,
    /// New type
    pub new_type: String,
    /// Confidence in reclassification
    pub confidence: f64,
    /// When reclassification occurred
    pub reclassified_at: DateTime<Utc>,
}

/// Summary statistics for a temporal diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    /// Total memories at time_a
    pub memories_at_a: usize,
    /// Total memories at time_b
    pub memories_at_b: usize,
    /// Net change (memories_at_b - memories_at_a)
    pub net_change: i64,
    /// Average confidence at time_a
    pub avg_confidence_at_a: f64,
    /// Average confidence at time_b
    pub avg_confidence_at_b: f64,
    /// Confidence trend (positive = improving)
    pub confidence_trend: f64,
    /// Knowledge churn rate: (created + archived) / total
    pub knowledge_churn_rate: f64,
}
