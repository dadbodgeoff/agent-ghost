//! Shared application state for all API route handlers.
//!
//! This is the keystone type that connects the gateway's runtime state
//! to every axum route handler via `State<Arc<AppState>>`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use cortex_core::safety::trigger::TriggerEvent;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agents::registry::AgentRegistry;
use crate::api::websocket::{EventReplayBuffer, WsEnvelope};
use crate::autonomy::AutonomyService;
use crate::channel_manager::ChannelManager;
use crate::cost::tracker::CostTracker;
use crate::db_pool::DbPool;
use crate::gateway::GatewaySharedState;
use crate::itp_router::ITPEventRouter;
use crate::safety::kill_gate_bridge::KillGateBridge;
use crate::safety::kill_switch::KillSwitch;
use crate::safety::quarantine::QuarantineManager;
use ghost_agent_loop::itp_emitter::ITPEmitter;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RuntimeSubsystemStatus {
    Healthy,
    Degraded,
    Disabled,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, Default)]
pub struct BackupSchedulerRuntimeStatus {
    pub enabled: bool,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_error: Option<String>,
    pub status: RuntimeSubsystemStatus,
}

#[derive(Debug, Clone, Default)]
pub struct ConfigWatcherRuntimeStatus {
    pub enabled: bool,
    pub mode: Option<String>,
    pub watched_path: Option<String>,
    pub last_reload_at: Option<String>,
    pub last_error: Option<String>,
    pub status: RuntimeSubsystemStatus,
}

#[derive(Debug, Clone, Default)]
pub struct MonitorRuntimeStatus {
    pub sampled_at: Option<String>,
    pub connected: bool,
    pub uptime_seconds: Option<u64>,
    pub agent_count: Option<usize>,
    pub event_count: Option<u64>,
    pub last_computation: Option<String>,
    pub last_error: Option<String>,
}

/// Central application state shared across all API handlers.
///
/// Constructed during bootstrap, wrapped in `Arc`, and injected into
/// the axum router via `.with_state()`.
pub struct AppState {
    /// Gateway process start instant for uptime reporting.
    pub started_at: Instant,

    /// Gateway FSM state (Initializing, Healthy, Degraded, etc.)
    pub gateway: Arc<GatewaySharedState>,

    /// Resolved config path for the running gateway instance.
    pub config_path: std::path::PathBuf,

    /// Live agent registry — populated during step 4 of bootstrap.
    pub agents: Arc<RwLock<AgentRegistry>>,

    /// Canonical channel lifecycle and routing authority.
    pub channel_manager: Arc<ChannelManager>,

    /// Kill switch — 3-level safety system (Pause, Quarantine, KillAll).
    pub kill_switch: Arc<KillSwitch>,

    /// Quarantine manager — forensic state preservation.
    pub quarantine: Arc<RwLock<QuarantineManager>>,

    /// SQLite connection pool: 1 writer + N readers (WAL mode).
    pub db: Arc<DbPool>,

    /// Broadcast channel for real-time events to WebSocket clients.
    pub event_tx: broadcast::Sender<WsEnvelope>,

    /// Shared trigger channel feeding automatic safety evaluation.
    pub trigger_sender: tokio::sync::mpsc::Sender<TriggerEvent>,

    /// Interactive sandbox review queue + approval coordinator.
    pub sandbox_reviews: Arc<crate::sandbox_reviews::SandboxReviewCoordinator>,

    /// Shared ADE-side ITP emitter attached to live runners when tracking is enabled.
    pub itp_emitter: Option<ITPEmitter>,

    /// Shared router for monitor delivery, degraded buffering, and replay.
    pub itp_router: Option<Arc<ITPEventRouter>>,

    /// Gateway-owned active session tracker for ADE ITP lifecycle management.
    pub itp_session_tracker: Option<Arc<crate::itp_bridge::ITPSessionTracker>>,

    /// Ring buffer for event replay on WebSocket reconnect (Task 1.6).
    pub replay_buffer: Arc<EventReplayBuffer>,

