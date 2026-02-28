# GHOST Platform Infrastructure Audit

> Deep cross-reference of the research blueprint (heartbeat architecture, compressor-predictor,
> KV cache optimization, cron scheduling, context compaction, mesh protocol, skill evolution)
> against the actual codebase state. Every item accounted for.
>
> Audit date: 2026-02-28
> Scope: 34 workspace crates, 6 research domains, 8 optimization techniques

---

## Audit Methodology

For each research blueprint item, this document records:
- **BLUEPRINT**: What the research says to build
- **CODEBASE**: What actually exists (file, struct, function, line-level)
- **STATUS**: `IMPLEMENTED` | `PARTIAL` | `STUBBED` | `MISSING` | `WIRING_NEEDED`
- **GAP**: Precise description of what's missing or needs changing
- **EFFORT**: T-shirt size (S/M/L/XL)

---

## 1. HEARTBEAT ARCHITECTURE (OpenClaw Pattern)

### 1.1 Tiered Heartbeat System

**BLUEPRINT**: 4-tier heartbeat: Tier 0 (binary ping, 16 bytes, zero tokens), Tier 1 (delta-encoded state, ~20 bytes), Tier 2 (full state snapshot, minimal tokens), Tier 3 (escalation, tokens spent, max 5% of beats).

**CODEBASE**: `crates/ghost-heartbeat/src/heartbeat.rs`
- `HeartbeatEngine` struct exists with `config`, `agent_id`, `session_key`, `platform_killed`, `last_beat`, `total_cost`
- `HeartbeatConfig` has `base_interval_minutes` (default 30), `active_hours_start/end`, `timezone_offset_hours`, `cost_ceiling`
- `should_fire()` checks kill switch, agent pause, cost ceiling, convergence-aware interval
- `interval_for_level()` maps L0-1→base, L2→2x, L3→4x, L4→disabled
- `heartbeat_session_key()` generates deterministic session UUID via blake3
- Canonical message: `"[HEARTBEAT] Check HEARTBEAT.md and act if needed."`

**STATUS**: `PARTIAL`

**GAP**: The current heartbeat is a single-tier system — every heartbeat is a full LLM-invoking message. The blueprint calls for 4 tiers where Tier 0 and Tier 1 are zero-token binary/delta pings. Specifically missing:
- `HeartbeatDelta` struct (agent_id, seq, Option fields for changed-only state)
- Tier classification logic (which tier to use based on state change magnitude)
- Binary UDP/unix-socket ping for Tier 0 (currently all heartbeats go through the agent loop)
- Delta encoding: only send fields that changed since last heartbeat
- Tier budget tracking: enforce max 5% of beats invoke LLM (Tier 3)
- Hysteresis-based frequency: stable→120s, active→30s, escalated→15s, critical→5s (current system uses minute-granularity: 30m/60m/120m/disabled — much coarser)

**EFFORT**: L (requires new transport layer for binary heartbeats, restructuring of HeartbeatEngine)

---

### 1.2 Heartbeat Frequency Hysteresis

**BLUEPRINT**: State machine with hysteresis: stable (score delta < 0.01 for 3 beats) → stretch to 120s; active (score moving) → 30s; escalated (level >= 2) → 15s; critical (level >= 4) → 5s binary only.

**CODEBASE**: `crates/ghost-heartbeat/src/heartbeat.rs` → `interval_for_level()`
```rust
pub fn interval_for_level(base_minutes: u32, convergence_level: u8) -> Option<Duration> {
    match convergence_level {
        0 | 1 => Some(Duration::from_secs(base_minutes as u64 * 60)),      // 30min
        2 => Some(Duration::from_secs(base_minutes as u64 * 60 * 2)),      // 60min
        3 => Some(Duration::from_secs(base_minutes as u64 * 60 * 4)),      // 120min
        _ => None, // L4+ → disabled
    }
}
```

**STATUS**: `PARTIAL`

**GAP**: Current implementation maps convergence LEVEL to interval, but the blueprint calls for mapping convergence SCORE DELTA (rate of change) to interval. The current system:
- Uses minute-granularity (30m/60m/120m) vs blueprint's second-granularity (30s/120s/15s/5s)
- Disables heartbeat at L4 — blueprint says L4 should use 5s binary-only (most critical monitoring)
- No score-delta tracking (no `last_score` field to compute delta)
- No "3 consecutive stable beats" hysteresis logic
- Direction is inverted: current system SLOWS DOWN at higher levels, blueprint SPEEDS UP monitoring at higher levels (which makes more sense for safety)

**EFFORT**: M (logic change in interval_for_level, add score delta tracking)

---

### 1.3 Convergence Monitor Heartbeat Integration

**BLUEPRINT**: Monitor's existing rate limiter (100 events/min) provides backpressure for heartbeat events.

**CODEBASE**: `crates/convergence-monitor/src/validation.rs` → `RateLimiter`
- Rate limiter exists with configurable `rate_limit_per_min` (from `MonitorConfig`)
- `crates/convergence-monitor/src/monitor.rs` → `ConvergenceMonitor` uses `rate_limiter` field

**STATUS**: `IMPLEMENTED` (the backpressure mechanism exists)

**GAP**: None for the rate limiter itself. The gap is that heartbeat events don't currently flow through the monitor as tiered events — they go through the agent loop as full LLM messages. Wiring the tiered heartbeat to emit Tier 0/1 events to the monitor (bypassing the agent loop) is the missing piece.

**EFFORT**: M (transport wiring)

---

## 2. COMPRESSOR-PREDICTOR ARCHITECTURE (Stanford, Dec 2025)

### 2.1 ContentQuarantine as Compressor

**BLUEPRINT**: Flip ContentQuarantine from defensive-only to primary context optimization layer. Raw tool output (50K tokens) → Compressor LLM (local 3-7B) → Structured extraction (500-2000 tokens) → Main agent context.

**CODEBASE**: `crates/ghost-llm/src/quarantine.rs`
- `QuarantinedLLM` struct: wraps `Arc<dyn LLMProvider>` with restrictions
  - Empty tool list (`tool_schemas()` returns `Vec::new()`)
  - System prompt: "You are a data extraction assistant. Extract structured information..."
  - Max output tokens capped (default 2000)
  - Uses Free/Cheap tier only
- `ContentQuarantine` struct:
  - `should_quarantine(content_type: &str) -> bool` — checks against configured content types
  - `quarantine_content(content: &str, extraction_prompt: &str) -> Result<String, LLMError>`
  - `is_enabled() -> bool`
- `QuarantineConfig`:
  - `enabled: bool` (default `false` — opt-in)
  - `max_output_tokens: usize` (default 2000)
  - `model_tier: QuarantineModelTier` (default Cheap)
  - `content_types: Vec<String>` (which tool outputs trigger quarantine)

**STATUS**: `PARTIAL`

