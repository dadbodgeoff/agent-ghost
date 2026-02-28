use cortex_core::errors::*;

#[test]
fn cortex_error_memory_not_found_carries_id() {
    let err = CortexError::MemoryNotFound {
        id: "abc-123".into(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("abc-123"),
        "error should contain the memory id"
    );
}

#[test]
fn cortex_error_invalid_type_carries_name() {
    let err = CortexError::InvalidType {
        type_name: "bogus".into(),
    };
    assert!(err.to_string().contains("bogus"));
}

#[test]
fn cortex_error_token_budget_exceeded_carries_values() {
    let err = CortexError::TokenBudgetExceeded {
        needed: 5000,
        available: 2000,
    };
    let msg = err.to_string();
    assert!(msg.contains("5000"));
    assert!(msg.contains("2000"));
}

#[test]
fn cortex_error_causal_cycle_carries_path() {
    let err = CortexError::CausalCycle {
        path: "A -> B -> A".into(),
    };
    assert!(err.to_string().contains("A -> B -> A"));
}

#[test]
fn cortex_error_degraded_mode_carries_component_and_fallback() {
    let err = CortexError::DegradedMode {
        component: "embeddings".into(),
        fallback: "tfidf".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("embeddings"));
    assert!(msg.contains("tfidf"));
}

// --- From impls ---

#[test]
fn storage_error_converts_to_cortex_error() {
    let storage_err = StorageError::SqliteError {
        message: "disk full".into(),
    };
    let cortex_err: CortexError = storage_err.into();
    assert!(matches!(cortex_err, CortexError::StorageError(_)));
}

#[test]
fn embedding_error_converts_to_cortex_error() {
    let emb_err = EmbeddingError::DimensionMismatch {
        expected: 1024,
        actual: 384,
    };
    let cortex_err: CortexError = emb_err.into();
    assert!(matches!(cortex_err, CortexError::EmbeddingError(_)));
}

#[test]
fn consolidation_error_converts_to_cortex_error() {
    let cons_err = ConsolidationError::ClusteringFailed {
        reason: "too few points".into(),
    };
    let cortex_err: CortexError = cons_err.into();
    assert!(matches!(cortex_err, CortexError::ConsolidationError(_)));
}

#[test]
fn cloud_error_converts_to_cortex_error() {
    let cloud_err = CloudError::AuthFailed {
        reason: "expired token".into(),
    };
    let cortex_err: CortexError = cloud_err.into();
    assert!(matches!(cortex_err, CortexError::CloudSyncError(_)));
}

#[test]
fn serialization_error_converts_to_cortex_error() {
    let json_err = serde_json::from_str::<String>("not valid json").unwrap_err();
    let cortex_err: CortexError = json_err.into();
    assert!(matches!(cortex_err, CortexError::SerializationError(_)));
}

// --- Sub-error variants carry context ---

#[test]
fn storage_error_migration_failed_carries_version() {
    let err = StorageError::MigrationFailed {
        version: 5,
        reason: "syntax error".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("5"));
    assert!(msg.contains("syntax error"));
}

#[test]
fn embedding_error_model_load_failed_carries_path() {
    let err = EmbeddingError::ModelLoadFailed {
        path: "/models/jina.onnx".into(),
        reason: "file not found".into(),
    };
    assert!(err.to_string().contains("/models/jina.onnx"));
}

#[test]
fn causal_error_cycle_detected_carries_path() {
    let err = CausalError::CycleDetected {
        path: "X -> Y -> X".into(),
    };
    assert!(err.to_string().contains("X -> Y -> X"));
}

#[test]
fn consolidation_error_recall_gate_carries_scores() {
    let err = ConsolidationError::RecallGateFailed {
        score: 0.42,
        threshold: 0.85,
    };
    let msg = err.to_string();
    assert!(msg.contains("0.42"));
    assert!(msg.contains("0.85"));
}

#[test]
fn cloud_error_sync_conflict_carries_ids() {
    let err = CloudError::SyncConflict {
        memory_id: "mem-1".into(),
        local_version: 3,
        remote_version: 5,
    };
    let msg = err.to_string();
    assert!(msg.contains("mem-1"));
    assert!(msg.contains("3"));
    assert!(msg.contains("5"));
}

#[test]
fn retrieval_error_budget_exceeded_carries_values() {
    let err = RetrievalError::BudgetExceeded {
        needed: 3000,
        available: 1000,
    };
    let msg = err.to_string();
    assert!(msg.contains("3000"));
    assert!(msg.contains("1000"));
}
