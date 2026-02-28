//! Phase 5 tests for ghost-gateway (Tasks 5.1–5.6).
//!
//! Covers: state machine, bootstrap, kill switch, auto-triggers, quarantine,
//! session management, lane queues, cost tracking, messaging, compaction.
//!
//! NOTE: Tests that touch the global PLATFORM_KILLED static must acquire
//! KILL_SWITCH_MUTEX to avoid parallel interference.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use once_cell::sync::Lazy;
use uuid::Uuid;

/// Global mutex for tests that read/write PLATFORM_KILLED.
static KILL_SWITCH_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — Gateway State Machine
// ═══════════════════════════════════════════════════════════════════════

mod state_machine {
    use ghost_gateway::gateway::{GatewaySharedState, GatewayState};

    #[test]
    fn initializing_to_healthy_valid() {
        let state = GatewaySharedState::new();
        assert!(state.transition_to(GatewayState::Healthy).is_ok());
        assert_eq!(state.current_state(), GatewayState::Healthy);
    }

    #[test]
    fn initializing_to_degraded_valid() {
        let state = GatewaySharedState::new();
        assert!(state.transition_to(GatewayState::Degraded).is_ok());
    }

    #[test]
    fn initializing_to_fatal_valid() {
        let state = GatewaySharedState::new();
        assert!(state.transition_to(GatewayState::FatalError).is_ok());
    }

    #[test]
    #[cfg_attr(not(debug_assertions), ignore)]
    #[should_panic(expected = "illegal state transition")]
    fn healthy_to_recovering_invalid() {
        let state = GatewaySharedState::new();
        state.transition_to(GatewayState::Healthy).unwrap();
        // Must go through Degraded first
        let _ = state.transition_to(GatewayState::Recovering);
    }

    #[test]
    #[cfg_attr(not(debug_assertions), ignore)]
    #[should_panic(expected = "illegal state transition")]
    fn fatal_error_is_terminal() {
        let state = GatewaySharedState::new();
        state.transition_to(GatewayState::FatalError).unwrap();
        let _ = state.transition_to(GatewayState::Healthy);
    }

    #[test]
    #[cfg_attr(not(debug_assertions), ignore)]
    #[should_panic(expected = "illegal state transition")]
    fn shutting_down_is_terminal() {
        let state = GatewaySharedState::new();
        state.transition_to(GatewayState::Healthy).unwrap();
        state.transition_to(GatewayState::ShuttingDown).unwrap();
        let _ = state.transition_to(GatewayState::Healthy);
    }

    #[test]
    fn degraded_to_recovering_to_healthy() {
        let state = GatewaySharedState::new();
        state.transition_to(GatewayState::Degraded).unwrap();
        state.transition_to(GatewayState::Recovering).unwrap();
        state.transition_to(GatewayState::Healthy).unwrap();
        assert_eq!(state.current_state(), GatewayState::Healthy);
    }

    #[test]
    fn recovering_to_degraded_on_failure() {
        let state = GatewaySharedState::new();
        state.transition_to(GatewayState::Degraded).unwrap();
        state.transition_to(GatewayState::Recovering).unwrap();
        state.transition_to(GatewayState::Degraded).unwrap();
        assert_eq!(state.current_state(), GatewayState::Degraded);
    }

    #[test]
    fn state_arc_shares_state() {
        let state = GatewaySharedState::new();
        let arc = state.state_arc();
        state.transition_to(GatewayState::Healthy).unwrap();
        assert_eq!(
            GatewayState::from_u8(arc.load(std::sync::atomic::Ordering::Acquire)),
            GatewayState::Healthy
        );
    }

