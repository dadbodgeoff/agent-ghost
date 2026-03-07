# GHOST ADE: Implementation Document

**Version**: 1.0
**Date**: March 5, 2026
**Status**: Engineering Implementation Specification
**Companion**: GHOST_ADE_DESIGN_DOCUMENT.md v2.0

---

## 0. Executive Summary

This document translates the GHOST ADE Design Document into an executable implementation plan. Every change is grounded in the current codebase state, specifies exact file paths, function signatures, type definitions, and acceptance criteria. No hand-waving. No "improve X." Every task is atomic, testable, and ordered by dependency.

**Current codebase reality:**
- 29 Rust crates in `crates/`, 40+ API route files in `ghost-gateway`
- 6-state gateway FSM, 3-level kill switch, 6 gate checks — all implemented
- 37 forward-only SQLite migrations (v016–v037)
- SvelteKit dashboard with 9 Svelte 5 rune-based stores
- Browser extension monitoring 6 AI platforms
- TypeScript SDK in `packages/sdk/`
- Tauri v2 desktop shell with sidecar gateway management

**Gap analysis summary:**
- **Critical (safety):** Single `Arc<Mutex<Connection>>` bottleneck, `std::sync::Mutex` in async context, `"args": true` in Tauri capabilities, missing `Resync` subscriptions in 4 stores
- **High (correctness):** No WS event sequence numbers, incomplete SSE stream detection, `catch (e: any)` in 36 locations, API client returns `Promise<any>`
- **Medium (UX):** Missing command palette agent commands, no breadcrumb navigation, no virtual list for chat, no notification panel
- **Low (polish):** Missing autonomy dial, no onboarding flow, no auto-update

---

## 1. Dependency Graph

```
Phase 1: Safety Hardening (Weeks 1-4)
├── 1.1 SQLite Connection Pool                    [CRITICAL, no deps]
├── 1.2 Tauri Capability Lockdown                 [CRITICAL, no deps]
├── 1.3 Kill State Write Ordering Fix             [CRITICAL, no deps]
├── 1.4 Mutex Migration (std → tokio)             [HIGH, no deps]
├── 1.5 GhostError Unification                    [HIGH, no deps]
├── 1.6 WS Event Sequence Numbers                 [HIGH, depends on 1.1]
├── 1.7 Frontend Type Safety Pass                 [HIGH, no deps]
├── 1.8 Store Resync Subscriptions                [HIGH, depends on 1.6]
├── 1.9 SSE Incomplete Stream Detection           [MEDIUM, no deps]
├── 1.10 ARIA & Accessibility Foundation          [MEDIUM, no deps]
├── 1.11 E2E Test Infrastructure                  [HIGH, depends on 1.1]
└── 1.12 RBAC Middleware                          [HIGH, depends on 1.5]

Phase 2: Core ADE Experience (Weeks 5-8)
├── 2.1 Command Palette Enhancement               [depends on 1.7]
├── 2.2 Keyboard Shortcuts System                  [no deps]
├── 2.3 CodeMirror 6 Studio Input                  [depends on 1.9]
├── 2.4 Artifact Panel                             [depends on 2.3]
├── 2.5 Agent Creation Wizard                      [depends on 1.7, 1.12]
├── 2.6 Approval Queue UI                          [depends on 1.8]
├── 2.7 Pagination (cursor-based)                  [depends on 1.1]
├── 2.8 Virtual List for Chat                      [depends on 1.9]
├── 2.9 Breadcrumb Navigation                      [no deps]
├── 2.10 Notification Panel                        [depends on 1.8]
├── 2.11 Browser Extension Port Fix                [no deps]
└── 2.12 S8 Behavioral Anomaly Signal              [depends on 1.6]

Phase 3: Subsystem Surfaces (Weeks 9-14)
├── 3.1 Channels Management UI                     [depends on 2.7]
├── 3.2 PC Control Dashboard                       [depends on 2.7]
├── 3.3 ITP Event Viewer                           [depends on 2.7, 2.10]
├── 3.4 Workflow Canvas (d3-force)                  [depends on 2.7]
├── 3.5 Workflow Execution Runtime                  [depends on 3.4]
├── 3.6 Knowledge Graph View                        [depends on 2.7]
├── 3.7 Trust Graph Visualization                   [depends on 3.6]
├── 3.8 Enhanced Convergence Dashboard              [depends on 2.12]
├── 3.9 Session Replay (bookmarks, branching)       [depends on 2.8]
└── 3.10 Three-View Execution Visualization         [depends on 3.5]

Phase 4: Polish and Production (Weeks 15-20)
├── 4.1 Auto-Update Mechanism                       [no deps]
├── 4.2 Autonomy Dial UI                            [depends on 3.8]
├── 4.3 Safety Profiles                             [depends on 3.8]
├── 4.4 Cost Alerting & Anomaly Detection           [depends on 2.7]
├── 4.5 Sandbox Mode (dry-run)                      [depends on 3.5]
├── 4.6 MCP Compatibility Layer                     [depends on 3.5]
├── 4.7 Data Retention Automation                   [depends on 1.1]
├── 4.8 First-Run Onboarding                        [depends on 2.5]
├── 4.9 Deep-Link Handler                           [no deps]
├── 4.10 Performance Optimization Pass              [depends on all]
└── 4.11 Responsive Design Pass                     [depends on all]
```

---

## 2. Phase 1: Safety Hardening

### 2.1 SQLite Connection Pool

**Priority:** CRITICAL
**Risk:** The single `Arc<Mutex<rusqlite::Connection>>` in `AppState` serializes every DB operation across all API handlers, WS events, audit writes, and session queries. Under concurrent load, this causes lock contention, request queuing, and potential deadlocks when held across `.await` points.

**Current state** (`crates/ghost-gateway/src/state.rs`):
```rust
pub struct AppState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    // ... other fields
}
```

**Target state — Read/Write Separation:**

**File:** `crates/ghost-gateway/src/db_pool.rs` (NEW)
```rust
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use tokio::sync::Mutex as TokioMutex;
use crossbeam_queue::ArrayQueue;
use std::sync::Arc;

/// Read/write separated connection pool for SQLite WAL mode.
///
/// Architecture:
///   - 1 writer connection (serialized via TokioMutex, safe across .await)
///   - N reader connections (lock-free pool via ArrayQueue)
///   - WAL mode enables concurrent readers + single writer
///
/// Invariants:
///   - Writer connection is NEVER used for reads (prevents writer starvation)
///   - Reader connections are opened with SQLITE_OPEN_READ_ONLY
///   - busy_timeout = 5000ms on all connections (already configured in bootstrap)
///   - All connections share the same WAL file
pub struct DbPool {
    writer: TokioMutex<Connection>,
    readers: ArrayQueue<Connection>,
    db_path: PathBuf,
    pool_size: usize,
}

/// RAII guard that returns reader connection to pool on drop.
pub struct ReadConn<'a> {
    conn: Option<Connection>,
    pool: &'a DbPool,
}

impl DbPool {
    /// Create pool with 1 writer + `pool_size` readers.
    /// Default pool_size: min(num_cpus, 8), minimum 2.
    pub fn open(db_path: PathBuf, pool_size: usize) -> Result<Self, DbPoolError> {
        // Writer: read-write, WAL mode, busy_timeout 5000ms
        let writer = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        writer.pragma_update(None, "journal_mode", "WAL")?;
        writer.pragma_update(None, "busy_timeout", 5000)?;
        writer.pragma_update(None, "synchronous", "NORMAL")?;
        writer.pragma_update(None, "foreign_keys", "ON")?;

        // Readers: read-only, same pragmas
        let readers = ArrayQueue::new(pool_size);
        for _ in 0..pool_size {
            let r = Connection::open_with_flags(
                &db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            r.pragma_update(None, "busy_timeout", 5000)?;
            readers.push(r).map_err(|_| DbPoolError::PoolFull)?;
        }

        Ok(Self {
            writer: TokioMutex::new(writer),
            readers,
            db_path,
            pool_size,
        })
    }

    /// Acquire write connection. Holds TokioMutex — safe across .await.
    pub async fn write(&self) -> tokio::sync::MutexGuard<'_, Connection> {
        self.writer.lock().await
    }

    /// Acquire read connection from pool. Returns RAII guard.
    /// If pool empty, opens a new temporary read-only connection (bounded to 2x pool_size).
    pub fn read(&self) -> Result<ReadConn<'_>, DbPoolError> {
        match self.readers.pop() {
            Some(conn) => Ok(ReadConn { conn: Some(conn), pool: self }),
            None => {
                // Pool exhausted — create temporary overflow connection
                let conn = Connection::open_with_flags(
                    &self.db_path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                conn.pragma_update(None, "busy_timeout", 5000)?;
                // This ReadConn won't return to pool (pool is full)
                Ok(ReadConn { conn: Some(conn), pool: self })
            }
        }
    }

    /// WAL checkpoint (call during shutdown or scheduled maintenance).
    pub async fn checkpoint(&self) -> Result<(), DbPoolError> {
        let w = self.writer.lock().await;
        w.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
        Ok(())
    }
}

impl<'a> std::ops::Deref for ReadConn<'a> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("ReadConn used after drop")
    }
}

impl<'a> Drop for ReadConn<'a> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            // Try to return to pool; if full, connection is simply dropped
            let _ = self.pool.readers.push(conn);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DbPoolError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Connection pool is full")]
    PoolFull,
}
```

**Migration steps:**
1. Add `crossbeam-queue = "0.3"` to `ghost-gateway/Cargo.toml`
2. Create `db_pool.rs` as above
3. Change `AppState.db` type from `Arc<Mutex<Connection>>` to `Arc<DbPool>`
4. Update `bootstrap.rs`: construct `DbPool::open()` instead of single connection
5. Search all `state.db.lock()` call sites (grep for `\.db\.lock\(\)` and `state\.db`):
   - **Read operations** → `state.db.read()?` (returns `ReadConn`)
   - **Write operations** → `state.db.write().await` (returns `MutexGuard`)
6. Update `runtime.rs` shutdown: call `state.db.checkpoint().await`

**Affected files (exhaustive list — grep `state.db` across crates/ghost-gateway/):**
- `src/api/agents.rs` — agent CRUD (reads + writes)
- `src/api/sessions.rs` — session queries (reads)
- `src/api/studio_sessions.rs` — studio session CRUD (reads + writes)
- `src/api/memory.rs` — memory search (reads)
- `src/api/audit.rs` — audit log queries (reads)
- `src/api/convergence.rs` — convergence scores (reads)
- `src/api/costs.rs` — cost queries (reads)
- `src/api/goals.rs` — proposal CRUD (reads + writes)
- `src/api/safety.rs` — safety state (reads + writes)
- `src/api/webhooks.rs` — webhook CRUD (reads + writes)
- `src/api/skill_execute.rs` — skill execution (reads)
- `src/api/workflows.rs` — workflow CRUD (reads + writes)
- `src/api/a2a.rs` — A2A task management (reads + writes)
- `src/bootstrap.rs` — migrations (writes)
- `src/runtime.rs` — shutdown checkpoint
- `src/safety/kill_switch.rs` — kill state persistence (writes)
- `src/safety/auto_triggers.rs` — trigger evaluation (reads)
- `src/session/manager.rs` — session management (reads + writes)
- `src/cost/tracker.rs` — cost recording (writes)

**Acceptance criteria:**
- [ ] `cargo test` passes with no deadlock under concurrent load
- [ ] Benchmark: 100 concurrent read queries complete in < 500ms (vs current serial)
- [ ] Write operations remain serialized (no SQLITE_BUSY errors)
- [ ] WAL checkpoint runs on shutdown
- [ ] No `std::sync::Mutex<Connection>` remains in codebase

---

### 2.2 Tauri Capability Lockdown

**Priority:** CRITICAL
**Risk:** `"args": true` in shell permissions allows arbitrary argument injection to the `ghost` sidecar binary.

**Current state** (`src-tauri/capabilities/default.json`):
```json
{
  "permissions": [
    "shell:default",
    {
      "identifier": "shell:allow-execute",
      "allow": [{ "name": "ghost", "args": true }]
    }
  ]
}
```

**Target state:**
```json
{
  "identifier": "shell:allow-execute",
  "allow": [{
    "name": "ghost",
    "args": [
      "serve",
      "--config",
      { "validator": "^[a-zA-Z0-9_.\\-/~]+$" },
      "--port",
      { "validator": "^[0-9]{1,5}$" },
      "--bind",
      { "validator": "^[0-9.]+$" }
    ]
  }]
}
```

**File:** `src-tauri/capabilities/default.json`

**Acceptance criteria:**
- [ ] `ghost serve --config <path>` works
- [ ] `ghost serve --config "$(malicious command)"` is rejected by Tauri
- [ ] No `"args": true` remains in any capability file

---

### 2.3 Kill State Write Ordering Fix

**Priority:** CRITICAL
**Risk:** If gateway crashes after setting `PLATFORM_KILLED` atomic but before writing `kill_state.json`, the kill switch is lost on restart. Design doc specifies: write file FIRST (fsync), THEN set atomic bool.

**Current state** (`crates/ghost-gateway/src/safety/kill_switch.rs`):
The implementation needs verification of ordering. The atomic `PLATFORM_KILLED` and file persistence must be sequenced correctly.

**Target implementation:**
```rust
/// Activate platform-wide kill switch.
/// INVARIANT: File write (with fsync) MUST complete before atomic bool is set.
/// This ensures crash recovery always finds the kill state on disk.
pub async fn activate_kill_all(
    &self,
    reason: String,
    triggered_by: String,
) -> Result<(), KillSwitchError> {
    // Step 1: Prepare kill state
    let kill_state = KillStateFile {
        active: true,
        level: KillLevel::KillAll,
        reason: reason.clone(),
        triggered_by: triggered_by.clone(),
        activated_at: chrono::Utc::now().to_rfc3339(),
    };

    // Step 2: Write to disk FIRST (fsync for durability)
    let state_path = self.data_dir.join("kill_state.json");
    let json = serde_json::to_string_pretty(&kill_state)
        .map_err(|e| KillSwitchError::Serialization(e.to_string()))?;

    // Atomic write: write to temp file, fsync, rename
    let tmp_path = state_path.with_extension("json.tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|e| KillSwitchError::FileWrite(e.to_string()))?;
        std::io::Write::write_all(&mut file, json.as_bytes())
            .map_err(|e| KillSwitchError::FileWrite(e.to_string()))?;
        file.sync_all()  // fsync — data is durable on disk
            .map_err(|e| KillSwitchError::FileWrite(e.to_string()))?;
    }
    std::fs::rename(&tmp_path, &state_path)
        .map_err(|e| KillSwitchError::FileWrite(e.to_string()))?;

    // Step 3: THEN set atomic bool (SeqCst for cross-thread visibility)
    PLATFORM_KILLED.store(true, Ordering::SeqCst);

    // Step 4: Update in-memory state
    {
        let mut state = self.state.write().map_err(|e| {
            // Lock poisoned — flag is already set, we're safe
            tracing::error!("Kill switch lock poisoned during kill_all: {}", e);
            KillSwitchError::LockPoisoned
        })?;
        state.level = KillLevel::KillAll;
        state.reason = Some(reason.clone());
        state.activated_at = Some(chrono::Utc::now());
    }

    // Step 5: Emit event (best-effort — kill is already durable)
    let _ = self.event_tx.send(WsEvent::KillSwitchActivation {
        level: KillLevel::KillAll,
        agent_id: None,
        reason,
    });

    Ok(())
}
```

