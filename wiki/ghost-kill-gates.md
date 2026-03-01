# ghost-kill-gates

> Distributed kill gate coordination for multi-node GHOST platforms — extends the single-node kill switch into a cluster-wide safety mechanism with hash-chained audit, bounded propagation, and quorum-based resume.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Infrastructure Services) |
| Type | Library |
| Location | `crates/ghost-kill-gates/` |
| Workspace deps | `cortex-core` (Layer 1), `ghost-signing` (Layer 0) |
| External deps | `blake3`, `tokio`, `serde`, `chrono`, `uuid`, `tracing`, `thiserror` |
| Modules | `gate`, `chain`, `config`, `quorum`, `relay` |
| Public API | `KillGate`, `GateChainEvent`, `KillGateConfig`, `QuorumTracker`, `KillGateRelay`, `GateRelayMessage` |
| Gate states | Normal → GateClosed → Propagating → Confirmed → (QuorumResume → Normal) |
| Hash chain | blake3-based, same genesis convention as `cortex-temporal` |
| Default propagation timeout | 500ms |
| Default quorum | ceil(n/2) + 1 |
| Test coverage | Unit tests, state machine tests, chain verification tests, quorum tests, relay tests |
| Downstream consumers | `ghost-gateway` (gate lifecycle), `ghost-agent-loop` (GATE 3 check) |

---

## Why This Crate Exists

The single-node `KillSwitch` in `ghost-gateway` can halt all agents on one machine. But GHOST can run as a multi-node cluster — multiple gateway instances coordinating across a network. When one node detects a safety violation and triggers its kill switch, ALL nodes in the cluster must stop their agents. A single rogue node continuing to operate defeats the purpose of the kill switch.

`ghost-kill-gates` solves the distributed coordination problem:

1. **Propagation** — When one node closes its gate, the event is broadcast to all peers via fan-out
2. **Acknowledgment** — Each peer acks the close, confirming they've also stopped
3. **Confirmation** — Once all peers ack (or propagation times out), the gate is confirmed closed
4. **Quorum resume** — Reopening requires ceil(n/2) + 1 votes from distinct nodes. A single node cannot unilaterally resume.
5. **Audit chain** — Every gate event is recorded in a blake3 hash chain for tamper-evident logging

### Key Invariants

- **INV-KG-01: Monotonic severity.** Gate severity never decreases without quorum. Once closed, it stays closed until enough nodes vote to resume.
- **INV-KG-02: Fail-closed on timeout.** If propagation times out (default 500ms), the gate stays closed. Network partitions don't cause gates to reopen.
- **INV-KG-03: No single-node resume.** Even in a single-node cluster, the quorum requirement is at least 1 vote. In multi-node clusters, a majority is required.
- **INV-KG-05: SeqCst atomic state.** Gate state is stored as an `AtomicU8` with `SeqCst` ordering. No stale reads, no torn writes.

---

## Module Breakdown

### `gate.rs` — The Distributed Kill Gate

The `KillGate` is the central state machine. It wraps an `AtomicU8` for the gate state (fast-path check) and an `RwLock<GateInner>` for the detailed state (chain, acks, quorum tracker).

#### Gate State Machine

```
Normal ──close()──→ GateClosed ──begin_propagation()──→ Propagating
                                                            │
                                                    record_ack() × N
                                                            │
                                                            ▼
                                                       Confirmed
                                                            │
                                                  cast_resume_vote() × quorum
                                                            │
                                                            ▼
                                                    QuorumResume → Normal
```

#### `is_closed()` — The Fast Path

```rust
pub fn is_closed(&self) -> bool {
    self.state.load(Ordering::SeqCst) != STATE_NORMAL
}
```

This is called on every agent loop iteration (GATE 3). It must be as fast as possible — a single atomic load with no lock acquisition. Any state other than `Normal` means the gate is closed, including `QuorumResume` (the gate only opens after quorum is fully confirmed).

**Why SeqCst?** Sequential consistency ensures that if Node A closes the gate and Node B reads the state, Node B sees the closed state. Weaker orderings (`Relaxed`, `Acquire/Release`) could allow stale reads in multi-threaded scenarios where the gate close and the agent loop check happen on different threads.

#### `close()` — Triggering the Gate

When a safety violation is detected:
1. State transitions to `GateClosed` (atomic store)
2. Timestamp and reason are recorded
3. A `GateChainEvent` is appended to the hash chain
4. The event is returned for relay propagation

**Monotonicity:** If the gate is already closed, the close is still recorded in the chain (for audit) but the state doesn't change. This prevents a lower-severity close from overriding a higher-severity one.

**Poisoned lock recovery:** If the `RwLock` is poisoned (a thread panicked while holding the lock), the gate recovers by extracting the inner data from the poisoned guard. This is safe because gate operations are idempotent — a duplicate close or ack doesn't corrupt state.

#### `record_ack()` — Peer Acknowledgment

When a peer acks the close:
1. The peer's node ID is added to the acked list (deduplicated)
2. A chain event is recorded
3. If all peers have acked (`acked_nodes.len() >= cluster_size - 1`), state transitions to `Confirmed`

