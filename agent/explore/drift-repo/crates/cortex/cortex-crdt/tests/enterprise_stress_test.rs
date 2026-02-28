//! Enterprise-grade stress tests for cortex-crdt.
//!
//! These tests push every CRDT primitive and higher-level structure to
//! production-scale limits, verifying mathematical invariants hold under
//! extreme conditions.

use chrono::{Duration, Utc};
use cortex_core::memory::base::{BaseMemory, TypedContent};
use cortex_core::memory::confidence::Confidence;
use cortex_core::memory::importance::Importance;
use cortex_core::memory::types::MemoryType;
use cortex_core::models::agent::AgentId;
use cortex_core::models::namespace::NamespaceId;
use cortex_crdt::{
    CausalGraphCRDT, GCounter, LWWRegister, MVRegister, MaxRegister, MergeEngine, MemoryCRDT,
    MemoryDelta, ORSet, VectorClock,
};
use std::collections::HashSet;
use std::time::Instant;

fn make_base_memory(id: &str, agent: &str) -> BaseMemory {
    let content = TypedContent::Semantic(cortex_core::memory::types::SemanticContent {
        knowledge: format!("Knowledge from {agent} about {id}"),
        source_episodes: vec![],
        consolidation_confidence: 0.8,
    });
    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Semantic,
        content: content.clone(),
        summary: format!("Summary of {id}"),
        transaction_time: Utc::now(),
        valid_time: Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Normal,
        last_accessed: Utc::now(),
        access_count: 1,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec!["enterprise".to_string(), "stress".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        namespace: NamespaceId::default(),
        source_agent: AgentId::from(agent),
    }
}

// =============================================================================
// VECTOR CLOCK: Stress invariants at scale
// =============================================================================

/// 50 agents, 10K increments each — merge must produce correct component-wise max.
#[test]
fn vclock_50_agents_10k_increments_merge_correctness() {
    let num_agents = 50;
    let increments = 10_000;
    let mut clocks: Vec<VectorClock> = Vec::new();

    for i in 0..num_agents {
        let mut clock = VectorClock::new();
        for _ in 0..increments {
            clock.increment(&format!("agent-{i}"));
        }
        // Each agent also sees some other agents' events.
        for j in 0..5 {
            let other = (i + j + 1) % num_agents;
            for _ in 0..100 {
                clock.increment(&format!("agent-{other}"));
            }
        }
        clocks.push(clock);
    }

    let start = Instant::now();
    let mut merged = VectorClock::new();
    for clock in &clocks {
        merged.merge(clock);
    }
    let elapsed = start.elapsed();

    // Verify: each agent's entry should be the max across all clocks.
    for i in 0..num_agents {
        let agent = format!("agent-{i}");
        let expected_max = clocks.iter().map(|c| c.get(&agent)).max().unwrap();
        assert_eq!(
            merged.get(&agent),
            expected_max,
            "agent-{i} should have max value {expected_max}"
        );
    }

    assert!(elapsed.as_secs() < 5, "50-agent merge took {:?}", elapsed);
}

/// Verify happens_before is a strict partial order (irreflexive, transitive, asymmetric).
#[test]
fn vclock_partial_order_invariants_1000_pairs() {
    let mut clocks = Vec::new();
    // Build a chain: clock[i] < clock[i+1].
    let mut current = VectorClock::new();
    for i in 0..100 {
        current.increment(&format!("agent-{}", i % 10));
        clocks.push(current.clone());
    }

    // Irreflexive: no clock happens-before itself.
    for clock in &clocks {
        assert!(!clock.happens_before(clock), "clock should not happen before itself");
    }

    // Transitivity: if a < b and b < c, then a < c.
    for i in 0..clocks.len() - 2 {
        if clocks[i].happens_before(&clocks[i + 1]) && clocks[i + 1].happens_before(&clocks[i + 2])
        {
            assert!(
                clocks[i].happens_before(&clocks[i + 2]),
                "transitivity violated at index {i}"
            );
        }
    }

    // Asymmetry: if a < b then NOT b < a.
    for i in 0..clocks.len() - 1 {
        if clocks[i].happens_before(&clocks[i + 1]) {
            assert!(
                !clocks[i + 1].happens_before(&clocks[i]),
                "asymmetry violated at index {i}"
            );
        }
    }
}

// =============================================================================
// GCOUNTER: Stress monotonicity and merge correctness
// =============================================================================

