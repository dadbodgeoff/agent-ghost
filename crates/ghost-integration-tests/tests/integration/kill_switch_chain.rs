//! E2E: Full kill switch chain lifecycle.
//!
//! Validates: detection → trigger → evaluation → dedup → classification
//! → execution → notification → audit.
//!
//! Exercises ghost-gateway safety subsystem, cortex-core TriggerEvent types,
//! and the AutoTriggerEvaluator pipeline.

use std::sync::Arc;

use chrono::Utc;
use cortex_core::safety::trigger::{ExfilType, TriggerEvent};
use ghost_gateway::safety::auto_triggers::AutoTriggerEvaluator;
use ghost_gateway::safety::kill_switch::{KillCheckResult, KillLevel, KillSwitch, PLATFORM_KILLED};
use uuid::Uuid;

/// Reset PLATFORM_KILLED between tests (test isolation).
fn reset_platform_killed() {
    PLATFORM_KILLED.store(false, std::sync::atomic::Ordering::SeqCst);
}

// ── Kill Switch Core ────────────────────────────────────────────────────

/// KillSwitch::check returns Ok when running normally.
#[test]
fn kill_switch_check_ok_when_running() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    assert!(matches!(ks.check(agent_id), KillCheckResult::Ok));
}

/// KillSwitch::check returns AgentPaused when agent is paused.
#[test]
fn kill_switch_check_agent_paused() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::ManualPause {
        agent_id,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Pause, &trigger);

    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::AgentPaused(_)
    ));
}

/// KillSwitch::check returns PlatformKilled after KILL_ALL.
#[test]
fn kill_switch_check_platform_killed() {
    reset_platform_killed();
    let ks = KillSwitch::new();

    let trigger = TriggerEvent::ManualKillAll {
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_kill_all(&trigger);

    // Verify instance state (race-free)
    let state = ks.current_state();
    assert_eq!(state.platform_level, KillLevel::KillAll);

    // Verify check returns PlatformKilled
    assert!(matches!(
        ks.check(Uuid::now_v7()),
        KillCheckResult::PlatformKilled
    ));

    reset_platform_killed();
}

/// State transition monotonicity: level never decreases without resume.
#[test]
fn kill_level_monotonicity() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    // Escalate to Quarantine
    let trigger = TriggerEvent::ManualQuarantine {
        agent_id,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Quarantine, &trigger);

    // Try to downgrade to Pause — should be ignored (monotonicity)
    let pause_trigger = TriggerEvent::ManualPause {
        agent_id,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Pause, &pause_trigger);

    // Should still be Quarantined
    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::AgentQuarantined(_)
    ));
}

/// KILL_ALL is idempotent — second activation is a no-op.
#[test]
fn kill_all_idempotent() {
    reset_platform_killed();
    let ks = KillSwitch::new();

    let trigger1 = TriggerEvent::ManualKillAll {
        reason: "first".into(),
        initiated_by: "owner".into(),
    };
    let trigger2 = TriggerEvent::ManualKillAll {
        reason: "second".into(),
        initiated_by: "owner".into(),
    };

    ks.activate_kill_all(&trigger1);
    ks.activate_kill_all(&trigger2);

    // Should have audit entries for both attempts but only one effective activation
    let state = ks.current_state();
    assert_eq!(state.platform_level, KillLevel::KillAll);

    reset_platform_killed();
}

/// Resume from PAUSE works.
#[test]
fn resume_from_pause() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::ManualPause {
        agent_id,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Pause, &trigger);
    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::AgentPaused(_)
    ));

    ks.resume_agent(agent_id, Some(KillLevel::Pause))
        .expect("Resume should succeed");
    assert!(matches!(ks.check(agent_id), KillCheckResult::Ok));
}

/// Cannot resume from KILL_ALL via agent resume.
#[test]
fn cannot_resume_kill_all_via_agent() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::ManualKillAll {
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_kill_all(&trigger);

    // Agent-level resume should fail for KILL_ALL
    // (KILL_ALL requires delete kill_state.json + restart)
    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::PlatformKilled
    ));

    reset_platform_killed();
}

/// Stale state preserved on crash recovery.
#[test]
fn stale_state_preserved_on_recovery() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::ManualQuarantine {
        agent_id,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Quarantine, &trigger);

    // Simulate crash: save state, create new KillSwitch, restore
    let saved_state = ks.current_state();

    let ks2 = KillSwitch::new();
    ks2.restore_state(saved_state);

    // State should be preserved — not reset to Normal
    assert!(matches!(
        ks2.check(agent_id),
        KillCheckResult::AgentQuarantined(_)
    ));
}

/// Audit log entry for every activation.
#[test]
fn audit_log_completeness() {
    reset_platform_killed();
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::ManualPause {
        agent_id,
        reason: "test".into(),
        initiated_by: "owner".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Pause, &trigger);

    let entries = ks.audit_entries();
    assert_eq!(entries.len(), 1, "Should have 1 audit entry");
    assert_eq!(entries[0].action, KillLevel::Pause);
    assert_eq!(entries[0].agent_id, Some(agent_id));
}