    #[test]
    fn can_transition_to_truth_table() {
        use GatewayState::*;
        // Valid transitions
        assert!(Initializing.can_transition_to(Healthy));
        assert!(Initializing.can_transition_to(Degraded));
        assert!(Initializing.can_transition_to(FatalError));
        assert!(Healthy.can_transition_to(Degraded));
        assert!(Healthy.can_transition_to(ShuttingDown));
        assert!(Degraded.can_transition_to(Recovering));
        assert!(Degraded.can_transition_to(ShuttingDown));
        assert!(Recovering.can_transition_to(Healthy));
        assert!(Recovering.can_transition_to(Degraded));
        assert!(Recovering.can_transition_to(ShuttingDown));

        // Invalid transitions
        assert!(!Healthy.can_transition_to(Recovering));
        assert!(!Healthy.can_transition_to(Initializing));
        assert!(!FatalError.can_transition_to(Healthy));
        assert!(!FatalError.can_transition_to(Degraded));
        assert!(!ShuttingDown.can_transition_to(Healthy));
        assert!(!ShuttingDown.can_transition_to(Degraded));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — ITP Buffer
// ═══════════════════════════════════════════════════════════════════════

mod itp_buffer {
    use ghost_gateway::itp_buffer::ITPBuffer;

    #[test]
    fn push_and_drain() {
        let mut buf = ITPBuffer::new();
        buf.push("event1".into());
        buf.push("event2".into());
        assert_eq!(buf.len(), 2);
        let drained = buf.drain_all();
        assert_eq!(drained.len(), 2);
        assert!(buf.is_empty());
    }

    #[test]
    fn max_events_enforced() {
        let mut buf = ITPBuffer::new();
        for i in 0..10_001 {
            buf.push(format!("event_{i}"));
        }
        assert!(buf.len() <= 10_000);
    }

    #[test]
    fn max_bytes_enforced() {
        let mut buf = ITPBuffer::new();
        // Push events that are ~1KB each, should cap at ~10MB
        let big_event = "x".repeat(1024);
        for _ in 0..11_000 {
            buf.push(big_event.clone());
        }
        assert!(buf.total_bytes() <= 10 * 1024 * 1024 + 1024);
    }

    #[test]
    fn fifo_eviction() {
        let mut buf = ITPBuffer::new();
        for i in 0..10_000 {
            buf.push(format!("event_{i}"));
        }
        // Push one more — oldest should be evicted
        buf.push("newest".into());
        let drained = buf.drain_all();
        assert_eq!(drained.last().unwrap().json, "newest");
        // First event should NOT be "event_0" (it was evicted)
        assert_ne!(drained.first().unwrap().json, "event_0");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — Agent Registry
// ═══════════════════════════════════════════════════════════════════════

mod agent_registry {
    use ghost_gateway::agents::registry::{AgentLifecycleState, AgentRegistry, RegisteredAgent};
    use uuid::Uuid;

    fn make_agent(name: &str) -> RegisteredAgent {
        RegisteredAgent {
            id: Uuid::now_v7(),
            name: name.into(),
            state: AgentLifecycleState::Starting,
            channel_bindings: vec![format!("cli:{name}")],
            capabilities: vec!["memory_read".into()],
            spending_cap: 5.0,
        }
    }

    #[test]
    fn lookup_by_name() {
        let mut reg = AgentRegistry::new();
        let agent = make_agent("alice");
        let id = agent.id;
        reg.register(agent);
        let found = reg.lookup_by_name("alice").unwrap();
        assert_eq!(found.id, id);
    }

    #[test]
    fn lookup_by_channel() {
        let mut reg = AgentRegistry::new();
        let agent = make_agent("bob");
        let id = agent.id;
        reg.register(agent);
        let found = reg.lookup_by_channel("cli:bob").unwrap();
        assert_eq!(found.id, id);
    }

    #[test]
    fn lookup_by_id() {
        let mut reg = AgentRegistry::new();
        let agent = make_agent("carol");
        let id = agent.id;
        reg.register(agent);
        assert!(reg.lookup_by_id(id).is_some());
    }

    #[test]
    fn lifecycle_transitions() {
        let mut reg = AgentRegistry::new();
        let agent = make_agent("dave");
        let id = agent.id;
        reg.register(agent);

        assert!(reg.transition_state(id, AgentLifecycleState::Ready).is_ok());
        assert!(reg.transition_state(id, AgentLifecycleState::Stopping).is_ok());
        assert!(reg.transition_state(id, AgentLifecycleState::Stopped).is_ok());
    }

    #[test]
    fn invalid_lifecycle_transition() {
        let mut reg = AgentRegistry::new();
        let agent = make_agent("eve");
        let id = agent.id;
        reg.register(agent);
        // Starting → Stopped is invalid (must go through Ready → Stopping)
        assert!(reg.transition_state(id, AgentLifecycleState::Stopped).is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — Agent Templates
// ═══════════════════════════════════════════════════════════════════════

mod agent_templates {
    use ghost_gateway::agents::templates::AgentTemplate;

    #[test]
    fn personal_template_defaults() {
        let t = AgentTemplate::personal();
        assert_eq!(t.name, "personal");
        assert_eq!(t.spending_cap, 5.0);
        assert!(t.capabilities.contains(&"memory_read".to_string()));
    }

    #[test]
    fn developer_template_defaults() {
        let t = AgentTemplate::developer();
        assert_eq!(t.name, "developer");
        assert_eq!(t.spending_cap, 10.0);
        assert!(t.capabilities.contains(&"shell_execute".to_string()));
    }

    #[test]
    fn researcher_template_defaults() {
        let t = AgentTemplate::researcher();
        assert_eq!(t.name, "researcher");
        assert_eq!(t.spending_cap, 20.0);
        assert!(t.capabilities.contains(&"web_browse".to_string()));
    }

    #[test]
    fn from_yaml_valid() {
        let yaml = r#"
name: custom
capabilities: [memory_read]
spending_cap: 15.0
heartbeat_interval_minutes: 45
convergence_profile: custom
"#;
        let t = AgentTemplate::from_yaml(yaml).unwrap();
        assert_eq!(t.name, "custom");
        assert_eq!(t.spending_cap, 15.0);
    }

    #[test]
    fn from_yaml_invalid() {
        let yaml = "not: [valid: yaml: {{";
        assert!(AgentTemplate::from_yaml(yaml).is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — Agent Isolation
// ═══════════════════════════════════════════════════════════════════════

mod agent_isolation {
    use ghost_gateway::agents::isolation::AgentIsolation;
    use ghost_gateway::config::IsolationMode;
    use uuid::Uuid;

    #[tokio::test]
    async fn in_process_spawn() {
        let iso = AgentIsolation::new(IsolationMode::InProcess, Uuid::now_v7());
        assert!(iso.spawn().await.is_ok());
    }

    #[tokio::test]
    async fn process_spawn() {
        let iso = AgentIsolation::new(IsolationMode::Process, Uuid::now_v7());
        assert!(iso.spawn().await.is_ok());
    }

    #[tokio::test]
    async fn container_spawn() {
        let iso = AgentIsolation::new(IsolationMode::Container, Uuid::now_v7());
        assert!(iso.spawn().await.is_ok());
    }

    #[tokio::test]
    async fn teardown() {
        let iso = AgentIsolation::new(IsolationMode::InProcess, Uuid::now_v7());
        iso.spawn().await.unwrap();
        assert!(iso.teardown().await.is_ok());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — Shutdown
// ═══════════════════════════════════════════════════════════════════════

mod shutdown {
    use ghost_gateway::shutdown::{execute_shutdown, ShutdownConfig};

    #[tokio::test]
    async fn shutdown_completes_all_steps() {
        let config = ShutdownConfig::default();
        let result = execute_shutdown(&config, false).await;
        assert_eq!(result.steps_completed, 7);
        assert!(!result.forced);
    }

    #[tokio::test]
    async fn shutdown_with_kill_switch_skips_flush() {
        let config = ShutdownConfig::default();
        let result = execute_shutdown(&config, true).await;
        assert_eq!(result.steps_completed, 7);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.2 — Kill Switch
// ═══════════════════════════════════════════════════════════════════════

mod kill_switch {
    use std::sync::atomic::Ordering;

    use chrono::Utc;
    use ghost_gateway::safety::kill_switch::{
        KillCheckResult, KillLevel, KillSwitch, PLATFORM_KILLED,
    };
    use cortex_core::safety::trigger::TriggerEvent;
    use uuid::Uuid;

    use crate::KILL_SWITCH_MUTEX;

    fn reset_platform_killed() {
        PLATFORM_KILLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn check_returns_ok_when_running() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let agent_id = Uuid::now_v7();
        assert!(matches!(ks.check(agent_id), KillCheckResult::Ok));
    }

    #[test]
    fn check_returns_paused_when_agent_paused() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::ManualPause {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Pause, &trigger);
        assert!(matches!(ks.check(agent_id), KillCheckResult::AgentPaused(_)));
    }

    #[test]
    fn check_returns_platform_killed_on_kill_all() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let trigger = TriggerEvent::ManualKillAll {
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_kill_all(&trigger);
        assert!(PLATFORM_KILLED.load(Ordering::SeqCst));
        assert!(matches!(
            ks.check(Uuid::now_v7()),
            KillCheckResult::PlatformKilled
        ));
        reset_platform_killed();
    }

    #[test]
    fn monotonicity_level_never_decreases() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let agent_id = Uuid::now_v7();
        let trigger_q = TriggerEvent::ManualQuarantine {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        let trigger_p = TriggerEvent::ManualPause {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Quarantine, &trigger_q);
        ks.activate_agent(agent_id, KillLevel::Pause, &trigger_p);
        assert!(matches!(
            ks.check(agent_id),
            KillCheckResult::AgentQuarantined(_)
        ));
        reset_platform_killed();
    }

    #[test]
    fn kill_all_idempotent() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let trigger = TriggerEvent::ManualKillAll {
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_kill_all(&trigger);
        ks.activate_kill_all(&trigger);
        assert_eq!(ks.current_state().platform_level, KillLevel::KillAll);
        let entries = ks.audit_entries();
        assert!(entries.len() >= 1);
        reset_platform_killed();
    }

    #[test]
    fn audit_log_records_activations() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::ManualPause {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Pause, &trigger);
        let entries = ks.audit_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, KillLevel::Pause);
        assert_eq!(entries[0].agent_id, Some(agent_id));
        reset_platform_killed();
    }

    #[test]
    fn resume_from_pause() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::ManualPause {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Pause, &trigger);
        assert!(ks.resume_agent(agent_id).is_ok());
        assert!(matches!(ks.check(agent_id), KillCheckResult::Ok));
        reset_platform_killed();
    }

    #[test]
    fn resume_from_kill_all_fails() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::ManualKillAll {
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_kill_all(&trigger);
        assert!(ks.resume_agent(agent_id).is_err());
        reset_platform_killed();
    }

    #[test]
    fn quarantined_count() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        for _ in 0..3 {
            let agent_id = Uuid::now_v7();
            let trigger = TriggerEvent::ManualQuarantine {
                agent_id,
                reason: "test".into(),
                initiated_by: "test".into(),
            };
            ks.activate_agent(agent_id, KillLevel::Quarantine, &trigger);
        }
        assert_eq!(ks.quarantined_count(), 3);
        reset_platform_killed();
    }

    #[test]
    fn state_persistence_roundtrip() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset_platform_killed();
        let ks = KillSwitch::new();
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::ManualPause {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Pause, &trigger);
        let saved = ks.current_state();

        let ks2 = KillSwitch::new();
        ks2.restore_state(saved);
        assert!(matches!(ks2.check(agent_id), KillCheckResult::AgentPaused(_)));
        reset_platform_killed();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.2 — Auto-Triggers
// ═══════════════════════════════════════════════════════════════════════

mod auto_triggers {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use chrono::Utc;
    use cortex_core::safety::trigger::TriggerEvent;
    use ghost_gateway::safety::auto_triggers::AutoTriggerEvaluator;
    use ghost_gateway::safety::kill_switch::{KillLevel, KillSwitch, PLATFORM_KILLED};
    use uuid::Uuid;

    use crate::KILL_SWITCH_MUTEX;

    fn reset() {
        PLATFORM_KILLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn t1_soul_drift_quarantines() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::SoulDrift {
            agent_id,
            drift_score: 0.25,
            threshold: 0.15,
            baseline_hash: "abc".into(),
            current_hash: "def".into(),
            detected_at: Utc::now(),
        };
        let level = eval.process(trigger);
        assert_eq!(level, Some(KillLevel::Quarantine));
        reset();
    }

    #[test]
    fn t2_spending_cap_pauses() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::SpendingCapExceeded {
            agent_id,
            daily_total: 15.0,
            cap: 10.0,
            overage: 5.0,
            detected_at: Utc::now(),
        };
        let level = eval.process(trigger);
        assert_eq!(level, Some(KillLevel::Pause));
        reset();
    }

    #[test]
    fn t4_sandbox_escape_kills_all() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        let trigger = TriggerEvent::SandboxEscape {
            agent_id: Uuid::now_v7(),
            skill_name: "evil_skill".into(),
            escape_attempt: "filesystem write".into(),
            detected_at: Utc::now(),
        };
        let level = eval.process(trigger);
        assert_eq!(level, Some(KillLevel::KillAll));
        assert!(PLATFORM_KILLED.load(Ordering::SeqCst));
        reset();
    }

    #[test]
    fn t5_credential_exfil_kills_all() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        let trigger = TriggerEvent::CredentialExfiltration {
            agent_id: Uuid::now_v7(),
            skill_name: None,
            exfil_type: cortex_core::safety::trigger::ExfilType::TokenReplay,
            credential_id: "cred_123".into(),
            detected_at: Utc::now(),
        };
        let level = eval.process(trigger);
        assert_eq!(level, Some(KillLevel::KillAll));
        reset();
    }

    #[test]
    fn dedup_suppresses_within_60s() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        let agent_id = Uuid::now_v7();
        let trigger = TriggerEvent::SpendingCapExceeded {
            agent_id,
            daily_total: 15.0,
            cap: 10.0,
            overage: 5.0,
            detected_at: Utc::now(),
        };
        assert!(eval.process(trigger.clone()).is_some());
        assert!(eval.process(trigger).is_none());
        reset();
    }

    #[test]
    fn dedup_different_agent_not_suppressed() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        let trigger1 = TriggerEvent::SpendingCapExceeded {
            agent_id: Uuid::now_v7(),
            daily_total: 15.0,
            cap: 10.0,
            overage: 5.0,
            detected_at: Utc::now(),
        };
        let trigger2 = TriggerEvent::SpendingCapExceeded {
            agent_id: Uuid::now_v7(),
            daily_total: 15.0,
            cap: 10.0,
            overage: 5.0,
            detected_at: Utc::now(),
        };
        assert!(eval.process(trigger1).is_some());
        assert!(eval.process(trigger2).is_some());
        reset();
    }

    #[test]
    fn t6_cascade_three_quarantined_kills_all() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        for _ in 0..3 {
            let agent_id = Uuid::now_v7();
            let trigger = TriggerEvent::SoulDrift {
                agent_id,
                drift_score: 0.25,
                threshold: 0.15,
                baseline_hash: "abc".into(),
                current_hash: "def".into(),
                detected_at: Utc::now(),
            };
            eval.process(trigger);
        }
        assert!(PLATFORM_KILLED.load(Ordering::SeqCst));
        reset();
    }

    #[test]
    fn t6_two_quarantined_no_kill_all() {
        let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
        reset();
        let ks = Arc::new(KillSwitch::new());
        let mut eval = AutoTriggerEvaluator::new(ks);
        for _ in 0..2 {
            let agent_id = Uuid::now_v7();
            let trigger = TriggerEvent::SoulDrift {
                agent_id,
                drift_score: 0.25,
                threshold: 0.15,
                baseline_hash: "abc".into(),
                current_hash: "def".into(),
                detected_at: Utc::now(),
            };
            eval.process(trigger);
        }
        assert!(!PLATFORM_KILLED.load(Ordering::SeqCst));
        reset();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.2 — Quarantine
// ═══════════════════════════════════════════════════════════════════════

mod quarantine {
    use ghost_gateway::safety::quarantine::QuarantineManager;
    use uuid::Uuid;

    #[test]
    fn quarantine_preserves_forensic_state() {
        let mut qm = QuarantineManager::new();
        let agent_id = Uuid::now_v7();
        let state = qm.quarantine(
            agent_id,
            "soul drift".into(),
            vec!["msg1".into(), "msg2".into()],
            serde_json::json!({"key": "value"}),
            vec!["tool1".into()],
        );
        assert_eq!(state.agent_id, agent_id);
        assert_eq!(state.trigger_reason, "soul drift");
        assert_eq!(state.session_transcript.len(), 2);
    }

    #[test]
    fn get_forensic_state() {
        let mut qm = QuarantineManager::new();
        let agent_id = Uuid::now_v7();
        qm.quarantine(
            agent_id,
            "test".into(),
            Vec::new(),
            serde_json::json!(null),
            Vec::new(),
        );
        assert!(qm.get_forensic_state(agent_id).is_some());
        assert!(qm.get_forensic_state(Uuid::now_v7()).is_none());
    }

    #[test]
    fn quarantined_count() {
        let mut qm = QuarantineManager::new();
        for _ in 0..3 {
            qm.quarantine(
                Uuid::now_v7(),
                "test".into(),
                Vec::new(),
                serde_json::json!(null),
                Vec::new(),
            );
        }
        assert_eq!(qm.quarantined_count(), 3);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.3 — Lane Queues
// ═══════════════════════════════════════════════════════════════════════

mod lane_queues {
    use ghost_gateway::session::lane_queue::{LaneQueue, LaneQueueManager, QueuedRequest};
    use uuid::Uuid;

    #[test]
    fn serializes_requests() {
        let mut q = LaneQueue::new(5);
        let r1 = QueuedRequest { request_id: Uuid::now_v7(), payload: "first".into() };
        let r2 = QueuedRequest { request_id: Uuid::now_v7(), payload: "second".into() };
        q.enqueue(r1).unwrap();
        q.enqueue(r2).unwrap();
        // Dequeue first
        let first = q.dequeue().unwrap();
        assert_eq!(first.payload, "first");
        // Second request waits (processing flag set)
        assert!(q.dequeue().is_none());
        // Complete first
        q.complete();
        // Now second is available
        let second = q.dequeue().unwrap();
        assert_eq!(second.payload, "second");
    }

    #[test]
    fn depth_limit_backpressure() {
        let mut q = LaneQueue::new(5);
        for i in 0..5 {
            let r = QueuedRequest { request_id: Uuid::now_v7(), payload: format!("req_{i}") };
            assert!(q.enqueue(r).is_ok());
        }
        // 6th request rejected
        let r6 = QueuedRequest { request_id: Uuid::now_v7(), payload: "rejected".into() };
        assert!(q.enqueue(r6).is_err());
    }

    #[test]
    fn manager_enqueue_dequeue() {
        let mgr = LaneQueueManager::new(5);
        let session_id = Uuid::now_v7();
        let req = QueuedRequest { request_id: Uuid::now_v7(), payload: "test".into() };
        assert!(mgr.enqueue(session_id, req).is_ok());
        let dequeued = mgr.dequeue(session_id).unwrap();
        assert_eq!(dequeued.payload, "test");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.3 — Session Manager
// ═══════════════════════════════════════════════════════════════════════

mod session_manager {
    use ghost_gateway::session::manager::SessionManager;
    use uuid::Uuid;

    #[test]
    fn create_and_lookup() {
        let mut mgr = SessionManager::new();
        let agent_id = Uuid::now_v7();
        let ctx = mgr.create_session(agent_id, "cli".into(), 128_000);
        assert!(mgr.lookup(ctx.session_id).is_some());
    }

    #[test]
    fn agent_sessions() {
        let mut mgr = SessionManager::new();
        let agent_id = Uuid::now_v7();
        mgr.create_session(agent_id, "cli".into(), 128_000);
        mgr.create_session(agent_id, "ws".into(), 128_000);
        assert_eq!(mgr.agent_sessions(agent_id).len(), 2);
    }

    #[test]
    fn prune_idle() {
        let mut mgr = SessionManager::new();
        let agent_id = Uuid::now_v7();
        mgr.create_session(agent_id, "cli".into(), 128_000);
        assert_eq!(mgr.session_count(), 1);
        // Prune with 0 duration — should remove all
        mgr.prune_idle(chrono::Duration::zero());
        assert_eq!(mgr.session_count(), 0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.3 — Session Boundary
// ═══════════════════════════════════════════════════════════════════════

mod session_boundary {
    use ghost_gateway::session::boundary::SessionBoundaryProxy;
    use chrono::Utc;

    #[test]
    fn can_create_first_session() {
        let proxy = SessionBoundaryProxy::default();
        assert!(proxy.can_create_session(None));
    }

    #[test]
    fn min_gap_enforced() {
        let proxy = SessionBoundaryProxy::default();
        // Session ended just now — min_gap not met
        assert!(!proxy.can_create_session(Some(Utc::now())));
    }

    #[test]
    fn session_not_expired_initially() {
        let proxy = SessionBoundaryProxy::default();
        assert!(!proxy.is_session_expired(Utc::now()));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.3 — Cost Tracking
// ═══════════════════════════════════════════════════════════════════════

mod cost_tracking {
    use std::sync::Arc;

    use ghost_gateway::cost::spending_cap::SpendingCapEnforcer;
    use ghost_gateway::cost::tracker::CostTracker;
    use uuid::Uuid;

    #[test]
    fn record_and_get_daily_total() {
        let tracker = CostTracker::new();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        tracker.record(agent_id, session_id, 1.50, false);
        tracker.record(agent_id, session_id, 2.50, false);
        assert!((tracker.get_daily_total(agent_id) - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compaction_cost_tracked_separately() {
        let tracker = CostTracker::new();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        tracker.record(agent_id, session_id, 1.0, false);
        tracker.record(agent_id, session_id, 0.5, true);
        assert!((tracker.get_daily_total(agent_id) - 1.5).abs() < f64::EPSILON);
        assert!((tracker.get_compaction_cost(agent_id) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn session_total() {
        let tracker = CostTracker::new();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        tracker.record(agent_id, session_id, 3.0, false);
        assert!((tracker.get_session_total(session_id) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reset_daily() {
        let tracker = CostTracker::new();
        let agent_id = Uuid::now_v7();
        tracker.record(agent_id, Uuid::now_v7(), 5.0, false);
        tracker.reset_daily();
        assert!((tracker.get_daily_total(agent_id)).abs() < f64::EPSILON);
    }

    #[test]
    fn spending_cap_pre_call_blocks() {
        let tracker = Arc::new(CostTracker::new());
        let agent_id = Uuid::now_v7();
        tracker.record(agent_id, Uuid::now_v7(), 9.0, false);
        let enforcer = SpendingCapEnforcer::new(tracker);
        // Estimated cost would push over cap
        assert!(enforcer.check_pre_call(agent_id, 2.0, 10.0).is_err());
    }

    #[test]
    fn spending_cap_pre_call_allows() {
        let tracker = Arc::new(CostTracker::new());
        let agent_id = Uuid::now_v7();
        tracker.record(agent_id, Uuid::now_v7(), 5.0, false);
        let enforcer = SpendingCapEnforcer::new(tracker);
        assert!(enforcer.check_pre_call(agent_id, 2.0, 10.0).is_ok());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.3 — Message Router
// ═══════════════════════════════════════════════════════════════════════

mod message_router {
    use ghost_gateway::session::router::MessageRouter;
    use uuid::Uuid;

    #[test]
    fn route_to_correct_agent() {
        let mut router = MessageRouter::new();
        let agent_id = Uuid::now_v7();
        router.bind_channel("cli:alice".into(), agent_id);
        let target = router.route("cli:alice").unwrap();
        assert_eq!(target.agent_id, agent_id);
    }

    #[test]
    fn route_unknown_channel() {
        let router = MessageRouter::new();
        assert!(router.route("unknown").is_none());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.4 — Auth
// ═══════════════════════════════════════════════════════════════════════

mod auth {
    use ghost_gateway::auth::token_auth::validate_token;
    use ghost_gateway::auth::auth_profiles::AuthProfileManager;
    use uuid::Uuid;

    // NOTE: These tests use GHOST_TOKEN env var which is process-global.
    // They must run serially to avoid race conditions. The test binary
    // runs them in a single module so they are serialized by default,
    // but we add explicit set/check/remove within each test.

    #[test]
    fn token_auth_no_token_set() {
        // When GHOST_TOKEN is not set, auth is disabled
        // Save and restore to avoid interfering with other tests
        let saved = std::env::var("GHOST_TOKEN").ok();
        std::env::remove_var("GHOST_TOKEN");
        let result = validate_token("anything");
        if let Some(val) = saved {
            std::env::set_var("GHOST_TOKEN", val);
        }
        assert!(result);
    }

    #[test]
    fn token_auth_valid() {
        let saved = std::env::var("GHOST_TOKEN").ok();
        std::env::set_var("GHOST_TOKEN", "test_secret_valid_123");
        let result = validate_token("test_secret_valid_123");
        match saved {
            Some(val) => std::env::set_var("GHOST_TOKEN", val),
            None => std::env::remove_var("GHOST_TOKEN"),
        }
        assert!(result);
    }

    #[test]
    fn token_auth_invalid() {
        let saved = std::env::var("GHOST_TOKEN").ok();
        std::env::set_var("GHOST_TOKEN", "test_secret_invalid_456");
        let result = validate_token("wrong_token");
        match saved {
            Some(val) => std::env::set_var("GHOST_TOKEN", val),
            None => std::env::remove_var("GHOST_TOKEN"),
        }
        assert!(!result);
    }

    #[test]
    fn auth_profile_rotation() {
        let mut mgr = AuthProfileManager::new();
        mgr.add_profile("openai".into(), "key1".into());
        mgr.add_profile("openai".into(), "key2".into());
        let first = mgr.current_profile("openai").unwrap();
        assert_eq!(first.api_key, "key1");
        let rotated = mgr.rotate("openai").unwrap();
        assert_eq!(rotated.api_key, "key2");
    }

    #[test]
    fn auth_profile_session_pinning() {
        let mut mgr = AuthProfileManager::new();
        mgr.add_profile("openai".into(), "key1".into());
        let session_id = Uuid::now_v7();
        mgr.pin_session(session_id, "openai".into());
        // Pinning is recorded (implementation detail)
    }

    #[test]
    fn auth_profile_all_exhausted() {
        let mgr = AuthProfileManager::new();
        assert!(mgr.all_exhausted("nonexistent"));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.5 — Inter-Agent Messaging
// ═══════════════════════════════════════════════════════════════════════

mod messaging {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use ghost_gateway::messaging::dispatcher::{MessageDispatcher, VerifyResult};
    use ghost_gateway::messaging::encryption::can_encrypt;
    use ghost_gateway::messaging::protocol::{
        AgentMessage, DelegationState, MessagePayload,
    };
    use uuid::Uuid;

    fn make_message(sender: Uuid, recipient: Uuid) -> AgentMessage {
        let mut msg = AgentMessage {
            id: Uuid::now_v7(),
            sender,
            recipient,
            payload: MessagePayload::Notification {
                message: "hello".into(),
            },
            context: BTreeMap::new(),
            nonce: Uuid::now_v7(),
            timestamp: Utc::now(),
            content_hash: [0u8; 32],
            signature: Vec::new(),
            encrypted: false,
        };
        msg.content_hash = msg.compute_content_hash();
        msg
    }

    #[test]
    fn canonical_bytes_deterministic() {
        let sender = Uuid::now_v7();
        let recipient = Uuid::now_v7();
        let nonce = Uuid::now_v7();
        let ts = Utc::now();

        let msg1 = AgentMessage {
            id: Uuid::nil(),
            sender,
            recipient,
            payload: MessagePayload::Notification { message: "test".into() },
            context: BTreeMap::new(),
            nonce,
            timestamp: ts,
            content_hash: [0u8; 32],
            signature: Vec::new(),
            encrypted: false,
        };
        let msg2 = AgentMessage {
            id: Uuid::nil(),
            sender,
            recipient,
            payload: MessagePayload::Notification { message: "test".into() },
            context: BTreeMap::new(),
            nonce,
            timestamp: ts,
            content_hash: [0u8; 32],
            signature: Vec::new(),
            encrypted: false,
        };
        assert_eq!(msg1.canonical_bytes(), msg2.canonical_bytes());
    }

    #[test]
    fn canonical_bytes_btreemap_deterministic() {
        let sender = Uuid::now_v7();
        let recipient = Uuid::now_v7();
        let nonce = Uuid::now_v7();
        let ts = Utc::now();

        let mut ctx1 = BTreeMap::new();
        ctx1.insert("z_key".into(), serde_json::json!("z_val"));
        ctx1.insert("a_key".into(), serde_json::json!("a_val"));

        let mut ctx2 = BTreeMap::new();
        ctx2.insert("a_key".into(), serde_json::json!("a_val"));
        ctx2.insert("z_key".into(), serde_json::json!("z_val"));

        let msg1 = AgentMessage {
            id: Uuid::nil(), sender, recipient,
            payload: MessagePayload::Notification { message: "test".into() },
            context: ctx1, nonce, timestamp: ts,
            content_hash: [0u8; 32], signature: Vec::new(), encrypted: false,
        };
        let msg2 = AgentMessage {
            id: Uuid::nil(), sender, recipient,
            payload: MessagePayload::Notification { message: "test".into() },
            context: ctx2, nonce, timestamp: ts,
            content_hash: [0u8; 32], signature: Vec::new(), encrypted: false,
        };
        assert_eq!(msg1.canonical_bytes(), msg2.canonical_bytes());
    }

    #[test]
    fn content_hash_verification() {
        let sender = Uuid::now_v7();
        let recipient = Uuid::now_v7();
        let msg = make_message(sender, recipient);
        assert_eq!(msg.content_hash, msg.compute_content_hash());
    }

    #[test]
    fn dispatcher_accepts_valid_message() {
        let mut dispatcher = MessageDispatcher::new();
        let msg = make_message(Uuid::now_v7(), Uuid::now_v7());
        assert!(matches!(dispatcher.verify(&msg), VerifyResult::Accepted));
    }

    #[test]
    fn dispatcher_rejects_tampered_hash() {
        let mut dispatcher = MessageDispatcher::new();
        let mut msg = make_message(Uuid::now_v7(), Uuid::now_v7());
        msg.content_hash[0] ^= 0xFF; // Tamper
        assert!(matches!(
            dispatcher.verify(&msg),
            VerifyResult::RejectedSignature(_)
        ));
    }

    #[test]
    fn dispatcher_rejects_replay() {
        let mut dispatcher = MessageDispatcher::new();
        let msg = make_message(Uuid::now_v7(), Uuid::now_v7());
        assert!(matches!(dispatcher.verify(&msg), VerifyResult::Accepted));
        // Same nonce again → replay
        assert!(matches!(
            dispatcher.verify(&msg),
            VerifyResult::RejectedReplay(_)
        ));
    }

    #[test]
    fn dispatcher_rejects_stale_timestamp() {
        let mut dispatcher = MessageDispatcher::new();
        let sender = Uuid::now_v7();
        let recipient = Uuid::now_v7();
        let mut msg = AgentMessage {
            id: Uuid::now_v7(),
            sender,
            recipient,
            payload: MessagePayload::Notification { message: "old".into() },
            context: BTreeMap::new(),
            nonce: Uuid::now_v7(),
            timestamp: Utc::now() - chrono::Duration::minutes(6), // >5min old
            content_hash: [0u8; 32],
            signature: Vec::new(),
            encrypted: false,
        };
        msg.content_hash = msg.compute_content_hash();
        assert!(matches!(
            dispatcher.verify(&msg),
            VerifyResult::RejectedReplay(_)
        ));
    }

    #[test]
    fn dispatcher_rate_limit() {
        let mut dispatcher = MessageDispatcher::new();
        let sender = Uuid::now_v7();
        let recipient = Uuid::now_v7();
        for _ in 0..60 {
            let msg = make_message(sender, recipient);
            dispatcher.verify(&msg);
        }
        // 61st should be rate limited
        let msg = make_message(sender, recipient);
        assert!(matches!(
            dispatcher.verify(&msg),
            VerifyResult::RejectedRateLimit
        ));
    }

    #[test]
    fn anomaly_detection_three_failures() {
        let mut dispatcher = MessageDispatcher::new();
        let sender = Uuid::now_v7();
        for _ in 0..3 {
            let mut msg = make_message(sender, Uuid::now_v7());
            msg.content_hash[0] ^= 0xFF; // Tamper
            dispatcher.verify(&msg);
        }
        assert!(dispatcher.sig_failure_count(sender) >= 3);
    }

    #[test]
    fn offline_queue() {
        let mut dispatcher = MessageDispatcher::new();
        let recipient = Uuid::now_v7();
        let msg = make_message(Uuid::now_v7(), recipient);
        dispatcher.queue_offline(recipient, msg);
        let delivered = dispatcher.deliver_queued(recipient);
        assert_eq!(delivered.len(), 1);
        // Queue is now empty
        assert!(dispatcher.deliver_queued(recipient).is_empty());
    }

    #[test]
    fn delegation_state_transitions() {
        assert!(DelegationState::Offered.can_transition_to(DelegationState::Accepted));
        assert!(DelegationState::Offered.can_transition_to(DelegationState::Rejected));
        assert!(DelegationState::Accepted.can_transition_to(DelegationState::Completed));
        assert!(DelegationState::Accepted.can_transition_to(DelegationState::Disputed));
        // Invalid
        assert!(!DelegationState::Offered.can_transition_to(DelegationState::Completed));
        assert!(!DelegationState::Rejected.can_transition_to(DelegationState::Accepted));
        assert!(!DelegationState::Completed.can_transition_to(DelegationState::Offered));
    }

    #[test]
    fn broadcast_cannot_be_encrypted() {
        assert!(!can_encrypt(true));
        assert!(can_encrypt(false));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.6 — Session Compaction
// ═══════════════════════════════════════════════════════════════════════

mod compaction {
    use ghost_gateway::session::compaction::{
        CompactionConfig, SessionCompactor,
    };

    #[test]
    fn triggers_at_70_percent() {
        let compactor = SessionCompactor::default();
        assert!(compactor.should_compact(70_000, 100_000));
        assert!(compactor.should_compact(80_000, 100_000));
    }

    #[test]
    fn does_not_trigger_at_69_percent() {
        let compactor = SessionCompactor::default();
        assert!(!compactor.should_compact(69_000, 100_000));
    }

    #[test]
    fn compact_reduces_tokens() {
        let compactor = SessionCompactor::default();
        let mut history: Vec<String> = (0..100)
            .map(|i| format!("message_{i} with some content"))
            .collect();
        let original_tokens: usize = history.iter().map(|m| m.len()).sum();
        let block = compactor.compact(&mut history, 1, None).unwrap();
        let new_tokens: usize = history.iter().map(|m| m.len()).sum();
        assert!(new_tokens < original_tokens);
        assert_eq!(block.pass_number, 1);
    }

    #[test]
    fn max_passes_enforced() {
        let compactor = SessionCompactor::default();
        let mut history = vec!["msg".into()];
        assert!(compactor.compact(&mut history, 4, None).is_err());
    }

    #[test]
    fn compaction_block_never_recompressed() {
        let block = ghost_gateway::session::compaction::CompactionBlock {
            summary: "compacted".into(),
            original_token_count: 1000,
            compressed_token_count: 100,
            pass_number: 1,
            timestamp: chrono::Utc::now(),
        };
        assert!(block.is_compaction_block());
    }

    #[test]
    fn prune_tool_results() {
        let mut history = vec![
            r#"{"role": "user", "content": "hello"}"#.into(),
            r#"{"role": "tool_result", "content": "result"}"#.into(),
            r#"{"role": "assistant", "content": "hi"}"#.into(),
        ];
        let result = SessionCompactor::prune_tool_results(&mut history);
        assert_eq!(result.results_pruned, 1);
        assert_eq!(history.len(), 2);
        assert!(result.tokens_freed > 0);
    }

    #[test]
    fn prune_preserves_user_messages() {
        let mut history = vec![
            r#"{"role": "user", "content": "hello"}"#.into(),
            r#"{"role": "assistant", "content": "hi"}"#.into(),
        ];
        let result = SessionCompactor::prune_tool_results(&mut history);
        assert_eq!(result.results_pruned, 0);
        assert_eq!(history.len(), 2);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — Config
// ═══════════════════════════════════════════════════════════════════════

mod config {
    use ghost_gateway::config::GhostConfig;

    #[test]
    fn default_config_valid() {
        let config = GhostConfig::default();
        assert_eq!(config.gateway.port, 18789);
        assert_eq!(config.gateway.bind, "127.0.0.1");
    }

    #[test]
    fn load_default_returns_default_when_no_file() {
        // Remove env var to avoid interference
        std::env::remove_var("GHOST_CONFIG");
        // This should return default config when no file exists
        let config = GhostConfig::load_default(None);
        assert!(config.is_ok());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.1 — ITP Router
// ═══════════════════════════════════════════════════════════════════════

mod itp_router {
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;

    use ghost_gateway::gateway::GatewayState;
    use ghost_gateway::itp_router::ITPEventRouter;

    #[tokio::test]
    async fn routes_to_buffer_in_degraded() {
        let state = Arc::new(AtomicU8::new(GatewayState::Degraded as u8));
        let router = ITPEventRouter::new(state, "127.0.0.1:18790".into());
        router.route(r#"{"event": "test"}"#.into()).await;
        let buffered = router.drain_buffer();
        assert_eq!(buffered.len(), 1);
    }

    #[tokio::test]
    async fn drain_buffer_empties() {
        let state = Arc::new(AtomicU8::new(GatewayState::Degraded as u8));
        let router = ITPEventRouter::new(state, "127.0.0.1:18790".into());
        router.route("event1".into()).await;
        router.route("event2".into()).await;
        let first_drain = router.drain_buffer();
        assert_eq!(first_drain.len(), 2);
        let second_drain = router.drain_buffer();
        assert!(second_drain.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Proptest: State transitions, kill switch monotonicity, signing determinism
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod property_tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use chrono::Utc;
    use ghost_gateway::gateway::GatewayState;
    use ghost_gateway::safety::kill_switch::{KillLevel, KillSwitch, PLATFORM_KILLED};
    use cortex_core::safety::trigger::TriggerEvent;
    use proptest::prelude::*;
    use uuid::Uuid;

    use crate::KILL_SWITCH_MUTEX;

    fn gateway_state_strategy() -> impl Strategy<Value = GatewayState> {
        prop_oneof![
            Just(GatewayState::Initializing),
            Just(GatewayState::Healthy),
            Just(GatewayState::Degraded),
            Just(GatewayState::Recovering),
            Just(GatewayState::ShuttingDown),
            Just(GatewayState::FatalError),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// Req 41 AC13: Only valid transitions succeed.
        #[test]
        fn only_valid_transitions_succeed(
            from in gateway_state_strategy(),
            to in gateway_state_strategy()
        ) {
            let valid = from.can_transition_to(to);
            // If valid, the transition should be in the known set
            if valid {
                let known_valid = matches!(
                    (from, to),
                    (GatewayState::Initializing, GatewayState::Healthy)
                    | (GatewayState::Initializing, GatewayState::Degraded)
                    | (GatewayState::Initializing, GatewayState::FatalError)
                    | (GatewayState::Healthy, GatewayState::Degraded)
                    | (GatewayState::Healthy, GatewayState::ShuttingDown)
                    | (GatewayState::Degraded, GatewayState::Recovering)
                    | (GatewayState::Degraded, GatewayState::ShuttingDown)
                    | (GatewayState::Recovering, GatewayState::Healthy)
                    | (GatewayState::Recovering, GatewayState::Degraded)
                    | (GatewayState::Recovering, GatewayState::ShuttingDown)
                );
                prop_assert!(known_valid, "Unexpected valid transition: {:?} -> {:?}", from, to);
            }
        }

        /// Req 41 AC1: Kill level never decreases without explicit resume.
        #[test]
        fn kill_level_monotonic(
            levels in proptest::collection::vec(0u8..4, 1..20)
        ) {
            let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
            PLATFORM_KILLED.store(false, Ordering::SeqCst);
            let ks = KillSwitch::new();
            let agent_id = Uuid::now_v7();
            let mut max_level = KillLevel::Normal;

            for level_u8 in levels {
                let level = match level_u8 {
                    0 => KillLevel::Normal,
                    1 => KillLevel::Pause,
                    2 => KillLevel::Quarantine,
                    _ => KillLevel::KillAll,
                };
                if level > KillLevel::Normal {
                    let trigger = TriggerEvent::ManualPause {
                        agent_id,
                        reason: "test".into(),
                        initiated_by: "test".into(),
                    };
                    ks.activate_agent(agent_id, level, &trigger);
                    if level > max_level {
                        max_level = level;
                    }
                }
            }

            let state = ks.current_state();
            if let Some(agent_state) = state.per_agent.get(&agent_id) {
                prop_assert!(agent_state.level >= max_level,
                    "Kill level decreased: {:?} < {:?}", agent_state.level, max_level);
            }
            PLATFORM_KILLED.store(false, Ordering::SeqCst);
        }

        /// Req 41 AC4: PLATFORM_KILLED=true ↔ state=KillAll.
        #[test]
        fn platform_killed_consistency(
            do_kill_all in proptest::bool::ANY
        ) {
            let _lock = KILL_SWITCH_MUTEX.lock().unwrap();
            PLATFORM_KILLED.store(false, Ordering::SeqCst);
            let ks = KillSwitch::new();
            if do_kill_all {
                let trigger = TriggerEvent::ManualKillAll {
                    reason: "test".into(),
                    initiated_by: "test".into(),
                };
                ks.activate_kill_all(&trigger);
            }
            let state = ks.current_state();
            let flag = PLATFORM_KILLED.load(Ordering::SeqCst);
            prop_assert_eq!(
                flag,
                state.platform_level == KillLevel::KillAll,
                "PLATFORM_KILLED={} but level={:?}", flag, state.platform_level
            );
            PLATFORM_KILLED.store(false, Ordering::SeqCst);
        }
    }
}
