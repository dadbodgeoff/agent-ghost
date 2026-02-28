//! Session-aware deduplication.
//!
//! Filters already-sent memories to achieve 30–50% token savings.
//! Multi-agent extension: dedup key includes agent_id and namespace_id,
//! allowing different agents to have the same content in different namespaces.

use cortex_core::models::agent::AgentId;
use cortex_core::models::namespace::NamespaceId;

use crate::manager::SessionManager;

/// Result of deduplication filtering.
#[derive(Debug, Clone)]
pub struct DeduplicationResult {
    /// Memory IDs that should be sent (not yet seen in this session).
    pub to_send: Vec<String>,
    /// Memory IDs filtered out (already sent).
    pub filtered: Vec<String>,
    /// Estimated tokens saved by deduplication.
    pub tokens_saved: usize,
}

/// Compute a dedup key that includes agent and namespace context.
///
/// Old key: `(session_id, content_hash)` — still works when all memories
/// have default agent/namespace.
/// New key: `(session_id, agent_id, namespace_id, content_hash)` — allows
/// same content from different agents without dedup.
pub fn dedup_key(
    session_id: &str,
    agent_id: &AgentId,
    namespace_id: &NamespaceId,
    content_hash: &str,
) -> String {
    format!(
        "{}:{}:{}:{}",
        session_id,
        agent_id.0,
        namespace_id.to_uri(),
        content_hash
    )
}

/// Filter a list of candidate memory IDs, removing those already sent in the session.
///
/// `token_estimates` maps memory_id → estimated token count.
pub fn filter_duplicates(
    session_manager: &SessionManager,
    session_id: &str,
    candidate_ids: &[String],
    token_estimates: &std::collections::HashMap<String, usize>,
) -> DeduplicationResult {
    let mut to_send = Vec::new();
    let mut filtered = Vec::new();
    let mut tokens_saved = 0;

    for id in candidate_ids {
        if session_manager.is_memory_sent(session_id, id) {
            filtered.push(id.clone());
            tokens_saved += token_estimates.get(id).copied().unwrap_or(0);
        } else {
            to_send.push(id.clone());
        }
    }

    DeduplicationResult {
        to_send,
        filtered,
        tokens_saved,
    }
}
