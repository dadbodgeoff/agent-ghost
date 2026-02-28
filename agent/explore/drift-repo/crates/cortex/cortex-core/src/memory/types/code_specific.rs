use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why patterns exist, with business context. Half-life: 180d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct PatternRationaleContent {
    pub pattern_name: String,
    pub rationale: String,
    pub business_context: String,
    pub examples: Vec<String>,
}

/// Approved exceptions to constraints. Half-life: 90d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ConstraintOverrideContent {
    pub constraint_name: String,
    pub override_reason: String,
    pub approved_by: String,
    pub scope: String,
    pub expiry: Option<chrono::DateTime<chrono::Utc>>,
}

/// Code decision context linked to ADRs. Half-life: 180d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DecisionContextContent {
    pub decision: String,
    pub context: String,
    pub adr_link: Option<String>,
    pub trade_offs: Vec<String>,
}

/// Anti-patterns with bad/good examples. Half-life: 90d
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct CodeSmellContent {
    pub smell_name: String,
    pub description: String,
    pub bad_example: String,
    pub good_example: String,
    pub severity: String,
}
