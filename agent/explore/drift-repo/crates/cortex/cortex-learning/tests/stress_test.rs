//! Learning stress tests: categorization at scale, dedup correctness,
//! principle extraction, and edge cases.

use chrono::Utc;
use cortex_core::memory::types::InsightContent;
use cortex_core::memory::*;
use cortex_core::traits::{Correction, ILearner};
use cortex_learning::analysis;
use cortex_learning::deduplication::{self, DedupAction};
use cortex_learning::engine::LearningEngine;
use std::time::Instant;

fn make_insight(id: &str, summary: &str, hash: &str) -> BaseMemory {
    let content = TypedContent::Insight(InsightContent {
        observation: summary.to_string(),
        evidence: vec![],
    });
    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Insight,
        content,
        summary: summary.to_string(),
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
        tags: vec![],
        archived: false,
        superseded_by: None,
        supersedes: None,
        namespace: Default::default(),
        source_agent: Default::default(),
        content_hash: hash.to_string(),
    }
}

// ── Categorization stress ────────────────────────────────────────────────

#[test]
fn stress_categorization_1000_corrections() {
    let test_cases = [
        (
            "This violates the SOLID design pattern",
            "class design",
            analysis::CorrectionCategory::PatternViolation,
        ),
        (
            "Use parameterized queries to prevent SQL injection",
            "security",
            analysis::CorrectionCategory::SecurityIssue,
        ),
        (
            "This loop is O(n^2), use a hash map instead",
            "performance",
            analysis::CorrectionCategory::PerformanceIssue,
        ),
        (
            "The function name should be snake_case",
            "naming",
            analysis::CorrectionCategory::NamingConvention,
        ),
        (
            "Add proper error handling for the network call",
            "error handling constraint",
            analysis::CorrectionCategory::ConstraintViolation,
        ),
    ];

    let start = Instant::now();
    for i in 0..1000 {
        let (text, context, expected) = &test_cases[i % test_cases.len()];
        let category = analysis::categorize(text, context);
        assert_eq!(
            category, *expected,
            "Iteration {i}: expected {:?}, got {:?} for '{text}'",
            expected, category
        );
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "1000 categorizations took {:?}",
        elapsed
    );
}

// ── Dedup stress ─────────────────────────────────────────────────────────

#[test]
fn stress_dedup_exact_hash_match_1000() {
    let existing: Vec<BaseMemory> = (0..100)
        .map(|i| {
            make_insight(
                &format!("existing-{i}"),
                &format!("Principle about topic {i}"),
                &format!("hash-{i}"),
            )
        })
        .collect();

    let start = Instant::now();
    for i in 0..1000 {
        let hash = format!("hash-{}", i % 100);
        let action = deduplication::check_dedup(&hash, "irrelevant", &existing);
        assert_eq!(action, DedupAction::Noop, "Exact hash match should be Noop");
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "1000 dedup checks took {:?}",
        elapsed
    );
}

#[test]
fn stress_dedup_fuzzy_match() {
    let existing = vec![
        make_insight(
            "e1",
            "always validate user input before processing",
            "hash-1",
        ),
        make_insight("e2", "use bcrypt for password hashing not md5", "hash-2"),
        make_insight(
            "e3",
            "prefer composition over inheritance in design",
            "hash-3",
        ),
    ];

    // Very similar summaries should trigger Update.
    let similar_cases = [
        ("always validate user input before processing", "e1"),
        ("use bcrypt for password hashing not md5", "e2"),
        ("prefer composition over inheritance in design", "e3"),
    ];

    for (summary, expected_id) in &similar_cases {
        let action = deduplication::check_dedup("new-hash", summary, &existing);
        match action {
            DedupAction::Update(id) => assert_eq!(&id, expected_id),
            _ => panic!(
                "Expected Update for similar summary '{summary}', got {:?}",
                action
            ),
        }
    }

    // Completely different summaries should trigger Add.
    let action = deduplication::check_dedup(
        "brand-new-hash",
        "something completely unrelated to anything",
        &existing,
    );
    assert_eq!(action, DedupAction::Add);
}

#[test]
fn stress_dedup_no_existing_memories() {
    let action = deduplication::check_dedup("any-hash", "any summary", &[]);
    assert_eq!(
        action,
        DedupAction::Add,
        "No existing memories should always Add"
    );
}

// ── Learning pipeline stress ─────────────────────────────────────────────

#[test]
fn stress_learning_pipeline_100_corrections() {
    let engine = LearningEngine::new();

    let start = Instant::now();
    for i in 0..100 {
        let correction = Correction {
            original_memory_id: if i % 2 == 0 {
                Some(format!("orig-{i}"))
            } else {
                None
            },
            correction_text: format!(
                "Correction number {i}: use pattern X instead of Y for task {i}"
            ),
            context: format!("code review round {}", i % 10),
            source: "stress_test".to_string(),
        };

        let result = engine.analyze(&correction).unwrap();

        // Every correction should produce a category.
        assert!(
            !result.category.is_empty(),
            "Correction {i} has empty category"
        );

        // Most corrections should produce a memory.
        // (Some might not if dedup kicks in, but with unique text they should.)
        if i < 50 {
            assert!(
                result.memory_created.is_some(),
                "Correction {i} should create a memory"
            );
        }
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "100 learning analyses took {:?}",
        elapsed
    );
}

#[test]
fn stress_principle_extraction_diverse_inputs() {
    let engine = LearningEngine::new();

    let corrections = [
        "Don't use unwrap in production code, use proper error handling",
        "Always add unit tests for public API functions",
        "Use dependency injection instead of global state",
        "Prefer iterators over manual loops for collection processing",
        "Add logging at error boundaries for debugging",
    ];

    for text in &corrections {
        let correction = Correction {
            original_memory_id: None,
            correction_text: text.to_string(),
            context: "code review".to_string(),
            source: "test".to_string(),
        };

        let result = engine.analyze(&correction).unwrap();
        assert!(
            result.principle.is_some(),
            "Should extract principle from '{}'",
            text
        );
        let principle = result.principle.unwrap();
        assert!(
            !principle.is_empty(),
            "Principle should not be empty for '{}'",
            text
        );
    }
}

// ── Edge cases ───────────────────────────────────────────────────────────

#[test]
fn stress_empty_correction_text() {
    let engine = LearningEngine::new();
    let correction = Correction {
        original_memory_id: None,
        correction_text: String::new(),
        context: String::new(),
        source: "test".to_string(),
    };

    // Should not panic, even with empty input.
    let result = engine.analyze(&correction);
    assert!(result.is_ok(), "Empty correction should not panic");
}

#[test]
fn stress_very_long_correction() {
    let engine = LearningEngine::new();
    let long_text = "Use proper error handling. ".repeat(500);
    let correction = Correction {
        original_memory_id: None,
        correction_text: long_text,
        context: "code review".to_string(),
        source: "test".to_string(),
    };

    let start = Instant::now();
    let result = engine.analyze(&correction).unwrap();
    let elapsed = start.elapsed();

    assert!(!result.category.is_empty());
    assert!(elapsed.as_secs() < 5, "Long correction took {:?}", elapsed);
}
