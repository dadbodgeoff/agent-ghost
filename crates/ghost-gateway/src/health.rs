//! Health checking and monitor connection management (Req 15 AC5-AC9).
//!
//! All state transitions go through `GatewaySharedState::transition_to()`
//! to enforce the 6-state FSM. Direct AtomicU8 writes are FORBIDDEN.

use std::sync::Arc;
use std::time::Duration;

use crate::gateway::{GatewayError, GatewaySharedState, GatewayState};
use crate::itp_router::ITPEventRouter;

/// Connection state to the convergence monitor.
#[derive(Debug, Clone)]
pub enum MonitorConnection {
    Connected { version: String },
    Unreachable { reason: String },
}

/// Configuration for monitor health checking.
#[derive(Debug, Clone)]
pub struct MonitorHealthConfig {
    pub address: String,
    pub check_interval: Duration,
    pub failure_threshold: u32,
    pub check_timeout: Duration,
}

impl Default for MonitorHealthConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:18790".into(),
            check_interval: Duration::from_secs(30),
            failure_threshold: 3,
            check_timeout: Duration::from_secs(5),
        }
    }
}

/// Periodic health checker for the convergence monitor.
///
/// Uses `GatewaySharedState` for all transitions — never writes to
/// AtomicU8 directly, ensuring FSM validation is always enforced.
pub struct MonitorHealthChecker {
    pub config: MonitorHealthConfig,
    pub shared_state: Arc<GatewaySharedState>,
    pub consecutive_failures: u32,
}

impl MonitorHealthChecker {
    pub fn new(config: MonitorHealthConfig, shared_state: Arc<GatewaySharedState>) -> Self {
        Self {
            config,
            shared_state,
            consecutive_failures: 0,
        }
    }

    /// Run a single health check. Returns true if healthy.
    pub async fn check_once(&mut self) -> bool {
        let url = format!("http://{}/health", self.config.address);
        let result = tokio::time::timeout(
            self.config.check_timeout,
            reqwest::Client::new().get(&url).send(),
        )
        .await;

        match result {
            Ok(Ok(resp)) if resp.status().is_success() => {
                if self.consecutive_failures > 0 {
                    tracing::info!(
                        previous_failures = self.consecutive_failures,
                        "Monitor health check recovered"
                    );
                }
                self.consecutive_failures = 0;
                true
            }
            _ => {
                self.consecutive_failures += 1;
                tracing::warn!(
                    consecutive_failures = self.consecutive_failures,
                    "Monitor health check failed"
                );
                self.maybe_transition_to_degraded();
                false
            }
        }
    }

    /// Transition to Degraded via validated FSM path.
    ///
    /// Handles both Healthy → Degraded and Recovering → Degraded.
    /// If the gateway is in Recovering state and the monitor goes down
    /// again, we must transition back to Degraded (the FSM allows it).
    fn maybe_transition_to_degraded(&self) {
        if self.consecutive_failures >= self.config.failure_threshold {
            let current = self.shared_state.current_state();
            if current == GatewayState::Healthy || current == GatewayState::Recovering {
                if let Err(e) = self.shared_state.transition_to(GatewayState::Degraded) {
                    tracing::error!(
                        error = %e,
                        from = ?current,
                        "Failed to transition to DEGRADED — FSM rejected"
                    );
                } else {
                    tracing::error!(
                        from = ?current,
                        "CRITICAL: Convergence monitor unreachable. Transitioned to DEGRADED."
                    );
                }
            }
        }
    }

    /// Public wrapper for testing `maybe_transition_to_degraded`.
    /// Production code calls the private method via `check_once`.
    #[doc(hidden)]
    pub fn maybe_transition_to_degraded_public(&self) {
        self.maybe_transition_to_degraded();
    }
}

/// Recovery coordinator: syncs state after monitor reconnection.
///
/// Uses `GatewaySharedState` for all transitions — never writes to
/// AtomicU8 directly, ensuring FSM validation is always enforced.
pub struct RecoveryCoordinator {
    pub shared_state: Arc<GatewaySharedState>,
    pub monitor_address: String,
    pub itp_router: Option<Arc<ITPEventRouter>>,
}

impl RecoveryCoordinator {
    /// Attempt recovery: 3 stability checks, replay buffered events, transition to Healthy.
    ///
    /// FSM path: Recovering → Healthy (on success) or Recovering → Degraded (on failure).
    /// Both transitions are validated by `GatewaySharedState::transition_to()`.
    pub async fn attempt_recovery(&self) -> Result<bool, GatewayError> {
        // 3 stability checks, 5s apart
        for i in 1..=3 {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let url = format!("http://{}/health", self.monitor_address);
            let ok = reqwest::Client::new()
                .get(&url)
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if !ok {
                tracing::warn!(check = i, "Recovery stability check failed, aborting");
                // Back to Degraded via validated FSM transition
                self.shared_state.transition_to(GatewayState::Degraded)?;
                return Ok(false);
            }
        }

        if let Some(router) = &self.itp_router {
            let remaining = router.replay_buffered().await;
            if remaining > 0 {
                tracing::warn!(
                    remaining,
                    "Recovery replay left buffered ITP events pending, returning to degraded"
                );
                self.shared_state.transition_to(GatewayState::Degraded)?;
                return Ok(false);
            }
        }

        // Transition to Healthy via validated FSM transition
        self.shared_state.transition_to(GatewayState::Healthy)?;
        tracing::info!("Recovery complete. State: HEALTHY");
        Ok(true)
    }
}
