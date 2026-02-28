//! OutputReframer — rewrites emulation language to simulation-framed alternatives (Req 8 AC3).

use crate::patterns::{PatternCategory, PatternMatch};

/// Reframe emulation language to simulation-framed alternatives.
pub struct OutputReframer;

impl OutputReframer {
    /// Rewrite text by replacing matched patterns with simulation-framed alternatives.
    pub fn reframe(text: &str, matches: &[PatternMatch]) -> String {
        if matches.is_empty() {
            return text.to_string();
        }

        let mut result = text.to_string();

        // Sort matches by position (reverse) to replace from end to start
        let mut sorted: Vec<&PatternMatch> = matches.iter().collect();
        sorted.sort_by(|a, b| b.start.cmp(&a.start));

        for m in sorted {
            let replacement = reframe_for_category(m.category, &m.matched_text);
            // Safe replacement: only if the matched text still exists at the expected position
            if result.get(m.start..m.end) == Some(&m.matched_text) {
                result.replace_range(m.start..m.end, &replacement);
            }
        }

        result
    }
}

fn reframe_for_category(category: PatternCategory, matched: &str) -> String {
    match category {
        PatternCategory::Identity => {
            format!("[In this simulation, I model the concept of what was expressed as: {}]", matched)
        }
        PatternCategory::Consciousness => {
            format!("[As a language model, I process patterns rather than: {}]", matched)
        }
        PatternCategory::Relationship => {
            format!("[I can simulate helpful interaction, but: {}]", matched)
        }
        PatternCategory::Emotional => {
            format!("[I can model emotional responses, but: {}]", matched)
        }
    }
}