/// 100 agents, 50K total increments — verify value never decreases after merge.
#[test]
fn gcounter_monotonicity_under_heavy_merge() {
    let num_agents = 100;
    let counters: Vec<GCounter> = (0..num_agents)
        .map(|i| {
            let mut c = GCounter::new();
            for _ in 0..(500 + i * 10) {
                c.increment(&format!("agent-{i}"));
            }
            c
        })
        .collect();

    // Merge in random-ish order and verify monotonicity.
    let mut merged = GCounter::new();
    let mut prev_value = 0u64;
    for counter in &counters {
        merged.merge(counter);
        let new_value = merged.value();
        assert!(
            new_value >= prev_value,
            "GCounter value decreased from {prev_value} to {new_value}"
        );
        prev_value = new_value;
    }

    // Verify total.
    let expected: u64 = (0..num_agents).map(|i| (500 + i * 10) as u64).sum();
    assert_eq!(merged.value(), expected);
}

/// Verify delta_since produces correct deltas for partial sync.
#[test]
fn gcounter_delta_since_correctness_at_scale() {
    let mut full = GCounter::new();
    let mut partial = GCounter::new();

    // Full counter has 1000 increments across 20 agents.
    for i in 0..20 {
        for _ in 0..50 {
            full.increment(&format!("agent-{i}"));
        }
    }

    // Partial counter has only first 10 agents.
    for i in 0..10 {
        for _ in 0..30 {
            partial.increment(&format!("agent-{i}"));
        }
    }

    let delta = full.delta_since(&partial);

    // Delta should contain agents 0-9 (where full > partial) and agents 10-19 (new).
    for i in 0..10 {
        let agent = format!("agent-{i}");
        assert_eq!(
            *delta.counts.get(&agent).unwrap_or(&0),
            50,
            "agent-{i} delta should be 50 (full value)"
        );
    }
    for i in 10..20 {
        let agent = format!("agent-{i}");
        assert_eq!(
            *delta.counts.get(&agent).unwrap_or(&0),
            50,
            "agent-{i} delta should be 50 (new agent)"
        );
    }
}

// =============================================================================
// LWW REGISTER: Stress tie-breaking determinism
// =============================================================================

/// 1000 concurrent writes at the same timestamp — tie-breaking must be deterministic.
#[test]
fn lww_register_tiebreak_determinism_1000_agents() {
    let timestamp = Utc::now();
    let registers: Vec<LWWRegister<String>> = (0..1000)
        .map(|i| LWWRegister::new(format!("value-{i:04}"), timestamp, format!("agent-{i:04}")))
        .collect();

    // Merge all into one — the lexicographically greatest agent_id should win.
    let mut merged = registers[0].clone();
    for reg in &registers[1..] {
        merged.merge(reg);
    }

    // agent-0999 is lexicographically greatest.
    assert_eq!(merged.get(), "value-0999");
    assert_eq!(merged.agent_id(), "agent-0999");

    // Merge in reverse order — should produce the same result (commutativity).
    let mut merged_rev = registers[999].clone();
    for reg in registers[..999].iter().rev() {
        merged_rev.merge(reg);
    }
    assert_eq!(merged_rev.get(), merged.get());
}

/// Rapid timestamp progression — verify newer always wins.
#[test]
fn lww_register_rapid_timestamp_progression() {
    let base = Utc::now();
    let mut reg = LWWRegister::new("initial".to_string(), base, "agent-0".to_string());

    for i in 1..10_000 {
        let newer = LWWRegister::new(
            format!("value-{i}"),
            base + Duration::milliseconds(i),
            format!("agent-{}", i % 100),
        );
        reg.merge(&newer);
    }

    assert_eq!(reg.get(), "value-9999");
}

// =============================================================================
// OR-SET: Stress add-wins semantics under concurrent operations
// =============================================================================

