//! Domain pattern matching utility for egress policy enforcement.
//!
//! Supports exact domain matching and wildcard patterns (`*.slack.com`).
//! All matching is case-insensitive. Domains are stripped of ports and paths
//! before matching.

use regex::Regex;

/// Compiled domain matcher that supports exact and wildcard patterns.
#[derive(Debug, Clone)]
pub struct DomainMatcher {
    patterns: Vec<CompiledPattern>,
}

#[derive(Debug, Clone)]
struct CompiledPattern {
    /// Original pattern string for display/debug.
    original: String,
    /// Compiled regex for matching.
    regex: Regex,
}

impl DomainMatcher {
    /// Create a new DomainMatcher from a list of domain patterns.
    ///
    /// Patterns can be:
    /// - Exact: `api.openai.com`
    /// - Wildcard: `*.slack.com` (matches `api.slack.com` but NOT `slack.com` or `evil-slack.com`)
    pub fn new(patterns: &[String]) -> Self {
        let compiled = patterns
            .iter()
            .filter_map(|p| Self::compile_pattern(p))
            .collect();
        Self { patterns: compiled }
    }

    /// Check if a domain matches any of the compiled patterns.
    ///
    /// The domain is normalized before matching:
    /// - Lowercased
    /// - Port stripped (e.g. `api.openai.com:443` → `api.openai.com`)
    /// - Path stripped (e.g. `api.openai.com/v1/chat` → `api.openai.com`)
    /// - Leading/trailing whitespace trimmed
    ///
    /// Returns `false` for empty domains.
    pub fn matches(&self, domain: &str) -> bool {
        let normalized = match Self::normalize_domain(domain) {
            Some(d) => d,
            None => return false,
        };
        self.patterns.iter().any(|p| p.regex.is_match(&normalized))
    }

    /// Return the list of original pattern strings.
    pub fn patterns(&self) -> Vec<&str> {
        self.patterns.iter().map(|p| p.original.as_str()).collect()
    }

    /// Normalize a domain string for matching.
    ///
    /// Returns `None` for empty or invalid domains.
    fn normalize_domain(domain: &str) -> Option<String> {
        let trimmed = domain.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Strip any path component (only match domain portion).
        let domain_part = trimmed.split('/').next().unwrap_or(trimmed);

        // Strip port if present.
        let without_port = if domain_part.contains(':') {
            domain_part.rsplit_once(':').map(|(d, _)| d).unwrap_or(domain_part)
        } else {
            domain_part
        };

        let lowered = without_port.to_lowercase();

        // Reject domains with invalid characters.
        if lowered.contains('\0') || lowered.contains(' ') {
            return None;
        }

        if lowered.is_empty() {
            return None;
        }

        Some(lowered)
    }

    /// Compile a single domain pattern into a regex.
    fn compile_pattern(pattern: &str) -> Option<CompiledPattern> {
        let trimmed = pattern.trim().to_lowercase();
        if trimmed.is_empty() {
            return None;
        }

        let is_wildcard = trimmed.starts_with("*.");
        let regex_str = if is_wildcard {
            // *.slack.com → matches any subdomain of slack.com, but NOT slack.com itself
            // and NOT evil-slack.com (must be a proper subdomain).
            let base = &trimmed[2..]; // strip "*."
            let escaped = regex::escape(base);
            format!("^[a-z0-9]([a-z0-9\\-]*[a-z0-9])?\\.{}$", escaped)
        } else {
            // Exact match.
            let escaped = regex::escape(&trimmed);
            format!("^{}$", escaped)
        };

        Regex::new(&regex_str).ok().map(|regex| CompiledPattern {
            original: pattern.to_string(),
            regex,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_domain_match() {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        assert!(matcher.matches("api.openai.com"));
        assert!(matcher.matches("API.OPENAI.COM")); // case-insensitive
        assert!(!matcher.matches("openai.com"));
        assert!(!matcher.matches("evil-api.openai.com"));
    }

    #[test]
    fn wildcard_matches_subdomain() {
        let matcher = DomainMatcher::new(&["*.slack.com".to_string()]);
        assert!(matcher.matches("api.slack.com"));
        assert!(matcher.matches("hooks.slack.com"));
    }

    #[test]
    fn wildcard_does_not_match_bare_domain() {
        let matcher = DomainMatcher::new(&["*.slack.com".to_string()]);
        assert!(!matcher.matches("slack.com"));
    }

    #[test]
    fn wildcard_does_not_match_evil_prefix() {
        let matcher = DomainMatcher::new(&["*.slack.com".to_string()]);
        assert!(!matcher.matches("evil-slack.com"));
    }

    #[test]
    fn strips_port_before_matching() {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        assert!(matcher.matches("api.openai.com:443"));
    }

    #[test]
    fn strips_path_before_matching() {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        assert!(matcher.matches("api.openai.com/../../etc/passwd"));
        // Only the domain portion is matched — path traversal is irrelevant.
    }

    #[test]
    fn empty_domain_returns_false() {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        assert!(!matcher.matches(""));
        assert!(!matcher.matches("   "));
    }

    #[test]
    fn domain_with_null_byte_returns_false() {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        assert!(!matcher.matches("api.openai.com\0evil"));
    }

    #[test]
    fn domain_with_spaces_returns_false() {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        assert!(!matcher.matches("api openai com"));
    }

    #[test]
    fn unicode_domain_rejected() {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        // Unicode homograph attack — should not match.
        assert!(!matcher.matches("аpi.openai.com")); // Cyrillic 'а'
    }

    #[test]
    fn multiple_patterns() {
        let matcher = DomainMatcher::new(&[
            "api.openai.com".to_string(),
            "api.anthropic.com".to_string(),
            "*.slack.com".to_string(),
        ]);
        assert!(matcher.matches("api.openai.com"));
        assert!(matcher.matches("api.anthropic.com"));
        assert!(matcher.matches("hooks.slack.com"));
        assert!(!matcher.matches("evil.example.com"));
    }

    #[test]
    fn empty_pattern_list() {
        let matcher = DomainMatcher::new(&[]);
        assert!(!matcher.matches("anything.com"));
    }
}