    /// Cost tracker — per-agent daily totals, per-session totals.
    pub cost_tracker: Arc<CostTracker>,

    /// Distributed kill gate bridge (local KillSwitch + distributed coordination).
    pub kill_gate: Option<Arc<RwLock<KillGateBridge>>>,

    /// Secret provider for credential management (Phase 10).
    pub secret_provider: Arc<dyn ghost_secrets::SecretProvider>,

    /// OAuth broker for third-party API connections (Phase 12).
    pub oauth_broker: Arc<ghost_oauth::OAuthBroker>,

    /// Local mesh signing key used for A2A card signing and outbound dispatch auth.
    pub mesh_signing_key: Option<Arc<std::sync::Mutex<ghost_signing::SigningKey>>>,

    /// Soul drift threshold from security config (Finding #17).
    pub soul_drift_threshold: f64,

    /// Convergence profile name (Finding #18).
    pub convergence_profile: String,

    /// Model provider configurations (Finding #19).
    pub model_providers: Vec<crate::config::ProviderConfig>,
    /// Preferred provider name when multiple providers are configured.
    pub default_model_provider: Option<String>,

    /// Shared PC control circuit breaker instance used by registered skills.
    pub pc_control_circuit_breaker:
        Arc<std::sync::Mutex<ghost_pc_control::safety::PcControlCircuitBreaker>>,

    /// Canonical live PC control runtime state and policy handle.
    pub pc_control_runtime: Arc<crate::pc_control_runtime::PcControlRuntimeService>,

    /// Single-use WebSocket upgrade tickets keyed by a token hash.
    pub websocket_auth_tickets: Arc<dashmap::DashMap<String, crate::api::websocket::WsAuthTicket>>,

    /// Shared WebSocket connection tracker used by the WS upgrade route and observability.
    pub ws_connection_tracker: Arc<crate::api::websocket::WsConnectionTracker>,

    /// Require short-lived WebSocket tickets and reject legacy bearer upgrade auth.
    pub ws_ticket_auth_only: bool,

    /// Tool configurations (web_search, web_fetch, http_request, shell).
    pub tools_config: crate::config::ToolsConfig,

    /// Custom safety checks registered at runtime (T-4.3.2).
    pub custom_safety_checks: Arc<RwLock<Vec<crate::api::safety_checks::CustomSafetyCheck>>>,

    /// T-5.3.6: Cancellation token for graceful shutdown of background tasks.
    pub shutdown_token: tokio_util::sync::CancellationToken,

    /// T-5.3.6: JoinHandles for background tasks (convergence_watcher, backup_scheduler, etc.)
    pub background_tasks: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,

    /// Active live execution cancellation controls keyed by execution id.
    pub live_execution_controls: Arc<dashmap::DashMap<String, Arc<CancellationToken>>>,

    /// T-5.11.2: Safety endpoint cooldown tracker (3 actions / 10 min → 5 min cooldown).
    pub safety_cooldown: Arc<crate::api::rate_limit::SafetyCooldown>,

    /// Convergence monitor address from config (e.g. "127.0.0.1:18790").
    pub monitor_address: String,

    /// Whether convergence monitor health checking is enabled.
    pub monitor_enabled: bool,

    /// Whether degraded convergence protection blocks execution.
    pub monitor_block_on_degraded: bool,

    /// Maximum acceptable age of convergence state before it is stale.
    pub convergence_state_stale_after: std::time::Duration,

    /// Convergence monitor liveness flag — updated every 30s by MonitorHealthChecker.
    /// O(1) read, no lock, no disk I/O. Safe for high-frequency health probes.
    pub monitor_healthy: Arc<std::sync::atomic::AtomicBool>,

    /// Cached monitor runtime snapshot refreshed periodically by the gateway.
    pub monitor_runtime_status: Arc<RwLock<MonitorRuntimeStatus>>,

    /// Distributed kill remains feature-gated unless explicitly enabled.
    pub distributed_kill_enabled: bool,

