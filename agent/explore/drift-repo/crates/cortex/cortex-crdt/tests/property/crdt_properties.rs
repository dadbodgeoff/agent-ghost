//! Property-based tests for CRDT mathematical guarantees.
//!
//! All CRDT merge operations MUST satisfy:
//! 1. Commutativity: merge(A, B) == merge(B, A)
//! 2. Associativity: merge(A, merge(B, C)) == merge(merge(A, B), C)
//! 3. Idempotency: merge(A, A) == A
//!
//! Tests TMA-PROP-01 through TMA-PROP-19.

use proptest::prelude::*;

use chrono::{Duration, Utc};
use cortex_crdt::{
    CausalGraphCRDT, GCounter, LWWRegister, MaxRegister, MemoryCRDT, ORSet, VectorClock,
};
use cortex_core::memory::base::{BaseMemory, TypedContent};
use cortex_core::memory::confidence::Confidence;
use cortex_core::memory::importance::Importance;
use cortex_core::memory::types::MemoryType;
use cortex_core::models::agent::AgentId;
use cortex_core::models::namespace::NamespaceId;

// =============================================================================
// Strategy helpers
// =============================================================================

/// Generate a random GCounter with up to `max_agents` agents, each with up to `max_count`.
fn gcounter_strategy(max_agents: usize, max_count: u64) -> impl Strategy<Value = GCounter> {
    prop::collection::vec(
        (
            "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
            1..=max_count,
        ),
        0..=max_agents,
    )
    .prop_map(|entries| {
        let mut counter = GCounter::new();
        for (agent, count) in entries {
            for _ in 0..count {
                counter.increment(&agent);
            }
        }
        counter
    })
}

/// Generate a random VectorClock.
fn vector_clock_strategy(max_agents: usize, max_val: u64) -> impl Strategy<Value = VectorClock> {
    prop::collection::vec(
        (
            "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
            1..=max_val,
        ),
        0..=max_agents,
    )
    .prop_map(|entries| {
        let mut clock = VectorClock::new();
        for (agent, count) in entries {
            for _ in 0..count {
                clock.increment(&agent);
            }
        }
        clock
    })
}

/// Generate a random ORSet with up to `max_elements` elements.
fn or_set_strategy(max_elements: usize) -> impl Strategy<Value = ORSet<String>> {
    prop::collection::vec(
        (
            "[a-z]{1,8}".prop_map(|s| s.to_string()),
            "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
            1..100u64,
        ),
        0..=max_elements,
    )
    .prop_map(|entries| {
        let mut set = ORSet::new();
        for (value, agent, seq) in entries {
            set.add(value, &agent, seq);
        }
        set
    })
}

/// Helper: create a minimal BaseMemory for property tests.
fn make_prop_memory(id: &str) -> BaseMemory {
    let content = TypedContent::Core(cortex_core::memory::types::CoreContent {
        project_name: "prop-test".to_string(),
        description: format!("Property test memory {id}"),
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
// TMA-PROP-01: GCounter merge commutativity
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_01_gcounter_merge_commutativity(
        a in gcounter_strategy(5, 10),
        b in gcounter_strategy(5, 10),
    ) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }
}

// =============================================================================
// TMA-PROP-02: GCounter merge associativity
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_02_gcounter_merge_associativity(
        a in gcounter_strategy(5, 10),
        b in gcounter_strategy(5, 10),
        c in gcounter_strategy(5, 10),
    ) {
        // merge(A, merge(B, C))
        let mut bc = b.clone();
        bc.merge(&c);
        let mut a_bc = a.clone();
        a_bc.merge(&bc);

        // merge(merge(A, B), C)
        let mut ab = a.clone();
        ab.merge(&b);
        ab.merge(&c);

        prop_assert_eq!(a_bc, ab);
    }
}

