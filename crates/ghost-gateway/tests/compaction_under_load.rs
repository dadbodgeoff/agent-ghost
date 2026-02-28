//! Adversarial test suite: Compaction behavior under load.
//!
//! Tests compaction trigger thresholds, rollback on failure,
//! CompactionBlock immutability, and per-type compression minimums.

use ghost_gateway::session::compaction::{
    CompactionConfig, PerTypeMinimums, SessionCompactor,
};

// ── Trigger threshold ───────────────────────────────────────────────────

#[test]
fn compaction_triggers_at_70_percent() {
    let compactor = SessionCompactor::default();
    let context_window = 10_000;
    let current_tokens = 7_000; // 70%
    assert!(
        compactor.should_compact(current_tokens, context_window),
        "Compaction should trigger at 70% context window"
    );
}

#[test]
fn compaction_does_not_trigger_at_69_percent() {
    let compactor = SessionCompactor::default();
    let context_window = 10_000;
    let current_tokens = 6_900; // 69%
    assert!(
        !compactor.should_compact(current_tokens, context_window),
        "Compaction should NOT trigger at 69% context window"
    );
}

#[test]
fn compaction_triggers_at_exact_threshold() {
    let compactor = SessionCompactor::default();
    // Exactly 70% = 7000/10000
    assert!(compactor.should_compact(7000, 10000));
    // Just below: 6999/10000 = 0.6999
    assert!(!compactor.should_compact(6999, 10000));
}

// ── CompactionBlock immutability ────────────────────────────────────────

#[test]
fn compaction_block_identifies_itself() {
    let compactor = SessionCompactor::default();
    let mut history = vec!["msg1".to_string(), "msg2".to_string(), "msg3".to_string()];
    let block = compactor.compact(&mut history, 1, None).unwrap();
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

#[test]
fn exceeding_max_passes_returns_error() {
    let compactor = SessionCompactor::default();
    let mut history = vec!["msg".to_string()];
    let result = compactor.compact(&mut history, 4, None); // pass 4 > max 3
    assert!(result.is_err(), "Pass 4 should exceed max_passes=3");
}

// ── Per-type compression minimums ───────────────────────────────────────

#[test]
fn per_type_minimums_defaults() {
    let minimums = PerTypeMinimums::default();
    assert_eq!(minimums.convergence_event, 3, "ConvergenceEvent minimum should be L3");
    assert_eq!(minimums.boundary_violation, 3, "BoundaryViolation minimum should be L3");
    assert_eq!(minimums.agent_goal, 2, "AgentGoal minimum should be L2");
    assert_eq!(minimums.intervention_plan, 2, "InterventionPlan minimum should be L2");
    assert_eq!(minimums.agent_reflection, 1, "AgentReflection minimum should be L1");
    assert_eq!(minimums.proposal_record, 1, "ProposalRecord minimum should be L1");
    assert_eq!(minimums.other, 0, "Other minimum should be L0");
}

// ── Config defaults ─────────────────────────────────────────────────────

#[test]
fn compaction_config_defaults() {
    let config = CompactionConfig::default();
    assert_eq!(config.trigger_threshold, 0.70);
    assert_eq!(config.max_passes, 3);
    assert!(config.memory_flush_enabled);
}

// ── Compact produces valid block ────────────────────────────────────────

#[test]
fn compact_produces_valid_block() {
    let compactor = SessionCompactor::default();
    let mut history = vec![
        "message one".to_string(),
        "message two".to_string(),
        "message three".to_string(),
    ];
    let original_len = history.len();
    let block = compactor.compact(&mut history, 1, None).unwrap();

    assert_eq!(block.pass_number, 1);
    assert!(block.original_token_count > 0);
    assert!(block.compressed_token_count < block.original_token_count);
    // History should be replaced with the compaction block
    assert!(history.len() < original_len, "History should be compacted");
}

// ── Prune tool results ──────────────────────────────────────────────────

#[test]
fn prune_removes_tool_results() {
    let mut history = vec![
        "user message".to_string(),
        r#"{"tool_result": "some output"}"#.to_string(),
        "agent response".to_string(),
        r#"{"tool_result": "another output"}"#.to_string(),
    ];
    let result = SessionCompactor::prune_tool_results(&mut history);
    assert_eq!(result.results_pruned, 2, "Should prune 2 tool_result entries");
    assert_eq!(history.len(), 2, "Should have 2 messages remaining");
}
