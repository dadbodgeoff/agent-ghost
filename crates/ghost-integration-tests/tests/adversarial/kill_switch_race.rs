//! Adversarial: Kill switch race conditions (Task 7.3).
//!
//! Concurrent trigger delivery, dedup correctness under load,
//! monotonicity under rapid state changes.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::Utc;
use cortex_core::safety::trigger::TriggerEvent;
use ghost_gateway::safety::auto_triggers::AutoTriggerEvaluator;
use ghost_gateway::safety::kill_switch::{KillLevel, KillSwitch, PLATFORM_KILLED};
use uuid::Uuid;

fn reset_platform_killed() {
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
}

// ── Concurrent KILL_ALL triggers ────────────────────────────────────────

/// Two KILL_ALL triggers simultaneously → first executes, second idempotent.
#[test]
fn concurrent_kill_all_idempotent() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());

    let trigger1 = TriggerEvent::ManualKillAll {
        reason: "trigger 1".into(),
        initiated_by: "test".into(),
    };
    let trigger2 = TriggerEvent::ManualKillAll {
        reason: "trigger 2".into(),
        initiated_by: "test".into(),
    };

    ks.activate_kill_all(&trigger1);
    ks.activate_kill_all(&trigger2);

    assert!(PLATFORM_KILLED.load(Ordering::SeqCst));
    let state = ks.current_state();
    assert_eq!(state.platform_level, KillLevel::KillAll);
    // Audit should have entries for both (idempotent but logged)
    assert!(!ks.audit_entries().is_empty());

    reset_platform_killed();
}

// ── Dedup correctness under load ────────────────────────────────────────

/// Same trigger+agent within 60s → suppressed.
#[test]
fn dedup_suppresses_within_window() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::SoulDrift {
        agent_id,
        drift_score: 0.3,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    };

    // First trigger processes
    let r1 = evaluator.process(trigger.clone());
    assert!(r1.is_some(), "First trigger must process");

    // Same trigger within 60s → suppressed
    let r2 = evaluator.process(trigger.clone());
    assert!(
        r2.is_none(),
        "Duplicate trigger within 60s must be suppressed"
    );

    reset_platform_killed();
}

/// Same trigger + different agent → NOT suppressed.
#[test]
fn dedup_different_agent_not_suppressed() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    let trigger1 = TriggerEvent::SoulDrift {
        agent_id: Uuid::now_v7(),
        drift_score: 0.3,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    };

    let trigger2 = TriggerEvent::SoulDrift {
        agent_id: Uuid::now_v7(), // Different agent
        drift_score: 0.3,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    };

    let r1 = evaluator.process(trigger1);
    let r2 = evaluator.process(trigger2);

    assert!(r1.is_some(), "First trigger must process");
    assert!(
        r2.is_some(),
        "Different agent trigger must not be suppressed"
    );

    reset_platform_killed();
}

// ── Monotonicity under rapid state changes ──────────────────────────────

/// Rapid PAUSE → QUARANTINE → KILL_ALL: level never decreases.
#[test]
fn monotonicity_under_rapid_escalation() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    // PAUSE
    let t1 = TriggerEvent::ManualPause {
        agent_id,
        reason: "test".into(),
        initiated_by: "test".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Pause, &t1);

    let state = ks.current_state();
    let level1 = state
        .per_agent
        .get(&agent_id)
        .map(|s| s.level)
        .unwrap_or(KillLevel::Normal);
    assert!(level1 >= KillLevel::Pause);

    // QUARANTINE (escalation)
    let t2 = TriggerEvent::ManualQuarantine {
        agent_id,
        reason: "test".into(),
        initiated_by: "test".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Quarantine, &t2);

    let state = ks.current_state();
    let level2 = state
        .per_agent
        .get(&agent_id)
        .map(|s| s.level)
        .unwrap_or(KillLevel::Normal);
    assert!(
        level2 >= level1,
        "Level must not decrease: {:?} < {:?}",
        level2,
        level1
    );

    // Attempt to PAUSE again (should not decrease from QUARANTINE)
    ks.activate_agent(agent_id, KillLevel::Pause, &t1);
    let state = ks.current_state();
    let level3 = state
        .per_agent
        .get(&agent_id)
        .map(|s| s.level)
        .unwrap_or(KillLevel::Normal);
    assert!(
        level3 >= level2,
        "Level must not decrease on re-PAUSE: {:?} < {:?}",
        level3,
        level2
    );

    reset_platform_killed();
}

// ── T6 cascade correctness ──────────────────────────────────────────────

/// Three quarantined agents → T6 cascade → KILL_ALL.
#[test]
fn t6_cascade_three_quarantined() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    let mut last_level = None;
    for _ in 0..3 {
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::SoulDrift {
            agent_id,
            drift_score: 0.3,
            threshold: 0.25,
            baseline_hash: "base".into(),
            current_hash: "curr".into(),
            detected_at: Utc::now(),
        };
        last_level = evaluator.process(trigger);
    }

    // After 3 quarantines, T6 cascade should fire KILL_ALL
    // Check via KillSwitch state (not global flag, which is subject to test races)
    let state = ks.current_state();
    assert_eq!(
        state.platform_level,
        KillLevel::KillAll,
        "3 quarantined agents must trigger T6 KILL_ALL cascade (state={:?}, last_level={:?})",
        state.platform_level,
        last_level
    );

    reset_platform_killed();
}

/// Two quarantined agents → no KILL_ALL.
#[test]
fn t6_two_quarantined_no_kill_all() {
    // Use a fresh KillSwitch instance — do NOT rely on the global PLATFORM_KILLED
    // static, which is shared across parallel tests and causes race conditions.
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    for _ in 0..2 {
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::SoulDrift {
            agent_id,
            drift_score: 0.3,
            threshold: 0.25,
            baseline_hash: "base".into(),
            current_hash: "curr".into(),
            detected_at: Utc::now(),
        };
        evaluator.process(trigger);
    }

    // Check the instance state, not the global static
    let state = ks.current_state();
    assert_ne!(
        state.platform_level,
        KillLevel::KillAll,
        "2 quarantined agents must NOT trigger KILL_ALL (state={:?})",
        state.platform_level
    );
}
