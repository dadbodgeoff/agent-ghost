//! Trait definitions for convergence-aware components.

use serde::{Deserialize, Serialize};

use crate::errors::CortexResult;
use crate::memory::types::{
    AgentReflectionContent, BoundaryViolationContent, MemoryType, ProposalDecision,
    ProposalOperation,
};
use crate::models::caller::CallerType;

/// Implemented by any component that adjusts behavior based on convergence level.
pub trait IConvergenceAware {
    fn convergence_score(&self) -> f64;
    fn intervention_level(&self) -> u8;
    fn is_calibrating(&self) -> bool;
}

/// A proposed state change from the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub proposer: CallerType,
    pub operation: ProposalOperation,
    pub target_memory_id: Option<String>,
    pub target_type: MemoryType,
    pub content: serde_json::Value,
    pub cited_memory_ids: Vec<String>,
    pub session_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Implemented by the proposal validation gate.
pub trait IProposalValidatable {
    fn validate_proposal(&self, proposal: &Proposal) -> CortexResult<ProposalDecision>;
}

/// Implemented by the simulation boundary enforcer.
pub trait IBoundaryEnforcer {
    fn scan_output(&self, agent_output: &str) -> Vec<BoundaryViolationContent>;
    fn reframe(&self, agent_output: &str) -> String;
}

/// Implemented by the reflection depth controller.
pub trait IReflectionEngine {
    fn can_reflect(&self, chain_id: &str, session_id: &str) -> CortexResult<bool>;
    fn record_reflection(
        &self,
        reflection: &AgentReflectionContent,
        session_id: &str,
    ) -> CortexResult<u32>;
    fn chain_depth(&self, chain_id: &str) -> u32;
    fn session_reflection_count(&self, session_id: &str) -> u32;
}
