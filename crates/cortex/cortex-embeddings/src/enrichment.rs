//! Embedding enrichment — prepend structured metadata before embedding.
//!
//! Format: `[{type}|{importance}] {summary} Tags: {tags}`
//!
//! This improves embedding quality by giving the model type-aware context,
//! so semantically similar memories of different types cluster appropriately.

use cortex_core::memory::BaseMemory;

/// Enrich a memory's text representation for embedding.
///
/// Prepends structured metadata so the embedding model can distinguish
/// between memory types and importance levels.
pub fn enrich_for_embedding(memory: &BaseMemory) -> String {
    let mut parts = Vec::with_capacity(4);

    // Metadata prefix: [type|importance]
    let prefix = format!(
        "[{:?}|{:?}]",
        memory.memory_type,
        memory.importance,
    );
    parts.push(prefix);

    // Summary.
    if !memory.summary.is_empty() {
        parts.push(memory.summary.clone());
    }

    // Tags (replaces linked_files/linked_patterns from drift-repo version).
    if !memory.tags.is_empty() {
        parts.push(format!("Tags: {}", memory.tags.join(", ")));
    }

    parts.join(" ")
}

/// Enrich a plain text string with a query prefix.
///
/// Used when embedding queries or text that isn't a full BaseMemory.
pub fn enrich_query(text: &str) -> String {
    format!("[Query] {text}")
}

/// Compute a content hash for a memory (used as cache key).
///
/// The active BaseMemory has no `content_hash` field, so we derive one
/// from blake3(summary + content JSON).
pub fn content_hash(memory: &BaseMemory) -> String {
    let input = format!("{}{}", memory.summary, memory.content);
    blake3::hash(input.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cortex_core::memory::{Importance, types::MemoryType};

    fn make_memory() -> BaseMemory {
        BaseMemory {
            id: uuid::Uuid::new_v4(),
            memory_type: MemoryType::Conversation,
            content: serde_json::json!({"text": "Always use prepared statements"}),
            summary: "Use prepared statements for SQL".to_string(),
            importance: Importance::High,
            confidence: 0.9,
            created_at: Utc::now(),
            last_accessed: Some(Utc::now()),
            access_count: 5,
            tags: vec!["sql".to_string(), "security".to_string()],
            archived: false,
        }
    }

    #[test]
    fn enrichment_includes_metadata_prefix() {
        let mem = make_memory();
        let enriched = enrich_for_embedding(&mem);
        assert!(enriched.starts_with("[Conversation|High]"));
    }

    #[test]
    fn enrichment_includes_summary() {
        let mem = make_memory();
        let enriched = enrich_for_embedding(&mem);
        assert!(enriched.contains("Use prepared statements for SQL"));
    }

    #[test]
    fn enrichment_includes_tags() {
        let mem = make_memory();
        let enriched = enrich_for_embedding(&mem);
        assert!(enriched.contains("Tags: sql, security"));
    }

    #[test]
    fn query_enrichment() {
        let enriched = enrich_query("how to handle SQL injection");
        assert_eq!(enriched, "[Query] how to handle SQL injection");
    }

    #[test]
    fn content_hash_deterministic() {
        let mem = make_memory();
        let h1 = content_hash(&mem);
        let h2 = content_hash(&mem);
        assert_eq!(h1, h2);
    }

    #[test]
    fn content_hash_changes_with_content() {
        let mut mem = make_memory();
        let h1 = content_hash(&mem);
        mem.summary = "Different summary".to_string();
        let h2 = content_hash(&mem);
        assert_ne!(h1, h2);
    }
}
