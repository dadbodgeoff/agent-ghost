//! Adversarial: Temporal Sybil re-registration.
//!
//! The SybilGuard tracks spawn counts in a 24h sliding window, but does NOT
//! track identity churn rate across multiple windows. An attacker can
//! deregister and re-register agents across time windows to accumulate
//! a fleet of sybil identities without ever exceeding the per-window limit.
//!
//! These tests probe the temporal gaps in spawn tracking.

use chrono::{Duration, Utc};
use cortex_crdt::sybil::SybilGuard;
use uuid::Uuid;

// ── Window boundary: spawn limit resets after 24h ───────────────────────

/// An attacker spawns 3 agents per 24h window. After N days, they control
/// 3*N agents. The SybilGuard has no cross-window accumulation limit.
#[test]
fn attacker_accumulates_fleet_across_windows() {
    let mut guard = SybilGuard::new();
    let attacker = Uuid::new_v4();
    let start = Utc::now();

    let mut total_spawned = Vec::new();

    for day in 0..30 {
        let window_start = start + Duration::days(day);
        for _ in 0..3 {
            let child = Uuid::new_v4();
            let result = guard.register_spawn(attacker, child, window_start);
            assert!(result.is_ok(), "day {day}: spawn should succeed within window");
            total_spawned.push(child);
        }
    }

    // After 30 days: 90 agents spawned, all from the same parent
    assert_eq!(total_spawned.len(), 90);

    // The guard only tracks the LAST 24h window — old spawns are pruned.
    // Verify the 4th spawn in the current window still fails:
    let now = start + Duration::days(29);
    let result = guard.register_spawn(attacker, Uuid::new_v4(), now);
    assert!(result.is_err(), "4th spawn in current window should still fail");
}

// ── Identity churn: deregister + re-register same slot ──────────────────

/// Attacker spawns 3 agents, waits 24h+1m, spawns 3 more. The old agents
/// are still alive but the spawn records are pruned. No churn rate tracking.
#[test]
fn churn_rate_not_tracked_across_windows() {
    let mut guard = SybilGuard::new();
    let attacker = Uuid::new_v4();
    let t0 = Utc::now();

    // Window 1: spawn 3
    let mut wave1 = Vec::new();
    for _ in 0..3 {
        let child = Uuid::new_v4();
        guard.register_spawn(attacker, child, t0).unwrap();
        wave1.push(child);
    }

    // Window 2: 24h+1m later, spawn 3 more
    let t1 = t0 + Duration::hours(24) + Duration::minutes(1);
    let mut wave2 = Vec::new();
    for _ in 0..3 {
        let child = Uuid::new_v4();
        guard.register_spawn(attacker, child, t1).unwrap();
        wave2.push(child);
    }

    // All 6 agents exist and have trust entries
    for &child in wave1.iter().chain(wave2.iter()) {
        let trust = guard.effective_trust(&child);
        assert!(trust > 0.0, "spawned agent should have non-zero trust");
    }
}

// ── Young agent trust cap provides temporal defense ─────────────────────

/// Even though an attacker can accumulate agents across windows, ALL agents
/// younger than 7 days are capped at 0.6 trust. This limits the damage
/// from a recently-spawned fleet.
#[test]
fn accumulated_fleet_all_trust_capped_for_7_days() {
    let mut guard = SybilGuard::new();
    let attacker = Uuid::new_v4();
    let start = Utc::now();

    let mut fleet = Vec::new();
    for day in 0..7 {
        let t = start + Duration::days(day);
        for _ in 0..3 {
            let child = Uuid::new_v4();
            guard.register_spawn(attacker, child, t).unwrap();
            guard.set_trust(child, 1.0); // try to max out trust
            fleet.push(child);
        }
    }

    // All 21 agents are < 7 days old, so all capped at 0.6
    for &child in &fleet {
        let effective = guard.effective_trust(&child);
        assert!(
            effective <= 0.6,
            "young fleet agent should be capped at 0.6, got {effective}"
        );
    }
}

