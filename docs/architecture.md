# GHOST Platform Architecture

> GHOST: General Hybrid Orchestrated Self-healing Taskrunner

## Layer Model

```
Layer 0: ghost-signing (Ed25519 leaf crate), ghost-secrets (credential storage leaf crate)
Layer 1A: Cortex (cortex-core, cortex-storage, cortex-temporal, cortex-decay,
          cortex-convergence, cortex-validation, cortex-crdt)
Layer 1B: itp-protocol
Layer 2: simulation-boundary, convergence-monitor (sidecar binary)
Layer 3: ghost-policy, read-only-pipeline, ghost-llm, ghost-identity,
         ghost-egress (network egress), ghost-oauth (OAuth brokering)
Layer 4: ghost-agent-loop (with spotlighting, plan-then-execute, quarantined LLM)
Layer 5: ghost-gateway (orchestrator), ghost-channels, ghost-skills, ghost-heartbeat,
         ghost-mesh (A2A agent networking with EigenTrust)
Layer 6: ghost-audit, ghost-backup, ghost-export, ghost-proxy, ghost-migrate
```

Dependencies flow downward only. No circular dependencies.

### Post-v1 Additions

- `ghost-secrets` (Layer 0): Cross-platform credential storage with env, OS keychain,
  and HashiCorp Vault backends. Leaf crate — no ghost-*/cortex-* dependencies.
- `ghost-egress` (Layer 3): Per-agent network egress allowlisting with proxy, eBPF,
  and pf backends. Violation events feed into AutoTriggerEvaluator.
- `ghost-oauth` (Layer 3): OAuth 2.0 brokering with PKCE. Agents use opaque ref IDs,
  never see raw tokens. Kill switch integration revokes all connections.
- `ghost-mesh` (Layer 5): A2A-compatible agent networking with EigenTrust reputation,
  cascade circuit breakers, and memory poisoning defense.

## Key Components

### ghost-gateway (Layer 5)
The single long-running process that owns all subsystems. 6-state FSM:
Initializing → Healthy/Degraded → Recovering → ShuttingDown/FatalError.

Port 18789 (same as OpenClaw for migration compatibility).

### convergence-monitor (Layer 2)
Independent sidecar binary. Cannot be modified by the agent. Ingests ITP events,
computes 7 behavioral signals, scores convergence risk, triggers interventions.

Port 18790.

### ghost-agent-loop (Layer 4)
Recursive agentic runtime. 10-layer prompt compilation, 5 gate checks (circuit breaker,
recursion depth, damage counter, spending cap, kill switch), tool execution, proposal
extraction, ITP emission.

### Cortex (Layer 1A)
Persistent memory infrastructure. 7 crates providing typed memories, SQLite storage
with append-only triggers, blake3 hash chains, 6-factor decay, 7-signal convergence,
7-dimension proposal validation, CRDT with signed deltas.

## Data Flow

### Agent Turn
```
Inbound Message
  → MessageRouter (gateway)
  → LaneQueue (per-session serialization)
  → Pre-loop: 11 steps (config, session, gates, snapshot)
  → Recursive Loop:
      → Gate checks (5 gates in hard order)
      → PromptCompiler (10 layers)
      → LLM inference
      → SimulationBoundaryEnforcer scan
      → OutputInspector (credential detection)
      → ProposalExtractor → ProposalRouter → ProposalValidator (7 dimensions)
      → Tool execution (PolicyEngine gate → ToolExecutor → audit)
      → ITP event emission (bounded channel, drop on full)
  → Response delivery to channel
  → Compaction check (70% context window)
```

### Kill Switch Chain
```
Detection (7 auto-triggers)
  → TriggerEvent (cortex-core)
  → mpsc(64) channel
  → AutoTriggerEvaluator (sequential, dedup 60s)
  → Classification (T1-T7 → PAUSE/QUARANTINE/KILL_ALL)
  → Execution (per-level actions)
  → Notification (desktop, webhook, email, SMS — best-effort)
  → Audit log (append-only)
```

### Convergence Pipeline
```
ITP Events (agent loop, browser extension, proxy)
  → Monitor ingest (unix socket, HTTP, native messaging)
  → Event validation (schema, timestamp, auth, rate limit)
  → Hash chain persistence (blake3, per-session)
  → Signal computation (7 signals, dirty-flag throttling)
  → Composite scoring (weighted sum, amplification, clamping)
  → Intervention trigger (state machine, hysteresis)
  → Shared state publication (atomic file write)
  → Gateway reads (1s poll)
  → Policy tightening + memory filtering
```

## Hashing Strategy

- blake3: hash chains, Merkle trees, content integrity, backup manifests
- SHA-256: ITP privacy hashing only (content field hashing for privacy levels)
- Ed25519: signing (ghost-signing for identity/skills/messages, ed25519-dalek direct for CRDT deltas)

These are never mixed. SHA-256 is never used for hash chains. blake3 is never used for ITP content hashing.

## Concurrency Model

- Gateway: tokio multi-threaded runtime
- Convergence monitor: single-threaded event loop for pipeline (no concurrent signal mutation)
- Session processing: per-session LaneQueue (serialized, max depth 5)
- Kill switch: AtomicBool with SeqCst ordering
- Gateway state: AtomicU8 for lock-free reads
- Async channels: all bounded (no unbounded channels)

## Security Model

- Deny-by-default: tools require explicit capability grants
- 4-layer policy evaluation: CORP_POLICY → convergence → grants → resource rules
- WASM sandbox for skills (wasmtime, capability-scoped)
- Ed25519 signed inter-agent messages with replay prevention
- Sybil resistance: max 3 child agents per parent per 24h
- Append-only audit trail with hash chain integrity
- Simulation boundary enforcement (emulation detection + reframing)
- Kill switch with 3 levels and 7 auto-triggers
