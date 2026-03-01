# Distributed Kill Gates

## Overview

Distributed Kill Gates extend the existing single-node `KillSwitch` (Req 14) into a
multi-node coordination layer. When a GHOST platform runs across multiple gateway
instances (horizontal scaling, multi-region), a kill event on **any** node must
propagate to **all** nodes within a bounded time window.

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Gateway A   │     │  Gateway B   │     │  Gateway C   │
│  KillSwitch  │◄───►│  KillSwitch  │◄───►│  KillSwitch  │
│  KillGate    │     │  KillGate    │     │  KillGate    │
└──────┬───────┘     └──────┬───────┘     └──────┬───────┘
       │                    │                    │
       └────────────────────┼────────────────────┘
                            │
                   ┌────────▼────────┐
                   │  KillGateRelay  │
                   │  (gossip/fan-out)│
                   └─────────────────┘
```

## Design Principles

1. **Crashing-safe monotonicity**: A gate, once closed, stays closed until
   explicit distributed resume with quorum. Local `KillSwitch` monotonicity
   is preserved — distributed layer only adds propagation.

2. **Crashing-safe propagation**: Uses hash-chained gate events (blake3,
   same as `cortex-temporal`) so nodes can detect missed events on rejoin.

3. **Bounded propagation delay**: Configurable `max_propagation_ms` (default
   500ms). If a node hasn't received an ack within this window, it assumes
   network partition and enters local-only mode (fail-closed).

4. **Quorum resume**: Resuming from a distributed kill requires
   `ceil(n/2) + 1` nodes to agree. Single-node resume is impossible.

5. **Audit chain**: Every gate event (close, propagate, ack, resume) is
   appended to a blake3 hash chain per node, cross-verified on sync.

## Components

### `ghost-kill-gates` crate

New workspace crate at `crates/ghost-kill-gates/`.

| Module | Responsibility |
|--------|---------------|
| `gate.rs` | `KillGate` — distributed gate state machine |
| `relay.rs` | `KillGateRelay` — fan-out/gossip propagation |
| `chain.rs` | Gate event hash chain (blake3) |
| `quorum.rs` | Quorum logic for distributed resume |
| `config.rs` | `KillGateConfig` with propagation/quorum settings |
| `lib.rs` | Public API surface |

### Gate States

```
Normal ──► GateClosed ──► Propagating ──► Confirmed
                                              │
                                              ▼
                                     QuorumResume ──► Normal
```

- **Normal**: All gates open, agents can execute.
- **GateClosed**: Local kill triggered, propagation initiated.
- **Propagating**: Fan-out in progress, waiting for acks.
- **Confirmed**: All reachable nodes have acked.
- **QuorumResume**: Resume vote in progress.

### Integration Points

1. **ghost-gateway/safety**: `KillGate` wraps existing `KillSwitch`.
   `KillSwitch::activate_*` calls flow through `KillGate::close()` which
   handles local activation + distributed propagation.

2. **ghost-agent-loop/runner.rs**: GATE 3 (kill switch) check extended to
   also consult `KillGate::is_closed()`. Gate check order unchanged.

3. **ghost-mesh**: New `MeshError::KillGateClosed` variant for when a
   delegated task hits a closed gate on the target node.

4. **cortex-core/safety/trigger.rs**: New `TriggerEvent::DistributedKillGate`
   variant for gate propagation events.

5. **ghost-integration-tests**: Integration + adversarial tests for
   split-brain, race conditions, quorum edge cases.

## Configuration

```yaml
# ghost.yml
kill_gates:
  enabled: true
  max_propagation_ms: 500
  quorum_size: null        # auto: ceil(n/2) + 1
  heartbeat_interval_ms: 1000
  partition_timeout_ms: 3000
  chain_verify_on_sync: true
```

## Security Invariants

- **INV-KG-01**: Gate close is monotonic — severity never decreases without quorum resume.
- **INV-KG-02**: Propagation timeout → fail-closed (local kill persists).
- **INV-KG-03**: Resume requires quorum — single compromised node cannot unilaterally resume.
- **INV-KG-04**: All gate events are hash-chained — tampering is detectable.
- **INV-KG-05**: Gate state is SeqCst atomic — no stale reads across threads.
