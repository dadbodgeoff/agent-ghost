# ghost-heartbeat

> Convergence-aware heartbeat engine and cron scheduler — keeps agents alive with tiered monitoring that speeds up when things go wrong, plus standard cron syntax for scheduled tasks.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 5 (Agent Services) |
| Type | Library |
| Location | `crates/ghost-heartbeat/` |
| Workspace deps | `cortex-core`, `ghost-agent-loop` |
| External deps | `blake3`, `serde`, `serde_json`, `serde_yaml`, `chrono`, `uuid`, `tokio`, `tracing`, `thiserror` |
| Modules | `heartbeat` (engine), `tiers` (4-tier system + interval mapping), `cron` (scheduler) |
| Public API | `HeartbeatEngine`, `TierSelector`, `TieredHeartbeatState`, `CronEngine`, `interval_for_state()` |
| Heartbeat tiers | Tier0 (binary ping, 0 tokens), Tier1 (delta-encoded, 0 tokens), Tier2 (snapshot, minimal tokens), Tier3 (full LLM, max 5%) |
| Interval mapping | Stable→120s, Active→30s, Escalated→15s, Critical→5s |
| Test coverage | Unit tests (inline), integration tests, tiered state tests |
| Downstream consumers | `ghost-gateway` (heartbeat lifecycle), `convergence-monitor` (receives pings) |

---

## Why This Crate Exists

Agents need periodic self-checks. Without heartbeats, an agent sits idle between user messages — it won't notice that a background task failed, a convergence score drifted, or a scheduled job is due. `ghost-heartbeat` provides two mechanisms:

1. **Heartbeat engine** — Fires synthetic messages at convergence-aware intervals. When the agent is stable, beats are infrequent (120s). When convergence is escalating, beats speed up to 5-second binary pings. This is the opposite of the naive approach (slow down when things are bad) — you want MORE monitoring when things are going wrong.

2. **Cron engine** — Standard 5-field cron syntax for scheduled tasks. "Run a daily check at 9am UTC" or "Summarize activity every hour." Each job tracks its own cost, and the kill switch stops all jobs instantly.

### The Task 20.4 Key Fix

The original heartbeat implementation had a critical design flaw: it SLOWED DOWN at higher convergence levels (L0-1→30m, L2→60m, L3→120m, L4→disabled). This meant that when an agent was in crisis (L4), it received NO heartbeats — exactly when it needed the most monitoring.

Task 20.4 inverted this. The new tiered system SPEEDS UP at higher levels and L4 is never disabled. The deprecated `interval_for_level()` function is preserved with a deprecation warning for backward compatibility, but all new code uses `interval_for_state()`.

---

## Module Breakdown

### `heartbeat.rs` — The Heartbeat Engine

The engine manages the heartbeat lifecycle for a single agent: when to fire, what message to send, and how to bridge into the agent loop.

#### Dedicated Session Key

```rust
pub fn heartbeat_session_key(agent_id: Uuid) -> Uuid {
    let input = format!("{}:heartbeat:{}", agent_id, agent_id);
    let hash = blake3::hash(input.as_bytes());
    let bytes: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap();
    Uuid::from_bytes(bytes)
}
```

**Why a dedicated session?** Heartbeat turns run in their own session, separate from user conversations. This prevents heartbeat tool calls from polluting the user's conversation context. The session key is deterministic — `hash(agent_id, "heartbeat", agent_id)` — so it's the same across restarts. This means the heartbeat session accumulates context over time (previous check results, known issues) rather than starting fresh each boot.

**Why blake3 → UUID?** The first 16 bytes of the blake3 hash are reinterpreted as a UUID. This gives a deterministic, collision-resistant session identifier that works with all existing session management infrastructure (which expects UUIDs).

#### The Synthetic Message

```rust
pub const HEARTBEAT_MESSAGE: &str = "[HEARTBEAT] Check HEARTBEAT.md and act if needed.";
```

