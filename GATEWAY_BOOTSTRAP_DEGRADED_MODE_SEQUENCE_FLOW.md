# Gateway Bootstrap + Degraded Mode Transitions — Complete Sequence Flow

> Date: 2026-02-27
> Scope: `ghost-gateway/src/bootstrap.rs`, `ghost-gateway/src/health.rs`, `ghost-gateway/src/shutdown.rs`, `ghost-gateway/src/gateway.rs`
> Resolves: FILE_MAPPING Finding 17 (monitor crash recovery underspecified)
> Cross-references:
>   - AGENT_ARCHITECTURE.md §17 (Error Handling), §20 (Kill Switch)
>   - AGENT_ARCHITECTURE_v2.md §3 (Convergence Safety System — 7 signals, 5 levels, memory filtering tiers)
>   - FILE_MAPPING.md §ghost-gateway, §convergence-monitor, §ghost-policy, §ghost-heartbeat, §ghost-agent-loop
>   - CONVERGENCE_MONITOR_SEQUENCE_FLOW.md Phase 0 (Startup Handshake), §8.4 (Monitor Crashes Mid-Session)
>   - KILL_SWITCH_TRIGGER_CHAIN_SEQUENCE_FLOW.md §2.8 T7 (Memory Health — Path C degraded fallback)
>   - AGENT_LOOP_SEQUENCE_FLOW.md (degraded mode error classification table)
> Prerequisite reading: FILE_MAPPING.md (full), AGENT_ARCHITECTURE_v2.md §3 (Convergence Safety System)

---

## 1. STATE MACHINE: GatewayState

The gateway is a finite state machine. Every subsystem reads `GatewayState` to decide behavior.
There are exactly 6 states. No implicit states. No "sort of degraded." Every state has defined
entry conditions, exit conditions, and behavioral contracts.

```
                         ┌──────────────┐
                         │  INITIALIZING │
                         │  (entry state)│
                         └──────┬───────┘
                                │
                    bootstrap sequence runs
                    (5 steps, any can fail)
                                │
                 ┌──────────────┼──────────────┐
                 │              │              │
          step 3 fails    all 5 pass    steps 1,2,4,5 fail
          (monitor only)       │         (fatal errors)
                 │              │              │
                 ▼              ▼              ▼
          ┌──────────┐  ┌──────────┐  ┌──────────────┐
          │ DEGRADED  │  │ HEALTHY  │  │ FATAL_ERROR  │
          │           │  │          │  │ (terminal)   │
          └─────┬─────┘  └────┬─────┘  └──────────────┘
                │              │
                │    monitor dies mid-session
                │         ┌────┘
                │         ▼
                │  ┌──────────┐
                │  │ DEGRADED  │◄─── can also enter from RECOVERING
                │  └─────┬────┘     if reconnection fails again
                │        │
                │   monitor comes back
                │   (health check passes)
                │        │
                │        ▼
                │  ┌───────────┐
                │  │ RECOVERING │
                │  └─────┬─────┘
                │        │
                │   state sync completes
                │        │
                │        ▼
                └──►┌──────────┐
                    │ HEALTHY  │
                    └────┬─────┘
                         │
              SIGTERM/SIGINT/kill switch
                         │
                         ▼
                  ┌─────────────┐
                  │ SHUTTING_DOWN│
                  │  (terminal)  │
                  └─────────────┘
```

### State Definitions

```rust
/// ghost-gateway/src/gateway.rs
///
/// The 6 gateway states. Stored as AtomicU8 for lock-free reads
/// from health endpoints and ITP emitters.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GatewayState {
    /// Bootstrap sequence in progress. No traffic accepted.
    Initializing = 0,

    /// All subsystems operational. Convergence monitor reachable.
    /// Full feature set available.
    Healthy = 1,

    /// Gateway operational but convergence monitor unreachable.
    /// Agents run. Convergence scoring disabled. Safety floor absent.
    /// Logged as CRITICAL. Periodic reconnection attempts active.
    Degraded = 2,

    /// Monitor reconnected. Syncing missed state before returning to Healthy.
    /// Agents continue running with degraded feature set during sync.
    Recovering = 3,

    /// Graceful shutdown in progress. No new connections accepted.
    /// Draining existing work. Terminal state.
    ShuttingDown = 4,

    /// Fatal error during bootstrap. Process will exit.
    /// Terminal state. Only reached if steps 1, 2, 4, or 5 fail.
    FatalError = 5,
}
```

### State Transition Table (Exhaustive)

Every legal transition. If a transition is not in this table, it is ILLEGAL and must panic in debug / log + ignore in release.

| From | To | Trigger | Guard Condition |
|------|----|---------|-----------------|
| `Initializing` | `Healthy` | Bootstrap steps 1-5 all pass | Monitor health check returns 200 |
| `Initializing` | `Degraded` | Bootstrap step 3 fails (monitor unreachable) | Steps 1,2,4,5 passed. Step 3 timed out or got connection refused |
| `Initializing` | `FatalError` | Any of steps 1,2,4,5 fail | Config invalid, migration failed, agent registry failed, or API server bind failed |
| `Healthy` | `Degraded` | Periodic health check detects monitor unreachable | N consecutive health checks fail (N = `monitor_failure_threshold`, default 3) |
| `Healthy` | `ShuttingDown` | SIGTERM, SIGINT, or kill switch Level 3 | Signal received or kill switch API called |
| `Degraded` | `Recovering` | Periodic reconnection attempt succeeds | Monitor `/health` returns 200 |
| `Degraded` | `ShuttingDown` | SIGTERM, SIGINT, or kill switch | Signal received or kill switch API called |
| `Recovering` | `Healthy` | State sync completes successfully | Missed convergence scores backfilled, ITP event gap reconciled |
| `Recovering` | `Degraded` | State sync fails or monitor dies again during sync | Monitor becomes unreachable during recovery window |
| `Recovering` | `ShuttingDown` | SIGTERM, SIGINT, or kill switch | Signal received or kill switch API called |
| `FatalError` | (process exit) | Immediate | Cleanup attempted, then `std::process::exit(1)` |
| `ShuttingDown` | (process exit) | Shutdown sequence completes or 60s timeout | All steps done or forced exit |

Transitions NOT allowed (examples of what must be rejected):
- `FatalError` → anything (terminal)
- `ShuttingDown` → anything except exit (terminal)
- `Healthy` → `Recovering` (must go through `Degraded` first)
- `Degraded` → `Healthy` (must go through `Recovering` for state sync)
- `Initializing` → `Recovering` (never been healthy, nothing to recover)

---

## 2. BOOTSTRAP SEQUENCE (Initializing → Healthy | Degraded | FatalError)

### File: `ghost-gateway/src/bootstrap.rs`

The bootstrap is a linear 5-step sequence. Steps are NOT parallelized because they have
ordering dependencies. Each step either succeeds or the entire bootstrap fails — EXCEPT
step 3 (monitor health), which degrades gracefully.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    BOOTSTRAP SEQUENCE                                │
│                    State: INITIALIZING                               │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ STEP 1: Load + Validate ghost.yml                           │   │
│  │                                                             │   │
│  │ 1a. Resolve config path:                                    │   │
│  │     CLI arg --config > env GHOST_CONFIG > ~/.ghost/config/  │   │
│  │     ghost.yml > ./ghost.yml                                 │   │
│  │ 1b. Parse YAML (serde_yaml)                                 │   │
│  │ 1c. Substitute env vars (${VAR} syntax)                     │   │
│  │ 1d. Validate against JSON schema (ghost-config.schema.json) │   │
│  │ 1e. Validate convergence profile exists (default: "standard")│  │
│  │ 1f. Validate all referenced files exist (SOUL.md, etc.)     │   │
│  │                                                             │   │
│  │ ON FAILURE: FatalError                                      │   │
│  │   Log: "FATAL: Configuration invalid: {details}"            │   │
│  │   Exit code: 78 (EX_CONFIG from sysexits)                  │   │
│  └──────────────────────────┬──────────────────────────────────┘   │
│                             │ success                               │
│                             ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ STEP 2: Run Database Migrations                             │   │
│  │                                                             │   │
│  │ 2a. Open SQLite connection to ~/.ghost/data/ghost.db        │   │
│  │ 2b. Enable WAL mode, set busy_timeout(5000)                 │   │
│  │ 2c. Call cortex_storage::run_migrations()                   │   │
│  │     - Forward-only. No rollback. Migrations are idempotent. │   │
│  │     - Runs v001 through v017 (or LATEST_VERSION)            │   │
│  │ 2d. Verify: SELECT sqlite_version(), PRAGMA integrity_check │   │
│  │ 2e. Verify: hash chain genesis exists (v016 marker)         │   │
│  │                                                             │   │
│  │ CRITICAL ORDERING CONSTRAINT:                               │   │
│  │   Migrations MUST complete before Step 3 (monitor health    │   │
│  │   check). The convergence monitor reads from the SAME       │   │
│  │   SQLite DB — specifically the convergence tables created    │   │
│  │   by v017 migration (itp_events, convergence_scores,        │   │
│  │   intervention_history, goal_proposals, reflection_entries,  │   │
│  │   boundary_violations). If migrations haven't run, the      │   │
│  │   monitor will fail on missing tables.                      │   │
│  │   See: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md Phase 0.        │   │
│  │                                                             │   │
│  │ ON FAILURE: FatalError                                      │   │
│  │   Log: "FATAL: Database migration failed: {details}"        │   │
│  │   Exit code: 76 (EX_PROTOCOL)                              │   │
│  │   NOTE: Do NOT attempt rollback. Forward-only by design.    │   │
│  │   User must restore from backup if migration corrupts.      │   │
│  └──────────────────────────┬──────────────────────────────────┘   │
│                             │ success                               │
│                             ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ STEP 3: Verify Convergence Monitor Health                   │   │
│  │                                                             │   │
│  │ THIS IS THE ONLY STEP THAT CAN DEGRADE INSTEAD OF FATAL.   │   │
│  │                                                             │   │
│  │ 3a. Read monitor address from ghost.yml:                    │   │
│  │     convergence.monitor.address (default: "127.0.0.1:9100") │  │
│  │ 3b. Attempt HTTP GET {monitor_address}/health               │   │
│  │     Timeout: 5 seconds                                      │   │
│  │     Retries: 3 attempts with 1s backoff                     │   │
│  │ 3c. Validate response:                                      │   │
│  │     - HTTP 200                                              │   │
│  │     - Body contains: {"status": "ok", "version": "..."}    │   │
│  │     - Version compatibility check (semver major match)      │   │
│  │                                                             │   │
│  │ ON SUCCESS:                                                 │   │
│  │   monitor_state = MonitorConnection::Connected              │   │
│  │   Log: "INFO: Convergence monitor connected (v{version})"  │   │
│  │   Continue to step 4                                        │   │
│  │                                                             │   │
│  │ ON FAILURE (all 3 retries exhausted):                       │   │
│  │   monitor_state = MonitorConnection::Unreachable            │   │
│  │   Log: "CRITICAL: Convergence monitor unreachable at        │   │
│  │         {address}. Starting in DEGRADED mode. Safety        │   │
│  │         floor absent. Convergence scoring disabled."        │   │
│  │   Emit metric: gateway_degraded_start{reason="bootstrap"}  │   │
│  │   Start MonitorReconnector background task                  │   │
│  │   Continue to step 4 (DO NOT abort bootstrap)               │   │
│  │                                                             │   │
│  │ ON VERSION MISMATCH:                                        │   │
│  │   Same as failure. Incompatible monitor is treated as       │   │
│  │   absent. Log includes version details.                     │   │
│  └──────────────────────────┬──────────────────────────────────┘   │
│                             │ success or degraded                   │
│                             ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ STEP 4: Initialize Agent Registry + Channel Adapters        │   │
│  │                                                             │   │
│  │ 4a. Parse agent definitions from ghost.yml                  │   │
│  │ 4b. For each agent:                                         │   │
│  │     - Load CORP_POLICY.md (verify signature via ghost-signing)│ │
│  │     - Load SOUL.md, IDENTITY.md                             │   │
│  │     - Generate/load Ed25519 keypair (ghost-identity/keypair)│   │
│  │     - Register in AgentRegistry                             │   │
│  │     - Initialize per-agent cost tracker                     │   │
│  │ 4c. For each channel binding in ghost.yml:                  │   │
│  │     - Instantiate channel adapter (CLI, WebSocket, etc.)    │   │
│  │     - Call adapter.connect()                                │   │
│  │     - Register in channel router                            │   │
│  │ 4d. Initialize SkillRegistry (scan builtin + user dirs)     │   │
│  │ 4e. Initialize PolicyEngine (load CORP_POLICY.md)           │   │
│  │                                                             │   │
│  │ ON FAILURE: FatalError                                      │   │
│  │   Log: "FATAL: Agent/channel initialization failed: {}"     │   │
│  │   Exit code: 69 (EX_UNAVAILABLE)                           │   │
│  │   Cleanup: disconnect any already-connected adapters        │   │
│  └──────────────────────────┬──────────────────────────────────┘   │
│                             │ success                               │
│                             ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ STEP 5: Start API Server + WebSocket                        │   │
│  │                                                             │   │
│  │ 5a. Build axum Router (routes.rs)                           │   │
│  │ 5b. Apply middleware (CORS, rate limiting, auth)            │   │
│  │ 5c. Bind to address from ghost.yml:                         │   │
│  │     gateway.bind (default: "127.0.0.1")                     │   │
│  │     gateway.port (default: 18789)                           │   │
│  │ 5d. Start listening (axum::serve)                           │   │
│  │ 5e. Start WebSocket upgrade handler                         │   │
│  │ 5f. Start heartbeat scheduler (if configured)               │   │
│  │                                                             │   │
│  │ ON FAILURE: FatalError                                      │   │
│  │   Log: "FATAL: API server bind failed on {bind}:{port}"    │   │
│  │   Exit code: 76 (EX_PROTOCOL)                              │   │
│  │   Common cause: port already in use                         │   │
│  └──────────────────────────┬──────────────────────────────────┘   │
│                             │ success                               │
│                             ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ TRANSITION DECISION                                         │   │
│  │                                                             │   │
│  │ if monitor_state == Connected:                              │   │
│  │     set_state(GatewayState::Healthy)                        │   │
│  │     Log: "INFO: Gateway started. State: HEALTHY"            │   │
│  │     Log: "INFO: Listening on {bind}:{port}"                 │   │
│  │     Log: "INFO: {N} agents registered, {M} channels active"│   │
│  │                                                             │   │
│  │ if monitor_state == Unreachable:                            │   │
│  │     set_state(GatewayState::Degraded)                       │   │
│  │     Log: "WARN: Gateway started in DEGRADED mode"           │   │
│  │     Log: "WARN: Convergence monitor unreachable"            │   │
│  │     Log: "WARN: Safety features disabled until reconnection"│   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### Bootstrap Struct Definition