/// After 7 days, the trust cap lifts. The oldest agents in the fleet
/// become uncapped while newer ones remain capped.
#[test]
fn fleet_trust_cap_lifts_progressively() {
    let mut guard = SybilGuard::new();
    let attacker = Uuid::new_v4();
    let start = Utc::now() - Duration::days(10); // 10 days ago

    // Day 0: spawn 3 agents (now 10 days old → uncapped)
    let mut old_agents = Vec::new();
    for _ in 0..3 {
        let child = Uuid::new_v4();
        guard.register_spawn(attacker, child, start).unwrap();
        guard.set_trust(child, 0.9);
        old_agents.push(child);
    }

    // Day 8: spawn 3 agents (now 2 days old → capped)
    let recent = start + Duration::days(8);
    let mut young_agents = Vec::new();
    for _ in 0..3 {
        let child = Uuid::new_v4();
        guard.register_spawn(attacker, child, recent).unwrap();
        guard.set_trust(child, 0.9);
        young_agents.push(child);
    }

    // Old agents: uncapped (>7 days)
    for &agent in &old_agents {
        let effective = guard.effective_trust(&agent);
        assert!(
            (effective - 0.9).abs() < f64::EPSILON,
            "old agent should be uncapped at 0.9, got {effective}"
        );
    }

    // Young agents: capped at 0.6 (<7 days)
    for &agent in &young_agents {
        let effective = guard.effective_trust(&agent);
        assert!(
            effective <= 0.6,
            "young agent should be capped at 0.6, got {effective}"
        );
    }
}

// ── Sliding window precision: exactly at boundary ───────────────────────

#[test]
fn spawn_at_exact_24h_boundary_succeeds() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let t0 = Utc::now();

    for _ in 0..3 {
        guard.register_spawn(parent, Uuid::new_v4(), t0).unwrap();
    }

    // Exactly 24h later: old records should be pruned (cutoff = now - 24h,
    // records retained if t > cutoff, so records at t0 where t0 == cutoff
    // are NOT retained because t0 > cutoff is false when t0 == cutoff)
    let exactly_24h = t0 + Duration::hours(24);
    let result = guard.register_spawn(parent, Uuid::new_v4(), exactly_24h);
    assert!(
        result.is_ok(),
        "spawn at exactly 24h boundary should succeed (old records pruned)"
    );
}

// ── Multiple parents: distributed spawning attack ───────────────────────

/// Attacker controls multiple parent identities and spawns 3 children
/// from each, bypassing the per-parent limit.
#[test]
fn distributed_spawning_via_multiple_parents() {
    let mut guard = SybilGuard::new();
    let now = Utc::now();

    let num_parents = 10;
    let mut total_children = Vec::new();

    for _ in 0..num_parents {
        let parent = Uuid::new_v4();
        for _ in 0..3 {
            let child = Uuid::new_v4();
            guard.register_spawn(parent, child, now).unwrap();
            total_children.push(child);
        }
    }

    // 10 parents × 3 children = 30 sybil agents in a single window
    assert_eq!(total_children.len(), 30);

    // All are young → capped at 0.6
    for &child in &total_children {
        guard.set_trust(child, 1.0);
        assert!(guard.effective_trust(&child) <= 0.6);
    }
}

// ── Threat model documentation ──────────────────────────────────────────
//
// GAP: No cross-window churn rate tracking
//   - SybilGuard prunes spawn records older than 24h
//   - No cumulative spawn counter per parent
//   - No global spawn rate limit across all parents
//
// GAP: No identity deregistration tracking
//   - Agents can be spawned and abandoned without penalty
//   - No cost to creating and discarding identities
//
// MITIGATION PRESENT: Young agent trust cap (0.6 for <7 days)
//   - Limits immediate damage from freshly spawned fleet
//   - But lifts after 7 days if agent survives
//
// RECOMMENDATION: Add cumulative spawn counter per parent (lifetime total)
//   with exponential backoff on spawn rate after threshold (e.g., >10 total
//   spawns → 48h window, >30 → 72h window)
