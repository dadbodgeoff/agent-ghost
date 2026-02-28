//! Compare original vs corrected: additions, removals, modifications, semantic changes.

use serde::{Deserialize, Serialize};

/// Result of analyzing the diff between original and corrected text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffAnalysis {
    /// Lines/phrases added in the correction.
    pub additions: Vec<String>,
    /// Lines/phrases removed from the original.
    pub removals: Vec<String>,
    /// Lines/phrases that were modified (original → corrected pairs).
    pub modifications: Vec<(String, String)>,
    /// Whether the correction represents a semantic change (not just formatting).
    pub is_semantic_change: bool,
}

/// Analyze the diff between original and corrected text.
pub fn analyze_diff(original: &str, corrected: &str) -> DiffAnalysis {
    let orig_lines: Vec<&str> = original.lines().collect();
    let corr_lines: Vec<&str> = corrected.lines().collect();

    let orig_set: std::collections::HashSet<&str> = orig_lines.iter().copied().collect();
    let corr_set: std::collections::HashSet<&str> = corr_lines.iter().copied().collect();

    let additions: Vec<String> = corr_lines
        .iter()
        .filter(|l| !orig_set.contains(**l) && !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    let removals: Vec<String> = orig_lines
        .iter()
        .filter(|l| !corr_set.contains(**l) && !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    // Pair up modifications: lines at the same position that differ.
    let mut modifications = Vec::new();
    let min_len = orig_lines.len().min(corr_lines.len());
    for i in 0..min_len {
        if orig_lines[i] != corr_lines[i]
            && !orig_lines[i].trim().is_empty()
            && !corr_lines[i].trim().is_empty()
        {
            modifications.push((orig_lines[i].to_string(), corr_lines[i].to_string()));
        }
    }

    // Semantic change = not just whitespace/formatting differences.
    let orig_normalized = normalize(original);
    let corr_normalized = normalize(corrected);
    let is_semantic_change = orig_normalized != corr_normalized;

    DiffAnalysis {
        additions,
        removals,
        modifications,
        is_semantic_change,
    }
}

/// Normalize text for semantic comparison (lowercase, collapse whitespace).
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_additions() {
        let diff = analyze_diff("line one", "line one\nline two");
        assert!(!diff.additions.is_empty());
        assert!(diff.additions.iter().any(|a| a.contains("line two")));
    }

    #[test]
    fn detects_removals() {
        let diff = analyze_diff("line one\nline two", "line one");
        assert!(!diff.removals.is_empty());
        assert!(diff.removals.iter().any(|r| r.contains("line two")));
    }

    #[test]
    fn detects_modifications() {
        let diff = analyze_diff("use snake_case", "use camelCase");
        assert!(!diff.modifications.is_empty());
    }

    #[test]
    fn whitespace_only_is_not_semantic() {
        let diff = analyze_diff("hello  world", "hello world");
        assert!(!diff.is_semantic_change);
    }

    #[test]
    fn real_change_is_semantic() {
        let diff = analyze_diff("use var", "use const");
        assert!(diff.is_semantic_change);
    }
}
