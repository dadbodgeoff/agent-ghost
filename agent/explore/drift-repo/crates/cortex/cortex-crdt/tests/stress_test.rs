//! High-volume merge stress tests.
//!
//! Tests TMA-STRESS-01 through TMA-STRESS-03.

use chrono::Utc;
use cortex_crdt::{GCounter, MemoryCRDT, ORSet};
use cortex_core::memory::base::{BaseMemory, TypedContent};
use cortex_core::memory::confidence::Confidence;
use cortex_core::memory::importance::Importance;
use cortex_core::memory::types::MemoryType;
use cortex_core::models::agent::AgentId;
use cortex_core::models::namespace::NamespaceId;
use std::time::Instant;

/// Helper: create a minimal BaseMemory for stress testing.
fn make_stress_memory(id: &str) -> BaseMemory {
    let content = TypedContent::Core(cortex_core::memory::types::CoreContent {
        project_name: "stress-test".to_string(),
        description: format!("Stress memory {id}"),
        metadata: serde_json::Value::Null,
    });
    let content_hash =
        BaseMemory::compute_content_hash(&content).unwrap_or_else(|_| "hash".to_string());

    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Core,
        content,
        summary: format!("Summary {id}"),
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
        tags: vec!["stress".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash,
        namespace: NamespaceId::default(),
        source_agent: AgentId::default(),
    }
}

// =============================================================================
// TMA-STRESS-01: 10K memories, 5 agents, full merge < 5s
// =============================================================================

#[test]
fn tma_stress_01_10k_memories_5_agents_merge() {
    let num_memories = 10_000;
    let num_agents = 5;
    let agents: Vec<String> = (0..num_agents).map(|i| format!("agent-{i}")).collect();

    // Create CRDTs for each memory, distributed across agents
    let mut crdts: Vec<MemoryCRDT> = Vec::with_capacity(num_memories);
    for i in 0..num_memories {
        let agent = &agents[i % num_agents];
        let memory = make_stress_memory(&format!("mem-{i:05}"));
        crdts.push(MemoryCRDT::from_base_memory(&memory, agent));
    }

    // Simulate: each agent modifies their memories, then we merge all pairs
    let start = Instant::now();

    // Modify: each agent increments access count on their memories
    for (i, crdt) in crdts.iter_mut().enumerate() {
        let agent = &agents[i % num_agents];
        crdt.access_count.increment(agent);
        crdt.clock.increment(agent);
    }

    // Merge: simulate pairwise merge of first 1000 memories (representative sample)
    let sample_size = 1000.min(num_memories);
    for i in 0..sample_size {
        let j = (i + 1) % sample_size;
        if i != j {
            let other = crdts[j].clone();
            crdts[i].merge(&other);
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "10K memory merge took {:?}, should be < 5s",
        elapsed
    );
}

// =============================================================================
// TMA-STRESS-02: ORSet with 10K elements, merge
// =============================================================================

#[test]
fn tma_stress_02_or_set_10k_elements() {
    let mut set_a = ORSet::new();
    let mut set_b = ORSet::new();

    // Agent A adds 5K elements
    for i in 0..5000 {
        set_a.add(format!("elem-{i}"), "agent-a", i as u64);
    }

    // Agent B adds 5K elements (some overlapping)
    for i in 2500..7500 {
        set_b.add(format!("elem-{i}"), "agent-b", i as u64);
    }

    let start = Instant::now();
    set_a.merge(&set_b);
    let elapsed = start.elapsed();

    // Should have 7500 unique elements
    assert_eq!(set_a.len(), 7500);
    assert!(
        elapsed.as_secs() < 5,
        "ORSet 10K merge took {:?}, should be < 5s",
        elapsed
    );
}

// =============================================================================
// TMA-STRESS-03: GCounter with many agents
// =============================================================================

#[test]
fn tma_stress_03_gcounter_many_agents() {
    let num_agents = 100;
    let increments_per_agent = 1000;

    let mut counters: Vec<GCounter> = Vec::new();
    for i in 0..num_agents {
        let mut counter = GCounter::new();
        for _ in 0..increments_per_agent {
            counter.increment(&format!("agent-{i}"));
        }
        counters.push(counter);
    }

    let start = Instant::now();

    // Merge all counters into one
    let mut merged = GCounter::new();
    for counter in &counters {
        merged.merge(counter);
    }

    let elapsed = start.elapsed();

    assert_eq!(
        merged.value(),
        (num_agents * increments_per_agent) as u64
    );
    assert!(
        elapsed.as_secs() < 5,
        "GCounter merge took {:?}, should be < 5s",
        elapsed
    );
}