#### `cast_resume_vote()` — Quorum-Based Resume

Resuming from a closed gate requires a quorum of votes:
1. If no `QuorumTracker` exists, one is created with `effective_quorum(cluster_size)` required votes
2. State transitions to `QuorumResume`
3. The vote is recorded
4. If quorum is reached, state transitions to `Normal` and all close state is cleared

---

### `chain.rs` — Blake3 Hash Chain

Every gate event is recorded in a tamper-evident hash chain:

```rust
pub struct GateChainEvent {
    pub event_type: GateEventType,
    pub node_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub payload_json: String,
    pub event_hash: [u8; 32],
    pub previous_hash: [u8; 32],
}
```

**Hash computation:**
```
hash = blake3(event_type || "|" || node_id || "|" || timestamp || "|" || payload_json || "|" || previous_hash)
```

**Genesis hash:** `[0u8; 32]` — same convention as `cortex-temporal`. The first event in the chain has `previous_hash = GENESIS_HASH`.

**Event types:**
- `Close` — Gate closed (kill activated)
- `Propagate` — Close event sent to peer
- `Ack` — Peer acknowledged close
- `ResumeVote` — Resume vote cast
- `ResumeConfirmed` — Quorum reached, gate reopened
- `PartitionDetected` — Node detected as unreachable
- `Rejoin` — Partitioned node rejoined

**Why blake3?** Same rationale as `cortex-temporal` — blake3 is faster than SHA-256 and provides 256-bit collision resistance. The hash chain is verified on node sync/rejoin (configurable via `chain_verify_on_sync`).

---

### `config.rs` — Kill Gate Configuration

```rust
pub struct KillGateConfig {
    pub enabled: bool,                    // Default: true
    pub max_propagation: Duration,        // Default: 500ms
    pub quorum_size: Option<usize>,       // Default: None (auto)
    pub heartbeat_interval: Duration,     // Default: 1000ms
    pub partition_timeout: Duration,      // Default: 3000ms
    pub chain_verify_on_sync: bool,       // Default: true
}
```

**Propagation timeout (500ms):** If not all peers ack within 500ms, the gate stays closed (fail-closed). This is aggressive — it means network latency > 500ms will cause the gate to stay in `Propagating` state rather than reaching `Confirmed`. The rationale: safety over availability. A slow network is better than a partially-closed cluster.

**Auto quorum:** `effective_quorum(cluster_size)` computes `ceil(n/2) + 1`. For a 3-node cluster, quorum is 2. For a 5-node cluster, quorum is 3. This ensures a majority must agree to resume.

**Heartbeat and partition detection:** Nodes send heartbeats every 1 second. If no heartbeat is received for 3 seconds, the node is declared partitioned. Partitioned nodes cannot vote in quorum.

---

### `quorum.rs` — Resume Quorum Logic

```rust
pub struct ResumeVote {
    pub node_id: Uuid,
    pub reason: String,
    pub initiated_by: String,    // Who initiated the resume (operator ID)
    pub voted_at: DateTime<Utc>,
}
```

The `QuorumTracker` uses a `BTreeSet<Uuid>` for vote deduplication — each node can only vote once. The vote log records all votes for audit.

**Why `BTreeSet`?** Deterministic ordering for the vote log. If two nodes vote simultaneously, the log order is consistent across all nodes (sorted by UUID).

**Single-node resume prevention:** Even with `quorum_size = 1`, the vote must come from a distinct node. The `cast_vote()` method doesn't check if the voter is the same node that closed the gate — this is enforced at the relay level where the operator must explicitly initiate the resume.

---

### `relay.rs` — Fan-Out Propagation

The `KillGateRelay` manages peer tracking and message dispatch.

#### Message Types

```rust
pub enum GateRelayMessage {
    CloseNotification { origin_node, event },
    CloseAck { acking_node, origin_node, chain_head_hash },
    Heartbeat { node_id, gate_state, chain_length, timestamp },
    ResumeVoteBroadcast { node_id, reason, initiated_by },
}
```

#### Close Propagation Flow

```
Node A detects violation
  → gate.close("convergence spike")
  → relay.build_close_notification(event)
  → Send CloseNotification to all peers
  
Node B receives CloseNotification
  → relay.process_message(msg)
    → gate.close("propagated from Node A")
    → Returns CloseAck
  
Node A receives CloseAck from Node B
  → relay.process_message(msg)
    → gate.record_ack(Node B, cluster_size)
    → If all acked → gate transitions to Confirmed
```

#### Heartbeat-Based Liveness

Heartbeats carry the node's gate state and chain length. This allows peers to detect:
- A node that's closed its gate (gate_state != Normal)
- A node with a divergent chain (different chain_length)
- A node that's gone silent (no heartbeat within partition_timeout)

**Why fan-out instead of gossip?** Cluster sizes are expected to be small (< 20 nodes). Fan-out is simpler, more predictable, and has bounded latency. Gossip protocols are designed for large clusters (hundreds of nodes) where fan-out would be too expensive.

---

## Security Properties

### Fail-Closed on Every Failure Mode