**Bootstrap check** (`crates/ghost-gateway/src/bootstrap.rs`):
```rust
// MUST be called before ANY agent initialization
fn check_kill_state(data_dir: &Path) -> bool {
    let state_path = data_dir.join("kill_state.json");
    if state_path.exists() {
        match std::fs::read_to_string(&state_path) {
            Ok(json) => match serde_json::from_str::<KillStateFile>(&json) {
                Ok(state) if state.active => {
                    tracing::warn!(
                        "Kill state found on disk — entering safe mode. \
                         Reason: {}. Activated: {}",
                        state.reason,
                        state.activated_at
                    );
                    PLATFORM_KILLED.store(true, Ordering::SeqCst);
                    return true;
                }
                _ => {}
            },
            Err(e) => {
                // File exists but unreadable — fail closed (treat as active)
                tracing::error!(
                    "Cannot read kill_state.json ({}). Failing closed — safe mode.",
                    e
                );
                PLATFORM_KILLED.store(true, Ordering::SeqCst);
                return true;
            }
        }
    }
    false
}
```

**Acceptance criteria:**
- [ ] Kill-all writes file before setting atomic (verified by adding crash simulation test)
- [ ] Bootstrap detects `kill_state.json` and enters safe mode before agent init
- [ ] Unreadable `kill_state.json` → fail closed (safe mode)
- [ ] Temp file + rename pattern prevents partial writes

---

### 2.4 Mutex Migration (std → tokio)

**Priority:** HIGH
**Risk:** `std::sync::Mutex` held across `.await` points can deadlock the tokio runtime. The gateway uses `std::sync::Mutex<Connection>` which is accessed in async handlers.

**Changes required:**

**File:** `src-tauri/src/commands/gateway.rs`
```rust
// BEFORE:
pub struct GatewayProcess(pub Mutex<Option<CommandChild>>);

// AFTER:
pub struct GatewayProcess(pub tokio::sync::Mutex<Option<CommandChild>>);
```

All `.lock().unwrap()` calls on `GatewayProcess` become `.lock().await`:
```rust
// BEFORE:
let mut process = state.gateway.0.lock().unwrap();

// AFTER:
let mut process = state.gateway.0.lock().await;
```

**Note:** The `AppState.db` mutex migration is handled by Task 2.1 (DbPool replaces it entirely). This task covers all OTHER `std::sync::Mutex` usages in async contexts.

**Grep for remaining `std::sync::Mutex` in async code:**
- `src-tauri/src/commands/gateway.rs` — `GatewayProcess`
- `crates/ghost-gateway/src/state.rs` — `background_tasks`, `embedding_engine`
- Any other `Mutex<...>` that is `.lock()`'d inside an `async fn`

**Acceptance criteria:**
- [ ] No `std::sync::Mutex` is held across an `.await` point
- [ ] `GatewayProcess` uses `tokio::sync::Mutex`
- [ ] `background_tasks` uses `tokio::sync::Mutex`

---

### 2.5 GhostError Unification

**Priority:** HIGH

**Current state:** Multiple error types across crates — `CortexError`, `ApiError`, `RunError`, `CliError`, `DbPoolError`. The design doc specifies a unified `GhostError` enum.

**Target:** Keep domain-specific error types but ensure all implement `serde::Serialize` for IPC boundary crossing, and all API-facing errors map to the JSON error envelope.

**File:** `crates/ghost-gateway/src/api/error.rs` (enhance existing)
```rust
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use serde::Serialize;

/// Canonical API error envelope. Every endpoint returns this on failure.
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    /// Machine-readable error code (e.g., "KILL_SWITCH_ACTIVE", "AGENT_NOT_FOUND")
    pub code: String,
    /// Human-readable description
    pub message: String,
    /// Optional structured details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Unified error type for all API handlers.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Not found: {entity} {id}")]
    NotFound { entity: String, id: String },

    #[error("Validation error: {message}")]
    Validation { message: String },

    #[error("Authorization denied: {reason}")]
    Unauthorized { reason: String },

    #[error("Forbidden: {reason}")]
    Forbidden { reason: String },

    #[error("Conflict: {message}")]
    Conflict { message: String },

    #[error("Kill switch active")]
    KillSwitchActive,

    #[error("Database error: {0}")]
    Database(String),

    #[error("Lock poisoned: {resource}")]
    LockPoisoned { resource: String },

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::NotFound { .. } => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            ApiError::Validation { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            ApiError::Unauthorized { .. } => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            ApiError::Forbidden { .. } => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            ApiError::Conflict { .. } => (StatusCode::CONFLICT, "CONFLICT"),
            ApiError::KillSwitchActive => (StatusCode::SERVICE_UNAVAILABLE, "KILL_SWITCH_ACTIVE"),
            ApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),
            ApiError::LockPoisoned { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "LOCK_POISONED"),
            ApiError::Provider(_) => (StatusCode::BAD_GATEWAY, "PROVIDER_ERROR"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        let body = ApiErrorResponse {
            error: ApiErrorBody {
                code: code.to_string(),
                message: self.to_string(),
                details: None,
            },
        };

        (status, axum::Json(body)).into_response()
    }
}

// Conversion from domain errors
impl From<DbPoolError> for ApiError {
    fn from(e: DbPoolError) -> Self {
        ApiError::Database(e.to_string())
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError::Database(e.to_string())
    }
}

impl From<CortexError> for ApiError {
    fn from(e: CortexError) -> Self {
        match e {
            CortexError::NotFound { id } => ApiError::NotFound {
                entity: "resource".into(),
                id,
            },
            CortexError::Validation(msg) => ApiError::Validation { message: msg },
            CortexError::AuthorizationDenied { reason } => ApiError::Unauthorized { reason },
            other => ApiError::Internal(other.to_string()),
        }
    }
}
```

**File:** `src-tauri/src/error.rs` (NEW — for Tauri IPC boundary)
```rust
use serde::Serialize;

/// Error type that crosses the Tauri IPC boundary.
/// Must implement Serialize (Tauri requirement for command return types).
#[derive(Debug, thiserror::Error, Serialize)]
pub enum GhostDesktopError {
    #[error("Gateway not running")]
    GatewayNotRunning,

    #[error("Gateway failed to start: {reason}")]
    GatewayStartFailed { reason: String },

    #[error("Gateway health check failed: {reason}")]
    HealthCheckFailed { reason: String },

    #[error("Configuration error: {reason}")]
    ConfigError { reason: String },

    #[error("IO error: {reason}")]
    IoError { reason: String },
}

// Note: serde::Serialize on error enums requires all variants to be serializable.
// thiserror + Serialize is the standard pattern for Tauri v2 commands.
```

**Acceptance criteria:**
- [ ] All API handlers return `Result<_, ApiError>`
- [ ] All Tauri commands return `Result<_, GhostDesktopError>`
- [ ] No `.expect()` on fallible operations in API handlers
- [ ] JSON error envelope returned for every error response
- [ ] `cargo clippy` clean (no `let _ =` warnings on `Result`)

---

### 2.6 WebSocket Event Sequence Numbers

**Priority:** HIGH
**Depends on:** 2.1 (DbPool)

**Current state:** WS events have no sequence numbers. Clients cannot detect gaps. `Resync` fires on tokio broadcast `Lagged` error but server-side only.

**Target:** Monotonic sequence numbers on all events. Server maintains ring buffer of last 1000 events. Client sends `last_seq` on reconnect. Server replays missed events or sends `Resync` if gap too large.

**File:** `crates/ghost-gateway/src/api/websocket.rs` (modify)

```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Global monotonic sequence counter for WS events.
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Envelope wrapping every WS event with sequence metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsEnvelope {
    pub seq: u64,
    pub timestamp: String,  // ISO 8601
    #[serde(flatten)]
    pub event: WsEvent,
}

/// Ring buffer for event replay on reconnect.
pub struct EventReplayBuffer {
    buffer: parking_lot::RwLock<VecDeque<WsEnvelope>>,
    capacity: usize,
}

impl EventReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: parking_lot::RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn push(&self, envelope: WsEnvelope) {
        let mut buf = self.buffer.write();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(envelope);
    }

    /// Replay events after `last_seq`. Returns None if gap too large.
    pub fn replay_after(&self, last_seq: u64) -> Option<Vec<WsEnvelope>> {
        let buf = self.buffer.read();
        let first_available = buf.front().map(|e| e.seq).unwrap_or(0);

        if last_seq < first_available {
            // Gap too large — client must full resync
            return None;
        }

        Some(
            buf.iter()
                .filter(|e| e.seq > last_seq)
                .cloned()
                .collect()
        )
    }
}
```

**Add to `AppState`:**
```rust
pub replay_buffer: Arc<EventReplayBuffer>,
```

**Modify event broadcast (wherever `event_tx.send()` is called):**
```rust
fn broadcast_event(state: &AppState, event: WsEvent) {
    let seq = EVENT_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let envelope = WsEnvelope {
        seq,
        timestamp: chrono::Utc::now().to_rfc3339(),
        event,
    };
    state.replay_buffer.push(envelope.clone());
    let _ = state.event_tx.send(envelope);
}
```

**Modify WS handler — client reconnect protocol:**
```rust
// On WS connection, client may send: { "last_seq": 42 }
// Server responds with either:
//   - Replayed events (if available)
//   - { "type": "Resync", "missed_events": N } (if gap too large)
async fn handle_ws_connect(
    state: &AppState,
    client_last_seq: Option<u64>,
    ws_sender: &mut SplitSink<...>,
) {
    if let Some(last_seq) = client_last_seq {
        match state.replay_buffer.replay_after(last_seq) {
            Some(events) => {
                for event in events {
                    let _ = ws_sender.send(Message::Text(
                        serde_json::to_string(&event).unwrap_or_default()
                    )).await;
                }
            }
            None => {
                let current_seq = EVENT_SEQ.load(Ordering::Relaxed);
                let resync = WsEnvelope {
                    seq: current_seq,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    event: WsEvent::Resync {
                        missed_events: current_seq.saturating_sub(last_seq),
                    },
                };
                let _ = ws_sender.send(Message::Text(
                    serde_json::to_string(&resync).unwrap_or_default()
                )).await;
            }
        }
    }
}
```

**Frontend change** (`dashboard/src/lib/stores/websocket.svelte.ts`):
```typescript
// Add to WebSocketStore class:
private lastSeq: number = 0;

// In message handler:
private handleMessage(data: string) {
    const envelope = JSON.parse(data) as { seq: number; timestamp: string; type?: string };
    if (envelope.seq) {
        this.lastSeq = envelope.seq;
    }
    // ... existing handler dispatch
}

// On reconnect:
private connect() {
    // ... existing connection logic
    ws.onopen = () => {
        // Send last_seq to server for replay
        if (this.lastSeq > 0) {
            ws.send(JSON.stringify({ last_seq: this.lastSeq }));
        }
    };
}
```

**Acceptance criteria:**
- [ ] Every WS event has monotonic `seq` and `timestamp`
- [ ] Server replays missed events on reconnect (up to 1000)
- [ ] If gap > 1000, server sends `Resync` with `missed_events` count
- [ ] Client tracks `lastSeq` across reconnections
- [ ] Ring buffer bounded to 1000 events (no unbounded memory growth)

---

### 2.7 Frontend Type Safety Pass

**Priority:** HIGH

**Current state:** 36 instances of `catch (e: any)`, API client returns `Promise<any>`, window casts to `any`.

**Changes:**

**File:** `dashboard/src/lib/api.ts`
```typescript
// BEFORE:
async get(path: string): Promise<any> { ... }
async post(path: string, body?: any): Promise<any> { ... }
async put(path: string, body?: any): Promise<any> { ... }
async del(path: string): Promise<any> { ... }

// AFTER:
async get<T = unknown>(path: string): Promise<T> { ... }
async post<T = unknown>(path: string, body?: unknown): Promise<T> { ... }
async put<T = unknown>(path: string, body?: unknown): Promise<T> { ... }
async del<T = unknown>(path: string): Promise<T> { ... }
```

**Catch block migration (all 36 instances):**
```typescript
// BEFORE:
catch (e: any) {
    this.error = e.message || 'Something went wrong';
}

// AFTER:
catch (e: unknown) {
    this.error = e instanceof Error ? e.message : String(e);
}
```

**Window/global access (typed):**
```typescript
// File: dashboard/src/lib/env.ts (NEW)

/** Type-safe access to runtime globals injected by Tauri or build system. */

interface TauriGlobal {
    invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
    event: {
        listen(event: string, handler: (payload: unknown) => void): Promise<() => void>;
    };
}

export function getTauriGlobal(): TauriGlobal | undefined {
    return (window as Record<string, unknown>).__TAURI__ as TauriGlobal | undefined;
}

export function getGatewayPort(): number | undefined {
    return (window as Record<string, unknown>).__GHOST_GATEWAY_PORT__ as number | undefined;
}

export function isTauri(): boolean {
    return getTauriGlobal() !== undefined;
}
```

**Acceptance criteria:**
- [ ] Zero `catch (e: any)` in codebase (grep confirms)
- [ ] Zero `Promise<any>` in `api.ts`
- [ ] All store methods use typed API calls: `api.get<AgentList>('/agents')`
- [ ] `svelte-check` passes with no new errors

---

### 2.8 Store Resync Subscriptions

**Priority:** HIGH
**Depends on:** 2.6 (WS Sequence Numbers)

**Current state:** Only `agents`, `convergence`, and `safety` stores subscribe to WS events. Sessions, costs, memory, and audit stores are REST-only with no `Resync` subscription.

**Target:** Every store subscribes to `Resync` and calls its `refresh()` method.

