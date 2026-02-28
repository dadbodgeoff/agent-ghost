use serde::{Deserialize, Serialize};

/// Signals derived from git context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitSignals {
    /// Current branch name (e.g., "feature/auth-refactor").
    pub branch_name: Option<String>,
    /// Files modified in the working tree.
    pub modified_files: Vec<String>,
    /// Recent commit messages (most recent first).
    pub recent_commit_messages: Vec<String>,
}

impl GitSignals {
    /// Gather git signals from the provided context.
    pub fn gather(
        branch_name: Option<String>,
        modified_files: Vec<String>,
        recent_commit_messages: Vec<String>,
    ) -> Self {
        Self {
            branch_name,
            modified_files,
            recent_commit_messages,
        }
    }

    /// Extract domain keywords from the branch name.
    /// e.g., "feature/auth-refactor" → ["auth", "refactor"]
    pub fn branch_keywords(&self) -> Vec<String> {
        self.branch_name
            .as_deref()
            .unwrap_or("")
            .split(['/', '-', '_'])
            .filter(|s| !s.is_empty() && !is_branch_prefix(s))
            .map(|s| s.to_lowercase())
            .collect()
    }
}

/// Common branch prefixes that aren't meaningful keywords.
fn is_branch_prefix(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "feature"
            | "fix"
            | "bugfix"
            | "hotfix"
            | "release"
            | "chore"
            | "main"
            | "master"
            | "develop"
    )
}