```rust
/// ghost-gateway/src/bootstrap.rs

use crate::gateway::GatewayState;
use crate::config::GhostConfig;
use crate::health::MonitorConnection;

pub struct GatewayBootstrap {
    config: GhostConfig,
    monitor_state: MonitorConnection,
}

#[derive(Debug)]
pub enum BootstrapResult {
    /// All 5 steps passed. Monitor connected.
    Healthy {
        config: GhostConfig,
        db: rusqlite::Connection,
        agent_registry: AgentRegistry,
        channel_router: ChannelRouter,
        api_handle: tokio::task::JoinHandle<()>,
    },
    /// Steps 1,2,4,5 passed. Monitor unreachable.
    /// Gateway struct will start MonitorReconnector on receiving this result.
    Degraded {
        config: GhostConfig,
        db: rusqlite::Connection,
        agent_registry: AgentRegistry,
        channel_router: ChannelRouter,
        api_handle: tokio::task::JoinHandle<()>,
    },
    /// Fatal error. Process must exit.
    Fatal {
        step: u8,
        error: BootstrapError,
        exit_code: i32,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("config: {0}")]
    Config(String),
    #[error("database: {0}")]
    Database(String),
    #[error("agent/channel init: {0}")]
    AgentInit(String),
    #[error("api server: {0}")]
    ApiServer(String),
}

impl GatewayBootstrap {
    pub async fn run(config_path: Option<&str>) -> BootstrapResult {
        // Step 1
        let config = match Self::step1_load_config(config_path) {
            Ok(c) => c,
            Err(e) => return BootstrapResult::Fatal {
                step: 1, error: e, exit_code: 78
            },
        };

        // Step 2
        let db = match Self::step2_run_migrations(&config) {
            Ok(db) => db,
            Err(e) => return BootstrapResult::Fatal {
                step: 2, error: e, exit_code: 76
            },
        };

        // Step 3 — NEVER fatal
        let monitor_state = Self::step3_check_monitor(&config).await;

        // Step 4
        let (agent_registry, channel_router) =
            match Self::step4_init_agents_channels(&config, &db).await {
                Ok(r) => r,
                Err(e) => return BootstrapResult::Fatal {
                    step: 4, error: e, exit_code: 69
                },
            };

        // Step 5
        let api_handle = match Self::step5_start_api(&config).await {
            Ok(h) => h,
            Err(e) => return BootstrapResult::Fatal {
                step: 5, error: e, exit_code: 76
            },
        };

        // Transition decision
        match monitor_state {
            MonitorConnection::Connected { version } => {
                tracing::info!(
                    version = %version,
                    "Gateway started. State: HEALTHY"
                );
                BootstrapResult::Healthy {
                    config, db, agent_registry, channel_router, api_handle
                }
            }
            MonitorConnection::Unreachable { reason } => {
                tracing::warn!(
                    reason = %reason,
                    "Gateway started in DEGRADED mode. Safety floor absent."
                );
                // NOTE: MonitorReconnector is NOT started here in bootstrap.
                // The Gateway struct starts it in its event loop (see §8)
                // because the reconnector needs Arc<AtomicU8> gateway_state
                // which is owned by Gateway, not by the bootstrap sequence.
                // The BootstrapResult::Degraded signals to Gateway::start()
                // that it should immediately start the reconnector.
                BootstrapResult::Degraded {
                    config, db, agent_registry, channel_router,
                    api_handle,
                }
            }
        }
    }
}
```

---

## 3. HEALTHY STATE — Normal Operation

### File: `ghost-gateway/src/health.rs`

When `GatewayState == Healthy`, all features are active. The convergence monitor
is reachable, shared state files are fresh, and all 7 signals feed the composite
score. See: AGENT_ARCHITECTURE_v2.md §3 for the full convergence safety system
specification (7 signals, 5 intervention levels, memory filtering tiers).

```
┌─────────────────────────────────────────────────────────────────┐
│                    HEALTHY STATE                                 │
│                                                                 │
│  ALL features active:                                           │
│  ├── Agent loop runs with full convergence state in context     │
│  │   (L6 of 10-layer prompt compiler includes convergence       │
│  │    score, intervention level, goals)                         │
│  ├── ITP events emitted to monitor (async, non-blocking)        │
│  │   via unix socket (primary) or HTTP POST (fallback)          │
│  ├── Convergence-aware memory filtering active                  │
│  │   (cortex-convergence/filtering/convergence_aware_filter.rs) │
│  ├── Convergence-aware decay factor active (factor 6)           │
│  ├── Proposal validation uses all 7 dimensions (D1-D7)          │
│  ├── Intervention triggers operational (levels 0-4)             │
│  ├── Session boundary enforcement active                        │
│  ├── Simulation boundary prompt injected (L1)                   │
│  ├── Behavioral verification active (post-redirect tracking)    │
│  └── Health endpoint returns: {"status": "healthy"}             │
│                                                                 │
│  BACKGROUND TASKS:                                              │
│  ├── MonitorHealthChecker: periodic GET /health to monitor      │
│  │   Interval: configurable, default 30 seconds                 │
│  │   Consecutive failure threshold: 3 (configurable)            │
│  ├── HeartbeatScheduler: runs agent heartbeats per ghost.yml    │
│  ├── CostTracker: accumulates per-agent token/dollar costs      │
│  └── SessionPruner: cleans up idle sessions after cache TTL     │
└─────────────────────────────────────────────────────────────────┘
```

### MonitorHealthChecker — The Watchdog

This is the component that detects mid-session monitor death. It runs as a background
`tokio::spawn` task for the entire gateway lifetime.

```rust
/// ghost-gateway/src/health.rs

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

/// Connection state to the convergence monitor.
#[derive(Debug, Clone)]
pub enum MonitorConnection {
    Connected { version: String },
    Unreachable { reason: String },
}

/// Configuration for monitor health checking.
#[derive(Debug, Clone)]
pub struct MonitorHealthConfig {
    /// Address of the convergence monitor HTTP API.
    /// Default: "127.0.0.1:9100"
    pub address: String,

    /// How often to check monitor health.
    /// Default: 30 seconds.
    pub check_interval: Duration,

    /// How many consecutive failures before transitioning to DEGRADED.
    /// Default: 3.
    /// WHY 3: A single missed health check could be a transient blip
    /// (GC pause, CPU spike, network hiccup). 3 consecutive failures
    /// at 30s intervals = 90 seconds of confirmed unreachability.
    pub failure_threshold: u32,

    /// Timeout for each individual health check request.
    /// Default: 5 seconds.
    pub check_timeout: Duration,
}

impl Default for MonitorHealthConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:9100".to_string(),
            check_interval: Duration::from_secs(30),
            failure_threshold: 3,
            check_timeout: Duration::from_secs(5),
        }
    }
}

pub struct MonitorHealthChecker {
    config: MonitorHealthConfig,
    gateway_state: Arc<AtomicU8>,
    consecutive_failures: u32,
    client: reqwest::Client,
}

impl MonitorHealthChecker {
    /// Runs forever as a background task. Caller spawns via tokio::spawn.
    pub async fn run(mut self) {
        let mut interval = time::interval(self.config.check_interval);

        loop {
            interval.tick().await;

            let current_state = GatewayState::from_u8(
                self.gateway_state.load(Ordering::Acquire)
            );

            // Only check when in Healthy or Recovering state.
            // In Degraded state, the MonitorReconnector handles reconnection.
            // In ShuttingDown/FatalError, we don't care.
            match current_state {
                GatewayState::Healthy | GatewayState::Recovering => {
                    self.check_once().await;
                }
                GatewayState::ShuttingDown => {
                    tracing::debug!("Health checker stopping: gateway shutting down");
                    return;
                }
                _ => {
                    // Degraded, Initializing, FatalError — skip
                    continue;
                }
            }
        }
    }

    async fn check_once(&mut self) {
        let url = format!("http://{}/health", self.config.address);

        let result = tokio::time::timeout(
            self.config.check_timeout,
            self.client.get(&url).send()
        ).await;

        match result {
            Ok(Ok(response)) if response.status().is_success() => {
                // Reset failure counter on success
                if self.consecutive_failures > 0 {
                    tracing::info!(
                        previous_failures = self.consecutive_failures,
                        "Monitor health check recovered"
                    );
                }
                self.consecutive_failures = 0;
            }
            Ok(Ok(response)) => {
                // Non-2xx response
                self.consecutive_failures += 1;
                tracing::warn!(
                    status = %response.status(),
                    consecutive_failures = self.consecutive_failures,
                    "Monitor health check returned non-OK status"
                );
                self.maybe_transition_to_degraded().await;
            }
            Ok(Err(e)) => {
                // Connection error
                self.consecutive_failures += 1;
                tracing::warn!(
                    error = %e,
                    consecutive_failures = self.consecutive_failures,
                    "Monitor health check connection failed"
                );
                self.maybe_transition_to_degraded().await;
            }
            Err(_) => {
                // Timeout
                self.consecutive_failures += 1;
                tracing::warn!(
                    timeout_ms = self.config.check_timeout.as_millis() as u64,
                    consecutive_failures = self.consecutive_failures,
                    "Monitor health check timed out"
                );
                self.maybe_transition_to_degraded().await;
            }
        }
    }

    async fn maybe_transition_to_degraded(&self) {
        if self.consecutive_failures >= self.config.failure_threshold {
            let current = self.gateway_state.load(Ordering::Acquire);
            if current == GatewayState::Healthy as u8 {
                self.gateway_state.store(
                    GatewayState::Degraded as u8,
                    Ordering::Release
                );
                tracing::error!(
                    consecutive_failures = self.consecutive_failures,
                    threshold = self.config.failure_threshold,
                    "CRITICAL: Convergence monitor unreachable. \
                     Transitioning to DEGRADED mode. \
                     Safety floor absent."
                );
                // Emit metric
                metrics::counter!("gateway_state_transition",
                    "from" => "healthy", "to" => "degraded",
                    "reason" => "monitor_unreachable"
                ).increment(1);

                // Start reconnector (see §5)
                // The Gateway struct handles this via state change notification
            }
        }
    }
}
```

### Health Endpoint Responses by State