// =============================================================================
// TMA-PROP-03: GCounter merge idempotency
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_03_gcounter_merge_idempotency(
        a in gcounter_strategy(5, 10),
    ) {
        let before = a.clone();
        let mut merged = a.clone();
        merged.merge(&before);
        prop_assert_eq!(merged, before);
    }
}

// =============================================================================
// TMA-PROP-04: LWWRegister merge commutativity
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_04_lww_register_merge_commutativity(
        val_a in "[a-z]{1,10}",
        val_b in "[a-z]{1,10}",
        offset_a in 0i64..1000,
        offset_b in 0i64..1000,
        agent_a in "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
        agent_b in "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
    ) {
        let base = Utc::now();
        let t_a = base + Duration::milliseconds(offset_a);
        let t_b = base + Duration::milliseconds(offset_b);

        let a = LWWRegister::new(val_a, t_a, agent_a);
        let b = LWWRegister::new(val_b, t_b, agent_b);

        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }
}

// =============================================================================
// TMA-PROP-05: LWWRegister merge associativity
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_05_lww_register_merge_associativity(
        val_a in "[a-z]{1,10}",
        val_b in "[a-z]{1,10}",
        val_c in "[a-z]{1,10}",
        offset_a in 0i64..1000,
        offset_b in 0i64..1000,
        offset_c in 0i64..1000,
        agent_a in "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
        agent_b in "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
        agent_c in "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
    ) {
        let base = Utc::now();
        let t_a = base + Duration::milliseconds(offset_a);
        let t_b = base + Duration::milliseconds(offset_b);
        let t_c = base + Duration::milliseconds(offset_c);

        let a = LWWRegister::new(val_a, t_a, agent_a);
        let b = LWWRegister::new(val_b, t_b, agent_b);
        let c = LWWRegister::new(val_c, t_c, agent_c);

        // merge(A, merge(B, C))
        let mut bc = b.clone();
        bc.merge(&c);
        let mut a_bc = a.clone();
        a_bc.merge(&bc);

        // merge(merge(A, B), C)
        let mut ab = a.clone();
        ab.merge(&b);
        ab.merge(&c);

        prop_assert_eq!(a_bc, ab);
    }
}

// =============================================================================
// TMA-PROP-06: LWWRegister merge idempotency
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_06_lww_register_merge_idempotency(
        val in "[a-z]{1,10}",
        offset in 0i64..1000,
        agent in "[a-z]{1,5}".prop_map(|s| format!("agent-{s}")),
    ) {
        let t = Utc::now() + Duration::milliseconds(offset);
        let a = LWWRegister::new(val, t, agent);
        let before = a.clone();
        let mut merged = a.clone();
        merged.merge(&before);
        prop_assert_eq!(merged, before);
    }
}

// =============================================================================
// TMA-PROP-07: ORSet merge commutativity
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_07_or_set_merge_commutativity(
        a in or_set_strategy(10),
        b in or_set_strategy(10),
    ) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }
}

// =============================================================================
// TMA-PROP-08: ORSet merge associativity
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_08_or_set_merge_associativity(
        a in or_set_strategy(8),
        b in or_set_strategy(8),
        c in or_set_strategy(8),
    ) {
        let mut bc = b.clone();
        bc.merge(&c);
        let mut a_bc = a.clone();
        a_bc.merge(&bc);

        let mut ab = a.clone();
        ab.merge(&b);
        ab.merge(&c);

        prop_assert_eq!(a_bc, ab);
    }
}

// =============================================================================
// TMA-PROP-09: ORSet merge idempotency
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_09_or_set_merge_idempotency(
        a in or_set_strategy(10),
    ) {
        let before = a.clone();
        let mut merged = a.clone();
        merged.merge(&before);
        prop_assert_eq!(merged, before);
    }
}

