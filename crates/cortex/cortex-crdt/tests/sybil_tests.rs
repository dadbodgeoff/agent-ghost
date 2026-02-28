//! Sybil resistance unit tests (Task 3.6 — Req 29 AC2).

use chrono::{Duration, Utc};
use cortex_crdt::sybil::{SybilError, SybilGuard};
use uuid::Uuid;

// ── AC2: Max 3 children per parent per 24h ──────────────────────────────

#[test]
fn three_spawns_in_24h_all_succeed() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let now = Utc::now();

    for i in 0..3 {
        let child = Uuid::new_v4();
        let result = guard.register_spawn(parent, child, now);
        assert!(result.is_ok(), "spawn {i} should succeed");
    }
}

#[test]
fn fourth_spawn_in_24h_rejected() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let now = Utc::now();

    for _ in 0..3 {
        guard
            .register_spawn(parent, Uuid::new_v4(), now)
            .expect("first 3 should succeed");
    }

    let result = guard.register_spawn(parent, Uuid::new_v4(), now);
    assert!(result.is_err());
    match result.unwrap_err() {
        SybilError::SpawnLimitExceeded {
            parent_id,
            count,
            max,
        } => {
            assert_eq!(parent_id, parent);
            assert_eq!(count, 3);
            assert_eq!(max, 3);
        }
    }
}

// ── AC2: New agents start at trust 0.3 ──────────────────────────────────

#[test]
fn new_agent_trust_is_0_3() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    let now = Utc::now();

    let trust = guard.register_spawn(parent, child, now).unwrap();
    assert_eq!(trust.trust, guard.initial_trust());
    assert!((trust.trust - 0.3).abs() < f64::EPSILON);
}

// ── AC2: Trust capped at 0.6 for agents < 7 days old ───────────────────

#[test]
fn young_agent_trust_capped_at_0_6() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    let now = Utc::now();

    guard.register_spawn(parent, child, now).unwrap();

    // Set trust to 0.9 — should be capped at 0.6 for young agent
    guard.set_trust(child, 0.9);
    let effective = guard.effective_trust(&child);
    assert!(
        (effective - 0.6).abs() < f64::EPSILON,
        "young agent trust should be capped at 0.6, got {effective}"
    );
}

#[test]
fn old_agent_trust_not_capped() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    // Created 8 days ago
    let created = Utc::now() - Duration::days(8);

    guard.register_spawn(parent, child, created).unwrap();
    guard.set_trust(child, 0.9);

    let effective = guard.effective_trust(&child);
    assert!(
        (effective - 0.9).abs() < f64::EPSILON,
        "agent >7 days old should not be capped, got {effective}"
    );
}

// ── Boundary test: 23h59m spawn ─────────────────────────────────────────

#[test]
fn spawn_at_23h59m_still_rejected() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let now = Utc::now();

    // 3 spawns at time T
    for _ in 0..3 {
        guard
            .register_spawn(parent, Uuid::new_v4(), now)
            .unwrap();
    }

    // Try 4th at T + 23h59m — still within 24h window
    let almost_24h = now + Duration::hours(23) + Duration::minutes(59);
    let result = guard.register_spawn(parent, Uuid::new_v4(), almost_24h);
    assert!(result.is_err(), "4th spawn at 23h59m should still be rejected");
}

#[test]
fn spawn_after_24h_window_succeeds() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let now = Utc::now();

    // 3 spawns at time T
    for _ in 0..3 {
        guard
            .register_spawn(parent, Uuid::new_v4(), now)
            .unwrap();
    }

    // Try 4th at T + 24h01m — outside window, old records pruned
    let after_24h = now + Duration::hours(24) + Duration::minutes(1);
    let result = guard.register_spawn(parent, Uuid::new_v4(), after_24h);
    assert!(result.is_ok(), "spawn after 24h window should succeed");
}

// ── Different parents are independent ───────────────────────────────────

#[test]
fn different_parents_have_independent_limits() {
    let mut guard = SybilGuard::new();
    let parent_a = Uuid::new_v4();
    let parent_b = Uuid::new_v4();
    let now = Utc::now();

    // Parent A: 3 spawns
    for _ in 0..3 {
        guard
            .register_spawn(parent_a, Uuid::new_v4(), now)
            .unwrap();
    }

    // Parent B: should still be able to spawn
    let result = guard.register_spawn(parent_b, Uuid::new_v4(), now);
    assert!(result.is_ok(), "different parent should have independent limit");
}

// ── Unknown agent trust ─────────────────────────────────────────────────

#[test]
fn unknown_agent_effective_trust_is_zero() {
    let guard = SybilGuard::new();
    let unknown = Uuid::new_v4();
    assert_eq!(guard.effective_trust(&unknown), 0.0);
}

// ── Default impl ────────────────────────────────────────────────────────

#[test]
fn sybil_guard_default_matches_new() {
    let guard = SybilGuard::default();
    assert_eq!(guard.initial_trust(), 0.3);
    assert_eq!(guard.young_agent_cap(), 0.6);
}

// ── Trust at exactly 7 days ─────────────────────────────────────────────

#[test]
fn agent_at_exactly_7_days_is_not_capped() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    // Created exactly 7 days ago
    let created = Utc::now() - Duration::days(7);

    guard.register_spawn(parent, child, created).unwrap();
    guard.set_trust(child, 0.9);

    let effective = guard.effective_trust(&child);
    // At exactly 7 days, age >= 7 days, so not capped
    assert!(
        (effective - 0.9).abs() < f64::EPSILON,
        "agent at exactly 7 days should not be capped, got {effective}"
    );
}
