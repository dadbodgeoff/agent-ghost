//! Phase 5 tests for ghost-heartbeat (Task 5.9).
//! Updated for Task 20.4: tiered heartbeat with interval_for_state().

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════
// Task 5.9 + Task 20.4 — Heartbeat Engine (Tiered)
// ═══════════════════════════════════════════════════════════════════════

mod heartbeat {
    use super::*;
    use ghost_heartbeat::heartbeat::{heartbeat_session_key, HeartbeatConfig, HeartbeatEngine};
    use ghost_heartbeat::tiers::interval_for_state;

    fn make_engine() -> HeartbeatEngine {
        HeartbeatEngine::new(
            HeartbeatConfig::default(),
            Uuid::now_v7(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[test]
    fn fires_at_configured_interval() {
        let engine = make_engine();
        // No previous beat — should fire
        assert!(engine.should_fire(0));
    }

    #[test]
    fn uses_dedicated_session() {
        let agent_id = Uuid::now_v7();
        let key1 = heartbeat_session_key(agent_id);
        let key2 = heartbeat_session_key(agent_id);
        // Deterministic
        assert_eq!(key1, key2);
        // Different from agent_id
        assert_ne!(key1, agent_id);
    }

    #[test]
    fn message_matches_spec() {
        let engine = make_engine();
        assert_eq!(
            engine.message(),
            "[HEARTBEAT] Check HEARTBEAT.md and act if needed."
        );
    }

    // Task 20.4: interval_for_state() tests — SPEEDS UP at higher levels
    #[test]
    fn stable_state_120s() {
        // score_delta < 0.01, consecutive_stable >= 3, level 0 → 120s
        let interval = interval_for_state(0.005, 3, 0);
        assert_eq!(interval, Duration::from_secs(120));
    }

    #[test]
    fn active_state_30s() {
        // score moving, level 0 → 30s
        let interval = interval_for_state(0.03, 0, 0);
        assert_eq!(interval, Duration::from_secs(30));
    }

    #[test]
    fn escalated_state_15s() {
        // level >= 2 → 15s
        let interval = interval_for_state(0.06, 0, 2);
        assert_eq!(interval, Duration::from_secs(15));
    }

    #[test]
    fn critical_l4_not_disabled_5s() {
        // KEY FIX from Task 20.4: L4 is NOT disabled — uses 5s Tier0 binary pings
        let interval = interval_for_state(0.005, 3, 4);
        assert_eq!(interval, Duration::from_secs(5));
    }

    #[test]
    fn l3_escalated_15s() {
        // Level 3 (>= 2) → 15s
        let interval = interval_for_state(0.0, 0, 3);
        assert_eq!(interval, Duration::from_secs(15));
    }

    #[test]
    fn platform_killed_stops_heartbeat() {
        let killed = Arc::new(AtomicBool::new(true));
        let engine = HeartbeatEngine::new(
            HeartbeatConfig::default(),
            Uuid::now_v7(),
            killed,
            Arc::new(AtomicBool::new(false)),
        );
        assert!(!engine.should_fire(0));
    }

    #[test]
    fn agent_paused_stops_heartbeat() {
        let paused = Arc::new(AtomicBool::new(true));
        let engine = HeartbeatEngine::new(
            HeartbeatConfig::default(),
            Uuid::now_v7(),
            Arc::new(AtomicBool::new(false)),
            paused,
        );
        assert!(!engine.should_fire(0));
    }

    #[test]
    fn cost_ceiling_stops_heartbeat() {
        let mut engine = make_engine();
        engine.config.cost_ceiling = 1.0;
        engine.total_cost = 1.0;
        assert!(!engine.should_fire(0));
    }

    #[test]
    fn record_beat_updates_state() {
        let mut engine = make_engine();
        assert!(engine.last_beat.is_none());
        engine.record_beat(0.10);
        assert!(engine.last_beat.is_some());
        assert!((engine.total_cost - 0.10).abs() < f64::EPSILON);
    }

    #[test]
    fn record_beat_with_score_tracks_tiered_state() {
        let mut engine = make_engine();
        engine.record_beat_with_score(0.01, 0.5);
        assert_eq!(engine.tiered_state.last_score, Some(0.5));
        engine.record_beat_with_score(0.01, 0.5);
        assert_eq!(engine.tiered_state.consecutive_stable, 2);
    }

    #[test]
    fn l4_should_fire_not_disabled() {
        // Task 20.4: L4 must NOT return false (disabled). It should fire.
        let engine = make_engine();
        assert!(engine.should_fire(4));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.9 — Cron Engine
// ═══════════════════════════════════════════════════════════════════════

mod cron {
    use super::*;
    use ghost_heartbeat::cron::CronEngine;

    fn make_engine() -> CronEngine {
        CronEngine::new(
            Uuid::now_v7(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[test]
    fn parses_cron_syntax() {
        // "* * * * *" matches any time
        assert!(CronEngine::cron_matches("* * * * *", Utc::now()));
    }

    #[test]
    fn invalid_cron_syntax() {
        // Too few fields
        assert!(!CronEngine::cron_matches("* *", Utc::now()));
    }

    #[test]
    fn loads_jobs_from_yaml() {
        let mut engine = make_engine();
        let yaml = r#"
name: daily_check
schedule: "0 9 * * *"
message: "Run daily check"
timezone: UTC
enabled: true
"#;
        engine.load_jobs(&[yaml.into()]);
        assert_eq!(engine.jobs.len(), 1);
        assert_eq!(engine.jobs[0].def.name, "daily_check");
    }

    #[test]
    fn disabled_job_not_loaded() {
        let mut engine = make_engine();
        let yaml = r#"
name: disabled_job
schedule: "* * * * *"
message: "should not load"
enabled: false
"#;
        engine.load_jobs(&[yaml.into()]);
        assert_eq!(engine.jobs.len(), 0);
    }

    #[test]
    fn invalid_yaml_graceful() {
        let mut engine = make_engine();
        engine.load_jobs(&["not: valid: yaml: {{".into()]);
        assert_eq!(engine.jobs.len(), 0);
    }

    #[test]
    fn platform_killed_no_ready_jobs() {
        let killed = Arc::new(AtomicBool::new(true));
        let engine = CronEngine::new(Uuid::now_v7(), killed, Arc::new(AtomicBool::new(false)));
        assert!(engine.ready_jobs().is_empty());
    }

    #[test]
    fn agent_paused_no_ready_jobs() {
        let paused = Arc::new(AtomicBool::new(true));
        let engine = CronEngine::new(Uuid::now_v7(), Arc::new(AtomicBool::new(false)), paused);
        assert!(engine.ready_jobs().is_empty());
    }

    #[test]
    fn record_run_updates_state() {
        let mut engine = make_engine();
        let yaml = r#"
name: test_job
schedule: "* * * * *"
message: "test"
"#;
        engine.load_jobs(&[yaml.into()]);
        engine.record_run(0, 0.05);
        assert_eq!(engine.jobs[0].run_count, 1);
        assert!((engine.jobs[0].total_cost - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn timezone_defaults_to_utc() {
        let mut engine = make_engine();
        let yaml = r#"
name: tz_test
schedule: "* * * * *"
message: "test"
"#;
        engine.load_jobs(&[yaml.into()]);
        assert_eq!(engine.jobs[0].def.timezone, "UTC");
    }
}
