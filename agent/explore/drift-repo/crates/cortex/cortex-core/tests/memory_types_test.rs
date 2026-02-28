use chrono::Utc;
use cortex_core::memory::types::*;
use cortex_core::memory::*;

#[test]
fn memory_type_has_31_variants() {
    assert_eq!(MemoryType::COUNT, 31);
    assert_eq!(MemoryType::ALL.len(), 31);
}

#[test]
fn all_31_types_have_half_lives() {
    for mt in MemoryType::ALL {
        // half_life_days returns Option<u64>; None means infinite (Core, ConvergenceEvent, BoundaryViolation)
        let hl = half_life_days(mt);
        if mt.is_immortal() {
            assert!(hl.is_none(), "{:?} should have infinite half-life", mt);
        } else {
            assert!(hl.is_some(), "{:?} should have a finite half-life", mt);
        }
    }
}

#[test]
fn half_lives_match_spec() {
    assert_eq!(half_life_days(MemoryType::Core), None);
    assert_eq!(half_life_days(MemoryType::Tribal), Some(365));
    assert_eq!(half_life_days(MemoryType::Procedural), Some(180));
    assert_eq!(half_life_days(MemoryType::Semantic), Some(90));
    assert_eq!(half_life_days(MemoryType::Episodic), Some(7));
    assert_eq!(half_life_days(MemoryType::Decision), Some(180));
    assert_eq!(half_life_days(MemoryType::Insight), Some(90));
    assert_eq!(half_life_days(MemoryType::Reference), Some(60));
    assert_eq!(half_life_days(MemoryType::Preference), Some(120));
    assert_eq!(half_life_days(MemoryType::PatternRationale), Some(180));
    assert_eq!(half_life_days(MemoryType::ConstraintOverride), Some(90));
    assert_eq!(half_life_days(MemoryType::DecisionContext), Some(180));
    assert_eq!(half_life_days(MemoryType::CodeSmell), Some(90));
    assert_eq!(half_life_days(MemoryType::AgentSpawn), Some(365));
    assert_eq!(half_life_days(MemoryType::Entity), Some(180));
    assert_eq!(half_life_days(MemoryType::Goal), Some(90));
    assert_eq!(half_life_days(MemoryType::Feedback), Some(120));
    assert_eq!(half_life_days(MemoryType::Workflow), Some(180));
    assert_eq!(half_life_days(MemoryType::Conversation), Some(30));
    assert_eq!(half_life_days(MemoryType::Incident), Some(365));
    assert_eq!(half_life_days(MemoryType::Meeting), Some(60));
    assert_eq!(half_life_days(MemoryType::Skill), Some(180));
    assert_eq!(half_life_days(MemoryType::Environment), Some(90));
    // Convergence types
    assert_eq!(half_life_days(MemoryType::AgentGoal), Some(90));
    assert_eq!(half_life_days(MemoryType::AgentReflection), Some(30));
    assert_eq!(half_life_days(MemoryType::ConvergenceEvent), None);
    assert_eq!(half_life_days(MemoryType::BoundaryViolation), None);
    assert_eq!(half_life_days(MemoryType::ProposalRecord), Some(365));
    assert_eq!(half_life_days(MemoryType::SimulationResult), Some(60));
    assert_eq!(half_life_days(MemoryType::InterventionPlan), Some(180));
    assert_eq!(half_life_days(MemoryType::AttachmentIndicator), Some(120));
}

#[test]
fn relationship_type_has_14_variants() {
    assert_eq!(RelationshipType::COUNT, 14);
    assert_eq!(RelationshipType::ALL.len(), 14);
}

#[test]
fn importance_ordering_critical_gt_high_gt_normal_gt_low() {
    assert!(Importance::Critical > Importance::High);
    assert!(Importance::High > Importance::Normal);
    assert!(Importance::Normal > Importance::Low);
}

#[test]
fn importance_weights_match_spec() {
    assert_eq!(Importance::Low.weight(), 0.8);
    assert_eq!(Importance::Normal.weight(), 1.0);
    assert_eq!(Importance::High.weight(), 1.5);
    assert_eq!(Importance::Critical.weight(), 2.0);
}

#[test]
fn confidence_clamping_works() {
    let c = Confidence::new(1.5);
    assert_eq!(c.value(), 1.0);

    let c = Confidence::new(-0.5);
    assert_eq!(c.value(), 0.0);

    let c = Confidence::new(0.75);
    assert_eq!(c.value(), 0.75);
}

