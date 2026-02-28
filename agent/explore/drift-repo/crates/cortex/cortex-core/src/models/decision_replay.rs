//! Decision replay models — reconstructing decision context with hindsight.

use serde::{Deserialize, Serialize};

use crate::memory::BaseMemory;
use crate::models::CompressedMemory;

/// Result of replaying a decision with historical context and hindsight.
///
/// This is the output of Query Type 4 (TR3). It reconstructs what the agent
/// knew at decision time, what it would have retrieved, and what we know NOW
/// that we didn't know THEN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionReplay {
    /// The decision memory as it was at creation time
    pub decision: BaseMemory,

    /// All memories that existed at decision time
    pub available_context: Vec<BaseMemory>,

    /// What retrieval would have returned at decision time
    pub retrieved_context: Vec<CompressedMemory>,

    /// Causal graph state at decision time
    pub causal_state: CausalGraphSnapshot,

    /// What we know NOW but didn't know THEN
    pub hindsight: Vec<HindsightItem>,
}

/// A piece of knowledge that didn't exist at decision time but is relevant now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HindsightItem {
    /// The memory that was created after the decision
    pub memory: BaseMemory,

    /// Embedding similarity to the decision topic (0.0-1.0)
    pub relevance: f64,

    /// How this memory relates to the decision
    /// Values: "contradicts", "would_have_informed", "supersedes", "supports"
    pub relationship: String,
}

/// Snapshot of the causal graph at a specific point in time.
///
/// This is a serializable representation of the petgraph StableGraph state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalGraphSnapshot {
    /// Memory IDs that were nodes in the graph at that time
    pub nodes: Vec<String>,

    /// Edges with their strengths at that time
    pub edges: Vec<CausalEdgeSnapshot>,
}

/// A single edge in the causal graph snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdgeSnapshot {
    /// Source memory ID
    pub source: String,

    /// Target memory ID
    pub target: String,

    /// Relationship type (e.g., "causes", "enables", "contradicts")
    pub relation_type: String,

    /// Edge strength at that time (0.0-1.0)
    pub strength: f64,
}
