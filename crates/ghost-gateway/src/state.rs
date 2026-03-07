//! Shared application state for all API route handlers.
//!
//! This is the keystone type that connects the gateway's runtime state
//! to every axum route handler via `State<Arc<AppState>>`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use tokio::sync::broadcast;

use crate::agents::registry::AgentRegistry;
use crate::api::websocket::{EventReplayBuffer, WsEnvelope};
use crate::cost::tracker::CostTracker;
use crate::db_pool::DbPool;
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

    /// SQLite connection pool: 1 writer + N readers (WAL mode).
    pub db: Arc<DbPool>,

    /// Broadcast channel for real-time events to WebSocket clients.
    pub event_tx: broadcast::Sender<WsEnvelope>,

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

    /// Soul drift threshold from security config (Finding #17).
    pub soul_drift_threshold: f64,

    /// Convergence profile name (Finding #18).
    pub convergence_profile: String,

    /// Model provider configurations (Finding #19).
    pub model_providers: Vec<crate::config::ProviderConfig>,

    /// Tool configurations (web_search, web_fetch, http_request, shell).
    pub tools_config: crate::config::ToolsConfig,

    /// Custom safety checks registered at runtime (T-4.3.2).
    pub custom_safety_checks: Arc<RwLock<Vec<crate::api::safety_checks::CustomSafetyCheck>>>,

    /// T-5.3.6: Cancellation token for graceful shutdown of background tasks.
    pub shutdown_token: tokio_util::sync::CancellationToken,

    /// T-5.3.6: JoinHandles for background tasks (convergence_watcher, backup_scheduler, etc.)
    pub background_tasks: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,

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

    /// Distributed kill remains feature-gated unless explicitly enabled.
    pub distributed_kill_enabled: bool,

    /// Embedding engine for memory vector search.
    pub embedding_engine: Arc<tokio::sync::Mutex<cortex_embeddings::EmbeddingEngine>>,

    /// Phase 5 safety skills — platform-managed, always active, cannot be uninstalled.
    /// Keyed by skill name for O(1) lookup in the execute endpoint.
    pub safety_skills: Arc<std::collections::HashMap<String, Box<dyn ghost_skills::skill::Skill>>>,

    /// WP9-L: Client heartbeat tracker — maps session_id to last heartbeat instant.
    /// Frontend POSTs every 30s; backend pauses SSE if stale >90s.
    pub client_heartbeats: Arc<dashmap::DashMap<String, std::time::Instant>>,

    /// WP9-D: Session TTL in days. Inactive sessions beyond this are soft-deleted.
    pub session_ttl_days: u32,
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