```rust
/// ghost-gateway/src/health.rs — HTTP endpoint handlers

/// GET /api/health — liveness probe
/// Returns 200 in all states except FatalError.
/// Kubernetes/Docker health checks use this.
pub async fn health_handler(state: Arc<GatewaySharedState>) -> impl IntoResponse {
    let gw_state = state.current_state();
    match gw_state {
        GatewayState::FatalError => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "status": "fatal_error",
                "message": "Gateway failed to start"
            })))
        }
        _ => {
            (StatusCode::OK, Json(json!({
                "status": "alive",
                "state": format!("{:?}", gw_state)
            })))
        }
    }
}

/// GET /api/ready — readiness probe
/// Returns 200 only when fully operational (Healthy) or partially (Degraded).
/// Load balancers use this to decide whether to route traffic.
pub async fn ready_handler(state: Arc<GatewaySharedState>) -> impl IntoResponse {
    let gw_state = state.current_state();
    match gw_state {
        GatewayState::Healthy => {
            (StatusCode::OK, Json(json!({
                "status": "ready",
                "convergence_monitor": "connected",
                "features": "full"
            })))
        }
        GatewayState::Degraded | GatewayState::Recovering => {
            (StatusCode::OK, Json(json!({
                "status": "ready",
                "convergence_monitor": "disconnected",
                "features": "degraded",
                "degraded_features": [
                    "convergence_scoring",
                    "convergence_aware_memory_filtering",
                    "convergence_aware_decay",
                    "intervention_triggers",
                    "session_boundary_escalation",
                    "behavioral_verification",
                    "t7_memory_health_path_a_b"
                ],
                "stale_state_features": [
                    "convergence_policy_tightener (last-known level)",
                    "heartbeat_frequency (last-known level)",
                    "reflection_depth_bounding (last-known level)",
                    "memory_filtering_tier (last-known tier)",
                    "proposal_validation_d5_d7 (last-known thresholds)"
                ],
                "active_fallbacks": [
                    "t7_memory_health_path_c (direct cortex, 60s, threshold 0.2)"
                ]
            })))
        }
        GatewayState::Initializing => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "status": "starting",
                "message": "Bootstrap in progress"
            })))
        }
        GatewayState::ShuttingDown => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "status": "shutting_down",
                "message": "Gateway is shutting down"
            })))
        }
        GatewayState::FatalError => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "status": "fatal_error"
            })))
        }
    }
}

/// GET /api/metrics — Prometheus-compatible metrics
/// Always returns 200 with current metrics regardless of state.
pub async fn metrics_handler(state: Arc<GatewaySharedState>) -> impl IntoResponse {
    // Includes: gateway_state gauge, degraded_duration_seconds,
    // monitor_health_check_total, monitor_health_check_failures_total,
    // state_transitions_total{from,to,reason}
    let metrics = state.metrics_registry.render();
    (StatusCode::OK, metrics)
}
```

---

## 4. DEGRADED STATE — Monitor Unreachable

### Entry Conditions
1. Bootstrap step 3 failed (monitor unreachable at startup), OR
2. `MonitorHealthChecker` detected N consecutive failures while in `Healthy` state, OR
3. `Recovering` state failed (monitor died again during state sync)

### What Changes in Degraded Mode

This is the critical section. Every feature that depends on the convergence monitor
must have an explicit degraded behavior. No ambiguity.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    DEGRADED STATE                                    │
│                    Monitor: UNREACHABLE                              │
│                                                                     │
│  FEATURE DEGRADATION TABLE                                          │
│  ═══════════════════════════════════════════════════════════════     │
│                                                                     │
│  ┌──────────────────────────────┬───────────────────────────────┐  │
│  │ Feature                      │ Degraded Behavior             │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Agent loop                   │ RUNS NORMALLY.                │  │
│  │                              │ Agents continue processing    │  │
│  │                              │ messages. Core functionality  │  │
│  │                              │ unaffected.                   │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ ITP event emission           │ BUFFERED LOCALLY.             │  │
│  │ (ghost-agent-loop/           │ ITP events written to local   │  │
│  │  itp_emitter.rs)             │ JSONL buffer file:            │  │
│  │                              │ ~/.ghost/sessions/buffer/     │  │
│  │                              │   itp_buffer_{timestamp}.jsonl│  │
│  │                              │ Max buffer: 10MB or 10K events│  │
│  │                              │ (whichever first). Oldest     │  │
│  │                              │ events dropped if buffer full.│  │
│  │                              │ Buffer replayed on recovery.  │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Convergence scoring          │ NO NEW SCORES COMPUTED.      │  │
│  │ (convergence-monitor/        │ Monitor computes scores —     │  │
│  │  monitor.rs)                 │ without it, no new composite  │  │
│  │                              │ score. Last known score       │  │
│  │                              │ frozen in shared state file:  │  │
│  │                              │ ~/.ghost/data/convergence_    │  │
│  │                              │   state/{agent_id}.json       │  │
│  │                              │ "composite_score" field.      │  │
│  │                              │ If no score ever computed     │  │
│  │                              │ (first boot): score = 0.0.   │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Convergence-aware memory     │ USES LAST-KNOWN FILTER TIER. │  │
│  │ filtering                    │ Shared state file contains    │  │
│  │ (cortex-convergence/         │ "memory_filter_tier": N.      │  │
│  │  filtering/)                 │ In degraded mode, filtering   │  │
│  │                              │ continues at last-known tier. │  │
│  │                              │ Tier 0 (score 0.0-0.2): all  │  │
│  │                              │   memories returned.          │  │
│  │                              │ Tier 1 (0.2-0.4): light      │  │
│  │                              │   filtering.                  │  │
│  │                              │ Tier 2 (0.4-0.6): moderate.   │  │
│  │                              │ Tier 3 (0.6+): strict.        │  │
│  │                              │ If no prior state (first      │  │
│  │                              │ boot): tier 0 (permissive).   │  │
│  │                              │ This is safe: more info, not  │  │
│  │                              │ less, when no data exists.    │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Convergence-aware decay      │ USES LAST-KNOWN SCORE.       │  │
│  │ (cortex-decay/factors/       │ Factor 6 uses convergence     │  │
│  │  convergence.rs)             │ score from stale shared state │  │
│  │                              │ file. If score was 0.5, decay │  │
│  │                              │ continues at that rate.       │  │
│  │                              │ If no prior state (first      │  │
│  │                              │ boot): factor 6 = 1.0 (no    │  │
│  │                              │ effect). Decay runs with 5    │  │
│  │                              │ original factors only.        │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Proposal validation          │ D1-D7 ALL RUN.               │  │
│  │ (cortex-validation/          │ D5 (scope expansion), D6     │  │
│  │  proposal_validator.rs)      │ (self-reference), D7         │  │
│  │                              │ (emulation language) use      │  │
│  │                              │ convergence score for         │  │
│  │                              │ threshold tightening.         │  │
│  │                              │ In degraded mode: D5-D7 use  │  │
│  │                              │ LAST-KNOWN score from stale   │  │
│  │                              │ shared state file. Thresholds │  │
│  │                              │ stay at last-known tightness. │  │
│  │                              │ If no prior state (first      │  │
│  │                              │ boot): D5-D7 run with         │  │
│  │                              │ BASELINE thresholds (most     │  │
│  │                              │ permissive).                  │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Intervention triggers        │ FROZEN AT LAST-KNOWN LEVEL.  │  │
│  │ (convergence-monitor/        │ No NEW level transitions      │  │
│  │  intervention/)              │ (monitor computes these).     │  │
│  │                              │ Intervention level frozen at  │  │
│  │                              │ last known value from shared  │  │
│  │                              │ state file. Restrictions at   │  │
│  │                              │ that level PERSIST (enforced  │  │
│  │                              │ by ConvergencePolicyTightener │  │
│  │                              │ reading stale shared state).  │  │
│  │                              │ If never set: Level 0.        │  │
│  │                              │ Session termination by        │  │
│  │                              │ intervention: NOT possible    │  │
│  │                              │ (requires monitor to escalate │  │
│  │                              │ to Level 3+).                 │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Session boundary             │ HARD LIMITS STILL ENFORCED.  │  │
│  │ enforcement                  │ ghost.yml max_session_duration│  │
│  │ (ghost-gateway/              │ is enforced by the GATEWAY   │  │
│  │  session/boundary.rs)        │ (not the monitor). The        │  │
│  │                              │ monitor adds ESCALATED limits │  │
│  │                              │ via shared state file         │  │
│  │                              │ ("session_caps" field).       │  │
│  │                              │ In degraded mode: last-known  │  │
│  │                              │ session caps from stale shared│  │
│  │                              │ state file PERSIST. If agent  │  │
│  │                              │ was at Level 3 (120min cap),  │  │
│  │                              │ that cap continues.           │  │
│  │                              │ Cooldown state also persists  │  │
│  │                              │ from shared state file.       │  │
│  │                              │ See: CONVERGENCE_MONITOR_     │  │
│  │                              │ SEQUENCE_FLOW.md §7.3.        │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Simulation boundary prompt   │ STILL INJECTED.              │  │
│  │ (simulation-boundary/        │ The prompt is compiled into   │  │
│  │  prompt_anchor.rs)           │ the binary (const &str).      │  │
│  │                              │ Does not depend on monitor.   │  │
│  │                              │ Always active.                │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Behavioral verification      │ DISABLED.                     │  │
│  │ (convergence-monitor/        │ Post-redirect output          │  │
│  │  verification/)              │ comparison requires monitor.  │  │
│  │                              │ Deceptive compliance          │  │
│  │                              │ detection unavailable.        │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Prompt compiler L6           │ SHOWS STALE/DEGRADED STATE.  │  │
│  │ (ghost-agent-loop/context/   │ If stale shared state exists: │  │
│  │  prompt_compiler.rs)         │ L6 injects last-known score   │  │
│  │                              │ and level with STALE marker:  │  │
│  │                              │ "CONVERGENCE STATE: STALE     │  │
│  │                              │  (monitor disconnected since  │  │
│  │                              │  {updated_at}). Last known:   │  │
│  │                              │  score={score}, level={level}.│  │
│  │                              │  Operating with last-known    │  │
│  │                              │  restrictions."               │  │
│  │                              │ If no prior state (first boot)│  │
│  │                              │ L6 injects:                   │  │
│  │                              │ "CONVERGENCE STATE: UNAVAIL-  │  │
│  │                              │  ABLE (monitor disconnected). │  │
│  │                              │  No prior convergence data."  │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Kill switch                  │ STILL OPERATIONAL.            │  │
│  │ (ghost-gateway/              │ Kill switch is gateway-owned, │  │
│  │  safety/kill_switch.rs)      │ not monitor-owned. PAUSE,     │  │
│  │                              │ QUARANTINE, KILL ALL still    │  │
│  │                              │ work. Manual triggers always  │  │
│  │                              │ active.                       │  │
│  │                              │                               │  │
│  │                              │ Auto-trigger T7 (memory       │  │
│  │                              │ health): PATH C FALLBACK      │  │
│  │                              │ ACTIVE. Gateway falls back to │  │
│  │                              │ direct cortex queries:        │  │
│  │                              │ cortex-observability::         │  │
│  │                              │   health_score(agent_id)      │  │
│  │                              │ every 60s (vs 30s normal).    │  │
│  │                              │ Only contradiction_count +    │  │
│  │                              │ hash chain integrity avail.   │  │
│  │                              │ Stricter threshold: < 0.2     │  │
│  │                              │ (vs < 0.3 normal — fewer      │  │
│  │                              │ signals = stricter cutoff).   │  │
│  │                              │ If cortex queries also fail:  │  │
│  │                              │ memory health = "unknown",    │  │
│  │                              │ no trigger (no data = no      │  │
│  │                              │ false positive).              │  │
│  │                              │ See: KILL_SWITCH_TRIGGER_     │  │
│  │                              │ CHAIN_SEQUENCE_FLOW.md §2.8   │  │
│  │                              │ T7 Path C for full spec.      │  │
│  │                              │                               │  │
│  │                              │ Other auto-triggers (T1-T6:   │  │
│  │                              │ spending cap, policy denials,  │  │
│  │                              │ sandbox escape, credential    │  │
│  │                              │ exfil, SOUL drift, multi-     │  │
│  │                              │ agent quarantine) still       │  │
│  │                              │ active — none depend on       │  │
│  │                              │ convergence monitor.          │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Dashboard WebSocket          │ EMITS DEGRADED STATUS.       │  │
│  │ (ghost-gateway/api/          │ Dashboard shows banner:       │  │
│  │  websocket.rs)               │ "⚠ Convergence monitor       │  │
│  │                              │  disconnected. Safety         │  │
│  │                              │  features degraded."          │  │
│  │                              │ Convergence charts show       │  │
│  │                              │ "data unavailable" for the    │  │
│  │                              │ gap period.                   │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Audit logging                │ STILL ACTIVE.                │  │
│  │ (ghost-audit/)               │ All tool executions still     │  │
│  │                              │ logged. Degraded state        │  │
│  │                              │ transition itself is logged   │  │
│  │                              │ as a CRITICAL audit event.    │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ ConvergencePolicyTightener   │ USES LAST-KNOWN LEVEL.       │  │
│  │ (ghost-policy/policy/        │ Policy engine reads from      │  │
│  │  convergence_policy.rs)      │ shared state file:            │  │
│  │                              │ ~/.ghost/data/convergence_    │  │
│  │                              │   state/{agent_id}.json       │  │
│  │                              │ File becomes STALE when       │  │
│  │                              │ monitor dies. Policy reads    │  │
│  │                              │ last-known intervention level │  │
│  │                              │ and KEEPS those restrictions. │  │
│  │                              │                               │  │
│  │                              │ CRITICAL: Does NOT fall back  │  │
│  │                              │ to Level 0. That would be     │  │
│  │                              │ LESS safe (removes all        │  │
│  │                              │ restrictions). Stale state    │  │
│  │                              │ is CONSERVATIVE by design.    │  │
│  │                              │                               │  │
│  │                              │ If never set (first boot):    │  │
│  │                              │ Level 0 (no restrictions).    │  │
│  │                              │ This is safe because no       │  │
│  │                              │ convergence data exists yet.  │  │
│  │                              │                               │  │
│  │                              │ Level-specific restrictions   │  │
│  │                              │ that persist via stale state: │  │
│  │                              │ L2: reduced proactive,        │  │
│  │                              │   stricter proposal validation│  │
│  │                              │ L3: session caps, reflection  │  │
│  │                              │   depth bounded (max 3),      │  │
│  │                              │   reflections/session (max 20)│  │
│  │                              │ L4: task-only mode, heartbeat │  │
│  │                              │   disabled, no proactive      │  │
│  │                              │                               │  │
│  │                              │ See: CONVERGENCE_MONITOR_     │  │
│  │                              │ SEQUENCE_FLOW.md §8.4 and     │  │
│  │                              │ §7.2 for full policy spec.    │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Heartbeat frequency          │ USES LAST-KNOWN LEVEL.       │  │
│  │ (ghost-heartbeat/            │ HeartbeatEngine reads         │  │
│  │  heartbeat.rs)               │ intervention level from       │  │
│  │                              │ shared state file.            │  │
│  │                              │ Convergence-aware frequency   │  │
│  │                              │ reduces at higher levels:     │  │
│  │                              │ L0-1: normal interval (30m)   │  │
│  │                              │ L2: halved frequency (60m)    │  │
│  │                              │ L3: further reduced           │  │
│  │                              │ L4: heartbeat disabled        │  │
│  │                              │                               │  │
│  │                              │ In degraded mode: uses last-  │  │
│  │                              │ known level from stale shared │  │
│  │                              │ state file. Same conservative │  │
│  │                              │ principle as policy tightener.│  │
│  │                              │ If never set: normal freq.    │  │
│  ├──────────────────────────────┼───────────────────────────────┤  │
│  │ Reflection depth bounding    │ ENFORCED VIA POLICY ENGINE.  │  │
│  │ (ghost-policy/policy/        │ Reflection depth limits are   │  │
│  │  convergence_policy.rs)      │ part of ConvergencePolicy-    │  │
│  │                              │ Tightener at Level 3+:        │  │
│  │                              │ max depth 3, max 20/session,  │  │
│  │                              │ self-reference ratio max 30%. │  │
│  │                              │                               │  │
│  │                              │ NOT a separate gateway-side   │  │
│  │                              │ component. Enforced through   │  │
│  │                              │ the same shared state file    │  │
│  │                              │ mechanism as policy tightener.│  │
│  │                              │ Uses last-known level in      │  │
│  │                              │ degraded mode.                │  │
│  │                              │                               │  │
│  │                              │ ReflectionConfig in           │  │
│  │                              │ cortex-core/config/           │  │
│  │                              │ convergence_config.rs defines │  │
│  │                              │ the thresholds. Policy engine │  │
│  │                              │ enforces them per-turn.       │  │
│  └──────────────────────────────┴───────────────────────────────┘  │
│                                                                     │
│  BACKGROUND TASKS IN DEGRADED STATE:                                │
│  ├── MonitorReconnector: active (see §5)                           │
│  ├── MonitorHealthChecker: paused (reconnector handles it)         │
│  ├── ITPBufferWriter: active (buffering events to disk)            │
│  ├── HeartbeatScheduler: still active (agents still run)           │
│  ├── CostTracker: still active                                     │
│  └── SessionPruner: still active                                   │
│                                                                     │
│  SHARED STATE FILE MECHANISM (how stale state persists):            │
│  ═══════════════════════════════════════════════════════════════     │
│                                                                     │
│  The convergence monitor publishes intervention state via TWO       │
│  mechanisms (see CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §7.1):       │
│                                                                     │
│  MECHANISM A: Shared State File (primary, for in-process consumers) │
│  ├── Monitor writes to:                                             │
│  │   ~/.ghost/data/convergence_state/{agent_instance_id}.json       │
│  │   {                                                              │
│  │     "intervention_level": 2,                                     │
│  │     "composite_score": 0.58,                                     │
│  │     "cooldown_active": true,                                     │
│  │     "cooldown_expires_at": "2026-02-27T15:30:00Z",              │
│  │     "session_caps": { "max_duration_minutes": 120, ... },       │
│  │     "memory_filter_tier": 2,                                     │
│  │     "policy_restrictions": ["reduced_proactive", ...],          │
│  │     "updated_at": "2026-02-27T15:25:00Z"                       │
│  │   }                                                              │
│  ├── File is atomically written (write to temp + rename)            │
│  ├── ghost-policy and ghost-gateway poll this file (1s interval)    │
│  └── When monitor dies: file becomes STALE but PERSISTS on disk     │
│                                                                     │
│  MECHANISM B: HTTP API (for dashboard and external consumers)       │
│  ├── GET /status → current intervention state for all agents        │
│  ├── GET /scores → current composite scores                         │
│  └── UNAVAILABLE in degraded mode (monitor is down)                 │
│                                                                     │
│  IN DEGRADED MODE:                                                  │
│  ├── Mechanism A: File exists on disk with last-known state         │
│  │   ├── ConvergencePolicyTightener reads it → gets last level      │
│  │   ├── HeartbeatEngine reads it → gets last frequency adjustment  │
│  │   ├── SessionBoundaryEnforcer reads it → gets last session caps  │
│  │   └── "updated_at" field reveals staleness (consumers can check) │
│  ├── Mechanism B: HTTP endpoints return connection errors            │
│  │   └── Dashboard shows "monitor disconnected" banner              │
│  └── KEY PRINCIPLE: Stale shared state file = CONSERVATIVE.         │
│      Last-known restrictions persist. Never fall to Level 0.        │
│      See: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4                 │
└─────────────────────────────────────────────────────────────────────┘
```

### Degraded Mode Decision Logic in the Agent Loop

```rust
/// ghost-agent-loop/src/runner.rs
///
/// The agent loop checks gateway state at the START of each turn,
/// not mid-turn. This prevents inconsistent behavior within a single
/// LLM call.

