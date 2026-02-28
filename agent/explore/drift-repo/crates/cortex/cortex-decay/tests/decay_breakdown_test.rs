//! Decay breakdown verification: 5-factor math and 6-month forward simulation.
//! Verifies old low-importance memories fade to <0.1 confidence.

use chrono::{Duration, Utc};
use cortex_core::memory::*;
use cortex_decay::{DecayContext, DecayEngine};

fn make_test_memory(
    importance: Importance,
    memory_type: MemoryType,
    confidence: f64,
    access_count: u64,
    days_since_access: i64,
) -> BaseMemory {
    let now = Utc::now();
    let content = TypedContent::Tribal(cortex_core::memory::types::TribalContent {
        knowledge: "Test knowledge".to_string(),
        severity: "medium".to_string(),
        warnings: vec![],
        consequences: vec![],
    });
    BaseMemory {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type,
        content: content.clone(),
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
        content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        namespace: Default::default(),
        source_agent: Default::default(),
    }
}

// ── 5-factor math verification ───────────────────────────────────────────

#[test]
fn breakdown_product_equals_final_confidence() {
    let engine = DecayEngine::new();
    let memory = make_test_memory(Importance::Normal, MemoryType::Tribal, 0.9, 10, 30);
    let ctx = DecayContext {
        now: Utc::now(),
        stale_citation_ratio: 0.1,
        has_active_patterns: true,
        ..Default::default()
    };

    let bd = engine.calculate_breakdown(&memory, &ctx);

    // The final confidence should equal the product of all factors, clamped to [0,1].
    let manual_product = bd.base_confidence
        * bd.temporal
        * bd.citation
        * bd.usage
        * bd.importance
        * bd.pattern
        * bd.convergence;
    let expected = manual_product.clamp(0.0, 1.0);

    assert!(
        (bd.final_confidence - expected).abs() < 1e-10,
        "Breakdown product ({}) should equal final_confidence ({})",
        expected,
        bd.final_confidence
    );
}

#[test]
fn each_factor_in_expected_range() {
    let engine = DecayEngine::new();

    let test_cases = [
        (Importance::Low, MemoryType::Episodic, 0.5, 0, 30),
        (Importance::Normal, MemoryType::Tribal, 0.9, 50, 10),
        (Importance::High, MemoryType::Semantic, 0.7, 100, 60),
        (Importance::Critical, MemoryType::Decision, 1.0, 1000, 0),
    ];

    for (importance, mt, conf, access, days) in test_cases {
        let memory = make_test_memory(importance, mt, conf, access, days);
        let ctx = DecayContext {
            now: Utc::now(),
            stale_citation_ratio: 0.0,
            has_active_patterns: false,
            ..Default::default()
        };
        let bd = engine.calculate_breakdown(&memory, &ctx);

        assert!(
            bd.temporal >= 0.0 && bd.temporal <= 1.0,
            "Temporal should be [0,1], got {} for {:?}",
            bd.temporal,
            mt
        );
        assert!(
            bd.citation >= 0.5 && bd.citation <= 1.0,
            "Citation should be [0.5,1], got {} for {:?}",
            bd.citation,
            mt
        );
        assert!(
            bd.usage >= 1.0 && bd.usage <= 1.5,
            "Usage should be [1,1.5], got {} for {:?}",
            bd.usage,
            mt
        );
        assert!(
            bd.importance >= 0.8 && bd.importance <= 2.0,
            "Importance should be [0.8,2], got {} for {:?}",
            bd.importance,
            mt
        );
        assert!(
            bd.pattern >= 1.0 && bd.pattern <= 1.3,
            "Pattern should be [1,1.3], got {} for {:?}",
            bd.pattern,
            mt
        );
        assert!(
            bd.final_confidence >= 0.0 && bd.final_confidence <= 1.0,
            "Final should be [0,1], got {} for {:?}",
            bd.final_confidence,
            mt
        );
    }
}

// ── 6-month forward simulation ───────────────────────────────────────────

#[test]
fn low_importance_episodic_fades_below_01_in_6_months() {
    // Episodic half-life = 7 days. Low importance = 0.8x.
    // After 180 days with no access: temporal = e^(-180/7) ≈ 0.0
    // This should absolutely be < 0.1.
    let engine = DecayEngine::new();
    let now = Utc::now();

    let mut memory = make_test_memory(Importance::Low, MemoryType::Episodic, 0.8, 1, 0);
    memory.last_accessed = now; // Accessed right now.

    let ctx_6_months = DecayContext {
        now: now + Duration::days(180),
        stale_citation_ratio: 0.0,
        has_active_patterns: false,
        ..Default::default()
    };

    let decayed = engine
        .calculate_with_context(&memory, &ctx_6_months)
        .unwrap();
    assert!(
        decayed < 0.1,
        "Low-importance episodic memory after 6 months should be < 0.1, got {}",
        decayed
    );
}