    /// Embedding engine for memory vector search.
    pub embedding_engine: Arc<tokio::sync::Mutex<cortex_embeddings::EmbeddingEngine>>,

    /// Canonical compiled-skill catalog and install-state authority.
    pub skill_catalog: Arc<crate::skill_catalog::SkillCatalogService>,

    /// WP9-L: Client heartbeat tracker — maps session_id to last heartbeat instant.
    /// Frontend POSTs every 30s; backend pauses SSE if stale >90s.
    pub client_heartbeats: Arc<dashmap::DashMap<String, std::time::Instant>>,

    /// WP9-D: Session TTL in days. Inactive sessions beyond this are soft-deleted.
    pub session_ttl_days: u32,

    /// Backup scheduler runtime health and last-run bookkeeping.
    pub backup_scheduler_status: Arc<RwLock<BackupSchedulerRuntimeStatus>>,

    /// Config watcher runtime health and reload bookkeeping.
    pub config_watcher_status: Arc<RwLock<ConfigWatcherRuntimeStatus>>,

    /// Canonical gateway-owned autonomy control plane.
    pub autonomy: Arc<AutonomyService>,
}

pub struct LiveExecutionControlGuard {
    state: Arc<AppState>,
    execution_id: String,
    token: Arc<CancellationToken>,
}

impl Drop for LiveExecutionControlGuard {
    fn drop(&mut self) {
        if let Some(entry) = self.state.live_execution_controls.get(&self.execution_id) {
            let should_remove = Arc::ptr_eq(entry.value(), &self.token);
            drop(entry);
            if should_remove {
                self.state
                    .live_execution_controls
                    .remove(&self.execution_id);
            }
        }
    }
}

impl AppState {
    pub fn sync_agent_access_pullbacks(&self) -> Result<Vec<Uuid>, String> {
        let kill_state = self.kill_switch.current_state();
        let mut agents = self
            .agents
            .write()
            .map_err(|_| "agent registry lock poisoned".to_string())?;
        Ok(agents.sync_access_pullbacks(&kill_state))
    }

    pub fn acquire_live_execution_control(
        self: &Arc<Self>,
        execution_id: impl Into<String>,
    ) -> (Arc<CancellationToken>, LiveExecutionControlGuard) {
        let execution_id = execution_id.into();
        let token = Arc::new(self.shutdown_token.child_token());
        if let Some(previous) = self
            .live_execution_controls
            .insert(execution_id.clone(), Arc::clone(&token))
        {
            previous.cancel();
        }
        (
            Arc::clone(&token),
            LiveExecutionControlGuard {
                state: Arc::clone(self),
                execution_id,
                token,
            },
        )
    }

    pub fn cancel_live_execution(&self, execution_id: &str) -> bool {
        if let Some(token) = self.live_execution_controls.get(execution_id) {
            token.cancel();
            true
        } else {
            false
        }
    }
}

// ── Thread-safe API key store ────────────────────────────────────────
// Replaces `unsafe { std::env::set_var() }` with a lock-free-read,
// write-guarded store. Falls back to env vars for keys not explicitly set.

static API_KEYS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

fn key_store() -> &'static RwLock<HashMap<String, String>> {
    API_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Set an API key in the thread-safe store (replaces unsafe env::set_var).
pub fn set_api_key(name: &str, value: &str) {
    if let Ok(mut map) = key_store().write() {
        map.insert(name.to_string(), value.to_string());
    }
}

/// Remove an API key from the thread-safe store (replaces unsafe env::remove_var).
pub fn remove_api_key(name: &str) {
    if let Ok(mut map) = key_store().write() {
        map.remove(name);
    }
}

/// Get an API key: checks thread-safe store first, falls back to env var.
pub fn get_api_key(name: &str) -> Option<String> {
    if let Ok(map) = key_store().read() {
        if let Some(val) = map.get(name) {
            return Some(val.clone());
        }
    }
    std::env::var(name).ok().filter(|v| !v.is_empty())
}