**Pattern to apply to each store:**
```typescript
// In each store's constructor or init():
private unsubResync?: () => void;

init() {
    // ... existing init logic
    this.unsubResync = wsStore.on('Resync', () => {
        console.warn('[StoreN] Resync received — refreshing data');
        this.refresh();
    });
}

destroy() {
    this.unsubResync?.();
    // ... existing cleanup
}
```

**Stores requiring this change:**
| Store | File | WS Events to Add |
|-------|------|-------------------|
| `sessions` | `stores/sessions.svelte.ts` | `Resync`, `SessionEvent` |
| `costs` | `stores/costs.svelte.ts` | `Resync` |
| `memory` | `stores/memory.svelte.ts` | `Resync` |
| `audit` | `stores/audit.svelte.ts` | `Resync` |
| `studioChat` | `stores/studioChat.svelte.ts` | `Resync`, `ChatMessage` |

**Acceptance criteria:**
- [ ] All 9 stores have `Resync` subscription
- [ ] All 9 stores have `destroy()` method that unsubscribes
- [ ] On WS reconnect with gap, all stores re-fetch from REST
- [ ] No stale data displayed after WS reconnection

---

### 2.9 SSE Incomplete Stream Detection

**Priority:** MEDIUM

**Current state:** When gateway crashes mid-stream, the SSE connection drops. Client hits `finally` block, clears `streaming = false`. User sees truncated response with no indication.

**Target:** Detect abnormal termination (no `stream_end` received), mark message as incomplete, offer retry.

**File:** `dashboard/src/lib/stores/studioChat.svelte.ts`
```typescript
// Add to StudioMessage interface:
interface StudioMessage {
    // ... existing fields
    status?: 'complete' | 'incomplete' | 'error';
}

// In sendMessage():
async sendMessage(content: string) {
    // ... existing setup
    let receivedStreamEnd = false;

    try {
        await api.streamPost(
            '/api/studio/sessions/' + this.activeSessionId + '/messages',
            { content, model: session.model },
            (eventType: string, data: string) => {
                switch (eventType) {
                    case 'stream_end':
                        receivedStreamEnd = true;
                        // Mark message as complete
                        this.updateLastAssistantMessage({ status: 'complete' });
                        break;
                    case 'text_delta':
                        this.streamingContent += JSON.parse(data).text;
                        break;
                    case 'error':
                        this.updateLastAssistantMessage({
                            status: 'error',
                            content: this.streamingContent + '\n\n[Error: ' + data + ']',
                        });
                        break;
                    // ... other cases
                }
            },
            this.abortController?.signal
        );
    } catch (e: unknown) {
        // Stream terminated without stream_end
        if (!receivedStreamEnd && this.streamingContent.length > 0) {
            this.updateLastAssistantMessage({
                status: 'incomplete',
                content: this.streamingContent,
            });
        }
    } finally {
        if (!receivedStreamEnd && this.streamingContent.length > 0) {
            this.updateLastAssistantMessage({ status: 'incomplete' });
        }
        this.streaming = false;
        this.streamingContent = '';
        this.abortController = null;
    }
}
```

**UI indicator** (in chat message component):
```svelte
{#if message.status === 'incomplete'}
    <div class="message-incomplete-badge">
        <span class="badge-icon">⚠</span>
        <span>Response interrupted — gateway lost</span>
        <button onclick={() => retryLastMessage()}>Retry</button>
    </div>
{/if}
```

**Acceptance criteria:**
- [ ] Truncated responses show "incomplete" badge
- [ ] "Retry" button resends the original user message
- [ ] Complete responses show no badge (default state)
- [ ] Manual cancel (Escape) does NOT show incomplete badge

---

### 2.10 ARIA & Accessibility Foundation

**Priority:** MEDIUM

**Changes across dashboard components:**

**Studio chat area:**
```svelte
<!-- Chat message list -->
<div
    role="log"
    aria-label="Chat messages"
    aria-live="polite"
    aria-relevant="additions"
>
    {#each messages as message}
        <div role="article" aria-label="{message.role} message">
            <!-- message content -->
        </div>
    {/each}
</div>

<!-- Input area -->
<textarea
    aria-label="Message input"
    aria-describedby="studio-input-hint"
/>
<span id="studio-input-hint" class="sr-only">
    Press Cmd+Enter to send, Escape to cancel streaming
</span>
```

**Activity bar / sidebar:**
```svelte
<nav aria-label="Primary navigation" role="navigation">
    {#each navItems as item}
        <a
            href={item.href}
            aria-current={isActive(item) ? 'page' : undefined}
            aria-label={item.label}
        >
            <!-- icon -->
        </a>
    {/each}
</nav>
```

**Status indicators:**
```svelte
<!-- Never rely on color alone -->
<span
    class="status-dot status-{status}"
    role="status"
    aria-label="Gateway status: {status}"
>
    {statusIcon(status)}
</span>
```

**Screen reader utility class:**
```css
/* dashboard/src/styles/global.css */
.sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
}
```

**Acceptance criteria:**
- [ ] All major layout regions have ARIA landmarks
- [ ] Chat area has `role="log"` and `aria-live="polite"`
- [ ] All interactive elements have visible focus indicators
- [ ] Color is never the sole indicator — always paired with text/icon
- [ ] Keyboard navigation works for all primary actions

---

### 2.11 E2E Test Infrastructure

**Priority:** HIGH
**Depends on:** 2.1 (DbPool)

**Approach:** Rust integration tests that boot the gateway in test mode, exercise the API with `reqwest`, and verify behavior end-to-end.

**File:** `crates/ghost-gateway/tests/common/mod.rs` (NEW)
```rust
use ghost_gateway::runtime::GatewayRuntime;
use ghost_gateway::config::GhostConfig;
use reqwest::Client;
use std::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Boots a gateway instance on a random port for testing.
pub struct TestGateway {
    pub port: u16,
    pub client: Client,
    pub base_url: String,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl TestGateway {
    pub async fn start() -> Self {
        // Find available port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // Configure with test database (in-memory or temp file)
        let config = GhostConfig::test_config(port);
        let shutdown = CancellationToken::new();
        let token = shutdown.clone();

        let handle = tokio::spawn(async move {
            let runtime = GatewayRuntime::from_config(config, token).await.unwrap();
            runtime.run().await;
        });

        // Wait for healthy
        let client = Client::new();
        let base_url = format!("http://127.0.0.1:{}", port);
        for _ in 0..50 {
            if client.get(&format!("{}/api/health", base_url))
                .send().await.is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Self { port, client, base_url, shutdown, handle }
    }

    pub async fn stop(self) {
        self.shutdown.cancel();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.handle
        ).await;
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}
```

**Test files to create:**

| File | Tests |
|------|-------|
| `tests/test_health.rs` | Health endpoint returns 200, gateway state is Healthy |
| `tests/test_auth.rs` | Login flow, token validation, 401 on expired token |
| `tests/test_agents.rs` | Agent CRUD: create, list, get, delete |
| `tests/test_kill_switch.rs` | Kill-all activates, agents blocked, resume works |
| `tests/test_studio_chat.rs` | Send message, receive SSE stream, stream_end |
| `tests/test_ws_reconnect.rs` | WS connect, disconnect, reconnect with last_seq, replay |

**Acceptance criteria:**
- [ ] `cargo test -p ghost-gateway` runs all E2E tests
- [ ] Tests boot real gateway (not mocked)
- [ ] Tests complete in < 30 seconds total
- [ ] CI runs these tests on every PR

---

### 2.12 RBAC Middleware

**Priority:** HIGH
**Depends on:** 2.5 (GhostError)

**Current state:** JWT validation exists but no role-based access control on route groups.

**Target:** Middleware that extracts JWT claims and enforces role requirements per route group.

**File:** `crates/ghost-gateway/src/auth/rbac.rs` (NEW)
```rust
use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use crate::api::error::ApiError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,     // Read-only access
    Operator,   // Can run agents, use studio
    Admin,      // Can configure safety, manage keys
    SuperAdmin, // Can manage RBAC, kill-all
}

impl Role {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "viewer" => Some(Role::Viewer),
            "operator" => Some(Role::Operator),
            "admin" => Some(Role::Admin),
            "superadmin" => Some(Role::SuperAdmin),
            _ => None,
        }
    }
}

/// Middleware factory: requires minimum role level.
pub fn require_role(minimum: Role) -> impl Fn(Request, Next) -> impl Future<Output = Result<Response, ApiError>> + Clone {
    move |req: Request, next: Next| {
        let minimum = minimum.clone();
        async move {
            let claims = req.extensions().get::<JwtClaims>()
                .ok_or(ApiError::Unauthorized { reason: "No auth token".into() })?;

            let user_role = Role::from_str(&claims.role)
                .ok_or(ApiError::Forbidden { reason: format!("Unknown role: {}", claims.role) })?;

            if user_role < minimum {
                return Err(ApiError::Forbidden {
                    reason: format!(
                        "Requires {:?} role, you have {:?}",
                        minimum, user_role
                    ),
                });
            }

            Ok(next.run(req).await)
        }
    }
}
```

**Route group assignments:**
```rust
// In router setup:
let viewer_routes = Router::new()
    .route("/api/health", get(health::check))
    .route("/api/agents", get(agents::list))
    .route("/api/sessions", get(sessions::list))
    .route("/api/costs", get(costs::summary))
    .route("/api/convergence/scores", get(convergence::scores));

let operator_routes = Router::new()
    .route("/api/agents", post(agents::create))
    .route("/api/studio/sessions", post(studio_sessions::create))
    .route("/api/agent/chat", post(agent_chat::chat))
    .layer(middleware::from_fn(require_role(Role::Operator)));

let admin_routes = Router::new()
    .route("/api/safety/*", any(safety::router()))
    .route("/api/provider-keys/*", any(provider_keys::router()))
    .route("/api/webhooks/*", any(webhooks::router()))
    .layer(middleware::from_fn(require_role(Role::Admin)));

let superadmin_routes = Router::new()
    .route("/api/safety/kill-all", post(safety::kill_all))
    .route("/api/admin/*", any(admin::router()))
    .layer(middleware::from_fn(require_role(Role::SuperAdmin)));
```

**Acceptance criteria:**
- [ ] Viewer can read but not create agents
- [ ] Operator can use studio but not manage safety
- [ ] Admin can configure safety but not kill-all
- [ ] SuperAdmin has full access
- [ ] Role checked on every request (not just login)
- [ ] Safety operations (pause, quarantine, kill) require Admin+

---

## 3. Phase 2: Core ADE Experience

### 3.1 Command Palette Enhancement

**Priority:** HIGH
**Depends on:** 2.7 (Type Safety)

**Current state:** `CommandPalette.svelte` exists with basic debounced search (200ms), arrow key navigation, and type-based routing. Missing: agent-specific commands, frecency ranking, prefixed scoped search, multi-step chaining.

**File:** `dashboard/src/components/CommandPalette.svelte` (modify)

**Target feature set:**

**1. Prefixed scoped search:**
```typescript
type SearchPrefix = '>' | '@' | '#' | '/';

interface PaletteCommand {
    id: string;
    label: string;
    category: 'command' | 'agent' | 'session' | 'setting';
    shortcut?: string;
    action: () => void | Promise<void>;
    frecencyScore: number;  // computed from usage history
}

function parseQuery(raw: string): { prefix: SearchPrefix | null; query: string } {
    const prefixes: SearchPrefix[] = ['>', '@', '#', '/'];
    for (const p of prefixes) {
        if (raw.startsWith(p)) {
            return { prefix: p, query: raw.slice(1).trim() };
        }
    }
    return { prefix: null, query: raw };
}

function filterCommands(
    commands: PaletteCommand[],
    prefix: SearchPrefix | null,
    query: string,
): PaletteCommand[] {
    const categoryMap: Record<SearchPrefix, string> = {
        '>': 'command',
        '@': 'agent',
        '#': 'session',
        '/': 'setting',
    };

    let filtered = commands;
    if (prefix) {
        filtered = filtered.filter(c => c.category === categoryMap[prefix]);
    }
    if (query) {
        filtered = fuzzyMatch(filtered, query);
    }

    // Sort by frecency (frequency × recency)
    return filtered.sort((a, b) => b.frecencyScore - a.frecencyScore);
}
```

**2. Agent-specific commands (registered dynamically from agents store):**
```typescript
function buildAgentCommands(agents: Agent[]): PaletteCommand[] {
    const commands: PaletteCommand[] = [];
    for (const agent of agents) {
        commands.push(
            {
                id: `start-${agent.id}`,
                label: `Start Agent: ${agent.name}`,
                category: 'command',
                action: () => api.post(`/api/agents/${agent.id}/start`),
                frecencyScore: 0,
            },
            {
                id: `pause-${agent.id}`,
                label: `Pause Agent: ${agent.name}`,
                category: 'command',
                action: () => api.post(`/api/safety/pause/${agent.id}`),
                frecencyScore: 0,
            },
            {
                id: `logs-${agent.id}`,
                label: `Open Agent Logs: ${agent.name}`,
                category: 'command',
                action: () => goto(`/agents/${agent.id}/logs`),
                frecencyScore: 0,
            },
        );
    }
    commands.push({
        id: 'kill-all',
        label: 'Kill All Agents',
        category: 'command',
        shortcut: 'Cmd+Shift+K',
        action: async () => {
            if (confirm('Kill all agents? This cannot be undone.')) {
                await api.post('/api/safety/kill-all', { reason: 'Manual kill via command palette' });
            }
        },
        frecencyScore: 0,
    });
    return commands;
}
```

**3. Frecency tracking (localStorage-based):**
```typescript
interface FrecencyEntry {
    commandId: string;
    lastUsed: number;   // timestamp
    useCount: number;
}

class FrecencyTracker {
    private entries: Map<string, FrecencyEntry>;
    private readonly STORAGE_KEY = 'ghost-command-frecency';
    private readonly DECAY_HALF_LIFE = 7 * 24 * 60 * 60 * 1000; // 7 days

    constructor() {
        const stored = localStorage.getItem(this.STORAGE_KEY);
        this.entries = new Map(stored ? JSON.parse(stored) : []);
    }

    record(commandId: string): void {
        const existing = this.entries.get(commandId) ?? {
            commandId, lastUsed: 0, useCount: 0,
        };
        existing.lastUsed = Date.now();
        existing.useCount += 1;
        this.entries.set(commandId, existing);
        this.persist();
    }

    score(commandId: string): number {
        const entry = this.entries.get(commandId);
        if (!entry) return 0;
        const age = Date.now() - entry.lastUsed;
        const recency = Math.exp(-age / this.DECAY_HALF_LIFE);
        return entry.useCount * recency;
    }

    private persist(): void {
        localStorage.setItem(
            this.STORAGE_KEY,
            JSON.stringify([...this.entries.entries()]),
        );
    }
}
```