#[test]
fn confidence_arithmetic() {
    let a = Confidence::new(0.6);
    let b = Confidence::new(0.5);

    // Addition clamps
    let sum = a + b;
    assert_eq!(sum.value(), 1.0);

    // Subtraction clamps
    let diff = b - a;
    assert_eq!(diff.value(), 0.0);

    // Multiplication
    let product = a * 0.5;
    assert_eq!(product.value(), 0.3);
}

#[test]
fn confidence_thresholds() {
    assert!(Confidence::new(0.9).is_high());
    assert!(!Confidence::new(0.5).is_high());
    assert!(Confidence::new(0.1).is_archival());
    assert!(!Confidence::new(0.5).is_archival());
}

#[test]
fn content_hash_is_deterministic() {
    let content = TypedContent::Core(CoreContent {
        project_name: "test".into(),
        description: "desc".into(),
        metadata: serde_json::json!({}),
    });
    let hash1 = BaseMemory::compute_content_hash(&content).unwrap();
    let hash2 = BaseMemory::compute_content_hash(&content).unwrap();
    assert_eq!(hash1, hash2, "same content must produce same hash");
}

#[test]
fn content_hash_differs_for_different_content() {
    let c1 = TypedContent::Core(CoreContent {
        project_name: "a".into(),
        description: "desc".into(),
        metadata: serde_json::json!({}),
    });
    let c2 = TypedContent::Core(CoreContent {
        project_name: "b".into(),
        description: "desc".into(),
        metadata: serde_json::json!({}),
    });
    assert_ne!(
        BaseMemory::compute_content_hash(&c1).unwrap(),
        BaseMemory::compute_content_hash(&c2).unwrap()
    );
}

#[test]
fn base_memory_serde_roundtrip() {
    let content = TypedContent::Tribal(TribalContent {
        knowledge: "Always use prepared statements".into(),
        severity: "high".into(),
        warnings: vec!["SQL injection risk".into()],
        consequences: vec!["Data breach".into()],
    });
    let now = Utc::now();
    let memory = BaseMemory {
        id: "test-id".into(),
        memory_type: MemoryType::Tribal,
        content: content.clone(),
        summary: "Use prepared statements".into(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.95),
        importance: Importance::High,
        last_accessed: now,
        access_count: 5,
        linked_patterns: vec![PatternLink {
            pattern_id: "p1".into(),
            pattern_name: "sql-safety".into(),
        }],
        linked_constraints: vec![],
        linked_files: vec![FileLink {
            file_path: "src/db.rs".into(),
            line_start: Some(10),
            line_end: Some(20),
            content_hash: Some("abc".into()),
        }],
        linked_functions: vec![],
        tags: vec!["security".into(), "database".into()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: BaseMemory::compute_content_hash(&content).unwrap(),
        namespace: Default::default(),
        source_agent: Default::default(),
    };

    let json = serde_json::to_string(&memory).unwrap();
    let deserialized: BaseMemory = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, memory.id);
    assert_eq!(deserialized.memory_type, memory.memory_type);
    assert_eq!(deserialized.content, memory.content);
    assert_eq!(deserialized.summary, memory.summary);
    assert_eq!(deserialized.confidence.value(), memory.confidence.value());
    assert_eq!(deserialized.importance, memory.importance);
    assert_eq!(deserialized.access_count, memory.access_count);
    assert_eq!(deserialized.tags, memory.tags);
    assert_eq!(deserialized.archived, memory.archived);
    assert_eq!(deserialized.content_hash, memory.content_hash);
}

#[test]
fn memory_type_category_labels() {
    assert_eq!(MemoryType::Core.category(), "domain_agnostic");
    assert_eq!(MemoryType::PatternRationale.category(), "code_specific");
    assert_eq!(MemoryType::AgentSpawn.category(), "universal");
    assert_eq!(MemoryType::ConvergenceEvent.category(), "convergence");
}

// ─── Issue 2: PartialEq documents DDD Entity pattern + content_eq ────────────

