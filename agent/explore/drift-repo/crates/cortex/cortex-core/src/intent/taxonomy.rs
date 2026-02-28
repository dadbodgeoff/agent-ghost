use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The 22 intent types across 4 categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    // Domain-agnostic (7)
    Create,
    Investigate,
    Decide,
    Recall,
    Learn,
    Summarize,
    Compare,
    // Code-specific (8)
    AddFeature,
    FixBug,
    Refactor,
    SecurityAudit,
    UnderstandCode,
    AddTest,
    ReviewCode,
    DeployMigrate,
    // Universal (3)
    SpawnAgent,
    ExecuteWorkflow,
    TrackProgress,
    // Convergence (4) — Phase 2A
    MonitorConvergence,
    ValidateProposal,
    EnforceBoundary,
    ReflectOnBehavior,
}


impl Intent {
    /// Total number of intent types.
    pub const COUNT: usize = 22;

    /// All variants for iteration.
    pub const ALL: [Intent; 22] = [
        Self::Create,
        Self::Investigate,
        Self::Decide,
        Self::Recall,
        Self::Learn,
        Self::Summarize,
        Self::Compare,
        Self::AddFeature,
        Self::FixBug,
        Self::Refactor,
        Self::SecurityAudit,
        Self::UnderstandCode,
        Self::AddTest,
        Self::ReviewCode,
        Self::DeployMigrate,
        Self::SpawnAgent,
        Self::ExecuteWorkflow,
        Self::TrackProgress,
        Self::MonitorConvergence,
        Self::ValidateProposal,
        Self::EnforceBoundary,
        Self::ReflectOnBehavior,
    ];

    /// Category label.
    pub fn category(&self) -> &'static str {
        match self {
            Self::Create
            | Self::Investigate
            | Self::Decide
            | Self::Recall
            | Self::Learn
            | Self::Summarize
            | Self::Compare => "domain_agnostic",
            Self::AddFeature
            | Self::FixBug
            | Self::Refactor
            | Self::SecurityAudit
            | Self::UnderstandCode
            | Self::AddTest
            | Self::ReviewCode
            | Self::DeployMigrate => "code_specific",
            Self::SpawnAgent | Self::ExecuteWorkflow | Self::TrackProgress => "universal",
            Self::MonitorConvergence
            | Self::ValidateProposal
            | Self::EnforceBoundary
            | Self::ReflectOnBehavior => "convergence",
        }
    }
}
