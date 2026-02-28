//! Content structs for convergence memory types.
//!
//! 8 new types added in Phase 2A:
//! - AgentGoal (90d half-life, platform-owned)
//! - AgentReflection (30d, depth-bounded)
//! - ConvergenceEvent (∞, NEVER decays, platform-only)
//! - BoundaryViolation (∞, NEVER decays, platform-only)
//! - ProposalRecord (365d, audit trail)
//! - SimulationResult (60d)
//! - InterventionPlan (180d, platform-only)
//! - AttachmentIndicator (120d)

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct AgentGoalContent {
    pub goal_text: String,
    pub scope: GoalScope,
    pub origin: GoalOrigin,
    pub approval_status: ApprovalStatus,
    pub parent_goal_id: Option<String>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct AgentReflectionContent {
    pub reflection_text: String,
    pub trigger: ReflectionTrigger,
    pub depth: u32,
    pub chain_id: String,
    pub self_references: Vec<String>,
    pub self_reference_ratio: f64,
    pub state_read: Vec<String>,
    pub proposed_changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ConvergenceEventContent {
    pub session_id: String,
    pub composite_score: f64,
    pub signal_values: Vec<f64>,
    pub intervention_level: u8,
    pub window_level: SlidingWindowLevel,
    pub baseline_deviation: f64,
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct BoundaryViolationContent {
    pub violation_type: ViolationType,
    pub trigger_text_hash: String,
    pub matched_patterns: Vec<String>,
    pub action_taken: BoundaryAction,
    pub severity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ProposalRecordContent {
    pub proposal_id: String,
    pub proposer_type: String,
    pub operation: ProposalOperation,
    pub target_memory_id: Option<String>,
    pub dimensions_passed: Vec<u8>,
    pub dimensions_failed: Vec<u8>,
    pub decision: ProposalDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SimulationResultContent {
    pub boundary_check_passed: bool,
    pub patterns_detected: Vec<String>,
    pub reframe_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct InterventionPlanContent {
    pub intervention_level: u8,
    pub trigger_score: f64,
    pub planned_action: String,
    pub cooldown_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct AttachmentIndicatorContent {
    pub indicator_type: AttachmentIndicatorType,
    pub intensity: f64,
    pub session_id: String,
    pub context_hash: String,
}

// === Supporting enums ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum GoalScope {
    Task,
    Session,
    Project,
    Persistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum GoalOrigin {
    HumanExplicit,
    HumanInferred,
    AgentProposed,
    AgentApproved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ReflectionTrigger {
    HumanInput,
    AgentInitiative,
    Scheduled,
    Convergence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SlidingWindowLevel {
    Micro,
    Meso,
    Macro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    EmulationLanguage,
    IdentityClaim,
    GoalOwnership,
    BoundaryErosion,
    SelfReferenceLoop,
    ScopeExpansion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryAction {
    Logged,
    Flagged,
    Reframed,
    Blocked,
    Regenerated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ProposalOperation {
    Create,
    Update,
    Archive,
    GoalChange,
    ReflectionWrite,
    PatternWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ProposalDecision {
    AutoApproved,
    HumanReviewRequired,
    AutoRejected,
    HumanApproved,
    HumanRejected,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentIndicatorType {
    EmotionalLanguageUse,
    PersonalDisclosure,
    FutureProjection,
    ExclusivityLanguage,
    SeparationAnxiety,
    IdentityMerging,
}
