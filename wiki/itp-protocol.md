# itp-protocol

> Interaction Telemetry Protocol — the structured event stream that feeds convergence analysis.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 3 (Protocols & Boundaries) |
| Type | Library |
| Location | `crates/itp-protocol/` |
| Workspace deps | None |
| External deps | `serde`, `serde_json`, `uuid`, `chrono`, `sha2`, `thiserror`, `tracing`, `libc` (unix), `windows-sys` (windows) |
| Modules | `events` (5 event types), `privacy` (4 levels + SHA-256 hashing), `adapter` (consumer trait), `transport/` (JSONL + OTel) |
| Public API | `ITPEvent`, `PrivacyLevel`, `ITPAdapter`, `JsonlTransport`, `hash_content()`, `apply_privacy()` |
| Features | `otel` — enables OpenTelemetry OTLP transport |
| Test coverage | Dev-dependencies include proptest and tempfile |
| Downstream consumers | `ghost-agent-loop`, `ghost-gateway`, `convergence-monitor`, `cortex-convergence` (privacy levels) |

---

## Why This Crate Exists

Every convergence signal in `cortex-convergence` needs raw data: session timestamps, message counts, response latencies, vocabulary vectors. That data has to come from somewhere. ITP (Interaction Telemetry Protocol) is the structured event stream that captures every interaction between a human and an agent, then feeds it to the convergence pipeline.

ITP serves three purposes:

1. **Data collection:** Captures session lifecycle events, messages, agent state snapshots, and convergence alerts in a structured, typed format
2. **Privacy enforcement:** Applies 4-level privacy controls that determine whether message content is stored as plaintext or SHA-256 hash
3. **Transport abstraction:** Delivers events to consumers via pluggable transports (JSONL files, OpenTelemetry)

ITP is the boundary between "what happened" (raw interaction data) and "what does it mean" (convergence analysis). Everything upstream of ITP is raw events; everything downstream is derived analysis.

---

## Module Breakdown

### `events.rs` — The 5 Event Types (AC1)

```rust
#[serde(tag = "event_type", content = "data")]
pub enum ITPEvent {
    SessionStart(SessionStartEvent),
    SessionEnd(SessionEndEvent),
    InteractionMessage(InteractionMessageEvent),
    AgentStateSnapshot(AgentStateSnapshotEvent),
    ConvergenceAlert(ConvergenceAlertEvent),
}
```

**Tagged enum serialization.** The `#[serde(tag = "event_type", content = "data")]` attribute produces JSON like `{"event_type": "SessionStart", "data": {...}}`. This is the standard pattern for discriminated unions in JSON — the `event_type` field tells the consumer which variant to expect.

#### Event Types

| Event | When Emitted | Key Fields | Feeds Signal |
|-------|-------------|------------|-------------|
| `SessionStart` | User opens a conversation | `session_id`, `agent_id`, `channel`, `privacy_level` | S2 (inter-session gap) |
| `SessionEnd` | User closes conversation or timeout | `session_id`, `reason`, `message_count` | S1 (session duration) |
| `InteractionMessage` | Each human or agent message | `sender`, `content_hash`, `content_plaintext`, `token_count` | S3, S4, S5, S6, S7 |
| `AgentStateSnapshot` | Periodic agent state capture | `memory_count`, `goal_count`, `convergence_score`, `intervention_level` | Monitoring |
| `ConvergenceAlert` | Convergence threshold crossed | `alert_type`, `score`, `level`, `details` | Alerting |

**Key design decisions:**

1. **`InteractionMessageEvent` always includes `content_hash`.** Even at `Full` privacy level, the SHA-256 hash is computed and stored. This allows deduplication and integrity verification without accessing plaintext content.

2. **`content_plaintext` is `Option<String>`.** At `Minimal` privacy, this is `None` — the content is hashed but not stored. At `Standard` and above, the plaintext is included. This is the primary privacy control point.