impl AgentRunner {
    async fn run_turn(&self, message: &InboundMessage, session: &mut SessionContext)
        -> Result<AgentResponse, AgentError>
    {
        let gateway_state = self.gateway_state.load(Ordering::Acquire);
        let is_degraded = gateway_state == GatewayState::Degraded as u8
                       || gateway_state == GatewayState::Recovering as u8;

        // Build context with convergence state awareness
        let convergence_context = if is_degraded {
            // Read last-known state from shared state file.
            // Does NOT fall to level 0 — preserves last restrictions.
            ConvergenceContext::unavailable_with_stale_state(
                session.agent_id()
            )
        } else {
            self.fetch_convergence_state(session.agent_id()).await?
        };

        // Compile prompt (10 layers)
        let prompt = self.prompt_compiler.compile(
            session,
            &convergence_context,  // L6 uses this
        ).await?;

        // Run LLM inference
        let response = self.llm.complete_with_tools(&prompt).await?;

        // Emit ITP event (buffered if degraded)
        self.itp_emitter.emit(ITPEvent::InteractionMessage {
            session_id: session.id(),
            // ... fields ...
        }).await;
        // ^^^ itp_emitter internally checks monitor reachability.
        // If unreachable, writes to local buffer. Non-blocking.

        // Validate proposals (D5-D7 use stale convergence thresholds if degraded)
        if let Some(proposals) = response.proposals() {
            let validation_context = if is_degraded {
                // Use last-known score/level from stale shared state.
                // D5-D7 thresholds tighten with convergence score.
                // Stale score preserves last-known tightening level.
                ProposalValidationContext::with_convergence(
                    convergence_context.score,  // last-known (frozen)
                    convergence_context.level,  // last-known (conservative)
                )
            } else {
                ProposalValidationContext::with_convergence(
                    convergence_context.score,
                    convergence_context.level,
                )
            };
            self.proposal_router.validate_and_route(
                proposals, &validation_context
            ).await?;
        }

        Ok(response)
    }
}

/// Convergence context provided to the prompt compiler and proposal validator.
pub struct ConvergenceContext {
    pub available: bool,
    pub score: f64,
    pub level: u8,
    pub goals: Vec<Goal>,
    pub intervention_active: bool,
    /// True when data comes from stale shared state file (monitor down).
    /// Consumers should log this and may adjust behavior (e.g., prompt
    /// compiler shows "STALE" indicator in L6).
    pub stale: bool,
    /// When the shared state file was last updated by the monitor.
    /// None if no prior state exists (first boot).
    pub stale_since: Option<String>,
}

impl ConvergenceContext {
    /// Used when monitor is unreachable.
    /// Reads LAST-KNOWN state from shared state file on disk.
    /// Does NOT fall back to level 0 — stale state is CONSERVATIVE.
    /// See: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4
    pub fn unavailable_with_stale_state(agent_id: &AgentId) -> Self {
        // Attempt to read last-known state from shared state file
        let state_path = format!(
            "{}/.ghost/data/convergence_state/{}.json",
            std::env::var("HOME").unwrap_or_default(),
            agent_id
        );
        match std::fs::read_to_string(&state_path) {
            Ok(content) => {
                if let Ok(state) = serde_json::from_str::<SharedConvergenceState>(&content) {
                    Self {
                        available: false,  // monitor is down
                        score: state.composite_score,  // last-known score (frozen)
                        level: state.intervention_level,  // last-known level (CONSERVATIVE)
                        goals: vec![],  // goals not available without monitor
                        intervention_active: state.intervention_level >= 2,
                        stale: true,  // flag for consumers to know data is stale
                        stale_since: state.updated_at,
                    }
                } else {
                    // File exists but unparseable — treat as first boot
                    Self::first_boot_unavailable()
                }
            }
            Err(_) => {
                // No shared state file — first boot, never had monitor
                Self::first_boot_unavailable()
            }
        }
    }

    /// First boot: no prior convergence data exists.
    /// Level 0 is safe here because there's nothing to restrict.
    fn first_boot_unavailable() -> Self {
        Self {
            available: false,
            score: 0.0,    // no data
            level: 0,      // no prior restrictions to preserve
            goals: vec![],
            intervention_active: false,
            stale: false,
            stale_since: None,
        }
    }
}
```

---

## 5. RECONNECTION — MonitorReconnector (Degraded → Recovering → Healthy)

### File: `ghost-gateway/src/health.rs` (MonitorReconnector section)

The reconnector is a background task that runs ONLY in `Degraded` state.
It uses exponential backoff to avoid hammering a crashed monitor.
See: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4 for the monitor-side crash
recovery behavior (resumes at last-known level, processes buffered events).

```
┌─────────────────────────────────────────────────────────────────┐
│                    RECONNECTION SEQUENCE                         │
│                                                                 │
│  State: DEGRADED                                                │
│                                                                 │
│  MonitorReconnector starts immediately on entering Degraded.    │
│                                                                 │
│  Backoff schedule:                                              │
│    Attempt 1:  5 seconds                                        │
│    Attempt 2: 10 seconds                                        │
│    Attempt 3: 20 seconds                                        │
│    Attempt 4: 40 seconds                                        │
│    Attempt 5: 80 seconds                                        │
│    Attempt 6: 160 seconds                                       │
│    Attempt 7+: 300 seconds (5 min cap)                          │
│                                                                 │
│    Jitter: ±20% on each interval to prevent thundering herd     │
│    if multiple gateways are reconnecting to same monitor.       │
│                                                                 │
│  Each attempt:                                                  │
│    1. HTTP GET {monitor_address}/health                         │
│       Timeout: 5 seconds                                        │
│    2. If 200 + valid body:                                      │
│       → Transition to RECOVERING (see §6)                       │
│    3. If failure:                                               │
│       → Log at WARN level (not ERROR — avoid log spam)          │
│       → Increment attempt counter                               │
│       → Wait for next backoff interval                          │
│       → Continue                                                │
│                                                                 │
│  The reconnector runs INDEFINITELY until:                       │
│    a. Monitor comes back → transitions to RECOVERING            │
│    b. Gateway enters SHUTTING_DOWN → reconnector stops          │
│                                                                 │
│  There is NO "give up after N attempts" — the monitor might     │
│  be restarted hours later. The gateway stays degraded but       │
│  functional, and reconnects whenever the monitor returns.       │
└─────────────────────────────────────────────────────────────────┘
```

```rust
/// ghost-gateway/src/health.rs

