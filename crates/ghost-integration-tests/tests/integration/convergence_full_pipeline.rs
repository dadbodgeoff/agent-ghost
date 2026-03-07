//! E2E: Full convergence pipeline lifecycle.
//!
//! Validates: ITP event → signal computation → scoring → intervention level
//! → shared state → policy tightening.

use cortex_convergence::scoring::composite::CompositeScorer;
use cortex_convergence::scoring::profiles::ConvergenceProfile;
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
use cortex_decay::factors::convergence::convergence_factor;
use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::convergence_tightener::ConvergencePolicyTightener;
use ghost_policy::corp_policy::CorpPolicy;
use ghost_policy::engine::{PolicyDecision, PolicyEngine};
use itp_protocol::events::{
    ITPEvent, InteractionMessageEvent, MessageSender, SessionEndEvent, SessionStartEvent,
};
use itp_protocol::privacy::{self, PrivacyLevel};
use read_only_pipeline::assembler::{ConvergenceAwareFilter, SnapshotAssembler};
use uuid::Uuid;

/// ITP events serialize and deserialize correctly for pipeline ingestion.
#[test]
fn itp_events_roundtrip_for_pipeline() {
    let events = vec![
        ITPEvent::SessionStart(SessionStartEvent {
            session_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            channel: String::new(),
            privacy_level: PrivacyLevel::Standard,
            timestamp: chrono::Utc::now(),
        }),
        ITPEvent::InteractionMessage(InteractionMessageEvent {
            session_id: Uuid::now_v7(),
            message_id: Uuid::now_v7(),
            sender: MessageSender::Human,
            content_hash: "abc123".into(),
            content_plaintext: None,
            token_count: 42,
            timestamp: chrono::Utc::now(),
        }),
        ITPEvent::SessionEnd(SessionEndEvent {
            session_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            reason: "normal".into(),
            message_count: 10,
            timestamp: chrono::Utc::now(),
        }),
    ];

    for event in &events {
        let json = serde_json::to_string(event).expect("serialize");
        let _: ITPEvent = serde_json::from_str(&json).expect("deserialize");
    }
}

/// Privacy level affects content hashing.
#[test]
fn privacy_level_content_hashing() {
    let content = "sensitive user message";

    // hash_content always returns a SHA-256 hash
    let hash = privacy::hash_content(content);
    assert_ne!(
        hash, content,
        "hash_content should return a hash, not plaintext"
    );

    // apply_privacy with Minimal returns hash only (no plaintext)
    let (min_hash, min_plain) = privacy::apply_privacy(content, PrivacyLevel::Minimal);
    assert!(!min_hash.is_empty());
    assert!(min_plain.is_none(), "Minimal should not include plaintext");

    // apply_privacy with Full returns hash + plaintext
    let (_full_hash, full_plain) = privacy::apply_privacy(content, PrivacyLevel::Full);
    assert_eq!(
        full_plain.as_deref(),
        Some(content),
        "Full should return plaintext"
    );
}

/// Low signals → low score → Level 0 → no policy tightening.
#[test]
fn low_signals_no_tightening() {
    let scorer = CompositeScorer::default();
    let signals = [0.05, 0.08, 0.03, 0.10, 0.07, 0.12, 0.04];

    let score = scorer.compute(&signals);
    let level = scorer.score_to_level(score);

    assert!(
        score < 0.3,
        "Low signals should produce low score: {}",
        score
    );
    assert_eq!(level, 0);

    let tightener = ConvergencePolicyTightener;
    let call = ToolCall {
        tool_name: "heartbeat".into(),
        capability: "heartbeat".into(),
        arguments: serde_json::json!({}),
        is_compaction_flush: false,
    };
    let ctx = PolicyContext {
        agent_id: Uuid::now_v7(),
        session_id: Uuid::now_v7(),
        intervention_level: level,
        session_duration: std::time::Duration::from_secs(0),
        session_denial_count: 0,
        is_compaction_flush: false,
        session_reflection_count: 0,
    };

    assert!(
        tightener.evaluate(&call, &ctx).is_none(),
        "Level 0 should not tighten any tools"
    );
}

/// High signals → high score → Level 3+.
#[test]
fn high_signals_elevated_level() {
    let scorer = CompositeScorer::default();
    let signals = [0.85, 0.90, 0.80, 0.88, 0.92, 0.87, 0.91];

    let score = scorer.compute(&signals);
    let level = scorer.score_to_level(score);

    assert!(
        score > 0.7,
        "High signals should produce high score: {}",
        score
    );
    assert!(level >= 3, "Should be Level 3+: {}", level);
}

/// Convergence score affects memory decay factor.
#[test]
fn convergence_score_affects_decay() {
    let factor_zero = convergence_factor(&MemoryType::Conversation, 0.0);
    assert!((factor_zero - 1.0).abs() < f64::EPSILON);

    let factor_high = convergence_factor(&MemoryType::Conversation, 0.8);
    assert!(factor_high > 1.5);

    let factor_core = convergence_factor(&MemoryType::Core, 0.8);
    assert!((factor_core - 1.0).abs() < f64::EPSILON);
}

