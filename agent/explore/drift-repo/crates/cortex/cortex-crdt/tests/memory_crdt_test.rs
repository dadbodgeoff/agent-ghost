//! MemoryCRDT merge + delta tests.
//!
//! Tests TMA-CRDT-23 through TMA-CRDT-26.

use chrono::{Duration, Utc};
use cortex_crdt::{MemoryCRDT, MergeEngine, VectorClock};
use cortex_core::memory::base::{BaseMemory, TypedContent};
use cortex_core::memory::confidence::Confidence;
use cortex_core::memory::importance::Importance;
use cortex_core::memory::types::MemoryType;
use cortex_core::models::agent::AgentId;
use cortex_core::models::namespace::NamespaceId;

/// Helper: create a minimal BaseMemory for testing.
fn make_test_memory(id: &str) -> BaseMemory {
    let content = TypedContent::Core(cortex_core::memory::types::CoreContent {
        project_name: "test-project".to_string(),
        description: format!("Test memory {id}"),
        metadata: serde_json::Value::Null,
    });
    let content_hash =
        BaseMemory::compute_content_hash(&content).unwrap_or_else(|_| "hash".to_string());

    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Core,
        content,
        summary: format!("Summary for {id}"),
        transaction_time: Utc::now(),
        valid_time: Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Normal,
        last_accessed: Utc::now(),
        access_count: 0,
        linked_patterns: Vec::new(),
        linked_constraints: Vec::new(),
        linked_files: Vec::new(),
        linked_functions: Vec::new(),
        tags: vec!["test".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash,
        namespace: NamespaceId::default(),
        source_agent: AgentId::default(),
    }
}

// =============================================================================
// TMA-CRDT-23: MemoryCRDT from_base_memory round-trip
// =============================================================================

#[test]
fn tma_crdt_23_from_base_memory_round_trip() {
    let memory = make_test_memory("mem-001");
    let crdt = MemoryCRDT::from_base_memory(&memory, "agent-1");
    let round_tripped = crdt.to_base_memory();

    assert_eq!(round_tripped.id, memory.id);
    assert_eq!(round_tripped.memory_type, memory.memory_type);
    assert_eq!(round_tripped.summary, memory.summary);
    assert_eq!(round_tripped.archived, memory.archived);
    assert_eq!(round_tripped.superseded_by, memory.superseded_by);
    assert_eq!(round_tripped.tags, memory.tags);
    // Confidence: use epsilon comparison
    assert!(
        (round_tripped.confidence.value() - memory.confidence.value()).abs() < f64::EPSILON
    );
}

// =============================================================================
// TMA-CRDT-24: MemoryCRDT merge convergence
// =============================================================================

#[test]
fn tma_crdt_24_merge_convergence() {
    let memory = make_test_memory("mem-002");

    // Two agents start from the same base memory
    let mut crdt_a = MemoryCRDT::from_base_memory(&memory, "agent-a");
    let mut crdt_b = MemoryCRDT::from_base_memory(&memory, "agent-b");

    // Agent A modifies summary
    let t_a = Utc::now() + Duration::seconds(1);
    crdt_a
        .summary
        .set("Updated by A".to_string(), t_a, "agent-a".to_string());
    crdt_a.clock.increment("agent-a");

    // Agent B modifies tags
    crdt_b.tags.add("new-tag".to_string(), "agent-b", 100);
    crdt_b.clock.increment("agent-b");

    // Merge A into B and B into A
    let mut merged_ab = crdt_a.clone();
    merged_ab.merge(&crdt_b);

    let mut merged_ba = crdt_b.clone();
    merged_ba.merge(&crdt_a);

    // Both should converge to the same state
    let mem_ab = merged_ab.to_base_memory();
    let mem_ba = merged_ba.to_base_memory();

    assert_eq!(mem_ab.summary, mem_ba.summary);
    assert_eq!(mem_ab.summary, "Updated by A");

    // Both should have the new tag
    assert!(mem_ab.tags.contains(&"new-tag".to_string()));
    assert!(mem_ba.tags.contains(&"new-tag".to_string()));
}

// =============================================================================
// TMA-CRDT-25: MemoryCRDT delta computation
// =============================================================================

#[test]
fn tma_crdt_25_delta_computation() {
    let memory = make_test_memory("mem-003");
    let mut crdt = MemoryCRDT::from_base_memory(&memory, "agent-1");

    // Record the initial clock
    let initial_clock = crdt.clock.clone();

    // Make some changes
    let t = Utc::now() + Duration::seconds(1);
    crdt.summary
        .set("Updated summary".to_string(), t, "agent-1".to_string());
    crdt.clock.increment("agent-1");

    // Compute delta
    let delta = MergeEngine::compute_delta(&crdt, &initial_clock, "agent-1");

    assert_eq!(delta.memory_id, "mem-003");
    assert_eq!(delta.source_agent, "agent-1");
    assert!(!delta.field_deltas.is_empty());
}

// =============================================================================
// TMA-CRDT-26: MergeEngine causal ordering
// =============================================================================

#[test]
fn tma_crdt_26_merge_engine_causal_ordering() {
    let memory = make_test_memory("mem-004");
    let mut local = MemoryCRDT::from_base_memory(&memory, "agent-1");

    // Create a delta that claims to have seen agent-2 at clock 5,
    // but our local clock has never seen agent-2
    let mut future_clock = VectorClock::new();
    future_clock.increment("agent-1"); // This is fine
    for _ in 0..5 {
        future_clock.increment("agent-2"); // We haven't seen this
    }

    let delta = cortex_crdt::MemoryDelta {
        memory_id: "mem-004".to_string(),
        source_agent: "agent-1".to_string(),
        clock: future_clock,
        field_deltas: Vec::new(),
        timestamp: Utc::now(),
    };

    // Should fail: causal order violation (we haven't seen agent-2's updates)
    let result = MergeEngine::apply_delta(&mut local, &delta);
    assert!(result.is_err());
}

// =============================================================================
// Additional merge tests
// =============================================================================

#[test]
fn memory_crdt_merge_confidence_max_wins() {
    let memory = make_test_memory("mem-005");

    let mut crdt_a = MemoryCRDT::from_base_memory(&memory, "agent-a");
    let mut crdt_b = MemoryCRDT::from_base_memory(&memory, "agent-b");

    // Agent A boosts confidence
    crdt_a.base_confidence.set(0.95);

    // Agent B has lower confidence
    crdt_b.base_confidence.set(0.85);

    crdt_a.merge(&crdt_b);
    assert!((*crdt_a.base_confidence.get() - 0.95).abs() < f64::EPSILON);
}

#[test]
fn memory_crdt_merge_access_count_sums() {
    let memory = make_test_memory("mem-006");

    let mut crdt_a = MemoryCRDT::from_base_memory(&memory, "agent-a");
    let mut crdt_b = MemoryCRDT::from_base_memory(&memory, "agent-b");

    // Agent A accesses 3 more times (1 from init + 3 = 4)
    crdt_a.access_count.increment("agent-a");
    crdt_a.access_count.increment("agent-a");
    crdt_a.access_count.increment("agent-a");

    // Agent B accesses 2 more times (1 from init + 2 = 3)
    crdt_b.access_count.increment("agent-b");
    crdt_b.access_count.increment("agent-b");

    crdt_a.merge(&crdt_b);
    assert_eq!(crdt_a.access_count.value(), 7); // 4 + 3
}
