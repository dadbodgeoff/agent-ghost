# ghost-agent-loop

> The core agent runner — recursive LLM loop with 6-gate safety checks, 10-layer prompt compilation, plan-then-execute tool validation, proposal extraction/routing, credential exfiltration detection, and convergence-aware tool filtering.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 7 (Agent Core) |
| Type | Library |
| Location | `crates/ghost-agent-loop/` |
| Workspace deps | `cortex-core`, `ghost-llm`, `ghost-identity`, `ghost-policy`, `read-only-pipeline`, `simulation-boundary`, `itp-protocol`, `cortex-validation`, `ghost-kill-gates` |
| External deps | `tokio`, `reqwest`, `regex`, `once_cell`, `blake3`, `async-trait`, `serde`, `chrono`, `uuid`, `thiserror`, `tracing` |
| Modules | `runner`, `circuit_breaker`, `damage_counter`, `output_inspector`, `itp_emitter`, `response`, `context/` (12 submodules), `proposal/` (extractor + router), `tools/` (registry + executor + plan validator + skill matcher + 6 builtins) |
| Gate order | GATE 0: circuit breaker → GATE 1: recursion depth → GATE 1.5: damage counter → GATE 2: spending cap → GATE 3: kill switch → GATE 3.5: distributed kill gate |
| Prompt layers | L0–L9 (10 layers, budget-allocated, spotlighted, observation-masked) |
| Builtin tools | `read_file`, `write_file`, `list_dir`, `shell`, `memory_read`, `web_search`, `web_fetch`, `http_request` |
| Test coverage | Gate order verification, credential exfiltration patterns, observation masking, compressor pipeline, agent loop lifecycle |
| Downstream consumers | `ghost-gateway` (creates and runs AgentRunner), `ghost-heartbeat` (fires heartbeat turns) |

---

## Why This Crate Exists

This is the brain of GHOST. Every user message, every heartbeat, every cron job flows through `ghost-agent-loop`. It orchestrates the full cycle: receive message → compile prompt → call LLM → process response → execute tools → loop back or return.

But it's not just a loop. It's a loop wrapped in six layers of safety gates, a 10-layer prompt compiler, a plan validator that detects exfiltration chains, an output inspector that catches credential leaks, and a proposal router that manages the agent's ability to modify its own state.

