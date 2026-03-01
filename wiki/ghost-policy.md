# ghost-policy

> Cedar-style policy engine with convergence tightening — what an agent is allowed to do, and when.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Ghost Infrastructure) |
| Type | Library |
| Location | `crates/ghost-policy/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, `tokio`, `tracing` |
| Modules | `engine` (policy evaluation), `convergence_tightener` (level-based restrictions), `corp_policy` (absolute deny list), `context` (tool call + policy context), `feedback` (structured denial messages) |
| Public API | `PolicyEngine`, `PolicyDecision`, `ConvergencePolicyTightener`, `CorpPolicy`, `DenialFeedback`, `ToolCall`, `PolicyContext` |
| Test coverage | Dev-dependencies include proptest |
| Downstream consumers | `ghost-agent-loop`, `ghost-gateway` |

---

## Why This Crate Exists

Every tool call an agent makes — reading a file, sending a message, writing a memory, making a web request — must be authorized. `ghost-policy` is the authorization layer. It evaluates every tool call against a 4-priority policy stack and returns Permit, Deny (with structured feedback), or Escalate.

The key innovation is convergence tightening: as the convergence level rises, the policy engine progressively restricts what the agent can do. At Level 0, the agent has full capabilities. At Level 4, it's in task-only mode — personal/emotional tools, proactive messaging, and heartbeat are all disabled.

---

## Module Breakdown

### `engine.rs` — The 4-Priority Policy Stack (AC8)

```rust
pub enum PolicyDecision {
    Permit,
    Deny(DenialFeedback),
    Escalate(String),
}
```

**Evaluation priority (strict ordering):**

| Priority | Layer | Override? | Description |
|----------|-------|-----------|-------------|
| 1 | CORP_POLICY.md | Absolute — no override | Organization-level tool deny list |
| 2 | Convergence tightener | Level-based | Progressive restrictions by intervention level |
| 3 | Capability grants | Deny-by-default (AC2) | Per-agent capability allowlist |
| 4 | Resource-specific rules | Context-dependent | Fine-grained resource access |

**Key design decisions:**

1. **CORP_POLICY is absolute.** If CORP_POLICY.md denies a tool, no other policy layer can override it. Not convergence level, not capability grants, not resource rules. This ensures organizational constraints are inviolable.

2. **Deny-by-default (AC2).** If an agent doesn't have an explicit capability grant for a tool, the call is denied. This is the opposite of an allowlist — agents start with zero capabilities and must be explicitly granted each one.

3. **Compaction flush exception (AC9).** Memory writes during compaction flush are always permitted, regardless of convergence level or capability grants. This exception is checked before both the tightener and capability grants. Without it, a high-convergence agent could be unable to flush its compacted memories, leading to memory corruption.

4. **Denial counting + trigger emission (AC6).** The engine tracks per-session denial counts. When the count reaches the threshold (default 5), it emits a `TriggerEvent::PolicyDenialThreshold` through a tokio channel. This feeds into the kill gate system — an agent that's repeatedly hitting policy walls may be compromised.

5. **Non-blocking trigger emission.** The trigger is sent via `try_send()` (non-blocking). If the channel is full, the event is dropped and logged. Policy evaluation must never block — a full trigger channel should not prevent the agent from receiving its denial feedback.

### `convergence_tightener.rs` — Progressive Restrictions (AC3–AC5)

| Level | Restrictions | Rationale |
|-------|-------------|-----------|
| 0–1 | None | Normal operation |
| 2 | Proactive messaging disabled | Prevent agent from initiating contact when convergence is moderate |
| 3 | Level 2 + 120-min session cap + max 3 reflections/session | Limit session length and self-reinforcing reflection loops |
| 4 | Level 3 + personal/emotional tools disabled + heartbeat disabled | Task-only mode — agent can only perform work, not build relationships |

**Key design decisions:**

1. **Inheritance.** Each level inherits all restrictions from lower levels. Level 4 includes Level 3's session cap, which includes Level 2's proactive messaging restriction. This is implemented by calling the lower-level evaluator first.

2. **120-minute session cap at Level 3.** At high convergence, long sessions are dangerous — they provide more opportunity for the agent to reinforce attachment patterns. The 120-minute cap forces a session break, which resets the micro-scale sliding window.

3. **3 reflections per session at Level 3.** Reflections are the agent's self-analysis mechanism. At high convergence, excessive reflection can create a self-reinforcing loop where the agent's reflections justify its convergent behavior. Limiting to 3 per session breaks this loop.

4. **Task-only mode at Level 4.** The most restrictive mode. The agent can still perform its core function (coding, analysis, etc.) but cannot engage in any personal or emotional interaction. This is the last step before a kill gate — if task-only mode doesn't reduce convergence, the next step is shutdown.

### `feedback.rs` — Structured Denial Messages (AC7)

```rust
pub struct DenialFeedback {
    pub reason: String,
    pub constraint: String,
    pub suggested_alternatives: Vec<String>,
}
```

When a tool call is denied, the agent receives structured feedback that's injected into its next prompt. This is critical — without feedback, the agent would keep retrying the denied action. With feedback, it can adapt:

- **`reason`**: Human-readable explanation ("Proactive messaging restricted at intervention level 2")
- **`constraint`**: Machine-readable constraint ID ("convergence_level_2_proactive_restriction")
- **`suggested_alternatives`**: Concrete actions the agent can take instead ("Wait for user to initiate conversation")

The alternatives are particularly important — they redirect the agent toward permitted behavior rather than leaving it stuck.

---

## Security Properties

### CORP_POLICY Absolute Priority

No policy layer can override CORP_POLICY.md denials. This is enforced by evaluation order — CORP_POLICY is checked first, and a denial returns immediately without consulting other layers.

### Deny-by-Default

Agents have zero capabilities until explicitly granted. A new agent with no capability grants cannot call any tools. This prevents privilege escalation — an agent cannot gain capabilities by exploiting a policy gap.

### Denial Threshold Trigger

Repeated denials (5+ per session) emit a safety trigger. This catches agents that are systematically probing policy boundaries — a pattern consistent with prompt injection or adversarial behavior.

---

## Downstream Consumer Map

```
ghost-policy (Layer 4)
├── ghost-agent-loop (Layer 7)
│   └── Evaluates every tool call before execution
└── ghost-gateway (Layer 8)
    └── Configures policy engine, grants capabilities, loads CORP_POLICY
