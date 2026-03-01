# GHOST Platform — GitHub Wiki

> The definitive deep-dive reference for every crate in the GHOST convergence-aware AI agent orchestration platform.

## What This Wiki Is

This wiki provides exhaustive, decision-level documentation for all 37 crates in the GHOST workspace. Each page goes beyond API docs — it explains **why** things are built the way they are, what tradeoffs were made, what invariants are enforced, and how each crate fits into the larger system.

## Architecture at a Glance

GHOST is a convergence-aware AI agent orchestration system built in Rust. It manages agent lifecycles, enforces safety boundaries, monitors behavioral convergence across 7 signals, and provides multi-channel communication — all with tamper-evident audit trails and cryptographic signing.

**37 crates. 10 dependency layers. Zero circular dependencies.**

```
Layer 10: ghost-integration-tests, cortex-test-fixtures
Layer  9: convergence-monitor (sidecar binary)
Layer  8: ghost-gateway (main binary + library)
Layer  7: ghost-agent-loop
Layer  6: ghost-audit, ghost-backup, ghost-export, ghost-migrate
Layer  5: ghost-channels, ghost-skills, ghost-heartbeat
Layer  4: ghost-identity, ghost-policy, ghost-llm, ghost-proxy,
          ghost-oauth, ghost-egress, ghost-mesh, ghost-kill-gates
Layer  3: itp-protocol, simulation-boundary, read-only-pipeline
Layer  2: cortex-convergence, cortex-validation, cortex-decay,
          cortex-observability, cortex-retrieval, cortex-privacy,
          cortex-multiagent, cortex-napi
Layer  1: cortex-core, cortex-crdt, cortex-storage, cortex-temporal
Layer  0: ghost-signing, ghost-secrets
```

## Crate Deep-Dives

### Layer 0 — Cryptographic Foundation
- [[ghost-signing]] — Ed25519 signing primitives, zeroize-on-drop, deterministic signatures
- [[ghost-secrets]] — Cross-platform credential storage (OS keychain, Vault, env)

### Layer 1 — Cortex Foundation
- [[cortex-core]] — Core types, traits, and configuration
- [[cortex-crdt]] — CRDT primitives with signed deltas and sybil resistance
- [[cortex-storage]] — SQLite with append-only triggers and hash chains
- [[cortex-temporal]] — Hash chains and Merkle trees for tamper evidence

### Layer 2 — Cortex Higher-Order
- [[cortex-convergence]] — 7-signal convergence computation
- [[cortex-validation]] — 7-dimension proposal validation
- [[cortex-decay]] — 6-factor memory decay engine
- [[cortex-observability]] — Prometheus-compatible convergence metrics
- [[cortex-retrieval]] — Convergence-aware memory retrieval
- [[cortex-privacy]] — Emotional content detection and privacy patterns
- [[cortex-multiagent]] — Multi-agent consensus shielding
- [[cortex-napi]] — Node.js/TypeScript bindings

### Layer 3 — Protocols & Boundaries
- [[itp-protocol]] — Interaction Telemetry Protocol
- [[simulation-boundary]] — Emulation pattern detection and reframing
- [[read-only-pipeline]] — Convergence-filtered state snapshots

### Layer 4 — Ghost Infrastructure
- [[ghost-identity]] — Soul documents, keypair lifecycle, drift detection
- [[ghost-policy]] — Cedar-style policy engine with convergence tightening
- [[ghost-llm]] — LLM provider abstraction, fallback chains, cost tracking
- [[ghost-proxy]] — Passive HTTPS proxy for convergence monitoring
- [[ghost-oauth]] — Self-hosted OAuth 2.0 PKCE broker
- [[ghost-egress]] — Per-agent network egress allowlisting
- [[ghost-mesh]] — A2A agent network with EigenTrust reputation
- [[ghost-kill-gates]] — Distributed kill gate coordination

### Layer 5 — Core Services
- [[ghost-channels]] — Unified channel adapter framework (6 platforms)
- [[ghost-skills]] — Skill registry and WASM sandbox
- [[ghost-heartbeat]] — Convergence-aware health monitoring

### Layer 6 — Data & Operations
- [[ghost-audit]] — Queryable audit log engine
- [[ghost-backup]] — Encrypted state backup and restore
- [[ghost-export]] — External AI platform data import
- [[ghost-migrate]] — OpenClaw migration tool

### Layer 7 — Agent Execution
- [[ghost-agent-loop]] — Core agent runner with 6-gate safety checks

### Layer 8 — Gateway Orchestrator
- [[ghost-gateway]] — The single long-running GHOST process

### Layer 9 — Sidecar Binaries
- [[convergence-monitor]] — Independent convergence monitoring sidecar

### Layer 10 — Testing
- [[ghost-integration-tests]] — Cross-crate integration tests and benchmarks
- [[cortex-test-fixtures]] — Shared proptest strategies and fixtures

## Cross-Cutting Concerns
- [[Convergence-Aware-Design]] — How convergence permeates every layer
- [[Safety-Architecture]] — Overlapping safety layers and defense in depth
- [[Cryptographic-Choices]] — blake3 vs SHA-256 vs Ed25519 decision map
- [[Dependency-Layer-Enforcement]] — How layer violations are prevented