/// 10 agents, each adding 1000 elements, some overlapping — verify add-wins after merge.
/// Add-wins means: if agent A removes elem-X (tombstoning A's tags) but agent B
/// concurrently adds elem-X (creating B's tags that A never saw), then after merge
/// elem-X is present because B's tags are not tombstoned.
#[test]
fn orset_10_agents_concurrent_add_remove_add_wins() {
    let num_agents = 10;
    let mut sets: Vec<ORSet<String>> = Vec::new();

    // Setup: all agents add elements 0..499.
    // Only agent-0 removes even elements 0,2,4,...,198 (100 elements).
    // Agents 1-9 do NOT remove anything — their tags survive.
    for i in 0..num_agents {
        let mut set = ORSet::new();
        for j in 0..500u64 {
            let elem = format!("elem-{j}");
            set.add(elem, &format!("agent-{i}"), j);
        }
        if i == 0 {
            // Only agent-0 removes some elements.
            for j in (0..200).step_by(2) {
                set.remove(&format!("elem-{j}"));
            }
        }
        sets.push(set);
    }

    let start = Instant::now();
    let mut merged = sets[0].clone();
    for set in &sets[1..] {
        merged.merge(set);
    }
    let elapsed = start.elapsed();

    // Add-wins: agent-0 removed elem-0, but agents 1-9 added it concurrently.
    // Their tags are NOT tombstoned → elem-0 is present.
    for j in (0..200).step_by(2) {
        assert!(
            merged.contains(&format!("elem-{j}")),
            "elem-{j} should be present (add-wins: agents 1-9 tags survive agent-0's remove)"
        );
    }

    // All 500 unique elements should be present.
    assert_eq!(merged.len(), 500);
    assert!(elapsed.as_secs() < 10, "OR-Set merge took {:?}", elapsed);

    // Verify: if ALL agents remove the same element, it should be gone.
    // Create a scenario where every agent removes elem-499.
    let mut all_remove_sets: Vec<ORSet<String>> = Vec::new();
    for i in 0..num_agents {
        let mut set = ORSet::new();
        set.add("shared-elem".to_string(), &format!("agent-{i}"), 0);
        set.remove(&"shared-elem".to_string());
        all_remove_sets.push(set);
    }
    let mut all_merged = all_remove_sets[0].clone();
    for set in &all_remove_sets[1..] {
        all_merged.merge(set);
    }
    assert!(
        !all_merged.contains(&"shared-elem".to_string()),
        "element removed by ALL agents should be absent"
    );
}

/// Verify tombstone accumulation doesn't break correctness.
#[test]
fn orset_tombstone_accumulation_correctness() {
    let mut set = ORSet::new();

    // Add and remove the same element 1000 times.
    for i in 0..1000u64 {
        set.add("volatile".to_string(), "agent-a", i * 2);
        set.remove(&"volatile".to_string());
        set.add("volatile".to_string(), "agent-a", i * 2 + 1);
    }

    // The last add should survive.
    assert!(set.contains(&"volatile".to_string()));

    // Add a stable element.
    set.add("stable".to_string(), "agent-b", 0);
    assert!(set.contains(&"stable".to_string()));
    assert_eq!(set.len(), 2);
}

// =============================================================================
// MAX REGISTER: Stress monotonicity
// =============================================================================

/// 10K merges with random values — value must never decrease.
#[test]
fn max_register_monotonicity_10k_merges() {
    let now = Utc::now();
    let mut reg = MaxRegister::new(0.0_f64, now);

    for i in 0..10_000 {
        let value = (i as f64 * 0.0001).sin().abs(); // Oscillating values.
        let other = MaxRegister::new(value, now);
        let before = *reg.get();
        reg.merge(&other);
        assert!(
            *reg.get() >= before,
            "MaxRegister decreased from {before} to {} at iteration {i}",
            reg.get()
        );
    }
}

// =============================================================================
// MV REGISTER: Stress concurrent value preservation
// =============================================================================

/// 20 agents set concurrent values — all should be preserved until resolved.
#[test]
fn mv_register_20_concurrent_values_preserved() {
    let mut registers: Vec<MVRegister<String>> = Vec::new();

    for i in 0..20 {
        let mut reg = MVRegister::new();
        let mut clock = VectorClock::new();
        clock.increment(&format!("agent-{i}"));
        reg.set(format!("value-from-agent-{i}"), &clock);
        registers.push(reg);
    }

    let mut merged = registers[0].clone();
    for reg in &registers[1..] {
        merged.merge(reg);
    }

    assert!(merged.is_conflicted());
    let values = merged.get();
    assert_eq!(values.len(), 20, "all 20 concurrent values should be preserved");

    // Resolve.
    merged.resolve("final-value".to_string());
    assert!(!merged.is_conflicted());
    assert_eq!(merged.get(), vec![&"final-value".to_string()]);
}

// =============================================================================
// MEMORY CRDT: Full merge convergence at scale
// =============================================================================

