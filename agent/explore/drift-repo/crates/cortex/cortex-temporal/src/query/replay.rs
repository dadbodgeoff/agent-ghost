//! Decision replay execution — "Reconstruct the exact context available
//! when Decision X was made."
//!
//! This is the most complex query handler. It chains temporal state
//! reconstruction, retrieval simulation, causal graph reconstruction,
//! and hindsight computation into a single pipeline.
//!
//! No existing AI memory system offers decision replay. It requires the
//! intersection of temporal state reconstruction + retrieval simulation +
//! causal graph reconstruction — all three of which Cortex uniquely has.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::errors::CortexResult;
use cortex_core::memory::{BaseMemory, MemoryType, TypedContent};
use cortex_core::models::{CompressedMemory, DecisionReplay, DecisionReplayQuery, HindsightItem};
use cortex_storage::pool::ReadPool;

use super::temporal_causal;
use crate::snapshot::reconstruct;

/// Execute a decision replay query.
///
/// The 10-step algorithm:
/// 1. Load decision memory
/// 2. Verify it's a decision type (error if not)
/// 3. Get decision creation time
/// 4. Reconstruct decision state at creation time
/// 5. Reconstruct all available context at decision time
/// 6. Simulate retrieval at decision time
/// 7. Reconstruct causal graph at decision time
/// 8. Get all memories created AFTER decision time
/// 9. Filter to those with similarity > 0.7 to decision topic
/// 10. Classify each as contradicts/would_have_informed/supersedes/supports
pub fn execute_replay(
    readers: &Arc<ReadPool>,
    query: &DecisionReplayQuery,
) -> CortexResult<DecisionReplay> {
    // Step 1: Load the decision memory (current state)
    let current_memory = load_memory(readers, &query.decision_memory_id)?;

    // Step 2: Verify it's a decision type
    if !is_decision_type(&current_memory) {
        return Err(cortex_core::CortexError::TemporalError(
            cortex_core::errors::TemporalError::QueryFailed(format!(
                "memory '{}' is not a decision type (found {:?}), cannot replay",
                query.decision_memory_id, current_memory.memory_type
            )),
        ));
    }

    // Step 3: Get decision creation time
    let decision_time = current_memory.transaction_time;

    // Step 4: Reconstruct decision state at creation time.
    // If reconstruction returns a shell with wrong type (e.g., Created event
    // didn't contain full BaseMemory), fall back to current memory.
    let decision =
        match reconstruct::reconstruct_at(readers, &query.decision_memory_id, decision_time)? {
            Some(reconstructed) if is_decision_type(&reconstructed) => reconstructed,
            _ => current_memory.clone(),
        };

    // Step 5: Reconstruct all available context at decision time
    let available_context = reconstruct::reconstruct_all_at(readers, decision_time)?;

    // Step 6: Simulate retrieval at decision time
    let budget = query.budget_override.unwrap_or(2000);
    let decision_topic = extract_decision_topic(&decision);
    let retrieved_context =
        simulate_retrieval(&available_context, &decision_topic, &decision, budget);

    // Step 7: Reconstruct causal graph at decision time
    let causal_state = temporal_causal::reconstruct_causal_snapshot(readers, decision_time)?;

    // Steps 8-10: Compute hindsight
    let hindsight = compute_hindsight(readers, &decision, decision_time)?;

    Ok(DecisionReplay {
        decision,
        available_context,
        retrieved_context,
        causal_state,
        hindsight,
    })
}

/// Load a memory by ID from the database.
fn load_memory(readers: &Arc<ReadPool>, memory_id: &str) -> CortexResult<BaseMemory> {
    let mid = memory_id.to_string();
    let result = readers
        .with_conn(move |conn| cortex_storage::queries::memory_crud::get_memory(conn, &mid))?;

    result.ok_or_else(|| {
        cortex_core::CortexError::TemporalError(cortex_core::errors::TemporalError::QueryFailed(
            format!("memory '{}' not found", memory_id),
        ))
    })
}

/// Check if a memory is a decision type (Decision or DecisionContext).
fn is_decision_type(memory: &BaseMemory) -> bool {
    matches!(
        memory.memory_type,
        MemoryType::Decision | MemoryType::DecisionContext
    )
}

/// Extract the decision topic/summary for similarity comparison.
fn extract_decision_topic(decision: &BaseMemory) -> String {
    match &decision.content {
        TypedContent::Decision(dc) => {
            format!("{} {}", dc.decision, dc.rationale)
        }
        TypedContent::DecisionContext(dc) => {
            format!("{} {}", dc.decision, dc.context)
        }
        _ => decision.summary.clone(),
    }
}

