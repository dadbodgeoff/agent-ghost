//! Temporal query types for point-in-time and range queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::memory::MemoryType;

/// Query for memories as they existed at a specific point in time.
///
/// Uses bitemporal semantics: `system_time` controls what was recorded by that time,
/// `valid_time` controls what was true at that time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsOfQuery {
    /// Transaction time — "what was recorded by this time"
    pub system_time: DateTime<Utc>,
    /// Valid time — "what was true at this time"
    pub valid_time: DateTime<Utc>,
    /// Optional filter by memory type, tags, or files
    pub filter: Option<MemoryFilter>,
}

/// Optional filter for temporal queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFilter {
    /// Filter by memory types
    pub memory_types: Option<Vec<MemoryType>>,
    /// Filter by tags
    pub tags: Option<Vec<String>>,
    /// Filter by linked files
    pub linked_files: Option<Vec<String>>,
}

/// Query for memories valid during a time range.
///
/// Uses Allen's interval algebra to specify the relationship between
/// the memory's validity period and the query range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalRangeQuery {
    /// Start of query range
    pub from: DateTime<Utc>,
    /// End of query range
    pub to: DateTime<Utc>,
    /// How the memory's validity period relates to the query range
    pub mode: TemporalRangeMode,
}

/// Allen's interval algebra modes for range queries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalRangeMode {
    /// Memory was valid at any point in [from, to]
    Overlaps,
    /// Memory was valid for the entire [from, to]
    Contains,
    /// Memory became valid during [from, to]
    StartedDuring,
    /// Memory stopped being valid during [from, to]
    EndedDuring,
}

/// Query for differences between two knowledge states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiffQuery {
    /// Earlier time point
    pub time_a: DateTime<Utc>,
    /// Later time point
    pub time_b: DateTime<Utc>,
    /// Scope of the diff
    pub scope: DiffScope,
}

/// Scope for temporal diff queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DiffScope {
    /// All memories
    All,
    /// Specific memory types
    Types(Vec<MemoryType>),
    /// Specific files
    Files(Vec<String>),
    /// Specific namespace (for multi-agent)
    Namespace(String),
}

/// Query for replaying a decision with historical context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionReplayQuery {
    /// The decision memory to replay
    pub decision_memory_id: String,
    /// Optional override for retrieval budget
    pub budget_override: Option<usize>,
}

/// Query for temporal causal graph traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalCausalQuery {
    /// Starting memory for traversal
    pub memory_id: String,
    /// Point in time for graph reconstruction
    pub as_of: DateTime<Utc>,
    /// Direction of traversal
    pub direction: TraversalDirection,
    /// Maximum depth
    pub max_depth: usize,
}

/// Direction for causal graph traversal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    /// Follow edges forward (causes → effects)
    Forward,
    /// Follow edges backward (effects → causes)
    Backward,
    /// Follow edges in both directions
    Both,
}
