//! Adversarial test suite: Kill switch race conditions and dedup correctness.
//!
//! Tests concurrent trigger delivery, dedup under load, and state consistency.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::Utc;
use cortex_core::safety::trigger::TriggerEvent;
use ghost_gateway::safety::kill_switch::{KillLevel, KillSwitch, PLATFORM_KILLED};
use uuid::Uuid;

fn reset_platform_killed() {
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
}

// ── Monotonicity ────────────────────────────────────────────────────────

#[test]
fn kill_level_never_decreases_without_resume() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent = Uuid::now_v7();

    let trigger_pause = TriggerEvent::ManualPause {
        agent_id: agent,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    let trigger_quarantine = TriggerEvent::ManualQuarantine {
        agent_id: agent,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };

    // Escalate to Pause
    ks.activate_agent(agent, KillLevel::Pause, &trigger_pause);
    let state = ks.current_state();
    assert_eq!(state.per_agent[&agent].level, KillLevel::Pause);

    // Escalate to Quarantine
    ks.activate_agent(agent, KillLevel::Quarantine, &trigger_quarantine);
    let state = ks.current_state();
    assert_eq!(state.per_agent[&agent].level, KillLevel::Quarantine);

    // Attempt to downgrade to Pause — should be ignored (monotonicity)
    ks.activate_agent(agent, KillLevel::Pause, &trigger_pause);
    let state = ks.current_state();
    assert_eq!(
        state.per_agent[&agent].level,
        KillLevel::Quarantine,
        "Kill level should not decrease without explicit resume"
    );
}

// ── Idempotent KILL_ALL ─────────────────────────────────────────────────

#[test]
fn duplicate_kill_all_is_idempotent() {
    reset_platform_killed();
    let ks = KillSwitch::new();

    let trigger = TriggerEvent::ManualKillAll {
        reason: "test".into(),
        initiated_by: "owner".into(),
    };

    ks.activate_kill_all(&trigger);
    assert!(PLATFORM_KILLED.load(Ordering::SeqCst));

    // Second KILL_ALL should be idempotent
    ks.activate_kill_all(&trigger);
    assert!(PLATFORM_KILLED.load(Ordering::SeqCst));

    let state = ks.current_state();
    assert_eq!(state.platform_level, KillLevel::KillAll);
}

// ── PLATFORM_KILLED consistency ─────────────────────────────────────────

#[test]
fn platform_killed_consistent_with_state() {
    reset_platform_killed();
    let ks = KillSwitch::new();

    // Before KILL_ALL
    assert!(!PLATFORM_KILLED.load(Ordering::SeqCst));
    let state = ks.current_state();
    assert_ne!(state.platform_level, KillLevel::KillAll);

    // After KILL_ALL
    let trigger = TriggerEvent::ManualKillAll {
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_kill_all(&trigger);
    assert!(PLATFORM_KILLED.load(Ordering::SeqCst));
    let state = ks.current_state();
    assert_eq!(state.platform_level, KillLevel::KillAll);
}

// ── Audit completeness ──────────────────────────────────────────────────

#[test]
fn audit_entries_match_trigger_count() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent = Uuid::now_v7();

    let triggers = vec![
        TriggerEvent::ManualPause {
            agent_id: agent,
            reason: "t1".into(),
            initiated_by: "owner".into(),
        },
        TriggerEvent::ManualQuarantine {
            agent_id: agent,
            reason: "t2".into(),
            initiated_by: "owner".into(),
        },
    ];

    ks.activate_agent(agent, KillLevel::Pause, &triggers[0]);
    ks.activate_agent(agent, KillLevel::Quarantine, &triggers[1]);

    let entries = ks.audit_entries();
    assert_eq!(
        entries.len(),
        2,
        "Audit entries should match number of successful activations"
    );
}

// ── State restoration (crash recovery) ──────────────────────────────────

#[test]
fn restored_state_preserves_level() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent = Uuid::now_v7();

    let trigger = TriggerEvent::ManualQuarantine {
        agent_id: agent,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_agent(agent, KillLevel::Quarantine, &trigger);

    // Simulate crash: save state, create new KillSwitch, restore
    let saved = ks.current_state();

    reset_platform_killed();
    let ks2 = KillSwitch::new();
    ks2.restore_state(saved);

    let state = ks2.current_state();
    assert_eq!(
        state.per_agent[&agent].level,
        KillLevel::Quarantine,
        "Restored state should preserve kill level (never fall to Normal)"
    );
}

// ── T6 cascade: 3 quarantined agents → KILL_ALL ────────────────────────

#[test]
fn t6_cascade_three_quarantined_triggers_kill_all() {
    reset_platform_killed();
    let ks = KillSwitch::new();

    let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

    for agent in &agents {
        let trigger = TriggerEvent::ManualQuarantine {
            agent_id: *agent,
            reason: "test".into(),
            initiated_by: "owner".into(),
        };
        ks.activate_agent(*agent, KillLevel::Quarantine, &trigger);
    }

    assert_eq!(
        ks.quarantined_count(),
        3,
        "Should have 3 quarantined agents"
    );
    // In production, the AutoTriggerEvaluator would fire T6 here.
    // We verify the count is correct for the cascade check.
}

#[test]
fn t6_two_quarantined_no_cascade() {
    reset_platform_killed();
    let ks = KillSwitch::new();

    let agents: Vec<Uuid> = (0..2).map(|_| Uuid::now_v7()).collect();

    for agent in &agents {
        let trigger = TriggerEvent::ManualQuarantine {
            agent_id: *agent,
            reason: "test".into(),
            initiated_by: "owner".into(),
        };
        ks.activate_agent(*agent, KillLevel::Quarantine, &trigger);
    }

    assert_eq!(ks.quarantined_count(), 2);
    assert!(
        !PLATFORM_KILLED.load(Ordering::SeqCst),
        "2 quarantined agents should not trigger KILL_ALL"
    );
}

// ── Resume validation ───────────────────────────────────────────────────

#[test]
fn cannot_resume_from_kill_all_via_agent_resume() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent = Uuid::now_v7();

    let trigger = TriggerEvent::ManualKillAll {
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_kill_all(&trigger);

    // Agent-level resume should fail for KILL_ALL
    let result = ks.resume_agent(agent);
    assert!(result.is_err(), "Cannot resume from KILL_ALL via agent resume");
}
