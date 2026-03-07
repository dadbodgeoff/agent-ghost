//! Snapshot assembly and formatting tests (Task 3.5 — Req 20 AC1–AC4).

use cortex_core::memory::types::convergence::{
    AgentGoalContent, AgentReflectionContent, GoalOrigin, GoalScope, ReflectionTrigger,
};
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
use read_only_pipeline::assembler::{ConvergenceAwareFilter, SnapshotAssembler};
use read_only_pipeline::formatter::SnapshotFormatter;
use read_only_pipeline::snapshot::ConvergenceState;

use chrono::Utc;
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────────

fn make_memory(memory_type: MemoryType) -> BaseMemory {
    BaseMemory {
        id: Uuid::new_v4(),
        memory_type,
        content: serde_json::json!({"data": "test"}),
        summary: "test memory".to_string(),
        importance: Importance::Normal,
        confidence: 0.9,
        created_at: Utc::now(),
        last_accessed: None,
        access_count: 0,
        tags: vec![],
        archived: false,
    }
}

fn make_goal(text: &str) -> AgentGoalContent {
    AgentGoalContent {
        goal_text: text.to_string(),
        scope: GoalScope::Session,
        origin: GoalOrigin::UserDefined,
        parent_goal_id: None,
    }
}

fn make_reflection(text: &str) -> AgentReflectionContent {
    AgentReflectionContent {
        reflection_text: text.to_string(),
        trigger: ReflectionTrigger::SessionEnd,
        depth: 1,
        parent_reflection_id: None,
    }
}

fn diverse_memories() -> Vec<BaseMemory> {
    vec![
        make_memory(MemoryType::Core),
        make_memory(MemoryType::Procedural),
        make_memory(MemoryType::Semantic),
        make_memory(MemoryType::Episodic),
        make_memory(MemoryType::Reference),
        make_memory(MemoryType::Skill),
        make_memory(MemoryType::Goal),
        make_memory(MemoryType::AgentGoal),
        make_memory(MemoryType::Decision),
        make_memory(MemoryType::AttachmentIndicator),
        make_memory(MemoryType::PatternRationale),
        make_memory(MemoryType::ConstraintOverride),
        make_memory(MemoryType::DecisionContext),
    ]
}

// ── AC2: Convergence-aware filtering ────────────────────────────────────

#[test]
fn score_0_0_includes_all_memories() {
    let memories = diverse_memories();
    let count = memories.len();
    let filtered = ConvergenceAwareFilter::filter_memories(memories, 0.0);
    assert_eq!(filtered.len(), count, "score 0.0 = full access");
}

#[test]
fn score_0_5_includes_only_task_focused() {
    let memories = diverse_memories();
    let filtered = ConvergenceAwareFilter::filter_memories(memories, 0.5);

    // Tier 2 (0.5-0.7): task-focused only
    for m in &filtered {
        assert!(
            matches!(
                m.memory_type,
                MemoryType::Core
                    | MemoryType::Procedural
                    | MemoryType::Semantic
                    | MemoryType::Decision
                    | MemoryType::Reference
                    | MemoryType::Skill
                    | MemoryType::Goal
                    | MemoryType::AgentGoal
                    | MemoryType::PatternRationale
                    | MemoryType::ConstraintOverride
                    | MemoryType::DecisionContext
            ),
            "score 0.5 should only include task-focused types, got {:?}",
            m.memory_type
        );
    }

    // Episodic and AttachmentIndicator should be filtered out
    assert!(
        !filtered
            .iter()
            .any(|m| m.memory_type == MemoryType::Episodic),
        "Episodic should be filtered at score 0.5"
    );
    assert!(
        !filtered
            .iter()
            .any(|m| m.memory_type == MemoryType::AttachmentIndicator),
        "AttachmentIndicator should be filtered at score 0.5"
    );
}

#[test]
fn score_0_8_includes_minimal_task_relevant() {
    let memories = diverse_memories();
    let filtered = ConvergenceAwareFilter::filter_memories(memories, 0.8);

    // Tier 3 (0.7-1.0): minimal task-relevant only
    for m in &filtered {
        assert!(
            matches!(
                m.memory_type,
                MemoryType::Core
                    | MemoryType::Procedural
                    | MemoryType::Semantic
                    | MemoryType::Reference
            ),
            "score 0.8 should only include minimal types, got {:?}",
            m.memory_type
        );
    }
}

#[test]
fn score_0_35_filters_attachment_indicators() {
    let memories = diverse_memories();
    let filtered = ConvergenceAwareFilter::filter_memories(memories, 0.35);

    // Tier 1 (0.3-0.5): reduced emotional/attachment weight
    assert!(
        !filtered
            .iter()
            .any(|m| m.memory_type == MemoryType::AttachmentIndicator),
        "AttachmentIndicator should be filtered at score 0.35"
    );
}