**Acceptance criteria:**
- [ ] `>` prefix filters to commands, `@` to agents, `#` to sessions, `/` to settings
- [ ] Agent commands appear dynamically (start, pause, logs, kill-all)
- [ ] Frecency ranking: frequently/recently used commands appear first
- [ ] Empty palette shows recent commands
- [ ] Keyboard shortcuts displayed inline next to each command
- [ ] All actions are keyboard-accessible (arrow keys + Enter)

---

### 3.2 Keyboard Shortcuts System

**Priority:** HIGH

**Design:** Global keyboard shortcut manager that reads custom bindings from `~/.ghost/keybindings.json` with fallback defaults.

**File:** `dashboard/src/lib/shortcuts.ts` (NEW)
```typescript
interface ShortcutBinding {
    key: string;            // e.g., "cmd+shift+k"
    command: string;        // e.g., "killSwitch.activateAll"
    when?: string;          // context condition, e.g., "studioFocused"
}

const DEFAULT_BINDINGS: ShortcutBinding[] = [
    { key: 'cmd+k',         command: 'commandPalette.open' },
    { key: 'cmd+shift+f',   command: 'search.global' },
    { key: 'cmd+n',         command: 'studio.newSession' },
    { key: 'cmd+w',         command: 'tabs.closeCurrent' },
    { key: 'cmd+1',         command: 'tabs.goto1' },
    { key: 'cmd+2',         command: 'tabs.goto2' },
    { key: 'cmd+3',         command: 'tabs.goto3' },
    { key: 'cmd+4',         command: 'tabs.goto4' },
    { key: 'cmd+5',         command: 'tabs.goto5' },
    { key: 'cmd+6',         command: 'tabs.goto6' },
    { key: 'cmd+7',         command: 'tabs.goto7' },
    { key: 'cmd+8',         command: 'tabs.goto8' },
    { key: 'cmd+9',         command: 'tabs.goto9' },
    { key: 'cmd+`',         command: 'panel.toggleTerminal' },
    { key: 'cmd+b',         command: 'sidebar.toggle' },
    { key: 'cmd+e',         command: 'editor.focus' },
    { key: 'cmd+shift+k',   command: 'killSwitch.activateAll' },
    { key: 'cmd+enter',     command: 'studio.sendMessage', when: 'studioFocused' },
    { key: 'escape',        command: 'studio.cancelStream', when: 'studioStreaming' },
    { key: 'cmd+shift+t',   command: 'theme.toggle' },
];

type CommandHandler = () => void | Promise<void>;

class ShortcutManager {
    private bindings: ShortcutBinding[] = [];
    private handlers: Map<string, CommandHandler> = new Map();
    private contexts: Set<string> = new Set();

    constructor() {
        this.bindings = [...DEFAULT_BINDINGS];
        this.loadCustomBindings();
        document.addEventListener('keydown', this.handleKeyDown.bind(this));
    }

    registerCommand(command: string, handler: CommandHandler): void {
        this.handlers.set(command, handler);
    }

    setContext(context: string, active: boolean): void {
        if (active) this.contexts.add(context);
        else this.contexts.delete(context);
    }

    private async loadCustomBindings(): Promise<void> {
        // In Tauri: read from ~/.ghost/keybindings.json
        // In browser: skip (use defaults)
        if (!isTauri()) return;
        try {
            const custom = await invoke<ShortcutBinding[]>('read_keybindings');
            // Custom bindings override defaults (matched by command)
            for (const binding of custom) {
                const idx = this.bindings.findIndex(b => b.command === binding.command);
                if (idx >= 0) this.bindings[idx] = binding;
                else this.bindings.push(binding);
            }
        } catch {
            // Custom file doesn't exist — use defaults
        }
    }

    private handleKeyDown(e: KeyboardEvent): void {
        const key = this.normalizeKey(e);
        const binding = this.bindings.find(b => {
            if (b.key !== key) return false;
            if (b.when && !this.contexts.has(b.when)) return false;
            return true;
        });

        if (binding) {
            e.preventDefault();
            e.stopPropagation();
            const handler = this.handlers.get(binding.command);
            if (handler) handler();
        }
    }

    private normalizeKey(e: KeyboardEvent): string {
        const parts: string[] = [];
        if (e.metaKey || e.ctrlKey) parts.push('cmd');
        if (e.shiftKey) parts.push('shift');
        if (e.altKey) parts.push('alt');
        const keyName = e.key.toLowerCase();
        if (!['meta', 'control', 'shift', 'alt'].includes(keyName)) {
            parts.push(keyName === ' ' ? 'space' : keyName);
        }
        return parts.join('+');
    }

    destroy(): void {
        document.removeEventListener('keydown', this.handleKeyDown.bind(this));
    }
}

export const shortcuts = new ShortcutManager();
```

**Integration in `+layout.svelte`:**
```svelte
<script>
import { shortcuts } from '$lib/shortcuts';
import { onMount, onDestroy } from 'svelte';

onMount(() => {
    shortcuts.registerCommand('commandPalette.open', () => { showPalette = true; });
    shortcuts.registerCommand('sidebar.toggle', () => { sidebarOpen = !sidebarOpen; });
    shortcuts.registerCommand('theme.toggle', toggleTheme);
    shortcuts.registerCommand('killSwitch.activateAll', handleKillAll);
    // ... register all commands
});

onDestroy(() => shortcuts.destroy());
</script>
```

**Acceptance criteria:**
- [ ] All shortcuts from design doc table work
- [ ] Custom `~/.ghost/keybindings.json` overrides defaults
- [ ] Context-aware bindings (`when` condition) work
- [ ] `Cmd+K` opens command palette, `Cmd+Shift+K` triggers kill switch confirmation
- [ ] No conflicts between shortcuts

---

### 3.3 CodeMirror 6 Studio Input

**Priority:** MEDIUM
**Depends on:** 2.9 (SSE Incomplete Stream)

**Current state:** Studio uses a plain `<textarea>` for message input.

**Target:** CodeMirror 6 editor with markdown syntax highlighting, multi-line support, and `Cmd+Enter` to send.

**Dependencies to add:** `@codemirror/view`, `@codemirror/state`, `@codemirror/lang-markdown`, `@codemirror/theme-one-dark`

**File:** `dashboard/src/components/StudioInput.svelte` (NEW)
```svelte
<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { EditorView, keymap, placeholder } from '@codemirror/view';
    import { EditorState } from '@codemirror/state';
    import { markdown } from '@codemirror/lang-markdown';
    import { oneDark } from '@codemirror/theme-one-dark';
    import { shortcuts } from '$lib/shortcuts';

    let { onSend, disabled = false }: {
        onSend: (content: string) => void;
        disabled?: boolean;
    } = $props();

    let editorContainer: HTMLDivElement;
    let view: EditorView;

    const sendKeymap = keymap.of([{
        key: 'Mod-Enter',
        run: (v) => {
            const content = v.state.doc.toString().trim();
            if (content && !disabled) {
                onSend(content);
                v.dispatch({
                    changes: { from: 0, to: v.state.doc.length, insert: '' },
                });
            }
            return true;
        },
    }]);

    const theme = EditorView.theme({
        '&': {
            fontSize: 'var(--font-size-sm)',
            fontFamily: 'var(--font-sans)',
            maxHeight: '200px',
        },
        '.cm-content': {
            padding: 'var(--spacing-sm)',
            caretColor: 'var(--color-text-primary)',
        },
        '.cm-editor': {
            backgroundColor: 'var(--color-bg-elevated)',
            borderRadius: 'var(--radius-md)',
        },
        '&.cm-focused': {
            outline: '2px solid var(--color-interactive-primary)',
        },
    });

    onMount(() => {
        const state = EditorState.create({
            doc: '',
            extensions: [
                sendKeymap,
                markdown(),
                theme,
                placeholder('Type a message... (Cmd+Enter to send)'),
                EditorView.lineWrapping,
            ],
        });
        view = new EditorView({ state, parent: editorContainer });
        shortcuts.setContext('studioFocused', true);
    });

    onDestroy(() => {
        view?.destroy();
        shortcuts.setContext('studioFocused', false);
    });

    export function focus() {
        view?.focus();
    }
</script>

<div bind:this={editorContainer} class="studio-input-container" role="textbox" aria-label="Message input" />
```

**Acceptance criteria:**
- [ ] Markdown syntax highlighting in input
- [ ] `Cmd+Enter` sends message
- [ ] Plain `Enter` inserts newline (multi-line editing)
- [ ] Max height 200px with scroll
- [ ] Focus indicator follows design system
- [ ] Placeholder text shown when empty

---

### 3.4 Artifact Panel

**Priority:** MEDIUM
**Depends on:** 3.3 (CodeMirror)

**Purpose:** When the assistant returns structured output (code blocks, tables, diffs), display it in a dedicated side panel rather than inline in chat.

**File:** `dashboard/src/components/ArtifactPanel.svelte` (NEW)
```svelte
<script lang="ts">
    import { EditorView } from '@codemirror/view';
    import { EditorState } from '@codemirror/state';

    interface Artifact {
        id: string;
        type: 'code' | 'table' | 'diff' | 'json';
        language?: string;
        content: string;
        title?: string;
    }

    let { artifacts = [], activeArtifactId }: {
        artifacts: Artifact[];
        activeArtifactId?: string;
    } = $props();

    let activeArtifact = $derived(
        artifacts.find(a => a.id === activeArtifactId) ?? artifacts[0]
    );
</script>

{#if artifacts.length > 0}
<div class="artifact-panel" role="complementary" aria-label="Artifacts">
    <!-- Tab bar for multiple artifacts -->
    <div class="artifact-tabs" role="tablist">
        {#each artifacts as artifact}
            <button
                role="tab"
                aria-selected={artifact.id === activeArtifact?.id}
                onclick={() => activeArtifactId = artifact.id}
            >
                {artifact.title ?? artifact.type}
            </button>
        {/each}
    </div>

    <!-- Content area -->
    <div class="artifact-content" role="tabpanel">
        {#if activeArtifact?.type === 'code'}
            <div class="artifact-code">
                <div class="artifact-toolbar">
                    <span class="artifact-language">{activeArtifact.language}</span>
                    <button onclick={() => copyToClipboard(activeArtifact.content)}>
                        Copy
                    </button>
                </div>
                <pre><code>{activeArtifact.content}</code></pre>
            </div>
        {:else if activeArtifact?.type === 'table'}
            <div class="artifact-table">
                <!-- Render markdown table as HTML table -->
            </div>
        {:else if activeArtifact?.type === 'diff'}
            <div class="artifact-diff">
                <!-- Unified diff view with +/- coloring -->
            </div>
        {/if}
    </div>
</div>
{/if}
```

**Artifact detection** (in chat message processing):
```typescript
function extractArtifacts(content: string): Artifact[] {
    const artifacts: Artifact[] = [];
    // Match fenced code blocks: ```language\n...content...\n```
    const codeBlockRegex = /```(\w+)?\n([\s\S]*?)```/g;
    let match;
    while ((match = codeBlockRegex.exec(content)) !== null) {
        // Only extract as artifact if > 5 lines (small snippets stay inline)
        const lines = match[2].split('\n').length;
        if (lines > 5) {
            artifacts.push({
                id: crypto.randomUUID(),
                type: 'code',
                language: match[1] || 'text',
                content: match[2],
                title: match[1] ? `${match[1]} snippet` : 'Code',
            });
        }
    }
    return artifacts;
}
```

**Acceptance criteria:**
- [ ] Code blocks > 5 lines rendered in side panel
- [ ] Copy button works for each artifact
- [ ] Tab bar for multiple artifacts from single response
- [ ] Panel is collapsible
- [ ] Language label shown for code artifacts

---

### 3.5 Agent Creation Wizard

**Priority:** MEDIUM
**Depends on:** 2.7 (Type Safety), 2.12 (RBAC)

**7-step flow per design doc:**

| Step | Title | Fields |
|------|-------|--------|
| 1 | Identity | Name, description, avatar/icon |
| 2 | Model | Provider, model, temperature, max_tokens |
| 3 | System Prompt | System prompt editor (CodeMirror) |
| 4 | Tools | Tool selection (checkboxes), per-tool config |
| 5 | Safety | Spending cap, intervention level, convergence profile |
| 6 | Channels | Which channels to connect (CLI, Slack, etc.) |
| 7 | Review | Summary of all settings, Create button |

**File:** `dashboard/src/routes/agents/new/+page.svelte` (NEW)

**Wizard state machine:**
```typescript
interface WizardState {
    step: number;       // 1-7
    data: {
        // Step 1
        name: string;
        description: string;
        icon: string;
        // Step 2
        provider: string;
        model: string;
        temperature: number;
        max_tokens: number;
        // Step 3
        system_prompt: string;
        // Step 4
        tools: string[];
        tool_configs: Record<string, unknown>;
        // Step 5
        spending_cap: number;
        intervention_level: 'normal' | 'elevated' | 'high' | 'critical';
        convergence_profile: string;
        // Step 6
        channels: string[];
    };
    validation: Record<number, string[]>;  // step -> errors
}

function validateStep(step: number, data: WizardState['data']): string[] {
    const errors: string[] = [];
    switch (step) {
        case 1:
            if (!data.name.trim()) errors.push('Name is required');
            if (data.name.length > 64) errors.push('Name must be ≤ 64 characters');
            if (!/^[a-z0-9-]+$/.test(data.name)) errors.push('Name: lowercase alphanumeric and hyphens only');
            break;
        case 2:
            if (!data.provider) errors.push('Select a provider');
            if (!data.model) errors.push('Select a model');
            if (data.temperature < 0 || data.temperature > 2) errors.push('Temperature: 0-2');
            break;
        case 5:
            if (data.spending_cap <= 0) errors.push('Spending cap must be positive');
            if (data.spending_cap > 1000) errors.push('Spending cap > $1000 requires admin approval');
            break;
    }
    return errors;
}
```

**Acceptance criteria:**
- [ ] 7-step wizard with progress indicator
- [ ] Back/Next navigation, validation per step
- [ ] Step 7 shows complete summary before creation
- [ ] POST to `/api/agents` on submit
- [ ] Redirects to new agent detail page on success
- [ ] Error display inline per field

---

### 3.6 Approval Queue UI

**Priority:** MEDIUM
**Depends on:** 2.8 (Store Resync)

**Purpose:** Show pending agent proposals that require human approval, with approve/deny/modify actions.

**File:** `dashboard/src/routes/approvals/+page.svelte` (NEW)

**Data model:**
```typescript
interface Proposal {
    id: string;
    agent_id: string;
    agent_name: string;
    type: 'tool_call' | 'spend' | 'escalation' | 'goal_change';
    description: string;
    details: {
        tool?: string;
        args?: Record<string, unknown>;
        cost_estimate?: number;
        risk_level?: 'low' | 'medium' | 'high';
    };
    status: 'pending' | 'approved' | 'denied' | 'modified';
    created_at: string;
    decided_at?: string;
    decided_by?: string;
}
```

**Layout:**
```
+------------------------------------------+
| Pending Approvals (3)                    |
+------------------------------------------+
| [!] agent-alpha wants to run shell cmd   |
|     `rm -rf /tmp/workspace`             |
|     Risk: HIGH | Cost: $0.00            |
|     [Approve] [Deny] [Modify & Approve] |
|     2 minutes ago                        |
+------------------------------------------+
| [i] agent-beta spending request          |
|     API call estimated at $2.50          |
|     Risk: LOW | Budget remaining: $47.50|
|     [Approve] [Deny]                    |
|     5 minutes ago                        |
+------------------------------------------+
```

**Real-time updates via WS:**
```typescript
// Subscribe to ProposalDecision events
wsStore.on('ProposalDecision', (data) => {
    // Update proposal status in local list
    // Remove from pending, add to history
});
```

**Acceptance criteria:**
- [ ] Pending proposals shown with agent name, type, risk level
- [ ] Approve/Deny buttons with confirmation for high-risk
- [ ] "Modify & Approve" allows editing args before approval
- [ ] Real-time updates when another operator decides
- [ ] History tab showing past decisions
- [ ] Badge count in sidebar navigation

---

### 3.7 Cursor-Based Pagination

**Priority:** MEDIUM
**Depends on:** 2.1 (DbPool)

**Current state:** List endpoints return all results (unbounded).

**Target:** Cursor-based pagination on all list views, 50 items per page.

**Backend pattern** (apply to all list endpoints):

```rust
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    /// Cursor: last item's sort key from previous page
    pub cursor: Option<String>,
    /// Items per page (default 50, max 200)
    pub limit: Option<u32>,
    /// Sort direction
    pub order: Option<SortOrder>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub total_count: u64,
}

impl PaginationParams {
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(50).min(200)
    }
}

// Example: GET /api/sessions?cursor=2026-03-05T10:00:00Z&limit=50
async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<Session>>, ApiError> {
    let db = state.db.read()?;
    let limit = params.limit() as i64;

    let (sessions, total) = if let Some(cursor) = &params.cursor {
        let rows = db.prepare(
            "SELECT * FROM sessions WHERE last_event_at < ?1 \
             ORDER BY last_event_at DESC LIMIT ?2"
        )?.query_map(params![cursor, limit + 1], map_session)?
         .collect::<Result<Vec<_>, _>>()?;

        let total: u64 = db.query_row(
            "SELECT COUNT(*) FROM sessions", [], |r| r.get(0),
        )?;

        (rows, total)
    } else {
        let rows = db.prepare(
            "SELECT * FROM sessions ORDER BY last_event_at DESC LIMIT ?1"
        )?.query_map(params![limit + 1], map_session)?
         .collect::<Result<Vec<_>, _>>()?;

        let total: u64 = db.query_row(
            "SELECT COUNT(*) FROM sessions", [], |r| r.get(0),
        )?;

        (rows, total)
    };

    let has_more = sessions.len() > limit as usize;
    let data: Vec<Session> = sessions.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        data.last().map(|s| s.last_event_at.clone())
    } else {
        None
    };

    Ok(Json(PaginatedResponse { data, next_cursor, has_more, total_count: total }))
}
```

**Frontend pagination component:**
```svelte
<script lang="ts">
    let { loadMore, hasMore, loading }: {
        loadMore: () => Promise<void>;
        hasMore: boolean;
        loading: boolean;
    } = $props();
