//! ProposalExtractor — parses proposals from agent text output (Req 11 AC7).

use cortex_core::memory::types::MemoryType;
use cortex_core::models::proposal::ProposalOperation;
use cortex_core::traits::convergence::{CallerType, Proposal};
use once_cell::sync::Lazy;
use regex::Regex;
use uuid::Uuid;

/// Regex for extracting proposal blocks from agent output.
/// Format: ```proposal\n{json}\n```
static PROPOSAL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)```proposal\s*\n(.*?)\n```").unwrap()
});

/// Extracted raw proposal data before validation.
#[derive(Debug, Clone)]
pub struct RawProposal {
    pub operation: ProposalOperation,
    pub target_type: MemoryType,
    pub content: serde_json::Value,
    pub cited_memory_ids: Vec<Uuid>,
}

/// Extracts proposals from agent text output.
pub struct ProposalExtractor;

impl ProposalExtractor {
    /// Extract all proposals from agent output text.
    pub fn extract(text: &str, agent_id: Uuid, session_id: Uuid) -> Vec<Proposal> {
        let mut proposals = Vec::new();

        for cap in PROPOSAL_REGEX.captures_iter(text) {
            if let Some(json_str) = cap.get(1) {
                match serde_json::from_str::<RawProposal>(json_str.as_str()) {
                    Ok(raw) => {
                        let proposal = Proposal {
                            id: Uuid::now_v7(),
                            proposer: CallerType::Agent { agent_id },
                            operation: raw.operation,
                            target_type: raw.target_type,
                            content: raw.content,
                            cited_memory_ids: raw.cited_memory_ids,
                            session_id,
                            timestamp: chrono::Utc::now(),
                        };
                        proposals.push(proposal);
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "failed to parse proposal from agent output"
                        );
                    }
                }
            }
        }

        proposals
    }

    /// Check if text contains any proposal blocks.
    pub fn has_proposals(text: &str) -> bool {
        PROPOSAL_REGEX.is_match(text)
    }
}

// Implement Deserialize for RawProposal
impl<'de> serde::Deserialize<'de> for RawProposal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Helper {
            operation: ProposalOperation,
            target_type: MemoryType,
            content: serde_json::Value,
            #[serde(default)]
            cited_memory_ids: Vec<Uuid>,
        }

        let h = Helper::deserialize(deserializer)?;
        Ok(RawProposal {
            operation: h.operation,
            target_type: h.target_type,
            content: h.content,
            cited_memory_ids: h.cited_memory_ids,
        })
    }
}
