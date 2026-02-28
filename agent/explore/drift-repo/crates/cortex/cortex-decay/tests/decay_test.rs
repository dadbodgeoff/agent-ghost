use chrono::{Duration, Utc};
use cortex_core::memory::*;
use cortex_core::traits::IDecayEngine;
use cortex_decay::{DecayContext, DecayEngine};

fn make_test_memory(
    importance: Importance,
    memory_type: MemoryType,
    confidence: f64,
    access_count: u64,
    days_since_access: i64,
) -> BaseMemory {
    let now = Utc::now();
    BaseMemory {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type,
        content: TypedContent::Tribal(cortex_core::memory::types::TribalContent {
            knowledge: "Test knowledge".to_string(),
            severity: "medium".to_string(),
            warnings: vec![],
            consequences: vec![],
        }),
        summary: "Test memory".to_string(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(confidence),
        importance,
        last_accessed: now - Duration::days(days_since_access),
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

// ── T4-DEC-01: Monotonically decreasing over time ────────────────────────

#[test]
fn monotonically_decreasing_over_time_without_access() {
    let engine = DecayEngine::new();
    let now = Utc::now();

    let mut prev_confidence = 1.0;
    for days in [0, 1, 7, 30, 90, 180, 365] {
        let memory = make_test_memory(Importance::Normal, MemoryType::Tribal, 1.0, 0, 0);
        let ctx = DecayContext {
            now: now + Duration::days(days),
            stale_citation_ratio: 0.0,
            has_active_patterns: false,
            ..Default::default()
        };
        // Manually set last_accessed to `now` so days_since_access = `days`
        let mut m = memory;
        m.last_accessed = now;

        let decayed = engine.calculate_with_context(&m, &ctx).unwrap();
        assert!(
            decayed <= prev_confidence + f64::EPSILON,
            "Not monotonically decreasing at day {}: {} > {}",
            days,
            decayed,
            prev_confidence
        );
        prev_confidence = decayed;
    }
}

// ── T4-DEC-02: Bounded 0.0 ≤ confidence ≤ 1.0 ──────────────────────────

#[test]
fn confidence_bounded_zero_to_one() {
    let engine = DecayEngine::new();

    // Test with extreme values
    let test_cases = [
        (Importance::Critical, MemoryType::Core, 1.0, 10000, 0),
        (Importance::Low, MemoryType::Episodic, 0.01, 0, 365),
        (Importance::Critical, MemoryType::Tribal, 1.0, 10000, 0),
        (Importance::Low, MemoryType::Conversation, 0.1, 0, 1000),
    ];

    for (importance, mt, conf, access, days) in test_cases {
        let memory = make_test_memory(importance, mt, conf, access, days);
        let result = engine.calculate(&memory).unwrap();
        assert!(
            (0.0..=1.0).contains(&result),
            "Confidence out of bounds: {} for {:?}/{:?}",
            result,
            importance,
            mt
        );
    }
}

// ── T4-DEC-03: Importance anchor capped ──────────────────────────────────

#[test]
fn importance_anchor_capped_at_2x() {
    let engine = DecayEngine::new();
    let memory = make_test_memory(Importance::Critical, MemoryType::Tribal, 0.5, 0, 0);
    let ctx = DecayContext {
        now: Utc::now(),
        stale_citation_ratio: 0.0,
        has_active_patterns: true,
        ..Default::default()
    };
    let result = engine.calculate_with_context(&memory, &ctx).unwrap();
    // base=0.5, importance=2.0, but result clamped to 1.0
    assert!(
        result <= 1.0,
        "Result should be clamped to 1.0, got {}",
        result
    );
}

// ── T4-DEC-04: Usage boost capped at 1.5× ───────────────────────────────

#[test]
fn usage_boost_capped_at_1_5x() {
    let boost = cortex_decay::factors::usage::calculate(&make_test_memory(
        Importance::Normal,
        MemoryType::Tribal,
        1.0,
        1_000_000,
        0,
    ));
    assert!(
        boost <= 1.5,
        "Usage boost should be capped at 1.5, got {}",
        boost
    );
}

// ── T4-DEC-05: Adaptive half-life computes correctly ─────────────────────

#[test]
fn adaptive_half_life_increases_with_access() {
    let low_access = make_test_memory(Importance::Normal, MemoryType::Tribal, 0.9, 1, 0);
    let high_access = make_test_memory(Importance::Normal, MemoryType::Tribal, 0.9, 1000, 0);

    let low_hl = cortex_decay::adaptive::adaptive_half_life(&low_access).unwrap();
    let high_hl = cortex_decay::adaptive::adaptive_half_life(&high_access).unwrap();

    assert!(
        high_hl > low_hl,
        "Frequently accessed memory should have longer half-life: {} vs {}",
        high_hl,
        low_hl
    );
    // Tribal base = 365 days, with high access should be > 365
    assert!(
        high_hl > 365.0,
        "Frequently accessed tribal memory should have half-life > 365d, got {}",
        high_hl
    );
}

#[test]
fn core_memory_has_infinite_half_life() {
    let memory = make_test_memory(Importance::Critical, MemoryType::Core, 1.0, 0, 0);
    let hl = cortex_decay::adaptive::adaptive_half_life(&memory);
    assert!(hl.is_none(), "Core memory should have infinite half-life");
}

// ── T4-DEC-06: Archival triggers at threshold ────────────────────────────

#[test]
fn archival_triggers_below_threshold() {
    let engine = DecayEngine::new();
    let memory = make_test_memory(Importance::Low, MemoryType::Episodic, 0.1, 0, 365);

    let decayed = engine.calculate(&memory).unwrap();
    let decision = engine.evaluate_archival(&memory, decayed);

    // With very low confidence and long time since access, should be archived
    assert!(
        decision.should_archive,
        "Memory with confidence {} should be archived (threshold {})",
        decayed,
        engine.archival_threshold()
    );
}

#[test]
fn archival_does_not_trigger_above_threshold() {
    let engine = DecayEngine::new();
    let memory = make_test_memory(Importance::Critical, MemoryType::Core, 1.0, 100, 0);

    let decayed = engine.calculate(&memory).unwrap();
    let decision = engine.evaluate_archival(&memory, decayed);

    assert!(
        !decision.should_archive,
        "High-confidence core memory should not be archived"
    );
}

#[test]
fn already_archived_memory_not_re_archived() {
    let engine = DecayEngine::new();
    let mut memory = make_test_memory(Importance::Low, MemoryType::Episodic, 0.01, 0, 365);
    memory.archived = true;

    let decision = engine.evaluate_archival(&memory, 0.01);
    assert!(
        !decision.should_archive,
        "Already archived memory should not be re-archived"
    );
}

// ── Additional tests ──────────────────────────────────────────────────────

#[test]
fn decay_breakdown_factors_are_reasonable() {
    let engine = DecayEngine::new();
    let memory = make_test_memory(Importance::High, MemoryType::Tribal, 0.8, 50, 30);
    let ctx = DecayContext {
        now: Utc::now(),
        stale_citation_ratio: 0.2,
        has_active_patterns: true,
        ..Default::default()
    };

    let breakdown = engine.calculate_breakdown(&memory, &ctx);

    assert!(
        (0.0..=1.0).contains(&breakdown.temporal),
        "Temporal out of range"
    );
    assert!(
        (0.5..=1.0).contains(&breakdown.citation),
        "Citation out of range"
    );
    assert!((1.0..=1.5).contains(&breakdown.usage), "Usage out of range");
    assert!(
        (0.8..=2.0).contains(&breakdown.importance),
        "Importance out of range"
    );
    assert!(
        (1.0..=1.3).contains(&breakdown.pattern),
        "Pattern out of range"
    );
    assert!(
        (0.0..=1.0).contains(&breakdown.final_confidence),
        "Final out of range"
    );
}

#[test]
fn batch_processing_works() {
    let engine = DecayEngine::new();
    let memories: Vec<BaseMemory> = (0..100)
        .map(|i| {
            make_test_memory(
                Importance::Normal,
                MemoryType::Semantic,
                0.8,
                i,
                (i % 30) as i64,
            )
        })
        .collect();

    let ctx = DecayContext::default();
    let results = engine.process_batch(&memories, &ctx);

    assert_eq!(results.len(), 100);
    for (confidence, decision) in &results {
        assert!((0.0..=1.0).contains(confidence));
        assert!(!decision.memory_id.is_empty());
    }
}

#[test]
fn stale_citations_reduce_confidence() {
    let engine = DecayEngine::new();
    let mut memory = make_test_memory(Importance::Normal, MemoryType::Tribal, 0.9, 10, 0);
    memory.linked_files = vec![cortex_core::memory::FileLink {
        file_path: "src/main.rs".to_string(),
        line_start: Some(1),
        line_end: Some(10),
        content_hash: Some("old_hash".to_string()),
    }];

    let fresh_ctx = DecayContext {
        now: Utc::now(),
        stale_citation_ratio: 0.0,
        has_active_patterns: false,
        ..Default::default()
    };
    let stale_ctx = DecayContext {
        now: Utc::now(),
        stale_citation_ratio: 1.0,
        has_active_patterns: false,
        ..Default::default()
    };

    let fresh_result = engine.calculate_with_context(&memory, &fresh_ctx).unwrap();
    let stale_result = engine.calculate_with_context(&memory, &stale_ctx).unwrap();

    assert!(
        stale_result < fresh_result,
        "Stale citations should reduce confidence: fresh={}, stale={}",
        fresh_result,
        stale_result
    );
}