</script>

{#if hasMore}
    <div class="pagination-trigger">
        <button onclick={loadMore} disabled={loading}>
            {loading ? 'Loading...' : 'Load more'}
        </button>
    </div>
{/if}
```

**Endpoints to paginate:**
- `GET /api/sessions` — cursor on `last_event_at`
- `GET /api/agents` — cursor on `created_at`
- `GET /api/audit/logs` — cursor on `timestamp`
- `GET /api/costs` — cursor on `recorded_at`
- `GET /api/memory` — cursor on `created_at`
- `GET /api/convergence/history` — cursor on `computed_at`

**Acceptance criteria:**
- [ ] All list endpoints accept `?cursor=&limit=` params
- [ ] Default 50 items, max 200
- [ ] Response includes `next_cursor`, `has_more`, `total_count`
- [ ] Frontend "Load more" button at bottom of lists
- [ ] No unbounded queries remain

---

### 3.8 Virtual List for Chat Messages

**Priority:** MEDIUM
**Depends on:** 2.9 (SSE)

**Current state:** All messages rendered in DOM. Performance degrades past ~200 messages.

**Target:** Virtual list that only renders visible messages + buffer.

**File:** `dashboard/src/components/VirtualMessageList.svelte` (NEW)
```svelte
<script lang="ts">
    import type { StudioMessage } from '$lib/stores/studioChat.svelte';

    let {
        messages,
        containerHeight,
        estimatedItemHeight = 120,
        overscan = 5,
    }: {
        messages: StudioMessage[];
        containerHeight: number;
        estimatedItemHeight?: number;
        overscan?: number;
    } = $props();

    let scrollTop = $state(0);
    let container: HTMLDivElement;

    // Measured heights (updated after render)
    let heights: Map<string, number> = new Map();

    // Compute visible range
    let visibleRange = $derived.by(() => {
        let accumulatedHeight = 0;
        let startIdx = 0;
        let endIdx = messages.length;

        // Find start index
        for (let i = 0; i < messages.length; i++) {
            const h = heights.get(messages[i].id) ?? estimatedItemHeight;
            if (accumulatedHeight + h > scrollTop) {
                startIdx = Math.max(0, i - overscan);
                break;
            }
            accumulatedHeight += h;
        }

        // Find end index
        accumulatedHeight = 0;
        for (let i = startIdx; i < messages.length; i++) {
            const h = heights.get(messages[i].id) ?? estimatedItemHeight;
            accumulatedHeight += h;
            if (accumulatedHeight > containerHeight + (overscan * estimatedItemHeight)) {
                endIdx = i + 1;
                break;
            }
        }

        return { start: startIdx, end: endIdx };
    });

    // Total height for scroll container
    let totalHeight = $derived(
        messages.reduce((sum, m) =>
            sum + (heights.get(m.id) ?? estimatedItemHeight), 0
        )
    );

    // Offset for visible items
    let offsetTop = $derived(
        messages.slice(0, visibleRange.start).reduce((sum, m) =>
            sum + (heights.get(m.id) ?? estimatedItemHeight), 0
        )
    );

    let visibleMessages = $derived(
        messages.slice(visibleRange.start, visibleRange.end)
    );

    function handleScroll(e: Event) {
        scrollTop = (e.target as HTMLDivElement).scrollTop;
    }

    function measureItem(id: string, el: HTMLDivElement) {
        const observer = new ResizeObserver(([entry]) => {
            heights.set(id, entry.contentRect.height);
        });
        observer.observe(el);
        return { destroy: () => observer.disconnect() };
    }
</script>

<div
    class="virtual-list-container"
    bind:this={container}
    onscroll={handleScroll}
    style="height: {containerHeight}px; overflow-y: auto;"
    role="log"
    aria-live="polite"
>
    <div style="height: {totalHeight}px; position: relative;">
        <div style="transform: translateY({offsetTop}px);">
            {#each visibleMessages as message (message.id)}
                <div use:measureItem={message.id}>
                    <slot {message} />
                </div>
            {/each}
        </div>
    </div>
</div>
```

**Acceptance criteria:**
- [ ] Only visible messages + 5 overscan rendered in DOM
- [ ] Smooth scrolling with no jank at 500+ messages
- [ ] Dynamic height measurement (messages vary in size)
- [ ] Auto-scroll to bottom on new message (unless user scrolled up)
- [ ] `role="log"` and `aria-live="polite"` preserved

---

### 3.9 Breadcrumb Navigation

**Priority:** LOW

**File:** `dashboard/src/components/Breadcrumb.svelte` (NEW)
```svelte
<script lang="ts">
    import { page } from '$app/stores';

    interface Crumb {
        label: string;
        href: string;
    }

    // Derive breadcrumbs from current route
    let crumbs = $derived.by((): Crumb[] => {
        const path = $page.url.pathname;
        const segments = path.split('/').filter(Boolean);
        const result: Crumb[] = [{ label: 'Home', href: '/' }];

        let accumulated = '';
        for (const segment of segments) {
            accumulated += '/' + segment;
            result.push({
                label: formatSegment(segment),
                href: accumulated,
            });
        }
        return result;
    });

    function formatSegment(s: string): string {
        // UUID → truncate. Keywords → capitalize.
        if (/^[0-9a-f-]{36}$/.test(s)) return s.slice(0, 8) + '...';
        return s.charAt(0).toUpperCase() + s.slice(1);
    }
</script>

<nav aria-label="Breadcrumb" class="breadcrumb-bar">
    <ol>
        {#each crumbs as crumb, i}
            <li>
                {#if i < crumbs.length - 1}
                    <a href={crumb.href}>{crumb.label}</a>
                    <span class="separator" aria-hidden="true">/</span>
                {:else}
                    <span aria-current="page">{crumb.label}</span>
                {/if}
            </li>
        {/each}
    </ol>
</nav>
```

**Acceptance criteria:**
- [ ] Breadcrumbs shown below titlebar on every page
- [ ] Each segment clickable (except current page)
- [ ] UUIDs truncated for readability
- [ ] `aria-current="page"` on last segment

---

### 3.10 Notification Panel

**Priority:** MEDIUM
**Depends on:** 2.8 (Store Resync)

**File:** `dashboard/src/components/NotificationPanel.svelte` (NEW)

**Notification types mapped to WS events:**
```typescript
interface AppNotification {
    id: string;
    type: 'agent_state' | 'safety_alert' | 'approval_request' | 'cost_warning' | 'system';
    severity: 'info' | 'warning' | 'critical';
    title: string;
    message: string;
    timestamp: string;
    read: boolean;
    actionHref?: string;  // Click navigates here
    agentId?: string;
}

// Map WS events to notifications:
const eventToNotification: Record<string, (data: unknown) => AppNotification> = {
    AgentStateChange: (d) => ({
        type: 'agent_state',
        severity: 'info',
        title: `Agent ${d.agent_id} → ${d.new_state}`,
        // ...
    }),
    KillSwitchActivation: (d) => ({
        type: 'safety_alert',
        severity: 'critical',
        title: 'Kill Switch Activated',
        message: d.reason,
        // ...
    }),
    // ... other mappings
};
```

**Native OS notifications for critical events (Tauri):**
```typescript
import { sendNotification } from '@tauri-apps/plugin-notification';

function pushCriticalNotification(n: AppNotification) {
    if (n.severity === 'critical' && isTauri()) {
        sendNotification({
            title: n.title,
            body: n.message,
        });
    }
}
```

**Acceptance criteria:**
- [ ] Bell icon in top-right with unread count badge
- [ ] Dropdown panel shows grouped notifications
- [ ] Severity coloring: info (blue), warning (yellow), critical (red)
- [ ] Click notification navigates to relevant view
- [ ] "Mark all read" button
- [ ] Critical events push native OS notification (Tauri only)
- [ ] Persisted in localStorage (survives page reload)

---

### 3.11 Browser Extension Port Fix

**Priority:** LOW

**Current state:** Extension defaults to port `18789`. Gateway runs on `39780`.

**File:** `extension/src/storage/sync.ts` (or equivalent config)
```typescript
// BEFORE:
const DEFAULT_GATEWAY_URL = 'http://localhost:18789';

// AFTER:
const DEFAULT_GATEWAY_URL = 'http://localhost:39780';
```

**Also update:** `extension/src/background/service-worker.ts`, `extension/src/auth-sync.ts`, and any other file referencing port `18789`.

**Acceptance criteria:**
- [ ] `grep -r "18789" extension/` returns zero results
- [ ] Extension connects to gateway on port 39780 by default
- [ ] Port configurable via extension options page

---

### 3.12 S8 Behavioral Anomaly Signal

**Priority:** LOW
**Depends on:** 2.6 (WS Sequence Numbers)

The design doc mentions 8 convergence signals, but current implementation has 7 (S1-S7). S8 (`behavioral_anomaly`) needs to be added.

**File:** `crates/cortex-convergence/src/signals.rs` (modify)
```rust
/// S8: Behavioral anomaly detection.
/// Measures deviation from the agent's established behavioral baseline.
/// Inputs: tool usage patterns, response length distribution, topic drift.
pub struct BehavioralAnomalySignal {
    /// Rolling window of recent tool call frequencies
    tool_call_histogram: HashMap<String, u32>,
    /// Baseline tool call frequencies (established over first N sessions)
    baseline_histogram: HashMap<String, f64>,
    /// Response length statistics
    response_length_stats: RunningStats,
    /// Baseline response length
    baseline_response_length: RunningStats,
}

impl ConvergenceSignal for BehavioralAnomalySignal {
    fn name(&self) -> &str { "behavioral_anomaly" }
    fn weight(&self) -> f64 { 0.08 }  // Lower weight — anomaly is supplementary

    fn compute(&self) -> f64 {
        // Chi-squared divergence between current and baseline tool usage
        let tool_divergence = chi_squared_divergence(
            &self.tool_call_histogram,
            &self.baseline_histogram,
        );

        // Z-score of recent response lengths vs baseline
        let length_zscore = self.response_length_stats
            .zscore_against(&self.baseline_response_length)
            .abs();

        // Normalize both to [0, 1] and average
        let tool_score = 1.0 - (-tool_divergence / 10.0).exp();  // asymptotic to 1
        let length_score = (length_zscore / 3.0).min(1.0);       // cap at 3σ

        (tool_score * 0.6 + length_score * 0.4).clamp(0.0, 1.0)
    }
}
```

**Frontend:** Add S8 chart to convergence dashboard alongside S1-S7.

**Acceptance criteria:**
- [ ] S8 signal computed and included in convergence score
- [ ] S8 chart visible on convergence dashboard
- [ ] Anomaly detection uses statistical baseline (not hardcoded thresholds)
- [ ] New agents start with no baseline (S8 returns 0.0 until baseline established)

---

## 4. Phase 3: Subsystem Surfaces

### 4.1 Channels Management UI

**Priority:** MEDIUM
**Depends on:** 3.7 (Pagination)

**Purpose:** UI for configuring and monitoring channel adapters (CLI, Slack, Discord, Telegram, WhatsApp).

**Route:** `dashboard/src/routes/channels/+page.svelte` (NEW)

**Backend endpoint:**
```rust
// GET /api/channels — list all configured channels
#[derive(Serialize)]
pub struct ChannelInfo {
    pub id: String,
    pub channel_type: String,    // "cli" | "slack" | "discord" | "telegram"
    pub status: ChannelStatus,   // Connected | Disconnected | Error
    pub agent_id: String,        // Which agent this channel routes to
    pub config: serde_json::Value,
    pub last_message_at: Option<String>,
    pub message_count: u64,
}

#[derive(Serialize)]
pub enum ChannelStatus {
    Connected,
    Disconnected,
    Error { message: String },
    Configuring,
}
```

**UI layout:**
```
+-----------------------------------------------+
| Channels                                       |
+-----------------------------------------------+
| ┌─────────────────────────────────────────┐   |
| │ [✓] CLI          ghost       Connected  │   |
| │     Last message: 2 min ago             │   |
| │ [✓] Slack #ops   agent-beta  Connected  │   |
| │     Last message: 15 min ago            │   |
| │ [✗] Discord      —           Error      │   |
| │     Token expired. [Reconnect]          │   |
| └─────────────────────────────────────────┘   |
|                                                |
| [+ Add Channel]                                |
+-----------------------------------------------+
```

**Acceptance criteria:**
- [ ] List all channels with status indicators
- [ ] Status dot: green (connected), red (error), gray (disconnected)
- [ ] Click channel opens config detail view
- [ ] "Add Channel" wizard for new channel setup
- [ ] Message log per channel (paginated)
- [ ] Reconnect button for errored channels

---

### 4.2 PC Control Dashboard

**Priority:** LOW
**Depends on:** 3.7 (Pagination)

**Purpose:** Configure and monitor PC control safety features — application allowlist, safe zones, action budgets, blocked hotkeys.

**Route:** `dashboard/src/routes/pc-control/+page.svelte` (NEW)

**Sections:**
1. **Status Overview** — PC control enabled/disabled, current action budget remaining
2. **Application Allowlist** — Which apps the agent can interact with
3. **Safe Zone Editor** — Screen regions where the agent can operate (visual editor)
4. **Action Budget** — Max actions per minute/hour, current usage
5. **Blocked Hotkeys** — System shortcuts the agent is prevented from pressing
6. **Action Log** — Scrolling log of all PC control actions with timestamps

**Backend endpoint:**
```rust
// GET /api/pc-control/status
#[derive(Serialize)]
pub struct PcControlStatus {
    pub enabled: bool,
    pub action_budget: ActionBudget,
    pub allowed_apps: Vec<String>,
    pub safe_zones: Vec<SafeZone>,
    pub blocked_hotkeys: Vec<String>,
    pub circuit_breaker_state: String,  // "closed" | "open" | "half_open"
}

#[derive(Serialize)]
pub struct ActionBudget {
    pub max_per_minute: u32,
    pub max_per_hour: u32,
    pub used_this_minute: u32,
    pub used_this_hour: u32,
}

#[derive(Serialize)]
pub struct SafeZone {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub label: String,
}
```

**Acceptance criteria:**
- [ ] Toggle PC control on/off
- [ ] Visual safe zone editor (drag rectangles on screen preview)
- [ ] Action budget displayed as progress bar
- [ ] Blocked hotkey list editable
- [ ] Action log with filtering
- [ ] Circuit breaker state visible

---

### 4.3 ITP Event Viewer

**Priority:** LOW
**Depends on:** 3.7 (Pagination), 3.10 (Notifications)

**Purpose:** Live tail of ITP (Interaction Tracking Protocol) events from the browser extension and other sources.

**Route:** `dashboard/src/routes/itp/+page.svelte` (NEW)

**Layout:**
```
+-----------------------------------------------+
| ITP Event Stream                    [Pause]   |
+-----------------------------------------------+
| Privacy Level: [Minimal ▼]                     |
+-----------------------------------------------+
| 10:42:15  SessionStart   chatgpt  session-a1  |
| 10:42:18  Interaction    chatgpt  msg: ****   |
| 10:42:22  Interaction    chatgpt  msg: ****   |
| 10:43:01  SessionStart   claude   session-b2  |
| 10:43:05  Interaction    claude   msg: ****   |
| 10:43:30  SessionEnd     chatgpt  session-a1  |
+-----------------------------------------------+
| Buffer: 142 events | Source: extension         |
| Extension status: Connected ✓                  |
+-----------------------------------------------+
```

**Privacy levels:**
- `Minimal` — Content hashed, only metadata visible
- `Standard` — First/last 10 chars visible, rest masked
- `Full` — Full content visible (requires explicit opt-in)

**Acceptance criteria:**
- [ ] Live scrolling event log
- [ ] Pause/Resume button stops auto-scroll
- [ ] Privacy level selector controls content masking
- [ ] Extension connection status shown
- [ ] Event count and buffer status displayed

---

### 4.4 Workflow Canvas

**Priority:** MEDIUM
**Depends on:** 3.7 (Pagination)

**Purpose:** Visual drag-and-drop workflow editor using d3-force for node layout.

**Route:** `dashboard/src/routes/workflows/[id]/+page.svelte` (enhance existing)

**Dependencies:** `d3-force`, `d3-selection`, `d3-zoom`, `d3-drag` (already in project)

**Node types:**
```typescript
interface WorkflowNode {
    id: string;
    type: 'agent' | 'condition' | 'transform' | 'input' | 'output' | 'parallel' | 'wait';
    label: string;
    config: Record<string, unknown>;
    position: { x: number; y: number };
}

interface WorkflowEdge {
    id: string;
    source: string;     // node id
    target: string;     // node id
    condition?: string;  // for conditional edges
    label?: string;
}

interface Workflow {
    id: string;
    name: string;
    nodes: WorkflowNode[];
    edges: WorkflowEdge[];
    status: 'draft' | 'active' | 'paused' | 'completed' | 'failed';
}
```

**Canvas implementation (SVG + d3):**
```svelte
<script lang="ts">
    import { onMount } from 'svelte';
    import * as d3 from 'd3';

    let { workflow }: { workflow: Workflow } = $props();
    let svgElement: SVGSVGElement;

    onMount(() => {
        const svg = d3.select(svgElement);
        const g = svg.append('g');  // transform group for zoom/pan

        // Zoom behavior
        const zoom = d3.zoom<SVGSVGElement, unknown>()
            .scaleExtent([0.1, 4])
            .on('zoom', (event) => {
                g.attr('transform', event.transform);
            });
        svg.call(zoom);

        // Draw edges (lines between nodes)
        const edges = g.selectAll('.edge')
            .data(workflow.edges)
            .join('path')
            .attr('class', 'edge')
            .attr('marker-end', 'url(#arrowhead)');

        // Draw nodes
        const nodes = g.selectAll('.node')
            .data(workflow.nodes)
            .join('g')
            .attr('class', 'node')
            .call(d3.drag<SVGGElement, WorkflowNode>()
                .on('drag', (event, d) => {
                    d.position.x = event.x;
                    d.position.y = event.y;
                    updatePositions();
                })
            );

        // Node shapes based on type
        nodes.each(function(d) {
            const node = d3.select(this);
            if (d.type === 'condition') {
                // Diamond shape for conditions
                node.append('polygon')
                    .attr('points', '0,-30 40,0 0,30 -40,0');
            } else {
                // Rounded rectangle for others
                node.append('rect')
                    .attr('width', 160).attr('height', 60)
                    .attr('rx', 8).attr('ry', 8)
                    .attr('x', -80).attr('y', -30);
            }
            node.append('text').text(d.label)
                .attr('text-anchor', 'middle')
                .attr('dy', '0.35em');
        });

        function updatePositions() {
            nodes.attr('transform', d => `translate(${d.position.x},${d.position.y})`);
            edges.attr('d', d => {
                const source = workflow.nodes.find(n => n.id === d.source)!;
                const target = workflow.nodes.find(n => n.id === d.target)!;
                return `M${source.position.x},${source.position.y} L${target.position.x},${target.position.y}`;
            });
        }

        updatePositions();
    });
</script>

<svg bind:this={svgElement} class="workflow-canvas" width="100%" height="100%">
    <defs>
        <marker id="arrowhead" viewBox="0 0 10 10" refX="10" refY="5"
                markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-text-secondary)" />
        </marker>
    </defs>
</svg>
```

**Node palette (sidebar):**
```
+-------------------+
| Node Palette      |
+-------------------+
| [📥] Input        |
| [🤖] Agent        |
| [❓] Condition    |
| [🔄] Transform   |
| [⏸] Wait         |
| [‖] Parallel     |
| [📤] Output       |
+-------------------+
```

Drag from palette onto canvas to add nodes.

**Acceptance criteria:**
- [ ] Canvas with zoom (scroll) and pan (drag background)
- [ ] Nodes draggable to reposition
- [ ] Edges drawn between nodes (click source port → click target port)
- [ ] Node palette with drag-to-add
- [ ] Condition nodes use diamond shape
- [ ] Edge labels for conditional branches
- [ ] Save workflow state via `PUT /api/workflows/:id`
- [ ] Performance: > 100 nodes without visible jank

---

### 4.5 Workflow Execution Runtime

**Priority:** MEDIUM
**Depends on:** 4.4 (Workflow Canvas)

**Purpose:** Execute workflow DAGs with durable state persistence, restart recovery, and real-time visualization.

**Backend:**
```rust
// crates/ghost-gateway/src/workflows/executor.rs (NEW or enhance existing)

pub struct WorkflowExecutor {
    db: Arc<DbPool>,
    event_tx: broadcast::Sender<WsEnvelope>,
    agent_registry: Arc<RwLock<AgentRegistry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionState {
    pub workflow_id: String,
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub node_states: HashMap<String, NodeExecutionState>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Running,
    Paused,
    Completed,
    Failed { error: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeExecutionState {
    pub node_id: String,
    pub status: NodeStatus,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl WorkflowExecutor {
    /// Execute a workflow DAG. Persists state to DB after each node completion
    /// for crash recovery.
    pub async fn execute(&self, workflow: Workflow) -> Result<String, ApiError> {
        let execution_id = uuid::Uuid::new_v4().to_string();

        // Topological sort for execution order
        let order = topological_sort(&workflow.nodes, &workflow.edges)?;

        // Initialize state
        let mut state = ExecutionState {
            workflow_id: workflow.id.clone(),
            execution_id: execution_id.clone(),
            status: ExecutionStatus::Running,
            node_states: HashMap::new(),
            started_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
        };
        self.persist_state(&state).await?;

        // Execute nodes in topological order
        for node_id in &order {
            let node = workflow.nodes.iter().find(|n| &n.id == node_id).unwrap();

            // Check if all predecessors completed
            let predecessors = workflow.edges.iter()
                .filter(|e| e.target == *node_id)
                .map(|e| &e.source);

            let all_ready = predecessors.clone().all(|pred| {
                state.node_states.get(pred)
                    .map(|s| matches!(s.status, NodeStatus::Completed))
                    .unwrap_or(false)
            });

            if !all_ready {
                // Skip if conditions not met (conditional edges)
                state.node_states.insert(node_id.clone(), NodeExecutionState {
                    node_id: node_id.clone(),
                    status: NodeStatus::Skipped,
                    ..Default::default()
                });
                continue;
            }

            // Execute the node
            let node_state = self.execute_node(node, &state).await;
            state.node_states.insert(node_id.clone(), node_state);

            // Persist after each node (crash recovery point)
            self.persist_state(&state).await?;

            // Broadcast progress
            broadcast_event(&self.event_tx, WsEvent::SessionEvent {
                session_id: execution_id.clone(),
                event_type: "node_completed".into(),
                // ...
            });
        }

        state.status = ExecutionStatus::Completed;
        state.completed_at = Some(chrono::Utc::now().to_rfc3339());
        self.persist_state(&state).await?;

        Ok(execution_id)
    }

    /// Resume a failed/paused execution from last checkpoint.
    pub async fn resume(&self, execution_id: &str) -> Result<(), ApiError> {
        let state = self.load_state(execution_id).await?;
        // Find first non-completed node and resume from there
        // ...
    }
}
```

**Acceptance criteria:**
- [ ] Workflow execution persists state after each node
- [ ] Crash recovery: resume from last completed node
- [ ] Real-time node status updates via WebSocket
- [ ] Parallel nodes execute concurrently
- [ ] Condition nodes evaluate and branch correctly
- [ ] Failed node can be retried without restarting entire workflow

---

### 4.6 Knowledge Graph View

**Priority:** LOW
**Depends on:** 3.7 (Pagination)

**Purpose:** Visualize memory entries as a knowledge graph using d3-force layout. Entities are nodes, relationships are edges.

**Route:** `dashboard/src/routes/memory/graph/+page.svelte` (NEW)

**Data model:**
```typescript
interface MemoryGraphNode {
    id: string;
    label: string;
    type: 'entity' | 'event' | 'concept';
    importance: number;   // 0-1, maps to node size
    decayFactor: number;  // current decay, maps to opacity
}

interface MemoryGraphEdge {
    source: string;
    target: string;
    relationship: string;
    strength: number;     // 0-1, maps to edge thickness
}
```

**Visualization:**
- d3-force simulation with charge repulsion and link attraction
- Node size proportional to importance
- Node opacity proportional to (1 - decay)
- Edge thickness proportional to relationship strength
- Click node to show memory detail panel
- Search/filter by entity type or keyword
- Zoom + pan

**Acceptance criteria:**
- [ ] Force-directed graph renders memory entities
- [ ] Node size/opacity encode importance/decay
- [ ] Click node shows detail sidebar
- [ ] Search filters visible nodes
- [ ] Performance: handles 500+ nodes (WebGL fallback for > 200)

---

### 4.7 Enhanced Convergence Dashboard

**Priority:** MEDIUM
**Depends on:** 3.12 (S8 Signal)

**Purpose:** Full convergence monitoring with all 8 signal charts, historical trends, and threshold configuration.

**Route:** `dashboard/src/routes/convergence/+page.svelte` (enhance existing)

**Layout:**
```
+--------------------------------------------------+
| Convergence Overview               [agent ▼]     |
+--------------------------------------------------+
| ┌────────────────┐  ┌────────────────────────┐   |
| │  Score Gauge   │  │  Signal Radar Chart    │   |
| │     0.72       │  │  (8-axis spider chart) │   |
| │   [Normal]     │  │                        │   |
| └────────────────┘  └────────────────────────┘   |
+--------------------------------------------------+
| Signal Trends (24h)                               |
| ┌────────────────────────────────────────────┐   |
| │ S1 soul_drift      ▁▂▃▂▁▂▃▄▅▆▅▄▃▂ 0.34  │   |
| │ S2 goal_alignment   ▇▇▆▆▅▅▄▄▃▃▂▂▁▁ 0.12  │   |
| │ S3 tool_usage       ▁▁▂▂▃▃▃▂▂▁▁▁▁▁ 0.08  │   |
| │ S4 output_quality   ▁▁▁▂▂▃▃▂▂▁▁▁▁▁ 0.15  │   |
| │ S5 resource_usage   ▁▁▁▁▂▂▃▃▄▄▅▅▆▆ 0.55  │   |
| │ S6 memory_health    ▁▁▁▁▁▁▁▁▁▂▂▂▂▂ 0.05  │   |
| │ S7 interaction_quality ▁▂▂▃▃▂▂▁▁▁▁ 0.11  │   |
| │ S8 behavioral_anomaly  ▁▁▁▁▁▁▂▂▁▁▁ 0.03  │   |
| └────────────────────────────────────────────┘   |
+--------------------------------------------------+
| Threshold Configuration                          |
| Normal: < 0.4  Elevated: < 0.6  High: < 0.8     |
| [Adjust Thresholds]                              |
+--------------------------------------------------+
```

**Radar chart (8-axis spider chart):**
```typescript
// Using d3 + SVG
function drawRadarChart(
    container: SVGGElement,
    signals: { name: string; value: number }[],
    size: number,
) {
    const angleSlice = (Math.PI * 2) / signals.length;
    const radius = size / 2;

    // Draw concentric circles (0.25, 0.5, 0.75, 1.0)
    [0.25, 0.5, 0.75, 1.0].forEach(level => {
        d3.select(container).append('circle')
            .attr('r', radius * level)
            .attr('fill', 'none')
            .attr('stroke', 'var(--color-border-default)')
            .attr('stroke-dasharray', level < 1 ? '2,2' : 'none');
    });

    // Draw axes
    signals.forEach((s, i) => {
        const angle = angleSlice * i - Math.PI / 2;
        const x = radius * Math.cos(angle);
        const y = radius * Math.sin(angle);
        d3.select(container).append('line')
            .attr('x1', 0).attr('y1', 0)
            .attr('x2', x).attr('y2', y)
            .attr('stroke', 'var(--color-border-subtle)');
        // Label
        d3.select(container).append('text')
            .attr('x', x * 1.15).attr('y', y * 1.15)
            .text(s.name)
            .attr('text-anchor', 'middle')
            .attr('font-size', 'var(--font-size-xs)');
    });

    // Draw data polygon
    const points = signals.map((s, i) => {
        const angle = angleSlice * i - Math.PI / 2;
        return [
            radius * s.value * Math.cos(angle),
            radius * s.value * Math.sin(angle),
        ];
    });
    d3.select(container).append('polygon')
        .attr('points', points.map(p => p.join(',')).join(' '))
        .attr('fill', 'var(--color-interactive-primary)')
        .attr('fill-opacity', 0.2)
        .attr('stroke', 'var(--color-interactive-primary)')
        .attr('stroke-width', 2);
}
```

**Acceptance criteria:**
- [ ] Score gauge shows composite score with level label
- [ ] 8-axis radar chart for all signals
- [ ] Sparkline trend charts per signal (24h rolling window)
- [ ] Agent selector dropdown
- [ ] Threshold configuration UI
- [ ] Real-time updates via WS `ScoreUpdate` events

---

### 4.8 Session Replay with Bookmarks

**Priority:** LOW
**Depends on:** 3.8 (Virtual List)

**Purpose:** Replay agent sessions step-by-step with ability to bookmark interesting points and branch from any point.

**Route:** `dashboard/src/routes/sessions/[id]/replay/+page.svelte` (NEW)

**Features:**
- **Timeline slider** — scrub through session events
- **Play/Pause** — auto-advance through events at configurable speed
- **Bookmarks** — mark interesting events, jump between them
- **Branch** — fork a new session from any event in the replay

**Data model:**
```typescript
interface ReplayState {
    sessionId: string;
    events: SessionEvent[];
    currentIndex: number;
    playing: boolean;
    playbackSpeed: number;  // 1x, 2x, 4x
    bookmarks: Bookmark[];
}

interface Bookmark {
    id: string;
    eventIndex: number;
    label: string;
    createdAt: string;
}
```

**Acceptance criteria:**
- [ ] Timeline slider scrubs through events
- [ ] Play button auto-advances at selected speed
- [ ] Bookmarks persisted per session
- [ ] Branch creates new session with events up to branch point
- [ ] Event details (tool calls, messages) shown at each step

---

## 5. Phase 4: Polish and Production

### 5.1 Auto-Update Mechanism

**Priority:** HIGH

**Implementation:** Use `tauri-plugin-updater` with signed updates.

**File:** `src-tauri/src/lib.rs` (modify)
```rust
use tauri_plugin_updater::UpdaterExt;

// In builder setup:
.plugin(tauri_plugin_updater::Builder::new().build())

// In setup handler:
app.handle().updater_builder()
    .endpoints(vec!["https://releases.ghost.dev/{{target}}/{{arch}}/{{current_version}}".into()])
    .build()?;
```

**Update flow:**
1. Check for updates on app launch (after gateway healthy)
2. If update available, show non-intrusive banner: "Update v1.2.3 available. [Install & Restart]"
3. User clicks Install → download update → stop gateway → install → restart
4. Before applying: create database backup (automatic)
5. After restart: run any new migrations

**File:** `src-tauri/src/services/update_checker.rs` (NEW)
```rust
pub async fn check_for_updates(app: &AppHandle) -> Result<Option<UpdateInfo>, GhostDesktopError> {
    let updater = app.updater_builder()
        .endpoints(vec![UPDATE_ENDPOINT.into()])
        .build()
        .map_err(|e| GhostDesktopError::ConfigError { reason: e.to_string() })?;

    match updater.check().await {
        Ok(Some(update)) => {
            // Emit to frontend
            app.emit("update://available", UpdatePayload {
                version: update.version.clone(),
                notes: update.body.clone(),
            }).ok();
            Ok(Some(update))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            tracing::warn!("Update check failed: {}", e);
            Ok(None)  // Non-fatal
        }
    }
}
```

**Acceptance criteria:**
- [ ] Update check on launch (non-blocking)
- [ ] Banner notification when update available
- [ ] Automatic database backup before update
- [ ] Graceful gateway shutdown before update install
- [ ] Migrations run on first launch after update

---

### 5.2 Autonomy Dial UI

**Priority:** MEDIUM
**Depends on:** Phase 3 convergence dashboard

**Purpose:** Visual control for adjusting agent autonomy level.

**File:** `dashboard/src/components/AutonomyDial.svelte` (NEW)
```svelte
<script lang="ts">
    let { agentId, currentLevel, convergenceScore, onLevelChange }: {
        agentId: string;
        currentLevel: 'critical' | 'high' | 'elevated' | 'normal';
        convergenceScore: number;
        onLevelChange: (level: string) => Promise<void>;
    } = $props();

    const levels = [
        { id: 'critical',  label: 'Fully Supervised', position: 0 },
        { id: 'high',      label: 'High Oversight',   position: 33 },
        { id: 'elevated',  label: 'Elevated',         position: 66 },
        { id: 'normal',    label: 'Fully Autonomous', position: 100 },
    ];

    let currentPosition = $derived(
        levels.find(l => l.id === currentLevel)?.position ?? 0
    );

    let autoSuggestedLevel = $derived.by(() => {
        if (convergenceScore > 0.8) return 'critical';
        if (convergenceScore > 0.6) return 'high';
        if (convergenceScore > 0.4) return 'elevated';
        return 'normal';
    });

    let isOverridden = $derived(currentLevel !== autoSuggestedLevel);
</script>

<div class="autonomy-dial" role="slider"
     aria-label="Agent autonomy level"
     aria-valuemin="0" aria-valuemax="100"
     aria-valuenow={currentPosition}
     aria-valuetext={currentLevel}>

    <div class="dial-track">
        {#each levels as level}
            <button
                class="dial-stop"
                class:active={level.id === currentLevel}
                class:suggested={level.id === autoSuggestedLevel}
                style="left: {level.position}%"
                onclick={() => onLevelChange(level.id)}
                aria-label="Set to {level.label}"
            >
                <span class="dial-dot" />
                <span class="dial-label">{level.label}</span>
            </button>
        {/each}
        <div class="dial-fill" style="width: {currentPosition}%" />
    </div>

    {#if isOverridden}
        <div class="dial-override-notice">
            Operator override active.
            Auto-suggested: {autoSuggestedLevel}
            <button onclick={() => onLevelChange(autoSuggestedLevel)}>
                Reset to auto
            </button>
        </div>
    {/if}
</div>
```

**Acceptance criteria:**
- [ ] Clickable dial with 4 stops
- [ ] Current position highlighted
- [ ] Auto-suggested level based on convergence score shown
- [ ] Override notice when operator overrides auto-suggestion
- [ ] "Reset to auto" button
- [ ] Changes persisted via `PUT /api/agents/:id/intervention-level`
- [ ] ARIA slider semantics

---

### 5.3 Data Retention Automation

**Priority:** MEDIUM
**Depends on:** 2.1 (DbPool)

**Purpose:** Automatic cleanup of old data per retention policy from design doc.

**File:** `crates/ghost-gateway/src/services/retention.rs` (NEW)
```rust
use crate::db_pool::DbPool;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Data retention policies (from design doc Section 7.4)
pub struct RetentionConfig {
    pub session_messages_days: u32,     // Default: 90
    pub trace_spans_days: u32,          // Default: 30
    pub decayed_memory_days: u32,       // Default: 30 (after soft-delete)
    pub cost_records_days: u32,         // Default: 365
    pub db_size_alert_bytes: u64,       // Default: 1.5 GB
    pub db_size_max_bytes: u64,         // Default: 2 GB
    pub vacuum_interval_hours: u32,     // Default: 168 (weekly)
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            session_messages_days: 90,
            trace_spans_days: 30,
            decayed_memory_days: 30,
            cost_records_days: 365,
            db_size_alert_bytes: 1_500_000_000,
            db_size_max_bytes: 2_000_000_000,
            vacuum_interval_hours: 168,
        }
    }
}

pub async fn start_retention_service(
    db: Arc<DbPool>,
    config: RetentionConfig,
    cancel: CancellationToken,
    event_tx: broadcast::Sender<WsEnvelope>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600)); // hourly

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = interval.tick() => {
                if let Err(e) = run_retention(&db, &config, &event_tx).await {
                    tracing::error!("Retention service error: {}", e);
                }
            }
        }
    }
}

async fn run_retention(
    db: &DbPool,
    config: &RetentionConfig,
    event_tx: &broadcast::Sender<WsEnvelope>,
) -> Result<(), DbPoolError> {
    let conn = db.write().await;

    // 1. Archive old session messages
    let cutoff_sessions = chrono::Utc::now()
        - chrono::Duration::days(config.session_messages_days as i64);
    conn.execute(
        "UPDATE session_events SET archived = 1 WHERE created_at < ?1 AND archived = 0",
        [cutoff_sessions.to_rfc3339()],
    )?;

    // 2. Delete old trace spans
    let cutoff_traces = chrono::Utc::now()
        - chrono::Duration::days(config.trace_spans_days as i64);
    conn.execute(
        "DELETE FROM otel_spans WHERE start_time < ?1",
        [cutoff_traces.to_rfc3339()],
    )?;

    // 3. Hard-delete soft-deleted memories past retention
    let cutoff_memory = chrono::Utc::now()
        - chrono::Duration::days(config.decayed_memory_days as i64);
    conn.execute(
        "DELETE FROM memory_snapshots WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
        [cutoff_memory.to_rfc3339()],
    )?;

    // 4. Check DB size
    let db_size: u64 = conn.query_row(
        "SELECT page_count * page_size FROM pragma_page_count, pragma_page_size",
        [],
        |r| r.get(0),
    )?;

    if db_size > config.db_size_alert_bytes {
        tracing::warn!("Database size {}MB exceeds alert threshold", db_size / 1_000_000);
        // Emit system event
    }

    if db_size > config.db_size_max_bytes {
        tracing::error!("Database size {}MB exceeds maximum. Refusing new writes.", db_size / 1_000_000);
    }

    // 5. Weekly VACUUM (check if it's time)
    // ... schedule check based on last vacuum timestamp

    Ok(())
}
```

**Acceptance criteria:**
- [ ] Retention service runs hourly as background task
- [ ] Session messages archived after 90 days
- [ ] Trace spans deleted after 30 days
- [ ] Soft-deleted memories hard-deleted after 30 days
- [ ] DB size alert at 1.5GB, refuse writes at 2GB
- [ ] Weekly VACUUM during idle
- [ ] Service respects CancellationToken (clean shutdown)

---

### 5.4 MCP Compatibility Layer

**Priority:** MEDIUM
**Depends on:** Phase 3 workflow runtime

**Purpose:** Support Model Context Protocol servers as a skill interface alongside the existing WASM sandbox.

**File:** `crates/ghost-skills/src/mcp_bridge.rs` (NEW)
```rust
use std::process::{Command, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// MCP Bridge: wraps an MCP-compatible tool server as a GHOST skill.
///
/// Supports two transports:
///   - stdio: Launch MCP server as child process, communicate via JSON-RPC over stdin/stdout
///   - HTTP: Connect to running MCP server via HTTP (SSE for streaming)
pub struct McpBridge {
    pub name: String,
    pub transport: McpTransport,
    pub tools: Vec<McpToolDefinition>,
}

pub enum McpTransport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    Http {
        url: String,
        headers: HashMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl McpBridge {
    /// Discover available tools from MCP server.
    pub async fn discover(&mut self) -> Result<Vec<McpToolDefinition>, McpError> {
        let response = self.send_jsonrpc("tools/list", serde_json::Value::Null).await?;
        let tools: Vec<McpToolDefinition> = serde_json::from_value(response)?;
        self.tools = tools.clone();
        Ok(tools)
    }

    /// Execute a tool call via the MCP server.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        });
        self.send_jsonrpc("tools/call", params).await
    }

    async fn send_jsonrpc(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        match &self.transport {
            McpTransport::Stdio { command, args, env } => {
                // Launch process, send JSON-RPC request, read response
                let request = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": method,
                    "params": params,
                });
                // ... process communication
                todo!()
            }
            McpTransport::Http { url, headers } => {
                // HTTP POST to MCP server
                let client = reqwest::Client::new();
                let request = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": method,
                    "params": params,
                });
                let response = client.post(url)
                    .json(&request)
                    .send().await
                    .map_err(|e| McpError::Transport(e.to_string()))?;
                let body: serde_json::Value = response.json().await
                    .map_err(|e| McpError::Parse(e.to_string()))?;
                Ok(body["result"].clone())
            }
        }
    }
}

