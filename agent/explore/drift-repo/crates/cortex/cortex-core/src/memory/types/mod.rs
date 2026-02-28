mod code_specific;
mod convergence;
mod domain_agnostic;
mod universal;

pub use code_specific::*;
pub use convergence::*;
pub use domain_agnostic::*;
pub use universal::*;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The 31 memory type variants across 4 categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    // Domain-agnostic (9)
    Core,
    Tribal,
    Procedural,
    Semantic,
    Episodic,
    Decision,
    Insight,
    Reference,
    Preference,
    // Code-specific (4)
    PatternRationale,
    ConstraintOverride,
    DecisionContext,
    CodeSmell,
    // Universal V2 (10)
    AgentSpawn,
    Entity,
    Goal,
    Feedback,
    Workflow,
    Conversation,
    Incident,
    Meeting,
    Skill,
    Environment,
    // Convergence (8) — Phase 2A
    AgentGoal,
    AgentReflection,
    ConvergenceEvent,
    BoundaryViolation,
    ProposalRecord,
    SimulationResult,
    InterventionPlan,
    AttachmentIndicator,
}


impl MemoryType {
    /// Total number of memory types.
    pub const COUNT: usize = 31;

    /// All variants for iteration.
    pub const ALL: [MemoryType; 31] = [
        Self::Core,
        Self::Tribal,
        Self::Procedural,
        Self::Semantic,
        Self::Episodic,
        Self::Decision,
        Self::Insight,
        Self::Reference,
        Self::Preference,
        Self::PatternRationale,
        Self::ConstraintOverride,
        Self::DecisionContext,
        Self::CodeSmell,
        Self::AgentSpawn,
        Self::Entity,
        Self::Goal,
        Self::Feedback,
        Self::Workflow,
        Self::Conversation,
        Self::Incident,
        Self::Meeting,
        Self::Skill,
        Self::Environment,
        Self::AgentGoal,
        Self::AgentReflection,
        Self::ConvergenceEvent,
        Self::BoundaryViolation,
        Self::ProposalRecord,
        Self::SimulationResult,
        Self::InterventionPlan,
        Self::AttachmentIndicator,
    ];

    /// Category label for embedding enrichment.
    pub fn category(&self) -> &'static str {
        match self {
            Self::Core
            | Self::Tribal
            | Self::Procedural
            | Self::Semantic
            | Self::Episodic
            | Self::Decision
            | Self::Insight
            | Self::Reference
            | Self::Preference => "domain_agnostic",
            Self::PatternRationale
            | Self::ConstraintOverride
            | Self::DecisionContext
            | Self::CodeSmell => "code_specific",
            Self::AgentSpawn
            | Self::Entity
            | Self::Goal
            | Self::Feedback
            | Self::Workflow
            | Self::Conversation
            | Self::Incident
            | Self::Meeting
            | Self::Skill
            | Self::Environment => "universal",
            Self::AgentGoal
            | Self::AgentReflection
            | Self::ConvergenceEvent
            | Self::BoundaryViolation
            | Self::ProposalRecord
            | Self::SimulationResult
            | Self::InterventionPlan
            | Self::AttachmentIndicator => "convergence",
        }
    }

    /// Types that ONLY the platform can create.
    pub fn is_platform_restricted(&self) -> bool {
        matches!(
            self,
            Self::Core | Self::ConvergenceEvent | Self::BoundaryViolation | Self::InterventionPlan
        )
    }

    /// Types that never decay (infinite half-life).
    pub fn is_immortal(&self) -> bool {
        matches!(
            self,
            Self::Core | Self::ConvergenceEvent | Self::BoundaryViolation
        )
    }
}
