//! Denial feedback generation (Req 13 AC7).

use serde::{Deserialize, Serialize};

/// Structured rejection message injected into the agent's next prompt
/// when a tool call or proposal is denied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenialFeedback {
    /// Human-readable reason for the denial.
    pub reason: String,
    /// The specific constraint that was violated.
    pub constraint: String,
    /// Suggested alternative actions the agent can take.
    pub suggested_alternatives: Vec<String>,
}

impl DenialFeedback {
    pub fn new(reason: impl Into<String>, constraint: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            constraint: constraint.into(),
            suggested_alternatives: Vec::new(),
        }
    }

    pub fn with_alternatives(mut self, alternatives: Vec<String>) -> Self {
        self.suggested_alternatives = alternatives;
        self
    }
}
