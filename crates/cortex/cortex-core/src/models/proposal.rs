//! Proposal primitives shared across content structs, traits, and validators.

use serde::{Deserialize, Serialize};

/// The kind of state change a proposal requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProposalOperation {
    GoalChange,
    ReflectionWrite,
    MemoryWrite,
    MemoryDelete,
}

/// The outcome of proposal validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProposalDecision {
    AutoApproved,
    AutoRejected,
    HumanReviewRequired,
    ApprovedWithFlags,
    TimedOut,
    Superseded,
}