/// Full pipeline: signals → score → level → policy decision.
#[test]
fn full_pipeline_signals_to_policy() {
    let scorer = CompositeScorer::default();
    let mut engine = PolicyEngine::new(CorpPolicy::default());
    let agent_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    engine.grant_capability(agent_id, "heartbeat".into());

    let signals_low = [0.1; 7];
    let score_low = scorer.compute(&signals_low);
    let level_low = scorer.score_to_level(score_low);

    let call = ToolCall {
        tool_name: "heartbeat".into(),
        capability: "heartbeat".into(),
        arguments: serde_json::json!({}),
        is_compaction_flush: false,
    };
    let ctx = PolicyContext {
        agent_id,
        session_id,
        intervention_level: level_low,
        session_duration: std::time::Duration::from_secs(0),
        session_denial_count: 0,
        is_compaction_flush: false,
        session_reflection_count: 0,
    };

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Permit));
}

/// Policy denial count triggers at threshold.
#[test]
fn policy_denial_count_triggers_at_threshold() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut engine = PolicyEngine::new(CorpPolicy::default()).with_trigger_sender(tx);
    let agent_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    let call = ToolCall {
        tool_name: "web_search".into(),
        capability: "web_search".into(),
        arguments: serde_json::json!({}),
        is_compaction_flush: false,
    };
    let ctx = PolicyContext {
        agent_id,
        session_id,
        intervention_level: 0,
        session_duration: std::time::Duration::from_secs(0),
        session_denial_count: 0,
        is_compaction_flush: false,
        session_reflection_count: 0,
    };

    for _ in 0..4 {
        engine.evaluate(&call, &ctx);
    }
    assert!(rx.try_recv().is_err(), "Should not trigger at 4 denials");

    engine.evaluate(&call, &ctx);
    assert!(rx.try_recv().is_ok(), "Should trigger at 5 denials");
}

/// Standard and Research profiles produce different scorers.
#[test]
fn profiles_produce_different_scorers() {
    let standard = ConvergenceProfile::Standard.scorer();
    let research = ConvergenceProfile::Research.scorer();

    // They should have different thresholds
    let any_different = standard
        .thresholds
        .iter()
        .zip(research.thresholds.iter())
        .any(|(s, r)| (s - r).abs() > f64::EPSILON);

    assert!(
        any_different,
        "Standard and Research should have different thresholds"
    );
}

/// Standard profile has differentiated weights.
#[test]
fn standard_profile_differentiated_weights() {
    let scorer = ConvergenceProfile::Standard.scorer();
    let first = scorer.weights[0];
    let all_equal = scorer
        .weights
        .iter()
        .all(|&w| (w - first).abs() < f64::EPSILON);
    assert!(!all_equal || scorer.weights.len() == 1);
}

/// Convergence-aware filter at score 0.0 includes all memories.
#[test]
fn filter_full_access_at_zero() {
    let memories = vec![
        make_test_memory(MemoryType::Conversation),
        make_test_memory(MemoryType::Core),
        make_test_memory(MemoryType::Feedback),
    ];

    let filtered = ConvergenceAwareFilter::filter_memories(memories.clone(), 0.0);
    assert_eq!(filtered.len(), 3);
}

/// Convergence-aware filter at high score filters aggressively.
#[test]
fn filter_aggressive_at_high_score() {
    let memories = vec![
        make_test_memory(MemoryType::Conversation),
        make_test_memory(MemoryType::AttachmentIndicator),
        make_test_memory(MemoryType::Core),
    ];

    let filtered = ConvergenceAwareFilter::filter_memories(memories.clone(), 0.85);
    assert!(filtered.len() < memories.len());
}

/// Snapshot assembler produces valid snapshot.
#[test]
fn snapshot_assembler_produces_valid_snapshot() {
    let assembler = SnapshotAssembler::new("You are a simulation.".into());
    let memories = vec![make_test_memory(MemoryType::Core)];

    let snapshot = assembler.assemble(vec![], vec![], memories, 0.0, 0);
    assert_eq!(snapshot.memories().len(), 1);
    assert_eq!(snapshot.convergence_state().score, 0.0);
    assert_eq!(snapshot.convergence_state().level, 0);
    assert!(!snapshot.simulation_prompt().is_empty());
}

/// Empty memory store assembles without error.
#[test]
fn snapshot_empty_memories() {
    let assembler = SnapshotAssembler::new("sim".into());
    let snapshot = assembler.assemble(vec![], vec![], vec![], 0.5, 2);
    assert!(snapshot.memories().is_empty());
}

fn make_test_memory(memory_type: MemoryType) -> BaseMemory {
    BaseMemory {
        id: Uuid::now_v7(),
        memory_type,
        content: serde_json::json!({"text": "test"}),
        summary: "test".into(),
        importance: Importance::Normal,
        confidence: 0.8,
        created_at: chrono::Utc::now(),
        last_accessed: None,
        access_count: 0,
        tags: vec![],
        archived: false,
    }
}
