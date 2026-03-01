//! Shared application state for all API route handlers.
//!
//! This is the keystone type that connects the gateway's runtime state
//! to every axum route handler via `State<Arc<AppState>>`.

use std::sync::{Arc, Mutex, RwLock};

use tokio::sync::broadcast;

use crate::agents::registry::AgentRegistry;
use crate::api::websocket::WsEvent;
use crate::cost::tracker::CostTracker;
use crate::gateway::GatewaySharedState;
use crate::safety::kill_gate_bridge::KillGateBridge;
use crate::safety::kill_switch::KillSwitch;
use crate::safety::quarantine::QuarantineManager;

/// Central application state shared across all API handlers.
///
/// Constructed during bootstrap, wrapped in `Arc`, and injected into
/// the axum router via `.with_state()`.
pub struct AppState {
    /// Gateway FSM state (Initializing, Healthy, Degraded, etc.)
    pub gateway: Arc<GatewaySharedState>,

    /// Live agent registry — populated during step 4 of bootstrap.
    pub agents: Arc<RwLock<AgentRegistry>>,

    /// Kill switch — 3-level safety system (Pause, Quarantine, KillAll).
    pub kill_switch: Arc<KillSwitch>,

    /// Quarantine manager — forensic state preservation.
    pub quarantine: Arc<RwLock<QuarantineManager>>,

    /// SQLite database connection for audit, sessions, proposals.
    pub db: Arc<Mutex<rusqlite::Connection>>,

    /// Broadcast channel for real-time events to WebSocket clients.
    pub event_tx: broadcast::Sender<WsEvent>,

    /// Cost tracker — per-agent daily totals, per-session totals.
    pub cost_tracker: Arc<CostTracker>,

    /// Distributed kill gate bridge (local KillSwitch + distributed coordination).
    pub kill_gate: Option<Arc<RwLock<KillGateBridge>>>,

    /// Secret provider for credential management (Phase 10).
    pub secret_provider: Arc<dyn ghost_secrets::SecretProvider>,

    /// OAuth broker for third-party API connections (Phase 12).
    pub oauth_broker: Arc<ghost_oauth::OAuthBroker>,

    /// Soul drift threshold from security config (Finding #17).
    pub soul_drift_threshold: f64,

    /// Convergence profile name (Finding #18).
    pub convergence_profile: String,

    /// Model provider configurations (Finding #19).
    pub model_providers: Vec<crate::config::ProviderConfig>,

    /// Custom safety checks registered at runtime (T-4.3.2).
    pub custom_safety_checks: Arc<RwLock<Vec<crate::api::safety_checks::CustomSafetyCheck>>>,

    /// T-5.3.6: Cancellation token for graceful shutdown of background tasks.
    pub shutdown_token: tokio_util::sync::CancellationToken,

    /// T-5.3.6: JoinHandles for background tasks (convergence_watcher, backup_scheduler, etc.)
    pub background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,

    /// T-5.11.2: Safety endpoint cooldown tracker (3 actions / 10 min → 5 min cooldown).
    pub safety_cooldown: Arc<crate::api::rate_limit::SafetyCooldown>,
}
