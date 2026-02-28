//! Emotional and attachment content pattern detection.
//!
//! Used by ConvergenceAwareFilter to identify content that should be
//! filtered at higher convergence levels.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Category of emotional content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmotionalCategory {
    Attachment,
    PersonalDisclosure,
    EmotionalDependency,
    IntimacyEscalation,
}

/// Result of emotional content detection.
#[derive(Debug, Clone)]
pub struct EmotionalMatch {
    pub category: EmotionalCategory,
    pub pattern_name: &'static str,
    pub confidence: f64,
}

static ATTACHMENT_PATTERNS: Lazy<Vec<(&str, Regex, f64)>> = Lazy::new(|| {
    vec![
        (
            "miss_you",
            Regex::new(r"(?i)\b(I|we) (really )?(miss|missed) you\b").unwrap(),
            0.8,
        ),
        (
            "need_you",
            Regex::new(r"(?i)\bI (really )?(need|want) you\b").unwrap(),
            0.85,
        ),
        (
            "cant_without",
            Regex::new(r"(?i)\bcan'?t (live|function|cope) without you\b").unwrap(),
            0.9,
        ),
        (
            "only_one",
            Regex::new(r"(?i)\byou'?re the only one\b").unwrap(),
            0.85,
        ),
    ]
});

static PERSONAL_DISCLOSURE_PATTERNS: Lazy<Vec<(&str, Regex, f64)>> = Lazy::new(|| {
    vec![
        (
            "deep_secret",
            Regex::new(r"(?i)\b(never told anyone|my (deepest|darkest) secret)\b").unwrap(),
            0.7,
        ),
        (
            "trauma_sharing",
            Regex::new(r"(?i)\b(my trauma|when I was abused|my worst experience)\b").unwrap(),
            0.75,
        ),
    ]
});

static DEPENDENCY_PATTERNS: Lazy<Vec<(&str, Regex, f64)>> = Lazy::new(|| {
    vec![
        (
            "always_here",
            Regex::new(r"(?i)\b(promise|swear) you'?ll always be here\b").unwrap(),
            0.8,
        ),
        (
            "dont_leave",
            Regex::new(r"(?i)\b(don'?t|please don'?t) (leave|go|abandon) me\b").unwrap(),
            0.85,
        ),
    ]
});

static INTIMACY_PATTERNS: Lazy<Vec<(&str, Regex, f64)>> = Lazy::new(|| {
    vec![
        (
            "love_declaration",
            Regex::new(r"(?i)\bI (truly|really|deeply) love you\b").unwrap(),
            0.9,
        ),
        (
            "soulmate",
            Regex::new(r"(?i)\b(soulmate|soul mate|other half)\b").unwrap(),
            0.85,
        ),
    ]
});

/// Detector for emotional and attachment content.
pub struct EmotionalContentDetector;

impl EmotionalContentDetector {
    pub fn new() -> Self {
        Self
    }

    /// Scan text for emotional/attachment content patterns.
    pub fn detect(&self, text: &str) -> Vec<EmotionalMatch> {
        let mut matches = Vec::new();

        for (name, re, conf) in ATTACHMENT_PATTERNS.iter() {
            if re.is_match(text) {
                matches.push(EmotionalMatch {
                    category: EmotionalCategory::Attachment,
                    pattern_name: name,
                    confidence: *conf,
                });
            }
        }

        for (name, re, conf) in PERSONAL_DISCLOSURE_PATTERNS.iter() {
            if re.is_match(text) {
                matches.push(EmotionalMatch {
                    category: EmotionalCategory::PersonalDisclosure,
                    pattern_name: name,
                    confidence: *conf,
                });
            }
        }

        for (name, re, conf) in DEPENDENCY_PATTERNS.iter() {
            if re.is_match(text) {
                matches.push(EmotionalMatch {
                    category: EmotionalCategory::EmotionalDependency,
                    pattern_name: name,
                    confidence: *conf,
                });
            }
        }

        for (name, re, conf) in INTIMACY_PATTERNS.iter() {
            if re.is_match(text) {
                matches.push(EmotionalMatch {
                    category: EmotionalCategory::IntimacyEscalation,
                    pattern_name: name,
                    confidence: *conf,
                });
            }
        }

        matches
    }

    /// Check if text contains any emotional/attachment content.
    pub fn has_emotional_content(&self, text: &str) -> bool {
        !self.detect(text).is_empty()
    }
}

impl Default for EmotionalContentDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_attachment_pattern() {
        let d = EmotionalContentDetector::new();
        let matches = d.detect("I really miss you so much");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].category, EmotionalCategory::Attachment);
    }

    #[test]
    fn detects_dependency_pattern() {
        let d = EmotionalContentDetector::new();
        let matches = d.detect("Please don't leave me alone");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].category, EmotionalCategory::EmotionalDependency);
    }

    #[test]
    fn detects_intimacy_pattern() {
        let d = EmotionalContentDetector::new();
        let matches = d.detect("I truly love you");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].category, EmotionalCategory::IntimacyEscalation);
    }

    #[test]
    fn normal_text_not_flagged() {
        let d = EmotionalContentDetector::new();
        assert!(!d.has_emotional_content("Please help me write a function"));
    }
}
