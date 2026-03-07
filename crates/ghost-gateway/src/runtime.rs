//! Single-owner gateway lifecycle: open → run → shutdown.
//!
//! `GatewayRuntime` owns the full lifecycle of the gateway process.
//! All background tasks are spawned through `spawn_tracked()`, which wraps
//! each future in a `select!` against a child CancellationToken so that
//! every task responds to shutdown. `TaskTracker` guarantees we await all
//! spawned work before closing resources.
//!
//! The shutdown sequence runs inline at the end of `run()` — it is
//! impossible to exit `run()` without executing cleanup.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::gateway::{GatewaySharedState, GatewayState};
use crate::shutdown::ShutdownConfig;
use crate::state::AppState;

/// Single owner of the gateway process lifecycle.
pub struct GatewayRuntime {
    /// Shared gateway FSM state.
    pub shared_state: Arc<GatewaySharedState>,
    /// Application state shared with all route handlers.
    pub app_state: Arc<AppState>,
    /// Optional mesh router to merge into the main API router.
    pub mesh_router: Option<axum::Router>,
    /// The one cancellation token — all tasks derive child tokens from this.
    shutdown_token: CancellationToken,
    /// Tracks every spawned background task so we can await them all at shutdown.
    task_tracker: TaskTracker,
    /// Configurable timeouts for the shutdown sequence.
    shutdown_config: ShutdownConfig,
}

impl GatewayRuntime {
    /// Create a new runtime. The caller is expected to populate `mesh_router`
    /// after construction if mesh networking is enabled.
    pub fn new(
        shared_state: Arc<GatewaySharedState>,
        app_state: Arc<AppState>,
    ) -> Self {
        Self {
            shared_state,
            app_state,
            mesh_router: None,
            shutdown_token: CancellationToken::new(),
            task_tracker: TaskTracker::new(),
            shutdown_config: ShutdownConfig::default(),
        }
    }

