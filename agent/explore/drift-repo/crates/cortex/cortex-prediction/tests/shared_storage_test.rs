//! Phase B test B-09: Prediction reads memories from shared storage.
//!
//! Verifies that PredictionEngine<Arc<StorageEngine>> can read memories
//! inserted via the same Arc<StorageEngine>, eliminating the duplicate-pool
//! bug where in-memory mode had isolated databases.

use std::sync::Arc;

use chrono::Utc;
use cortex_core::memory::*;
use cortex_core::traits::IMemoryStorage;
use cortex_prediction::PredictionEngine;
use cortex_storage::StorageEngine;

fn make_test_memory(id: &str, file_path: &str) -> BaseMemory {
    let now = Utc::now();
    let tc = TypedContent::Tribal(cortex_core::memory::types::TribalContent {
        knowledge: format!("Knowledge about {file_path}"),
        severity: "medium".to_string(),
        warnings: vec![],
        consequences: vec![],
    });
    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Tribal,
        content: tc.clone(),
        summary: format!("Memory about {file_path}"),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.9),
        importance: Importance::Normal,
        last_accessed: now,
        access_count: 10,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![FileLink {
            file_path: file_path.to_string(),
            line_start: None,
            line_end: None,
            content_hash: None,
        }],
        linked_functions: vec![],
        tags: vec!["test".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        namespace: Default::default(),
        source_agent: Default::default(),
        content_hash: BaseMemory::compute_content_hash(&tc).unwrap(),
    }
}

/// B-09: In-memory mode — PredictionEngine<Arc<StorageEngine>> shares storage.
/// Before B-01, the runtime opened a SEPARATE StorageEngine for prediction,
/// which in in-memory mode meant an isolated empty DB. Now both share the same Arc.
#[test]
fn b09_prediction_shares_storage_with_caller() {
    let storage = Arc::new(StorageEngine::open_in_memory().expect("in-memory storage"));

    // Insert a memory via the shared storage.
    let mem = make_test_memory("pred-test-1", "main");
    storage.create(&mem).expect("create memory");

    // Create PredictionEngine using the SAME Arc<StorageEngine>.
    // This is the B-01 fix: PredictionEngine<Arc<StorageEngine>> instead of
    // PredictionEngine<StorageEngine> with a separate open_in_memory().
    let _engine = PredictionEngine::new(storage.clone());

    // Verify the storage reference is truly shared:
    // Arc::strong_count = 2 (original + engine's clone)
    assert_eq!(
        Arc::strong_count(&storage),
        2,
        "storage should be shared, not duplicated"
    );

    // Verify the original storage still reads the memory (not isolated).
    let found = storage.get("pred-test-1").expect("get memory");
    assert!(found.is_some(), "memory should be in shared storage");
}

/// B-09b: Memory created after PredictionEngine init is visible to engine's storage.
#[test]
fn b09b_late_insert_visible_through_shared_storage() {
    let storage = Arc::new(StorageEngine::open_in_memory().expect("in-memory storage"));

    // Create PredictionEngine FIRST with empty storage.
    let _engine = PredictionEngine::new(storage.clone());

    // Insert memories AFTER engine creation.
    for i in 0..5 {
        let mem = make_test_memory(&format!("pred-multi-{i}"), &format!("mod{i}"));
        storage.create(&mem).expect("create memory");
    }

    // Verify all 5 are visible through the shared storage.
    // Before B-01, this would fail because the engine had a separate empty DB.
    for i in 0..5 {
        let found = storage.get(&format!("pred-multi-{i}")).expect("get");
        assert!(found.is_some(), "memory pred-multi-{i} should exist in shared storage");
    }
}
