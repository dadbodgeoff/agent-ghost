//! Gateway state machine and top-level coordinator (Req 15).
//!
//! 6-state FSM: Initializing, Healthy, Degraded, Recovering, ShuttingDown, FatalError.
//! State stored as AtomicU8 for lock-free reads from health endpoints and ITP emitters.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use thiserror::Error;

/// The 6 gateway states. Stored as AtomicU8 for lock-free reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GatewayState {
    /// Bootstrap sequence in progress. No traffic accepted.
    Initializing = 0,
    /// All subsystems operational. Convergence monitor reachable.
    Healthy = 1,
    /// Gateway operational but convergence monitor unreachable.
    Degraded = 2,
    /// Monitor reconnected. Syncing missed state before returning to Healthy.
    Recovering = 3,
    /// Graceful shutdown in progress. Terminal state.
    ShuttingDown = 4,
    /// Fatal error during bootstrap. Terminal state.
    FatalError = 5,
}

impl GatewayState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Initializing,
            1 => Self::Healthy,
            2 => Self::Degraded,
            3 => Self::Recovering,
            4 => Self::ShuttingDown,
            5 => Self::FatalError,
            _ => Self::FatalError,
        }
    }

    /// Returns true if the transition from `self` to `to` is legal.
    pub fn can_transition_to(self, to: GatewayState) -> bool {
        matches!(
            (self, to),
            (Self::Initializing, Self::Healthy)
                | (Self::Initializing, Self::Degraded)
                | (Self::Initializing, Self::FatalError)
                | (Self::Healthy, Self::Degraded)
                | (Self::Healthy, Self::ShuttingDown)
                | (Self::Degraded, Self::Recovering)
                | (Self::Degraded, Self::ShuttingDown)
                | (Self::Recovering, Self::Healthy)
                | (Self::Recovering, Self::Degraded)
                | (Self::Recovering, Self::ShuttingDown)
        )
    }
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("illegal state transition: {from:?} -> {to:?}")]
    IllegalTransition {
        from: GatewayState,
        to: GatewayState,
    },
    #[error("bootstrap failed: {0}")]
    BootstrapFailed(String),
    #[error("shutdown error: {0}")]
    ShutdownError(String),
}

/// Shared gateway state accessible from all subsystems.
pub struct GatewaySharedState {
    state: Arc<AtomicU8>,
}

impl GatewaySharedState {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(GatewayState::Initializing as u8)),
        }
    }

    pub fn current_state(&self) -> GatewayState {
        GatewayState::from_u8(self.state.load(Ordering::Acquire))
    }

    /// Attempt a state transition. Returns error on illegal transition.
    /// In debug builds, illegal transitions panic. In release, they log and return error.
    pub fn transition_to(&self, to: GatewayState) -> Result<(), GatewayError> {
        let from = self.current_state();
        if !from.can_transition_to(to) {
            let err = GatewayError::IllegalTransition { from, to };
            #[cfg(debug_assertions)]
            panic!("{}", err);
            #[cfg(not(debug_assertions))]
            {
                tracing::error!(%err, "Illegal state transition ignored");
                return Err(err);
            }
        }
        self.state.store(to as u8, Ordering::Release);
        tracing::info!(from = ?from, to = ?to, "Gateway state transition");
        Ok(())
    }

    pub fn state_arc(&self) -> Arc<AtomicU8> {
        Arc::clone(&self.state)
    }
}

impl Default for GatewaySharedState {
    fn default() -> Self {
        Self::new()
    }
}

/// The top-level gateway coordinator.
pub struct Gateway {
    pub shared_state: GatewaySharedState,
}

impl Gateway {
    pub fn new(shared_state: GatewaySharedState) -> Self {
        Self { shared_state }
    }

    /// Run the gateway event loop with the API server until shutdown.
    pub async fn run(self) -> Result<(), GatewayError> {
        self.run_with_router(None, None).await
    }

    /// Run the gateway with an optional pre-built router and bind address.
    pub async fn run_with_router(
        self,
        router: Option<axum::Router>,
        bind_addr: Option<&str>,
    ) -> Result<(), GatewayError> {
        let addr = bind_addr.unwrap_or("127.0.0.1:18789");

        if let Some(router) = router {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| GatewayError::BootstrapFailed(format!("bind failed: {e}")))?;

            tracing::info!(addr = %addr, "Gateway API server listening");

            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.ok();
                    tracing::info!("Received shutdown signal");
                })
                .await
                .map_err(|e| GatewayError::ShutdownError(e.to_string()))?;
        } else {
            tracing::info!(state = ?self.shared_state.current_state(), "Gateway running (no API server)");
            tokio::signal::ctrl_c()
                .await
                .map_err(|e| GatewayError::ShutdownError(e.to_string()))?;
            tracing::info!("Received shutdown signal");
        }

        // Only transition if not already shutting down.
        let current = self.shared_state.current_state();
        if current != GatewayState::ShuttingDown {
            self.shared_state.transition_to(GatewayState::ShuttingDown)?;
        }

        Ok(())
    }
}