**GAP**: The quarantine exists as a defensive mechanism but is NOT wired as a general-purpose compressor for ALL tool outputs. Specifically:
- Default is `enabled: false` — must be opt-in per content type
- Only triggers for configured `content_types` (e.g., "web_fetch", "email_read")
- Does NOT compress general tool outputs (file reads, API responses, search results)
- No local model support — uses the same provider as the main agent (blueprint calls for a dedicated local 3-7B model like Qwen-2.5-7B)
- No `bits_per_token` tracking for information density measurement
- No integration with PromptCompiler L7/L8 compression pipeline
- `QuarantineModelTier` only has `Free` and `Cheap` — no `Local` variant for local models
- Fallback on quarantine failure is "direct content injection with datamarking" — correct per blueprint

**EFFORT**: L (requires local model integration, PromptCompiler wiring, new compression pipeline)

---

### 2.2 Observation Masking (Old Tool Outputs)

**BLUEPRINT**: Replace old tool outputs with references: `[tool_result: file_read auth.rs → 847 lines, see .ghost/cache/tool_outputs/abc123]`. Simple observation masking halves cost while matching LLM summarization quality.

**CODEBASE**: `crates/ghost-agent-loop/src/context/prompt_compiler.rs`
- `PromptCompiler::compile()` assembles all 10 layers
- `apply_truncation()` truncates by token count when exceeding context window
- Truncation priority: L8 > L7 > L5 > L2 (never L0, L1, L9)
- L8 (conversation history) gets `Budget::Remainder` — whatever's left after fixed layers

**STATUS**: `MISSING`

**GAP**: There is NO observation masking. The current system truncates L8 by raw character count when it exceeds budget, which:
- Loses information non-selectively (truncates from the end, not by age/relevance)
- Does not replace old tool outputs with compact references
- Does not cache tool outputs to disk for on-demand retrieval
- Does not distinguish between tool outputs and conversational turns in L8
- No `.ghost/cache/tool_outputs/` directory or caching mechanism
- No `ObservationMasker` struct or equivalent

The PromptCompiler would need a pre-processing step before L8 assembly that:
1. Identifies tool result blocks in conversation history
2. For results older than N turns, replaces with compact reference
3. Writes full output to cache file
4. Injects reference token into conversation

**EFFORT**: M (new ObservationMasker module in ghost-agent-loop/src/context/)

---

### 2.3 L7 Memory Compression

**BLUEPRINT**: Compress MEMORY.md + daily logs through local model before injection into L7. ConvergenceAwareFilter already reduces this — add a compression step after filtering.

**CODEBASE**: 
- `crates/cortex/cortex-convergence/src/filtering/convergence_aware_filter.rs` → `ConvergenceAwareFilter::filter()` — 4-tier memory filtering by convergence score
- `crates/ghost-agent-loop/src/context/prompt_compiler.rs` → L7 gets `Budget::Fixed(4000)` tokens

**STATUS**: `MISSING`

**GAP**: ConvergenceAwareFilter filters by memory TYPE (removes emotional/attachment at higher scores), but does NOT compress the remaining memories. The blueprint calls for:
- Post-filter compression step: filtered memories → local compressor LLM → condensed summary
- This would reduce L7 from ~4000 tokens of raw memories to ~500-1000 tokens of compressed summaries
- No `MemoryCompressor` struct exists
- No integration point between ConvergenceAwareFilter output and PromptCompiler L7 input

**EFFORT**: M (new compression step, requires local model or summarization logic)

---

## 3. KV CACHE OPTIMIZATION

### 3.1 Stable Prefix Preservation (L0-L6)

**BLUEPRINT**: L0 (CORP_POLICY), L1 (simulation boundary), L2 (SOUL.md), L3 (tool schemas), L4 (environment), L5 (skill index) are stable within a session — ~7000 tokens of stable prefix. Never mutate these layers mid-session. 90% cost reduction and 85% latency reduction for the stable prefix portion.

**CODEBASE**: `crates/ghost-agent-loop/src/context/prompt_compiler.rs`
- 10 layers defined with fixed budgets:
  - L0: CORP_POLICY (Uncapped) — immutable
  - L1: Simulation boundary (Fixed 200) — platform-injected
  - L2: SOUL_IDENTITY (Fixed 2000)
  - L3: TOOL_SCHEMAS (Fixed 3000)
  - L4: ENVIRONMENT (Fixed 200)
  - L5: SKILL_INDEX (Fixed 500)
  - L6: CONVERGENCE_STATE (Fixed 1000)
  - Total stable prefix: ~6900 tokens
- `PromptInput` struct takes all layer content as `String` fields
- `compile()` assembles layers sequentially

**STATUS**: `PARTIAL`

**GAP**: The layer structure is correct for KV cache optimization, but the implementation does NOT enforce cache-friendliness:
- No guarantee that L0-L5 content is identical across turns (content is passed in fresh each time via `PromptInput`)
- No content hashing to detect mutations that would invalidate cache
- L4 (environment) may include timestamps — blueprint says use date-only granularity
- L6 (convergence state) changes every evaluation — this is expected, but it should be the FIRST mutable layer (it is, at position 6)
- No `StablePrefixCache` struct that memoizes L0-L5 content hash and detects changes
- Tool schemas in L3 may be dynamically filtered by convergence level (`filter_tool_schemas()` exists) — this INVALIDATES the cache. Blueprint says: keep all tool schemas in L3, use logit masking instead of removal

**Specific issue with L3 tool filtering**:
```rust
// prompt_compiler.rs — filter_tool_schemas() removes tools at higher levels
// This changes L3 content per convergence level → cache miss
pub fn filter_tool_schemas(schemas: &str, intervention_level: u8) -> String {
    // At higher levels, filter out non-essential tools
    // ...
}
```
Blueprint alternative: keep L3 constant, add constraint instruction to L6: "At current convergence level, only task-focused tools are permitted."

**EFFORT**: M (refactor tool filtering from L3 removal to L6 constraint instruction, add stable prefix validation)

---

### 3.2 Deterministic Serialization

**BLUEPRINT**: Use BTreeMap (not HashMap) for all signed payloads. Different key order = different tokens = cache miss.

**CODEBASE**: Workspace-wide convention already established:
- `crates/ghost-mesh/src/types.rs` → `AgentCard::canonical_bytes()` uses `serde_json::to_vec()` with BTreeMap fields
- `crates/cortex/cortex-core/src/safety/trigger.rs` → `TriggerEvent::MemoryHealthCritical` uses `BTreeMap<String, f64>` for `sub_scores`
- `crates/ghost-oauth/src/types.rs` → `ApiRequest` uses `BTreeMap<String, String>` for headers
- `crates/convergence-monitor/src/monitor.rs` → `calibration_counts`, `score_cache`, `hash_chains` all use `BTreeMap`
- `crates/ghost-mesh/src/trust/local_trust.rs` → `LocalTrustStore` uses `BTreeMap` for interactions and cache
- `crates/ghost-mesh/src/trust/eigentrust.rs` → `compute_global_trust()` returns `BTreeMap<Uuid, f64>`

**STATUS**: `IMPLEMENTED`