#[test]
fn low_importance_conversation_fades_below_01_in_6_months() {
    // Conversation half-life = 30 days. Low importance = 0.8x.
    // After 180 days: temporal = e^(-180/30) = e^(-6) ≈ 0.0025
    // 0.8 * 0.0025 * 0.8 (low importance) ≈ 0.0016
    let engine = DecayEngine::new();
    let now = Utc::now();

    let content = TypedContent::Tribal(cortex_core::memory::types::TribalContent {
        knowledge: "Some conversation".to_string(),
        severity: "low".to_string(),
        warnings: vec![],
        consequences: vec![],
    });
    let memory = BaseMemory {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type: MemoryType::Conversation,
        content: content.clone(),
        summary: "Old conversation".to_string(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Low,
        last_accessed: now,
        access_count: 1,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec![],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        namespace: Default::default(),
        source_agent: Default::default(),
    };

    let ctx_6_months = DecayContext {
        now: now + Duration::days(180),
        stale_citation_ratio: 0.0,
        has_active_patterns: false,
        ..Default::default()
    };

    let decayed = engine
        .calculate_with_context(&memory, &ctx_6_months)
        .unwrap();
    assert!(
        decayed < 0.1,
        "Low-importance conversation after 6 months should be < 0.1, got {}",
        decayed
    );
}

#[test]
fn normal_importance_semantic_fades_below_01_in_6_months() {
    // Semantic half-life = 90 days. Normal importance = 1.0x.
    // After 180 days: temporal = e^(-180/90) = e^(-2) ≈ 0.135
    // 0.7 * 0.135 * 1.0 ≈ 0.095 — right around the boundary.
    let engine = DecayEngine::new();
    let now = Utc::now();

    let mut memory = make_test_memory(Importance::Normal, MemoryType::Semantic, 0.7, 0, 0);
    memory.last_accessed = now;

    let ctx_6_months = DecayContext {
        now: now + Duration::days(180),
        stale_citation_ratio: 0.0,
        has_active_patterns: false,
        ..Default::default()
    };

    let decayed = engine
        .calculate_with_context(&memory, &ctx_6_months)
        .unwrap();
    assert!(
        decayed < 0.1,
        "Normal-importance semantic memory (conf=0.7) after 6 months should be < 0.1, got {}",
        decayed
    );
}

#[test]
fn critical_core_memory_does_not_fade() {
    // Core has infinite half-life. Should not decay at all.
    let engine = DecayEngine::new();
    let now = Utc::now();

    let mut memory = make_test_memory(Importance::Critical, MemoryType::Core, 0.9, 10, 0);
    memory.last_accessed = now;

    let ctx_6_months = DecayContext {
        now: now + Duration::days(180),
        stale_citation_ratio: 0.0,
        has_active_patterns: false,
        ..Default::default()
    };

    let decayed = engine
        .calculate_with_context(&memory, &ctx_6_months)
        .unwrap();
    // Core memory: temporal=1.0, importance=2.0, so result = 0.9 * 1.0 * 1.0 * usage * 2.0 * 1.0
    // Clamped to 1.0. Should definitely be > 0.9.
    assert!(
        decayed >= 0.9,
        "Critical core memory should not fade after 6 months, got {}",
        decayed
    );
}

#[test]
fn forward_simulation_monotonic_decay_curve() {
    // Simulate day-by-day for 180 days. Confidence should never increase.
    let engine = DecayEngine::new();
    let now = Utc::now();

    let mut memory = make_test_memory(Importance::Low, MemoryType::Episodic, 1.0, 0, 0);
    memory.last_accessed = now;

    let mut prev = 1.0_f64;
    for day in 0..=180 {
        let ctx = DecayContext {
            now: now + Duration::days(day),
            stale_citation_ratio: 0.0,
            has_active_patterns: false,
            ..Default::default()
        };
        let decayed = engine.calculate_with_context(&memory, &ctx).unwrap();
        assert!(
            decayed <= prev + f64::EPSILON,
            "Day {}: confidence increased from {} to {}",
            day,
            prev,
            decayed
        );
        prev = decayed;
    }

    // After 180 days, a low-importance episodic memory (half-life 7d) should be near zero.
    assert!(
        prev < 0.01,
        "After 180 days, low-importance episodic should be near zero, got {}",
        prev
    );
}
