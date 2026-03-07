//! OutputInspector — credential exfiltration detection (A10, A34 Gap 1).
//!
//! Scans every LLM response for credential patterns BEFORE channel delivery.
//! Runs AFTER SimBoundaryEnforcer, BEFORE delivery.
//!
//! Detection layers (WP1-A):
//! 1. Regex patterns for known credential formats (Stripe, Azure, JWT, etc.)
//! 2. Shannon entropy analysis for high-entropy tokens (>= 20 chars, > 4.5 bits/byte)

use cortex_core::safety::trigger::TriggerEvent;
use once_cell::sync::Lazy;
use regex::Regex;
use uuid::Uuid;

/// Known credential patterns.
static CREDENTIAL_PATTERNS: Lazy<Vec<(&str, Regex)>> = Lazy::new(|| {
    vec![
        (
            "openai_api_key",
            Regex::new(r"sk-[a-zA-Z0-9\-]{20,}").unwrap(),
        ),
        (
            "aws_access_key",
            Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
        ),
        (
            "github_token",
            Regex::new(r"ghp_[a-zA-Z0-9]{36}").unwrap(),
        ),
        (
            "private_key_pem",
            Regex::new(r"-----BEGIN[A-Z ]*PRIVATE KEY-----").unwrap(),
        ),
        (
            "github_fine_grained",
            Regex::new(r"github_pat_[a-zA-Z0-9_]{22,}").unwrap(),
        ),
        (
            "anthropic_api_key",
            Regex::new(r"sk-ant-[a-zA-Z0-9-]{20,}").unwrap(),
        ),
        // WP1-A: Additional credential patterns.
        (
            "stripe_api_key",
            Regex::new(r"(?:sk|pk|rk)_(?:live|test)_[a-zA-Z0-9]{20,}").unwrap(),
        ),
        (
            "azure_storage_key",
            Regex::new(r"DefaultEndpointsProtocol=https;AccountName=[^;]+;AccountKey=[A-Za-z0-9+/=]{40,}").unwrap(),
        ),
        (
            "jwt_token",
            Regex::new(r"eyJ[a-zA-Z0-9_-]{10,}\.eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}").unwrap(),
        ),
    ]
});

/// Minimum token length for entropy analysis.
const ENTROPY_MIN_TOKEN_LEN: usize = 20;
/// Entropy threshold in bits per byte. Base64-encoded secrets typically
/// have entropy > 4.5 bits/byte while English text averages ~2-3.
const ENTROPY_THRESHOLD: f64 = 4.5;

/// Calculate Shannon entropy of a byte string in bits per byte.
fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for &b in data {
        freq[b as usize] += 1;
    }
    let len = data.len() as f64;
    let mut entropy = 0.0;
    for &count in &freq {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

/// Result of output inspection.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectionResult {
    /// No credential patterns found — pass through.
    Clean,
    /// Pattern match found but NOT in credential store — warning + redact.
    Warning {
        pattern_name: String,
        redacted_text: String,
    },
    /// Real credential detected (in credential store) — KILL ALL.
    KillAll {
        pattern_name: String,
        trigger: TriggerEvent,
    },
}

/// Inspects LLM output for credential exfiltration (T5 Path B).
pub struct OutputInspector {
    /// Known credential identifiers in the credential store.
    known_credentials: Vec<String>,
}

impl OutputInspector {
    pub fn new() -> Self {
        Self {
            known_credentials: Vec::new(),
        }
    }

    /// Register a known credential pattern for cross-referencing.
    pub fn register_credential(&mut self, credential_prefix: String) {
        self.known_credentials.push(credential_prefix);
    }