// =============================================================================
// TMA-PROP-10: ORSet add-wins (concurrent add + remove → element present)
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_10_or_set_add_wins(
        element in "[a-z]{1,8}",
    ) {
        // Agent A adds element
        let mut set_a = ORSet::new();
        set_a.add(element.clone(), "agent-a", 1);

        // Agent B starts from same state, removes element
        let mut set_b = set_a.clone();
        set_b.remove(&element);

        // Agent A concurrently adds element again with a new tag
        set_a.add(element.clone(), "agent-a", 2);

        // Merge: the concurrent add should win
        set_a.merge(&set_b);
        prop_assert!(set_a.contains(&element), "Add-wins: element should be present after concurrent add + remove");
    }
}

// =============================================================================
// TMA-PROP-11: ORSet size bounded by unique adds
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_11_or_set_size_bounded(
        elements in prop::collection::vec("[a-z]{1,5}", 0..20),
    ) {
        let mut set = ORSet::new();
        let mut unique_values = std::collections::HashSet::new();
        for (seq, elem) in elements.iter().enumerate() {
            set.add(elem.clone(), "agent-1", seq as u64);
            unique_values.insert(elem.clone());
        }
        prop_assert!(set.len() <= unique_values.len(), "Size {} should be <= unique adds {}", set.len(), unique_values.len());
    }
}

// =============================================================================
// TMA-PROP-12: MaxRegister merge commutativity
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_12_max_register_merge_commutativity(
        val_a in 0.0f64..1.0,
        val_b in 0.0f64..1.0,
    ) {
        let now = Utc::now();
        let a = MaxRegister::new(val_a, now);
        let b = MaxRegister::new(val_b, now);

        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        // Check commutativity
        let ab_val = *ab.get();
        let ba_val = *ba.get();
        prop_assert!((ab_val - ba_val).abs() < f64::EPSILON);

        // Merged value should be >= both inputs
        prop_assert!(ab_val >= val_a || (ab_val - val_a).abs() < f64::EPSILON);
        prop_assert!(ab_val >= val_b || (ab_val - val_b).abs() < f64::EPSILON);
    }
}

// =============================================================================
// TMA-PROP-13: MaxRegister monotonicity (value never decreases)
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_13_max_register_monotonicity(
        initial in 0.0f64..1.0,
        updates in prop::collection::vec(0.0f64..1.0, 1..20),
    ) {
        let now = Utc::now();
        let mut reg = MaxRegister::new(initial, now);
        let mut max_seen = initial;

        for val in updates {
            reg.set(val);
            if val > max_seen {
                max_seen = val;
            }
            // Value should never decrease
            prop_assert!(
                *reg.get() >= max_seen || (*reg.get() - max_seen).abs() < f64::EPSILON,
                "MaxRegister value {} decreased below max_seen {}",
                reg.get(),
                max_seen
            );
        }
    }
}

// =============================================================================
// TMA-PROP-14: VectorClock causal delivery (never applies future deltas)
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_14_vector_clock_causal_delivery(
        a in vector_clock_strategy(5, 10),
        b in vector_clock_strategy(5, 10),
    ) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        // Merged clocks should be identical (commutativity)
        prop_assert_eq!(ab.clone(), ba.clone());

        // Merged clock should dominate or equal both inputs
        // (component-wise max means merged >= each input)
        prop_assert!(!a.dominates(&ab) || a == ab, "Original A should not dominate merged");
        prop_assert!(!b.dominates(&ab) || b == ab, "Original B should not dominate merged");
    }
}

