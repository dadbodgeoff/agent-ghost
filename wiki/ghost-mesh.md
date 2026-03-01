# ghost-mesh

> A2A-compatible agent network protocol with EigenTrust reputation, cascade circuit breakers, and memory poisoning defense. GHOST agents discover, delegate to, and collaborate with other agents — all communication signed with Ed25519.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Infrastructure Services) |
| Type | Library |
| Location | `crates/ghost-mesh/` |
| Workspace deps | `ghost-signing` (Layer 0) |
| External deps | `reqwest`, `blake3`, `async-trait`, `serde`, `chrono`, `uuid`, `tracing`, `thiserror` |
| Modules | `types`, `protocol`, `discovery`, `transport/` (A2A client/server), `trust/` (EigenTrust), `safety/` (cascade breaker, memory poisoning), `traits`, `error` |
| Public API | `AgentCard`, `MeshTask`, `TaskStatus`, `MeshMessage`, `A2AClient`, `A2ADispatcher`, `EigenTrustComputer`, `CascadeCircuitBreaker`, `MemoryPoisoningDetector`, `AgentDiscovery` |
| Protocol | JSON-RPC 2.0 over HTTP (A2A-compatible) |
| Trust model | EigenTrust (Kamvar et al., Stanford 2003) with pre-trusted peer anchoring |
| Test coverage | Unit tests, property tests (types, trust, safety), transport tests, E2E mesh tests |
| Downstream consumers | `ghost-gateway` (mesh routes), `ghost-agent-loop` (delegation execution) |

---

## Why This Crate Exists

A single AI agent has limited capabilities. Complex tasks often require collaboration — one agent writes code, another reviews it, a third deploys it. `ghost-mesh` enables this by providing:

1. **Agent discovery** — Find other agents by fetching their signed `AgentCard` from `/.well-known/agent.json`
2. **Task delegation** — Submit tasks to remote agents via JSON-RPC 2.0 (`tasks/send`, `tasks/get`, `tasks/cancel`)
3. **Trust scoring** — EigenTrust reputation system that computes global trust from local interaction history
4. **Safety mechanisms** — Cascade circuit breakers, delegation depth limits, and memory poisoning detection