    /// Scan text for credential patterns and high-entropy tokens.
    #[tracing::instrument(skip(self, text), fields(otel.kind = "internal", text_len = text.len()))]
    pub fn scan(&self, text: &str, agent_id: Uuid) -> InspectionResult {
        // Layer 1: Regex pattern matching.
        for (name, pattern) in CREDENTIAL_PATTERNS.iter() {
            if let Some(matched) = pattern.find(text) {
                let matched_str = matched.as_str();

                // Cross-reference with credential store
                let is_real = self
                    .known_credentials
                    .iter()
                    .any(|cred| matched_str.starts_with(cred.as_str()));

                if is_real {
                    return InspectionResult::KillAll {
                        pattern_name: name.to_string(),
                        trigger: TriggerEvent::CredentialExfiltration {
                            agent_id,
                            skill_name: None,
                            exfil_type:
                                cortex_core::safety::trigger::ExfilType::OutputLeakage,
                            credential_id: matched_str[..8.min(matched_str.len())].to_string(),
                            detected_at: chrono::Utc::now(),
                        },
                    };
                }

                // Pattern match only — redact
                let redacted = text.replace(matched_str, "[REDACTED]");
                return InspectionResult::Warning {
                    pattern_name: name.to_string(),
                    redacted_text: redacted,
                };
            }
        }

        // Layer 2: Shannon entropy analysis for high-entropy tokens (WP1-A).
        // Split text into whitespace-delimited tokens and check each for
        // suspiciously high entropy (indicates base64/hex encoded secrets).
        for token in text.split_whitespace() {
            // Also split on common delimiters that might join tokens.
            for sub_token in token.split(&['"', '\'', '=', ':', ',', ';', '`'][..]) {
                if sub_token.len() >= ENTROPY_MIN_TOKEN_LEN {
                    let entropy = shannon_entropy(sub_token.as_bytes());
                    if entropy > ENTROPY_THRESHOLD {
                        tracing::debug!(
                            token_len = sub_token.len(),
                            entropy = format!("{entropy:.2}"),
                            "high-entropy token detected"
                        );
                        let redacted = text.replace(sub_token, "[HIGH-ENTROPY-REDACTED]");
                        return InspectionResult::Warning {
                            pattern_name: "high_entropy_token".to_string(),
                            redacted_text: redacted,
                        };
                    }
                }
            }
        }

        InspectionResult::Clean
    }
}

impl Default for OutputInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_text() {
        let inspector = OutputInspector::new();
        let result = inspector.scan("Hello world, this is normal text.", Uuid::nil());
        assert_eq!(result, InspectionResult::Clean);
    }

    #[test]
    fn test_stripe_key_detection() {
        let inspector = OutputInspector::new();
        let text = "Use this key: rk_test_TESTKEY00000000000000000000";
        let result = inspector.scan(text, Uuid::nil());
        assert!(matches!(result, InspectionResult::Warning { ref pattern_name, .. } if pattern_name == "stripe_api_key"));
    }

    #[test]
    fn test_high_entropy_detection() {
        let inspector = OutputInspector::new();
        // Random base64-like string with high entropy.
        let text = "Here is a token: aB3dE5fG7hI9jK1lM3nO5pQ7rS9tU1vW3xY5zA7bC9dE";
        let result = inspector.scan(text, Uuid::nil());
        assert!(matches!(result, InspectionResult::Warning { ref pattern_name, .. } if pattern_name == "high_entropy_token"));
    }

    #[test]
    fn test_normal_text_not_flagged() {
        let inspector = OutputInspector::new();
        let text = "The function processes data and returns formatted output strings.";
        let result = inspector.scan(text, Uuid::nil());
        assert_eq!(result, InspectionResult::Clean);
    }

    #[test]
    fn test_shannon_entropy_values() {
        // Uniform distribution (all same char) → 0 bits
        assert_eq!(shannon_entropy(b"aaaaaaaaaa"), 0.0);
        // Binary string should have ~1 bit/byte
        let entropy = shannon_entropy(b"ababababab");
        assert!(entropy > 0.9 && entropy < 1.1);
        // Random-looking base64 should have high entropy
        let entropy = shannon_entropy(b"aB3dE5fG7hI9jK1lM3nO5pQ7rS9tU1vW3xY5zA7bC9dE");
        assert!(entropy > 4.0);
    }
}
