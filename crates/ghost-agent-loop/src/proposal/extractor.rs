//! ProposalExtractor — parses proposals from agent text output (Req 11 AC7).
//!
//! WP3-C: Uses a state-machine parser that correctly handles nested code fences
//! (e.g. proposal JSON containing markdown with triple-backtick code blocks).
//! Falls back to JSON key scanning if the regex/state-machine yields nothing.

use cortex_core::memory::types::MemoryType;
use cortex_core::models::proposal::ProposalOperation;
use cortex_core::traits::convergence::{CallerType, Proposal};
use uuid::Uuid;

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
    ///
    /// Uses a state-machine parser that counts backtick sequences to handle
    /// nested code fences correctly. Falls back to JSON key extraction if
    /// the state machine yields nothing but the text contains proposal keys.
    #[tracing::instrument(skip(text), fields(otel.kind = "internal", text_len = text.len()))]
    pub fn extract(text: &str, agent_id: Uuid, session_id: Uuid) -> Vec<Proposal> {
        let json_blocks = extract_proposal_blocks(text);

        let mut proposals = Vec::new();
        for json_str in &json_blocks {
            match serde_json::from_str::<RawProposal>(json_str) {
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
                        json_len = json_str.len(),
                        "failed to parse proposal from agent output"
                    );
                }
            }
        }

        // Fallback: if state-machine found nothing but text contains proposal keys,
        // try to extract JSON objects containing those keys.
        if proposals.is_empty()
            && text.contains("\"operation\"")
            && text.contains("\"target_type\"")
        {
            tracing::debug!("state-machine found no proposals, trying JSON fallback extraction");
            if let Some(raw) = try_extract_json_fallback(text) {
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
        }

        proposals
    }

    /// Check if text contains any proposal blocks.
    pub fn has_proposals(text: &str) -> bool {
        text.contains("```proposal")
            || (text.contains("\"operation\"") && text.contains("\"target_type\""))
    }
}

/// State-machine parser for extracting ```proposal blocks.
/// Correctly handles nested code fences by tracking backtick nesting depth.
fn extract_proposal_blocks(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Look for opening ```proposal fence
        if trimmed.starts_with("```proposal") {
            i += 1;
            let mut block = String::new();
            let mut nesting_depth: u32 = 0;

            while i < lines.len() {
                let line = lines[i];
                let line_trimmed = line.trim();

                // Count backtick sequences
                if line_trimmed.starts_with("```") {
                    if nesting_depth == 0 && !line_trimmed[3..].trim().is_empty() {
                        // Opening a nested code fence (e.g. ```json or ```rust)
                        nesting_depth += 1;
                    } else if nesting_depth > 0 {
                        // Could be closing a nested fence or opening another
                        nesting_depth -= 1;
                    } else {
                        // Closing the proposal fence at depth 0
                        break;
                    }
                }

                if !block.is_empty() {
                    block.push('\n');
                }
                block.push_str(line);
                i += 1;
            }

            if !block.trim().is_empty() {
                results.push(block);
            }
        }
        i += 1;
    }

    results
}

/// Fallback: scan text for JSON objects that look like proposals.
/// Finds balanced `{...}` blocks containing required proposal keys.
fn try_extract_json_fallback(text: &str) -> Option<RawProposal> {
    let mut start = 0;
    while let Some(pos) = text[start..].find('{') {
        let abs_pos = start + pos;
        // Find matching closing brace
        let mut depth = 0i32;
        let mut end = None;
        for (i, ch) in text[abs_pos..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(abs_pos + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }

        if let Some(end_pos) = end {
            let candidate = &text[abs_pos..end_pos];
            if candidate.contains("\"operation\"") && candidate.contains("\"target_type\"") {
                if let Ok(raw) = serde_json::from_str::<RawProposal>(candidate) {
                    return Some(raw);
                }
            }
            start = end_pos;
        } else {
            break;
        }
    }
    None
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