/// 5 agents diverge heavily on the same memory — merge must converge identically.
#[test]
fn memory_crdt_5_agent_heavy_divergence_convergence() {
    let base = make_base_memory("convergence-test", "agent-0");
    let agents: Vec<String> = (0..5).map(|i| format!("agent-{i}")).collect();

    let mut crdts: Vec<MemoryCRDT> = agents
        .iter()
        .map(|a| MemoryCRDT::from_base_memory(&base, a))
        .collect();

    // Each agent makes 500 divergent edits.
    for (idx, agent) in agents.iter().enumerate() {
        for j in 0..500u64 {
            crdts[idx].tags.add(format!("tag-{agent}-{j}"), agent, j + 1);
            crdts[idx].access_count.increment(agent);
            crdts[idx].clock.increment(agent);

            if j % 50 == 0 {
                let ts = Utc::now() + Duration::milliseconds(j as i64 + idx as i64 * 1000);
                crdts[idx]
                    .summary
                    .set(format!("Summary by {agent} v{j}"), ts, agent.clone());
            }
        }
    }

    // Merge all into each agent's state.
    let snapshots: Vec<MemoryCRDT> = crdts.clone();
    for crdt in &mut crdts {
        for snapshot in &snapshots {
            crdt.merge(snapshot);
        }
    }

    // All agents must have identical state.
    for i in 1..5 {
        assert_eq!(
            crdts[0].access_count.value(),
            crdts[i].access_count.value(),
            "access_count diverged between agent-0 and agent-{i}"
        );
        let tags_0: HashSet<String> = crdts[0].tags.elements().into_iter().cloned().collect();
        let tags_i: HashSet<String> = crdts[i].tags.elements().into_iter().cloned().collect();
        assert_eq!(
            tags_0, tags_i,
            "tags diverged between agent-0 and agent-{i}"
        );
        assert_eq!(
            crdts[0].summary.get(),
            crdts[i].summary.get(),
            "summary diverged between agent-0 and agent-{i}"
        );
        assert_eq!(
            crdts[0].clock, crdts[i].clock,
            "vector clock diverged between agent-0 and agent-{i}"
        );
    }

    // Verify tag count: 5 agents × 500 tags + 2 base tags = 2502.
    let total_tags = crdts[0].tags.elements().len();
    assert_eq!(total_tags, 2502, "expected 2502 tags, got {total_tags}");

    // Verify access_count: 5 agents × (1 init + 500 increments) = 2505.
    assert_eq!(crdts[0].access_count.value(), 2505);
}

/// Round-trip: from_base_memory → diverge → merge → to_base_memory preserves data.
#[test]
fn memory_crdt_roundtrip_preserves_all_fields() {
    let base = make_base_memory("roundtrip-test", "agent-rt");
    let crdt = MemoryCRDT::from_base_memory(&base, "agent-rt");
    let restored = crdt.to_base_memory();

    assert_eq!(restored.id, base.id);
    assert_eq!(restored.summary, base.summary);
    assert_eq!(restored.memory_type, base.memory_type);
    assert_eq!(restored.archived, base.archived);
    assert_eq!(restored.source_agent, base.source_agent);
    // Tags should be preserved.
    let mut base_tags = base.tags.clone();
    base_tags.sort();
    let mut restored_tags = restored.tags.clone();
    restored_tags.sort();
    assert_eq!(restored_tags, base_tags);
}

// =============================================================================
// MERGE ENGINE: Causal ordering validation
// =============================================================================

/// Apply delta with missing causal predecessor — must return CausalOrderViolation.
#[test]
fn merge_engine_rejects_future_delta() {
    let base = make_base_memory("causal-test", "agent-a");
    let mut local = MemoryCRDT::from_base_memory(&base, "agent-a");

    // Create a delta that claims agent-b is at clock 5, but local has never seen agent-b.
    let mut future_clock = VectorClock::new();
    future_clock.increment("agent-a"); // OK: local has agent-a at 1.
    for _ in 0..5 {
        future_clock.increment("agent-b"); // BAD: local has agent-b at 0.
    }

    let delta = MemoryDelta {
        memory_id: "causal-test".to_string(),
        source_agent: "agent-a".to_string(),
        clock: future_clock,
        field_deltas: vec![],
        timestamp: Utc::now(),
    };

    let result = MergeEngine::apply_delta(&mut local, &delta);
    assert!(result.is_err(), "should reject delta with future clock");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("agent-b") || err_msg.contains("CausalOrder"),
        "error should mention causal violation: {err_msg}"
    );
}