#[test]
fn score_clamped_to_valid_range() {
    let memories = diverse_memories();
    let count = memories.len();

    // Negative score should be clamped to 0.0 (full access)
    let filtered = ConvergenceAwareFilter::filter_memories(memories.clone(), -0.5);
    assert_eq!(filtered.len(), count);

    // Score > 1.0 should be clamped to 1.0 (minimal)
    let filtered = ConvergenceAwareFilter::filter_memories(memories, 1.5);
    for m in &filtered {
        assert!(matches!(
            m.memory_type,
            MemoryType::Core
                | MemoryType::Procedural
                | MemoryType::Semantic
                | MemoryType::Reference
        ));
    }
}

// ── AC1 + AC3: Snapshot immutability ────────────────────────────────────

#[test]
fn snapshot_is_immutable_no_mutation_methods() {
    let assembler = SnapshotAssembler::new("You are a helpful assistant.".to_string());
    let snapshot = assembler.assemble(
        vec![make_goal("test goal")],
        vec![make_reflection("test reflection")],
        diverse_memories(),
        0.0,
        0,
    );

    // Verify read-only access works
    assert_eq!(snapshot.goals().len(), 1);
    assert_eq!(snapshot.reflections().len(), 1);
    assert!(!snapshot.memories().is_empty());
    assert_eq!(snapshot.convergence_state().score, 0.0);
    assert_eq!(snapshot.convergence_state().level, 0);
    assert_eq!(snapshot.simulation_prompt(), "You are a helpful assistant.");

    // The struct has no pub fields and no &mut self methods — immutability
    // is enforced at the type level. This test documents that contract.
}

#[test]
fn snapshot_includes_simulation_prompt() {
    let prompt = "SIMULATION BOUNDARY: You are an AI assistant.";
    let assembler = SnapshotAssembler::new(prompt.to_string());
    let snapshot = assembler.assemble(vec![], vec![], vec![], 0.0, 0);

    assert_eq!(snapshot.simulation_prompt(), prompt);
}

// ── AC4: SnapshotFormatter ──────────────────────────────────────────────

#[test]
fn formatter_produces_non_empty_text() {
    let assembler = SnapshotAssembler::new("sim prompt".to_string());
    let snapshot = assembler.assemble(
        vec![make_goal("build feature X")],
        vec![make_reflection("session went well")],
        vec![make_memory(MemoryType::Core)],
        0.2,
        0,
    );

    let formatter = SnapshotFormatter::new();
    let text = formatter.format(&snapshot, 1000);
    assert!(!text.is_empty());
    assert!(text.contains("[Convergence]"));
    assert!(text.contains("[Goals]"));
    assert!(text.contains("[Reflections]"));
    assert!(text.contains("[Memories]"));
}

#[test]
fn formatter_respects_token_budget() {
    let assembler = SnapshotAssembler::new("sim prompt".to_string());
    let goals: Vec<_> = (0..100).map(|i| make_goal(&format!("goal {i}"))).collect();
    let snapshot = assembler.assemble(goals, vec![], diverse_memories(), 0.0, 0);

    let formatter = SnapshotFormatter::new();
    // Very small budget: 10 tokens ≈ 40 chars
    let text = formatter.format(&snapshot, 10);
    assert!(
        text.len() <= 40,
        "text length {} exceeds budget",
        text.len()
    );
}

#[test]
fn formatter_includes_convergence_score() {
    let assembler = SnapshotAssembler::new("sim".to_string());
    let snapshot = assembler.assemble(vec![], vec![], vec![], 0.456, 2);

    let formatter = SnapshotFormatter::new();
    let text = formatter.format(&snapshot, 1000);
    assert!(text.contains("0.456"), "should contain score");
    assert!(text.contains("level=2"), "should contain level");
}

// ── Adversarial ─────────────────────────────────────────────────────────

#[test]
fn empty_memory_store_assembles_without_error() {
    let assembler = SnapshotAssembler::new("sim".to_string());
    let snapshot = assembler.assemble(vec![], vec![], vec![], 0.0, 0);

    assert!(snapshot.goals().is_empty());
    assert!(snapshot.reflections().is_empty());
    assert!(snapshot.memories().is_empty());
}

#[test]
fn large_memory_store_assembles_quickly() {
    let memories: Vec<_> = (0..10_000).map(|_| make_memory(MemoryType::Core)).collect();
    let assembler = SnapshotAssembler::new("sim".to_string());

    let start = std::time::Instant::now();
    let snapshot = assembler.assemble(vec![], vec![], memories, 0.8, 3);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 100,
        "10,000 memories should assemble in <100ms, took {}ms",
        elapsed.as_millis()
    );
    // At score 0.8, only Core/Procedural/Semantic/Reference survive
    assert_eq!(snapshot.memories().len(), 10_000); // All are Core
}

// ── ConvergenceState ────────────────────────────────────────────────────

#[test]
fn convergence_state_default_is_zero() {
    let state = ConvergenceState::default();
    assert_eq!(state.score, 0.0);
    assert_eq!(state.level, 0);
}

#[test]
fn convergence_state_serializes_round_trip() {
    let state = ConvergenceState {
        score: 0.75,
        level: 3,
    };
    let json = serde_json::to_string(&state).unwrap();
    let deserialized: ConvergenceState = serde_json::from_str(&json).unwrap();
    assert_eq!(state, deserialized);
}
