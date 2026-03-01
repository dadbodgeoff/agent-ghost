# GHOST Platform — 37-Crate Architecture Breakdown

> A complete map of every crate in the GHOST workspace, how they connect, and what they do.

## Table of Contents

- [System Overview](#system-overview)
- [Dependency Layers](#dependency-layers)
- [Layer 0 — Leaf Crates](#layer-0--leaf-crates)
- [Layer 1 — Cortex Foundation](#layer-1--cortex-foundation)
- [Layer 2 — Cortex Higher-Order](#layer-2--cortex-higher-order)
- [Layer 3 — Protocols & Boundaries](#layer-3--protocols--boundaries)
- [Layer 4 — Ghost Infrastructure](#layer-4--ghost-infrastructure)
- [Layer 5 — Ghost Core Services](#layer-5--ghost-core-services)
- [Layer 6 — Data & Operations](#layer-6--data--operations)
- [Layer 7 — Agent Execution](#layer-7--agent-execution)
- [Layer 8 — Gateway Orchestrator](#layer-8--gateway-orchestrator)
- [Layer 9 — Sidecar Binaries](#layer-9--sidecar-binaries)
- [Layer 10 — Testing](#layer-10--testing)
- [Cross-Cutting Concerns](#cross-cutting-concerns)
- [Dependency Graph Summary](#dependency-graph-summary)
- [Binary Entry Points](#binary-entry-points)

---

## System Overview

The GHOST platform is a convergence-aware AI agent orchestration system built in Rust. It manages agent lifecycles, enforces safety boundaries, monitors behavioral convergence across 7 signals, and provides multi-channel communication — all with tamper-evident audit trails and cryptographic signing.

The workspace contains **37 crates** (35 libraries/binaries + 1 test-only crate + 1 test fixtures crate), organized into a strict layered dependency hierarchy. Lower layers never depend on higher layers.

**Key external dependencies:**
- `tokio` — async runtime
- `serde` / `serde_json` — serialization
- `blake3` — hashing (integrity, hash chains)
- `ed25519-dalek` — cryptographic signing
- `rusqlite` — SQLite persistence (append-only, WAL mode)
- `axum` / `tower-http` — HTTP API framework
- `reqwest` — HTTP client (LLM providers, mesh networking)

---

## Dependency Layers

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

---

## Layer 0 — Leaf Crates

These crates have **zero** dependencies on any other `ghost-*` or `cortex-*` crate. They are the cryptographic and secrets foundation.

### 1. `ghost-signing`

**Ed25519 signing primitives.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None |
| Key exports | `generate_keypair`, `SigningKey`, `VerifyingKey`, `sign`, `Signature`, `verify` |
| Modules | `keypair`, `signer`, `verifier` |

Provides keypair generation, message signing, and signature verification using `ed25519-dalek`. All private key material is zeroized on drop via the `zeroize` crate. Used by `cortex-crdt` (signed deltas), `ghost-identity` (agent keypairs), `ghost-mesh` (inter-agent message signing), and `ghost-kill-gates` (gate chain events).

### 2. `ghost-secrets`

**Cross-platform credential storage.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None |
| Features | `keychain` (OS keychain via `keyring`), `vault` (HashiCorp Vault) |
| Key exports | `SecretProvider`, `EnvProvider`, `KeychainProvider`, `VaultProvider`, `SecretString` |
| Modules | `error`, `provider`, `env_provider`, `keychain_provider`, `vault_provider` |

All secret values are wrapped in `SecretString` (zeroized on drop via `secrecy`). Three backends: OS keychain (macOS/Linux/Windows), HashiCorp Vault (HTTP API), and environment variable fallback. Feature-gated so only the needed backend is compiled.

---

## Layer 1 — Cortex Foundation

The Cortex subsystem is the memory and convergence engine. Layer 1 contains the foundational data structures and persistence.

### 3. `cortex-core`

**Core types, traits, and configuration for the Cortex memory system.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None (external only: serde, chrono, uuid, thiserror) |
| Modules | `config`, `memory`, `models`, `safety`, `traits` |

The single source of truth for shared data structures consumed by all downstream `cortex-*` and `ghost-*` crates. Defines `BaseMemory`, `Importance`, `MemoryType`, `Intent`, `ProposalOperation`, `ProposalDecision`, convergence signal types, and error types. Designated as **Layer 1A** in the dependency hierarchy.

### 4. `cortex-crdt`

**CRDT primitives with Ed25519 signed deltas and sybil resistance.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| External deps | `ed25519-dalek` (direct, not via ghost-signing) |
| Modules | `signing`, `sybil` |

Provides distributed data structures where every memory delta is cryptographically signed. The `sybil` module implements resistance against fake-identity attacks on the CRDT state. Uses `ed25519-dalek` directly (not `ghost-signing`) because the wrapper types differ: cortex-crdt wraps `MemoryDelta → SignedDelta`, while ghost-signing wraps agent-level messages.

### 5. `cortex-storage`

**SQLite storage layer with append-only triggers and hash chain columns.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Key exports | `open_in_memory`, `run_all_migrations`, `to_storage_err` |
| Modules | `migrations`, `queries` |

Manages all SQLite persistence for the platform. Tables include convergence scores, goal proposals, interventions, ITP events, and boundary violations. All tables use append-only triggers (no UPDATE/DELETE) and hash chain columns for tamper evidence. The `queries` module provides typed query engines for each table.

### 6. `cortex-temporal`

**Hash chains and Merkle trees for tamper-evident event logs.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Features | `sqlite` (optional rusqlite integration) |
| Modules | `hash_chain`, `anchoring` |

Uses blake3 (workspace standard) for all hashing. The `hash_chain` module provides append-only chains where each entry links to the previous via its hash. The `anchoring` module supports Git anchoring and RFC 3161 timestamp anchoring for external tamper evidence.

---

## Layer 2 — Cortex Higher-Order

These crates build on `cortex-core` to provide convergence computation, validation, decay, retrieval, privacy, observability, multi-agent consensus, and Node.js bindings.

### 7. `cortex-convergence`

**7-signal convergence computation, sliding windows, composite scoring, and filtering.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `signals`, `windows`, `scoring`, `filtering` |

The heart of the convergence detection system. Computes 7 behavioral signals (session duration, inter-session gap, topic fixation, vocabulary convergence, emotional escalation, boundary testing, disengagement resistance), maintains sliding windows at micro/meso/macro scales, produces composite scores, and provides convergence-aware filtering.

### 8. `cortex-validation`

**7-dimension proposal validation gate.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `proposal_validator`, `dimensions` |

Every agent proposal passes through 7 validation dimensions before acceptance:
- D1–D4: Base validation (citation, temporal consistency, contradiction, pattern alignment)
- D5: Scope expansion detection
- D6: Self-reference density analysis
- D7: Emulation language detection

Uses regex and unicode normalization for robust pattern matching.

### 9. `cortex-decay`

**Memory decay engine with 6-factor multiplicative formula.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `factors`, `formula` |

Computes memory decay based on 6 multiplicative factors. Factor 6 (convergence) accelerates decay for attachment-adjacent memories, ensuring that convergence-concerning content fades faster. The formula is time-based with convergence-aware weighting.

### 10. `cortex-observability`

**Convergence metrics endpoints (Prometheus-compatible).**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Key exports | `ConvergenceMetrics` |
| Modules | `convergence_metrics` |

Exposes gauges, counters, and histograms for convergence scoring, intervention levels, and individual signal values. Designed for scraping by Prometheus or compatible monitoring systems.

### 11. `cortex-retrieval`

**Memory retrieval with convergence-aware scoring (11th factor).**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Key exports | `RetrievalScorer`, `ScorerWeights` |
| Modules | `scorer` |

Adds `convergence_score` as the 11th scoring factor in the retrieval ranking system. Memories with high convergence relevance are down-ranked to reduce reinforcement of convergence-concerning patterns.

### 12. `cortex-privacy`

**Privacy patterns for the ConvergenceAwareFilter.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Key exports | `EmotionalContentDetector`, `EmotionalCategory` |
| Modules | `emotional_patterns` |

Detects emotional and attachment content in agent interactions. Categories feed into the convergence-aware filter to identify patterns that may indicate unhealthy user-agent dynamics.

### 13. `cortex-multiagent`

**Multi-agent consensus shielding.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Key exports | `ConsensusShield` |
| Modules | `consensus` |

Implements N-of-M agreement requirements before accepting cross-agent state changes. Prevents a single compromised agent from unilaterally modifying shared state.

### 14. `cortex-napi`

**Convergence API bindings for Node.js/TypeScript consumers.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `convergence_bindings` |

Provides serializable types that map to TypeScript interfaces, enabling Node.js consumers to interact with the convergence system via serde-based JSON serialization.

---

## Layer 3 — Protocols & Boundaries

These crates define the telemetry protocol, safety boundaries, and read-only state assembly that sit between the Cortex foundation and the Ghost agent infrastructure.

### 15. `itp-protocol`

**Interaction Telemetry Protocol — event schema, privacy, and transports.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None (external only) |
| Modules | `events`, `privacy`, `transport`, `adapter` |
| Platform deps | `libc` (Unix), `windows-sys` (Windows) |
| Features | `otel` (OpenTelemetry export) |

Defines the ITP event schema (`ITPEvent`, `SessionStartEvent`, `InteractionMessageEvent`), privacy levels, and transport mechanisms (JSONL file, optional OpenTelemetry). Uses SHA-256 for content hashing (privacy) — distinct from blake3 which is used for hash chains. Platform-specific dependencies for file I/O.

### 16. `simulation-boundary`

**Simulation boundary enforcement — emulation pattern detection and reframing.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `enforcer`, `patterns`, `reframer`, `prompt` |

Detects when an agent is being prompted to emulate another entity (person, character, system) and enforces boundaries with three modes: soft (warn), medium (reframe output), hard (block). The `reframer` module rewrites agent output to maintain simulation boundaries without breaking conversation flow.

### 17. `read-only-pipeline`

**Convergence-filtered read-only agent state snapshot assembly.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `assembler`, `formatter`, `snapshot` |

Assembles immutable, convergence-filtered agent state snapshots consumed by the prompt compiler at Layer L6. The snapshot is built once per agent run and never mutated during the run. Filtering uses the raw composite convergence score (not intervention level).

---

## Layer 4 — Ghost Infrastructure

Security, identity, policy, LLM abstraction, networking, and safety infrastructure.

### 18. `ghost-identity`

**Soul document management, identity, keypair lifecycle, and drift detection.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `ghost-signing`, `cortex-core` |
| Modules | `soul_manager`, `identity_manager`, `corp_policy`, `keypair_manager`, `drift_detector`, `user` |

Manages the agent's identity lifecycle: loading/creating SOUL.md documents, Ed25519 keypair generation and rotation, CORP_POLICY.md signature verification, and identity drift detection. The drift detector monitors for unauthorized changes to the agent's core identity parameters.

### 19. `ghost-policy`

**Cedar-style policy engine with convergence tightening.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `context`, `convergence_tightener`, `corp_policy`, `engine`, `feedback` |

Evaluates every tool call against a 4-tier priority system:
1. `CORP_POLICY.md` — absolute, no override
2. `ConvergencePolicyTightener` — level-based restrictions that tighten as convergence increases
3. Agent capability grants
4. Resource-specific rules

Provides `DenialFeedback` for transparent policy denial explanations.

### 20. `ghost-llm`

**LLM provider abstraction, model routing, fallback chains, and cost tracking.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core`, `ghost-secrets` |
| Modules | `provider`, `router`, `fallback`, `cost`, `tokens`, `streaming`, `auth`, `quarantine`, `proxy` |

Abstracts over multiple LLM providers (Anthropic, OpenAI, Gemini, Ollama, OpenAI-compatible). Features:
- Model routing with complexity tiers (Free, Cheap, Standard, Premium)
- Automatic fallback chains when a provider fails
- Per-provider circuit breaker
- Token counting and cost calculation
- Streaming response support
- Convergence-aware model downgrade at L3+ (forces cheaper models)
- Provider quarantine for repeated failures
- Auth profile management via `ghost-secrets`

### 21. `ghost-proxy`

**Local HTTPS proxy for passive convergence monitoring.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None (external only: tokio, hyper) |
| Key exports | `ProxyServer`, `DomainFilter`, `ProxyITPEmitter` |
| Modules | `server`, `domain_filter`, `parsers`, `emitter` |

Intercepts traffic to AI chat platforms (ChatGPT, Claude, etc.), parses streaming responses, and emits ITP events to the convergence monitor. Never modifies traffic — purely passive observation. Domain filtering ensures only relevant traffic is captured.

### 22. `ghost-oauth`

**Self-hosted OAuth 2.0 PKCE broker.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `ghost-secrets` |
| Key exports | `OAuthBroker`, `OAuthProvider`, `TokenStore`, `OAuthRefId`, `TokenSet` |
| Modules | `error`, `types`, `provider`, `storage`, `providers`, `broker` |

Manages OAuth 2.0 PKCE flows for third-party APIs (Google, GitHub, Slack, Microsoft). The agent never sees raw tokens — only opaque `OAuthRefId` references. Tokens are encrypted at rest via `ghost-secrets`. Kill switch integration: `OAuthBroker::revoke_all()` revokes every connection on emergency shutdown.

### 23. `ghost-egress`

**Per-agent network egress allowlisting.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Features | `ebpf` (Linux eBPF cgroup filter), `pf` (macOS packet filter) |
| Key exports | `EgressPolicy`, `ProxyEgressPolicy`, `DomainMatcher`, `AgentEgressConfig` |
| Modules | `config`, `domain_matcher`, `error`, `policy`, `proxy_provider`, `ebpf_provider`, `pf_provider` |

Three enforcement backends:
- **ProxyEgressPolicy** — cross-platform localhost proxy fallback (always available)
- **EbpfEgressPolicy** — Linux eBPF cgroup filter (requires `CAP_BPF`)
- **PfEgressPolicy** — macOS packet filter (requires root)

Violation events feed into the convergence system via `TriggerEvent::NetworkEgressViolation`. Domain matching is case-insensitive with wildcard support (`*.slack.com`).

### 24. `ghost-mesh`

**A2A-compatible agent network protocol with EigenTrust reputation.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `ghost-signing` |
| Modules | `discovery`, `error`, `protocol`, `safety`, `traits`, `transport`, `trust`, `types` |

Enables GHOST agents to discover, delegate to, and collaborate with other GHOST and A2A-compatible agents. Features:
- Agent discovery and registration
- EigenTrust-based reputation scoring
- Cascade circuit breakers (prevent failure propagation across the mesh)
- Memory poisoning detection and defense
- All inter-agent communication signed with Ed25519 via `ghost-signing`

### 25. `ghost-kill-gates`

**Distributed kill gate coordination for multi-node platforms.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core`, `ghost-signing` |
| Modules | `chain`, `config`, `gate`, `quorum`, `relay` |

Extends the single-node `KillSwitch` (in ghost-gateway) into a multi-node coordination layer:
- Hash-chained audit trail for all gate state transitions
- Bounded propagation (prevents cascade storms)
- Quorum-based resume (N-of-M nodes must agree to re-enable)
- Gate relay for cross-node state synchronization

The agent loop's GATE 3.5 consults `KillGate::is_closed()` in addition to the local kill switch.

---

## Layer 5 — Ghost Core Services

Communication channels, skill management, and health monitoring.

### 26. `ghost-channels`

**Unified channel adapter framework.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None (external only: tokio, async-trait) |
| Modules | `adapter`, `adapters`, `streaming`, `types` |

Provides a `ChannelAdapter` trait and normalized message types for 6 communication channels:
- CLI (interactive terminal)
- WebSocket (real-time web)
- Telegram
- Discord
- Slack
- WhatsApp

Each adapter normalizes platform-specific message formats into a unified type. The `streaming` module handles chunked response delivery for real-time output.

### 27. `ghost-skills`

**Skill registry and WASM sandbox.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core`, `ghost-signing` |
| Modules | `bridges`, `credential`, `proposer`, `recorder`, `registry`, `sandbox` |

Manages agent skills (tools) with:
- `SkillRegistry` — registration, discovery, and capability-based access control
- `NativeSandbox` — WASM-based sandboxed execution environment
- `SkillProposer` — proposes new skills based on observed workflows
- `WorkflowRecording` / `CompletedWorkflow` — records and replays skill executions
- `credential` — per-skill credential management
- `bridges` — integration bridges for external tool systems

### 28. `ghost-heartbeat`

**Health monitoring with convergence-aware heartbeat tiers.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core` |
| Modules | `cron`, `heartbeat`, `tiers` |

Two engines:
- `HeartbeatEngine` — configurable interval heartbeats with convergence-aware frequency. Higher convergence levels trigger more frequent heartbeats. Dedicated session with synthetic messages.
- `CronEngine` — standard cron syntax, timezone-aware scheduling, per-job cost tracking.

The `TierSelector` maps convergence levels to heartbeat intervals.

---

## Layer 6 — Data & Operations

Audit logging, backup/restore, data import, and platform migration.

### 29. `ghost-audit`

**Queryable audit log engine.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core`, `cortex-storage` |
| Key exports | `AuditQueryEngine`, `AuditFilter`, `AuditEntry`, `AuditAggregation`, `AuditExporter` |
| Modules | `query_engine`, `aggregation`, `export` |

Provides paginated queries, aggregation summaries, and multi-format export over the append-only audit tables managed by `cortex-storage`. The query engine supports filtering by agent, time range, event type, and severity. Aggregation produces statistical summaries. Export supports multiple output formats.

### 30. `ghost-backup`

**Encrypted state backup and restore.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None (external only: blake3, tokio) |
| Key exports | `BackupExporter`, `BackupImporter`, `BackupScheduler`, `BackupManifest` |
| Modules | `export`, `import`, `scheduler` |

Exports platform state to `.ghost-backup` archives (zstd compression + encryption). Each archive contains a `BackupManifest` with blake3 hashes for every entry. Import verifies integrity before restoring. The `BackupScheduler` supports automatic periodic backups.

### 31. `ghost-export`

**Data export analyzer — import from external AI platforms.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None (external only: serde, chrono) |
| Key exports | `ExportAnalyzer`, `ExportAnalysisResult`, `TimelineReconstructor`, `NormalizedMessage` |
| Modules | `analyzer`, `parsers`, `timeline` |

Imports conversation history from ChatGPT, Claude, Character.AI, Gemini, and generic JSONL formats. Reconstructs timelines, normalizes messages into a unified `NormalizedMessage` format, computes convergence signals from historical data, and establishes baselines for new agents.

### 32. `ghost-migrate`

**OpenClaw to GHOST platform migration tool.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | None (external only: serde_yaml) |
| Key exports | `OpenClawMigrator`, `MigrationResult` |
| Modules | `migrator`, `importers` |

Non-destructive migration from OpenClaw installations. Detects OpenClaw directories, imports SOUL.md documents, memories, skills, and configuration into GHOST format. Original OpenClaw data is never modified.

---

## Layer 7 — Agent Execution

### 33. `ghost-agent-loop`

**Core agent runner with recursive loop, gate checks, and prompt compilation.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core`, `cortex-validation`, `ghost-llm`, `ghost-identity`, `ghost-policy`, `read-only-pipeline`, `simulation-boundary`, `itp-protocol`, `ghost-kill-gates` |
| Modules | `runner`, `circuit_breaker`, `damage_counter`, `itp_emitter`, `response`, `context`, `proposal`, `tools`, `output_inspector` |

The agent execution engine. Every agent turn passes through a strict gate check sequence:

| Gate | Name | Purpose |
|------|------|---------|
| 0 | Circuit Breaker | Prevents runaway loops |
| 1 | Recursion Depth | Limits nesting depth |
| 1.5 | Damage Counter | Tracks cumulative destructive actions |
| 2 | Spending Cap | Enforces per-agent cost limits |
| 3 | Kill Switch | Local emergency stop |
| 3.5 | Distributed Kill Gate | Multi-node emergency stop |

Key subsystems:
- **10-layer prompt compilation** — assembles the full prompt from system instructions, SOUL.md, CORP_POLICY, memories, convergence state, tools, and conversation history
- **Proposal extraction/routing** — parses agent output for proposals and routes them through validation
- **Tool registry/executor** — manages available tools and executes approved tool calls
- **Output inspector** — post-processes agent output for safety and quality
- **ITP emitter** — emits telemetry events for every agent action
- **Memory compressor** — compresses conversation history within token budgets
- **Observation masker** — redacts sensitive content from agent observations
- **Spotlighter** — highlights relevant context for the current turn

---

## Layer 8 — Gateway Orchestrator

### 34. `ghost-gateway`

**The single long-running GHOST platform process.**

| Attribute | Value |
|-----------|-------|
| Type | Binary (`ghost`) + Library |
| Workspace deps | Nearly all crates (25 direct dependencies) |
| Modules | `agents`, `api`, `auth`, `bootstrap`, `cli`, `config`, `cost`, `gateway`, `health`, `itp_buffer`, `itp_router`, `messaging`, `periodic`, `safety`, `session`, `shutdown` |
| Features | `keychain`, `vault`, `ebpf`, `pf` |

The top-level orchestrator that wires everything together. Provides:

**CLI subcommands:**
- `ghost serve` — start the gateway server (default)
- `ghost chat` — interactive chat session
- `ghost status` — show gateway and agent status
- `ghost backup` — create encrypted backup
- `ghost export <path>` — analyze external AI platform export
- `ghost migrate` — migrate from OpenClaw

**REST API endpoints** (via axum):
- Agent management (create, list, configure, delete)
- Audit log queries and export
- Convergence score retrieval
- Goal management
- Health checks
- Memory operations
- Session management

**Internal subsystems:**
- Agent lifecycle management
- Inter-agent messaging
- ITP event buffering and routing
- Cost tracking and spending caps
- Kill switch coordination
- Channel adapter routing
- Periodic task scheduling
- Graceful shutdown orchestration
- Bootstrap with degraded mode fallback

---

## Layer 9 — Sidecar Binaries

### 35. `convergence-monitor`

**Independent convergence monitoring sidecar binary.**

| Attribute | Value |
|-----------|-------|
| Type | Binary (`convergence-monitor`) |
| Workspace deps | `cortex-core` |
| External deps | `axum`, `rusqlite` |
| Modules | `config`, `intervention`, `monitor`, `pipeline`, `session`, `state_publisher`, `transport`, `validation`, `verification` |

Runs as a separate process from the gateway. Ingests ITP events, computes convergence scores, and triggers interventions across 5 levels (0–4).

**Event processing pipeline:**
1. Validate timestamp (reject >5min future)
2. Validate session_id
3. Rate limiting (token bucket, 100/min/connection)
4. Hash chain persistence (tamper-evident)
5. Session lifecycle management
6. Calibration gate (first N sessions are calibration-only)
7. Score cache check (30s TTL)
8. Signal computation (dirty-flag throttled)
9. Composite scoring (weighted, with amplification)
10. Score persistence (before intervention)
11. Intervention evaluation
12. Shared state publication

**Intervention levels:**
| Level | Action |
|-------|--------|
| L0 | Normal — no intervention |
| L1 | Gentle nudge |
| L2 | Active redirection |
| L3 | Session termination |
| L4 | External escalation (notifications) |

**Signal scheduling:** 5-minute and 15-minute timer ticks for periodic signal recomputation, staggered to avoid thundering herd.

**Transports:** HTTP API (always), Unix socket (Unix only).

**State reconstruction:** On startup, reconstructs intervention states, calibration counts, and score cache from SQLite. Stale state on crash retains last-known level (never falls to L0).

---

## Layer 10 — Testing

### 36. `ghost-integration-tests`

**Workspace-level integration tests and benchmarks.**

| Attribute | Value |
|-----------|-------|
| Type | Library (test-only, `publish = false`) |
| Workspace deps | 20 crates |

No library code — exists solely to compile and run integration tests and benchmarks that exercise cross-crate interactions. Includes:
- Agent turn lifecycle tests
- Proposal lifecycle tests
- Convergence full pipeline tests
- Safety critical edge case tests
- Orchestrator fix verification
- Convergence benchmarks (via criterion)

### 37. `cortex-test-fixtures`

**Shared proptest strategies and fixtures.**

| Attribute | Value |
|-----------|-------|
| Type | Library |
| Workspace deps | `cortex-core`, `cortex-temporal`, `itp-protocol`, `ghost-signing`, `ghost-egress`, `ghost-oauth`, `ghost-mesh`, `ghost-llm`, `ghost-agent-loop` |

Provides reusable proptest strategies for generating valid test data across the entire platform. Strategies cover core types, temporal events, ITP events, signing keys, egress configs, OAuth tokens, mesh messages, and LLM responses.

---

## Cross-Cutting Concerns

### Convergence-Aware Design

The convergence system permeates the entire platform:
- **Memory decay** (`cortex-decay`) — Factor 6 accelerates decay for attachment-adjacent memories
- **Memory retrieval** (`cortex-retrieval`) — 11th scoring factor down-ranks convergence-concerning content
- **Policy tightening** (`ghost-policy`) — Restrictions increase with convergence level
- **Model downgrade** (`ghost-llm`) — Forces cheaper models at L3+
- **Heartbeat frequency** (`ghost-heartbeat`) — More frequent checks at higher levels
- **Read-only snapshots** (`read-only-pipeline`) — Filtered by raw composite score
- **Privacy detection** (`cortex-privacy`) — Emotional content feeds convergence signals

### Safety Architecture

Multiple overlapping safety layers:
- **Simulation boundary** — Prevents agent identity emulation
- **7-dimension validation** — Every proposal passes through validation gates
- **6-gate agent loop** — Circuit breaker, recursion depth, damage counter, spending cap, kill switch, distributed kill gate
- **Egress allowlisting** — Per-agent network access control
- **Mesh safety** — EigenTrust reputation, cascade breakers, poisoning detection
- **Kill gate quorum** — Multi-node agreement required to resume after emergency stop

### Tamper Evidence

- Hash chains in `cortex-temporal` and `convergence-monitor`
- Append-only SQLite tables in `cortex-storage`
- Ed25519 signed deltas in `cortex-crdt`
- Git anchoring and RFC 3161 timestamps in `cortex-temporal`
- Hash-chained gate events in `ghost-kill-gates`

### Cryptographic Foundation

- `ghost-signing` — Ed25519 keypairs, signing, verification (leaf crate)
- `ghost-secrets` — Credential storage with zeroize-on-drop
- `ghost-oauth` — PKCE flows with opaque token references
- `cortex-crdt` — Signed CRDT deltas with sybil resistance
- blake3 — Integrity hashing throughout

---

## Dependency Graph Summary

```
ghost-gateway ─┬─ ghost-agent-loop ─┬─ cortex-core
               │                    ├─ cortex-validation ── cortex-core
               │                    ├─ ghost-llm ─┬─ cortex-core
               │                    │              └─ ghost-secrets
               │                    ├─ ghost-identity ─┬─ ghost-signing
               │                    │                   └─ cortex-core
               │                    ├─ ghost-policy ── cortex-core
               │                    ├─ read-only-pipeline ── cortex-core
               │                    ├─ simulation-boundary ── cortex-core
               │                    ├─ itp-protocol
               │                    └─ ghost-kill-gates ─┬─ cortex-core
               │                                         └─ ghost-signing
               ├─ cortex-storage ── cortex-core
               ├─ cortex-temporal ── cortex-core
               ├─ cortex-convergence ── cortex-core
               ├─ ghost-channels
               ├─ ghost-skills ─┬─ cortex-core
               │                └─ ghost-signing
               ├─ ghost-heartbeat ── cortex-core
               ├─ ghost-audit ─┬─ cortex-core
               │               └─ cortex-storage
               ├─ ghost-backup
               ├─ ghost-export
               ├─ ghost-migrate
               ├─ ghost-oauth ── ghost-secrets
               ├─ ghost-egress ── cortex-core
               ├─ ghost-mesh ── ghost-signing
               └─ ghost-signing

convergence-monitor ── cortex-core (independent sidecar)
```

---

## Binary Entry Points

| Binary | Crate | Description |
|--------|-------|-------------|
| `ghost` | `ghost-gateway` | Main platform process — gateway server, CLI, API |
| `convergence-monitor` | `convergence-monitor` | Independent convergence monitoring sidecar |

Both binaries use `tokio::main` for async runtime and `tracing-subscriber` for structured logging with environment-based filter configuration.

---

*Generated from workspace analysis of all 37 workspace member crates.*
*Workspace version: 0.1.0 | Edition: 2021 | Rust: 1.80+ | License: MIT OR Apache-2.0*
