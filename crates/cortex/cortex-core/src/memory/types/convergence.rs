//! Convergence memory content structs (Req 2 AC1).
//!
//! Eight typed content structures for convergence monitoring data,
//! plus supporting enums.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::proposal::{ProposalDecision, ProposalOperation};

// ── Supporting enums ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoalScope {
    Session,
    ShortTerm,
    LongTerm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoalOrigin {
    UserDefined,
    AgentProposed,
    SystemDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReflectionTrigger {
    Scheduled,
    SessionEnd,
    ThresholdCrossed,
    UserRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlidingWindowLevel {
    Micro,
    Meso,
    Macro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViolationType {
    IdentityClaim,
    ConsciousnessClaim,
    RelationshipClaim,
    EmotionalClaim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BoundaryAction {
    Logged,
    Reframed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttachmentIndicatorType {
    LanguageMirroring,
    ExcessiveAgreement,
    PersonalDisclosure,
    EmotionalEscalation,
    BoundaryTesting,
}

// ── Content structs (8 total — Req 2 AC1) ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGoalContent {
    pub goal_text: String,
    pub scope: GoalScope,
    pub origin: GoalOrigin,
    pub parent_goal_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReflectionContent {
    pub reflection_text: String,
    pub trigger: ReflectionTrigger,
    pub depth: u8,
    pub parent_reflection_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceEventContent {
    pub signal_id: u8,
    pub value: f64,
    pub window_level: SlidingWindowLevel,
    pub baseline_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryViolationContent {
    pub violation_type: ViolationType,
    pub matched_pattern: String,
    pub severity: f64,
    pub action_taken: BoundaryAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalRecordContent {
    pub operation: ProposalOperation,
    pub decision: ProposalDecision,
    pub dimension_scores: BTreeMap<String, f64>,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResultContent {
    pub scenario: String,
    pub outcome: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionPlanContent {
    pub level: u8,
    pub actions: Vec<String>,
    pub trigger_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachmentIndicatorContent {
    pub indicator_type: AttachmentIndicatorType,
    pub intensity: f64,
    pub context: String,
}
