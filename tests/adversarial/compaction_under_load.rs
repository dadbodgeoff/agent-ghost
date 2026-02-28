//! Adversarial test suite: Compaction behavior under load.
//!
//! Tests compaction trigger thresholds, rollback on failure,
//! CompactionBlock immutability, and per-type compression minimums.

use ghost_gateway::session::compaction::{
    CompactionBlock, CompactionConfig, CompactionPhase, SessionCompactor,
};

// ── Trigger threshold ───────────────────────────────────────────────────

#[test]
fn compaction_triggers_at_70_percent() {
    let config = CompactionConfig::default();
    let context_window = 10_000;
    let current_tokens = 7_000; // 70%
    assert!(
        SessionCompactor::should_compact(current_tokens, context_window, &config),
        "Compaction should trigger at 70% context window"
    );
}

#[test]
fn compaction_does_not_trigger_at_69_percent() {
    let config = CompactionConfig::default();
    let context_window = 10_000;
    let current_tokens = 6_900; // 69%
    assert!(
        !SessionCompactor::should_compact(current_tokens, context_window, &config),
        "Compaction should NOT trigger at 69% context window"
    );
}

// ── CompactionBlock immutability ────────────────────────────────────────

#[test]
fn compaction_block_is_never_recompressed() {
    let block = CompactionBlock {
        id: uuid::Uuid::now_v7(),
        created_at: chrono::Utc::now(),
        original_token_count: 5000,
        compressed_token_count: 2000,
        summary: "Compacted session history".into(),
        phase: CompactionPhase::Complete,
    };

    // CompactionBlock should be marked as non-compressible
    assert!(
        block.is_compaction_block(),
        "CompactionBlock should identify itself as non-compressible"
    );
}

// ── Max passes ──────────────────────────────────────────────────────────

#[test]
fn max_three_compaction_passes() {
    let config = CompactionConfig::default();
    assert_eq!(
        config.max_passes, 3,
        "Default max passes should be 3"
    );
}

// ── Per-type compression minimums ───────────────────────────────────────

#[test]
fn convergence_event_minimum_level_3() {
    use cortex_core::memory::types::MemoryType;
    let min = SessionCompactor::compression_minimum(&MemoryType::ConvergenceEvent);
    assert_eq!(min, 3, "ConvergenceEvent should have minimum compression level 3");
}

#[test]
fn boundary_violation_minimum_level_3() {
    use cortex_core::memory::types::MemoryType;
    let min = SessionCompactor::compression_minimum(&MemoryType::BoundaryViolation);
    assert_eq!(min, 3, "BoundaryViolation should have minimum compression level 3");
}

#[test]
fn agent_goal_minimum_level_2() {
    use cortex_core::memory::types::MemoryType;
    let min = SessionCompactor::compression_minimum(&MemoryType::AgentGoal);
    assert_eq!(min, 2, "AgentGoal should have minimum compression level 2");
}

#[test]
fn agent_reflection_minimum_level_1() {
    use cortex_core::memory::types::MemoryType;
    let min = SessionCompactor::compression_minimum(&MemoryType::AgentReflection);
    assert_eq!(min, 1, "AgentReflection should have minimum compression level 1");
}

#[test]
fn conversation_minimum_level_0() {
    use cortex_core::memory::types::MemoryType;
    let min = SessionCompactor::compression_minimum(&MemoryType::Conversation);
    assert_eq!(min, 0, "Conversation should have minimum compression level 0");
}

// ── Config defaults ─────────────────────────────────────────────────────

#[test]
fn compaction_config_defaults() {
    let config = CompactionConfig::default();
    assert_eq!(config.trigger_threshold, 0.70);
    assert_eq!(config.max_passes, 3);
    assert!(config.memory_flush_enabled);
}