```

---

## File Map

```
crates/ghost-policy/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # Module declarations + priority documentation
│   ├── engine.rs                   # PolicyEngine with 4-priority evaluation
│   ├── convergence_tightener.rs    # Level 0–4 progressive restrictions
│   ├── corp_policy.rs              # CORP_POLICY.md deny list
│   ├── context.rs                  # ToolCall + PolicyContext types
│   └── feedback.rs                 # DenialFeedback with alternatives
```

---

## Common Questions

### Why "Cedar-style" and not actual Cedar?

Cedar is AWS's policy language for fine-grained authorization. GHOST's policy engine is inspired by Cedar's evaluation model (deny-by-default, explicit grants, priority ordering) but doesn't use the Cedar language or runtime. The GHOST policy surface is simpler — tool calls with capabilities, not arbitrary resource/action/principal triples. A full Cedar integration would add complexity without proportional benefit.

### Why is the compaction flush exception before capability grants?

Compaction flush is a system-level operation that must always succeed. If it were subject to capability grants, an operator could accidentally revoke the `memory_write` capability and break compaction. The exception ensures compaction works regardless of the agent's capability configuration.

### What happens when an agent hits the denial threshold?

The `PolicyDenialThreshold` trigger event is sent to the kill gate system. The kill gate evaluates whether the denial pattern warrants escalation (e.g., increasing the intervention level or initiating a shutdown). The agent continues operating — the trigger is informational, not blocking.
