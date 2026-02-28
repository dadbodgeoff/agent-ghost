//! Optional LLM-assisted extraction, falls back to rule_based if unavailable.

use super::rule_based;

/// Trait for LLM-based principle extraction.
pub trait LlmExtractor: Send + Sync {
    /// Extract a principle using an LLM. Returns None if LLM is unavailable.
    fn extract(&self, correction_text: &str, context: &str) -> Option<String>;
}

/// No-op extractor that always falls back to rule-based.
pub struct NoOpExtractor;

impl LlmExtractor for NoOpExtractor {
    fn extract(&self, _correction_text: &str, _context: &str) -> Option<String> {
        None
    }
}

/// Extract a principle, trying LLM first, falling back to rule-based.
pub fn extract_with_fallback(
    correction_text: &str,
    context: &str,
    llm: &dyn LlmExtractor,
) -> Option<String> {
    // Try LLM first.
    if let Some(principle) = llm.extract(correction_text, context) {
        return Some(principle);
    }

    // Fall back to rule-based.
    rule_based::extract_principle(correction_text, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_rule_based() {
        let extractor = NoOpExtractor;
        let result = extract_with_fallback("Don't use global state", "architecture", &extractor);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("Avoid:"));
    }

    struct MockExtractor;
    impl LlmExtractor for MockExtractor {
        fn extract(&self, _text: &str, _ctx: &str) -> Option<String> {
            Some("LLM-extracted principle".to_string())
        }
    }

    #[test]
    fn uses_llm_when_available() {
        let extractor = MockExtractor;
        let result = extract_with_fallback("anything", "ctx", &extractor);
        assert_eq!(result.unwrap(), "LLM-extracted principle");
    }
}