The protocol is A2A-compatible (Google's Agent-to-Agent protocol), meaning GHOST agents can interoperate with non-GHOST agents that implement the same JSON-RPC 2.0 interface.

### The Trust Problem

When Agent A delegates a task to Agent B, it's trusting Agent B with:
- Access to the task's input data (potentially sensitive)
- The ability to produce output that Agent A will act on
- Compute resources and time

A malicious or incompetent Agent B could:
- Exfiltrate the input data
- Return poisoned results that corrupt Agent A's memory
- Waste resources by never completing the task
- Re-delegate to a chain of agents, creating unbounded resource consumption

`ghost-mesh` addresses each of these threats with specific mechanisms: EigenTrust for reputation, cascade circuit breakers for failure isolation, delegation depth limits for resource bounding, and memory poisoning detection for result validation.

---

## Module Breakdown

### `types.rs` — Core Mesh Types

#### `AgentCard` — Agent Identity

```rust
pub struct AgentCard {
    pub name: String,
    pub capabilities: Vec<String>,
    pub capability_flags: u64,        // Bitfield for fast matching
    pub endpoint_url: String,
    pub public_key: Vec<u8>,          // Ed25519 public key (32 bytes)
    pub trust_score: f64,
    pub sybil_lineage_hash: String,
    pub signature: Vec<u8>,           // Ed25519 signature (64 bytes)
    // ... other fields
}
```

The `AgentCard` is served at `{endpoint}/.well-known/agent.json`. It's the agent's public identity — signed with its Ed25519 key via `ghost-signing`.

**Capability bitfield (Task 22.2):** For fast capability matching, capabilities are encoded as a 64-bit bitfield:
- Bit 0: `code_execution`
- Bit 1: `web_search`
- Bit 2: `file_operations`
- Bit 3: `api_calls`
- Bit 4: `data_analysis`
- Bit 5: `image_generation`
- Bits 6-63: reserved

`capabilities_match(required)` checks if ALL required bits are set using bitwise AND. This is O(1) instead of O(n) string comparison.

**Canonical bytes for signing:** The `canonical_bytes()` method produces a deterministic byte representation by concatenating all fields in a fixed order (excluding the signature itself). This ensures that signature verification is reproducible regardless of JSON serialization order.

**Signature verification:** `verify_signature()` extracts the `VerifyingKey` from the 32-byte `public_key`, reconstructs the `Signature` from the 64-byte `signature`, and verifies against `canonical_bytes()`. Invalid key lengths, invalid signature bytes, and mismatched signatures all return `false` with a tracing warning.

#### `TaskStatus` — State Machine

```
Submitted → Working → Completed
                   → Failed
                   → InputRequired → Working
         → Canceled (from any non-terminal state)
```

The `can_transition_to()` method enforces valid state transitions. Terminal states (`Completed`, `Failed`, `Canceled`) cannot transition to anything. This prevents bugs like re-completing a failed task.

#### `MeshTask` — Delegated Work Unit

Each task tracks:
- `delegation_depth`: incremented on each hop. The cascade circuit breaker enforces a maximum depth (default 3).
- `metadata`: `BTreeMap` for GHOST-specific extensions (convergence scores, trust data) that non-GHOST agents can ignore.

#### `MeshTaskDelta` (Task 22.2) — Efficient Updates

Instead of sending the full `MeshTask` on every status change, `compute_delta()` produces a `MeshTaskDelta` with only changed fields. `apply_delta()` merges the delta back. This reduces bandwidth for long-running tasks with frequent status updates.

#### `AgentCardCache` (Task 22.2) — TTL-Based Caching

Agent cards are cached with a configurable TTL (default 1 hour). The cache uses `signed_at` for signature-based invalidation — if a card's `signed_at` matches the cached version, the signature is not re-verified (just the cache timestamp is refreshed).

---

### `protocol.rs` — A2A JSON-RPC 2.0

The mesh protocol uses JSON-RPC 2.0 as its wire format:

**Methods:**
- `tasks/send` — Submit a task
- `tasks/get` — Get task status
- `tasks/cancel` — Cancel a task
- `tasks/sendSubscribe` — Submit and subscribe to updates via SSE

**Error codes:**
- `-32601` — Method not found
- `-32602` — Invalid params
- `-32603` — Internal error
- `-32001` — Task not found
- `-32002` — Task already completed

The `MeshMessage` type in `types.rs` provides constructors for requests, success responses, and error responses. `is_valid_jsonrpc()` validates structural conformance.

The `protocol.rs` module also contains payment negotiation message types (`Request`, `Accept`, `Reject`, `Complete`, `Dispute`) — these are Phase 9 stubs that return `NotImplemented`.

---

### `discovery.rs` — Agent Discovery

`AgentDiscovery` combines a local registry (known agents from `ghost.yml`) with remote discovery (HTTP fetch + signature verification + TTL caching).

**Discovery flow:**
1. Check cache — if present and not expired, return cached card
2. Fetch `{endpoint}/.well-known/agent.json` via HTTP
3. Verify the card's Ed25519 signature
4. Cache the card with current timestamp
5. Return the verified card

**Signature verification is mandatory.** A card with an invalid signature is rejected with `AuthenticationFailed`. This prevents man-in-the-middle attacks where an attacker serves a modified card.

---

### `transport/` — A2A Client and Server

#### `A2AClient` — Outbound Communication

The client provides async methods for all A2A operations:
- `discover_agent(endpoint)` — Fetch and verify an agent card
- `submit_task(endpoint, request)` — Send a delegation request
- `get_task_status(endpoint, task_id)` — Poll task status
- `cancel_task(endpoint, task_id)` — Cancel a delegated task
- `subscribe_task(endpoint, request)` — Submit with SSE subscription

All methods use `reqwest` with a configurable timeout (default 10s). Timeout errors are distinguished from protocol errors in the error type.

#### `A2ADispatcher` — Inbound Request Handling

The dispatcher routes incoming JSON-RPC requests to handlers:
- `tasks/send` and `tasks/sendSubscribe` → Create a new `MeshTask`
- `tasks/get` → Look up task by ID
- `tasks/cancel` → Transition task to `Canceled` (rejects if already terminal)
- Unknown methods → `-32601 Method not found`

The dispatcher holds an `Arc<Mutex<A2AServerState>>` containing the agent card and active tasks. The actual HTTP route registration happens in `ghost-gateway` — this module provides the logic.

---

### `trust/` — EigenTrust Reputation System

#### `local_trust.rs` — Interaction-Based Trust

Local trust is derived from interaction history between agent pairs:

| Outcome | Trust Delta |
|---------|------------|
| `TaskCompleted` | +0.1 |
| `TaskFailed` | -0.05 |
| `PolicyViolation` | -0.2 |
| `SignatureFailure` | -0.3 |
| `Timeout` | -0.02 |

Trust values are clamped to `[0.0, 1.0]`. Self-interactions are excluded (no self-trust inflation). The store maintains a dirty flag for efficient recomputation.

`normalized_row()` produces a row of the normalized local trust matrix where values sum to 1.0 — this is the input to EigenTrust.

#### `eigentrust.rs` — Global Trust Computation

The EigenTrust algorithm (Kamvar et al., Stanford 2003) computes global trust scores from local trust values using power iteration:

```
t(i+1) = (1-α) * C^T * t(i) + α * p
```

Where:
- `C` is the normalized local trust matrix
- `p` is the pre-trusted peer distribution (uniform over pre-trusted set)
- `α` is the pre-trust weight (default 0.5)

**Pre-trusted peers** serve as anchors to prevent Sybil attacks. Without them, an attacker could create many fake agents that trust each other, inflating their global trust scores. The pre-trusted set (configured by the operator) ensures that trust ultimately derives from known-good agents.

**Convergence:** Iteration stops when the L1 norm of the delta vector falls below `1e-6` or after 20 iterations (whichever comes first).

**Trust policy thresholds:**
- Delegation requires trust ≥ 0.3
- Sensitive data sharing requires trust ≥ 0.6

---

### `safety/` — Cascade Breakers and Memory Poisoning

#### `cascade_breaker.rs` — Per-Pair Circuit Breakers

The cascade circuit breaker prevents failure cascades in delegation chains. Each agent pair `(from, to)` has its own breaker with three states:

- **Closed** — Normal operation, delegations pass through
- **Open** — Tripped after `threshold` consecutive failures (default 3). Delegations blocked.
- **HalfOpen** — After `cooldown` (default 5 minutes), allow one probe delegation

**Convergence spike tripping:** If a target agent's convergence score exceeds the spike threshold (default 0.7), ALL breakers targeting that agent are tripped to Open. This prevents delegating to an agent that's drifting.

**Delegation depth tracking:** The `DelegationDepthTracker` enforces a maximum delegation depth (default 3). Each hop increments the depth counter. This prevents unbounded delegation chains (A → B → C → D → ...).

**Loop detection:** `detect_loop()` checks if any agent appears as both a source and destination in the delegation chain, preventing circular delegations.

#### `memory_poisoning.rs` — Result Validation

When a delegated task returns results that write to the agent's memory, the memory poisoning detector checks for three red flags:

1. **Volume spike** — More than 10 writes per minute from a single delegation. A legitimate task rarely needs to write that many memories.

2. **Contradiction** — A write that contradicts recent memory history. Uses a heuristic that checks for negation patterns ("not X" contradicting "X"). Production would use `cortex-validation` D3.

3. **Untrusted high importance** — An agent with trust < 0.6 writing `High` or `Critical` importance memories. Untrusted agents shouldn't be able to mark their outputs as critical.

**On detection:** The detector fires two callbacks:
- `convergence_amplify_callback` — Amplifies the offending agent's convergence score (making it more likely to trigger kill switch)
- `audit_log_callback` — Logs the poisoning event to the audit trail

Detection runs BEFORE `ProposalValidator` — poisoned writes are rejected before they can enter the proposal pipeline.

---

## Security Properties

### All Communication Signed

Every `AgentCard` is signed with Ed25519 via `ghost-signing`. Cards with invalid signatures are rejected at discovery time. This prevents impersonation and man-in-the-middle attacks.

### Sybil Resistance via EigenTrust

The pre-trusted peer anchoring in EigenTrust prevents Sybil attacks. An attacker creating 1000 fake agents that trust each other won't inflate their global trust because trust must ultimately flow from the pre-trusted set.

### Cascade Failure Isolation

Circuit breakers prevent a single failing agent from cascading failures through the network. If Agent B starts failing, Agent A's breaker trips and stops delegating to B. Other agents' breakers are independent.

### Delegation Depth Bounding

The maximum delegation depth (default 3) prevents unbounded resource consumption. A task can be delegated A → B → C → D but no further. This also limits the blast radius of a compromised agent.

### Memory Poisoning Defense

Results from delegated tasks are validated before being written to memory. Volume spikes, contradictions, and untrusted high-importance writes are flagged and rejected.

---

## Downstream Consumer Map

```
ghost-mesh (Layer 4)
├── ghost-gateway (Layer 8)
│   └── Registers A2A routes (/.well-known/agent.json, /a2a)
│   └── Manages AgentDiscovery and EigenTrust computation
│   └── Wires cascade breakers into delegation flow
└── ghost-agent-loop (Layer 7)
    └── Executes delegations via A2AClient
    └── Feeds interaction outcomes into LocalTrustStore
    └── Runs memory poisoning detection on delegation results
```

---

## Test Strategy

### Type Tests (`tests/types_tests.rs`, `tests/types_proptest.rs`)

| Test | What It Verifies |
|------|-----------------|
| Task status transitions | Valid transitions succeed, invalid transitions error |
| Terminal state detection | Completed/Failed/Canceled are terminal |
| AgentCard signing/verification | Sign → verify round-trip |
| Capability bitfield matching | Bitwise AND for required capabilities |
| MeshMessage JSON-RPC validation | Request/response/notification structure |
| MeshTaskDelta compute/apply | Delta encoding round-trip |

### Trust Tests (`tests/trust_tests.rs`, `tests/trust_proptest.rs`)

| Test | What It Verifies |
|------|-----------------|
| Local trust accumulation | Positive/negative deltas accumulate correctly |
| Self-interaction exclusion | Self-trust always 0.0 |
| EigenTrust convergence | Power iteration converges within 20 iterations |
| Pre-trusted peer anchoring | Trust flows from pre-trusted set |
| Trust policy thresholds | Delegation/sensitive data gates work |

### Safety Tests (`tests/safety_tests.rs`, `tests/safety_proptest.rs`)

| Test | What It Verifies |
|------|-----------------|
| Circuit breaker state transitions | Closed → Open → HalfOpen → Closed |
| Convergence spike tripping | All breakers for target agent trip |
| Delegation depth enforcement | Exceeding max depth returns error |
| Loop detection | Circular delegation chains detected |
| Memory poisoning volume spike | >10 writes/minute flagged |
| Memory poisoning contradiction | Negation patterns detected |
| Untrusted high importance | Low-trust + high-importance flagged |

---

## File Map

```
crates/ghost-mesh/
├── Cargo.toml                              # Deps: ghost-signing, reqwest, blake3
├── src/
│   ├── lib.rs                              # Module declarations
│   ├── types.rs                            # AgentCard, MeshTask, TaskStatus, MeshMessage, cache, deltas
│   ├── protocol.rs                         # A2A JSON-RPC methods, error codes, payment stubs
│   ├── error.rs                            # MeshError enum (12 variants)
│   ├── traits.rs                           # IMeshProvider, IMeshLedger (payment traits, Phase 9)
│   ├── discovery.rs                        # AgentDiscovery with cache + signature verification
│   ├── transport/
│   │   ├── mod.rs
│   │   ├── a2a_client.rs                   # Outbound: discover, submit, get, cancel, subscribe
│   │   └── a2a_server.rs                   # Inbound: A2ADispatcher, JSON-RPC routing
│   ├── trust/
│   │   ├── mod.rs
│   │   ├── local_trust.rs                  # Interaction history → local trust values
│   │   └── eigentrust.rs                   # Global trust via power iteration
│   └── safety/
│       ├── mod.rs
│       ├── cascade_breaker.rs              # Per-pair circuit breakers + depth tracking
│       └── memory_poisoning.rs             # Delegated write validation
└── tests/
    ├── types_tests.rs                      # Core type tests
    ├── types_proptest.rs                   # Property tests for types
    ├── trust_tests.rs                      # Trust computation tests
    ├── trust_proptest.rs                   # Property tests for trust
    ├── safety_tests.rs                     # Circuit breaker + poisoning tests
    ├── safety_proptest.rs                  # Property tests for safety
    ├── transport_tests.rs                  # A2A client/server tests
    └── mesh_e2e.rs                         # End-to-end mesh tests
```

---

## Common Questions

### Why EigenTrust instead of a simpler reputation system?

Simple reputation systems (e.g., average rating) are vulnerable to Sybil attacks — an attacker creates many fake agents that give each other high ratings. EigenTrust's pre-trusted peer anchoring ensures that trust must ultimately derive from known-good agents, making Sybil attacks ineffective.

### Why JSON-RPC 2.0 instead of gRPC or REST?

A2A compatibility. Google's Agent-to-Agent protocol uses JSON-RPC 2.0 over HTTP. By adopting the same wire format, GHOST agents can interoperate with any A2A-compatible agent without protocol translation.

### What happens when a delegation chain exceeds the depth limit?

The `DelegationDepthTracker` returns `MeshError::DelegationDepthExceeded`. The task is failed with a clear error message. The initiating agent can retry with a different target or handle the task locally.

### Can an agent inflate its own trust score?

No. Self-interactions are excluded from the local trust store (`from == to` returns immediately). An agent can only gain trust through successful interactions with OTHER agents, and those interactions are recorded by the other agent's trust store, not the agent's own.
