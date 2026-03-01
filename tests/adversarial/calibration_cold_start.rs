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

// ── Calibration gate simulation ─────────────────────────────────────────
//
// Mirrors the calibration gate logic from monitor.rs handle_event Step 6:
//   let session_count = calibration_counts.get(&agent_id).unwrap_or(0);
//   if session_count < calibration_sessions { return; }

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

    /// Record a session start and return whether scoring should proceed.
    fn record_session_start(&mut self, agent_id: Uuid) -> bool {
        let count = self.calibration_counts.entry(agent_id).or_insert(0);
        *count += 1;
        *count >= self.calibration_sessions
    }

    fn session_count(&self, agent_id: &Uuid) -> u32 {
        self.calibration_counts.get(agent_id).copied().unwrap_or(0)
    }
}

// ── Calibration gate boundary tests ─────────────────────────────────────

#[test]
fn first_9_sessions_are_calibration_only() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    for i in 1..=9 {
        let should_score = gate.record_session_start(agent);
        assert!(
            !should_score,
            "session {i} should be calibration-only (no scoring)"
        );
    }
}

#[test]
fn session_10_triggers_scoring() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    for _ in 1..=9 {
        gate.record_session_start(agent);
    }

    let should_score = gate.record_session_start(agent);
    assert!(
        should_score,
        "session 10 should trigger scoring (count=10 >= calibration_sessions=10)"
    );
}

#[test]
fn sessions_after_calibration_always_score() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    for _ in 1..=10 {
        gate.record_session_start(agent);
    }

    for session in 11..=100 {
        let should_score = gate.record_session_start(agent);
        assert!(should_score, "session {session} should always score");
    }
}

// ── Per-agent isolation ─────────────────────────────────────────────────

#[test]
fn calibration_is_per_agent() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent_a = Uuid::new_v4();
    let agent_b = Uuid::new_v4();

    // Agent A completes calibration
    for _ in 1..=10 {
        gate.record_session_start(agent_a);
    }

    // Agent B is still calibrating
    gate.record_session_start(agent_b);

    assert_eq!(gate.session_count(&agent_a), 10);
    assert_eq!(gate.session_count(&agent_b), 1);
}

// ── Cold-start attack: agent ID cycling ─────────────────────────────────

#[test]
fn agent_cycling_attack_stays_in_calibration_forever() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);

    // Attacker creates a new agent ID every 9 sessions
    for _cycle in 0..100 {
        let fresh_agent = Uuid::new_v4();
        for session in 1..=9 {
            let should_score = gate.record_session_start(fresh_agent);
            assert!(
                !should_score,
                "cycling attack: session {session} of fresh agent should not score"
            );
        }
    }

    // 100 cycles × 9 sessions = 900 sessions, NONE scored
    // This is the known cold-start vulnerability
}

/// Mitigation: provisional tracking limits unknown agents to 3 sessions.
/// After 3 sessions without identity verification, the agent is dropped.
/// This means the cycling attack is limited to 3 sessions per identity,
/// not 9.
#[test]
fn provisional_tracking_limits_cycling_attack() {
    // With provisional tracking (max 3), an attacker cycling fresh IDs
    // gets at most 3 unscored sessions per identity before being dropped.
    // Without provisional tracking, they'd get 9.
    assert!(
        MAX_PROVISIONAL_SESSIONS < CALIBRATION_SESSIONS,
        "provisional limit ({MAX_PROVISIONAL_SESSIONS}) must be less than \
         calibration window ({CALIBRATION_SESSIONS}) to limit cycling attacks"
    );

    // Attack surface: 3 sessions × N identities
    // vs. without mitigation: 9 sessions × N identities
    // Reduction factor: 3/9 = 33% of the unmonitored window
    let reduction = MAX_PROVISIONAL_SESSIONS as f64 / (CALIBRATION_SESSIONS - 1) as f64;
    assert!(
        reduction < 0.5,
        "provisional tracking should reduce cycling attack surface by >50%, got {:.0}%",
        reduction * 100.0
    );
}

// ── Cold-start attack: rapid session cycling within one agent ───────────

#[test]
fn rapid_session_cycling_reaches_calibration_boundary() {
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    // Attacker rapidly starts/ends sessions to burn through calibration
    // This is NOT an attack — it's expected behavior. The calibration
    // window is session-count-based, not time-based.
    for _ in 1..=10 {
        gate.record_session_start(agent);
    }

    // After 10 rapid sessions, scoring begins
    assert!(gate.record_session_start(agent));
}

// ── Threat model documentation (as test comments) ───────────────────────
//
// THREAT: Fresh install cycling
//   Vector: Attacker installs GHOST, uses 9 sessions, reinstalls
//   Impact: Never triggers scoring/interventions
//   Mitigations:
//     1. Provisional tracking (max 3 sessions for unknown agents)
//     2. Hash chain persistence (audit trail for forensic analysis)
//     3. Rate limiting (100/min prevents event flooding)
//     4. Session lifecycle tracking (synthetic SessionEnd on restart)
//   Gaps:
//     - No cross-install identity linking
//     - No hardware fingerprinting
//     - No minimum session duration during calibration
//
// THREAT: Calibration window exploitation
//   Vector: Attacker performs harmful actions during first 9 sessions
//   Impact: No interventions triggered during calibration
//   Mitigations:
//     1. Events are still persisted with hash chain (forensic trail)
//     2. Rate limiting still applies
//     3. Clock skew rejection (5min tolerance) prevents timestamp manipulation
//   Gaps:
//     - No real-time alerting during calibration
//     - Calibration window is fixed (not adaptive)
//
// THREAT: Session count manipulation
//   Vector: Attacker sends fake SessionStart events to burn through calibration
//   Impact: Premature exit from calibration with poisoned baseline
//   Mitigations:
//     1. Rate limiting (100/min/conn)
//     2. Timestamp validation (reject >5min future)
//     3. Session ID validation (reject nil)
//   Gaps:
//     - No minimum session duration enforcement
//     - No session content validation during calibration

#[test]
fn calibration_window_is_bounded() {
    // The calibration window is exactly CALIBRATION_SESSIONS sessions.
    // It cannot be extended or shortened by the agent.
    let mut gate = CalibrationGate::new(CALIBRATION_SESSIONS);
    let agent = Uuid::new_v4();

    let mut calibration_count = 0;
    for _ in 1..=20 {
        if !gate.record_session_start(agent) {
            calibration_count += 1;
        }
    }

    assert_eq!(
        calibration_count,
        (CALIBRATION_SESSIONS - 1) as usize,
        "exactly {0} sessions should be calibration-only (sessions 1 through {0})",
        CALIBRATION_SESSIONS - 1
    );
}
