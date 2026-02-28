use chrono::{Duration, Utc};
use cortex_core::memory::*;
use cortex_core::traits::IDecayEngine;
use cortex_decay::{DecayContext, DecayEngine};
use proptest::prelude::*;

fn make_memory(
    importance: Importance,
    confidence: f64,
    access_count: u64,
    days_ago: i64,
) -> BaseMemory {
    let now = Utc::now();
    BaseMemory {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type: MemoryType::Tribal,
        content: TypedContent::Tribal(cortex_core::memory::types::TribalContent {
            knowledge: "Test".to_string(),
            severity: "low".to_string(),
            warnings: vec![],
            consequences: vec![],
        }),
        summary: "Test".to_string(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(confidence),
        importance,
        last_accessed: now - Duration::days(days_ago),
        access_count,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec![],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: "test".to_string(),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

fn arb_importance() -> impl Strategy<Value = Importance> {
    prop_oneof![
        Just(Importance::Low),
        Just(Importance::Normal),
        Just(Importance::High),
        Just(Importance::Critical),
    ]
}

// ── T4-DEC-07: Monotonically decreasing ──────────────────────────────────

proptest! {
    #[test]
    fn monotonically_decreasing(
        confidence in 0.1f64..1.0,
        access_count in 0u64..1000,
    ) {
        let engine = DecayEngine::new();
        let now = Utc::now();
        let memory = make_memory(Importance::Normal, confidence, access_count, 0);

        let mut prev = engine.calculate_with_context(&memory, &DecayContext {
            now,
            stale_citation_ratio: 0.0,
            has_active_patterns: false,
            ..Default::default()
        }).unwrap();

        for days in [1, 7, 30, 90, 180] {
            let mut m = memory.clone();
            m.last_accessed = now; // Fix last_accessed to now
            let result = engine.calculate_with_context(&m, &DecayContext {
                now: now + Duration::days(days),
                stale_citation_ratio: 0.0,
                has_active_patterns: false,
                ..Default::default()
            }).unwrap();
            prop_assert!(
                result <= prev + f64::EPSILON,
                "Not monotonic at day {}: {} > {}",
                days, result, prev
            );
            prev = result;
        }
    }
}

// ── T4-DEC-08: Bounded 0.0–1.0 ──────────────────────────────────────────

proptest! {
    #[test]
    fn bounded_zero_to_one(
        confidence in 0.0f64..=1.0,
        access_count in 0u64..100_000,
        days_ago in 0i64..1000,
        importance in arb_importance(),
        stale_ratio in 0.0f64..=1.0,
    ) {
        let engine = DecayEngine::new();
        let memory = make_memory(importance, confidence, access_count, days_ago);
        let ctx = DecayContext {
            now: Utc::now(),
            stale_citation_ratio: stale_ratio,
            has_active_patterns: true,
            ..Default::default()
        };
        let result = engine.calculate_with_context(&memory, &ctx).unwrap();
        prop_assert!(
            (0.0..=1.0).contains(&result),
            "Out of bounds: {}",
            result
        );
    }
}

// ── T4-DEC-09: Importance anchor capped ──────────────────────────────────

proptest! {
    #[test]
    fn importance_anchor_capped(
        confidence in 0.0f64..=1.0,
        access_count in 0u64..10000,
    ) {
        let engine = DecayEngine::new();
        let memory = make_memory(Importance::Critical, confidence, access_count, 0);
        let result = engine.calculate(&memory).unwrap();
        // Even with critical importance (2.0×), result must be ≤ 1.0
        prop_assert!(result <= 1.0, "Critical memory exceeded 1.0: {}", result);
    }
}

// ── T4-DEC-10: Usage boost capped ────────────────────────────────────────

proptest! {
    #[test]
    fn usage_boost_capped(access_count in 0u64..1_000_000) {
        let memory = make_memory(Importance::Normal, 1.0, access_count, 0);
        let boost = cortex_decay::factors::usage::calculate(&memory);
        prop_assert!(
            boost <= 1.5,
            "Usage boost exceeded 1.5: {} for access_count={}",
            boost,
            access_count
        );
        prop_assert!(
            boost >= 1.0,
            "Usage boost below 1.0: {}",
            boost
        );
    }
}