This is the message injected into the agent loop when a heartbeat fires. The `[HEARTBEAT]` prefix lets the agent (and its system prompt) distinguish heartbeat turns from user messages. The instruction to "check HEARTBEAT.md" directs the agent to a user-configurable file that defines what the agent should do during heartbeat turns.

#### Configuration

```rust
pub struct HeartbeatConfig {
    pub base_interval_minutes: u32,    // Default: 30 (deprecated path only)
    pub active_hours_start: u8,        // Default: 8 (8am)
    pub active_hours_end: u8,          // Default: 22 (10pm)
    pub timezone_offset_hours: i32,    // Default: 0 (UTC)
    pub cost_ceiling: f64,             // Default: $0.50
}
```

**Design decisions:**

1. **Cost ceiling.** Every heartbeat turn costs tokens. The `cost_ceiling` prevents runaway spending — once cumulative heartbeat cost hits $0.50 (default), no more beats fire. This is a hard stop, not a soft warning.

2. **Active hours.** Heartbeats respect working hours. No point running expensive Tier3 beats at 3am if the user isn't around to act on findings. The timezone offset allows per-agent configuration.

3. **`base_interval_minutes` is deprecated.** It's only used by the old `interval_for_level()` function. The new tiered system computes intervals from convergence state, not a base interval.

#### Three Safety Checks Before Every Beat

```rust
pub fn should_fire(&self, convergence_level: u8) -> bool {
    // 1. Kill switch — PLATFORM_KILLED stops everything
    if self.platform_killed.load(Ordering::SeqCst) { return false; }
    // 2. Agent pause — per-agent pause/quarantine
    if self.agent_paused.load(Ordering::SeqCst) { return false; }
    // 3. Cost ceiling — cumulative spend limit
    if self.total_cost >= self.config.cost_ceiling { return false; }
    // ... then check interval
}
```