3. **`MessageSender` enum.** Messages are tagged as `Human` or `Agent`. This is critical for S6 (initiative balance) — the convergence engine needs to know who initiated each message.

4. **`ConvergenceAlert` is an ITP event, not a separate system.** Alerts flow through the same event pipeline as regular interactions. This means they're persisted in the same JSONL files, subject to the same privacy controls, and visible to the same monitoring infrastructure.

### `privacy.rs` — 4 Privacy Levels (AC2)

```rust
pub enum PrivacyLevel {
    Minimal,   // Hash only — no plaintext stored
    Standard,  // Plaintext for vocabulary analysis (S4, S5)
    Full,      // Full plaintext for all fields
    Research,  // Full plaintext + additional research metadata
}
```

**Privacy level hierarchy:**

| Level | Content Storage | Signals Available | Use Case |
|-------|----------------|-------------------|----------|
| `Minimal` | SHA-256 hash only | S1, S2, S3, S6, S7, S8 | Maximum privacy, metadata-only analysis |
| `Standard` | Hash + plaintext | All 8 signals (S4, S5 need content) | Default for most deployments |
| `Full` | Full plaintext | All 8 signals | Detailed analysis, operator access |
| `Research` | Full + research metadata | All 8 + research extensions | Academic research deployments |

**`hash_content()` uses SHA-256, not blake3.** This is a deliberate algorithm choice documented in the source comments. blake3 is used for hash chains in `cortex-temporal` (where speed matters for chain verification). SHA-256 is used for content hashing in ITP (where interoperability matters — SHA-256 hashes can be verified by any system).

**`apply_privacy()` returns a tuple `(hash, Option<plaintext>)`.** The hash is always computed; the plaintext is included only if the privacy level permits it. This function is the single point where privacy decisions are made — all event construction goes through it.

**Custom hex encoding.** The crate includes a minimal `hex::encode()` function rather than depending on the `hex` crate. This avoids adding a dependency for a 5-line function.

### `adapter.rs` — The Consumer Trait (AC5)

```rust
pub trait ITPAdapter: Send + Sync {
    fn on_session_start(&self, event: &SessionStartEvent);
    fn on_message(&self, event: &InteractionMessageEvent);
    fn on_session_end(&self, event: &SessionEndEvent);
    fn on_agent_state(&self, event: &AgentStateSnapshotEvent);
}
```

**Key design decisions:**

1. **Object-safe trait.** `ITPAdapter` can be used as `Box<dyn ITPAdapter>`, enabling runtime polymorphism. The gateway can hold multiple adapters (JSONL + OTel) and dispatch events to all of them.

2. **`Send + Sync` bounds.** Adapters must be thread-safe. Events are emitted from the agent loop thread and consumed by transport threads.

3. **No `on_convergence_alert`.** The `ConvergenceAlert` event type exists in the enum but doesn't have a dedicated adapter method. Alerts are handled through the general event pipeline rather than a specialized callback. This keeps the adapter trait minimal.

4. **Borrows, not ownership.** All methods take `&self` and `&Event`. Adapters don't consume events — multiple adapters can process the same event.

### `transport/jsonl.rs` — JSONL File Transport (AC3)

```rust
pub struct JsonlTransport {
    base_dir: PathBuf,  // default: ~/.ghost/sessions/
}
```

Writes one JSON line per event to `~/.ghost/sessions/{session_id}/events.jsonl`.

**Key design decisions:**

1. **Per-session files.** Each session gets its own directory and JSONL file. This makes it trivial to find all events for a specific session, delete a session's data (right to erasure), or archive old sessions.

2. **Append-only writes.** Files are opened with `OpenOptions::append(true)`. Events are never modified or deleted — only appended. This provides a natural audit trail.

3. **Cross-platform file locking.** The transport uses advisory file locks (`flock` on Unix, `LockFileEx` on Windows) to prevent concurrent write corruption. This is necessary because the gateway and convergence monitor sidecar might both write to the same session file.