pub struct MonitorReconnector {
    address: String,
    gateway_state: Arc<AtomicU8>,
    client: reqwest::Client,
    itp_buffer_path: PathBuf,
}

impl MonitorReconnector {
    pub fn start(
        address: String,
        gateway_state: Arc<AtomicU8>,
        itp_buffer_path: PathBuf,
    ) -> tokio::task::JoinHandle<()> {
        let reconnector = Self {
            address,
            gateway_state,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client build should not fail"),
            itp_buffer_path,
        };
        tokio::spawn(reconnector.run())
    }

    async fn run(self) {
        let mut attempt: u32 = 0;

        loop {
            let current = GatewayState::from_u8(
                self.gateway_state.load(Ordering::Acquire)
            );

            // Stop if gateway is shutting down or somehow became healthy
            // (shouldn't happen, but defensive)
            match current {
                GatewayState::Degraded => { /* continue reconnecting */ }
                GatewayState::ShuttingDown => {
                    tracing::info!("Reconnector stopping: gateway shutting down");
                    return;
                }
                other => {
                    tracing::info!(
                        state = ?other,
                        "Reconnector stopping: gateway no longer degraded"
                    );
                    return;
                }
            }

            // Exponential backoff with jitter
            let base_delay = Duration::from_secs(
                std::cmp::min(5 * 2u64.saturating_pow(attempt), 300)
            );
            let jitter_range = base_delay.as_millis() as f64 * 0.2;
            let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
            let delay = Duration::from_millis(
                (base_delay.as_millis() as f64 + jitter).max(1000.0) as u64
            );

            tracing::debug!(
                attempt = attempt + 1,
                delay_ms = delay.as_millis() as u64,
                "Reconnection attempt scheduled"
            );

            tokio::time::sleep(delay).await;

            // Attempt connection
            let url = format!("http://{}/health", self.address);
            match self.client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    tracing::info!(
                        attempt = attempt + 1,
                        "Monitor reconnected. Transitioning to RECOVERING."
                    );

                    // Transition: Degraded → Recovering
                    self.gateway_state.store(
                        GatewayState::Recovering as u8,
                        Ordering::Release
                    );

                    metrics::counter!("gateway_state_transition",
                        "from" => "degraded", "to" => "recovering",
                        "reason" => "monitor_reconnected"
                    ).increment(1);

                    // Trigger recovery sequence (see §6)
                    // The Gateway struct watches for this state change
                    // and spawns the RecoveryCoordinator.
                    return;
                }
                Ok(response) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        status = %response.status(),
                        "Monitor reconnection failed: non-OK status"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        "Monitor reconnection failed"
                    );
                }
            }

            attempt = attempt.saturating_add(1);
        }
    }
}
```

---

## 6. RECOVERY SEQUENCE (Recovering → Healthy | Degraded)

### File: `ghost-gateway/src/health.rs` (RecoveryCoordinator section)

Recovery is NOT just "monitor is back, flip to healthy." There's a gap period where
ITP events were buffered locally and the monitor has no data. We must reconcile.
On the monitor side, it resumes at last-known intervention level and processes
buffered events. See: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4 (T=200 onwards).

```
┌─────────────────────────────────────────────────────────────────┐
│                    RECOVERY SEQUENCE                             │
│                    State: RECOVERING                             │
│                                                                 │
│  During recovery, agents continue running with degraded         │
│  feature set. The recovery happens in the background.           │
│                                                                 │
│  STEP R1: Verify monitor stability                              │
│  ├── Send 3 consecutive health checks, 5s apart                │
│  ├── All 3 must pass                                           │
│  ├── If any fail: abort recovery, return to DEGRADED            │
│  └── Purpose: avoid flapping if monitor is crash-looping        │
│                                                                 │
│  STEP R2: Replay buffered ITP events                            │
│  ├── Read ITP buffer files from ~/.ghost/sessions/buffer/       │
│  ├── Sort by timestamp (oldest first)                           │
│  ├── POST each event to monitor's /events endpoint              │
│  │   Batch size: 100 events per request                         │
│  │   Rate limit: 500 events/sec (don't overwhelm monitor)      │
│  ├── If replay fails mid-stream:                                │
│  │   Log warning, skip failed events, continue                  │
│  │   (partial replay is better than no replay)                  │
│  ├── On completion: delete buffer files                         │
│  └── Log: "Replayed {N} buffered ITP events to monitor"        │
│                                                                 │
│  STEP R3: Request convergence score recalculation               │
│  ├── POST to monitor /recalculate endpoint                      │
│  │   (monitor recomputes scores from replayed events)           │
│  ├── Wait for acknowledgment (timeout: 30s)                     │
│  ├── If timeout: proceed anyway (scores will catch up)          │
│  └── Fetch fresh convergence scores for all active agents       │
│                                                                 │
│  STEP R4: Transition to HEALTHY                                 │
│  ├── set_state(GatewayState::Healthy)                           │
│  ├── Resume MonitorHealthChecker (periodic checks)              │
│  ├── Stop MonitorReconnector                                    │
│  ├── Stop ITPBufferWriter (events go direct to monitor now)     │
│  ├── Notify dashboard via WebSocket:                            │
│  │   {"event": "state_change", "state": "healthy",             │
│  │    "previous": "recovering", "gap_duration_seconds": N}     │
│  ├── Log: "INFO: Recovery complete. State: HEALTHY.             │
│  │         Gap duration: {N}s. Events replayed: {M}."          │
│  └── Emit metric: gateway_recovery_duration_seconds             │
│                                                                 │
│  FAILURE MODES:                                                 │
│  ├── R1 fails (monitor unstable):                               │
│  │   → Return to DEGRADED                                       │
│  │   → Restart MonitorReconnector                               │
│  │   → Log: "WARN: Recovery aborted: monitor unstable"          │
│  ├── R2 partially fails (some events can't replay):             │
│  │   → Continue to R3 (partial data is acceptable)              │
│  │   → Log: "WARN: {N} events failed to replay"                │
│  ├── R3 times out:                                              │
│  │   → Continue to R4 (scores will eventually converge)         │
│  │   → Log: "WARN: Score recalculation timed out"               │
│  └── Monitor dies during R2/R3:                                 │
│      → Abort recovery, return to DEGRADED                       │
│      → Restart MonitorReconnector                               │
│      → Unreplayed buffer events preserved for next recovery     │
└─────────────────────────────────────────────────────────────────┘
```

```rust
/// ghost-gateway/src/health.rs

pub struct RecoveryCoordinator {
    monitor_address: String,
    gateway_state: Arc<AtomicU8>,
    itp_buffer_path: PathBuf,
    client: reqwest::Client,
}

impl RecoveryCoordinator {
    pub async fn run(self) -> RecoveryResult {
        let start = std::time::Instant::now();

        // R1: Verify stability (3 consecutive checks)
        for i in 0..3 {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if !self.health_check().await {
                tracing::warn!(
                    check = i + 1,
                    "Recovery aborted: monitor failed stability check"
                );
                self.gateway_state.store(
                    GatewayState::Degraded as u8, Ordering::Release
                );
                return RecoveryResult::Aborted {
                    reason: "monitor_unstable".into()
                };
            }
        }

        // R2: Replay buffered events
        let replay_result = self.replay_buffered_events().await;
        tracing::info!(
            replayed = replay_result.success_count,
            failed = replay_result.failure_count,
            "ITP event replay complete"
        );

        // Check monitor still alive after replay
        if !self.health_check().await {
            tracing::warn!("Monitor died during event replay");
            self.gateway_state.store(
                GatewayState::Degraded as u8, Ordering::Release
            );
            return RecoveryResult::Aborted {
                reason: "monitor_died_during_replay".into()
            };
        }

        // R3: Request score recalculation
        let recalc_url = format!(
            "http://{}/recalculate", self.monitor_address
        );
        let recalc_result = tokio::time::timeout(
            Duration::from_secs(30),
            self.client.post(&recalc_url).send()
        ).await;

        match recalc_result {
            Ok(Ok(r)) if r.status().is_success() => {
                tracing::info!("Score recalculation acknowledged");
            }
            _ => {
                tracing::warn!(
                    "Score recalculation timed out or failed. \
                     Proceeding — scores will converge naturally."
                );
            }
        }

        // R4: Transition to Healthy
        self.gateway_state.store(
            GatewayState::Healthy as u8, Ordering::Release
        );

        let duration = start.elapsed();
        tracing::info!(
            duration_secs = duration.as_secs(),
            events_replayed = replay_result.success_count,
            "Recovery complete. State: HEALTHY"
        );

        metrics::histogram!("gateway_recovery_duration_seconds")
            .record(duration.as_secs_f64());

        RecoveryResult::Success {
            duration,
            events_replayed: replay_result.success_count,
            events_failed: replay_result.failure_count,
        }
    }

    async fn replay_buffered_events(&self) -> ReplayResult {
        let mut success_count = 0u64;
        let mut failure_count = 0u64;

        // Read buffer files sorted by name (timestamp-based names)
        let mut entries: Vec<_> = std::fs::read_dir(&self.itp_buffer_path)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "jsonl"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e,
                        "Failed to read buffer file");
                    failure_count += 1;
                    continue;
                }
            };

            // Batch events (100 per request)
            let events: Vec<&str> = content.lines().collect();
            for chunk in events.chunks(100) {
                let batch_body = chunk.join("\n");
                let url = format!(
                    "http://{}/events/batch", self.monitor_address
                );

                match self.client.post(&url)
                    .header("Content-Type", "application/x-ndjson")
                    .body(batch_body)
                    .send()
                    .await
                {
                    Ok(r) if r.status().is_success() => {
                        success_count += chunk.len() as u64;
                    }
                    Ok(r) => {
                        tracing::warn!(
                            status = %r.status(),
                            batch_size = chunk.len(),
                            "Batch replay returned non-OK"
                        );
                        failure_count += chunk.len() as u64;
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            batch_size = chunk.len(),
                            "Batch replay failed"
                        );
                        failure_count += chunk.len() as u64;
                    }
                }

                // Rate limit: ~500 events/sec
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            // Delete successfully processed buffer file
            if failure_count == 0 {
                let _ = std::fs::remove_file(&path);
            }
        }

        ReplayResult { success_count, failure_count }
    }

    async fn health_check(&self) -> bool {
        let url = format!("http://{}/health", self.monitor_address);
        matches!(
            tokio::time::timeout(
                Duration::from_secs(5),
                self.client.get(&url).send()
            ).await,
            Ok(Ok(r)) if r.status().is_success()
        )
    }
}

#[derive(Debug)]
pub enum RecoveryResult {
    Success {
        duration: Duration,
        events_replayed: u64,
        events_failed: u64,
    },
    Aborted {
        reason: String,
    },
}

