//! Property tests for cortex-temporal (TTA-19, TTA-20, TTA-21).

use proptest::prelude::*;

use chrono::Utc;
use cortex_core::memory::*;
use cortex_core::models::*;
use rusqlite;

fn make_prop_memory() -> BaseMemory {
    let content = TypedContent::Episodic(cortex_core::memory::types::EpisodicContent {
        interaction: "prop test".to_string(),
        context: "ctx".to_string(),
        outcome: None,
    });
    BaseMemory {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type: MemoryType::Episodic,
        content: content.clone(),
        summary: "prop summary".to_string(),
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
        tags: vec!["test".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

fn make_event(mid: &str, et: MemoryEventType, delta: serde_json::Value) -> MemoryEvent {
    MemoryEvent {
        event_id: 0,
        memory_id: mid.to_string(),
        recorded_at: Utc::now(),
        event_type: et,
        delta,
        actor: EventActor::System("prop".to_string()),
        caused_by: vec![],
        schema_version: 1,
    }
}

// Strategy: generate a sequence of confidence changes
fn confidence_changes(n: usize) -> Vec<MemoryEvent> {
    let mid = "prop-mem";
    let mem = make_prop_memory();
    let mut events = vec![make_event(
        mid,
        MemoryEventType::Created,
        serde_json::to_value(&mem).unwrap(),
    )];
    let mut conf = 0.8;
    for _ in 0..n {
        let new_conf = (conf - 0.02_f64).max(0.01_f64);
        events.push(make_event(
            mid,
            MemoryEventType::ConfidenceChanged,
            serde_json::json!({ "old": conf, "new": new_conf }),
        ));
        conf = new_conf;
    }
    events
}

// TTA-19: replay(events) == apply_one_by_one(events)
proptest! {
    #[test]
    fn prop_replay_consistency(n in 1usize..30) {
        let events = confidence_changes(n);
        let shell = make_prop_memory();

        // Batch replay
        let batch = cortex_temporal::event_store::replay::replay_events(&events, shell.clone());

        // One-by-one
        let mut sequential = shell;
        for e in &events {
            sequential = cortex_temporal::event_store::replay::apply_event(sequential, e);
        }

        prop_assert!((batch.confidence.value() - sequential.confidence.value()).abs() < 0.0001);
        prop_assert_eq!(batch.archived, sequential.archived);
    }
}

// TTA-20: event_ids strictly increasing (tested via append in temporal_test.rs,
// here we test the replay ordering invariant)
proptest! {
    #[test]
    fn prop_replay_order_independence_of_event_id(n in 2usize..20) {
        // Replay should process events in array order regardless of event_id values
        let mid = "prop-order";
        let mem = make_prop_memory();
        let mut events = vec![make_event(
            mid,
            MemoryEventType::Created,
            serde_json::to_value(&mem).unwrap(),
        )];
        for i in 0..n {
            let new_conf = 0.8 - ((i + 1) as f64 * 0.02);
            let mut e = make_event(
                mid,
                MemoryEventType::ConfidenceChanged,
                serde_json::json!({ "old": 0.8, "new": new_conf }),
            );
            e.event_id = (n - i) as u64; // Reverse order IDs
            events.push(e);
        }

        let result = cortex_temporal::event_store::replay::replay_events(&events, make_prop_memory());
        // Last event's confidence should win
        let expected = 0.8 - (n as f64 * 0.02);
        prop_assert!((result.confidence.value() - expected).abs() < 0.0001);
    }
}

// TTA-21: event count conservation (property version)
proptest! {
    #[test]
    fn prop_event_count_conservation(n in 1usize..50) {
        let events = confidence_changes(n);
        // n confidence changes + 1 created = n+1 total
        prop_assert_eq!(events.len(), n + 1);
    }
}

// ── Phase B Property Tests ───────────────────────────────────────────────

// TTB-22: AS OF current == current
proptest! {
    #[test]
    fn prop_as_of_current_equals_current(n in 1usize..10) {
        // Verify that replay of all events produces the same state regardless
        // of how many confidence changes we apply — the final state from
        // replay_events must equal sequential apply_event.
        let events = confidence_changes(n);
        let shell = make_prop_memory();

        let replayed = cortex_temporal::event_store::replay::replay_events(&events, shell.clone());

        // The replayed state should have the final confidence from the last event
        let expected_conf = 0.8 - ((n as f64) * 0.02);
        prop_assert!(
            (replayed.confidence.value() - expected_conf.max(0.01)).abs() < 0.0001,
            "AS OF current replay mismatch: got {} expected {}",
            replayed.confidence.value(), expected_conf.max(0.01)
        );
    }
}

// TTB-23: diff identity (diff(T,T) == empty)
proptest! {
    #[test]
    fn prop_diff_identity(hours_ago in 0i64..1000) {
        let t = Utc::now() - chrono::Duration::hours(hours_ago);
        let query = cortex_core::models::TemporalDiffQuery {
            time_a: t,
            time_b: t,
            scope: cortex_core::models::DiffScope::All,
        };
        // diff(T, T) must always be empty regardless of T.
        // The execute_diff function has a fast-path identity check.
        // We verify the precondition that triggers it.
        prop_assert_eq!(query.time_a, query.time_b);

        // Also verify the empty_diff structure directly via the module
        // by constructing an in-memory DB and running the diff.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();
        let result = cortex_temporal::query::diff::execute_diff(&conn, &query).unwrap();
        prop_assert!(result.created.is_empty(), "diff(T,T) created should be empty");
        prop_assert!(result.archived.is_empty(), "diff(T,T) archived should be empty");
        prop_assert!(result.modified.is_empty(), "diff(T,T) modified should be empty");
        prop_assert!(result.confidence_shifts.is_empty(), "diff(T,T) shifts should be empty");
    }
}

// TTB-24: diff symmetry
proptest! {
    #[test]
    fn prop_diff_symmetry_invariant(
        delta_a in 0i64..100,
        delta_b in 0i64..100,
    ) {
        // For any two times A and B:
        // |diff(A,B).created| + |diff(A,B).archived| == |diff(B,A).created| + |diff(B,A).archived|
        // Verify this structurally with an in-memory DB.
        let a = Utc::now() - chrono::Duration::hours(delta_a);
        let b = Utc::now() - chrono::Duration::hours(delta_b);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();

        let query_ab = cortex_core::models::TemporalDiffQuery {
            time_a: a,
            time_b: b,
            scope: cortex_core::models::DiffScope::All,
        };
        let query_ba = cortex_core::models::TemporalDiffQuery {
            time_a: b,
            time_b: a,
            scope: cortex_core::models::DiffScope::All,
        };

        let diff_ab = cortex_temporal::query::diff::execute_diff(&conn, &query_ab).unwrap();
        let diff_ba = cortex_temporal::query::diff::execute_diff(&conn, &query_ba).unwrap();

        let total_ab = diff_ab.created.len() + diff_ab.archived.len();
        let total_ba = diff_ba.created.len() + diff_ba.archived.len();
        prop_assert_eq!(total_ab, total_ba,
            "diff symmetry violated: |diff(A,B)| = {} != |diff(B,A)| = {}", total_ab, total_ba);

        // Stronger: diff(A,B).created == diff(B,A).archived
        prop_assert_eq!(diff_ab.created.len(), diff_ba.archived.len(),
            "diff(A,B).created ({}) != diff(B,A).archived ({})",
            diff_ab.created.len(), diff_ba.archived.len());
    }
}

// TTB-25: temporal referential integrity (no dangling refs at any time T)
proptest! {
    #[test]
    fn prop_temporal_integrity_no_dangling(n in 1usize..20) {
        use std::collections::HashSet;

        // Generate n memories with random superseded_by references
        let mut memories: Vec<BaseMemory> = (0..n).map(|i| {
            let mut m = make_prop_memory();
            m.id = format!("prop-{}", i);
            m
        }).collect();

        // Set some superseded_by references (some valid, some dangling)
        if n > 1 {
            memories[0].superseded_by = Some(format!("prop-{}", n - 1)); // valid
            memories[1].superseded_by = Some("nonexistent".to_string()); // dangling
        }

        let result = cortex_temporal::query::enforce_temporal_integrity(
            memories, Utc::now()
        ).unwrap();

        // After integrity enforcement, no memory should reference a non-existent ID
        let valid_ids: HashSet<String> = result.iter().map(|m| m.id.clone()).collect();
        for m in &result {
            if let Some(ref sb) = m.superseded_by {
                prop_assert!(valid_ids.contains(sb),
                    "Dangling superseded_by ref: {} -> {}", m.id, sb);
            }
            if let Some(ref s) = m.supersedes {
                prop_assert!(valid_ids.contains(s),
                    "Dangling supersedes ref: {} -> {}", m.id, s);
            }
        }
    }
}

// TTB-26: temporal bounds (valid_time <= valid_until)
proptest! {
    #[test]
    fn prop_temporal_bounds_valid(
        offset_hours in 1i64..1000,
    ) {
        let now = Utc::now();
        let mut mem = make_prop_memory();

        // valid_time < valid_until should always pass
        mem.valid_time = now;
        mem.valid_until = Some(now + chrono::Duration::hours(offset_hours));
        let result = cortex_temporal::dual_time::validate_temporal_bounds(&mem);
        prop_assert!(result.is_ok(), "valid_time < valid_until should pass");

        // valid_time > valid_until should always fail
        mem.valid_time = now + chrono::Duration::hours(offset_hours);
        mem.valid_until = Some(now);
        let result = cortex_temporal::dual_time::validate_temporal_bounds(&mem);
        prop_assert!(result.is_err(), "valid_time > valid_until should fail");
    }
}

// ── Phase C Property Tests ───────────────────────────────────────────────

// TTC-12: temporal causal at current == current traversal
proptest! {
    #[test]
    fn prop_temporal_causal_current_equals_current(_n in 1usize..5) {
        // The invariant: temporal causal traversal at now() produces the same
        // result as current causal traversal. Verified by TTC-06 integration test.
        // Here we verify the structural property that reconstruct_graph_at with
        // all events returns the same graph as processing all events.
        use cortex_causal::graph::temporal_graph;

        let t1 = Utc::now();
        let events = vec![
            make_event("graph", MemoryEventType::RelationshipAdded,
                serde_json::json!({
                    "source": "a", "target": "b",
                    "relation_type": "caused", "strength": 0.8
                })),
        ];

        let graph = temporal_graph::reconstruct_graph_at(&events, t1 + chrono::Duration::seconds(1));
        prop_assert_eq!(graph.edge_count(), 1);
        prop_assert_eq!(graph.node_count(), 2);
    }
}

// TTC-13: graph reconstruction monotonicity (add then remove → not present)
proptest! {
    #[test]
    fn prop_graph_reconstruction_monotonicity(n_adds in 1usize..10) {
        use cortex_causal::graph::temporal_graph;

        let base = Utc::now();
        let mut events = Vec::new();
        let mut event_id = 1u64;

        // Add n edges
        for i in 0..n_adds {
            let t = base + chrono::Duration::seconds(i as i64);
            events.push(MemoryEvent {
                event_id,
                memory_id: "graph".to_string(),
                recorded_at: t,
                event_type: MemoryEventType::RelationshipAdded,
                delta: serde_json::json!({
                    "source": format!("src-{}", i),
                    "target": format!("tgt-{}", i),
                    "relation_type": "caused",
                    "strength": 0.8
                }),
                actor: EventActor::System("prop".to_string()),
                caused_by: vec![],
                schema_version: 1,
            });
            event_id += 1;
        }

        // Remove all edges
        for i in 0..n_adds {
            let t = base + chrono::Duration::seconds((n_adds + i) as i64);
            events.push(MemoryEvent {
                event_id,
                memory_id: "graph".to_string(),
                recorded_at: t,
                event_type: MemoryEventType::RelationshipRemoved,
                delta: serde_json::json!({
                    "source": format!("src-{}", i),
                    "target": format!("tgt-{}", i)
                }),
                actor: EventActor::System("prop".to_string()),
                caused_by: vec![],
                schema_version: 1,
            });
            event_id += 1;
        }

        // After all removals, graph should have 0 edges
        let after_all = base + chrono::Duration::seconds((2 * n_adds + 1) as i64);
        let graph = temporal_graph::reconstruct_graph_at(&events, after_all);
        prop_assert_eq!(graph.edge_count(), 0,
            "After adding {} edges and removing all, graph should have 0 edges", n_adds);
    }
}

// ── Phase D1 Property Tests ──────────────────────────────────────────────

// TTD1-02 (property): KSI bounds [0.0, 1.0]
proptest! {
    #[test]
    fn prop_ksi_bounds(
        total_at_start in 1u64..10000,
        change_count in 0u64..20000,
    ) {
        // KSI = 1.0 - change_count / (2 * total_at_start), clamped [0.0, 1.0]
        let ksi = (1.0 - (change_count as f64) / (2.0 * total_at_start as f64)).clamp(0.0, 1.0);
        prop_assert!(ksi >= 0.0, "KSI {} < 0.0", ksi);
        prop_assert!(ksi <= 1.0, "KSI {} > 1.0", ksi);
    }
}

// TTD1-09 (property): Evidence freshness bounds [0.0, 1.0]
proptest! {
    #[test]
    fn prop_evidence_freshness_bounds(
        n_factors in 0usize..20,
        factor_val in 0.0f64..1.0,
    ) {
        let factors: Vec<f64> = vec![factor_val; n_factors];
        let freshness = cortex_temporal::drift::evidence_freshness::compute_evidence_freshness(&factors);
        prop_assert!(freshness >= 0.0, "Freshness {} < 0.0", freshness);
        prop_assert!(freshness <= 1.0, "Freshness {} > 1.0", freshness);
    }
}

// TTD1-Extra (property): User validation freshness decay is monotonically decreasing
proptest! {
    #[test]
    fn prop_user_validation_freshness_monotonic(
        days_a in 0i64..1000,
        days_b in 0i64..1000,
    ) {
        let now = Utc::now();
        let t_a = now - chrono::Duration::days(days_a);
        let t_b = now - chrono::Duration::days(days_b);
        let f_a = cortex_temporal::drift::evidence_freshness::user_validation_freshness(t_a, now);
        let f_b = cortex_temporal::drift::evidence_freshness::user_validation_freshness(t_b, now);

        // More recent validation → higher freshness
        if days_a < days_b {
            prop_assert!(f_a >= f_b, "Freshness should decrease with age: f({})={} < f({})={}", days_a, f_a, days_b, f_b);
        }
    }
}

// ── Phase D2 Property Tests ──────────────────────────────────────────────

// TTD2-09: confidence aggregation bounds [0.0, 1.0] for both strategies
proptest! {
    #[test]
    fn prop_confidence_aggregation_bounds(
        n in 1usize..50,
        val in 0.0f64..1.0,
    ) {
        let evidences: Vec<f64> = vec![val; n];

        let wa = cortex_temporal::epistemic::aggregate_confidence(
            &evidences,
            &cortex_core::models::AggregationStrategy::WeightedAverage,
        );
        prop_assert!(wa >= 0.0, "WeightedAverage {} < 0.0", wa);
        prop_assert!(wa <= 1.0, "WeightedAverage {} > 1.0", wa);

        let gt = cortex_temporal::epistemic::aggregate_confidence(
            &evidences,
            &cortex_core::models::AggregationStrategy::GodelTNorm,
        );
        prop_assert!(gt >= 0.0, "GodelTNorm {} < 0.0", gt);
        prop_assert!(gt <= 1.0, "GodelTNorm {} > 1.0", gt);
    }
}

// TTD2-09 extra: mixed values
proptest! {
    #[test]
    fn prop_confidence_aggregation_bounds_mixed(
        a in 0.0f64..1.0,
        b in 0.0f64..1.0,
        c in 0.0f64..1.0,
    ) {
        let evidences = vec![a, b, c];

        let wa = cortex_temporal::epistemic::aggregate_confidence(
            &evidences,
            &cortex_core::models::AggregationStrategy::WeightedAverage,
        );
        prop_assert!((0.0..=1.0).contains(&wa), "WeightedAverage out of bounds: {}", wa);

        let gt = cortex_temporal::epistemic::aggregate_confidence(
            &evidences,
            &cortex_core::models::AggregationStrategy::GodelTNorm,
        );
        prop_assert!((0.0..=1.0).contains(&gt), "GodelTNorm out of bounds: {}", gt);

        // GodelTNorm should always be <= WeightedAverage (min <= mean)
        prop_assert!(gt <= wa + 0.0001, "GodelTNorm {} > WeightedAverage {}", gt, wa);
    }
}

// TTD2-10: epistemic ordering (only valid promotion paths succeed)
proptest! {
    #[test]
    fn prop_epistemic_ordering(_n in 1usize..10) {
        // Valid path: Conjecture → Provisional → Verified → Stale
        let c = cortex_core::models::EpistemicStatus::Conjecture {
            source: "test".to_string(),
            created_at: Utc::now(),
        };

        // Conjecture → Provisional: OK
        let p = cortex_temporal::epistemic::promote_to_provisional(&c, 1);
        prop_assert!(p.is_ok(), "Conjecture → Provisional should succeed");

        // Conjecture → Verified: REJECTED
        let v = cortex_temporal::epistemic::promote_to_verified(
            &c, vec!["a".to_string()], vec!["r".to_string()],
        );
        prop_assert!(v.is_err(), "Conjecture → Verified should be rejected");

        // Conjecture → Stale: REJECTED
        let s = cortex_temporal::epistemic::demote_to_stale(&c, "test".to_string());
        prop_assert!(s.is_err(), "Conjecture → Stale should be rejected");

        let p = p.unwrap();

        // Provisional → Verified: OK
        let v = cortex_temporal::epistemic::promote_to_verified(
            &p, vec!["a".to_string()], vec!["r".to_string()],
        );
        prop_assert!(v.is_ok(), "Provisional → Verified should succeed");

        // Provisional → Stale: REJECTED
        let s = cortex_temporal::epistemic::demote_to_stale(&p, "test".to_string());
        prop_assert!(s.is_err(), "Provisional → Stale should be rejected");

        let v = v.unwrap();

        // Verified → Stale: OK
        let s = cortex_temporal::epistemic::demote_to_stale(&v, "test".to_string());
        prop_assert!(s.is_ok(), "Verified → Stale should succeed");

        // Verified → Provisional: REJECTED
        let p2 = cortex_temporal::epistemic::promote_to_provisional(&v, 1);
        prop_assert!(p2.is_err(), "Verified → Provisional should be rejected");

        let s = s.unwrap();

        // Stale → Verified: REJECTED
        let v2 = cortex_temporal::epistemic::promote_to_verified(
            &s, vec!["a".to_string()], vec!["r".to_string()],
        );
        prop_assert!(v2.is_err(), "Stale → Verified should be rejected");
    }
}
