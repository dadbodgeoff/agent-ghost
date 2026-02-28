use cortex_core::memory::{Importance, MemoryType};
/// Serde roundtrip tests for all 16 shared models.
use cortex_core::models::*;

fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned>(val: &T) -> T {
    let json = serde_json::to_string(val).unwrap();
    serde_json::from_str(&json).unwrap()
}

#[test]
fn compressed_memory_roundtrip() {
    let m = CompressedMemory {
        memory_id: "id-1".into(),
        memory_type: MemoryType::Tribal,
        importance: Importance::High,
        level: 2,
        text: "compressed text".into(),
        token_count: 42,
        relevance_score: 0.95,
    };
    let r = roundtrip(&m);
    assert_eq!(r.memory_id, m.memory_id);
    assert_eq!(r.level, 2);
    assert_eq!(r.token_count, 42);
}

#[test]
fn retrieval_context_roundtrip() {
    let ctx = RetrievalContext {
        focus: "auth flow".into(),
        intent: Some(cortex_core::Intent::Investigate),
        active_files: vec!["src/auth.rs".into()],
        budget: 2000,
        sent_ids: vec![],
    };
    let r = roundtrip(&ctx);
    assert_eq!(r.focus, "auth flow");
    assert_eq!(r.budget, 2000);
}

#[test]
fn consolidation_result_roundtrip() {
    let res = ConsolidationResult {
        created: vec!["new-1".into()],
        archived: vec!["old-1".into()],
        metrics: ConsolidationMetrics {
            precision: 0.9,
            compression_ratio: 3.5,
            lift: 0.2,
            stability: 0.95,
        },
    };
    let r = roundtrip(&res);
    assert_eq!(r.created.len(), 1);
    assert_eq!(r.metrics.precision, 0.9);
}

#[test]
fn validation_result_roundtrip() {
    let res = ValidationResult {
        memory_id: "m-1".into(),
        dimension_scores: DimensionScores {
            citation: 0.7,
            temporal: 0.9,
            contradiction: 0.95,
            pattern_alignment: 0.8,
        },
        overall_score: 0.84,
        healing_actions: vec![HealingAction {
            action_type: HealingActionType::CitationUpdate,
            description: "refresh stale citation".into(),
            applied: false,
        }],
        passed: true,
    };
    let r = roundtrip(&res);
    assert_eq!(r.dimension_scores.temporal, 0.9);
    assert!(r.passed);
}

#[test]
fn learning_result_roundtrip() {
    let res = LearningResult {
        category: "factual".into(),
        principle: Some("Always validate input".into()),
        memory_created: Some("mem-new".into()),
    };
    let r = roundtrip(&res);
    assert_eq!(r.category, "factual");
    assert!(r.principle.is_some());
}

#[test]
fn health_report_roundtrip() {
    let report = HealthReport {
        overall_status: HealthStatus::Healthy,
        subsystems: vec![SubsystemHealth {
            name: "storage".into(),
            status: HealthStatus::Healthy,
            message: None,
        }],
        metrics: HealthMetrics {
            total_memories: 100,
            active_memories: 90,
            archived_memories: 10,
            average_confidence: 0.75,
            db_size_bytes: 1024 * 1024,
            embedding_cache_hit_rate: 0.85,
        },
    };
    let r = roundtrip(&report);
    assert_eq!(r.metrics.total_memories, 100);
}

#[test]
fn causal_narrative_roundtrip() {
    let n = CausalNarrative {
        sections: vec![NarrativeSection {
            title: "Root Cause".into(),
            content: "The auth module was misconfigured".into(),
            memory_ids: vec!["m-1".into()],
        }],
        summary: "Auth misconfiguration caused login failures".into(),
        confidence: 0.88,
    };
    let r = roundtrip(&n);
    assert_eq!(r.sections.len(), 1);
    assert_eq!(r.confidence, 0.88);
}

#[test]
fn why_context_roundtrip() {
    let ctx = WhyContext {
        patterns: vec![WhyEntry {
            memory_id: "p-1".into(),
            summary: "Repository pattern".into(),
            confidence: 0.9,
        }],
        decisions: vec![],
        tribal: vec![],
        warnings: vec!["Deprecated approach".into()],
    };
    let r = roundtrip(&ctx);
    assert_eq!(r.patterns.len(), 1);
    assert_eq!(r.warnings.len(), 1);
}

#[test]
fn generation_context_roundtrip() {
    let ctx = GenerationContext {
        allocations: vec![BudgetAllocation {
            category: "patterns".into(),
            percentage: 0.3,
            memories: vec![],
            tokens_used: 600,
        }],
        total_tokens: 600,
        total_budget: 2000,
    };
    let r = roundtrip(&ctx);
    assert_eq!(r.total_budget, 2000);
}

#[test]
fn prediction_result_roundtrip() {
    let res = PredictionResult {
        memory_ids: vec!["m-1".into()],
        signals: vec!["file_opened".into()],
        confidence: 0.7,
    };
    let r = roundtrip(&res);
    assert_eq!(r.memory_ids.len(), 1);
}

#[test]
fn session_context_roundtrip() {
    let mut sent = std::collections::HashSet::new();
    sent.insert("m-1".to_string());
    let ctx = SessionContext {
        session_id: "s-1".into(),
        sent_memory_ids: sent,
        tokens_used: 500,
        token_budget: 2000,
    };
    let r = roundtrip(&ctx);
    assert!(r.sent_memory_ids.contains("m-1"));
}

#[test]
fn audit_entry_roundtrip() {
    let entry = AuditEntry {
        memory_id: "m-1".into(),
        operation: AuditOperation::Create,
        details: serde_json::json!({"source": "user"}),
        actor: AuditActor::User,
        timestamp: chrono::Utc::now(),
    };
    let r = roundtrip(&entry);
    assert_eq!(r.operation, AuditOperation::Create);
    assert_eq!(r.actor, AuditActor::User);
}

#[test]
fn embedding_info_roundtrip() {
    let info = EmbeddingModelInfo {
        name: "jina-code-v2".into(),
        dimensions: 1024,
        status: EmbeddingModelStatus::Active,
    };
    let r = roundtrip(&info);
    assert_eq!(r.dimensions, 1024);
}

#[test]
fn contradiction_roundtrip() {
    let c = Contradiction {
        contradiction_type: ContradictionType::Direct,
        memory_ids: vec!["m-1".into(), "m-2".into()],
        confidence_delta: 0.3,
        description: "opposing statements about X".into(),
        detected_by: DetectionStrategy::AbsoluteStatement,
    };
    let r = roundtrip(&c);
    assert_eq!(r.memory_ids.len(), 2);
}

#[test]
fn consolidation_metrics_roundtrip() {
    let m = ConsolidationMetrics {
        precision: 0.92,
        compression_ratio: 4.0,
        lift: 0.15,
        stability: 0.98,
    };
    let r = roundtrip(&m);
    assert_eq!(r.precision, 0.92);
}

#[test]
fn degradation_event_roundtrip() {
    let e = DegradationEvent {
        component: "embeddings".into(),
        failure: "ONNX runtime crash".into(),
        fallback_used: "tfidf".into(),
        timestamp: chrono::Utc::now(),
    };
    let r = roundtrip(&e);
    assert_eq!(r.component, "embeddings");
    assert_eq!(r.fallback_used, "tfidf");
}
