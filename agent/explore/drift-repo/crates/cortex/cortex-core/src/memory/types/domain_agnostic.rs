use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Core project/workspace metadata. Half-life: ∞
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct CoreContent {
    pub project_name: String,
    pub description: String,
    pub metadata: serde_json::Value,
}

/// Institutional knowledge. Half-life: 365d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct TribalContent {
    pub knowledge: String,
    pub severity: String,
    pub warnings: Vec<String>,
    pub consequences: Vec<String>,
}

/// How-to procedures. Half-life: 180d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ProceduralContent {
    pub title: String,
    pub steps: Vec<ProceduralStep>,
    pub prerequisites: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ProceduralStep {
    pub order: u32,
    pub instruction: String,
    pub completed: bool,
}

/// Consolidated knowledge from episodic memories. Half-life: 90d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SemanticContent {
    pub knowledge: String,
    pub source_episodes: Vec<String>,
    pub consolidation_confidence: f64,
}

/// Raw interaction records. Half-life: 7d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct EpisodicContent {
    pub interaction: String,
    pub context: String,
    pub outcome: Option<String>,
}

/// Standalone decisions. Half-life: 180d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DecisionContent {
    pub decision: String,
    pub rationale: String,
    pub alternatives: Vec<Alternative>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct Alternative {
    pub description: String,
    pub reason_rejected: String,
}

/// Learned observations. Half-life: 90d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct InsightContent {
    pub observation: String,
    pub evidence: Vec<String>,
}

/// External references/citations. Half-life: 60d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ReferenceContent {
    pub title: String,
    pub url: Option<String>,
    pub citation: String,
}

/// User/team preferences. Half-life: 120d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct PreferenceContent {
    pub preference: String,
    pub scope: String,
    pub value: serde_json::Value,
}
