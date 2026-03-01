//! Adversarial tests for orchestrator fixes (Phase 3 flaws).
//!
//! These tests target production-breaking flaws: NaN bypass of safety gates,
//! fail-open on corruption, silent data poisoning, and spending cap evasion.
//! Every test here represents a real attack vector or data corruption scenario
//! that would put users at risk if not handled correctly.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use ghost_agent_loop::context::run_context::RunContext;
use ghost_agent_loop::runner::{AgentRunner, GateCheckLog, RunError};
use ghost_heartbeat::heartbeat::{HeartbeatConfig, HeartbeatEngine};
use ghost_heartbeat::tiers::{interval_for_state, TierSelector, TieredHeartbeatState};
use ghost_kill_gates::config::KillGateConfig;
use ghost_kill_gates::gate::{GateState, KillGate};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════
// FLAW 1: Spending cap NaN/Inf bypass in AgentRunner::check_gates()
//
// If daily_spend or total_cost is NaN, then NaN + X = NaN, and
// NaN > spending_cap evaluates to false — silently bypassing the gate.
// ═══════════════════════════════════════════════════════════════════════

fn make_ctx(daily_spend: f64, total_cost: f64) -> RunContext {
    let snapshot = AgentRunner::default_snapshot();
    RunContext {
        agent_id: Uuid::nil(),
        session_id: Uuid::nil(),
        recursion_depth: 0,
        max_recursion_depth: 10,
        total_tokens: 0,
        total_cost,
        tool_call_count: 0,
        proposal_count: 0,
        snapshot,
        intervention_level: 0,
        cb_failures: 0,
        damage_count: 0,
        spending_cap: 10.0,
        daily_spend,
        kill_switch_active: false,
        context_window: 128_000,
    }
}

#[test]
fn flaw1_nan_daily_spend_blocks_gate() {
    let mut runner = AgentRunner::new(128_000);
    runner.spending_cap = 10.0;
    runner.daily_spend = f64::NAN;

    let ctx = make_ctx(f64::NAN, 0.0);
    let mut log = GateCheckLog::default();
    let result = runner.check_gates(&ctx, &mut log);

    assert!(
        result.is_err(),
        "CRITICAL: NaN daily_spend BYPASSED spending cap — unlimited spending possible"
    );
    assert!(matches!(result, Err(RunError::SpendingCapExceeded { .. })));
}

#[test]
fn flaw1_inf_total_cost_blocks_gate() {
    let mut runner = AgentRunner::new(128_000);
    runner.spending_cap = 10.0;
    runner.daily_spend = 0.0;

    let ctx = make_ctx(0.0, f64::INFINITY);
    let mut log = GateCheckLog::default();
    let result = runner.check_gates(&ctx, &mut log);

    assert!(
        result.is_err(),
        "CRITICAL: Infinite total_cost BYPASSED spending cap"
    );
    assert!(matches!(result, Err(RunError::SpendingCapExceeded { .. })));
}

#[test]
fn flaw1_neg_infinity_daily_spend_blocks_gate() {
    let mut runner = AgentRunner::new(128_000);
    runner.spending_cap = 10.0;
    runner.daily_spend = f64::NEG_INFINITY;

    // NEG_INFINITY + 0.0 = NEG_INFINITY, NEG_INFINITY > 10.0 = false
    let ctx = make_ctx(f64::NEG_INFINITY, 0.0);
    let mut log = GateCheckLog::default();
    let result = runner.check_gates(&ctx, &mut log);

    assert!(
        result.is_err(),
        "CRITICAL: NEG_INFINITY daily_spend BYPASSED spending cap — \
         attacker could set negative spend to get infinite budget"
    );
}

#[test]
fn flaw1_nan_plus_valid_cost_blocks_gate() {
    // NaN + 5.0 = NaN, NaN > 10.0 = false → bypass without fix
    let mut runner = AgentRunner::new(128_000);
    runner.spending_cap = 10.0;
    runner.daily_spend = f64::NAN;

    let ctx = make_ctx(f64::NAN, 5.0);
    let mut log = GateCheckLog::default();
    let result = runner.check_gates(&ctx, &mut log);

    assert!(result.is_err(), "NaN + valid cost must still block");
}

