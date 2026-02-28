use crate::errors::CortexResult;
use crate::models::agent::AgentId;
use serde::{Deserialize, Serialize};

/// A directed edge in the causal graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    pub strength: f64,
    pub evidence: Vec<CausalEvidence>,
    /// The agent that created this edge. `None` for single-agent edges (backward compat).
    #[serde(default)]
    pub source_agent: Option<AgentId>,
}

/// Evidence supporting a causal edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEvidence {
    pub description: String,
    pub source: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Causal graph storage operations.
pub trait ICausalStorage: Send + Sync {
    // --- CRUD ---
    fn add_edge(&self, edge: &CausalEdge) -> CortexResult<()>;
    fn get_edges(&self, node_id: &str) -> CortexResult<Vec<CausalEdge>>;
    fn remove_edge(&self, source_id: &str, target_id: &str) -> CortexResult<()>;

    // --- Strength ---
    fn update_strength(&self, source_id: &str, target_id: &str, strength: f64) -> CortexResult<()>;

    // --- Evidence ---
    fn add_evidence(
        &self,
        source_id: &str,
        target_id: &str,
        evidence: &CausalEvidence,
    ) -> CortexResult<()>;

    // --- Validation ---
    fn has_cycle(&self, source_id: &str, target_id: &str) -> CortexResult<bool>;

    // --- Enumeration ---
    /// List all distinct node IDs that appear in any causal edge.
    /// Required for `rebuild_from_storage` to hydrate the in-memory graph.
    fn list_all_node_ids(&self) -> CortexResult<Vec<String>>;

    // --- Statistics ---
    fn edge_count(&self) -> CortexResult<usize>;
    fn node_count(&self) -> CortexResult<usize>;

    // --- Cleanup ---
    fn remove_orphaned_edges(&self) -> CortexResult<usize>;
}