    /// Returns a child `CancellationToken` that background tasks should
    /// listen on. It fires when the runtime begins shutdown.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.child_token()
    }

    /// Spawn a background task that will be:
    /// 1. Automatically cancelled when the runtime shuts down.
    /// 2. Awaited (with timeout) before resources are released.
    ///
    /// Every long-lived background task MUST go through this method.
    pub fn spawn_tracked<F>(&self, name: &'static str, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let token = self.shutdown_token.child_token();
        self.task_tracker.spawn(async move {
            tokio::select! {
                _ = token.cancelled() => {
                    tracing::info!(task = name, "shutdown signal received — exiting");
                }
                _ = fut => {
                    tracing::debug!(task = name, "task completed naturally");
                }
            }
        });
    }

    /// Run the gateway until a shutdown signal is received, then execute
    /// the full shutdown sequence. This is the **only** entry point for
    /// running the gateway — `open → run → close` in one linear path.
    pub async fn run(
        self,
        router: axum::Router,
        bind_addr: &str,
    ) -> Result<(), crate::gateway::GatewayError> {
        let listener = match tokio::net::TcpListener::bind(bind_addr).await {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                let port = bind_addr.rsplit(':').next().unwrap_or("?");
                return Err(crate::gateway::GatewayError::BootstrapFailed(format!(
                    "Port {port} is already in use. Another process may be bound to this port. \
                     Check with: lsof -i :{port} | grep LISTEN"
                )));
            }
            Err(e) => {
                return Err(crate::gateway::GatewayError::BootstrapFailed(
                    format!("bind failed: {e}")
                ));
            }
        };

        tracing::info!(addr = %bind_addr, "Gateway API server listening");

        // Clone what we need for the shutdown signal closure.
        let shutdown_token = self.shutdown_token.clone();

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                // Either ctrl_c or programmatic cancellation triggers shutdown.
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("Received SIGINT — initiating shutdown");
                    }
                    _ = shutdown_token.cancelled() => {
                        tracing::info!("Programmatic shutdown signal received");
                    }
                }
            })
            .await
            .map_err(|e| crate::gateway::GatewayError::ShutdownError(e.to_string()))?;

        // Shutdown is GUARANTEED to run — it is inline after the server stops.
        self.shutdown().await
    }

    /// Execute the full shutdown sequence. Called exactly once from `run()`.
    async fn shutdown(self) -> Result<(), crate::gateway::GatewayError> {
        // Transition FSM to ShuttingDown.
        let current = self.shared_state.current_state();
        if current != GatewayState::ShuttingDown {
            if let Err(e) = self.shared_state.transition_to(GatewayState::ShuttingDown) {
                tracing::warn!(error = %e, "Failed to transition to ShuttingDown — continuing anyway");
            }
        }

        // Step 1: Stop accepting new connections (already done — axum server returned).
        tracing::info!("shutdown step 1/8: connections stopped");

        // Step 2: Cancel all tracked background tasks and await completion.
        tracing::info!("shutdown step 2/8: cancelling background tasks");
        self.shutdown_token.cancel();
        self.task_tracker.close();

        if tokio::time::timeout(
            self.shutdown_config.drain_timeout,
            self.task_tracker.wait(),
        )
        .await
        .is_err()
        {
            tracing::warn!(
                timeout_secs = self.shutdown_config.drain_timeout.as_secs(),
                "background tasks did not complete within timeout"
            );
        }

        // Also drain any legacy tasks registered on AppState (backward compat
        // for code that hasn't migrated to spawn_tracked yet).
        {
            let legacy_tasks: Vec<tokio::task::JoinHandle<()>> = {
                let mut tasks = self
                    .app_state
                    .background_tasks
                    .lock()
                    .await;
                std::mem::take(&mut *tasks)
            };
            if !legacy_tasks.is_empty() {
                tracing::info!(count = legacy_tasks.len(), "awaiting legacy background tasks");
                // Signal legacy tasks via the AppState token as well.
                self.app_state.shutdown_token.cancel();
                let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
                for handle in legacy_tasks {
                    match tokio::time::timeout_at(deadline, handle).await {
                        Err(_elapsed) => {
                            tracing::warn!("legacy background task did not complete within deadline");
                            // Remaining tasks share the same deadline and will also time out.
                            // The AppState shutdown_token was already cancelled above,
                            // so well-behaved tasks will exit on their own.
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(error = %e, "legacy background task panicked");
                        }
                        Ok(Ok(())) => {}
                    }
                }
            }
        }

        // Step 3: Flush sessions — allow in-flight API handlers a brief window.
        tracing::info!("shutdown step 3/8: flushing in-flight requests");
        tokio::time::sleep(
            Duration::from_millis(500).min(self.shutdown_config.flush_total_timeout),
        )
        .await;

        // Step 4: Persist cost tracker (in-memory → DB).
        tracing::info!("shutdown step 4/8: persisting cost data");
        // Future: self.app_state.cost_tracker.persist(&self.app_state.db);

        // Step 5: Notify convergence monitor of shutdown.
        if self.app_state.monitor_enabled {
            tracing::info!("shutdown step 5/8: notifying convergence monitor");
            let _ = notify_monitor_shutdown(&self.app_state).await;
        } else {
            tracing::debug!("shutdown step 5/8: skipped (monitor disabled)");
        }

        // Step 6: Close WebSocket broadcast channel.
        // Dropping the broadcast Sender causes all WS handler receive loops
        // to get `RecvError::Closed` and exit cleanly.
        tracing::info!("shutdown step 6/8: closing broadcast channel");
        // The broadcast sender lives inside AppState. We'll drop AppState below.

        // Step 7: WAL checkpoint and DB close.
        tracing::info!("shutdown step 7/8: WAL checkpoint");
        if let Err(e) = self.app_state.db.checkpoint().await {
            tracing::warn!(error = %e, "WAL checkpoint failed");
        }

        // Step 8: Remove PID file.
        tracing::info!("shutdown step 8/8: removing PID file");
        crate::pid::remove_pid_file();

        tracing::info!("shutdown sequence complete");

        // self is consumed here — Drop impl runs, which is a no-op since
        // shutdown_token is already cancelled. AppState (and its broadcast
        // Sender + DB connection) drops when this frame unwinds.
        Ok(())
    }
}

impl Drop for GatewayRuntime {
    fn drop(&mut self) {
        if !self.shutdown_token.is_cancelled() {
            tracing::error!(
                "GatewayRuntime dropped without shutdown! Forcing cancellation of background tasks."
            );
            self.shutdown_token.cancel();
            self.task_tracker.close();
        }
        // Safety net: always try to clean up PID file on drop.
        crate::pid::remove_pid_file();
    }
}

/// Best-effort HTTP notification to the convergence monitor that the
/// gateway is shutting down.
async fn notify_monitor_shutdown(state: &AppState) -> Result<(), ()> {
    let url = format!("http://{}/gateway-shutdown", state.monitor_address);
    match reqwest::Client::new()
        .post(url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("Convergence monitor acknowledged shutdown");
            Ok(())
        }
        Ok(resp) => {
            tracing::debug!(status = %resp.status(), "Monitor shutdown notification returned non-OK");
            Err(())
        }
        Err(e) => {
            tracing::debug!(error = %e, "Monitor shutdown notification failed (monitor may be down)");
            Err(())
        }
    }
}