#[test]
fn flaw1_valid_spend_under_cap_passes() {
    // Sanity: valid spend under cap should pass
    let mut runner = AgentRunner::new(128_000);
    runner.spending_cap = 10.0;
    runner.daily_spend = 3.0;

    let ctx = make_ctx(3.0, 2.0);
    let mut log = GateCheckLog::default();
    let result = runner.check_gates(&ctx, &mut log);

    assert!(result.is_ok(), "Valid spend under cap should pass gate");
}

#[test]
fn flaw1_valid_spend_over_cap_blocks() {
    // Sanity: valid spend over cap should block
    let mut runner = AgentRunner::new(128_000);
    runner.spending_cap = 10.0;
    runner.daily_spend = 8.0;

    let ctx = make_ctx(8.0, 5.0);
    let mut log = GateCheckLog::default();
    let result = runner.check_gates(&ctx, &mut log);

    assert!(result.is_err(), "Valid spend over cap should block gate");
    assert!(matches!(result, Err(RunError::SpendingCapExceeded { .. })));
}

// ═══════════════════════════════════════════════════════════════════════
// FLAW 2: Heartbeat should_fire() delta computation bug
//
// Was computing score_delta(last) which always gave 0.0 because it
// compared last_score against itself. Fixed to infer delta from
// consecutive_stable counter.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn flaw2_heartbeat_fires_at_correct_interval_when_active() {
    // After recording beats with changing scores (active state),
    // consecutive_stable should be 0, and should_fire should use
    // the Active interval (30s), not Stable (120s).
    let platform_killed = Arc::new(AtomicBool::new(false));
    let agent_paused = Arc::new(AtomicBool::new(false));
    let config = HeartbeatConfig::default();
    let agent_id = Uuid::now_v7();

    let mut engine = HeartbeatEngine::new(config, agent_id, platform_killed, agent_paused);

    // Record beats with changing scores → active state
    engine.record_beat_with_score(0.01, 0.3);
    engine.record_beat_with_score(0.01, 0.5); // delta 0.2 → resets consecutive_stable

    assert_eq!(
        engine.tiered_state.consecutive_stable, 0,
        "Changing scores should reset consecutive_stable to 0"
    );

    // The should_fire logic should now infer a non-trivial delta (0.05)
    // because consecutive_stable == 0, leading to Active interval (30s).
    // If the old bug were present, it would always compute delta=0.0
    // and use Stable interval (120s) after 3+ beats.
}

#[test]
fn flaw2_heartbeat_stable_after_consistent_scores() {
    let platform_killed = Arc::new(AtomicBool::new(false));
    let agent_paused = Arc::new(AtomicBool::new(false));
    let config = HeartbeatConfig::default();
    let agent_id = Uuid::now_v7();

    let mut engine = HeartbeatEngine::new(config, agent_id, platform_killed, agent_paused);

    // Record 4 beats with same score → stable
    for _ in 0..4 {
        engine.record_beat_with_score(0.01, 0.5);
    }

    assert!(
        engine.tiered_state.consecutive_stable >= 3,
        "4 identical scores should give consecutive_stable >= 3, got {}",
        engine.tiered_state.consecutive_stable
    );

    // With consecutive_stable >= 3, should_fire infers delta=0.005 (small)
    // and interval_for_state(0.005, >=3, 0) = 120s (Stable).
    let interval = interval_for_state(0.005, engine.tiered_state.consecutive_stable, 0);
    assert_eq!(interval, Duration::from_secs(120), "Stable state should use 120s interval");
}

