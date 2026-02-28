//! Provenance tracking types for multi-agent memory.
//!
//! Records the origin and chain of custody for every memory as it flows
//! between agents, enabling trust computation and correction propagation.
//!
//! # Examples
//!
//! ```
//! use cortex_core::models::provenance::{ProvenanceOrigin, ProvenanceAction, ProvenanceHop};
//! use cortex_core::models::agent::AgentId;
//! use chrono::Utc;
//!
//! let hop = ProvenanceHop {
//!     agent_id: AgentId::from("agent-1"),
//!     action: ProvenanceAction::Created,
//!     timestamp: Utc::now(),
//!     confidence_delta: 0.0,
//! };
//! assert_eq!(hop.action, ProvenanceAction::Created);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::agent::AgentId;

/// Full provenance record for a memory, including origin and chain of custody.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProvenanceRecord {
    /// The memory this provenance record belongs to.
    pub memory_id: String,
    /// How this memory was originally created.
    pub origin: ProvenanceOrigin,
    /// Chain of custody hops (ordered, oldest first).
    pub chain: Vec<ProvenanceHop>,
    /// Cumulative confidence through the chain (product of hop deltas).
    pub chain_confidence: f64,
}

/// How a memory was originally created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProvenanceOrigin {
    /// Created by a human user.
    Human,
    /// Created by an agent autonomously.
    AgentCreated,
    /// Derived from one or more existing memories.
    Derived,
    /// Imported from an external source.
    Imported,
    /// Created via a namespace projection.
    Projected,
}

/// A single hop in the provenance chain.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProvenanceHop {
    /// The agent that performed this action.
    pub agent_id: AgentId,
    /// What action was taken.
    pub action: ProvenanceAction,
    /// When this hop occurred.
    pub timestamp: DateTime<Utc>,
    /// Change in confidence at this hop (range: -1.0 to 1.0).
    pub confidence_delta: f64,
}

/// Actions that can appear in a provenance chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceAction {
    /// Memory was created at this hop.
    Created,
    /// Memory was shared to another namespace.
    SharedTo,
    /// Memory was projected to another namespace.
    ProjectedTo,
    /// Memory was merged with another memory.
    MergedWith,
    /// Memory was consolidated from multiple sources.
    ConsolidatedFrom,
    /// Memory was validated by this agent.
    ValidatedBy,
    /// Memory was used in a decision.
    UsedInDecision,
    /// Memory was corrected by this agent.
    CorrectedBy,
    /// Memory was reclassified from a different type.
    ReclassifiedFrom,
    /// Memory was retracted (archived/tombstoned).
    Retracted,
}
