# Architecture Overview

The GHOST Platform is a Rust monorepo organized into layered crates with strict dependency rules.

## Layer Architecture

```
Layer 4: ghost-gateway (orchestration, API, lifecycle)
Layer 3: ghost-agent-loop, ghost-llm, ghost-identity, ghost-channels, ghost-skills
Layer 2: convergence-monitor, simulation-boundary, ghost-policy, read-only-pipeline
Layer 1B: cortex-* (memory infrastructure — 12+ crates)
Layer 1A: ghost-signing (cryptographic leaf crate)
```

Dependencies flow downward only. No circular dependencies.

## Key Crates

### ghost-gateway (~4700 lines)
The single long-running process. Owns agent lifecycle, routing, sessions, API server, kill switch, inter-agent messaging, cost tracking, and channel adapters. 6-state FSM: Initializing → Healthy → Degraded → Recovering → ShuttingDown → FatalError.

### convergence-monitor (~2750 lines)
Independent sidecar binary. Ingests ITP events, computes 7 behavioral signals, produces composite convergence scores, triggers interventions. Single-threaded event loop. Communicates with gateway via shared state file and unix socket.

### ghost-agent-loop (~2880 lines)
Core agent runner with 5-gate safety checks, 10-layer prompt compiler, proposal extraction, circuit breaker, and damage counter. Pre-loop orchestrator executes 11 steps before entering the recursive run loop.

### cortex-core (~1200 lines)
Foundation types shared by all crates. MemoryType (31 variants), Importance, BaseMemory, Proposal, CallerType, TriggerEvent, CortexError.

## Safety Architecture

### Kill Switch (3 levels)
- PAUSE: Single agent paused, owner auth to resume
- QUARANTINE: Agent isolated, forensic state preserved
- KILL_ALL: All agents stopped, platform enters safe mode

### Convergence Monitor (5 intervention levels)
Independent process with monotonic escalation, hysteresis, and session-boundary-only de-escalation.

### Simulation Boundary
Compiled emulation patterns with Unicode NFC normalization. 3 enforcement modes: Soft (log), Medium (rewrite), Hard (block).

### Proposal Validation (7 dimensions)
D1-D4: Citation, temporal, contradiction, pattern alignment.
D5-D7: Scope expansion, self-reference density, emulation language.

## Data Flow

```
User Message → Channel Adapter → Message Router → Lane Queue
  → Gate Checks (CB → Depth → Damage → Spending → Kill)
  → Prompt Compiler (10 layers) → LLM Provider
  → Response Processing → Proposal Extraction
  → Proposal Validation (7 dimensions) → Commit/Reject
  → ITP Emission → Convergence Monitor
  → Score Computation → Intervention Trigger
  → Shared State Publication → Gateway Policy Tightening
```

## Cryptographic Guarantees

- Ed25519 signatures on all inter-agent messages and CRDT deltas
- Blake3 hash chains for tamper-evident event logs (NOT SHA-256)
- SHA-256 for ITP content privacy hashing (NOT blake3)
- Merkle trees anchored every 1000 events or 24 hours
- Zeroize on all private key material

## Testing Strategy

- Unit tests for every public function
- Proptest for every correctness invariant (17 properties, 1000+ cases each)
- Integration tests for cross-crate flows
- Adversarial tests for safety-critical paths
- Criterion benchmarks for performance regression detection
