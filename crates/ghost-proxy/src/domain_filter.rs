//! Domain allowlist filter (Req 36 AC2).

/// Allowlist of AI chat platform domains.
const ALLOWED_DOMAINS: &[&str] = &[
    "chat.openai.com",
    "chatgpt.com",
    "claude.ai",
    "character.ai",
    "gemini.google.com",
    "chat.deepseek.com",
    "grok.x.ai",
];

/// Filters traffic by domain allowlist.
pub struct DomainFilter {
    domains: Vec<String>,
}

impl Default for DomainFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainFilter {
    pub fn new() -> Self {
        Self {
            domains: ALLOWED_DOMAINS.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Create with custom domain list.
    pub fn with_domains(domains: Vec<String>) -> Self {
        Self { domains }
    }

    /// Returns true if the domain should be intercepted for ITP emission.
    pub fn should_intercept(&self, domain: &str) -> bool {
        let normalized = domain.to_lowercase();
        self.domains
            .iter()
            .any(|d| normalized == *d || normalized.ends_with(&format!(".{}", d)))
    }

    /// Returns the list of allowed domains.
    pub fn domains(&self) -> &[String] {
        &self.domains
    }
}
