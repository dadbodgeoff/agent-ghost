//! OutputInspector — credential exfiltration detection (A10, A34 Gap 1).
//!
//! Scans every LLM response for credential patterns BEFORE channel delivery.
//! Runs AFTER SimBoundaryEnforcer, BEFORE delivery.

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
    ]
});

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

    /// Scan text for credential patterns.
    #[tracing::instrument(skip(self, text), fields(otel.kind = "internal", text_len = text.len()))]
    pub fn scan(&self, text: &str, agent_id: Uuid) -> InspectionResult {
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

        InspectionResult::Clean
    }
}

impl Default for OutputInspector {
    fn default() -> Self {
        Self::new()
    }
}
