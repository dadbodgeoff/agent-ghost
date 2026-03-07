//! E2E: Gateway shutdown with in-flight work.
//!
//! Validates graceful shutdown: ShuttingDown state prevents new work,
//! in-flight compaction aborts cleanly, state transitions are terminal.

use ghost_gateway::gateway::{GatewaySharedState, GatewayState};
use ghost_gateway::session::compaction::{CompactionConfig, SessionCompactor};
use std::sync::atomic::AtomicBool;

/// Gateway enters ShuttingDown from Healthy.
#[test]
fn shutdown_from_healthy() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Healthy).unwrap();
    state.transition_to(GatewayState::ShuttingDown).unwrap();
    assert_eq!(state.current_state(), GatewayState::ShuttingDown);
}

/// Gateway enters ShuttingDown from Degraded.
#[test]
fn shutdown_from_degraded() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Degraded).unwrap();
    state.transition_to(GatewayState::ShuttingDown).unwrap();
    assert_eq!(state.current_state(), GatewayState::ShuttingDown);
}

/// Gateway enters ShuttingDown from Recovering.
#[test]
fn shutdown_from_recovering() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Degraded).unwrap();
    state.transition_to(GatewayState::Recovering).unwrap();
    state.transition_to(GatewayState::ShuttingDown).unwrap();
    assert_eq!(state.current_state(), GatewayState::ShuttingDown);
}

/// ShuttingDown is terminal — no further transitions allowed.
#[test]
fn shutdown_is_terminal() {
    let state = GatewaySharedState::new();
    state.transition_to(GatewayState::Healthy).unwrap();
    state.transition_to(GatewayState::ShuttingDown).unwrap();

    // All transitions from ShuttingDown should be invalid
    assert!(!GatewayState::ShuttingDown.can_transition_to(GatewayState::Healthy));
    assert!(!GatewayState::ShuttingDown.can_transition_to(GatewayState::Degraded));
    assert!(!GatewayState::ShuttingDown.can_transition_to(GatewayState::Recovering));
    assert!(!GatewayState::ShuttingDown.can_transition_to(GatewayState::FatalError));
    assert!(!GatewayState::ShuttingDown.can_transition_to(GatewayState::Initializing));
}

/// In-flight compaction aborts on shutdown signal.
#[test]
fn inflight_compaction_aborts_on_shutdown() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let shutdown_signal = AtomicBool::new(false);

    let mut history: Vec<String> = (0..20)
        .map(|i| {
            format!(
                "Message {} with content for compaction testing during shutdown scenario",
                i
            )
        })
        .collect();
    let snapshot = history.clone();

    // Set shutdown signal before compaction
    shutdown_signal.store(true, std::sync::atomic::Ordering::SeqCst);

    let result = compactor.compact(&mut history, 1, Some(&shutdown_signal));
    assert!(
        result.is_err(),
        "Compaction should abort when shutdown signal is set"
    );

    // History must be unchanged (rollback)
    assert_eq!(history, snapshot, "History must be rolled back on abort");
}

/// Shutdown signal set mid-lifecycle: gateway transitions correctly.
#[test]
fn shutdown_mid_lifecycle() {
    let state = GatewaySharedState::new();

    // Normal lifecycle
    state.transition_to(GatewayState::Healthy).unwrap();
    assert_eq!(state.current_state(), GatewayState::Healthy);

    // Simulate degraded mode
    state.transition_to(GatewayState::Degraded).unwrap();

    // Shutdown requested during degraded mode
    state.transition_to(GatewayState::ShuttingDown).unwrap();
    assert_eq!(state.current_state(), GatewayState::ShuttingDown);
}