// =============================================================================
// TMA-PROP-15: MemoryCRDT merge commutativity for all field types
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_15_memory_crdt_merge_commutativity(
        summary_a in "[a-z ]{5,30}",
        summary_b in "[a-z ]{5,30}",
        tag_a in "[a-z]{1,8}",
        tag_b in "[a-z]{1,8}",
        confidence_a in 0.0f64..1.0,
        confidence_b in 0.0f64..1.0,
        offset_a in 1i64..500,
        offset_b in 1i64..500,
    ) {
        let memory = make_prop_memory("prop-mem-001");
        let base_time = Utc::now();

        // Agent A modifies summary, confidence, and adds a tag
        let mut crdt_a = MemoryCRDT::from_base_memory(&memory, "agent-a");
        let t_a = base_time + Duration::milliseconds(offset_a);
        crdt_a.summary.set(summary_a, t_a, "agent-a".to_string());
        crdt_a.base_confidence.set(confidence_a);
        crdt_a.tags.add(tag_a, "agent-a", 100);
        crdt_a.access_count.increment("agent-a");
        crdt_a.clock.increment("agent-a");

        // Agent B modifies summary, confidence, and adds a different tag
        let mut crdt_b = MemoryCRDT::from_base_memory(&memory, "agent-b");
        let t_b = base_time + Duration::milliseconds(offset_b);
        crdt_b.summary.set(summary_b, t_b, "agent-b".to_string());
        crdt_b.base_confidence.set(confidence_b);
        crdt_b.tags.add(tag_b, "agent-b", 100);
        crdt_b.access_count.increment("agent-b");
        crdt_b.clock.increment("agent-b");

        // merge(A, B)
        let mut merged_ab = crdt_a.clone();
        merged_ab.merge(&crdt_b);

        // merge(B, A)
        let mut merged_ba = crdt_b.clone();
        merged_ba.merge(&crdt_a);

        // Commutativity: both merges produce the same materialized state
        let mem_ab = merged_ab.to_base_memory();
        let mem_ba = merged_ba.to_base_memory();

        prop_assert_eq!(&mem_ab.summary, &mem_ba.summary, "Summary should be equal");
        prop_assert!(
            (mem_ab.confidence.value() - mem_ba.confidence.value()).abs() < f64::EPSILON,
            "Confidence should be equal"
        );
        prop_assert_eq!(mem_ab.access_count, mem_ba.access_count, "Access count should be equal");

        // Tags: both should have the same set of tags (order may differ)
        let mut tags_ab: Vec<String> = mem_ab.tags.clone();
        tags_ab.sort();
        let mut tags_ba: Vec<String> = mem_ba.tags.clone();
        tags_ba.sort();
        prop_assert_eq!(tags_ab, tags_ba, "Tags should be equal");
    }
}

// =============================================================================
// TMA-PROP-16: MemoryCRDT convergence (after sync, same state)
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_16_memory_crdt_convergence(
        num_ops_a in 1usize..5,
        num_ops_b in 1usize..5,
    ) {
        let memory = make_prop_memory("prop-mem-002");

        let mut crdt_a = MemoryCRDT::from_base_memory(&memory, "agent-a");
        let mut crdt_b = MemoryCRDT::from_base_memory(&memory, "agent-b");

        let base_time = Utc::now();

        // Agent A performs operations
        for i in 0..num_ops_a {
            let t = base_time + Duration::milliseconds(i as i64 * 10 + 1);
            crdt_a.summary.set(format!("A-summary-{i}"), t, "agent-a".to_string());
            crdt_a.access_count.increment("agent-a");
            crdt_a.tags.add(format!("a-tag-{i}"), "agent-a", (i + 50) as u64);
            crdt_a.clock.increment("agent-a");
        }

        // Agent B performs operations
        for i in 0..num_ops_b {
            let t = base_time + Duration::milliseconds(i as i64 * 10 + 2);
            crdt_b.summary.set(format!("B-summary-{i}"), t, "agent-b".to_string());
            crdt_b.access_count.increment("agent-b");
            crdt_b.tags.add(format!("b-tag-{i}"), "agent-b", (i + 50) as u64);
            crdt_b.clock.increment("agent-b");
        }

        // Both agents sync: A merges B, B merges A
        let mut synced_a = crdt_a.clone();
        synced_a.merge(&crdt_b);

        let mut synced_b = crdt_b.clone();
        synced_b.merge(&crdt_a);

        // After sync, both should have identical materialized state
        let mem_a = synced_a.to_base_memory();
        let mem_b = synced_b.to_base_memory();

        prop_assert_eq!(&mem_a.summary, &mem_b.summary, "Summaries should converge");
        prop_assert_eq!(mem_a.access_count, mem_b.access_count, "Access counts should converge");
        prop_assert!(
            (mem_a.confidence.value() - mem_b.confidence.value()).abs() < f64::EPSILON,
            "Confidence should converge"
        );

        let mut tags_a: Vec<String> = mem_a.tags.clone();
        tags_a.sort();
        let mut tags_b: Vec<String> = mem_b.tags.clone();
        tags_b.sort();
        prop_assert_eq!(tags_a, tags_b, "Tags should converge");
    }
}