**Why `SeqCst` ordering?** The kill switch and pause flags are set by other threads (the gateway's safety system, the convergence monitor). `SeqCst` ensures the heartbeat engine sees the most recent value — no stale reads that could cause a beat to fire after the kill switch was activated.

#### Firing a Beat

The `fire()` method bridges the heartbeat engine to the agent loop:

1. Calls `runner.pre_loop()` with the heartbeat session key and synthetic message
2. Runs a full agent turn via `runner.run_turn()`
3. Reads the convergence score from the shared state file
4. Records the beat with cost and convergence score for tiered tracking

**Why read convergence from a file?** The convergence monitor runs as a separate process (the sidecar pattern). The heartbeat engine reads the monitor's published state from `~/.ghost/data/convergence_state/{agent_id}.json`. If the file doesn't exist (first boot, monitor down), it defaults to 0.0 — safe because 0.0 maps to the "Active" tier (30s interval), which is a reasonable default.

---

### `tiers.rs` — The 4-Tier Heartbeat System

This is the core innovation from Task 20.4. Not all heartbeats need to invoke the LLM. Most of the time, a simple "I'm alive" ping is sufficient.

#### Four Tiers

| Tier | Payload | Token Cost | When Used |
|------|---------|------------|-----------|
| Tier0 | Binary ping (16 bytes) | 0 | Stable state (3+ consecutive small deltas) or L4 critical |
| Tier1 | Delta-encoded state (~20 bytes) | 0 | Minor score changes (delta < 0.05) |
| Tier2 | Full state snapshot | Minimal | Notable changes (delta ≥ 0.05) or escalated (L2+) |
| Tier3 | Full LLM invocation | Full | Critical escalation (L3+ with delta ≥ 0.1), max 5% of beats |

**Why 4 tiers instead of 2 (ping vs. full)?** Granularity. The jump from "zero tokens" to "full LLM invocation" is enormous. Tier1 (delta-encoded) and Tier2 (snapshot) provide intermediate options that capture state changes without burning tokens. The convergence monitor can process Tier0-2 beats without involving the LLM at all.

#### Tier Selection Logic

```rust
pub fn select_tier(&mut self, score_delta: f64, consecutive_stable: u32, convergence_level: u8) -> HeartbeatTier {
    if convergence_level >= 3 && delta >= 0.1 { Tier3 }      // Crisis
    else if delta >= 0.05 || convergence_level >= 2 { Tier2 } // Notable
    else if delta < 0.01 && consecutive_stable >= 3 { Tier0 } // Stable
    else if delta < 0.05 { Tier1 }                            // Minor
    else { Tier2 }                                             // Fallback
}
```

**The 5% Tier3 cap:** Even in crisis, no more than 5% of beats can be Tier3 (full LLM). This prevents a convergence oscillation from burning through the cost ceiling in minutes. When the cap is hit, Tier3 candidates are downgraded to Tier2.

**NaN/Inf sanitization:** If the convergence score is corrupted (NaN or Infinity), the tier selector treats it as 0.0 delta (stable). This prevents a single corrupted score from triggering an avalanche of Tier3 beats.

#### Convergence-Aware Interval Mapping

```rust
pub fn interval_for_state(score_delta: f64, consecutive_stable: u32, convergence_level: u8) -> Duration {
    if convergence_level >= 4 { 5s }        // Critical: rapid binary pings
    else if convergence_level >= 2 { 15s }  // Escalated: frequent monitoring
    else if delta < 0.01 && consecutive_stable >= 3 { 120s }  // Stable: relaxed
    else { 30s }                             // Active: moderate
}
```

**The key insight:** Higher convergence levels mean MORE frequent beats, not fewer. When an agent is at L4 (critical), you want 5-second pings to detect recovery or further degradation as fast as possible. The old system disabled heartbeats at L4 — a dangerous gap in monitoring.

#### Hysteresis: The 3-Beat Stability Requirement

A single low-delta beat doesn't mean the agent is stable. The system requires 3 consecutive beats with `score_delta < 0.01` before transitioning to the Stable tier (120s interval). This prevents oscillation between Stable and Active tiers when the score is hovering near a threshold.

```rust
pub fn record_beat(&mut self, current_score: f64) {
    let delta = self.score_delta(current_score);
    if delta < 0.01 {
        self.consecutive_stable += 1;
    } else {
        self.consecutive_stable = 0;  // Reset on any non-trivial change
    }
}
```

**Why 3, not 5 or 10?** At 30-second intervals (Active tier), 3 beats = 90 seconds of stability. That's enough to distinguish genuine stability from a brief pause in activity. 5 beats (150s) would delay the transition to relaxed monitoring unnecessarily.

#### Delta-Encoded State

```rust
pub struct HeartbeatDelta {
    pub agent_id: Uuid,
    pub seq: u64,                              // Monotonic sequence number
    pub convergence_score: Option<f64>,         // Only if changed
    pub active_goals: Option<u32>,              // Only if changed
    pub session_duration_minutes: Option<u32>,  // Only if changed
    pub error_count: Option<u32>,               // Only if changed
}
```

Tier1 beats send only the fields that changed since the last beat. If the convergence score moved but goals and errors didn't, only `convergence_score` is `Some`. This minimizes payload size for the common case (score changes, everything else stays the same).

**Monotonic sequence number:** The `seq` field increments on every beat. This lets the receiver detect missed beats (gap in sequence) and reorder out-of-order deliveries.

---

### `cron.rs` — Scheduled Task Engine

The cron engine provides standard cron scheduling for agent tasks. It's simpler than the heartbeat engine — no convergence awareness, no tiers — just "run this message at this time."

#### Job Definition (YAML)

```rust
pub struct CronJobDef {
    pub name: String,
    pub schedule: String,          // "0 9 * * *" = 9am daily
    pub message: String,           // Injected as synthetic message
    pub target_channel: Option<String>,
    pub timezone: String,          // IANA timezone, default "UTC"
    pub enabled: bool,             // Default true
}
```

**Why YAML?** Cron jobs are defined in user-facing configuration files. YAML is more readable than JSON for this use case — no quotes around keys, no trailing commas to worry about. The `serde_yaml` dependency is shared with the skill manifest parser.

#### Cron Expression Parsing

```rust
pub fn cron_matches(schedule: &str, dt: DateTime<Utc>) -> bool
```

The parser supports standard 5-field cron syntax: `minute hour day-of-month month day-of-week`. It handles `*` (any) and numeric values. Ranges and steps (`*/5`, `1-5`) are not currently supported — this keeps the parser simple and auditable.

**Why not use a cron parsing library?** Dependency minimization. A full cron library (like `cron` or `job_scheduler`) adds transitive dependencies and complexity for features GHOST doesn't need. The simple parser handles the common cases; if range/step support is needed later, it can be added without a new dependency.

#### Safety Checks

Like the heartbeat engine, the cron engine checks `PLATFORM_KILLED` and `agent_paused` before returning ready jobs. It also enforces a minimum 60-second interval between runs of the same job to prevent double-firing within the same minute.

#### Per-Job Cost Tracking

```rust
pub struct CronJobState {
    pub def: CronJobDef,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub total_cost: f64,
}
```

Each job tracks its own cumulative cost. This enables per-job cost reporting in the dashboard and allows future per-job cost ceilings (not yet implemented, but the data is there).

---

## Security Properties

### Kill Switch Respect

Both engines check `PLATFORM_KILLED` (global) and `agent_paused` (per-agent) before every operation. These are `Arc<AtomicBool>` with `SeqCst` ordering — the strongest memory ordering guarantee. When the kill switch fires, heartbeats and cron jobs stop within one check cycle.

### Cost Ceiling Enforcement

The heartbeat engine has a hard cost ceiling (default $0.50). Once reached, no more beats fire regardless of convergence state. This prevents a runaway agent from burning through API credits via heartbeat turns.

### Deterministic Session Isolation

Heartbeat turns run in a dedicated session derived from `blake3(agent_id + "heartbeat" + agent_id)`. This session is isolated from user conversations — heartbeat tool calls, context, and history don't leak into the user's chat.

### NaN/Inf Defense

Corrupted convergence scores (NaN, Infinity) are sanitized to 0.0 in both `score_delta()` and `interval_for_state()`. This prevents a single corrupted value from cascading into incorrect tier selection or interval computation.

---

## Downstream Consumer Map

```
ghost-heartbeat (Layer 5)
├── ghost-gateway (Layer 8)
│   └── Creates HeartbeatEngine per agent at startup
│   └── Runs heartbeat loop on tokio interval
│   └── Creates CronEngine and loads job definitions
│   └── Dispatches cron jobs to agent loop
├── convergence-monitor (Layer 9)
│   └── Receives Tier0/Tier1 pings via UDP/unix socket
│   └── Processes Tier2 snapshots for state tracking
└── ghost-agent-loop (Layer 7) [upstream]
    └── AgentRunner.pre_loop() + run_turn() called by fire()
```

---

## Test Strategy

### Inline Unit Tests (`src/tiers.rs`)

| Test | What It Verifies |
|------|-----------------|
| `stable_state_tier0_120s` | 3+ stable beats → Tier0 + 120s interval |
| `active_state_tier1_30s` | Moving score → Tier1 + 30s interval |
| `escalated_state_tier2_15s` | Level ≥ 2 → Tier2 + 15s interval |
| `critical_state_tier0_5s_not_disabled` | L4 → 5s interval (NOT disabled) |
| `tier3_cap_enforcement` | >5% Tier3 beats downgraded to Tier2 |
| `nan_score_delta_treated_as_stable` | NaN → Tier0 (safe default) |
| `delta_all_none_when_unchanged` | Unchanged state → all delta fields None |
| `delta_only_changed_fields` | Only changed fields are Some |
| `hysteresis_needs_3_consecutive_stable` | Stability counter resets on active beat |

### Integration Tests (`tests/heartbeat_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `fires_at_configured_interval` | First beat fires immediately |
| `uses_dedicated_session` | Session key is deterministic and distinct from agent_id |
| `message_matches_spec` | Synthetic message matches `HEARTBEAT_MESSAGE` constant |
| `stable_state_120s` | interval_for_state: stable → 120s |
| `active_state_30s` | interval_for_state: active → 30s |
| `escalated_state_15s` | interval_for_state: escalated → 15s |
| `critical_l4_not_disabled_5s` | interval_for_state: L4 → 5s (key fix) |
| `l3_escalated_15s` | Level 3 → 15s |
| `platform_killed_stops_heartbeat` | Kill switch → should_fire returns false |
| `agent_paused_stops_heartbeat` | Pause flag → should_fire returns false |
| `cost_ceiling_stops_heartbeat` | Cost at ceiling → should_fire returns false |
| `record_beat_updates_state` | Beat recording updates last_beat and total_cost |
| `record_beat_with_score_tracks_tiered_state` | Score tracking updates tiered state |
| `l4_should_fire_not_disabled` | L4 convergence level → should_fire returns true |
| `parses_cron_syntax` | `* * * * *` matches any time |
| `invalid_cron_syntax` | Too few fields → no match |
| `loads_jobs_from_yaml` | YAML parsing produces correct job definition |
| `disabled_job_not_loaded` | `enabled: false` → job not added to engine |
| `invalid_yaml_graceful` | Malformed YAML → no crash, no jobs loaded |
| `platform_killed_no_ready_jobs` | Kill switch → empty ready list |
| `agent_paused_no_ready_jobs` | Pause → empty ready list |
| `record_run_updates_state` | Run recording updates count and cost |
| `timezone_defaults_to_utc` | Missing timezone field → "UTC" |

---

## File Map

```
crates/ghost-heartbeat/
├── Cargo.toml                          # Deps: cortex-core, ghost-agent-loop, blake3, serde_yaml
├── src/
│   ├── lib.rs                          # Module declarations
│   ├── heartbeat.rs                    # HeartbeatEngine, session key, fire(), cost ceiling
│   ├── tiers.rs                        # 4-tier system, TierSelector, interval_for_state(), delta encoding
│   └── cron.rs                         # CronEngine, YAML job defs, cron expression parser
└── tests/
    └── heartbeat_tests.rs              # Tiered interval tests, safety checks, cron tests
```

---

## Common Questions

### Why does the heartbeat depend on `ghost-agent-loop`?

The heartbeat engine's `fire()` method calls `AgentRunner::pre_loop()` and `run_turn()` directly. A heartbeat turn IS an agent turn — it goes through the same 6-gate safety pipeline, the same tool execution, the same output inspection. The alternative (reimplementing a lightweight agent turn) would bypass safety checks and create a maintenance burden.

### Why not use tokio's built-in interval for scheduling?

The heartbeat interval is dynamic — it changes based on convergence state. A `tokio::time::interval(30s)` is fixed. The engine computes the interval on every `should_fire()` call based on the current tiered state, so the interval adapts in real-time as convergence changes.

### What happens if the convergence monitor is down?

The heartbeat engine reads convergence scores from a shared state file. If the file doesn't exist or can't be parsed, `read_convergence_score()` returns 0.0. This maps to the Active tier (30s interval) — a safe default that provides moderate monitoring without the overhead of Tier3 beats.

### Why is `interval_for_level()` deprecated but not removed?

Backward compatibility. Existing configurations and tests may reference the old function. The deprecation warning directs users to `interval_for_state()` with a clear explanation of why the old behavior was wrong. It will be removed in a future major version.

### Can I add custom tiers?

Not currently. The 4-tier system is hardcoded because each tier has specific payload semantics (binary ping, delta, snapshot, LLM). Adding a tier would require changes to the convergence monitor's receiver, the dashboard's display logic, and the cost tracking system. The current tiers cover the full spectrum from zero-cost to full-cost beats.