/// Simulate retrieval at decision time by scoring available context
/// against the decision topic using a simplified scoring approach.
///
/// This approximates what the retrieval engine would have returned
/// without requiring the full retrieval pipeline (which needs embeddings).
/// We use text-based similarity as a proxy for embedding similarity.
fn simulate_retrieval(
    available_context: &[BaseMemory],
    decision_topic: &str,
    decision: &BaseMemory,
    budget: usize,
) -> Vec<CompressedMemory> {
    let topic_words: Vec<&str> = decision_topic
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .collect();

    let mut scored: Vec<(f64, &BaseMemory)> = available_context
        .iter()
        .filter(|m| m.id != decision.id) // Exclude the decision itself
        .map(|m| {
            let score = compute_text_relevance(m, &topic_words);
            (score, m)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Pack into budget (approximate: ~50 tokens per memory)
    let max_memories = budget / 50;
    scored
        .into_iter()
        .take(max_memories)
        .map(|(score, m)| CompressedMemory {
            memory_id: m.id.clone(),
            memory_type: m.memory_type,
            importance: m.importance,
            level: 2, // Summary + key fields
            text: m.summary.clone(),
            token_count: estimate_tokens(&m.summary),
            relevance_score: score,
        })
        .collect()
}

/// Compute text-based relevance between a memory and topic words.
/// Returns a score in [0.0, 1.0].
fn compute_text_relevance(memory: &BaseMemory, topic_words: &[&str]) -> f64 {
    if topic_words.is_empty() {
        return 0.0;
    }

    let memory_text = format!("{} {}", memory.summary, memory.tags.join(" ")).to_lowercase();

    let matches = topic_words
        .iter()
        .filter(|w| memory_text.contains(&w.to_lowercase()))
        .count();

    matches as f64 / topic_words.len() as f64
}

/// Rough token count estimate (~4 chars per token).
fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

/// Compute hindsight: memories created after the decision that are relevant.
///
/// Steps 8-10 of the replay algorithm:
/// 8. Get all memories created AFTER decision time
/// 9. Filter to those with similarity > 0.7 to decision topic
/// 10. Classify each relationship to the decision
fn compute_hindsight(
    readers: &Arc<ReadPool>,
    decision: &BaseMemory,
    decision_time: DateTime<Utc>,
) -> CortexResult<Vec<HindsightItem>> {
    let decision_topic = extract_decision_topic(decision);
    let topic_words: Vec<&str> = decision_topic
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .collect();

    // Get all memories created after decision time
    let post_decision_memories = get_memories_after(readers, decision_time)?;

    let mut hindsight: Vec<HindsightItem> = post_decision_memories
        .into_iter()
        .filter(|m| m.id != decision.id)
        .filter_map(|m| {
            let relevance = compute_text_relevance(&m, &topic_words);
            if relevance > 0.7 {
                let relationship = classify_relationship(&m, decision);
                Some(HindsightItem {
                    memory: m,
                    relevance,
                    relationship,
                })
            } else {
                None
            }
        })
        .collect();

    // Sort by relevance descending
    hindsight.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(hindsight)
}

/// Get all active memories created after a given time.
fn get_memories_after(
    readers: &Arc<ReadPool>,
    after: DateTime<Utc>,
) -> CortexResult<Vec<BaseMemory>> {
    let after_str = after.to_rfc3339();
    readers.with_conn(move |conn| {
        cortex_storage::queries::temporal_ops::get_memories_created_after(conn, &after_str)
    })
}

/// Classify how a hindsight memory relates to the decision.
///
/// Returns one of: "contradicts", "would_have_informed", "supersedes", "supports"
fn classify_relationship(memory: &BaseMemory, decision: &BaseMemory) -> String {
    // Check if the memory directly supersedes the decision
    if memory.supersedes.as_deref() == Some(&decision.id) {
        return "supersedes".to_string();
    }

    // Check if the decision is superseded by this memory
    if decision.superseded_by.as_deref() == Some(&memory.id) {
        return "supersedes".to_string();
    }

    // Check for contradiction signals in the memory content
    if has_contradiction_signals(memory, decision) {
        return "contradicts".to_string();
    }

    // Check if the memory is of a type that typically supports decisions
    if is_supporting_type(memory) {
        return "supports".to_string();
    }

    // Default: would have informed the decision
    "would_have_informed".to_string()
}

/// Check if a memory has signals that it contradicts a decision.
fn has_contradiction_signals(memory: &BaseMemory, decision: &BaseMemory) -> bool {
    // Check tags for contradiction indicators
    let contradiction_tags = [
        "deprecated",
        "obsolete",
        "incorrect",
        "revised",
        "retracted",
    ];
    let has_contradiction_tag = memory.tags.iter().any(|t| {
        contradiction_tags
            .iter()
            .any(|ct| t.to_lowercase().contains(ct))
    });

    if has_contradiction_tag {
        return true;
    }

    // Check if the memory's content type suggests contradiction
    // (e.g., a new decision that covers the same topic)
    if matches!(
        memory.memory_type,
        MemoryType::Decision | MemoryType::DecisionContext
    ) {
        // Another decision on a similar topic likely contradicts or supersedes
        let decision_topic = extract_decision_topic(decision).to_lowercase();
        let memory_topic = extract_decision_topic(memory).to_lowercase();

        // Simple overlap check
        let decision_words: std::collections::HashSet<&str> =
            decision_topic.split_whitespace().collect();
        let memory_words: std::collections::HashSet<&str> =
            memory_topic.split_whitespace().collect();
        let overlap = decision_words.intersection(&memory_words).count();
        let total = decision_words.len().max(1);

        if overlap as f64 / total as f64 > 0.5 {
            return true;
        }
    }

    false
}

/// Check if a memory type typically supports decisions.
fn is_supporting_type(memory: &BaseMemory) -> bool {
    matches!(
        memory.memory_type,
        MemoryType::Core
            | MemoryType::Semantic
            | MemoryType::Reference
            | MemoryType::PatternRationale
    )
}
