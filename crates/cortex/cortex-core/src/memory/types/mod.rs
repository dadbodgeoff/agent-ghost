//! Memory type taxonomy and typed content variants.

pub mod convergence;

use serde::{Deserialize, Serialize};

/// All memory type variants in the Cortex system.
///
/// The first 23 are the original domain-agnostic and code-specific types.
/// The last 8 are convergence monitoring types added for GHOST Platform v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryType {
    // ── Domain-agnostic (existing) ──────────────────────────────────
    Core,
    Tribal,
    Procedural,
    Semantic,
    Episodic,
    Decision,
    Insight,
    Reference,
    Preference,
    Conversation,
    Feedback,
    Skill,
    Goal,
    Relationship,
    Context,
    Observation,
    Hypothesis,
    Experiment,
    Lesson,
    // ── Code-specific (existing) ────────────────────────────────────
    PatternRationale,
    ConstraintOverride,
    DecisionContext,
    CodeSmell,
    // ── Convergence (new — GHOST Platform v1) ───────────────────────
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
    /// Default half-life in days. `None` means the memory never decays.
    pub fn half_life_days(&self) -> Option<u32> {
        match self {
            // ── Domain-agnostic ─────────────────────────────────────
            Self::Core => None,
            Self::Tribal => Some(180),
            Self::Procedural => Some(90),
            Self::Semantic => Some(120),
            Self::Episodic => Some(60),
            Self::Decision => Some(90),
            Self::Insight => Some(120),
            Self::Reference => Some(365),
            Self::Preference => Some(90),
            Self::Conversation => Some(30),
            Self::Feedback => Some(60),
            Self::Skill => Some(180),
            Self::Goal => Some(90),
            Self::Relationship => Some(120),
            Self::Context => Some(30),
            Self::Observation => Some(60),
            Self::Hypothesis => Some(90),
            Self::Experiment => Some(60),
            Self::Lesson => Some(120),
            // ── Code-specific ───────────────────────────────────────
            Self::PatternRationale => Some(180),
            Self::ConstraintOverride => Some(90),
            Self::DecisionContext => Some(90),
            Self::CodeSmell => Some(60),
            // ── Convergence (8 new entries — Req 2 AC8) ─────────────
            Self::AgentGoal => Some(90),
            Self::AgentReflection => Some(30),
            Self::ConvergenceEvent => None,  // never decay
            Self::BoundaryViolation => None, // never decay
            Self::ProposalRecord => Some(365),
            Self::SimulationResult => Some(60),
            Self::InterventionPlan => Some(180),
            Self::AttachmentIndicator => Some(120),
        }
    }

    /// Platform-restricted types that only `CallerType::Platform` (and Human)
    /// may create. Agent callers are denied.
    pub fn is_platform_restricted(&self) -> bool {
        matches!(
            self,
            Self::Core | Self::ConvergenceEvent | Self::BoundaryViolation | Self::InterventionPlan
        )
    }
}