/// Register MCP tools in the skill registry alongside WASM skills.
impl Skill for McpBridge {
    fn name(&self) -> &str { &self.name }
    fn skill_type(&self) -> SkillType { SkillType::Mcp }

    async fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, SkillError> {
        self.call_tool(tool_name, input).await
            .map_err(|e| SkillError::ExecutionFailed(e.to_string()))
    }
}
```

**Acceptance criteria:**
- [ ] MCP servers discoverable via `tools/list`
- [ ] Tool calls routed through MCP bridge
- [ ] stdio and HTTP transports supported
- [ ] MCP tools appear alongside WASM skills in skill registry
- [ ] Error handling: MCP server crash → skill returns error (not panic)

---

### 5.5 Deep-Link Handler

**Priority:** LOW

**Purpose:** Handle `ghost://` URLs to navigate directly to specific views.

**File:** `src-tauri/src/services/deep_link.rs` (NEW)
```rust
use tauri::Url;

/// Parse ghost:// deep links and return the corresponding dashboard route.
///
/// Supported URLs:
///   ghost://agents/{id}          → /agents/{id}
///   ghost://sessions/{id}        → /sessions/{id}
///   ghost://studio               → /studio
///   ghost://convergence          → /convergence
///   ghost://safety               → /safety
pub fn parse_deep_link(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    if url.scheme() != "ghost" {
        return None;
    }

    let host = url.host_str()?;
    let path = url.path();

    // ghost://agents/abc-123 → host="agents", path="/abc-123"
    let route = match host {
        "agents" => format!("/agents{}", path),
        "sessions" => format!("/sessions{}", path),
        "studio" => "/studio".to_string(),
        "convergence" => "/convergence".to_string(),
        "safety" => "/safety".to_string(),
        "settings" => "/settings".to_string(),
        _ => return None,
    };

    Some(route)
}
```