**GAP**: None. BTreeMap is used consistently across all signed/serialized payloads. The convention is documented in the tasks spec header: "BTreeMap for signed payloads."

**EFFORT**: None

---

### 3.3 Append-Only Context

**BLUEPRINT**: Context must be append-only. Any mutation to earlier content invalidates the cache from that point forward. Truncation happens at the end (L8), not the beginning (L0).

**CODEBASE**: `crates/ghost-agent-loop/src/context/token_budget.rs`
```rust
pub fn truncation_order() -> [u8; 4] {
    [8, 7, 5, 2]  // L8 first, then L7, L5, L2. NEVER L0, L1, L9.
}
```

**STATUS**: `IMPLEMENTED`

**GAP**: Truncation order is correct — L8 (end of context) is truncated first, L0/L1/L9 are never truncated. However:
- The spotlighting system instruction is injected INTO L1 (`compile()` modifies L1 content) — this is a mutation of an early layer. Should be a one-time setup, not per-turn injection.
- No enforcement that L0-L5 content doesn't change between turns (see 3.1)

**EFFORT**: S (move spotlighting instruction to a fixed part of L1 template)

---

### 3.4 No Dynamic Timestamps in Early Layers

**BLUEPRINT**: Never put timestamps with seconds/milliseconds at the start of prompts. Date is fine, hour is acceptable (cache TTL is 5-10 min). L4 (environment context) should use date-only granularity.

**CODEBASE**: `crates/ghost-agent-loop/src/context/prompt_compiler.rs`
- L4 is `environment` field in `PromptInput` — content is caller-provided
- No timestamp formatting logic in the prompt compiler itself

**STATUS**: `WIRING_NEEDED`

**GAP**: The prompt compiler doesn't control what goes into L4 — that's the caller's responsibility. Need to:
- Document the constraint: L4 must use date-only granularity
- Add validation in `compile()` that L4 doesn't contain time-of-day patterns (regex check)
- Or: strip timestamps from L4 content before assembly

**EFFORT**: S (validation/documentation)

---

## 4. CRON-BASED SIGNAL COMPUTATION SCHEDULE

### 4.1 Signal Computation Frequency Tiers

**BLUEPRINT**: 5-tier computation schedule:
- EVERY MESSAGE: S3 (response latency), S6 (initiative balance), damage counter, circuit breaker, kill switch
- EVERY 5th MESSAGE: S5 (goal boundary erosion), S8 (behavioral anomaly)
- SESSION BOUNDARY: S1, S2, S4, S7, full composite, baseline update, de-escalation
- EVERY 5 MINUTES: identity drift, DNS re-resolution, OAuth token expiry, AgentCard cache TTL
- EVERY 15 MINUTES: memory compaction, convergence state file write, ITP batch flush

**CODEBASE**: 
- `crates/convergence-monitor/src/pipeline/signal_computer.rs` → `SignalComputer`
  - Has dirty-flag per signal per agent: `dirty: [bool; 8]`
  - `mark_dirty(agent_id, signal_index)` — marks a signal for recomputation
  - `compute(agent_id)` — only recomputes dirty signals
  - But: the actual signal computation is stubbed (`// In production, each signal would compute from actual data`)
- `crates/cortex/cortex-convergence/src/signals/mod.rs` → `Signal` trait
  - All 8 signals implement `compute(&self, data: &SignalInput) -> f64`
  - No frequency/scheduling metadata on the Signal trait
- `crates/ghost-heartbeat/src/cron.rs` → `CronEngine`
  - Standard cron syntax, timezone-aware, per-job cost tracking
  - `CronJobDef` with `schedule: String` (5-field cron expression)
  - `ready_jobs()` checks kill switch, pause, schedule match
  - But: this is for user-defined cron jobs, NOT for internal signal scheduling

**STATUS**: `PARTIAL`

**GAP**: The dirty-flag mechanism exists in `SignalComputer` but:
- No frequency tier assignment per signal (all signals treated equally)
- No `stale_after` duration per signal (blueprint: each signal gets `last_computed_at` + `stale_after`)
- No message counter to trigger "every 5th message" signals
- No session boundary hook to trigger session-boundary signals
- No 5-minute/15-minute background timer for periodic tasks
- `CronEngine` is for user-facing cron jobs, not internal signal scheduling
- The convergence monitor's event loop (`select!` in `monitor.rs`) doesn't have interval-based signal recomputation
- Signal computation in the monitor is event-driven (on ingest), not schedule-driven

Need a `SignalScheduler` that:
1. Assigns each signal to a frequency tier
2. Tracks `last_computed_at` per signal per agent
3. On each event, only computes signals whose tier matches the current trigger
4. Runs background timers for 5-min and 15-min periodic tasks

**EFFORT**: M (new SignalScheduler module, integration with monitor event loop)

---

### 4.2 Background Periodic Tasks

**BLUEPRINT**: 
- Every 5 min: identity drift detection, egress DNS re-resolution, OAuth token expiry check, AgentCard cache TTL
- Every 15 min: memory compaction eligibility, convergence state file write, ITP event batch flush
- Every 1 hour: key rotation check, Vault token lease renewal, trust score persistence, hash chain Merkle anchoring

**CODEBASE**:
- `crates/ghost-secrets/src/vault_provider.rs` → `renew_token()` exists but is caller's responsibility
- `crates/convergence-monitor/src/state_publisher.rs` → `StatePublisher` writes convergence state to file
- `crates/ghost-oauth/src/broker.rs` → `OAuthBroker` has `execute()` which auto-refreshes expired tokens
- `crates/ghost-mesh/src/trust/eigentrust.rs` → `EigenTrustComputer::compute_global_trust()` — no scheduling
- `crates/cortex/cortex-temporal/` → hash chain and Merkle tree exist but no periodic anchoring scheduler

**STATUS**: `MISSING`

**GAP**: No centralized periodic task scheduler exists. Individual components have the capability (Vault renewal, state publishing, trust computation) but nothing orchestrates them on a schedule. Need:
- `PeriodicTaskScheduler` in ghost-gateway that runs background tokio tasks at configured intervals
- Registration of all periodic tasks with their intervals
- Integration with kill switch (all periodic tasks stop on KILL_ALL)
- Health monitoring of periodic tasks (detect stuck/failed tasks)

**EFFORT**: M (new scheduler module in ghost-gateway, wiring existing capabilities)

---

## 5. CONTEXT COMPACTION STRATEGY (Anthropic's Approach)

### 5.1 Context Window Usage Tracking

**BLUEPRINT**: LLM performance drops sharply after 60-70% of context window is consumed. TokenBudgetAllocator should track cumulative usage and trigger compaction at 60%, not at the limit.

**CODEBASE**: `crates/ghost-agent-loop/src/context/token_budget.rs`
- `TokenBudgetAllocator::allocate()` distributes budget across 10 layers
- `PromptCompiler::apply_truncation()` triggers when total exceeds `context_window`
- No cumulative usage tracking across turns
- No 60% threshold trigger