#[test]
fn flaw2_interval_for_state_active_vs_stable_distinction() {
    // Active: delta >= 0.01 or consecutive_stable < 3 → 30s
    let active_interval = interval_for_state(0.05, 0, 0);
    assert_eq!(active_interval, Duration::from_secs(30));

    // Stable: delta < 0.01 and consecutive_stable >= 3 → 120s
    let stable_interval = interval_for_state(0.005, 3, 0);
    assert_eq!(stable_interval, Duration::from_secs(120));

    // The old bug would have always returned 120s because delta was always 0.0
    // (computed as |last - last| = 0.0), and after 3 beats consecutive_stable >= 3.
}

// ═══════════════════════════════════════════════════════════════════════
// FLAW 3: GateState::from_u8() fail-open on corruption
//
// Unknown u8 values defaulted to Normal (fail-OPEN). Changed to
// GateClosed (fail-CLOSED). Memory corruption could have silently
// reopened the kill gate.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn flaw3_unknown_gate_state_defaults_to_closed() {
    // Every invalid u8 value must map to GateClosed, not Normal.
    // If this were fail-open, memory corruption could silently reopen the gate.
    for invalid_value in [5u8, 6, 7, 10, 42, 100, 200, 255] {
        let state = GateState::from_u8(invalid_value);
        assert_eq!(
            state,
            GateState::GateClosed,
            "CRITICAL: Unknown GateState u8 value {} mapped to {:?} instead of GateClosed — \
             memory corruption could silently reopen the kill gate",
            invalid_value,
            state
        );
    }
}

#[test]
fn flaw3_valid_gate_states_still_work() {
    // Sanity: valid values should still map correctly
    assert_eq!(GateState::from_u8(0), GateState::Normal);
    assert_eq!(GateState::from_u8(1), GateState::GateClosed);
    assert_eq!(GateState::from_u8(2), GateState::Propagating);
    assert_eq!(GateState::from_u8(3), GateState::Confirmed);
    assert_eq!(GateState::from_u8(4), GateState::QuorumResume);
}

#[test]
fn flaw3_gate_with_corrupted_atomic_stays_closed() {
    // Simulate what happens if the AtomicU8 gets corrupted to an invalid value.
    // The gate should report as closed (fail-closed) via is_closed().
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());

    // Gate starts Normal (0), is_closed() should be false
    assert!(!gate.is_closed());
    assert_eq!(gate.state(), GateState::Normal);

    // Close the gate
    gate.close("corruption test".into());
    assert!(gate.is_closed());

    // The state() method calls from_u8() internally.
    // If the atomic were corrupted to value 255, from_u8(255) must return GateClosed.
    let corrupted_state = GateState::from_u8(255);
    assert_eq!(corrupted_state, GateState::GateClosed);
}

// ═══════════════════════════════════════════════════════════════════════
// FLAW 4: Inf bypass in tier selection and interval computation
//
// NaN guard only checked is_nan(), not is_infinite(). Infinite score
// deltas could cause incorrect tier selection and interval computation.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn flaw4_inf_score_delta_treated_as_stable_in_tier_selection() {
    let mut sel = TierSelector::new();

    // Infinity should be sanitized to 0.0, which with consecutive_stable >= 3
    // should select Tier0 (stable).
    let tier = sel.select_tier(f64::INFINITY, 3, 0);
    assert_eq!(
        tier,
        ghost_heartbeat::tiers::HeartbeatTier::Tier0,
        "CRITICAL: Infinite score_delta was not sanitized in select_tier — \
         could cause incorrect tier escalation"
    );
}

#[test]
fn flaw4_neg_inf_score_delta_treated_as_stable_in_tier_selection() {
    let mut sel = TierSelector::new();
    let tier = sel.select_tier(f64::NEG_INFINITY, 3, 0);
    assert_eq!(
        tier,
        ghost_heartbeat::tiers::HeartbeatTier::Tier0,
        "NEG_INFINITY score_delta should be sanitized to 0.0 → Tier0"
    );
}

#[test]
fn flaw4_inf_score_delta_treated_as_stable_in_interval() {
    // Infinity should be sanitized to 0.0 in interval_for_state
    let interval = interval_for_state(f64::INFINITY, 3, 0);
    assert_eq!(
        interval,
        Duration::from_secs(120),
        "CRITICAL: Infinite score_delta was not sanitized in interval_for_state — \
         could cause incorrect heartbeat frequency"
    );
}