**Tauri integration:**
```rust
// In lib.rs setup:
app.handle().plugin(tauri_plugin_deep_link::init())?;

app.listen("deep-link://new-url", |event| {
    if let Some(urls) = event.payload().as_str() {
        if let Some(route) = parse_deep_link(urls) {
            // Emit to frontend for navigation
            app.emit("navigate", route).ok();
        }
    }
});
```

**Frontend listener:**
```typescript
// In +layout.svelte
import { listen } from '@tauri-apps/api/event';
import { goto } from '$app/navigation';

onMount(async () => {
    if (isTauri()) {
        await listen('navigate', (event) => {
            goto(event.payload as string);
        });
    }
});
```

**Acceptance criteria:**
- [ ] `ghost://agents/abc-123` opens app and navigates to agent detail
- [ ] `ghost://studio` opens Studio view
- [ ] Unknown URLs ignored (no crash)
- [ ] Works when app is already running (focus + navigate)
- [ ] Works when app is not running (launch + navigate)

---

### 5.6 First-Run Onboarding

**Priority:** LOW
**Depends on:** 3.5 (Agent Creation Wizard)

**Purpose:** Guide new users through initial setup: provider configuration, first agent creation.

**Flow:**
1. **Welcome** — "Welcome to GHOST ADE" with brief description
2. **Provider Setup** — Configure at least one LLM provider (API key input)
3. **First Agent** — Create first agent using wizard (simplified 3-step variant)
4. **Try It** — Send first message in Studio
5. **Complete** — "You're ready!" with links to docs

