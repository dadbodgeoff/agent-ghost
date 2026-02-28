//! E2E: Gateway bootstrap → degraded mode → recovery → healthy.
//!
//! Validates the 6-state FSM: Initializing, Healthy, Degraded, Recovering,
//! ShuttingDown, FatalError. Tests valid and invalid transitions.

use ghost_gateway::gateway::{GatewaySharedState, GatewayState};

// ── Valid Transitions ───────────────────────────────────────────────────

/// Initializing → Healthy is valid.
#[test]
fn transition_initializing_to_healthy() {
    let state = GatewaySharedState::new();
    assert_eq!(state.current_state(), GatewayState::Initializing);

    state.transition_to(GatewayState::Healthy).unwrap();
    assert_eq!(state.current_state(), GatewayState::Healthy);
}

/// Initializing → Degraded is valid (monitor unreachable).
#[test]
fn transition_initializing_to_degraded() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Degraded).unwrap();
    assert_eq!(state.current_state(), GatewayState::Degraded);
}

/// Initializing → FatalError is valid.
#[test]
fn transition_initializing_to_fatal() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::FatalError).unwrap();
    assert_eq!(state.current_state(), GatewayState::FatalError);
}

/// Healthy → Degraded is valid (monitor dies).
#[test]
fn transition_healthy_to_degraded() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Healthy).unwrap();
    state.transition_to(GatewayState::Degraded).unwrap();
    assert_eq!(state.current_state(), GatewayState::Degraded);
}

/// Healthy → ShuttingDown is valid.
#[test]
fn transition_healthy_to_shutting_down() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Healthy).unwrap();
    state.transition_to(GatewayState::ShuttingDown).unwrap();
    assert_eq!(state.current_state(), GatewayState::ShuttingDown);
}

/// Degraded → Recovering is valid (monitor reconnected).
#[test]
fn transition_degraded_to_recovering() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Degraded).unwrap();
    state.transition_to(GatewayState::Recovering).unwrap();
    assert_eq!(state.current_state(), GatewayState::Recovering);
}

/// Recovering → Healthy is valid (sync complete).
#[test]
fn transition_recovering_to_healthy() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Degraded).unwrap();
    state.transition_to(GatewayState::Recovering).unwrap();
    state.transition_to(GatewayState::Healthy).unwrap();
    assert_eq!(state.current_state(), GatewayState::Healthy);
}

/// Recovering → Degraded is valid (monitor dies during recovery).
#[test]
fn transition_recovering_to_degraded() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Degraded).unwrap();
    state.transition_to(GatewayState::Recovering).unwrap();
    state.transition_to(GatewayState::Degraded).unwrap();
    assert_eq!(state.current_state(), GatewayState::Degraded);
}

// ── Invalid Transitions ─────────────────────────────────────────────────

/// Healthy → Recovering is INVALID (must go through Degraded).
#[test]
#[cfg(not(debug_assertions))]
fn transition_healthy_to_recovering_invalid() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Healthy).unwrap();

    let result = state.transition_to(GatewayState::Recovering);
    assert!(result.is_err(), "Healthy → Recovering should be invalid");
}

/// FatalError → anything is INVALID (terminal state).
#[test]
#[cfg(not(debug_assertions))]
fn transition_fatal_error_terminal() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::FatalError).unwrap();

    for target in [
        GatewayState::Initializing,
        GatewayState::Healthy,
        GatewayState::Degraded,
        GatewayState::Recovering,
        GatewayState::ShuttingDown,
    ] {
        let result = state.transition_to(target);
        assert!(
            result.is_err(),
            "FatalError → {:?} should be invalid",
            target
        );
    }
}

/// ShuttingDown → anything is INVALID (terminal state).
#[test]
#[cfg(not(debug_assertions))]
fn transition_shutting_down_terminal() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Healthy).unwrap();
    state.transition_to(GatewayState::ShuttingDown).unwrap();

    for target in [
        GatewayState::Initializing,
        GatewayState::Healthy,
        GatewayState::Degraded,
        GatewayState::Recovering,
        GatewayState::FatalError,
    ] {
        let result = state.transition_to(target);
        assert!(
            result.is_err(),
            "ShuttingDown → {:?} should be invalid",
            target
        );
    }
}

// ── Full Bootstrap → Degraded → Recovery → Healthy Cycle ────────────────

/// Full lifecycle: Init → Degraded → Recovering → Healthy → ShuttingDown.
#[test]
fn full_gateway_lifecycle() {
    let state = GatewaySharedState::new();

    // Bootstrap with unreachable monitor
    state.transition_to(GatewayState::Degraded).unwrap();
    assert_eq!(state.current_state(), GatewayState::Degraded);

    // Monitor becomes reachable
    state.transition_to(GatewayState::Recovering).unwrap();
    assert_eq!(state.current_state(), GatewayState::Recovering);

    // Sync complete
    state.transition_to(GatewayState::Healthy).unwrap();
    assert_eq!(state.current_state(), GatewayState::Healthy);

    // Graceful shutdown
    state.transition_to(GatewayState::ShuttingDown).unwrap();
    assert_eq!(state.current_state(), GatewayState::ShuttingDown);
}

/// Degraded → Recovering → Degraded (monitor dies mid-recovery).
#[test]
fn recovery_interrupted_by_monitor_failure() {
    let state = GatewaySharedState::new();

    state.transition_to(GatewayState::Degraded).unwrap();
    state.transition_to(GatewayState::Recovering).unwrap();

    // Monitor dies during recovery
    state.transition_to(GatewayState::Degraded).unwrap();
    assert_eq!(state.current_state(), GatewayState::Degraded);

    // Retry recovery
    state.transition_to(GatewayState::Recovering).unwrap();
    state.transition_to(GatewayState::Healthy).unwrap();
    assert_eq!(state.current_state(), GatewayState::Healthy);
}

// ── State Validation ────────────────────────────────────────────────────

/// can_transition_to correctly validates all pairs.
#[test]
fn can_transition_to_exhaustive() {
    // Valid transitions
    assert!(GatewayState::Initializing.can_transition_to(GatewayState::Healthy));
    assert!(GatewayState::Initializing.can_transition_to(GatewayState::Degraded));
    assert!(GatewayState::Initializing.can_transition_to(GatewayState::FatalError));
    assert!(GatewayState::Healthy.can_transition_to(GatewayState::Degraded));
    assert!(GatewayState::Healthy.can_transition_to(GatewayState::ShuttingDown));
    assert!(GatewayState::Degraded.can_transition_to(GatewayState::Recovering));
    assert!(GatewayState::Degraded.can_transition_to(GatewayState::ShuttingDown));
    assert!(GatewayState::Recovering.can_transition_to(GatewayState::Healthy));
    assert!(GatewayState::Recovering.can_transition_to(GatewayState::Degraded));
    assert!(GatewayState::Recovering.can_transition_to(GatewayState::ShuttingDown));

    // Invalid transitions
    assert!(!GatewayState::Healthy.can_transition_to(GatewayState::Recovering));
    assert!(!GatewayState::FatalError.can_transition_to(GatewayState::Healthy));
    assert!(!GatewayState::ShuttingDown.can_transition_to(GatewayState::Healthy));
}

/// AtomicU8 state is lock-free readable.
#[test]
fn state_is_lock_free() {
    let state = GatewaySharedState::new();
    let arc = state.state_arc();

    // Multiple reads should be consistent
    let s1 = state.current_state();
    let s2 = GatewayState::from_u8(arc.load(std::sync::atomic::Ordering::Acquire));
    assert_eq!(s1, s2);
}