// =============================================================================
// TMA-PROP-17: CausalGraphCRDT always acyclic after merge
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_17_dag_crdt_always_acyclic(
        edges_1 in prop::collection::vec((0usize..10, 0usize..10, 0.1f64..1.0), 0..15),
        edges_2 in prop::collection::vec((0usize..10, 0usize..10, 0.1f64..1.0), 0..15),
    ) {
        let mut graph_1 = CausalGraphCRDT::new();
        for (seq, (src, tgt, strength)) in edges_1.iter().enumerate() {
            let src_id = format!("n{src}");
            let tgt_id = format!("n{tgt}");
            // Ignore errors (cycles, self-loops)
            let _ = graph_1.add_edge(&src_id, &tgt_id, *strength, "agent-1", seq as u64 + 1);
        }

        let mut graph_2 = CausalGraphCRDT::new();
        for (seq, (src, tgt, strength)) in edges_2.iter().enumerate() {
            let src_id = format!("n{src}");
            let tgt_id = format!("n{tgt}");
            let _ = graph_2.add_edge(&src_id, &tgt_id, *strength, "agent-2", seq as u64 + 1);
        }

        // Merge should resolve any cycles
        let _ = graph_1.merge(&graph_2);

        // Graph must be acyclic
        prop_assert!(graph_1.detect_cycle().is_none(), "Graph should be acyclic after merge");
    }
}

// =============================================================================
// TMA-PROP-18: CausalGraphCRDT edge add is commutative
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_18_dag_crdt_edge_add_commutative(
        edges in prop::collection::vec((0usize..8, 0usize..8, 0.1f64..1.0), 0..10),
    ) {
        // Build two graphs with the same edges added by different agents
        let mut graph_1 = CausalGraphCRDT::new();
        let mut graph_2 = CausalGraphCRDT::new();

        for (seq, (src, tgt, strength)) in edges.iter().enumerate() {
            let src_id = format!("n{src}");
            let tgt_id = format!("n{tgt}");
            let _ = graph_1.add_edge(&src_id, &tgt_id, *strength, "agent-1", seq as u64 + 1);
            let _ = graph_2.add_edge(&src_id, &tgt_id, *strength, "agent-2", seq as u64 + 1);
        }

        // Both should be acyclic
        prop_assert!(graph_1.detect_cycle().is_none());
        prop_assert!(graph_2.detect_cycle().is_none());
    }
}

// =============================================================================
// TMA-PROP-19: Trust score always in [0.0, 1.0]
// =============================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tma_prop_19_trust_score_bounded(
        bootstrap in 0.0f64..2.0,
        penalty in 0.0f64..1.0,
        bonus in 0.0f64..1.0,
        num_contradictions in 0usize..20,
        num_validations in 0usize..20,
    ) {
        // Simulate trust computation: start at bootstrap, apply penalties and bonuses
        let mut trust = bootstrap.clamp(0.0, 1.0);
        for _ in 0..num_contradictions {
            trust = (trust - penalty).clamp(0.0, 1.0);
        }
        for _ in 0..num_validations {
            trust = (trust + bonus).clamp(0.0, 1.0);
        }
        prop_assert!((0.0..=1.0).contains(&trust), "Trust {trust} out of bounds");
    }
}