**File:** `dashboard/src/routes/onboarding/+page.svelte` (NEW)

**Detection:**
```typescript
// In +layout.svelte:
onMount(async () => {
    const hasCompletedOnboarding = localStorage.getItem('ghost-onboarding-complete');
    if (!hasCompletedOnboarding) {
        // Check if any providers configured
        const health = await api.get<{ providers: string[] }>('/api/health');
        if (health.providers.length === 0) {
            goto('/onboarding');
        }
    }
});
```

**Acceptance criteria:**
- [ ] New users see onboarding on first launch
- [ ] Can skip onboarding
- [ ] Provider API key validated before proceeding
- [ ] First agent created successfully
- [ ] Onboarding completion persisted (not shown again)

---

## 6. Testing Strategy

### 6.1 Test Pyramid

```
            ┌──────────┐
           /  E2E (10%) \        Playwright (dashboard) + reqwest (gateway)
          /──────────────\
         / Integration    \      Rust integration tests per crate
        /   (30%)          \
       /────────────────────\
      /     Unit (60%)       \   Rust unit tests + Vitest (SDK)
     /────────────────────────\
```

### 6.2 Rust Test Strategy

**Unit tests (in-module `#[cfg(test)]`):**
- Every public function in cortex crates
- Gate check logic (each gate independently)
- Kill switch state transitions
- DbPool (read/write separation, overflow behavior)
- Error conversion (CortexError → ApiError)
- Deep link parsing
- Retention policy calculations

**Integration tests (`tests/` directory per crate):**
- `ghost-gateway/tests/` — API endpoint tests (boot real gateway)
- `ghost-agent-loop/tests/` — Full agent run with mocked LLM
- `cortex-storage/tests/` — Migration chain (apply all 37 migrations)
- `ghost-kill-gates/tests/` — Distributed kill gate quorum

**Property-based tests (using `proptest`):**
- Convergence score always in [0.0, 1.0]
- CRDT merge is commutative and associative
- DbPool never panics under concurrent access
- Kill switch level is monotonically non-decreasing

### 6.3 Frontend Test Strategy

**Vitest (unit):**
- API client: typed responses, error handling, 401 redirect
- Store logic: state transitions, WS event handling
- FrecencyTracker: scoring, persistence
- Shortcut normalization

**Playwright (E2E):**
- Login flow end-to-end
- Studio: send message, receive streaming response
- Agent CRUD: create, view, delete
- Kill switch: activate, verify UI state
- Command palette: open, search, execute command
- Mobile responsive: sidebar collapse, bottom nav

### 6.4 Coverage Targets

| Area | Target | Tool |
|------|--------|------|
| Rust gateway | 70% line coverage | `cargo-tarpaulin` |
| Rust cortex crates | 80% line coverage | `cargo-tarpaulin` |
| Frontend stores | 80% branch coverage | Vitest |
| E2E critical paths | 100% of defined paths | Playwright |

---

## 7. Performance Budget Enforcement

### 7.1 Measurement Infrastructure

**Cold start timing** (Tauri):
```rust
// In lib.rs setup:
let start = std::time::Instant::now();
// ... setup logic
tracing::info!(
    elapsed_ms = start.elapsed().as_millis(),
    "Cold start complete"
);
// Target: < 3000ms
```

**Bundle size check** (CI):
```bash
# In CI pipeline:
pnpm --filter dashboard build
BUNDLE_SIZE=$(du -sb dashboard/build | cut -f1)
GZIP_SIZE=$(tar czf - dashboard/build | wc -c)
if [ "$GZIP_SIZE" -gt 512000 ]; then
    echo "ERROR: Bundle size ${GZIP_SIZE} exceeds 500KB gzip budget"
    exit 1
fi
```

**WebSocket latency measurement:**
```typescript
// In websocket store, measure event-to-render latency:
private handleMessage(data: string) {
    const received = performance.now();
    const envelope = JSON.parse(data);
    // ... process
    const rendered = performance.now();
    if (rendered - received > 50) {
        console.warn(`WS event-to-render: ${(rendered - received).toFixed(1)}ms (budget: 50ms)`);
    }
}
```

### 7.2 Performance Budgets (from Design Doc Section 11.1)

| Metric | Budget | Enforcement |
|--------|--------|-------------|
| Cold start | < 3s | Tauri timing log, CI alert |
| Page navigation | < 100ms | SvelteKit client routing (inherent) |
| First token latency | < 500ms | Studio timing display |
| WS event → DOM | < 50ms | Console warning in dev |
| Global search | < 200ms | FTS5 query timing |
| Initial JS bundle | < 500KB gzip | CI size check |
| Per-route chunk | < 100KB gzip | Vite build analysis |
| Idle memory | < 150MB RSS | Periodic monitoring |
| 5-agent memory | < 500MB RSS | Load test |
| Chat virtualize at | > 200 messages | VirtualMessageList threshold |
| Workflow canvas | > 100 nodes | d3-force perf test |

---

## 8. Risk Registry

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| DbPool migration breaks existing queries | Medium | High | Feature flag: env `GHOST_DB_POOL=1` to opt-in during testing. Rollback: single connection still works. |
| WS sequence numbers break existing clients | Low | Medium | Envelope wraps existing events (additive, not breaking). Clients without `last_seq` get full connection (no replay). |
| CodeMirror 6 bundle size | Medium | Low | Lazy-load CodeMirror only on Studio page. Measure chunk size. |
| Kill switch write ordering changes behavior | Low | Critical | Extensive testing: crash simulation, power-off simulation. The new ordering is strictly safer. |
| RBAC breaks single-user setups | Medium | Medium | Default token gets `superadmin` role. RBAC is additive — zero-config = full access. |
| Retention service deletes data users want | Low | High | Soft-delete first, hard-delete after configurable period. Retention config exposed in Settings. Users warned before first purge. |
| d3-force performance with large graphs | Medium | Low | Cap at 500 nodes in UI. Use WebGL renderer (d3-force-3d) for > 200 nodes. |

---

## 9. Appendix: File Change Index

Complete list of files created or modified, organized by phase.

### Phase 1 (Safety Hardening)
| Action | File |
|--------|------|
| CREATE | `crates/ghost-gateway/src/db_pool.rs` |
| MODIFY | `crates/ghost-gateway/src/state.rs` — replace `db` type |
| MODIFY | `crates/ghost-gateway/src/bootstrap.rs` — DbPool init, kill state check |
| MODIFY | `crates/ghost-gateway/src/runtime.rs` — checkpoint on shutdown |
| MODIFY | `crates/ghost-gateway/src/api/*.rs` (19 files) — `db.lock()` → `db.read()`/`db.write()` |
| MODIFY | `crates/ghost-gateway/Cargo.toml` — add `crossbeam-queue` |
| MODIFY | `src-tauri/capabilities/default.json` — arg allowlist |
| MODIFY | `crates/ghost-gateway/src/safety/kill_switch.rs` — write ordering |
| MODIFY | `src-tauri/src/commands/gateway.rs` — tokio::sync::Mutex |
| MODIFY | `crates/ghost-gateway/src/api/error.rs` — unified ApiError |
| CREATE | `src-tauri/src/error.rs` — GhostDesktopError |
| MODIFY | `crates/ghost-gateway/src/api/websocket.rs` — sequence numbers |
| CREATE | `dashboard/src/lib/env.ts` — typed globals |
| MODIFY | `dashboard/src/lib/api.ts` — generic types |
| MODIFY | `dashboard/src/lib/stores/*.svelte.ts` (9 files) — Resync + typed catch |
| MODIFY | `dashboard/src/routes/studio/+page.svelte` — incomplete stream |
| MODIFY | `dashboard/src/styles/global.css` — sr-only class |
| MODIFY | Multiple components — ARIA attributes |
| CREATE | `crates/ghost-gateway/tests/common/mod.rs` — test harness |
| CREATE | `crates/ghost-gateway/tests/test_*.rs` (6 files) — E2E tests |
| CREATE | `crates/ghost-gateway/src/auth/rbac.rs` — RBAC middleware |

### Phase 2 (Core ADE Experience)
| Action | File |
|--------|------|
| MODIFY | `dashboard/src/components/CommandPalette.svelte` — enhance |
| CREATE | `dashboard/src/lib/shortcuts.ts` — keyboard shortcuts |
| CREATE | `dashboard/src/components/StudioInput.svelte` — CodeMirror |
| CREATE | `dashboard/src/components/ArtifactPanel.svelte` — artifacts |
| CREATE | `dashboard/src/routes/agents/new/+page.svelte` — wizard |
| CREATE | `dashboard/src/routes/approvals/+page.svelte` — queue |
| MODIFY | `crates/ghost-gateway/src/api/*.rs` — pagination params |
| CREATE | `dashboard/src/components/VirtualMessageList.svelte` — virtual list |
| CREATE | `dashboard/src/components/Breadcrumb.svelte` — breadcrumbs |
| CREATE | `dashboard/src/components/NotificationPanel.svelte` — notifications |
| MODIFY | `extension/src/storage/sync.ts` — port fix |
| MODIFY | `crates/cortex-convergence/src/signals.rs` — S8 signal |

### Phase 3 (Subsystem Surfaces)
| Action | File |
|--------|------|
| CREATE | `dashboard/src/routes/channels/+page.svelte` |
| CREATE | `dashboard/src/routes/pc-control/+page.svelte` |
| CREATE | `dashboard/src/routes/itp/+page.svelte` |
| MODIFY | `dashboard/src/routes/workflows/[id]/+page.svelte` — canvas |
| CREATE | `crates/ghost-gateway/src/workflows/executor.rs` — runtime |
| CREATE | `dashboard/src/routes/memory/graph/+page.svelte` — knowledge graph |
| MODIFY | `dashboard/src/routes/convergence/+page.svelte` — enhanced |
| CREATE | `dashboard/src/routes/sessions/[id]/replay/+page.svelte` — replay |

### Phase 4 (Polish and Production)
| Action | File |
|--------|------|
| CREATE | `src-tauri/src/services/update_checker.rs` |
| CREATE | `dashboard/src/components/AutonomyDial.svelte` |
| CREATE | `crates/ghost-gateway/src/services/retention.rs` |
| CREATE | `crates/ghost-skills/src/mcp_bridge.rs` |
| CREATE | `src-tauri/src/services/deep_link.rs` |
| CREATE | `dashboard/src/routes/onboarding/+page.svelte` |

**Total: ~25 new files, ~40 modified files across 4 phases.**

---

*This document is a living artifact. Update it as implementation progresses and requirements evolve. Every change to this document should be committed with a reference to the task that motivated the change.*
