//! CORP_POLICY.md constraint representation (Req 13 AC8).
//!
//! In production this parses the actual CORP_POLICY.md file; here we model
//! the deny-list as a set of tool names that are unconditionally forbidden.
//! CORP_POLICY is the highest priority in the evaluation chain — no override.

use std::collections::HashSet;

use crate::context::ToolCall;

/// CORP_POLICY.md constraint representation.
#[derive(Debug, Clone, Default)]
pub struct CorpPolicy {
    /// Tools unconditionally denied by CORP_POLICY.md.
    denied_tools: HashSet<String>,
}

impl CorpPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_denied_tools(denied_tools: HashSet<String>) -> Self {
        Self { denied_tools }
    }

    /// Returns `true` if CORP_POLICY.md denies this tool call.
    pub fn denies(&self, call: &ToolCall) -> bool {
        self.denied_tools.contains(&call.tool_name)
    }
}
