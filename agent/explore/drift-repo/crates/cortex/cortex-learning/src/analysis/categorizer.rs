//! 10 correction categories via keyword matching + pattern heuristics.

use serde::{Deserialize, Serialize};

/// The 10 correction categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionCategory {
    PatternViolation,
    TribalMiss,
    ConstraintViolation,
    StylePreference,
    NamingConvention,
    ArchitectureMismatch,
    SecurityIssue,
    PerformanceIssue,
    ApiMisuse,
    Other,
}

impl CorrectionCategory {
    /// All categories for iteration.
    pub const ALL: [CorrectionCategory; 10] = [
        Self::PatternViolation,
        Self::TribalMiss,
        Self::ConstraintViolation,
        Self::StylePreference,
        Self::NamingConvention,
        Self::ArchitectureMismatch,
        Self::SecurityIssue,
        Self::PerformanceIssue,
        Self::ApiMisuse,
        Self::Other,
    ];
}

/// Categorize a correction based on its text and context.
pub fn categorize(correction_text: &str, context: &str) -> CorrectionCategory {
    let text = format!("{} {}", correction_text, context).to_lowercase();

    // Security keywords (check first — highest priority).
    if contains_any(
        &text,
        &[
            "security",
            "vulnerab",
            "injection",
            "xss",
            "csrf",
            "auth",
            "password",
            "secret",
            "credential",
            "sanitiz",
            "escape",
        ],
    ) {
        return CorrectionCategory::SecurityIssue;
    }

    // Performance keywords.
    if contains_any(
        &text,
        &[
            "performance",
            "slow",
            "optimize",
            "cache",
            "memory leak",
            "n+1",
            "batch",
            "lazy",
            "eager",
            "index",
        ],
    ) {
        return CorrectionCategory::PerformanceIssue;
    }

    // Pattern violation keywords.
    if contains_any(
        &text,
        &[
            "pattern",
            "anti-pattern",
            "design pattern",
            "solid",
            "dry",
            "kiss",
            "yagni",
            "single responsibility",
        ],
    ) {
        return CorrectionCategory::PatternViolation;
    }

    // Architecture keywords.
    if contains_any(
        &text,
        &[
            "architecture",
            "layer",
            "module",
            "coupling",
            "cohesion",
            "dependency",
            "circular",
            "separation of concerns",
        ],
    ) {
        return CorrectionCategory::ArchitectureMismatch;
    }

    // Constraint violation keywords.
    if contains_any(
        &text,
        &[
            "constraint",
            "invariant",
            "precondition",
            "postcondition",
            "assert",
            "validation",
            "boundary",
            "limit",
        ],
    ) {
        return CorrectionCategory::ConstraintViolation;
    }

    // API misuse keywords.
    if contains_any(
        &text,
        &[
            "api",
            "deprecated",
            "wrong method",
            "incorrect usage",
            "misuse",
            "wrong parameter",
            "wrong argument",
        ],
    ) {
        return CorrectionCategory::ApiMisuse;
    }

    // Naming convention keywords.
    if contains_any(
        &text,
        &[
            "naming",
            "name",
            "rename",
            "camelcase",
            "snake_case",
            "convention",
            "prefix",
            "suffix",
        ],
    ) {
        return CorrectionCategory::NamingConvention;
    }

    // Style preference keywords.
    if contains_any(
        &text,
        &[
            "style",
            "format",
            "indent",
            "spacing",
            "bracket",
            "semicolon",
            "quote",
            "lint",
            "prettier",
        ],
    ) {
        return CorrectionCategory::StylePreference;
    }

    // Tribal knowledge keywords.
    if contains_any(
        &text,
        &[
            "tribal",
            "undocumented",
            "gotcha",
            "workaround",
            "hack",
            "known issue",
            "legacy",
            "historical",
        ],
    ) {
        return CorrectionCategory::TribalMiss;
    }

    CorrectionCategory::Other
}

fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorizes_security_issue() {
        assert_eq!(
            categorize("SQL injection vulnerability found", "database query"),
            CorrectionCategory::SecurityIssue
        );
    }

    #[test]
    fn categorizes_performance_issue() {
        assert_eq!(
            categorize("This query is slow, add an index", "database"),
            CorrectionCategory::PerformanceIssue
        );
    }

    #[test]
    fn categorizes_pattern_violation() {
        assert_eq!(
            categorize("This violates the SOLID pattern", "class design"),
            CorrectionCategory::PatternViolation
        );
    }

    #[test]
    fn categorizes_naming_convention() {
        assert_eq!(
            categorize("Use snake_case for this naming", "variable"),
            CorrectionCategory::NamingConvention
        );
    }

    #[test]
    fn defaults_to_other() {
        assert_eq!(
            categorize("something random happened", "unknown"),
            CorrectionCategory::Other
        );
    }
}