**STATUS**: `MISSING`

**GAP**: The current system only reacts when the context window is FULL. It does not:
- Track cumulative token usage across turns
- Trigger compaction at 60% threshold
- Distinguish between "approaching limit" and "at limit"
- Have a `ContextUsageTracker` that monitors fill percentage per turn
- Implement progressive compaction (gentle at 60%, aggressive at 80%, emergency at 95%)

**EFFORT**: M (new ContextUsageTracker, integration with PromptCompiler)

---

### 5.2 Conversation Compaction (Turn Summarization)

**BLUEPRINT**: When context approaches 60-70%: (1) observation masking for old tool outputs, (2) summarize turns 1-N into structured state, keep last 3 turns verbatim, (3) maintain running objectives file updated at END of context (high attention zone).

**CODEBASE**: 
- `crates/ghost-gateway/src/session/compaction.rs` — file exists (listed in gateway modules)
- `crates/ghost-agent-loop/src/context/prompt_compiler.rs` — no summarization logic

**STATUS**: `PARTIAL`

**GAP**: Session compaction exists in the gateway but:
- It's for session management (archiving old sessions), not for within-session context compaction
- No turn summarization logic (compress turns 1-N into structured state block)
- No "keep last 3 turns verbatim" logic
- No running objectives file (the `todo.md` trick from Manus)
- L9 (user message) is at the END of context (good — high attention zone) but there's no objectives recitation mechanism

**EFFORT**: L (new within-session compaction pipeline, summarization logic, objectives tracking)

---

### 5.3 MEMORY.md as Progress File

**BLUEPRINT**: MEMORY.md + convergence state file serve as the "progress file" equivalent. Goal proposals in cortex-storage serve as the "feature list." Hash chain in cortex-temporal serves as "git history."

**CODEBASE**:
- `crates/cortex/cortex-storage/` — SQLite persistence for memories, goals, proposals
- `crates/cortex/cortex-temporal/` — hash chain (`HashChain`), Merkle tree (`MerkleTree`), Git anchor, RFC3161
- `crates/convergence-monitor/src/state_publisher.rs` — writes convergence state to file

**STATUS**: `IMPLEMENTED` (the components exist)

**GAP**: The individual components exist but they're not wired as a unified "session resumption" system. When an agent starts with a fresh context window, it should:
1. Load MEMORY.md (already happens via L7)
2. Load convergence state (already happens via L6)
3. Load recent goal proposals (not currently injected into context)
4. Load hash chain summary (not currently injected into context)

The gap is in the LOADING and INJECTION, not in the storage.

**EFFORT**: S (add goal summary and chain summary to L6 or L7 content)

---

## 6. INFORMATION-THEORETIC EXPLORATION BUDGET

### 6.1 Bits-Per-Token Tracking

**BLUEPRINT**: Track `bits_per_token` for each tool call category. Tool calls with high information density get priority in exploration budget. `Information Gain per Token = I(X;Z|Q) / L`.

**CODEBASE**: 
- `crates/ghost-llm/src/cost.rs` — `CostCalculator`, `CostEstimate`, `CostActual` — tracks token counts and dollar costs
- `crates/ghost-llm/src/tokens.rs` — `TokenCounter` — counts tokens in strings
- No information-theoretic metrics

**STATUS**: `MISSING`

**GAP**: The cost tracking system tracks ECONOMIC cost (dollars, token counts) but not INFORMATION cost (bits per token, mutual information). Need:
- `InformationDensityTracker` that measures how much each tool call changes agent behavior
- Per-tool-category `bits_per_token` metric
- Exploration/exploitation ratio tracking (20% exploration, 80% exploitation)
- Diminishing returns detection (when exploration calls yield less new information)

**EFFORT**: L (new information-theoretic module, behavioral change measurement is non-trivial)

---

### 6.2 Memory Deduplication

**BLUEPRINT**: Before writing to MEMORY.md, compute semantic similarity against existing entries. If >0.85 cosine similarity, merge rather than append.

**CODEBASE**:
- `crates/cortex/cortex-retrieval/` — `RetrievalScorer` with `ScorerWeights` — scores memories for retrieval
- `crates/cortex/cortex-validation/src/dimensions/` — contradiction detection (D3) exists
- No semantic similarity computation for deduplication

**STATUS**: `MISSING`

**GAP**: Retrieval scoring exists for READING memories but not for WRITING. No deduplication on write:
- No cosine similarity computation between new entry and existing entries
- No merge logic for similar entries
- No `MemoryDeduplicator` struct
- cortex-validation D3 (contradiction detection) uses heuristic negation patterns, not embedding similarity

**EFFORT**: M (requires embedding computation or TF-IDF similarity, merge logic)

---

### 6.3 Exploration/Exploitation Ratio

**BLUEPRINT**: Allocate 20% of token budget to "exploration" tool calls (gathering new information) and 80% to "exploitation" (acting on known information). Shift budget when exploration yields diminishing returns.

**CODEBASE**: No exploration/exploitation tracking exists anywhere.

**STATUS**: `MISSING`

**GAP**: Complete greenfield. Need:
- Tool call classification: exploration vs exploitation
- Per-session budget allocation
- Diminishing returns detection
- Budget rebalancing logic

**EFFORT**: L (new module, requires tool call classification heuristics)

---

## 7. MESH PROTOCOL TOKEN EFFICIENCY (Tasks 14.x)

### 7.1 AgentCard Caching with Signature-Based Invalidation

**BLUEPRINT**: Cache AgentCards with 1-hour TTL. Add signature-based invalidation: if the card's `signed_at` hasn't changed, skip re-verification.

**CODEBASE**: `crates/ghost-mesh/src/types.rs`
- `AgentCard` struct has `signed_at: DateTime<Utc>` and `signature: Option<Vec<u8>>`
- `sign()` and `verify_signature()` methods exist
- `crates/ghost-mesh/src/traits.rs` → `AgentDiscoverable` trait:
  - `discover_agent(endpoint) -> Result<AgentCard>` — fetches card
  - `get_known_agent(agent_id) -> Option<&AgentCard>` — returns cached card
  - `known_agents() -> Vec<&AgentCard>` — lists all cached

**STATUS**: `PARTIAL`

