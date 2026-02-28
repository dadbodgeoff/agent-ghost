//! Phase C decay wiring tests (C-17, C-18, C-20).
//!
//! Verify that the decay engine correctly reduces confidence,
//! triggers archival, and performs efficiently.

use chrono::{Duration, Utc};
use cortex_core::memory::*;
use cortex_decay::engine::DecayEngine;
use cortex_decay::factors::DecayContext;

fn make_memory(id: &str, confidence: f64, last_accessed_days_ago: i64) -> BaseMemory {
    let now = Utc::now();
    let tc = TypedContent::Tribal(cortex_core::memory::types::TribalContent {
        knowledge: "test knowledge".to_string(),
        severity: "medium".to_string(),
        warnings: vec![],
        consequences: vec![],
    });
    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Tribal,
        content: tc.clone(),
        summary: "test memory".to_string(),
        transaction_time: now - Duration::days(last_accessed_days_ago),
        valid_time: now - Duration::days(last_accessed_days_ago),
        valid_until: None,
        confidence: Confidence::new(confidence),
        importance: Importance::Normal,
        last_accessed: now - Duration::days(last_accessed_days_ago),
        access_count: 1,
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
        content_hash: BaseMemory::compute_content_hash(&tc).unwrap(),
    }
}

/// C-17: Decay actually reduces confidence for old memories.
#[test]
fn c17_decay_reduces_confidence() {
    let engine = DecayEngine::new();
    let ctx = DecayContext::default();

    // Memory not accessed in 90 days should have reduced confidence.
    let memory = make_memory("old-mem", 0.9, 90);
    let decayed = engine
        .calculate_with_context(&memory, &ctx)
        .expect("decay should succeed");

    assert!(
        decayed < 0.9,
        "confidence should decrease for 90-day-old memory: got {decayed}"
    );
}

/// C-18: Decay triggers archival for low-confidence old memories.
#[test]
fn c18_decay_triggers_archival() {
    let engine = DecayEngine::new();
    let ctx = DecayContext::default();

    // Memory with very low confidence, very old → should be archived.
    let memory = make_memory("archive-me", 0.16, 180);
    let decayed = engine
        .calculate_with_context(&memory, &ctx)
        .expect("decay should succeed");

    let decision = engine.evaluate_archival(&memory, decayed);
    assert!(
        decision.should_archive,
        "memory with confidence {decayed:.3} (from 0.16, 180 days old) should be archived"
    );
}

/// C-20: Decay batch processes 1000 memories under 1s.
#[test]
fn c20_decay_batch_performance() {
    let engine = DecayEngine::new();
    let ctx = DecayContext::default();

    let memories: Vec<BaseMemory> = (0..1000)
        .map(|i| make_memory(&format!("perf-{i}"), 0.5 + (i as f64 * 0.0004), i % 365))
        .collect();

    let start = std::time::Instant::now();
    let results = engine.process_batch(&memories, &ctx);
    let elapsed = start.elapsed();

    assert_eq!(results.len(), 1000, "should process all 1000 memories");
    assert!(
        elapsed.as_secs() < 1,
        "batch decay of 1000 memories should complete under 1s, took {:?}",
        elapsed
    );
}
