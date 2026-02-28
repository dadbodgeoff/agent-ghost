use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::cross_agent::CrossAgentRelation;

/// The 14 relationship types between memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    // Core (5)
    Supersedes,
    Supports,
    Contradicts,
    Related,
    DerivedFrom,
    // Semantic V2 (8)
    Owns,
    Affects,
    Blocks,
    Requires,
    References,
    LearnedFrom,
    AssignedTo,
    DependsOn,
    // Multi-agent (1) — cross-agent relationship, detail in RelationshipEdge
    CrossAgent,
}

impl RelationshipType {
    /// Total number of relationship types.
    pub const COUNT: usize = 14;

    /// All variants for iteration.
    pub const ALL: [RelationshipType; 14] = [
        Self::Supersedes,
        Self::Supports,
        Self::Contradicts,
        Self::Related,
        Self::DerivedFrom,
        Self::Owns,
        Self::Affects,
        Self::Blocks,
        Self::Requires,
        Self::References,
        Self::LearnedFrom,
        Self::AssignedTo,
        Self::DependsOn,
        Self::CrossAgent,
    ];
}

/// An edge in the relationship graph between two memories.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RelationshipEdge {
    pub source_id: String,
    pub target_id: String,
    pub relationship_type: RelationshipType,
    /// Strength of the relationship, 0.0–1.0.
    pub strength: f64,
    /// Evidence supporting this relationship.
    pub evidence: Vec<String>,
    /// Cross-agent relation detail (only set when relationship_type == CrossAgent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_agent_relation: Option<CrossAgentRelation>,
}
