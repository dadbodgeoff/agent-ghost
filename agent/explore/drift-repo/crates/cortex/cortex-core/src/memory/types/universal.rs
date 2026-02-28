use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Reusable agent configurations. Half-life: 365d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct AgentSpawnContent {
    pub agent_name: String,
    pub configuration: serde_json::Value,
    pub purpose: String,
}

/// Projects, products, teams, systems. Half-life: 180d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct EntityContent {
    pub entity_name: String,
    pub entity_type: String,
    pub description: String,
    pub attributes: serde_json::Value,
}

/// Objectives with progress tracking. Half-life: 90d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct GoalContent {
    pub title: String,
    pub description: String,
    pub progress: f64,
    pub milestones: Vec<String>,
}

/// Corrections and learning signals. Half-life: 120d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct FeedbackContent {
    pub feedback: String,
    pub category: String,
    pub source: String,
}

/// Step-by-step processes. Half-life: 180d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct WorkflowContent {
    pub name: String,
    pub steps: Vec<WorkflowStep>,
    pub trigger: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct WorkflowStep {
    pub order: u32,
    pub action: String,
    pub condition: Option<String>,
}

/// Summarized past discussions. Half-life: 30d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ConversationContent {
    pub summary: String,
    pub participants: Vec<String>,
    pub key_points: Vec<String>,
}

/// Postmortems with root cause. Half-life: 365d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct IncidentContent {
    pub title: String,
    pub root_cause: String,
    pub impact: String,
    pub resolution: String,
    pub lessons_learned: Vec<String>,
}

/// Meeting notes and action items. Half-life: 60d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct MeetingContent {
    pub title: String,
    pub attendees: Vec<String>,
    pub notes: String,
    pub action_items: Vec<String>,
}

/// Knowledge domains and proficiency. Half-life: 180d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SkillContent {
    pub skill_name: String,
    pub proficiency: String,
    pub domain: String,
    pub evidence: Vec<String>,
}

/// System/environment configurations. Half-life: 90d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct EnvironmentContent {
    pub name: String,
    pub config: serde_json::Value,
    pub platform: Option<String>,
}
