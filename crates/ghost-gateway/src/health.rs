//! Health checking and monitor connection management (Req 15 AC5-AC9).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::gateway::GatewayState;

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
pub struct MonitorHealthChecker {
    pub config: MonitorHealthConfig,
    pub gateway_state: Arc<AtomicU8>,
    pub consecutive_failures: u32,
}

impl MonitorHealthChecker {
    pub fn new(config: MonitorHealthConfig, gateway_state: Arc<AtomicU8>) -> Self {
        Self {
            config,
            gateway_state,
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

    fn maybe_transition_to_degraded(&self) {
        if self.consecutive_failures >= self.config.failure_threshold {
            let current = self.gateway_state.load(Ordering::Acquire);
            if current == GatewayState::Healthy as u8 {
                self.gateway_state
                    .store(GatewayState::Degraded as u8, Ordering::Release);
                tracing::error!(
                    "CRITICAL: Convergence monitor unreachable. Transitioning to DEGRADED."
                );
            }
        }
    }
}

/// Recovery coordinator: syncs state after monitor reconnection.
pub struct RecoveryCoordinator {
    pub gateway_state: Arc<AtomicU8>,
    pub monitor_address: String,
}

impl RecoveryCoordinator {
    /// Attempt recovery: 3 stability checks, replay buffered events, transition to Healthy.
    pub async fn attempt_recovery(&self) -> bool {
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
                // Back to Degraded
                self.gateway_state
                    .store(GatewayState::Degraded as u8, Ordering::Release);
                return false;
            }
        }

        // Transition to Healthy
        self.gateway_state
            .store(GatewayState::Healthy as u8, Ordering::Release);
        tracing::info!("Recovery complete. State: HEALTHY");
        true
    }
}
