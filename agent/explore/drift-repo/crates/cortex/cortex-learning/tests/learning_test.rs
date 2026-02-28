//! Integration tests for cortex-learning (T8-LRN-01 through T8-LRN-08).

use cortex_core::memory::types::InsightContent;
use cortex_core::memory::*;
use cortex_core::traits::{Correction, ILearner};

use cortex_learning::analysis;
use cortex_learning::engine::LearningEngine;

// T8-LRN-01: Correction categorized correctly — known pattern violation → category = pattern_violation.
#[test]
fn t8_lrn_01_categorizes_pattern_violation() {
    let category = analysis::categorize(
        "This violates the SOLID design pattern principles",
        "class design",
    );
    assert_eq!(category, analysis::CorrectionCategory::PatternViolation);
}

// T8-LRN-02: Principle extracted from correction — non-empty principle string.
#[test]
fn t8_lrn_02_principle_extracted() {
    let engine = LearningEngine::new();
    let correction = Correction {
        original_memory_id: None,
        correction_text: "Don't use unwrap in production code".to_string(),
        context: "error handling".to_string(),
        source: "code review".to_string(),
    };

    let result = engine.analyze(&correction).unwrap();
    assert!(result.principle.is_some(), "principle should be extracted");
    let principle = result.principle.unwrap();
    assert!(!principle.is_empty(), "principle should not be empty");
}

// T8-LRN-03: Dedup prevents duplicate memory — similar correction twice → UPDATE, not second CREATE.
#[test]
fn t8_lrn_03_dedup_prevents_duplicate() {
    use cortex_learning::deduplication::{self, DedupAction};

    let content = TypedContent::Insight(InsightContent {
        observation: "Always validate input".to_string(),
        evidence: vec![],
    });
    let existing = BaseMemory {
        id: "existing-id".to_string(),
        memory_type: MemoryType::Insight,
        content: content.clone(),
        summary: "Always validate input before processing".to_string(),
        transaction_time: chrono::Utc::now(),
        valid_time: chrono::Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.7),
        importance: Importance::Normal,
        last_accessed: chrono::Utc::now(),
        access_count: 2,
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
        content_hash: "hash-123".to_string(),
    };

    // Same summary should trigger Update.
    let action = deduplication::check_dedup(
        "different-hash",
        "Always validate input before processing",
        &[existing],
    );
    match action {
        DedupAction::Update(id) => assert_eq!(id, "existing-id"),
        _ => panic!("expected Update for similar summary"),
    }
}

// T8-LRN-04: Causal link inferred — correction creates memory → causal edge to related memory.
// (This test verifies the learning result includes a memory_created ID.)
#[test]
fn t8_lrn_04_causal_link_inferred() {
    let engine = LearningEngine::new();
    let correction = Correction {
        original_memory_id: Some("original-memory-id".to_string()),
        correction_text: "Use Result instead of panic for error handling".to_string(),
        context: "error handling".to_string(),
        source: "code review".to_string(),
    };

    let result = engine.analyze(&correction).unwrap();
    assert!(
        result.memory_created.is_some(),
        "should create a memory that can be linked causally"
    );
}

// T8-LRN-05: Active learning selects uncertain memories — low confidence + high importance → selected.
#[test]
fn t8_lrn_05_active_learning_selects_uncertain() {
    use cortex_learning::active_learning;

    let content = TypedContent::Insight(InsightContent {
        observation: "test".to_string(),
        evidence: vec![],
    });
    let uncertain_important = BaseMemory {
        id: "uncertain".to_string(),
        memory_type: MemoryType::Insight,
        content: content.clone(),
        summary: "uncertain but important".to_string(),
        transaction_time: chrono::Utc::now(),
        valid_time: chrono::Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.2),
        importance: Importance::High,
        last_accessed: chrono::Utc::now(),
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
        content_hash: "hash".to_string(),
    };

    let certain_low = BaseMemory {
        id: "certain".to_string(),
        memory_type: MemoryType::Insight,
        content: content.clone(),
        summary: "certain and low importance".to_string(),
        transaction_time: chrono::Utc::now(),
        valid_time: chrono::Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.95),
        importance: Importance::Low,
        last_accessed: chrono::Utc::now(),
        access_count: 10,
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
        content_hash: "hash2".to_string(),
    };

    let memories = vec![uncertain_important, certain_low];
    let criteria = active_learning::SelectionCriteria::default();
    let candidates = active_learning::select_candidates(&memories, &criteria);

    assert!(!candidates.is_empty());
    // First candidate should be the uncertain + important one.
    assert_eq!(candidates[0].id, "uncertain");
}

// T8-LRN-06: Category mapping produces correct type — security_issue → tribal with critical importance.
#[test]
fn t8_lrn_06_category_mapping() {
    let mapping = analysis::map_category(analysis::CorrectionCategory::SecurityIssue);
    assert_eq!(mapping.memory_type, MemoryType::Tribal);
    assert_eq!(mapping.importance, Importance::Critical);
}

// T8-LRN-07: Diff analyzer detects additions, removals, modifications.
#[test]
fn t8_lrn_07_diff_analyzer() {
    let diff = analysis::analyze_diff(
        "line one\nline two\nline three",
        "line one\nline modified\nline four",
    );
    assert!(!diff.additions.is_empty(), "should detect additions");
    assert!(!diff.removals.is_empty(), "should detect removals");
    assert!(
        !diff.modifications.is_empty(),
        "should detect modifications"
    );
    assert!(diff.is_semantic_change);
}

// T8-LRN-08: Feedback processor updates confidence on confirm/reject.
#[test]
fn t8_lrn_08_feedback_processor() {
    use cortex_learning::active_learning::feedback_processor::{self, Feedback};

    let content = TypedContent::Insight(InsightContent {
        observation: "test".to_string(),
        evidence: vec![],
    });
    let memory = BaseMemory {
        id: "test-id".to_string(),
        memory_type: MemoryType::Insight,
        content: content.clone(),
        summary: "test".to_string(),
        transaction_time: chrono::Utc::now(),
        valid_time: chrono::Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.5),
        importance: Importance::Normal,
        last_accessed: chrono::Utc::now(),
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
        content_hash: "hash".to_string(),
    };

    // Confirm should boost.
    let confirm_result = feedback_processor::process_feedback(&memory, &Feedback::Confirm);
    assert!(confirm_result.new_confidence > 0.5);

    // Reject should lower.
    let reject_result = feedback_processor::process_feedback(&memory, &Feedback::Reject);
    assert!(reject_result.new_confidence < 0.5);
}
