//! CORP_POLICY.md constraint representation (Req 13 AC8).
//!
//! In production this parses the actual CORP_POLICY.md file; here we model
//! the deny-list as a set of tool names that are unconditionally forbidden.
//! CORP_POLICY is the highest priority in the evaluation chain — no override.

use std::collections::HashSet;
use std::path::Path;

use crate::context::ToolCall;

/// Errors that can occur when loading CORP_POLICY.md from disk.
#[derive(Debug, thiserror::Error)]
pub enum CorpPolicyLoadError {
    #[error("CORP_POLICY.md not found at {path}")]
    NotFound { path: String },
    #[error("failed to read CORP_POLICY.md: {0}")]
    ReadError(String),
}

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

    /// Load and parse CORP_POLICY.md from disk (T-3.1.2).
    ///
    /// Extracts denied tool names from markdown list items under a
    /// `## Denied Tools` heading. Each line starting with `- ` under
    /// that heading is treated as a denied tool name (trimmed).
    pub fn load(path: &Path) -> Result<Self, CorpPolicyLoadError> {
        if !path.exists() {
            return Err(CorpPolicyLoadError::NotFound {
                path: path.display().to_string(),
            });
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| CorpPolicyLoadError::ReadError(e.to_string()))?;

        let denied_tools = parse_denied_tools(&content);
        Ok(Self { denied_tools })
    }

    /// Get the set of denied tool names.
    pub fn denied_tools(&self) -> &HashSet<String> {
        &self.denied_tools
    }

    /// Returns `true` if CORP_POLICY.md denies this tool call.
    pub fn denies(&self, call: &ToolCall) -> bool {
        self.denied_tools.contains(&call.tool_name)
    }
}

/// Parse denied tools from CORP_POLICY.md content.
///
/// Looks for a `## Denied Tools` section and collects bullet items.
/// Stops at the next `##` heading or end of file.
fn parse_denied_tools(content: &str) -> HashSet<String> {
    let mut denied = HashSet::new();
    let mut in_denied_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.eq_ignore_ascii_case("## denied tools")
            || trimmed.eq_ignore_ascii_case("## denied-tools")
        {
            in_denied_section = true;
            continue;
        }

        if in_denied_section && trimmed.starts_with("## ") {
            break;
        }

        if in_denied_section {
            if let Some(tool) = trimmed.strip_prefix("- ") {
                let tool = tool.trim();
                if !tool.is_empty() {
                    // Strip inline code backticks if present
                    let tool = tool.trim_matches('`');
                    denied.insert(tool.to_string());
                }
            }
        }
    }

    denied
}