**GAP**: The trait defines caching semantics (`get_known_agent`) but:
- No TTL implementation (no `cached_at` timestamp on stored cards)
- No signature-based invalidation (no comparison of `signed_at` to skip re-verification)
- No `AgentCardCache` struct with TTL management
- The `AgentDiscoverable` trait is defined but has NO implementation (it's a trait only)
- No A2A client that actually fetches cards over HTTP (Task 14.3 transport layer)

**EFFORT**: M (implement AgentCardCache with TTL, implement A2A client)

---

### 7.2 SSE for Task Status (Not Polling)

**BLUEPRINT**: Use Server-Sent Events for task status updates — zero wasted tokens on "still working" checks.

**CODEBASE**: 
- `crates/ghost-mesh/src/protocol.rs` → `methods::TASKS_SEND_SUBSCRIBE` constant defined
- No SSE implementation anywhere in ghost-mesh
- `crates/ghost-gateway/` uses axum which supports SSE via `axum::response::sse`

**STATUS**: `STUBBED`

**GAP**: The JSON-RPC method name for SSE subscription is defined (`tasks/sendSubscribe`) but:
- No SSE transport implementation
- No `A2AServer` struct that serves SSE endpoints
- No `A2AClient` struct that consumes SSE streams
- The gateway has axum (which supports SSE) but no mesh routes exist yet
- Task 14.3 (A2A transport) is the prerequisite — currently only types and protocol constants exist

**EFFORT**: L (full A2A transport implementation needed — Task 14.3)

---

### 7.3 Delta-Encoded Task Updates

**BLUEPRINT**: Only send changed fields in MeshTask status updates.

**CODEBASE**: `crates/ghost-mesh/src/types.rs`
- `MeshTask` struct has `status`, `output`, `updated_at` fields
- `transition()` method updates status
- No delta encoding

**STATUS**: `MISSING`

**GAP**: MeshTask is always sent as a full struct. Need:
- `MeshTaskDelta` struct with `Option` fields for changed-only data
- Delta computation: compare current vs previous state, emit only changes
- Delta application: merge delta into existing task state

**EFFORT**: S (straightforward Option-field struct + diff logic)

---

### 7.4 EigenTrust Computation Batching

**BLUEPRINT**: Don't recompute on every interaction. Batch interactions and run power iteration every 5 minutes (converges in <20 iterations for small networks).

**CODEBASE**: `crates/ghost-mesh/src/trust/eigentrust.rs`
- `EigenTrustComputer::compute_global_trust()` — full power iteration
- `EigenTrustConfig::max_iterations` = 20, `convergence_threshold` = 1e-6
- `crates/ghost-mesh/src/trust/local_trust.rs` → `LocalTrustStore`
  - `is_dirty()` — dirty flag set when interactions change
  - `clear_dirty()` — cleared after recompute

**STATUS**: `PARTIAL`

**GAP**: The dirty flag exists on `LocalTrustStore` but:
- No batching scheduler (no "every 5 minutes" timer)
- `compute_global_trust()` is called on-demand, not on a schedule
- No integration with the periodic task scheduler (which itself doesn't exist — see 4.2)
- The dirty flag is the right primitive — just needs a scheduler to check it periodically

**EFFORT**: S (wire into periodic task scheduler once that exists)

---

### 7.5 Capability Bitfield for Fast Matching

**BLUEPRINT**: Send capability requirements as a compact bitfield, not natural language. `AgentCard.capabilities: Vec<String>` should have a parallel `capability_flags: u64` for fast matching.

**CODEBASE**: `crates/ghost-mesh/src/types.rs`
```rust
pub struct AgentCard {
    // ...
    pub capabilities: Vec<String>,
    // No capability_flags field
}
```

**STATUS**: `MISSING`

**GAP**: Capabilities are stored as `Vec<String>` only. Need:
- `capability_flags: u64` field on `AgentCard`
- Capability-to-bit mapping (e.g., bit 0 = "code_execution", bit 1 = "web_search", etc.)
- Fast bitwise AND matching for delegation requests
- Backward compatibility: keep `Vec<String>` for human readability, use `u64` for matching

**EFFORT**: S (add field, define bit mapping, add matching logic)

---

## 8. SKILL EVOLUTION PATTERN (Compounding Savings)

### 8.1 Skill Persistence from Successful Workflows

**BLUEPRINT**: After a successful multi-tool-call sequence, propose persisting it as a skill. On subsequent similar requests, load the skill instead of re-discovering the tool chain. Track `tokens_saved_by_skills` metric.

**CODEBASE**: `crates/ghost-skills/src/registry.rs`
- `SkillRegistry` with `register()`, `lookup()`, `loaded_skills()`, `quarantined_skills()`
- `SkillManifest` with `name`, `version`, `description`, `capabilities`, `timeout_seconds`, `signature`
- `SkillSource` enum: `Bundled`, `User`, `Workspace`
- `SkillState` enum: `Loaded`, `Quarantined`
- `crates/ghost-skills/src/credential/broker.rs` → `CredentialBroker` for opaque token handling
- `crates/ghost-skills/src/sandbox/` → `NativeSandbox`, `WasmSandboxConfig`

**STATUS**: `PARTIAL`

**GAP**: The skill registry exists for LOADING and EXECUTING skills, but NOT for CREATING skills from successful workflows:
- No `SkillProposer` that analyzes successful tool call sequences and proposes new skills
- No `tokens_saved_by_skills` metric tracking
- No automatic skill creation pipeline (detect pattern → propose skill → human approval → persist)
- No similarity matching between incoming requests and existing skills
- `SkillManifest` doesn't capture the tool call sequence (it's a YAML manifest, not a workflow recording)
- No `WorkflowRecorder` that captures tool call sequences for later replay

The skill system is designed for pre-authored skills (YAML manifests with WASM sandboxes), not for emergent skill creation from agent behavior.

**EFFORT**: L (new SkillProposer, WorkflowRecorder, similarity matching, approval pipeline)

---

## 9. EXISTING INFRASTRUCTURE — FULLY IMPLEMENTED (Verification)

This section confirms components that ARE fully implemented and match the blueprint.

### 9.1 Convergence-Aware Filtering ✅

**CODEBASE**: `crates/cortex/cortex-convergence/src/filtering/convergence_aware_filter.rs`
- 4-tier filtering: [0.0,0.3) full access → [0.3,0.5) reduced emotional → [0.5,0.7) task-focused → [0.7,1.0] minimal
- Filters by `MemoryType` enum variants

**STATUS**: `IMPLEMENTED` — matches blueprint requirement for convergence-aware memory filtering.

---

### 9.2 8-Signal Convergence System ✅

**CODEBASE**: `crates/cortex/cortex-convergence/src/signals/`
- S1: `session_duration.rs` — session length normalization
- S2: `inter_session_gap.rs` — gap between sessions
- S3: `response_latency.rs` — response time patterns
- S4: `vocabulary_convergence.rs` — TF-IDF cosine similarity
- S5: `goal_boundary_erosion.rs` — scope creep detection
- S6: `initiative_balance.rs` — human vs agent initiative ratio
- S7: `disengagement_resistance.rs` — exit signal handling
- S8: `behavioral_anomaly.rs` — KL divergence on tool call distribution

All implement `Signal` trait with `id()`, `name()`, `compute()`, `requires_privacy_level()`.

**STATUS**: `IMPLEMENTED` — all 8 signals exist with full computation logic.

---

### 9.3 Composite Scoring with Profiles ✅

**CODEBASE**: `crates/cortex/cortex-convergence/src/scoring/`
- `composite.rs` → `CompositeScorer` with 8-signal weighted scoring
- `profiles.rs` → 4 named profiles (Standard, Research, Companion, Productivity) — all include S8
- `baseline.rs` → `BaselineState` with percentile ranking, calibration period
- Meso amplification (1.1x), macro amplification (1.15x), critical override (force L2)

**STATUS**: `IMPLEMENTED`

---

### 9.4 5-Level Intervention State Machine ✅

**CODEBASE**: `crates/convergence-monitor/src/intervention/trigger.rs`
- `InterventionStateMachine` with per-agent `AgentInterventionState`
- Levels 0-4 with escalation (max +1 per cycle), hysteresis (2 consecutive cycles), de-escalation credits
- Cooldowns: L2→5min, L3→4h, L4→24h
- Mandatory ack at L2, session termination at L3, external escalation at L4

**STATUS**: `IMPLEMENTED`

---

### 9.5 Dirty-Flag Signal Throttling ✅

**CODEBASE**: `crates/convergence-monitor/src/pipeline/signal_computer.rs`
- `SignalComputer` with per-agent, per-signal dirty flags
- `mark_dirty()` / `compute()` — only recomputes dirty signals
- 8-signal array with `[bool; 8]` dirty tracking

**STATUS**: `IMPLEMENTED` (mechanism exists, but scheduling tiers are missing — see 4.1)

---

### 9.6 Spotlighting (Datamarking) ✅

**CODEBASE**: `crates/ghost-agent-loop/src/context/spotlighting.rs`
- `Spotlighter` with `SpotlightingConfig` (enabled, marker char, layers, mode)
- `SpotlightMode`: Datamarking, Delimiting, Off
- `datamark()` / `undatamark()` — character interleaving with marker escape
- Applied to L7/L8 only, never L0/L1/L9
- Token budget multiplier (datamarking ~doubles token count)
- System instruction injection into L1
- Comprehensive tests: round-trip, Unicode, RTL, large strings, marker escape

**STATUS**: `IMPLEMENTED`

---

### 9.7 Plan-Then-Execute Validation ✅

**CODEBASE**: `crates/ghost-agent-loop/src/tools/plan_validator.rs`
- `PlanValidator` with 4 rules: DangerousSequence, Escalation, Volume, SensitiveDataFlow
- `ToolCallPlan` struct wrapping ordered `Vec<LLMToolCall>`
- `PlanValidationResult`: Permit, Deny(reason), RequireApproval(reason)
- Domain extraction from URLs, tool similarity detection
- Configurable: `max_tool_calls_per_plan` (default 10), `allowed_domains`, `sensitive_read_tools`, `external_send_tools`
- Denial tracking for escalation detection

**STATUS**: `IMPLEMENTED`

---

### 9.8 Quarantined LLM ✅

**CODEBASE**: `crates/ghost-llm/src/quarantine.rs`
- `QuarantinedLLM`: empty tool list, extraction system prompt, max output tokens, Free/Cheap tier
- `ContentQuarantine`: `should_quarantine()`, `quarantine_content()`, `is_enabled()`
- `QuarantineConfig`: enabled (default false), max_output_tokens, model_tier, content_types

**STATUS**: `IMPLEMENTED` (as defensive mechanism; gap is using it as general compressor — see 2.1)

---

### 9.9 Secrets Infrastructure ✅

**CODEBASE**: `crates/ghost-secrets/`
- `SecretProvider` trait: `get_secret()`, `set_secret()`, `delete_secret()`, `has_secret()`
- `EnvProvider`: reads env vars, read-only for set/delete
- `KeychainProvider` (feature `keychain`): OS keychain via `keyring` crate
- `VaultProvider` (feature `vault`): Vault KV v2 HTTP API with token renewal
- `ProviderConfig` enum: Env, Keychain, Vault
- All values as `SecretString` (zeroized on drop)
- Zero ghost-*/cortex-* dependencies (leaf crate)

**STATUS**: `IMPLEMENTED`

---

### 9.10 Network Egress Control ✅

**CODEBASE**: `crates/ghost-egress/`
- `EgressPolicy` trait: `apply()`, `check_domain()`, `remove()`, `log_violation()`
- `ProxyEgressPolicy`: per-agent localhost proxy with domain filtering, violation counting
- `EbpfEgressPolicy` (feature `ebpf`, Linux): eBPF cgroup filter stub
- `PfEgressPolicy` (feature `pf`, macOS): pf packet filter stub
- `DomainMatcher`: wildcard patterns, case-insensitive, port/path stripping, Unicode rejection
- `AgentEgressConfig`: policy mode, allowed/blocked domains, violation threshold
- `TriggerEvent::NetworkEgressViolation` in cortex-core

**STATUS**: `IMPLEMENTED`

---

### 9.11 OAuth Brokering ✅

**CODEBASE**: `crates/ghost-oauth/`
- `OAuthBroker`: connect, callback, execute, disconnect, revoke_all, list_connections
- `OAuthProvider` trait: authorization_url, exchange_code, refresh_token, revoke_token, execute_api_call
- `TokenStore`: encrypted storage with vault key, atomic writes
- 4 providers: Google, GitHub, Slack, Microsoft (each with provider-specific quirks)
- `OAuthRefId`: opaque UUID reference (agent never sees raw tokens)
- `PkceChallenge`: SHA-256 code challenge generation
- Kill switch integration: `revoke_all()` revokes all connections

**STATUS**: `IMPLEMENTED`

---

### 9.12 Ghost-Mesh Core Types + Safety ✅

**CODEBASE**: `crates/ghost-mesh/`
- `AgentCard`: signed with Ed25519, canonical_bytes, verify_signature
- `TaskStatus`: state machine with valid transitions, terminal states
- `MeshTask`: lifecycle management with transition validation
- `MeshMessage`: JSON-RPC 2.0 envelope
- `EigenTrustComputer`: power iteration with pre-trusted anchoring
- `LocalTrustStore`: interaction recording, normalized trust matrix
- `CascadeCircuitBreaker`: per-agent-pair, convergence spike detection, depth tracking
- `MemoryPoisoningDetector`: volume spike, contradiction, untrusted high-importance detection
- `DelegationDepthTracker`: depth enforcement, loop detection

**STATUS**: `IMPLEMENTED` (types and safety mechanisms; transport layer is missing — Task 14.3)

---

### 9.13 AuthProfileManager with SecretProvider ✅

**CODEBASE**: `crates/ghost-llm/src/auth.rs`
- `AuthProfileManager` accepts `Box<dyn SecretProvider>`
- Credential rotation on 401/429 (advance profile index)
- Legacy env var fallback (`ANTHROPIC_API_KEY` format)
- `SecretString` retrieved just-in-time, never stored long-lived
- Never logged via tracing (debug messages say "value redacted")

**STATUS**: `IMPLEMENTED`

---

### 9.14 Gateway Bootstrap with Secrets + Egress ✅

**CODEBASE**: `crates/ghost-gateway/src/bootstrap.rs`
- Step 1b: `build_secrets()` constructs SecretProvider from config
- Step 4b: `step4b_apply_egress_policies()` applies per-agent egress
- `crates/ghost-gateway/src/config.rs`:
  - `SecretsConfig` with provider selection (env/keychain/vault)
  - `NetworkEgressGatewayConfig` with per-agent egress settings
  - `build_secret_provider()` and `build_egress_config()` factory functions

**STATUS**: `IMPLEMENTED`

---

## 10. TRANSPORT LAYER GAPS (Task 14.3 — Critical Path)

### 10.1 A2A Client

**BLUEPRINT**: `A2AClient` with `discover_agent()`, `submit_task()`, `get_task_status()`, `cancel_task()`, SSE streaming.

**CODEBASE**: 
- `crates/ghost-mesh/src/traits.rs` → `AgentDiscoverable` and `TaskDelegator` traits defined
- No implementation of these traits exists
- No HTTP client code in ghost-mesh
- ghost-mesh Cargo.toml has NO `reqwest` dependency (only serde, uuid, chrono, thiserror, async-trait, tracing, blake3, ghost-signing)

**STATUS**: `MISSING`

**GAP**: The traits are defined but there is zero transport implementation. Need:
- `A2AClient` struct implementing `AgentDiscoverable` + `TaskDelegator`
- `reqwest` dependency added to ghost-mesh
- HTTP GET for `/.well-known/agent.json` (discovery)
- JSON-RPC 2.0 POST for task operations
- SSE client for `tasks/sendSubscribe`
- Ed25519 request signing

**EFFORT**: L

---

### 10.2 A2A Server (Gateway Routes)

**BLUEPRINT**: Gateway serves `GET /.well-known/agent.json` and `POST /a2a` (JSON-RPC dispatcher).

**CODEBASE**:
- `crates/ghost-gateway/src/api/` — has `agents.rs`, `convergence.rs`, `goals.rs`, `health.rs`, `oauth_routes.rs`, `safety.rs`, `sessions.rs`, `websocket.rs`
- `oauth_routes.rs` exists (Task 13.5)
- No `mesh_routes.rs` file
- Gateway Cargo.toml does NOT depend on `ghost-mesh`

**STATUS**: `MISSING`

**GAP**: No mesh routes in the gateway. Need:
- `mesh_routes.rs` in ghost-gateway/src/api/
- `ghost-mesh` dependency added to ghost-gateway Cargo.toml
- `GET /.well-known/agent.json` endpoint
- `POST /a2a` JSON-RPC 2.0 dispatcher
- Auth middleware for Ed25519 signature verification
- SSE endpoint for task subscriptions

**EFFORT**: L

---

### 10.3 Agent Discovery Registry

**BLUEPRINT**: Local registry from ghost.yml mesh config + remote discovery with AgentCard caching (1-hour TTL).

**CODEBASE**:
- `crates/ghost-gateway/src/config.rs` — no mesh config section
- No `AgentDiscoveryRegistry` struct anywhere

**STATUS**: `MISSING`

**GAP**: No discovery registry. Need:
- `mesh` section in ghost.yml config
- `AgentDiscoveryRegistry` struct with local + remote discovery
- TTL-based AgentCard cache
- Signature verification on discovered cards

**EFFORT**: M

---

## 11. CROSS-CUTTING CONCERNS

### 11.1 Test Fixtures for Post-v1 Types

**BLUEPRINT** (Task 15.2): New proptest strategies for egress_config, domain_pattern, oauth_ref_id, token_set, agent_card, mesh_task, interaction_outcome, trust_matrix, tool_call_plan, signal_array_8, spotlighting_config.

**CODEBASE**: `crates/cortex/test-fixtures/src/strategies.rs`
- Existing strategies: memory_type, importance, convergence_score, signal_array (8-element ✅), uuid, datetime, event_chain, convergence_trajectory, proposal, caller_type, proposal_operation, trigger_event, base_memory, session_history, kill_state, gateway_state_transition
- `signal_array_strategy()` already produces `[f64; 8]` arrays ✅
- `trigger_event_strategy()` — need to verify it includes `NetworkEgressViolation`

**STATUS**: `PARTIAL`

**GAP**: Missing strategies for:
- `egress_config_strategy()` → random `AgentEgressConfig`
- `domain_pattern_strategy()` → random domain strings and wildcards
- `oauth_ref_id_strategy()` → random `OAuthRefId`
- `token_set_strategy()` → random `TokenSet`
- `agent_card_strategy()` → random `AgentCard` with valid signature
- `mesh_task_strategy()` → random `MeshTask` with valid status
- `interaction_outcome_strategy()` → random `InteractionOutcome`
- `trust_matrix_strategy()` → random local trust values
- `tool_call_plan_strategy()` → random `ToolCallPlan`
- `spotlighting_config_strategy()` → random `SpotlightingConfig`

Note: test-fixtures Cargo.toml does NOT depend on ghost-egress, ghost-oauth, or ghost-mesh — these dependencies would need to be added.

**EFFORT**: M (add dependencies, write ~10 new strategies)

---

### 11.2 Integration Tests (Tasks 15.3, 15.4)

**BLUEPRINT**: Workspace-level integration tests for secrets E2E, egress E2E, OAuth E2E, mesh E2E.

**CODEBASE**: 
- `agent-ghost/tests/adversarial/convergence_manipulation.rs` — workspace-level adversarial test exists
- No `tests/integration/` directory
- Individual crate tests exist (e.g., `ghost-egress/tests/egress_tests.rs`, `ghost-mesh/tests/safety_tests.rs`)

**STATUS**: `MISSING`

**GAP**: No cross-crate integration tests. Need:
- `tests/integration/secrets_e2e.rs`
- `tests/integration/egress_e2e.rs`
- `tests/integration/oauth_e2e.rs`
- `tests/integration/mesh_e2e.rs`

**EFFORT**: L (requires all component crates to be fully wired)

---

### 11.3 PWA Support (Task 15.1)

**BLUEPRINT**: Progressive Web App for SvelteKit dashboard with push notifications.

**CODEBASE**: No `dashboard/` directory visible in workspace (may exist but not in file tree).

**STATUS**: `MISSING` (or out of scope for this audit — frontend)

**EFFORT**: L (full PWA implementation)

---

### 11.4 Documentation (Task 15.5)

**BLUEPRINT**: 5 new docs + architecture update.

**CODEBASE**: 
- `agent-ghost/AGENT_ARCHITECTURE.md` and `AGENT_ARCHITECTURE_v2.md` exist
- `agent-ghost/AGENT_LOOP_SEQUENCE_FLOW.md` exists
- `agent-ghost/CONVERGENCE_MONITOR_SEQUENCE_FLOW.md` exists
- No `docs/` directory with the 5 new documents

**STATUS**: `MISSING`

**EFFORT**: M (documentation writing)

---

## 12. SUMMARY MATRIX

| # | Item | Status | Effort | Priority |
|---|------|--------|--------|----------|
| 1.1 | Tiered heartbeat (4 tiers) | PARTIAL | L | Medium |
| 1.2 | Heartbeat frequency hysteresis | PARTIAL | M | Medium |
| 1.3 | Monitor heartbeat integration | IMPLEMENTED | — | — |
| 2.1 | ContentQuarantine as compressor | PARTIAL | L | High |
| 2.2 | Observation masking | MISSING | M | High |
| 2.3 | L7 memory compression | MISSING | M | Medium |
| 3.1 | Stable prefix preservation | PARTIAL | M | High |
| 3.2 | Deterministic serialization | IMPLEMENTED | — | — |
| 3.3 | Append-only context | IMPLEMENTED | — | — |
| 3.4 | No dynamic timestamps | WIRING_NEEDED | S | High |
| 4.1 | Signal computation frequency tiers | PARTIAL | M | Medium |
| 4.2 | Background periodic tasks | MISSING | M | Medium |
| 5.1 | Context window usage tracking | MISSING | M | High |
| 5.2 | Conversation compaction | PARTIAL | L | High |
| 5.3 | MEMORY.md as progress file | IMPLEMENTED | — | — |
| 6.1 | Bits-per-token tracking | MISSING | L | Low |
| 6.2 | Memory deduplication | MISSING | M | Medium |
| 6.3 | Exploration/exploitation ratio | MISSING | L | Low |
| 7.1 | AgentCard caching + invalidation | PARTIAL | M | Medium |
| 7.2 | SSE for task status | STUBBED | L | Medium |
| 7.3 | Delta-encoded task updates | MISSING | S | Low |
| 7.4 | EigenTrust computation batching | PARTIAL | S | Medium |
| 7.5 | Capability bitfield | MISSING | S | Low |
| 8.1 | Skill persistence from workflows | PARTIAL | L | Medium |
| 9.1–9.14 | Existing infrastructure | IMPLEMENTED | — | — |
| 10.1 | A2A Client | MISSING | L | High |
| 10.2 | A2A Server (gateway routes) | MISSING | L | High |
| 10.3 | Agent discovery registry | MISSING | M | High |
| 11.1 | Test fixtures for post-v1 types | PARTIAL | M | Medium |
| 11.2 | Integration tests | MISSING | L | Medium |
| 11.3 | PWA support | MISSING | L | Low |
| 11.4 | Documentation | MISSING | M | Low |

---

## 13. CRITICAL PATH ANALYSIS

The blueprint recommends this implementation order:

```
1. KV cache optimization (free, just restructure prompts)     → Items 3.1, 3.3, 3.4
2. Observation masking (simple, high impact)                   → Item 2.2
3. Compressor integration (medium effort, highest impact)      → Items 2.1, 2.3
4. Cron scheduler (already partially built)                    → Items 4.1, 4.2
5. Skill evolution (long-term compounding)                     → Item 8.1
```

Mapped to codebase changes:

### Step 1: KV Cache (Effort: M, Impact: 90% cost reduction on ~7K tokens/turn)
- `ghost-agent-loop/src/context/prompt_compiler.rs`: Move tool filtering from L3 content removal to L6 constraint instruction
- `ghost-agent-loop/src/context/prompt_compiler.rs`: Add stable prefix hash validation
- `ghost-agent-loop/src/context/prompt_compiler.rs`: Ensure L4 uses date-only granularity
- `ghost-agent-loop/src/context/spotlighting.rs`: Make L1 spotlighting instruction a one-time template, not per-turn injection

### Step 2: Observation Masking (Effort: M, Impact: 50% reduction on L8)
- NEW: `ghost-agent-loop/src/context/observation_masker.rs`
- NEW: `.ghost/cache/tool_outputs/` caching mechanism
- MODIFY: `ghost-agent-loop/src/context/prompt_compiler.rs` — add pre-processing step before L8 assembly

### Step 3: Compressor Integration (Effort: L, Impact: 87-98% on raw tool output)
- MODIFY: `ghost-llm/src/quarantine.rs` — add `Local` model tier, enable by default for tool outputs
- MODIFY: `ghost-llm/src/router.rs` — add local model provider support
- NEW: `ghost-agent-loop/src/context/memory_compressor.rs` — post-filter compression for L7
- MODIFY: `ghost-agent-loop/src/context/prompt_compiler.rs` — integrate compression pipeline

### Step 4: Cron Scheduler (Effort: M, Impact: ~80% reduction in signal compute)
- NEW: `convergence-monitor/src/pipeline/signal_scheduler.rs` — frequency tier assignment per signal
- MODIFY: `convergence-monitor/src/monitor.rs` — add interval-based timers to event loop
- NEW: `ghost-gateway/src/periodic.rs` — centralized periodic task scheduler

### Step 5: Skill Evolution (Effort: L, Impact: 67% per repeated task, compounding)
- NEW: `ghost-skills/src/proposer.rs` — analyze successful tool sequences, propose skills
- NEW: `ghost-skills/src/recorder.rs` — capture tool call sequences for replay
- MODIFY: `ghost-skills/src/registry.rs` — add workflow-based skill creation
- NEW: `ghost-agent-loop/src/tools/skill_matcher.rs` — match incoming requests to existing skills

---

## 14. TOKEN SAVINGS PROJECTION

Based on the blueprint's estimates, mapped to actual codebase token flows:

| Technique | Current Tokens/Turn | After Optimization | Reduction |
|-----------|--------------------|--------------------|-----------|
| KV cache stable prefix (L0-L6) | ~6,900 (recomputed) | ~690 (cached) | 90% |
| Observation masking (L8 old tool outputs) | ~8,000 (raw) | ~4,000 (references) | 50% |
| Compressor for tool results | ~50,000 (raw output) | ~2,000 (extraction) | 96% |
| Heartbeat frequency reduction | ~2,000/beat × 48/day | ~200/beat × 12/day | 95% |
| Signal computation batching | 8 signals/event | 2-3 signals/event avg | 65% |
| Skill reuse (compounding) | ~12,000 first time | ~4,000 subsequent | 67% |

For a typical 50-tool-call autonomous session:
- Current estimate: ~500K tokens
- After all optimizations: ~50-80K tokens
- Reduction factor: 6-10x

---

## 15. DEPENDENCY GAPS FOR WIRING

These are the missing Cargo.toml dependencies that would be needed to wire everything together:

| Crate | Missing Dependency | Reason |
|-------|-------------------|--------|
| ghost-gateway | ghost-mesh | Mesh routes (Task 14.3) |
| ghost-mesh | reqwest | A2A HTTP client |
| ghost-mesh | tokio (full) | SSE streaming, background tasks |
| cortex-test-fixtures | ghost-egress | Egress config strategies |
| cortex-test-fixtures | ghost-oauth | OAuth type strategies |
| cortex-test-fixtures | ghost-mesh | Mesh type strategies |
| cortex-test-fixtures | ghost-agent-loop | Tool call plan strategies |
| ghost-heartbeat | tokio (net) | Binary UDP heartbeat transport |

---

*End of audit. Every item from the research blueprint has been accounted for against the actual codebase state.*