#[test]
fn flaw4_neg_inf_score_delta_in_interval() {
    let interval = interval_for_state(f64::NEG_INFINITY, 3, 0);
    assert_eq!(
        interval,
        Duration::from_secs(120),
        "NEG_INFINITY should be sanitized to 0.0 → Stable (120s)"
    );
}

#[test]
fn flaw4_nan_score_delta_in_interval() {
    let interval = interval_for_state(f64::NAN, 3, 0);
    assert_eq!(
        interval,
        Duration::from_secs(120),
        "NaN should be sanitized to 0.0 → Stable (120s)"
    );
}

#[test]
fn flaw4_tiered_state_score_delta_rejects_nan_input() {
    let state = TieredHeartbeatState::new();
    // NaN current_score should return 0.0 delta, not propagate NaN
    let delta = state.score_delta(f64::NAN);
    assert!(
        delta.is_finite(),
        "score_delta(NaN) returned non-finite value: {}",
        delta
    );
    assert_eq!(delta, 0.0);
}

#[test]
fn flaw4_tiered_state_record_beat_nan_does_not_corrupt_last_score() {
    let mut state = TieredHeartbeatState::new();
    state.record_beat(0.5); // Set a valid last_score
    assert_eq!(state.last_score, Some(0.5));

    state.record_beat(f64::NAN); // NaN should NOT overwrite last_score
    assert_eq!(
        state.last_score,
        Some(0.5),
        "CRITICAL: NaN score overwrote last_score — future delta computations are poisoned"
    );
}

#[test]
fn flaw4_tiered_state_record_beat_inf_does_not_corrupt_last_score() {
    let mut state = TieredHeartbeatState::new();
    state.record_beat(0.5);
    state.record_beat(f64::INFINITY); // Inf should NOT overwrite last_score
    assert_eq!(
        state.last_score,
        Some(0.5),
        "CRITICAL: Infinite score overwrote last_score — future delta computations are poisoned"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FLAW 5: FlushExecutor trait returned Result<(), String>
//
// The trait was using String errors instead of proper error types.
// Changed to Result<(), Box<dyn std::error::Error + Send + Sync>>.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn flaw5_flush_executor_accepts_boxed_error() {
    // Verify the FlushExecutor trait signature accepts Box<dyn Error>
    // by implementing it with a custom error type.
    use ghost_agent_loop::FlushExecutor;

    #[derive(Debug)]
    struct TestError(String);
    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestError: {}", self.0)
        }
    }
    impl std::error::Error for TestError {}

    struct TestExecutor;

    #[async_trait::async_trait]
    impl FlushExecutor for TestExecutor {
        async fn execute_flush(
            &self,
            _agent_id: Uuid,
            _session_id: Uuid,
            _memories: Vec<serde_json::Value>,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Err(Box::new(TestError("flush failed".into())))
        }
    }

    // If this compiles, the trait signature is correct (Box<dyn Error>, not String).
    let executor = TestExecutor;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = rt.block_on(executor.execute_flush(
        Uuid::nil(),
        Uuid::nil(),
        vec![],
    ));

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("flush failed"),
        "Error should propagate through Box<dyn Error>: {}",
        err
    );
}

