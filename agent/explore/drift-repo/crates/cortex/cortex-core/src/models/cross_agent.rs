//! Cross-agent relationship and trust types for multi-agent memory.
//!
//! # Examples
//!
//! ```
//! use cortex_core::models::cross_agent::{AgentTrust, TrustEvidence};
//! use cortex_core::models::agent::AgentId;
//! use chrono::Utc;
//!
//! let trust = AgentTrust {
//!     agent_id: AgentId::from("agent-a"),
//!     target_agent: AgentId::from("agent-b"),
//!     overall_trust: 0.5,
//!     domain_trust: std::collections::HashMap::new(),
//!     evidence: TrustEvidence::default(),
//!     last_updated: Utc::now(),
//! };
//! assert!((0.0..=1.0).contains(&trust.overall_trust));
//! ```

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::agent::AgentId;

/// Relationship types between memories owned by different agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CrossAgentRelation {
    /// This memory was informed by another agent's memory.
    InformedBy,
    /// A decision was based on another agent's memory.
    DecisionBasedOn,
    /// Two agents independently arrived at the same conclusion.
    IndependentCorroboration,
    /// Two agents' memories contradict each other.
    CrossAgentContradiction,
    /// This memory refines another agent's memory.
    Refinement,
}

/// A detected contradiction between two agents' memories.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CrossAgentContradiction {
    /// First memory in the contradiction.
    pub memory_a: String,
    /// Agent that owns memory_a.
    pub agent_a: AgentId,
    /// Trust score of agent_a.
    pub trust_a: f64,
    /// Second memory in the contradiction.
    pub memory_b: String,
    /// Agent that owns memory_b.
    pub agent_b: AgentId,
    /// Trust score of agent_b.
    pub trust_b: f64,
    /// Type of contradiction.
    pub contradiction_type: String,
    /// How this contradiction was or should be resolved.
    pub resolution: ContradictionResolution,
}

/// How a cross-agent contradiction is resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum ContradictionResolution {
    /// The agent with higher trust wins (trust difference > 0.3).
    TrustWins,
    /// Trust scores are too close; needs human review.
    NeedsHumanReview,
    /// Resolution depends on context (different scope tags).
    ContextDependent,
    /// Newer validated memory supersedes older one.
    TemporalSupersession,
}

/// Trust relationship from one agent toward another.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentTrust {
    /// The agent holding this trust assessment.
    pub agent_id: AgentId,
    /// The agent being assessed.
    pub target_agent: AgentId,
    /// Overall trust score in [0.0, 1.0].
    pub overall_trust: f64,
    /// Per-domain trust scores (domain name → score in [0.0, 1.0]).
    pub domain_trust: HashMap<String, f64>,
    /// Evidence supporting this trust assessment.
    pub evidence: TrustEvidence,
    /// When this trust was last updated.
    pub last_updated: DateTime<Utc>,
}

/// Accumulated evidence for trust computation.
///
/// Trust formula: `(validated + useful) / (total + 1) * (1 - contradicted / (total + 1))`
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrustEvidence {
    /// Number of memories validated as correct.
    pub validated_count: u64,
    /// Number of memories that contradicted known facts.
    pub contradicted_count: u64,
    /// Number of memories that were useful in decisions.
    pub useful_count: u64,
    /// Total memories received from the target agent.
    pub total_received: u64,
}

impl TrustEvidence {
    /// Compute the overall trust score from this evidence.
    ///
    /// Formula: `(validated + useful) / (total + 1) * (1 - contradicted / (total + 1))`
    /// Result is clamped to [0.0, 1.0].
    pub fn compute_trust(&self) -> f64 {
        let total = self.total_received as f64;
        let positive = (self.validated_count + self.useful_count) as f64;
        let negative = self.contradicted_count as f64;
        let trust = (positive / (total + 1.0)) * (1.0 - negative / (total + 1.0));
        trust.clamp(0.0, 1.0)
    }
}
