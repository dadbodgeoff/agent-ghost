//! Response processing — NO_REPLY handling (Req 11 AC10).

use ghost_llm::provider::LLMResponse;

/// Check if a response should be suppressed (NO_REPLY).
///
/// Empty response, or "NO_REPLY"/"HEARTBEAT_OK" with ≤300 chars → suppress.
pub fn is_no_reply(response: &LLMResponse) -> bool {
    match response {
        LLMResponse::Empty => true,
        LLMResponse::Text(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return true;
            }
            if trimmed.len() <= 300 {
                let upper = trimmed.to_uppercase();
                if upper.contains("NO_REPLY") || upper.contains("HEARTBEAT_OK") {
                    return true;
                }
            }
            false
        }
        LLMResponse::ToolCalls(_) | LLMResponse::Mixed { .. } => false,
    }
}