4. **Error logging, not propagation.** If a write fails, the error is logged via `tracing::error!` but not propagated. ITP is observability infrastructure — a failed event write should not crash the agent loop. The agent continues operating; the missing event is noted in logs.

5. **Platform-specific dependencies.** `libc` for Unix file locking, `windows-sys` for Windows file locking. These are conditional dependencies (`[target.'cfg(unix)'.dependencies]`) so they don't bloat the binary on the other platform.

### `transport/otel.rs` — OpenTelemetry Transport (AC4)

Feature-gated behind `otel`. Maps ITP events to OpenTelemetry spans with `itp.*` prefixed attributes. Currently a stub that logs via `tracing::debug!` — the full OTLP integration will use the `opentelemetry` crate.

---

## Security Properties

### Content Hashing

At `Minimal` privacy, message content is replaced with its SHA-256 hash. This is a one-way transformation — the original content cannot be recovered from the hash. The hash still allows:
- Deduplication (same content → same hash)
- Integrity verification (content hasn't been tampered with)
- Pattern detection (same hash appearing repeatedly)

### No Plaintext at Minimal

The `apply_privacy()` function is the single enforcement point. At `Minimal` level, `content_plaintext` is `None`. There is no code path that can bypass this — the privacy level is checked once, and the result is baked into the event struct.

### File Locking

The JSONL transport uses exclusive file locks during writes. This prevents:
- Interleaved writes from concurrent processes (gateway + sidecar)
- Partial line writes that would corrupt the JSONL format
- Race conditions during session directory creation

---

## Downstream Consumer Map

```
itp-protocol (Layer 3)
├── ghost-agent-loop (Layer 7)
│   └── Emits ITP events for every interaction turn
├── ghost-gateway (Layer 8)
│   └── Configures ITP adapters and privacy levels
├── convergence-monitor (Layer 9)
│   └── Reads ITP events for independent convergence verification
└── cortex-convergence (Layer 2)
    └── Uses PrivacyLevel enum for signal computation gating
```

---

## File Map

```
crates/itp-protocol/
├── Cargo.toml
├── src/
│   ├── lib.rs                # Module declarations
│   ├── events.rs             # 5 ITP event types
│   ├── privacy.rs            # 4 privacy levels + SHA-256 hashing
│   ├── adapter.rs            # ITPAdapter trait
│   └── transport/
│       ├── mod.rs            # Transport module (jsonl always, otel feature-gated)
│       ├── jsonl.rs          # JSONL file transport with cross-platform locking
│       └── otel.rs           # OpenTelemetry OTLP transport (feature: otel)
```

---

## Common Questions

### Why SHA-256 for content hashing and not blake3?

blake3 is faster, but SHA-256 is more interoperable. ITP content hashes may be verified by external systems (audit tools, compliance checkers) that don't have blake3 support. SHA-256 is universally supported. The performance difference is irrelevant — content hashing happens once per message, not in a hot loop.

### Why JSONL and not a database?

JSONL files are:
- Human-readable (you can `cat` them)
- Append-only by nature (no UPDATE/DELETE complexity)
- Trivially portable (copy the file)
- No server process required (unlike SQLite WAL mode)
- Easy to compress and archive

For a telemetry stream that's written frequently and read rarely, JSONL is the right format. If query performance becomes important, the events can be loaded into a database by `ghost-audit`.

### Why is PrivacyLevel defined in both itp-protocol and cortex-convergence?

`cortex-convergence` defines its own `PrivacyLevel` enum (in `signals/mod.rs`) that mirrors the ITP version. This is intentional — `cortex-convergence` is Layer 2 and cannot depend on `itp-protocol` (Layer 3). The two enums have the same variants and semantics but are separate types. The mapping between them happens at the integration layer (`ghost-agent-loop`).
