//! Mem0-inspired dedup: check existing memories → ADD, UPDATE, or NOOP.

use cortex_core::memory::BaseMemory;

/// Dedup decision for a new learning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DedupAction {
    /// Create a new memory.
    Add,
    /// Update an existing memory (ID provided).
    Update(String),
    /// No action needed — duplicate already exists.
    Noop,
}

/// Check if a new principle already exists among existing memories.
///
/// Uses content hash comparison for exact dedup, and summary similarity
/// for fuzzy dedup.
pub fn check_dedup(
    new_content_hash: &str,
    new_summary: &str,
    existing: &[BaseMemory],
) -> DedupAction {
    // Exact match by content hash.
    for mem in existing {
        if mem.content_hash == new_content_hash {
            return DedupAction::Noop;
        }
    }

    // Fuzzy match by summary similarity (simple word overlap).
    for mem in existing {
        let similarity = word_overlap_similarity(new_summary, &mem.summary);
        if similarity > 0.9 {
            return DedupAction::Update(mem.id.clone());
        }
    }

    DedupAction::Add
}

/// Simple word overlap similarity (Jaccard index).
fn word_overlap_similarity(a: &str, b: &str) -> f64 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();

    if a_words.is_empty() && b_words.is_empty() {
        return 1.0;
    }

    let intersection = a_words.intersection(&b_words).count() as f64;
    let union = a_words.union(&b_words).count() as f64;

    if union < f64::EPSILON {
        0.0
    } else {
        intersection / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cortex_core::memory::types::InsightContent;
    use cortex_core::memory::*;

    fn make_insight(summary: &str, hash: &str) -> BaseMemory {
        let content = TypedContent::Insight(InsightContent {
            observation: summary.to_string(),
            evidence: vec![],
        });
        BaseMemory {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type: MemoryType::Insight,
            content,
            summary: summary.to_string(),
            transaction_time: Utc::now(),
            valid_time: Utc::now(),
            valid_until: None,
            confidence: Confidence::new(0.8),
            importance: Importance::Normal,
            last_accessed: Utc::now(),
            access_count: 0,
            linked_patterns: vec![],
            linked_constraints: vec![],
            linked_files: vec![],
            linked_functions: vec![],
            tags: vec![],
            archived: false,
            superseded_by: None,
            supersedes: None,
            namespace: Default::default(),
            source_agent: Default::default(),
            content_hash: hash.to_string(),
        }
    }

    #[test]
    fn exact_hash_match_returns_noop() {
        let existing = vec![make_insight("test", "hash123")];
        assert_eq!(check_dedup("hash123", "test", &existing), DedupAction::Noop);
    }

    #[test]
    fn similar_summary_returns_update() {
        let existing = vec![make_insight(
            "always validate user input before processing",
            "different-hash",
        )];
        let action = check_dedup(
            "new-hash",
            "always validate user input before processing",
            &existing,
        );
        match action {
            DedupAction::Update(_) => {}
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn no_match_returns_add() {
        let existing = vec![make_insight("something else entirely", "other-hash")];
        assert_eq!(
            check_dedup("new-hash", "brand new principle", &existing),
            DedupAction::Add
        );
    }
}
