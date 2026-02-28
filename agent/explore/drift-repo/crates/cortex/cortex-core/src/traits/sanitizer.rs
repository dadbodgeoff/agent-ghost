use crate::errors::CortexResult;
use serde::{Deserialize, Serialize};

/// Result of sanitization with metadata about what was redacted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedText {
    pub text: String,
    pub redactions: Vec<Redaction>,
}

/// A single redaction applied during sanitization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Redaction {
    pub category: String,
    pub placeholder: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f64,
}

/// PII/secret sanitization.
pub trait ISanitizer: Send + Sync {
    /// Sanitize text, replacing PII and secrets with placeholders.
    fn sanitize(&self, text: &str) -> CortexResult<SanitizedText>;
}