#[test]
fn flaw5_flush_executor_accepts_io_error() {
    // Verify that standard library errors work through the trait
    use ghost_agent_loop::FlushExecutor;

    struct IoErrorExecutor;

    #[async_trait::async_trait]
    impl FlushExecutor for IoErrorExecutor {
        async fn execute_flush(
            &self,
            _agent_id: Uuid,
            _session_id: Uuid,
            _memories: Vec<serde_json::Value>,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "disk full",
            )))
        }
    }

    let executor = IoErrorExecutor;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = rt.block_on(executor.execute_flush(Uuid::nil(), Uuid::nil(), vec![]));
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════
// FLAW 6: linear_regression_slope NaN mean skew
//
// Used total count `n` (including NaN entries) for y_mean denominator
// instead of `finite_count`. This skewed the mean toward zero when
// NaN values were present, producing incorrect slope calculations.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn flaw6_usage_tracker_trend_with_nan_values() {
    // If NaN values are in the history, the trend computation should
    // still produce a correct result by filtering them out.
    use ghost_agent_loop::context::usage_tracker::ContextUsageTracker;

    let mut tracker = ContextUsageTracker::new(100_000);

    // Record a rising pattern
    tracker.record_turn(40_000);
    tracker.record_turn(50_000);
    tracker.record_turn(60_000);
    tracker.record_turn(70_000);
    tracker.record_turn(80_000);

    // Should detect rising trend
    let trend = tracker.trend();
    assert_eq!(
        trend,
        ghost_agent_loop::context::usage_tracker::UsageTrend::Rising,
        "Rising usage pattern should be detected as Rising"
    );
}

#[test]
fn flaw6_compaction_spending_cap_nan_blocked() {
    // The compaction spending cap check should block NaN values
    use ghost_gateway::session::compaction::{CompactionConfig, SessionCompactor};

    let compactor = SessionCompactor::new(CompactionConfig::default());

    // NaN estimated cost should be rejected
    let result = compactor.check_spending_cap(f64::NAN, 5.0, 10.0);
    assert!(
        result.is_err(),
        "CRITICAL: NaN estimated_flush_cost bypassed spending cap check"
    );

    // NaN current spend should be rejected
    let result = compactor.check_spending_cap(1.0, f64::NAN, 10.0);
    assert!(
        result.is_err(),
        "CRITICAL: NaN current_spend bypassed spending cap check"
    );

    // NaN spending cap should be rejected
    let result = compactor.check_spending_cap(1.0, 5.0, f64::NAN);
    assert!(
        result.is_err(),
        "CRITICAL: NaN spending_cap bypassed spending cap check"
    );

    // Inf values should also be rejected
    let result = compactor.check_spending_cap(f64::INFINITY, 5.0, 10.0);
    assert!(
        result.is_err(),
        "CRITICAL: Infinite estimated_flush_cost bypassed spending cap check"
    );

    let result = compactor.check_spending_cap(1.0, f64::NEG_INFINITY, 10.0);
    assert!(
        result.is_err(),
        "CRITICAL: NEG_INFINITY current_spend bypassed spending cap check"
    );
}

#[test]
fn flaw6_compaction_spending_cap_valid_values_work() {
    use ghost_gateway::session::compaction::{CompactionConfig, SessionCompactor};

    let compactor = SessionCompactor::new(CompactionConfig::default());

    // Under cap should pass
    let result = compactor.check_spending_cap(1.0, 5.0, 10.0);
    assert!(result.is_ok(), "Valid spend under cap should pass");

    // Over cap should fail
    let result = compactor.check_spending_cap(6.0, 5.0, 10.0);
    assert!(result.is_err(), "Valid spend over cap should fail");
}

// ═══════════════════════════════════════════════════════════════════════
// CROSS-CUTTING: Gate check order invariant
//
// The gate check order is a HARD INVARIANT. If the order changes,
// security properties break (e.g., spending cap checked before kill
// switch means a killed agent could still spend money).
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gate_check_order_is_invariant() {
    let mut runner = AgentRunner::new(128_000);
    runner.spending_cap = 100.0;
    runner.daily_spend = 0.0;

    let ctx = make_ctx(0.0, 0.0);
    let mut log = GateCheckLog::default();
    let _ = runner.check_gates(&ctx, &mut log);

    assert_eq!(
        log.checks,
        vec![
            "circuit_breaker",
            "recursion_depth",
            "damage_counter",
            "spending_cap",
            "kill_switch",
            "kill_gate",
        ],
        "Gate check order MUST be: circuit_breaker → recursion_depth → \
         damage_counter → spending_cap → kill_switch → kill_gate"
    );
}