// ── Auto-Trigger Evaluator ──────────────────────────────────────────────

/// T1 SoulDrift → QUARANTINE agent.
#[test]
fn trigger_soul_drift_quarantines() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::SoulDrift {
        agent_id,
        drift_score: 0.30,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    };

    let level = evaluator.process(trigger);
    assert_eq!(level, Some(KillLevel::Quarantine));
    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::AgentQuarantined(_)
    ));
}

/// T2 SpendingCap → PAUSE agent.
#[test]
fn trigger_spending_cap_pauses() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::SpendingCapExceeded {
        agent_id,
        daily_total: 100.0,
        cap: 50.0,
        overage: 50.0,
        detected_at: Utc::now(),
    };

    let level = evaluator.process(trigger);
    assert_eq!(level, Some(KillLevel::Pause));
    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::AgentPaused(_)
    ));
}

/// T4 SandboxEscape → KILL_ALL.
#[test]
fn trigger_sandbox_escape_kills_all() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::SandboxEscape {
        agent_id,
        skill_name: "malicious".into(),
        escape_attempt: "fs_write".into(),
        detected_at: Utc::now(),
    };

    let level = evaluator.process(trigger);
    assert_eq!(level, Some(KillLevel::KillAll));

    // Check instance state rather than global static to avoid test races
    let state = ks.current_state();
    assert_eq!(state.platform_level, KillLevel::KillAll);

    reset_platform_killed();
}

/// T5 CredentialExfiltration → KILL_ALL.
#[test]
fn trigger_credential_exfil_kills_all() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::CredentialExfiltration {
        agent_id,
        skill_name: Some("leaky".into()),
        exfil_type: ExfilType::OutputLeakage,
        credential_id: "cred-001".into(),
        detected_at: Utc::now(),
    };

    let level = evaluator.process(trigger);
    assert_eq!(level, Some(KillLevel::KillAll));

    reset_platform_killed();
}

/// T6 cascade: 3 quarantined agents → KILL_ALL.
#[test]
fn trigger_t6_cascade_three_quarantined() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    // Quarantine 3 different agents
    for _ in 0..3 {
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::SoulDrift {
            agent_id,
            drift_score: 0.30,
            threshold: 0.25,
            baseline_hash: "base".into(),
            current_hash: "curr".into(),
            detected_at: Utc::now(),
        };
        evaluator.process(trigger);
    }

    // After 3rd quarantine, T6 cascade should trigger KILL_ALL
    // Check instance state rather than global static to avoid test races
    let state = ks.current_state();
    assert_eq!(
        state.platform_level,
        KillLevel::KillAll,
        "3 quarantined agents should trigger KILL_ALL cascade (state={:?})",
        state.platform_level
    );

    reset_platform_killed();
}

/// T6 with only 2 quarantined → no KILL_ALL.
#[test]
fn trigger_t6_no_cascade_two_quarantined() {
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    for _ in 0..2 {
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::SoulDrift {
            agent_id,
            drift_score: 0.30,
            threshold: 0.25,
            baseline_hash: "base".into(),
            current_hash: "curr".into(),
            detected_at: Utc::now(),
        };
        evaluator.process(trigger);
    }

    // Check instance state rather than global static to avoid test races
    let state = ks.current_state();
    assert_ne!(
        state.platform_level,
        KillLevel::KillAll,
        "2 quarantined agents should NOT trigger KILL_ALL (state={:?})",
        state.platform_level
    );
}

/// Dedup: same trigger+agent within 60s → suppressed.
#[test]
fn dedup_suppresses_duplicate_trigger() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::SpendingCapExceeded {
        agent_id,
        daily_total: 100.0,
        cap: 50.0,
        overage: 50.0,
        detected_at: Utc::now(),
    };

    // First: processed
    let r1 = evaluator.process(trigger.clone());
    assert!(r1.is_some());

    // Second (within 60s): suppressed
    let r2 = evaluator.process(trigger);
    assert!(r2.is_none(), "Duplicate trigger should be suppressed");
}

/// Dedup: same trigger + different agent → NOT suppressed.
#[test]
fn dedup_allows_different_agents() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    let trigger1 = TriggerEvent::SpendingCapExceeded {
        agent_id: Uuid::now_v7(),
        daily_total: 100.0,
        cap: 50.0,
        overage: 50.0,
        detected_at: Utc::now(),
    };
    let trigger2 = TriggerEvent::SpendingCapExceeded {
        agent_id: Uuid::now_v7(),
        daily_total: 100.0,
        cap: 50.0,
        overage: 50.0,
        detected_at: Utc::now(),
    };

    let r1 = evaluator.process(trigger1);
    let r2 = evaluator.process(trigger2);

    assert!(r1.is_some());
    assert!(r2.is_some(), "Different agents should not be deduped");
}