- **Lock poisoned:** Recovered with poisoned guard, gate stays closed
- **Propagation timeout:** Gate stays in Propagating (closed), not Normal
- **Unknown state byte:** Defaults to GateClosed
- **Network partition:** Partitioned nodes can't vote, gate stays closed

### Tamper-Evident Audit Chain

Every gate event is recorded in a blake3 hash chain. Modifying any event invalidates all subsequent hashes. The chain is verified on node sync/rejoin to detect tampering or divergence.

### Quorum-Based Resume

A single compromised node cannot reopen the gate. Resume requires a majority of nodes to vote, and each vote is recorded with the operator's identity for accountability.

### SeqCst Atomic State

The gate state is stored as an `AtomicU8` with `SeqCst` ordering. This provides the strongest memory ordering guarantee — no thread can see a stale state. This is critical because the agent loop checks `is_closed()` on every iteration.

---

## Downstream Consumer Map

```
ghost-kill-gates (Layer 4)
├── ghost-gateway (Layer 8)
│   └── Creates KillGate on startup
│   └── Manages KillGateRelay for peer communication
│   └── Wires gate close to local KillSwitch
│   └── Exposes /api/gate/status and /api/gate/resume endpoints
└── ghost-agent-loop (Layer 7)
    └── GATE 3 check: gate.is_closed() on every iteration
    └── If closed, agent loop halts immediately
```

---

## Test Strategy

### Gate State Machine Tests

| Test | What It Verifies |
|------|-----------------|
| `gate_starts_normal` | Initial state is Normal |
| `close_transitions_to_gate_closed` | close() sets state to GateClosed |
| `is_closed_returns_true_after_close` | Fast-path check works |
| `record_ack_transitions_to_confirmed` | All peers acking → Confirmed |
| `resume_vote_quorum_reopens_gate` | Quorum votes → Normal |
| `monotonic_severity` | Double close doesn't downgrade |
| `poisoned_lock_recovery` | Gate recovers from poisoned RwLock |

### Chain Verification Tests

| Test | What It Verifies |
|------|-----------------|
| `chain_starts_with_genesis` | First event has previous_hash = GENESIS_HASH |
| `chain_events_linked` | Each event's previous_hash matches prior event's hash |
| `tampered_event_detected` | Modified payload invalidates hash |

### Quorum Tests

| Test | What It Verifies |
|------|-----------------|
| `quorum_requires_majority` | ceil(n/2) + 1 votes needed |
| `duplicate_votes_deduplicated` | Same node voting twice counts as one |
| `single_node_cannot_resume` | One vote insufficient for cluster > 1 |

### Relay Tests

| Test | What It Verifies |
|------|-----------------|
| `close_notification_propagates` | Receiving close triggers local close |
| `ack_sent_on_close_notification` | CloseAck returned after processing |
| `heartbeat_updates_liveness` | Peer marked alive on heartbeat |
| `resume_vote_broadcast_processed` | Vote cast on receiving broadcast |

---

## File Map

```
crates/ghost-kill-gates/
├── Cargo.toml                          # Deps: cortex-core, ghost-signing, blake3
├── src/
│   ├── lib.rs                          # Module declarations, integration docs
│   ├── gate.rs                         # KillGate state machine, AtomicU8 + RwLock
│   ├── chain.rs                        # Blake3 hash chain, GateChainEvent, genesis
│   ├── config.rs                       # KillGateConfig, auto quorum computation
│   ├── quorum.rs                       # QuorumTracker, ResumeVote, BTreeSet dedup
│   └── relay.rs                        # KillGateRelay, fan-out, GateRelayMessage
└── (tests in ghost-integration-tests)
```

---

## Common Questions

### Why 500ms propagation timeout?

Safety over availability. If a node can't reach its peers within 500ms, something is seriously wrong (network partition, peer crash). The gate stays closed until the situation is resolved. A longer timeout would mean agents continue running on unconfirmed nodes for longer.

### What happens during a network partition?

Partitioned nodes can't send or receive gate messages. If Node A closes its gate and Node B is partitioned:
- Node A's gate stays in Propagating (never reaches Confirmed because B can't ack)
- Node B's gate stays in whatever state it was in (Normal if it hasn't detected the violation)
- When the partition heals, Node B receives the close notification and closes its gate
- The chain is verified on rejoin to detect any divergence

### Can an operator override the quorum requirement?

Yes, via `quorum_size` in `KillGateConfig`. Setting it to 1 allows single-node resume. This is useful for development/testing but should never be used in production — it defeats the purpose of distributed coordination.

### Why blake3 instead of Ed25519 signatures for the chain?

The chain provides tamper evidence, not authentication. Each event is linked to the previous by hash — modifying any event breaks the chain. Ed25519 signatures would add authentication (proving WHO created the event) but at significant computational cost for every gate event. The chain is local to each node; cross-node verification uses the relay protocol.

### How does this interact with the single-node KillSwitch?

The `KillGate` wraps the local `KillSwitch`. When `gate.close()` is called, it also triggers the local kill switch. The agent loop checks both: `kill_switch.is_active()` (local) and `gate.is_closed()` (distributed). Either being true halts the agent.
