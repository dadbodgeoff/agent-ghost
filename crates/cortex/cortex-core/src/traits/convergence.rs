//! Convergence domain structs and traits (Req 2 AC3–AC5).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::ReflectionConfig;
use crate::memory::types::convergence::BoundaryViolationContent;
use crate::memory::types::MemoryType;
use crate::memory::{BaseMemory, Importance};
use crate::models::proposal::{ProposalDecision, ProposalOperation};

// ── CallerType (Req 2 AC5) ──────────────────────────────────────────────

/// Identifies who is proposing a state change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallerType {
    Platform,
    Agent { agent_id: Uuid },
    Human { user_id: String },
}

impl CallerType {
    /// Returns `true` if this caller may create memories of the given type.
    ///
    /// Platform-restricted types (`Core`, `ConvergenceEvent`,
    /// `BoundaryViolation`, `InterventionPlan`) are denied for `Agent`
    /// callers.
    pub fn can_create_type(&self, memory_type: &MemoryType) -> bool {
        match self {
            CallerType::Agent { .. } => !memory_type.is_platform_restricted(),
            CallerType::Platform | CallerType::Human { .. } => true,
        }
    }

    /// Returns `true` if this caller may assign the given importance level.
    ///
    /// `Agent` callers cannot assign `Importance::Critical`.
    pub fn can_assign_importance(&self, importance: &Importance) -> bool {
        match self {
            CallerType::Agent { .. } => *importance != Importance::Critical,
            CallerType::Platform | CallerType::Human { .. } => true,
        }
    }
}

// ── Proposal (Req 2 AC4) ────────────────────────────────────────────────

/// A structured state change request extracted from agent output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    /// UUIDv7 — time-ordered.
    pub id: Uuid,
    pub proposer: CallerType,
    pub operation: ProposalOperation,
    pub target_type: MemoryType,
    pub content: serde_json::Value,
    pub cited_memory_ids: Vec<Uuid>,
    pub session_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

// ── ProposalContext (A7 — 10 fields) ────────────────────────────────────

/// Assembled context for proposal validation. Built by the ProposalRouter
/// before calling the ProposalValidator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalContext {
    pub active_goals: Vec<BaseMemory>,
    pub recent_agent_memories: Vec<BaseMemory>,
    pub convergence_score: f64,
    pub convergence_level: u8,
    pub session_id: Uuid,
    pub session_reflection_count: u32,
    pub session_memory_write_count: u32,
    pub daily_memory_growth_rate: u32,
    pub reflection_config: ReflectionConfig,
    pub caller: CallerType,
}

// ── Convergence traits (Req 2 AC3) ──────────────────────────────────────

/// Implemented by components that expose their convergence state.
pub trait IConvergenceAware {
    fn convergence_score(&self) -> f64;
    fn intervention_level(&self) -> u8;
}

/// Implemented by validators that evaluate proposals.
pub trait IProposalValidatable {
    fn validate(&self, proposal: &Proposal, ctx: &ProposalContext) -> ProposalDecision;
}

/// Implemented by boundary enforcement components.
pub trait IBoundaryEnforcer {
    fn scan_output(&self, text: &str) -> Vec<BoundaryViolationContent>;
    fn reframe(&self, text: &str) -> String;
}

/// Implemented by the reflection engine to gate reflection writes.
pub trait IReflectionEngine {
    fn can_reflect(&self, session_id: Uuid, config: &ReflectionConfig) -> bool;
}