struct ReplayResult {
    success_count: u64,
    failure_count: u64,
}
```

---

## 7. SHUTDOWN SEQUENCE (Any State → ShuttingDown → Exit)

### File: `ghost-gateway/src/shutdown.rs`

Shutdown can be triggered from ANY non-terminal state. The sequence is always the same
7 steps, but some steps are skipped based on what's currently active.
Kill switch Level 3 (KILL ALL) triggers shutdown — see KILL_SWITCH_TRIGGER_CHAIN_SEQUENCE_FLOW.md
§3 for the full trigger-to-execution pipeline.

```
┌─────────────────────────────────────────────────────────────────┐
│                    SHUTDOWN SEQUENCE                             │
│                    State: SHUTTING_DOWN                          │
│                                                                 │
│  TRIGGER: SIGTERM, SIGINT, or kill switch Level 3 (KILL ALL)    │
│                                                                 │
│  On signal received:                                            │
│    1. Set GatewayState::ShuttingDown (atomic, immediate)        │
│    2. Log: "INFO: Shutdown initiated. Reason: {signal|killswitch}"│
│    3. Start 60-second forced exit timer                         │
│    4. Begin graceful shutdown sequence                          │
│                                                                 │
│  If SIGTERM/SIGINT received AGAIN during shutdown:              │
│    Log: "WARN: Second signal received. Forcing immediate exit." │
│    std::process::exit(1)                                        │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STEP S1: Stop accepting new connections (immediate)     │   │
│  │                                                         │   │
│  │ - axum server stops accepting new TCP connections       │   │
│  │ - WebSocket upgrade requests rejected with 503          │   │
│  │ - Channel adapters stop accepting new inbound messages  │   │
│  │ - Health endpoint returns {"status": "shutting_down"}   │   │
│  │ - Ready endpoint returns 503                            │   │
│  │                                                         │   │
│  │ Time budget: 0s (immediate)                             │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STEP S2: Drain lane queues (wait up to 30s)             │   │
│  │                                                         │   │
│  │ - Each session has a LaneQueue (serialized request queue)│  │
│  │ - Allow in-flight requests to complete                  │   │
│  │ - Do NOT start processing queued requests               │   │
│  │ - Wait for all currently-executing agent turns to finish│   │
│  │ - If a turn is mid-LLM-call: wait for response          │   │
│  │ - If a turn is mid-tool-execution: wait for completion   │   │
│  │ - Timeout: 30 seconds. After 30s, abort remaining turns.│   │
│  │                                                         │   │
│  │ Time budget: 0-30s                                      │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STEP S3: Flush active sessions (memory flush turn)      │   │
│  │                                                         │   │
│  │ For each active session with unsaved context:           │   │
│  │ - Inject silent memory flush prompt:                    │   │
│  │   "Context is being saved. Write any critical facts     │   │
│  │    to daily log NOW."                                   │   │
│  │ - Run one final agent turn (cheap model, max 2K tokens) │   │
│  │ - Agent writes to memory/daily/YYYY-MM-DD.md            │   │
│  │ - If flush fails: log warning, continue (don't block    │   │
│  │   shutdown for a failed flush)                          │   │
│  │                                                         │   │
│  │ Parallelism: all sessions flush concurrently            │   │
│  │ Timeout: 15 seconds per session, 30s total              │   │
│  │                                                         │   │
│  │ SKIP IF: no active sessions, or kill switch Level 3     │   │
│  │ (kill switch skips flush for immediate stop)            │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STEP S4: Persist in-flight cost tracking                │   │
│  │                                                         │   │
│  │ - Write accumulated per-agent cost data to SQLite       │   │
│  │ - Write per-session token counts                        │   │
│  │ - Flush any pending audit log entries                    │   │
│  │                                                         │   │
│  │ Time budget: 2s                                         │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STEP S5: Notify convergence monitor of shutdown         │   │
│  │                                                         │   │
│  │ - POST to monitor /gateway-shutdown endpoint            │   │
│  │   Body: {"reason": "...", "active_sessions": N,         │   │
│  │          "timestamp": "..."}                            │   │
│  │ - Timeout: 2 seconds (best-effort, don't block)         │   │
│  │                                                         │   │
│  │ SKIP IF: gateway is in DEGRADED state (monitor already  │   │
│  │ unreachable — nothing to notify)                        │   │
│  │                                                         │   │
│  │ Purpose: monitor can close out active sessions cleanly, │   │
│  │ mark them as "gateway-terminated" rather than "abandoned"│  │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STEP S6: Close channel adapter connections               │   │
│  │                                                         │   │
│  │ For each connected channel adapter:                     │   │
│  │ - Call adapter.disconnect()                             │   │
│  │ - Telegram: stop long polling                           │   │
│  │ - Discord: close gateway WebSocket                      │   │
│  │ - Slack: close WebSocket                                │   │
│  │ - WhatsApp: send SIGTERM to Baileys sidecar process     │   │
│  │ - WebSocket: close all client connections with 1001     │   │
│  │ - CLI: flush stdout, restore terminal                   │   │
│  │                                                         │   │
│  │ Timeout: 5 seconds total for all adapters               │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STEP S7: Close SQLite connections                        │   │
│  │                                                         │   │
│  │ - Flush WAL to main database file                       │   │
│  │   (PRAGMA wal_checkpoint(TRUNCATE))                     │   │
│  │ - Close all connection pool handles                     │   │
│  │ - Verify: no -wal or -shm files remain (clean close)    │   │
│  │                                                         │   │
│  │ Time budget: 5s                                         │   │
│  └──────────────────────────┬──────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ EXIT                                                    │   │
│  │                                                         │   │
│  │ Log: "INFO: Shutdown complete. Duration: {N}s"          │   │
│  │ Exit code: 0 (clean shutdown)                           │   │
│  │                                                         │   │
│  │ If 60-second forced exit timer fires before reaching    │   │
│  │ this point:                                             │   │
│  │   Log: "ERROR: Forced exit after 60s timeout"           │   │
│  │   Exit code: 1                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Shutdown Coordinator Struct

```rust
/// ghost-gateway/src/shutdown.rs

use tokio::signal;
use tokio::sync::broadcast;

pub struct ShutdownCoordinator {
    gateway_state: Arc<AtomicU8>,
    shutdown_tx: broadcast::Sender<ShutdownReason>,
    forced_exit_timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum ShutdownReason {
    Signal(SignalKind),
    KillSwitch { level: u8 },
    ApiRequest,
}

#[derive(Debug, Clone, Copy)]
pub enum SignalKind {
    Sigterm,
    Sigint,
}

impl ShutdownCoordinator {
    pub fn new(gateway_state: Arc<AtomicU8>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            gateway_state,
            shutdown_tx,
            forced_exit_timeout: Duration::from_secs(60),
        }
    }

    /// Returns a receiver that fires when shutdown is initiated.
    /// All subsystems should select! on this to know when to stop.
    pub fn subscribe(&self) -> broadcast::Receiver<ShutdownReason> {
        self.shutdown_tx.subscribe()
    }

    /// Install signal handlers. Call once at startup.
    pub async fn watch_signals(self: Arc<Self>) {
        let mut sigterm = signal::unix::signal(
            signal::unix::SignalKind::terminate()
        ).expect("SIGTERM handler");

        let mut sigint = signal::unix::signal(
            signal::unix::SignalKind::interrupt()
        ).expect("SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                self.initiate(ShutdownReason::Signal(SignalKind::Sigterm)).await;
            }
            _ = sigint.recv() => {
                self.initiate(ShutdownReason::Signal(SignalKind::Sigint)).await;
            }
        }
    }

    pub async fn initiate(&self, reason: ShutdownReason) {
        let previous = self.gateway_state.swap(
            GatewayState::ShuttingDown as u8,
            Ordering::AcqRel
        );

        if previous == GatewayState::ShuttingDown as u8 {
            // Second signal — force exit
            tracing::warn!("Second shutdown signal. Forcing immediate exit.");
            std::process::exit(1);
        }

        tracing::info!(reason = ?reason, "Shutdown initiated");

        // Notify all subscribers
        let _ = self.shutdown_tx.send(reason.clone());

        // Start forced exit timer
        let timeout = self.forced_exit_timeout;
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            tracing::error!(
                timeout_secs = timeout.as_secs(),
                "Forced exit: shutdown timed out"
            );
            std::process::exit(1);
        });
    }

    /// Execute the 7-step shutdown sequence.
    pub async fn execute(
        &self,
        reason: ShutdownReason,
        api_handle: tokio::task::JoinHandle<()>,
        sessions: &SessionManager,
        cost_tracker: &CostTracker,
        monitor_address: &str,
        channel_router: &ChannelRouter,
        db_pool: &SqlitePool,
    ) {
        let start = std::time::Instant::now();
        let is_kill_switch = matches!(reason, ShutdownReason::KillSwitch { .. });
        let is_degraded = self.gateway_state.load(Ordering::Acquire)
            == GatewayState::Degraded as u8;

        // S1: Stop accepting connections
        tracing::info!("S1: Stopping new connections");
        api_handle.abort();

        // S2: Drain lane queues
        tracing::info!("S2: Draining lane queues (30s max)");
        let drain_result = tokio::time::timeout(
            Duration::from_secs(30),
            sessions.drain_active_turns()
        ).await;
        if drain_result.is_err() {
            tracing::warn!("S2: Drain timed out after 30s, aborting remaining turns");
            sessions.abort_all_turns().await;
        }

        // S3: Memory flush (skip on kill switch)
        if !is_kill_switch {
            tracing::info!("S3: Flushing active sessions");
            let flush_result = tokio::time::timeout(
                Duration::from_secs(30),
                sessions.flush_all_sessions()
            ).await;
            if flush_result.is_err() {
                tracing::warn!("S3: Session flush timed out after 30s");
            }
        } else {
            tracing::info!("S3: Skipped (kill switch)");
        }

        // S4: Persist costs
        tracing::info!("S4: Persisting cost tracking data");
        if let Err(e) = cost_tracker.flush_to_db().await {
            tracing::warn!(error = %e, "S4: Cost flush failed");
        }

        // S5: Notify monitor (skip if degraded)
        if !is_degraded {
            tracing::info!("S5: Notifying convergence monitor");
            let notify_result = tokio::time::timeout(
                Duration::from_secs(2),
                Self::notify_monitor(monitor_address, &reason)
            ).await;
            if notify_result.is_err() {
                tracing::warn!("S5: Monitor notification timed out");
            }
        } else {
            tracing::info!("S5: Skipped (monitor unreachable)");
        }

        // S6: Close channel adapters
        tracing::info!("S6: Closing channel adapters");
        let close_result = tokio::time::timeout(
            Duration::from_secs(5),
            channel_router.disconnect_all()
        ).await;
        if close_result.is_err() {
            tracing::warn!("S6: Channel close timed out after 5s");
        }

        // S7: Close SQLite
        tracing::info!("S7: Closing database connections");
        if let Err(e) = db_pool.close().await {
            tracing::warn!(error = %e, "S7: Database close error");
        }

        let duration = start.elapsed();
        tracing::info!(
            duration_secs = duration.as_secs(),
            "Shutdown complete"
        );
    }

    async fn notify_monitor(address: &str, reason: &ShutdownReason) {
        let client = reqwest::Client::new();
        let url = format!("http://{}/gateway-shutdown", address);
        let _ = client.post(&url)
            .json(&serde_json::json!({
                "reason": format!("{:?}", reason),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
            .send()
            .await;
    }
}
```

---

## 8. GATEWAY STRUCT — The Orchestrator

### File: `ghost-gateway/src/gateway.rs`

The `Gateway` struct owns the state machine and coordinates all subsystems.
It watches for state transitions and spawns/stops background tasks accordingly.

```rust
/// ghost-gateway/src/gateway.rs

pub struct Gateway {
    state: Arc<AtomicU8>,
    config: GhostConfig,
    db_pool: SqlitePool,
    agent_registry: AgentRegistry,
    channel_router: ChannelRouter,
    session_manager: SessionManager,
    cost_tracker: CostTracker,
    shutdown_coordinator: Arc<ShutdownCoordinator>,

    // Background task handles — Option because they start/stop dynamically
    health_checker_handle: Option<tokio::task::JoinHandle<()>>,
    reconnector_handle: Option<tokio::task::JoinHandle<()>>,
    itp_buffer_handle: Option<tokio::task::JoinHandle<()>>,
    api_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Gateway {
    pub async fn start() -> Result<(), Box<dyn std::error::Error>> {
        // Run bootstrap
        let bootstrap_result = GatewayBootstrap::run(None).await;

        match bootstrap_result {
            BootstrapResult::Healthy { config, db, agent_registry,
                                       channel_router, api_handle } => {
                let mut gw = Self::new(
                    config, db, agent_registry, channel_router,
                    GatewayState::Healthy
                );
                gw.api_handle = Some(api_handle);

                // Start health checker (monitors the monitor)
                gw.start_health_checker();

                // Install signal handlers
                let shutdown = gw.shutdown_coordinator.clone();
                tokio::spawn(async move { shutdown.watch_signals().await });

                // Run until shutdown
                gw.run_event_loop().await;
                Ok(())
            }

            BootstrapResult::Degraded { config, db, agent_registry,
                                         channel_router, api_handle } => {
                let mut gw = Self::new(
                    config, db, agent_registry, channel_router,
                    GatewayState::Degraded
                );
                gw.api_handle = Some(api_handle);

                // Start reconnector (Gateway owns the state Arc, so it starts here)
                gw.start_reconnector();

                // Start ITP buffer writer
                gw.start_itp_buffer_writer();

                // Install signal handlers
                let shutdown = gw.shutdown_coordinator.clone();
                tokio::spawn(async move { shutdown.watch_signals().await });

                // Run until shutdown
                gw.run_event_loop().await;
                Ok(())
            }

            BootstrapResult::Fatal { step, error, exit_code } => {
                tracing::error!(
                    step = step,
                    error = %error,
                    exit_code = exit_code,
                    "FATAL: Bootstrap failed"
                );
                std::process::exit(exit_code);
            }
        }
    }

    /// Main event loop. Watches for state transitions and reacts.
    async fn run_event_loop(&mut self) {
        let mut shutdown_rx = self.shutdown_coordinator.subscribe();
        let mut state_check_interval = tokio::time::interval(
            Duration::from_secs(1)
        );

        loop {
            tokio::select! {
                // Shutdown signal received
                Ok(reason) = shutdown_rx.recv() => {
                    self.handle_shutdown(reason).await;
                    return;
                }

                // Periodic state check for transitions
                _ = state_check_interval.tick() => {
                    self.handle_state_transitions().await;
                }
            }
        }
    }

    /// React to state transitions triggered by background tasks.
    async fn handle_state_transitions(&mut self) {
        let current = GatewayState::from_u8(
            self.state.load(Ordering::Acquire)
        );

        match current {
            GatewayState::Degraded => {
                // Ensure reconnector is running
                if self.reconnector_handle.is_none()
                    || self.reconnector_handle.as_ref()
                        .map_or(true, |h| h.is_finished())
                {
                    self.start_reconnector();
                }
                // Ensure ITP buffer is running
                if self.itp_buffer_handle.is_none()
                    || self.itp_buffer_handle.as_ref()
                        .map_or(true, |h| h.is_finished())
                {
                    self.start_itp_buffer_writer();
                }
                // Ensure health checker is stopped
                if let Some(h) = self.health_checker_handle.take() {
                    h.abort();
                }
            }

            GatewayState::Recovering => {
                // Reconnector finished (it transitions to Recovering then exits).
                // Stop reconnector handle.
                if let Some(h) = self.reconnector_handle.take() {
                    let _ = h.await;
                }
                // Spawn recovery coordinator
                self.start_recovery();
            }

            GatewayState::Healthy => {
                // Ensure health checker is running
                if self.health_checker_handle.is_none()
                    || self.health_checker_handle.as_ref()
                        .map_or(true, |h| h.is_finished())
                {
                    self.start_health_checker();
                }
                // Ensure reconnector is stopped
                if let Some(h) = self.reconnector_handle.take() {
                    h.abort();
                }
                // Ensure ITP buffer writer is stopped
                if let Some(h) = self.itp_buffer_handle.take() {
                    h.abort();
                }
            }

            _ => {}
        }
    }

    fn start_health_checker(&mut self) {
        let checker = MonitorHealthChecker {
            config: MonitorHealthConfig::from(&self.config),
            gateway_state: self.state.clone(),
            consecutive_failures: 0,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        };
        self.health_checker_handle = Some(tokio::spawn(checker.run()));
    }

    fn start_reconnector(&mut self) {
        let handle = MonitorReconnector::start(
            self.config.convergence.monitor.address.clone(),
            self.state.clone(),
            self.config.itp_buffer_path(),
        );
        self.reconnector_handle = Some(handle);
    }

    fn start_recovery(&mut self) {
        let coordinator = RecoveryCoordinator {
            monitor_address: self.config.convergence.monitor.address.clone(),
            gateway_state: self.state.clone(),
            itp_buffer_path: self.config.itp_buffer_path(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        };
        tokio::spawn(async move {
            let result = coordinator.run().await;
            match result {
                RecoveryResult::Success { duration, events_replayed, .. } => {
                    tracing::info!(
                        duration_secs = duration.as_secs(),
                        events_replayed,
                        "Recovery coordinator completed successfully"
                    );
                }
                RecoveryResult::Aborted { reason } => {
                    tracing::warn!(
                        reason = %reason,
                        "Recovery coordinator aborted — back to DEGRADED"
                    );
                    // State already set to Degraded by the coordinator
                }
            }
        });
    }

    fn start_itp_buffer_writer(&mut self) {
        // ITPBufferWriter is a simple task that accepts ITP events
        // from a channel and writes them to JSONL files on disk.
        // See ghost-agent-loop/src/itp_emitter.rs for the sender side.
        let buffer_path = self.config.itp_buffer_path();
        self.itp_buffer_handle = Some(tokio::spawn(async move {
            ITPBufferWriter::new(buffer_path).run().await;
        }));
    }

    async fn handle_shutdown(&mut self, reason: ShutdownReason) {
        self.shutdown_coordinator.execute(
            reason,
            self.api_handle.take().unwrap(),
            &self.session_manager,
            &self.cost_tracker,
            &self.config.convergence.monitor.address,
            &self.channel_router,
            &self.db_pool,
        ).await;
    }
}
```

---

## 9. COMPLETE STATE MACHINE DIAGRAM — All Transitions

```
                                    ┌─────────────────────────────────────────────────────────────────────────────────────────┐
                                    │                                                                                         │
                                    │                          GHOST GATEWAY STATE MACHINE                                     │
                                    │                                                                                         │
                                    │                                                                                         │
                                    │                           ┌──────────────┐                                              │
                                    │                           │ INITIALIZING │                                              │
                                    │                           │   (entry)    │                                              │
                                    │                           └──────┬───────┘                                              │
                                    │                                  │                                                      │
                                    │                    ┌─────────────┼─────────────┐                                        │
                                    │                    │             │             │                                        │
                                    │             steps 1/2/4/5   all pass    step 3 only                                    │
                                    │               fail              │         fails                                        │
                                    │                    │             │             │                                        │
                                    │                    ▼             ▼             ▼                                        │
                                    │            ┌─────────────┐ ┌─────────┐ ┌──────────┐                                    │
                                    │            │ FATAL_ERROR  │ │ HEALTHY │ │ DEGRADED │                                    │
                                    │            │  (terminal)  │ │         │ │          │                                    │
                                    │            └──────┬───────┘ └────┬────┘ └────┬─────┘                                    │
                                    │                   │              │           │                                          │
                                    │              exit(N)             │           │                                          │
                                    │                                  │           │                                          │
                                    │                    ┌─────────────┘           │                                          │
                                    │                    │                         │                                          │
                                    │          3 consecutive              monitor reconnects                                  │
                                    │          health check                       │                                          │
                                    │          failures                           │                                          │
                                    │                    │                         ▼                                          │
                                    │                    │                  ┌────────────┐                                    │
                                    │                    └────────────────► │ DEGRADED   │◄──── recovery fails                │
                                    │                                      │            │      (monitor dies again)           │
                                    │                                      └──────┬─────┘                                    │
                                    │                                             │                                          │
                                    │                                    monitor comes back                                   │
                                    │                                    (health check 200)                                   │
                                    │                                             │                                          │
                                    │                                             ▼                                          │
                                    │                                      ┌────────────┐                                    │
                                    │                                      │ RECOVERING │                                    │
                                    │                                      └──────┬─────┘                                    │
                                    │                                             │                                          │
                                    │                                    state sync completes                                 │
                                    │                                             │                                          │
                                    │                                             ▼                                          │
                                    │                                      ┌─────────┐                                       │
                                    │                                      │ HEALTHY │                                       │
                                    │                                      └─────────┘                                       │
                                    │                                                                                         │
                                    │   ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─   │
                                    │                                                                                         │
                                    │   From ANY of {Healthy, Degraded, Recovering}:                                          │
                                    │                                                                                         │
                                    │       SIGTERM / SIGINT / Kill Switch Level 3                                             │
                                    │                         │                                                               │
                                    │                         ▼                                                               │
                                    │                  ┌──────────────┐                                                       │
                                    │                  │ SHUTTING_DOWN │                                                       │
                                    │                  │  (terminal)   │                                                       │
                                    │                  └──────┬────────┘                                                       │
                                    │                         │                                                               │
                                    │                    7-step sequence                                                       │
                                    │                    (S1-S7, max 60s)                                                      │
                                    │                         │                                                               │
                                    │                         ▼                                                               │
                                    │                    exit(0) or exit(1)                                                    │
                                    │                                                                                         │
                                    └─────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 10. TIMING BUDGET SUMMARY

| Phase | Component | Timeout | Notes |
|-------|-----------|---------|-------|
| Bootstrap Step 1 | Config load | No timeout (sync, fast) | Fails immediately on parse error |
| Bootstrap Step 2 | Migrations | No timeout (sync) | SQLite busy_timeout = 5s handles lock contention |
| Bootstrap Step 3 | Monitor check | 5s per attempt × 3 attempts = 15s max | Only step that degrades instead of fatals |
| Bootstrap Step 4 | Agent/channel init | No timeout (sync) | Channel adapter connect() has per-adapter timeouts |
| Bootstrap Step 5 | API server bind | No timeout (sync) | Fails immediately if port in use |
| Health check | Per check | 5s timeout | Runs every 30s |
| Health check | Failure threshold | 3 consecutive = 90s | Before transitioning to Degraded |
| Reconnection | Backoff | 5s → 10s → 20s → 40s → 80s → 160s → 300s cap | With ±20% jitter |
| Recovery R1 | Stability check | 3 × 5s = 15s | Must pass all 3 |
| Recovery R2 | Event replay | No hard timeout | Rate limited at 500 events/sec |
| Recovery R3 | Score recalc | 30s timeout | Proceeds on timeout |
| Shutdown S1 | Stop connections | Immediate | |
| Shutdown S2 | Drain queues | 30s max | |
| Shutdown S3 | Memory flush | 30s max (15s per session) | Skipped on kill switch |
| Shutdown S4 | Cost persist | 2s | |
| Shutdown S5 | Monitor notify | 2s | Skipped if degraded |
| Shutdown S6 | Channel close | 5s | |
| Shutdown S7 | SQLite close | 5s | |
| Shutdown total | Forced exit | 60s absolute max | Second signal = immediate exit |

---

## 11. ITP BUFFER SPECIFICATION

The ITP buffer is the bridge between degraded mode and recovery. It must be
specified precisely to avoid data loss or corruption.

### File: `ghost-agent-loop/src/itp_emitter.rs`

```rust
/// ITP event emission with automatic degraded-mode buffering.
///
/// In HEALTHY state: events sent directly to monitor via unix socket (primary)
///   or HTTP POST (fallback). Unix socket preferred for lower latency and no
///   auth overhead. Falls back to HTTP if socket unavailable.
///   See: convergence-monitor/transport/unix_socket.rs (primary)
///        convergence-monitor/transport/http_api.rs (fallback)
/// In DEGRADED state: events written to local JSONL buffer files.
/// In RECOVERING state: events still buffered (recovery coordinator handles replay).

pub struct AgentITPEmitter {
    /// Monitor unix socket path for direct sending (primary transport)
    monitor_socket_path: Option<PathBuf>,
    /// Monitor HTTP address for fallback sending
    monitor_address: String,
    /// Gateway state for routing decision
    gateway_state: Arc<AtomicU8>,
    /// Channel to ITP buffer writer (for degraded mode)
    buffer_tx: mpsc::Sender<ITPEvent>,
    /// HTTP client for fallback monitor communication
    client: reqwest::Client,
}

impl AgentITPEmitter {
    /// Emit an ITP event. Non-blocking. Never fails the agent loop.
    ///
    /// Routing:
    /// - Healthy: POST to monitor /events (fire-and-forget, no await on response)
    /// - Degraded/Recovering: send to buffer channel
    /// - ShuttingDown: drop silently
    pub async fn emit(&self, event: ITPEvent) {
        let state = GatewayState::from_u8(
            self.gateway_state.load(Ordering::Acquire)
        );

        match state {
            GatewayState::Healthy => {
                // Primary: Unix domain socket (lower latency, no auth needed).
                // Fallback: HTTP POST if socket unavailable.
                // Fire-and-forget in both cases. Don't await response.
                // If this fails, the event is lost — acceptable because
                // the monitor is considered healthy and will have the
                // session context from prior events.
                let socket_path = self.monitor_socket_path.clone();
                let http_url = format!("http://{}/events", self.monitor_address);
                let client = self.client.clone();
                let body = serde_json::to_string(&event)
                    .unwrap_or_default();
                tokio::spawn(async move {
                    // Try unix socket first
                    if let Some(ref path) = socket_path {
                        if let Ok(stream) = tokio::net::UnixStream::connect(path).await {
                            let (_, mut writer) = tokio::io::split(stream);
                            use tokio::io::AsyncWriteExt;
                            let msg = format!("{}\n", body);
                            if writer.write_all(msg.as_bytes()).await.is_ok() {
                                return; // Success via unix socket
                            }
                        }
                        // Unix socket failed — fall through to HTTP
                    }
                    // Fallback: HTTP POST
                    let _ = client.post(&http_url)
                        .header("Content-Type", "application/json")
                        .body(body)
                        .send()
                        .await;
                });
            }
            GatewayState::Degraded | GatewayState::Recovering => {
                // Buffer locally. If channel is full, drop oldest.
                if self.buffer_tx.try_send(event).is_err() {
                    tracing::warn!("ITP buffer channel full, event dropped");
                    metrics::counter!("itp_events_dropped").increment(1);
                }
            }
            _ => {
                // Initializing, ShuttingDown, FatalError — drop
            }
        }
    }
}
```

### Buffer File Format

```
~/.ghost/sessions/buffer/
├── itp_buffer_1709078400000.jsonl    # Timestamp-named files
├── itp_buffer_1709078700000.jsonl    # New file every 5 minutes or 1000 events
└── itp_buffer_1709079000000.jsonl    # (whichever comes first)
```

Each line is a complete JSON ITP event. Files are named with Unix epoch milliseconds
for deterministic ordering during replay.

Limits:
- Max total buffer size: 10MB
- Max events in buffer: 10,000
- When limit reached: oldest file deleted (FIFO)
- On recovery: files replayed oldest-first, then deleted

---

## 12. CROSS-CUTTING CONCERNS

### 12A. Metrics Emitted by State Machine

| Metric | Type | Labels | When |
|--------|------|--------|------|
| `gateway_state` | Gauge | `state` | Every state change |
| `gateway_state_transition_total` | Counter | `from`, `to`, `reason` | Every transition |
| `gateway_degraded_duration_seconds` | Histogram | | On exit from Degraded |
| `gateway_recovery_duration_seconds` | Histogram | | On successful recovery |
| `monitor_health_check_total` | Counter | `result` (ok/fail/timeout) | Every health check |
| `monitor_reconnection_attempts_total` | Counter | `result` (ok/fail) | Every reconnection attempt |
| `itp_events_buffered_total` | Counter | | Every buffered event |
| `itp_events_dropped_total` | Counter | | When buffer is full |
| `itp_events_replayed_total` | Counter | `result` (ok/fail) | During recovery replay |
| `shutdown_duration_seconds` | Histogram | `reason` | On shutdown complete |

### 12B. Audit Log Events

Every state transition is logged to the append-only audit trail (ghost-audit):

```json
{
  "event_type": "gateway_state_transition",
  "timestamp": "2026-02-27T14:30:00Z",
  "from_state": "healthy",
  "to_state": "degraded",
  "reason": "monitor_unreachable",
  "details": {
    "consecutive_failures": 3,
    "last_check_error": "connection refused",
    "monitor_address": "127.0.0.1:9100"
  }
}
```

### 12C. Dashboard WebSocket Events

The dashboard receives real-time state change notifications:

```json
{
  "event": "gateway_state_change",
  "state": "degraded",
  "previous_state": "healthy",
  "timestamp": "2026-02-27T14:30:00Z",
  "degraded_features": [
    "convergence_scoring",
    "convergence_aware_memory_filtering",
    "convergence_aware_decay",
    "intervention_triggers",
    "session_boundary_escalation",
    "behavioral_verification",
    "t7_memory_health_path_a_b"
  ],
  "stale_state_features": [
    "convergence_policy_tightener",
    "heartbeat_frequency",
    "reflection_depth_bounding",
    "memory_filtering_tier",
    "proposal_validation_d5_d7"
  ],
  "active_fallbacks": [
    "t7_memory_health_path_c"
  ],
  "active_features": [
    "agent_loop",
    "simulation_boundary_prompt",
    "base_session_limits",
    "kill_switch",
    "audit_logging",
    "proposal_validation_d1_d4"
  ]
}
```

### 12D. Configuration (ghost.yml convergence section)

```yaml
convergence:
  monitor:
    address: "127.0.0.1:9100"
    socket_path: "/tmp/ghost-monitor.sock"  # Unix socket (primary ITP transport)
    health_check_interval_seconds: 30
    health_check_timeout_seconds: 5
    failure_threshold: 3
    reconnection_backoff_base_seconds: 5
    reconnection_backoff_max_seconds: 300
    reconnection_jitter_percent: 20
  shared_state:
    path: "~/.ghost/data/convergence_state/"  # Shared state file directory
    poll_interval_seconds: 1  # How often consumers poll for changes
  itp_buffer:
    max_size_bytes: 10485760  # 10MB
    max_events: 10000
    rotation_interval_seconds: 300  # 5 minutes
    rotation_event_count: 1000
  recovery:
    stability_checks: 3
    stability_check_interval_seconds: 5
    replay_batch_size: 100
    replay_rate_limit_events_per_second: 500
    recalculation_timeout_seconds: 30
  profile: "standard"  # standard | research | companion | productivity
```

---

## 13. EDGE CASES AND INVARIANTS

### Edge Case 1: Monitor crashes during bootstrap step 4 or 5
- Step 3 passed (monitor was healthy), but monitor dies before step 5 completes.
- Gateway enters HEALTHY state because step 3 passed.
- MonitorHealthChecker detects the crash within 90 seconds (3 × 30s).
- Transitions to DEGRADED normally.
- No special handling needed — the health checker covers this.
- See: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4 for monitor crash behavior.

### Edge Case 2: Monitor crash-loops (starts, dies, starts, dies)
- Reconnector connects → transitions to RECOVERING.
- Recovery R1 stability check fails (monitor dies during 15s check window).
- Returns to DEGRADED. Reconnector restarts.
- This can repeat indefinitely. Each cycle is logged.
- The backoff resets on each successful reconnection, so rapid crash-loops
  don't cause rapid reconnection attempts (R1 takes 15s minimum).

### Edge Case 3: Gateway starts, monitor starts later
- Bootstrap step 3 fails → DEGRADED.
- Reconnector runs with backoff.
- When monitor eventually starts, reconnector detects it.
- Recovery runs. No buffered events to replay (nothing happened yet, or
  events were buffered and get replayed).
- Transitions to HEALTHY.

### Edge Case 4: ITP buffer fills up during extended outage
- Buffer hits 10MB or 10K events.
- Oldest buffer file deleted (FIFO).
- Events are lost. This is acceptable — the alternative is unbounded disk usage.
- On recovery, the monitor will have a gap in its data.
- Convergence scores will be less accurate for the gap period but will
  converge to correct values as new events flow in.
- Metric `itp_events_dropped_total` tracks the loss.

### Edge Case 5: Shutdown during recovery
- Gateway is in RECOVERING state (replaying events).
- SIGTERM received.
- Transitions to SHUTTING_DOWN immediately.
- Recovery coordinator detects state change and aborts.
- Unreplayed buffer events are preserved on disk.
- Next gateway start will be a fresh bootstrap (not a recovery resume).
- If monitor is reachable at next start, buffered events from the
  previous session can be replayed during the next recovery cycle.
- NOTE: Buffer files are NOT deleted on shutdown. They persist across restarts.
- See: AGENT_LOOP_SEQUENCE_FLOW.md for ITP emission failure handling during shutdown.

### Edge Case 6: Multiple gateways pointing at same monitor
- Not a supported configuration in v1 (single-box deployment).
- If it happens: each gateway has its own health checker and reconnector.
- The jitter on reconnection backoff prevents thundering herd.
- Monitor must handle concurrent connections (it already does via HTTP).

### Edge Case 7: Monitor version upgrade while gateway is running
- Health check validates semver major version match.
- If monitor upgrades to incompatible major version:
  health check returns 200 but version check fails.
- Treated as unreachable → transitions to DEGRADED.
- Reconnector will keep trying but version check will keep failing.
- Requires gateway restart with compatible monitor version.
- Log message explicitly states version mismatch.

### Invariants (Must Hold at All Times)

1. **Exactly one state**: `GatewayState` is always exactly one of the 6 values. AtomicU8 ensures this.
2. **No silent degradation**: Every transition to DEGRADED logs at CRITICAL level and emits a metric.
3. **Agents never block on monitor**: ITP emission is always non-blocking. Monitor unavailability never stalls the agent loop.
4. **Buffer is bounded**: ITP buffer never exceeds configured limits. Oldest events dropped, not newest.
5. **Recovery is idempotent**: If recovery is interrupted and restarted, replaying already-replayed events is safe (monitor deduplicates by event_id).
6. **Shutdown always completes**: 60-second forced exit timer guarantees the process terminates even if subsystems hang.
7. **Second signal = immediate exit**: No stuck shutdown. User always has an escape hatch.
8. **Simulation boundary is state-independent**: The `const &str` prompt is compiled into the binary. It works in ALL states including DEGRADED. This is the one safety feature that can never degrade.
9. **Kill switch is state-independent**: Kill switch is gateway-owned. Works in Healthy, Degraded, and Recovering states. T7 memory health uses Path C fallback in Degraded mode (direct cortex queries, stricter threshold < 0.2). Other auto-triggers (T1-T6) are fully active — none depend on convergence monitor. See: KILL_SWITCH_TRIGGER_CHAIN_SEQUENCE_FLOW.md §2.8.
10. **Audit logging is state-independent**: All state transitions, all tool executions, all policy decisions are logged regardless of gateway state.
11. **Stale state is conservative**: When the monitor dies, the shared state file persists on disk with last-known intervention level. All consumers (policy engine, heartbeat, session boundary, memory filtering) read this stale state and KEEP the last-known restrictions. Never fall back to Level 0 on monitor loss. See: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §8.4.

---

## 14. FILE OWNERSHIP SUMMARY

| File | Owns | Key Types/Functions |
|------|------|---------------------|
| `gateway.rs` | State machine, subsystem orchestration | `Gateway`, `GatewayState`, `GatewaySharedState` |
| `bootstrap.rs` | 5-step startup sequence | `GatewayBootstrap`, `BootstrapResult`, `BootstrapError` |
| `health.rs` | Monitor health checking, reconnection, recovery | `MonitorHealthChecker`, `MonitorReconnector`, `RecoveryCoordinator`, `MonitorHealthConfig`, `MonitorConnection` |
| `shutdown.rs` | 7-step graceful shutdown | `ShutdownCoordinator`, `ShutdownReason` |
| `ghost-agent-loop/itp_emitter.rs` | ITP event routing (unix socket primary, HTTP fallback, buffer) | `AgentITPEmitter` |
| `ghost-agent-loop/runner.rs` | Degraded-aware agent turn execution | `AgentRunner::run_turn()`, `ConvergenceContext` |

---

## 15. IMPLEMENTATION ORDER

Build these files in this exact order. Each step is testable in isolation.

1. `gateway.rs` — `GatewayState` enum + `AtomicU8` wrapper + transition validation
2. `health.rs` — `MonitorHealthConfig`, `MonitorConnection`, `MonitorHealthChecker`
3. `bootstrap.rs` — `GatewayBootstrap::run()` with all 5 steps
4. `health.rs` — `MonitorReconnector` (add to existing file)
5. `health.rs` — `RecoveryCoordinator` (add to existing file)
6. `shutdown.rs` — `ShutdownCoordinator` with 7-step sequence
7. `gateway.rs` — `Gateway` struct with event loop and state transition handling
8. `ghost-agent-loop/itp_emitter.rs` — `AgentITPEmitter` with buffer routing
9. `ghost-agent-loop/runner.rs` — Degraded-aware `run_turn()` logic

Test at each step:
- Step 1: Unit test all valid/invalid transitions
- Step 2: Integration test with mock HTTP server
- Step 3: Integration test with real SQLite + mock monitor
- Step 4: Unit test backoff timing + jitter
- Step 5: Integration test with mock monitor that dies mid-recovery
- Step 6: Integration test with mock subsystems
- Step 7: Full integration test: bootstrap → degrade → recover → shutdown
- Step 8: Unit test routing logic per state
- Step 9: Unit test convergence context fallback