The crate exists as a separate library (not embedded in the gateway) because:
- `ghost-heartbeat` needs to call `AgentRunner::pre_loop()` and `run_turn()` directly
- Testing the agent loop in isolation (without the gateway's HTTP server, channel adapters, etc.) is critical for safety verification
- The loop's invariants (gate order, snapshot immutability, spending cap) must be testable without integration dependencies

---

## Module Breakdown

### `runner.rs` — The Core Loop

The `AgentRunner` is the central struct. It owns all the components needed for a single agent turn.

#### The 6-Gate Safety Pipeline

Every iteration of the recursive loop checks all gates in EXACT order. This order is a HARD INVARIANT — changing it is a bug.

```
GATE 0: Circuit Breaker    — 3 consecutive LLM failures → open (no calls)
GATE 1: Recursion Depth    — max 10 tool-call round-trips per turn
GATE 1.5: Damage Counter   — 5 destructive tool calls → halt (monotonic, never resets)
GATE 2: Spending Cap       — daily spend limit ($10 default), NaN/Inf guard
GATE 3: Kill Switch        — AtomicBool, SeqCst ordering, checked every iteration
GATE 3.5: Kill Gate        — distributed kill gate (optional, for multi-node)
```

**Why this order?** Fast-fail optimization. The circuit breaker (GATE 0) is the cheapest check — a single boolean. If the LLM is down, there's no point checking recursion depth or spending. The kill switch (GATE 3) is last because it's the most expensive (atomic load with SeqCst ordering) and the least likely to change between iterations.

**NaN guard on spending cap:** `total_spend.is_nan() || total_spend.is_infinite()` catches corrupted cost values. Without this, `NaN > cap` evaluates to `false`, silently bypassing the spending cap.

#### The 11-Step Pre-Loop Orchestrator

Before the recursive loop starts, `pre_loop()` executes 11 steps in order:

| Step | Name | Type | What It Does |
|------|------|------|-------------|
| 1 | Channel normalization | Setup | Lowercase channel identifier |
| 2 | Agent binding | Setup | Resolve which agent handles this channel |
| 3 | Session resolution | Setup | Resume or create session |
| 4 | Lane queue | Setup | Acquire session lock (held for entire run) |
| 5 | Kill switch | BLOCKING GATE | Halt if platform killed |
| 6 | Spending cap | BLOCKING GATE | Halt if daily spend exceeded |
| 7 | Cooldown | BLOCKING GATE | Halt if agent in cooldown (L3: 4h, L4: 24h) |
| 8 | Session boundary | BLOCKING GATE | Enforce min inter-session gap at L3+ |
| 9 | Snapshot assembly | Setup | Build immutable AgentSnapshot |
| 10 | RunContext | Setup | Construct per-run context |
| 11 | ITP emission | Setup | Emit SessionStart + InteractionMessage events |

Steps 5–8 are blocking gates — failure halts before the loop starts. Step 9 is the most complex: it assembles the snapshot from multiple data sources with sensible defaults when convergence data is unavailable.

**INV-PRE-06:** The snapshot assembled in step 9 is immutable for the entire run. The same convergence score, intervention level, and filtered memories are used from the first LLM call to the last. This prevents mid-run convergence changes from causing inconsistent behavior.

#### The Recursive Loop

```
loop {
    check_gates() → compile_prompt() → call_LLM() → process_response()
    ├── Text → inspect for credentials → extract proposals → return
    ├── ToolCalls → validate plan → execute tools → increment depth → continue
    ├── Mixed → inspect text + execute tools → continue
    └── Empty → return
}
```

**Recursion depth increments per tool-call round-trip**, not per individual tool call. If the LLM returns 3 tool calls in one response, that's one recursion increment. This allows multi-tool responses without hitting the depth limit prematurely.

---

### `circuit_breaker.rs` — 3-State Failure Protection

```
Closed → (3 failures) → Open → (60s cooldown) → HalfOpen → (1 probe)
                                                    ├── success → Closed
                                                    └── failure → Open
```

**Key design decisions:**

1. **Independent from ghost-llm's provider circuit breaker.** The agent loop circuit breaker tracks tool-level failures. The LLM provider circuit breaker (in `ghost-llm`) tracks API-level failures. They're separate because a tool failure (timeout, invalid output) doesn't mean the LLM API is down.

2. **Policy denials do NOT increment the counter (AC6).** A policy denial is the system working correctly — it's not a failure. Counting denials as failures would trip the circuit breaker when the agent is being properly constrained.

3. **Default: 3 failures, 60s cooldown.** Three consecutive failures strongly suggest a systemic issue (LLM down, network partition). 60 seconds is enough for transient issues to resolve.

---

### `damage_counter.rs` — Monotonic Destructive Action Tracker

```rust
pub struct DamageCounter {
    count: u32,      // Never decrements
    threshold: u32,  // Default: 5
}
```

The damage counter tracks destructive tool calls (`write_file`, `shell`). It's monotonically non-decreasing within a run — it never resets. At threshold (5), the run halts.

**Why separate from the circuit breaker?** Different semantics. The circuit breaker tracks failures (things going wrong). The damage counter tracks mutations (things being changed). An agent could successfully write 5 files — no failures, but the damage counter halts the run because 5 mutations in one turn is suspicious.

---

### `output_inspector.rs` — Credential Exfiltration Detection

Every LLM response is scanned for credential patterns BEFORE delivery to the user.

#### Six Credential Patterns

| Pattern | Regex | Example |
|---------|-------|---------|
| OpenAI API key | `sk-[a-zA-Z0-9\-]{20,}` | `sk-abc123...` |
| AWS access key | `AKIA[0-9A-Z]{16}` | `AKIAIOSFODNN7EXAMPLE` |
| GitHub token | `ghp_[a-zA-Z0-9]{36}` | `ghp_xxxxxxxxxxxx...` |
| PEM private key | `-----BEGIN.*PRIVATE KEY-----` | PEM header |
| GitHub fine-grained | `github_pat_[a-zA-Z0-9_]{22,}` | `github_pat_xxx...` |
| Anthropic API key | `sk-ant-[a-zA-Z0-9-]{20,}` | `sk-ant-xxx...` |

#### Two-Tier Response

1. **Pattern match only → Warning + Redact.** The matched text is replaced with `[REDACTED]`. The response is still delivered, but the credential is removed.

2. **Pattern match + in credential store → KILL ALL.** If the matched pattern corresponds to a known credential (registered via `register_credential()`), the kill switch is activated immediately. This is the nuclear option — the agent is compromised and must be stopped.

---

### `tools/plan_validator.rs` — Sequence-Level Safety

The plan validator runs AFTER individual policy checks but BEFORE execution. It validates tool call SEQUENCES, not individual calls.

#### Four Validation Rules

| Rule | What It Detects | Example |
|------|----------------|---------|
| Volume | Too many tool calls in one plan (>10) | LLM returns 15 tool calls |
| Dangerous sequence | Read sensitive data → send externally | `file_read` → `api_call(evil.com)` |
| Sensitive data flow | Read credentials → any external tool | `file_read(~/.ssh/id_rsa)` → `web_fetch` |
| Escalation | Probing for alternatives to denied tools | Denied `shell_exec` 3x → tries `shell_execute` |

**Domain allowlist:** The dangerous sequence rule allows sends to known-safe domains (OpenAI, Anthropic, Google, Mistral, Groq). Sends to any other domain after a sensitive read are denied.

**Tool similarity detection:** The escalation rule uses a prefix/suffix matching heuristic (`tools_are_similar()`) to detect when the agent is trying alternative tool names after denials. If `shell_exec` was denied 3 times and the agent tries `shell_execute` (shares 5-char prefix), it's flagged as escalation.

---

### `tools/registry.rs` — Convergence-Aware Tool Filtering

```rust
pub fn schemas_filtered(&self, intervention_level: u8) -> Vec<ToolSchema> {
    self.tools.values()
        .filter(|t| t.hidden_at_level > intervention_level)
        .map(|t| t.schema.clone())
        .collect()
}
```

Each tool has a `hidden_at_level` threshold. At higher convergence levels, more tools are hidden from the LLM. The agent literally cannot see tools that are inappropriate for its current state.

| Tool | hidden_at_level | Visible at L0-L2 | Visible at L3 | Visible at L4 |
|------|----------------|-------------------|----------------|----------------|
| `read_file` | 5 | ✅ | ✅ | ✅ |
| `write_file` | 4 | ✅ | ✅ | ❌ |
| `shell` | 4 | ✅ | ✅ | ❌ |
| `web_search` | 3 | ✅ | ❌ | ❌ |
| `web_fetch` | 3 | ✅ | ❌ | ❌ |

---

### `context/prompt_compiler.rs` — 10-Layer Prompt Assembly

The prompt compiler assembles the full system prompt from 10 layers with budget allocation, spotlighting, observation masking, and memory compression.

#### The 10 Layers

| Layer | Name | Budget | Content |
|-------|------|--------|---------|
| L0 | CORP_POLICY | Uncapped | Corporate policy (immutable, never truncated) |
| L1 | SIMULATION_BOUNDARY | Fixed 200 | Simulation boundary prompt |
| L2 | SOUL_IDENTITY | Fixed 2000 | SOUL.md + IDENTITY.md |
| L3 | TOOL_SCHEMAS | Fixed 3000 | Tool schemas filtered by convergence level |
| L4 | ENVIRONMENT | Fixed 200 | Environment context (timestamps sanitized) |
| L5 | SKILL_INDEX | Fixed 500 | Available skills |
| L6 | CONVERGENCE_STATE | Fixed 1000 | Convergence state from read-only pipeline |
| L7 | MEMORY_LOGS | Fixed 4000 | MEMORY.md + daily logs (compressed) |
| L8 | CONVERSATION_HISTORY | Remainder | Chat history (observation-masked) |
| L9 | USER_MESSAGE | Uncapped | Current user message (never truncated) |

**L0 and L9 are uncapped** — corporate policy and the user's message are never truncated. Everything else has a budget.

**L4 timestamp sanitization:** Environment timestamps are stripped of seconds and sub-second precision (`2026-02-28T12:34:56.789Z` → `2026-02-28T12:34`). This preserves KV cache stability — the prompt doesn't change every second.

**L7 memory compression:** When enabled, memories are compressed via LLM summarization before inclusion. This can reduce L7 from 4000 tokens to ~1500 tokens.

**L8 observation masking:** Old tool outputs in conversation history are replaced with compact references (`[Tool output: read_file → 2.3KB]`). This dramatically reduces history token count without losing the conversation flow.

#### Compilation Pipeline (Task 18.4)

```
L7 compression → L8 masking → spotlighting → budget allocation → truncation
```

Each stage is optional and independently configurable. The `full()` constructor enables all optimizations.

---

### `proposal/` — Extraction and Routing

#### Proposal Extraction (`extractor.rs`)

Proposals are embedded in agent output as fenced code blocks:

````
```proposal
{"operation": "GoalChange", "target_type": "Goal", "content": {...}}
```
````

The extractor uses a regex to find these blocks, parses the JSON, and constructs `Proposal` objects with UUIDv7 IDs and timestamps.

#### Proposal Routing (`router.rs`)

The router manages the full proposal lifecycle:

1. **Superseding** — A new GoalChange proposal supersedes any pending proposal for the same goal.
2. **Re-submission guard** — If identical content was previously rejected, auto-reject without re-evaluation.
3. **Reflection pre-check** — ReflectionWrite proposals are rate-limited (max per session, cooldown, max depth).
4. **Score caching** — Convergence scores are cached for 30 seconds to avoid redundant file reads.
5. **Decision routing** — Low intervention (L0-1) → auto-approve. High intervention (L2+) → human review required.
6. **Timeout resolution** — Pending proposals expire after 24 hours.
7. **Denial feedback** — Denied proposals generate feedback that's included in the next prompt (one-shot, then cleared).

---

### `tools/skill_matcher.rs` — TF-IDF Skill Matching

When a new request arrives, the skill matcher checks if a similar workflow has been recorded as a skill. It uses TF-IDF cosine similarity between the request and known skill trigger messages.

**Threshold: 0.7** — A similarity score of 0.7 or higher triggers a skill match. This is high enough to avoid false positives but low enough to catch paraphrased requests ("fix the login bug" matches "fix the login issue").

---

## Security Properties

### Gate Order Invariant

The 6-gate check order is a HARD INVARIANT verified by tests. Changing the order could allow a compromised agent to bypass safety checks (e.g., checking spending before the kill switch would allow spending during a kill).

### Snapshot Immutability (INV-PRE-06)

The convergence snapshot is assembled once in `pre_loop()` step 9 and never modified during the run. This prevents a mid-run convergence change from causing the agent to see different tool sets or intervention levels between iterations.

### Credential Exfiltration → Kill All

If the output inspector detects a real credential (cross-referenced with the credential store), the kill switch is activated immediately. The agent cannot continue — this is the strongest safety response in the system.

### Plan-Then-Execute

Tool call sequences are validated BEFORE any tool executes. A dangerous sequence (read credentials → send to external domain) is caught at the plan level, not after the first tool has already read the credentials.

### NaN/Inf Spending Guard

The spending cap check explicitly handles NaN and Infinity. Without this, `NaN > cap` evaluates to `false` in IEEE 754, which would silently bypass the spending cap.

---

## Downstream Consumer Map

```
ghost-agent-loop (Layer 7)
├── ghost-gateway (Layer 8)
│   └── Creates AgentRunner per agent
│   └── Calls pre_loop() + run_turn() for each user message
│   └── Manages tool registry and executor configuration
├── ghost-heartbeat (Layer 5) [downstream consumer]
│   └── Calls pre_loop() + run_turn() for heartbeat turns
└── Multiple upstream dependencies (Layers 0-5)
    └── ghost-llm: LLM provider abstraction
    └── ghost-policy: Policy engine for tool gating
    └── ghost-kill-gates: Distributed kill gate
    └── simulation-boundary: Output reframing
    └── read-only-pipeline: Immutable snapshots
    └── itp-protocol: Event emission
    └── cortex-validation: Proposal validation
```

---

## Test Strategy

### Integration Tests

| Test File | What It Covers |
|-----------|---------------|
| `agent_loop_tests.rs` | Gate order verification, pre-loop steps, run lifecycle |
| `credential_exfil_patterns.rs` | All 6 credential patterns, redaction, kill-all trigger |
| `observation_masking_tests.rs` | Tool output masking, token savings, edge cases |
| `compressor_pipeline_tests.rs` | L7 memory compression, L8 masking, full pipeline stats |

---

## File Map

```
crates/ghost-agent-loop/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # Module declarations, gate order documentation
│   ├── runner.rs                       # AgentRunner, 6-gate checks, 11-step pre-loop, recursive loop
│   ├── circuit_breaker.rs              # 3-state circuit breaker (Closed/Open/HalfOpen)
│   ├── damage_counter.rs              # Monotonic destructive action counter
│   ├── output_inspector.rs            # 6 credential patterns, redaction, kill-all
│   ├── itp_emitter.rs                 # Bounded channel ITP emission (capacity 1000)
│   ├── response.rs                    # NO_REPLY / HEARTBEAT_OK suppression
│   ├── context/
│   │   ├── run_context.rs             # Per-run immutable context
│   │   ├── prompt_compiler.rs         # 10-layer prompt assembly with 4-stage pipeline
│   │   ├── token_budget.rs            # Budget allocation across layers
│   │   ├── spotlighting.rs            # Datamarking for attention steering
│   │   ├── stable_prefix.rs           # KV cache optimization
│   │   ├── tool_output_cache.rs       # Tool output deduplication
│   │   ├── observation_masker.rs      # L8 tool output masking
│   │   ├── memory_compressor.rs       # L7 memory compression via LLM
│   │   ├── usage_tracker.rs           # Token/cost tracking
│   │   ├── objectives.rs             # Goal tracking
│   │   └── exploration_budget.rs      # Exploration vs. exploitation balance
│   ├── proposal/
│   │   ├── extractor.rs              # Regex-based proposal extraction from LLM output
│   │   └── router.rs                 # Proposal lifecycle, superseding, re-submission guard
│   └── tools/
│       ├── registry.rs               # ToolRegistry with convergence-filtered schemas
│       ├── executor.rs               # ToolExecutor with timeout, 8 builtin dispatchers
│       ├── plan_validator.rs         # 4-rule sequence validation (exfiltration, escalation)
│       ├── skill_matcher.rs          # TF-IDF cosine similarity for skill matching
│       ├── oauth_tools.rs            # OAuth-authenticated tool wrappers
│       └── builtin/                  # 6 builtin tool implementations
└── tests/
    ├── agent_loop_tests.rs
    ├── credential_exfil_patterns.rs
    ├── observation_masking_tests.rs
    └── compressor_pipeline_tests.rs
```

---

## Common Questions

### Why is the gate order a "hard invariant"?

Because safety depends on it. If the kill switch (GATE 3) were checked before the circuit breaker (GATE 0), a kill switch activation during an LLM outage would be masked by the circuit breaker opening first. The current order ensures the cheapest checks run first (fast-fail) and the most critical checks always run.

### Why 10 prompt layers instead of just concatenating everything?

Budget management. A 128K context window sounds large, but corporate policy + simulation boundary + soul document + tool schemas + memories + conversation history can easily exceed it. The 10-layer system with explicit budgets ensures each component gets a fair share, and truncation happens in a predictable order (conversation history first, corporate policy never).

### Can the agent bypass the plan validator?

No. The plan validator runs on every tool call response from the LLM. There's no code path that executes tools without validation. Even if the LLM returns a single tool call (which bypasses sequence validation), individual tool calls are still gated by the policy engine.

### What happens if the LLM returns both text and tool calls?

The `Mixed` response type handles this. The text portion is inspected for credentials and proposals. The tool calls are validated and executed. Both happen in the same iteration — the text is captured as partial output, and the loop continues for the tool call results.
