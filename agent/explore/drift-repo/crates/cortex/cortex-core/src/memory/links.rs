use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Link to a code pattern.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct PatternLink {
    pub pattern_id: String,
    pub pattern_name: String,
}

/// Link to a constraint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ConstraintLink {
    pub constraint_id: String,
    pub constraint_name: String,
}

/// Link to a file with citation information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct FileLink {
    pub file_path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    /// blake3 hash of the cited content for staleness detection.
    pub content_hash: Option<String>,
}

/// Link to a function or method.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct FunctionLink {
    pub function_name: String,
    pub file_path: String,
    pub signature: Option<String>,
}