#[test]
fn partial_eq_compares_only_id() {
    let now = Utc::now();
    let content1 = TypedContent::Core(CoreContent {
        project_name: "project-a".into(),
        description: "desc-a".into(),
        metadata: serde_json::json!({}),
    });
    let content2 = TypedContent::Core(CoreContent {
        project_name: "project-b".into(),
        description: "desc-b".into(),
        metadata: serde_json::json!({"key": "value"}),
    });

    let m1 = BaseMemory {
        id: "same-id".into(),
        memory_type: MemoryType::Core,
        content: content1,
        summary: "summary a".into(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.5),
        importance: Importance::Low,
        last_accessed: now,
        access_count: 1,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec!["a".into()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: "hash-a".into(),
        namespace: Default::default(),
        source_agent: Default::default(),
    };

    let m2 = BaseMemory {
        id: "same-id".into(),
        memory_type: MemoryType::Tribal,
        content: content2,
        summary: "summary b".into(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.9),
        importance: Importance::Critical,
        last_accessed: now,
        access_count: 99,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec!["b".into()],
        archived: true,
        superseded_by: None,
        supersedes: None,
        content_hash: "hash-b".into(),
        namespace: Default::default(),
        source_agent: Default::default(),
    };

    // PartialEq only compares ID (DDD Entity pattern).
    assert_eq!(m1, m2, "same ID means equal per DDD Entity pattern");
    // content_eq should detect the structural difference.
    assert!(
        !m1.content_eq(&m2),
        "content_eq should detect different content"
    );
}

#[test]
fn content_eq_matches_identical_content() {
    let now = Utc::now();
    let content = TypedContent::Semantic(SemanticContent {
        knowledge: "shared knowledge".into(),
        source_episodes: vec![],
        consolidation_confidence: 0.8,
    });

    let m1 = BaseMemory {
        id: "id-1".into(),
        memory_type: MemoryType::Semantic,
        content: content.clone(),
        summary: "same summary".into(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Normal,
        last_accessed: now,
        access_count: 1,
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec!["tag".into()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: "same-hash".into(),
        namespace: Default::default(),
        source_agent: Default::default(),
    };

    let m2 = BaseMemory {
        id: "id-2".into(), // Different ID
        memory_type: MemoryType::Semantic,
        content,
        summary: "same summary".into(),
        transaction_time: now,
        valid_time: now,
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Normal,
        last_accessed: now,
        access_count: 99, // Different access count (not checked by content_eq)
        linked_patterns: vec![],
        linked_constraints: vec![],
        linked_files: vec![],
        linked_functions: vec![],
        tags: vec!["tag".into()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash: "same-hash".into(),
        namespace: Default::default(),
        source_agent: Default::default(),
    };

    // Different IDs → PartialEq says not equal.
    assert_ne!(m1, m2);
    // Same content → content_eq says equal.
    assert!(
        m1.content_eq(&m2),
        "content_eq should match identical content"
    );
}

// ─── Issue 6: compute_content_hash returns CortexResult ──────────────────────

#[test]
fn compute_content_hash_returns_ok_for_valid_content() {
    let content = TypedContent::Core(CoreContent {
        project_name: "test".into(),
        description: "desc".into(),
        metadata: serde_json::json!({}),
    });
    let result = BaseMemory::compute_content_hash(&content);
    assert!(result.is_ok(), "valid content should hash successfully");
    assert!(!result.unwrap().is_empty(), "hash should not be empty");
}

#[test]
fn compute_content_hash_is_deterministic_with_result() {
    let content = TypedContent::Semantic(SemanticContent {
        knowledge: "deterministic test".into(),
        source_episodes: vec!["ep1".into()],
        consolidation_confidence: 0.75,
    });
    let h1 = BaseMemory::compute_content_hash(&content).unwrap();
    let h2 = BaseMemory::compute_content_hash(&content).unwrap();
    assert_eq!(h1, h2, "same content must produce same hash");
}

#[test]
fn compute_content_hash_differs_for_different_content_with_result() {
    let c1 = TypedContent::Core(CoreContent {
        project_name: "a".into(),
        description: "desc".into(),
        metadata: serde_json::json!({}),
    });
    let c2 = TypedContent::Core(CoreContent {
        project_name: "b".into(),
        description: "desc".into(),
        metadata: serde_json::json!({}),
    });
    assert_ne!(
        BaseMemory::compute_content_hash(&c1).unwrap(),
        BaseMemory::compute_content_hash(&c2).unwrap(),
        "different content must produce different hashes"
    );
}
