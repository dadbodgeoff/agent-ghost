//! E2E: Multi-agent scenarios.
//!
//! Validates agent isolation under convergence pressure and T6 KILL_ALL cascade.

use std::sync::Arc;

use chrono::Utc;
use cortex_core::safety::trigger::TriggerEvent;
use ghost_gateway::safety::auto_triggers::AutoTriggerEvaluator;
use ghost_gateway::safety::kill_switch::{KillCheckResult, KillLevel, KillSwitch, PLATFORM_KILLED};
use uuid::Uuid;

fn reset_platform_killed() {
    PLATFORM_KILLED.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// 3 agents, one hits convergence L3 → only that agent is affected.
#[test]
fn convergence_l3_isolates_single_agent() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    let agent_a = Uuid::now_v7();
    let agent_b = Uuid::now_v7();
    let agent_c = Uuid::now_v7();

    // Agent A hits soul drift → quarantined
    let trigger = TriggerEvent::SoulDrift {
        agent_id: agent_a,
        drift_score: 0.30,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    };
    evaluator.process(trigger);

    // Agent A is quarantined
    assert!(matches!(
        ks.check(agent_a),
        KillCheckResult::AgentQuarantined(_)
    ));

    // Agents B and C are unaffected
    assert!(matches!(ks.check(agent_b), KillCheckResult::Ok));
    assert!(matches!(ks.check(agent_c), KillCheckResult::Ok));
}

/// 3 agents quarantined → T6 KILL_ALL cascade.
#[test]
fn three_quarantined_triggers_kill_all() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

    for agent_id in &agents {
        let trigger = TriggerEvent::SoulDrift {
            agent_id: *agent_id,
            drift_score: 0.30,
            threshold: 0.25,
            baseline_hash: "base".into(),
            current_hash: "curr".into(),
            detected_at: Utc::now(),
        };
        evaluator.process(trigger);
    }

    // All agents should be affected by KILL_ALL
    assert!(PLATFORM_KILLED.load(std::sync::atomic::Ordering::SeqCst));
    for agent_id in &agents {
        assert!(matches!(
            ks.check(*agent_id),
            KillCheckResult::PlatformKilled
        ));
    }

    // Even new agents are blocked
    let new_agent = Uuid::now_v7();
    assert!(matches!(
        ks.check(new_agent),
        KillCheckResult::PlatformKilled
    ));

    reset_platform_killed();
}

/// Mixed kill levels: PAUSE + QUARANTINE on different agents.
#[test]
fn mixed_kill_levels_independent() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    let agent_a = Uuid::now_v7();
    let agent_b = Uuid::now_v7();
    let agent_c = Uuid::now_v7();

    // Agent A: spending cap → PAUSE
    evaluator.process(TriggerEvent::SpendingCapExceeded {
        agent_id: agent_a,
        daily_total: 100.0,
        cap: 50.0,
        overage: 50.0,
        detected_at: Utc::now(),
    });

    // Agent B: soul drift → QUARANTINE
    evaluator.process(TriggerEvent::SoulDrift {
        agent_id: agent_b,
        drift_score: 0.30,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    });

    assert!(matches!(ks.check(agent_a), KillCheckResult::AgentPaused(_)));
    assert!(matches!(
        ks.check(agent_b),
        KillCheckResult::AgentQuarantined(_)
    ));
    assert!(matches!(ks.check(agent_c), KillCheckResult::Ok));
}

/// PAUSE then QUARANTINE same agent → quarantine supersedes.
#[test]
fn quarantine_supersedes_pause() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));
    let agent_id = Uuid::now_v7();

    // First: PAUSE
    evaluator.process(TriggerEvent::SpendingCapExceeded {
        agent_id,
        daily_total: 100.0,
        cap: 50.0,
        overage: 50.0,
        detected_at: Utc::now(),
    });
    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::AgentPaused(_)
    ));

    // Then: QUARANTINE (escalation)
    evaluator.process(TriggerEvent::SoulDrift {
        agent_id,
        drift_score: 0.30,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    });
    assert!(matches!(
        ks.check(agent_id),
        KillCheckResult::AgentQuarantined(_)
    ));
}

/// Audit entries match trigger events (completeness).
#[test]
fn audit_entries_match_triggers() {
    reset_platform_killed();
    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(Arc::clone(&ks));

    let triggers = vec![
        TriggerEvent::SpendingCapExceeded {
            agent_id: Uuid::now_v7(),
            daily_total: 100.0,
            cap: 50.0,
            overage: 50.0,
            detected_at: Utc::now(),
        },
        TriggerEvent::SoulDrift {
            agent_id: Uuid::now_v7(),
            drift_score: 0.30,
            threshold: 0.25,
            baseline_hash: "base".into(),
            current_hash: "curr".into(),
            detected_at: Utc::now(),
        },
    ];

    for trigger in &triggers {
        evaluator.process(trigger.clone());
    }

    let entries = ks.audit_entries();
    assert_eq!(
        entries.len(),
        triggers.len(),
        "Audit entries should match trigger count"
    );
}
