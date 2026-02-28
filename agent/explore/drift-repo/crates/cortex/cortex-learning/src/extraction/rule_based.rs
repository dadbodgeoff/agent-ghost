//! Rule-based extraction for offline: keyword matching, pattern templates,
//! negation detection, generalization rules.

/// Extract a principle from a correction using rule-based heuristics.
///
/// Returns None if no clear principle can be extracted.
pub fn extract_principle(correction_text: &str, context: &str) -> Option<String> {
    let text = correction_text.trim();
    if text.is_empty() {
        return None;
    }

    // Try negation-based extraction: "Don't X" → "Avoid X"
    if let Some(principle) = extract_negation(text) {
        return Some(principle);
    }

    // Try imperative extraction: "Use X instead of Y"
    if let Some(principle) = extract_imperative(text) {
        return Some(principle);
    }

    // Try pattern template: "Always/Never X"
    if let Some(principle) = extract_pattern_template(text) {
        return Some(principle);
    }

    // Fallback: generalize the correction text.
    Some(generalize(text, context))
}

/// Extract principle from negation patterns.
fn extract_negation(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    let negation_prefixes = ["don't ", "dont ", "do not ", "never ", "avoid ", "stop "];

    for prefix in &negation_prefixes {
        if lower.starts_with(prefix) {
            let rest = &text[prefix.len()..];
            return Some(format!("Avoid: {}", rest.trim()));
        }
    }

    None
}

/// Extract principle from imperative patterns.
fn extract_imperative(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    // "Use X instead of Y" pattern.
    if lower.contains("instead of") {
        return Some(format!("Prefer: {}", text.trim()));
    }

    // "Should X" pattern.
    if lower.contains("should ") {
        return Some(format!("Guideline: {}", text.trim()));
    }

    // "Must X" pattern.
    if lower.contains("must ") {
        return Some(format!("Requirement: {}", text.trim()));
    }

    None
}

/// Extract principle from pattern templates.
fn extract_pattern_template(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    if lower.starts_with("always ") {
        return Some(format!("Rule: {}", text.trim()));
    }

    if lower.starts_with("prefer ") || lower.starts_with("favor ") {
        return Some(format!("Preference: {}", text.trim()));
    }

    None
}

/// Generalize a correction into a principle.
fn generalize(text: &str, context: &str) -> String {
    if context.is_empty() {
        format!("Learned: {}", text)
    } else {
        format!("In context of {}: {}", context, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_negation_principle() {
        let p = extract_principle("Don't use unwrap in production code", "error handling");
        assert!(p.is_some());
        assert!(p.unwrap().starts_with("Avoid:"));
    }

    #[test]
    fn extracts_imperative_principle() {
        let p = extract_principle("Use Result instead of panic", "error handling");
        assert!(p.is_some());
        assert!(p.unwrap().starts_with("Prefer:"));
    }

    #[test]
    fn extracts_always_pattern() {
        let p = extract_principle("Always validate input before processing", "security");
        assert!(p.is_some());
        assert!(p.unwrap().starts_with("Rule:"));
    }

    #[test]
    fn generalizes_unknown_pattern() {
        let p = extract_principle("This approach works better", "refactoring");
        assert!(p.is_some());
        assert!(p.unwrap().contains("refactoring"));
    }

    #[test]
    fn empty_returns_none() {
        assert!(extract_principle("", "").is_none());
    }
}