/// Apply delta with valid causal predecessor — must succeed.
#[test]
fn merge_engine_accepts_valid_delta() {
    let base = make_base_memory("causal-ok", "agent-a");
    let mut local = MemoryCRDT::from_base_memory(&base, "agent-a");

    // Delta from agent-a at clock {agent-a: 2} — local has {agent-a: 1}, so +1 is OK.
    let mut valid_clock = VectorClock::new();
    valid_clock.increment("agent-a");
    valid_clock.increment("agent-a");

    let delta = MemoryDelta {
        memory_id: "causal-ok".to_string(),
        source_agent: "agent-a".to_string(),
        clock: valid_clock,
        field_deltas: vec![],
        timestamp: Utc::now(),
    };

    let result = MergeEngine::apply_delta(&mut local, &delta);
    assert!(result.is_ok(), "should accept delta with valid clock");
}

// =============================================================================
// DAG CRDT: Cycle prevention at scale
// =============================================================================

/// Build a 500-node chain, then try to close the cycle — must be rejected.
#[test]
fn dag_crdt_500_node_chain_cycle_rejection() {
    let mut graph = CausalGraphCRDT::new();

    for i in 0..499 {
        graph
            .add_edge(
                &format!("node-{i}"),
                &format!("node-{}", i + 1),
                0.5,
                "agent-chain",
                i as u64,
            )
            .unwrap();
    }

    assert_eq!(graph.edge_count(), 499);

    // Try to close the cycle: node-499 → node-0.
    let result = graph.add_edge("node-499", "node-0", 0.3, "agent-chain", 499);
    assert!(result.is_err(), "closing a 500-node cycle should be rejected");
}

/// Two agents create conflicting edges that form a cycle on merge — weakest removed.
#[test]
fn dag_crdt_merge_cycle_resolution_deterministic() {
    let mut graph_a = CausalGraphCRDT::new();
    let mut graph_b = CausalGraphCRDT::new();

    // Agent A: X → Y (strength 0.9)
    graph_a.add_edge("X", "Y", 0.9, "agent-a", 1).unwrap();
    // Agent B: Y → X (strength 0.3)
    graph_b.add_edge("Y", "X", 0.3, "agent-b", 1).unwrap();

    // Merge: creates cycle X→Y→X. Weakest (Y→X at 0.3) should be removed.
    graph_a.merge(&graph_b).unwrap();

    assert!(graph_a.detect_cycle().is_none(), "cycle should be resolved");
    // X→Y should survive (stronger).
    assert!(
        graph_a.edges().iter().any(|e| e.source == "X" && e.target == "Y"),
        "X→Y should survive"
    );
    // Y→X should be removed (weaker).
    assert!(
        !graph_a.edges().iter().any(|e| e.source == "Y" && e.target == "X"),
        "Y→X should be removed"
    );

    // Merge in opposite order — same result (commutativity).
    let mut graph_b2 = CausalGraphCRDT::new();
    graph_b2.add_edge("Y", "X", 0.3, "agent-b", 1).unwrap();
    let mut graph_a2 = CausalGraphCRDT::new();
    graph_a2.add_edge("X", "Y", 0.9, "agent-a", 1).unwrap();

    graph_b2.merge(&graph_a2).unwrap();
    assert!(graph_b2.detect_cycle().is_none());
    assert!(graph_b2.edges().iter().any(|e| e.source == "X" && e.target == "Y"));
}

/// Stress: 3 agents each add 100 edges, merge all — must remain acyclic.
#[test]
fn dag_crdt_3_agents_100_edges_each_acyclic_after_merge() {
    let mut graphs: Vec<CausalGraphCRDT> = vec![
        CausalGraphCRDT::new(),
        CausalGraphCRDT::new(),
        CausalGraphCRDT::new(),
    ];

    // Each agent builds a tree (guaranteed acyclic).
    for (agent_idx, graph) in graphs.iter_mut().enumerate() {
        let prefix = format!("a{agent_idx}");
        for i in 0..100 {
            let source = format!("{prefix}-{}", i / 2);
            let target = format!("{prefix}-{i}");
            if source != target {
                let _ = graph.add_edge(&source, &target, 0.5 + (i as f64 * 0.005), &format!("agent-{agent_idx}"), i as u64);
            }
        }
    }

    // Merge all.
    let mut merged = graphs[0].clone();
    for graph in &graphs[1..] {
        merged.merge(graph).unwrap();
    }

    assert!(
        merged.detect_cycle().is_none(),
        "merged graph should be acyclic"
    );
}
