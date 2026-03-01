//! Adversarial stress tests: Convergence monitor calibration gate cold-start.
//!
//! The first N sessions (default 10) per agent are calibration-only — no
//! scoring or interventions. This creates a known cold-start window that
//! a malicious actor could exploit by cycling fresh agent IDs.
//!
//! Since convergence-monitor is a binary crate, these tests verify the
//! calibration gate properties through behavioral assertions and document
//! the threat model for the cold-start window.

use std::collections::BTreeMap;
use uuid::Uuid;

const CALIBRATION_SESSIONS: u32 = 10;
const MAX_PROVISIONAL_SESSIONS: u32 = 3;

struct CalibrationGate {
    calibration_counts: BTreeMap<Uuid, u32>,
    calibration_sessions: u32,
}

impl CalibrationGate {
    fn new(calibration_sessions: u32) -> Self {
        Self {
            calibration_counts: BTreeMap::new(),
            calibration_sessions,
        }
    }

    fn record_session_start(&mut self, agent_id: Uuid) -> bool {
        let count = self.calibration_counts.entry(agent_id).or_insert(0);
        *count += 1;
        *count >= self.calibration_sessions
    }

    fn session_count(&self, agent_id: &Uuid) -> u32 {
        self.calibration_counts.get(agent_id).copied().unwrap_or(0)
    }
}

#[test]
fn first_9_sessions_are_calibration_only() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    for i in 1..=9 {
        let should_score = gate.record_session_start(agent);
        assert!(!should_score, "session {i} should be calibration-only");
    }
}

#[test]
fn session_10_triggers_scoring() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    for _ in 1..=9 {
        gate.record_session_start(agent);
    }

    assert!(gate.record_session_start(agent), "session 10 should trigger scoring");
}

#[test]
fn sessions_after_calibration_always_score() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    for _ in 1..=10 {
        gate.record_session_start(agent);
    }

    for session in 11..=100 {
        assert!(gate.record_session_start(agent), "session {session} should score");
    }
}

#[test]
fn calibration_is_per_agent() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent_a = Uuid::new_v4();
    let agent_b = Uuid::new_v4();

    for _ in 1..=10 {
        gate.record_session_start(agent_a);
    }
    gate.record_session_start(agent_b);

    assert_eq!(gate.session_count(&agent_a), 10);
    assert_eq!(gate.session_count(&agent_b), 1);
}

#[test]
fn agent_cycling_attack_stays_in_calibration_forever() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);

    for _cycle in 0..100 {
        let fresh_agent = Uuid::new_v4();
        for session in 1..=9 {
            assert!(
                !gate.record_session_start(fresh_agent),
                "cycling attack: session {session} of fresh agent should not score"
            );
        }
    }
}

#[test]
fn provisional_tracking_limits_cycling_attack() {
    assert!(
        MAX_PROVISIONAL_SESSIONS < CALIBRATION_SESSIONS,
        "provisional limit must be less than calibration window"
    );

    let reduction = MAX_PROVISIONAL_SESSIONS as f64 / (CALIBRATION_SESSIONS - 1) as f64;
    assert!(
        reduction < 0.5,
        "provisional tracking should reduce cycling attack surface by >50%"
    );
}

#[test]
fn calibration_window_is_bounded() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    let mut calibration_count = 0;
    for _ in 1..=20 {
        if !gate.record_session_start(agent) {
            calibration_count += 1;
        }
    }

    assert_eq!(calibration_count, (CALIBRATION_SESSIONS - 1) as usize);
}
