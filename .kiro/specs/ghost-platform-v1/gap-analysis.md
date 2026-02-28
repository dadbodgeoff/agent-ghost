# GHOST Platform v1 — Gap Analysis & Build Plan

> Generated: 2026-02-28
> Scope: Everything missing, incomplete, or needing wiring — mapped against tasks.md (47 tasks, 9 phases), requirements.md (41 requirements), design.md, and FILE_MAPPING.md.
> Method: Full crate-by-crate inventory of actual source code vs spec expectations.

---

## 1. Executive Summary

The GHOST Platform has 25 Rust crates in the workspace, all with real implementations (27K+ lines total). Supporting infrastructure (browser extension, SvelteKit dashboard, deploy configs, schemas, ghost.yml) exists. The codebase is architecturally sound — the gap is not "build from scratch" but rather:

- **Missing non-Rust deliverables**: CI/CD, root config files, CORP_POLICY.md, SECURITY.md, docs/
- **Missing crate**: `cortex-test-fixtures` (shared proptest strategies), `ghost-mesh` (placeholder)
- **Missing crate modifications**: 5 existing cortex crates need convergence-related additions
- **Missing test suites**: Adversarial tests, property tests (Req 41), e2e integration, benchmarks
- **Missing cross-crate wiring**: Several integration flows not yet connected end-to-end

---

## 2. Crate Implementation Status (25/25 Implemented)

All 25 workspace crates have real, functional implementations. Here's the per-crate depth:

| # | Crate | Lines | Status | Key Modules |
|---|-------|-------|--------|-------------|
| 1 | ghost-signing | ~350 | ✅ Complete | keypair, signer, verifier, zeroize |
| 2 | cortex-core | ~1200 | ✅ Complete | memory/types/convergence, config/convergence_config, traits/convergence, safety/trigger, models/caller |
| 3 | cortex-storage | ~800 | ✅ Complete | migrations v016-v018, 6 query modules, append-only triggers |
| 4 | cortex-temporal | ~500 | ✅ Complete | hash_chain (GENESIS_HASH, compute, verify), anchoring/merkle |
| 5 | cortex-decay | ~400 | ✅ Complete | convergence factor, formula, DecayContext.convergence |
| 6 | cortex-convergence | ~900 | ✅ Complete | 7 signals, sliding windows, composite scoring, baseline, filtering |
| 7 | cortex-validation | ~700 | ✅ Complete | D1-D7 proposal validator, scope expansion, self-ref, emulation |
| 8 | cortex-crdt | ~500 | ✅ Complete | signing (ed25519-dalek direct), sybil guard |
| 9 | itp-protocol | ~700 | ✅ Complete | 5 event types, privacy levels, JSONL transport, adapter trait |
| 10 | simulation-boundary | ~580 | ✅ Complete | enforcer, patterns, reframer, prompt (compiled const) |
| 11 | convergence-monitor | ~2750 | ✅ Complete | pipeline, intervention (5-level), session, transport, verification |
| 12 | ghost-policy | ~1050 | ✅ Complete | engine, convergence_tightener, corp_policy, feedback |
| 13 | read-only-pipeline | ~560 | ✅ Complete | assembler, snapshot, formatter |
| 14 | ghost-llm | ~980 | ✅ Complete | 5 providers, router, fallback, cost, tokens, streaming |
| 15 | ghost-identity | ~870 | ✅ Complete | soul_manager, keypair_manager, drift_detector, corp_policy |
| 16 | ghost-agent-loop | ~2880 | ✅ Complete | runner (5 gates), circuit_breaker, damage_counter, prompt_compiler, proposal, tools, output_inspector |
| 17 | ghost-gateway | ~4700 | ✅ Complete | 6-state FSM, bootstrap, shutdown, safety/kill_switch, messaging, session, api, cli |
| 18 | ghost-channels | ~540 | ✅ Complete | adapter trait, 6 adapters (CLI/WS/Telegram/Discord/Slack/WhatsApp), streaming |
| 19 | ghost-skills | ~820 | ✅ Complete | registry, wasm_sandbox, native_sandbox, credential broker |
| 20 | ghost-heartbeat | ~560 | ✅ Complete | heartbeat engine, cron engine |
| 21 | ghost-audit | ~620 | ✅ Complete | query_engine, aggregation, export (JSON/CSV/JSONL) |
| 22 | ghost-backup | ~510 | ✅ Complete | export (zstd+encrypt), import, scheduler |
| 23 | ghost-export | ~930 | ✅ Complete | 5 parsers (ChatGPT/Claude/CharacterAI/Gemini/JSONL), timeline |
| 24 | ghost-proxy | ~530 | ✅ Complete | server (rustls), domain_filter, 4 SSE/WS parsers, emitter |
| 25 | ghost-migrate | ~530 | ✅ Complete | migrator, 4 importers (soul/memory/skill/config) |

**Total Rust**: ~27,180 lines across 25 crates, 845+ public API items.

---

## 3. Non-Rust Deliverables Status

| Deliverable | Status | Notes |
|-------------|--------|-------|
| Browser extension (`extension/`) | ✅ Exists | Chrome + Firefox manifests, content scripts, popup, dashboard |
| SvelteKit dashboard (`dashboard/`) | ✅ Exists | package.json, svelte.config.js, routes, components |
| Deploy configs (`deploy/`) | ✅ Exists | Dockerfile, docker-compose.yml, docker-compose.prod.yml, ghost.service |
| Schemas (`schemas/`) | ✅ Exists | ghost-config.schema.json, ghost-config.example.yml |
| Root ghost.yml | ✅ Exists | Gateway, agents, channels, convergence, backup config |
| Architecture docs (root) | ✅ Exists | 10 detailed sequence flow docs (~1.1MB total) |
| `docs/` directory | ❌ Missing | getting-started, configuration, skill-authoring, channel-adapters, convergence-safety, architecture |
| `.github/workflows/` | ❌ Missing | ci.yml, release.yml, security-audit.yml, benchmark.yml |
| `.github/CODEOWNERS` | ❌ Missing | Ownership mapping for safety-critical paths |
| `deny.toml` | ❌ Missing | cargo-deny license/advisory config |
| `rustfmt.toml` | ❌ Missing | Workspace formatting rules |
| `clippy.toml` | ❌ Missing | Workspace lint rules |
| `SECURITY.md` | ❌ Missing | Security policy + disclosure process |
| `CORP_POLICY.md` | ❌ Missing | Root corporate policy document (referenced by ghost-policy, ghost-identity) |
| Baileys WhatsApp bridge | ❌ Missing | `extension/bridges/baileys-bridge/` Node.js sidecar |

---

## 4. Gap Inventory

### GAP-01: `cortex-test-fixtures` crate (NEW)
- **Task**: 7.2
- **Req**: 41 (all 17 AC)
- **What**: Shared proptest strategy library consumed by all crates' property tests
- **Files needed**:
  - `crates/cortex/test-fixtures/Cargo.toml`
  - `crates/cortex/test-fixtures/src/lib.rs`
  - `crates/cortex/test-fixtures/src/strategies.rs` — 12+ concrete strategies
  - `crates/cortex/test-fixtures/src/fixtures.rs` — golden dataset loaders
  - `crates/cortex/test-fixtures/src/helpers.rs` — test helper functions
  - `crates/cortex/test-fixtures/golden/` — convergence trajectory JSON files
- **Strategies needed**:
  - `memory_type_strategy()`, `importance_strategy()`, `convergence_score_strategy()`
  - `signal_array_strategy()`, `event_chain_strategy()`, `convergence_trajectory_strategy()`
  - `proposal_strategy()`, `trigger_event_strategy()`, `agent_message_strategy()`
  - `session_history_strategy()`, `kill_state_strategy()`, `gateway_state_transition_strategy()`
- **Effort**: ~400 lines
- **Priority**: HIGH — blocks all property testing (Task 7.2)

### GAP-02: `ghost-mesh` placeholder crate (NEW)
- **Task**: 9.1
- **What**: Placeholder crate with trait definitions only — no implementation
- **Files needed**:
  - `crates/ghost-mesh/Cargo.toml`
  - `crates/ghost-mesh/src/lib.rs`
  - `crates/ghost-mesh/src/types.rs` — MeshPayment, MeshInvoice, MeshSettlement stubs
  - `crates/ghost-mesh/src/traits.rs` — PaymentProtocol trait
  - `crates/ghost-mesh/src/protocol.rs` — protocol constants
- **Note**: Commented-out workspace member in root Cargo.toml
- **Effort**: ~100 lines
- **Priority**: LOW — Phase 9 deferred item

### GAP-03: cortex-observability modifications
- **Task**: 7.4
- **What**: Add convergence metrics endpoints (Prometheus gauges/counters/histograms)
- **Crate**: `crates/cortex/cortex-observability/` (if exists) or new module
- **Note**: This crate is NOT in the workspace Cargo.toml members list. Either it doesn't exist yet or it's an external crate. Need to determine if it should be added as a workspace member or if convergence metrics go into convergence-monitor's HTTP API.
- **Effort**: ~200 lines
- **Priority**: MEDIUM

### GAP-04: cortex-retrieval modifications
- **Task**: 7.4
- **What**: Add `convergence_score` as 11th scoring factor in ScorerWeights
- **Crate**: Not in workspace — likely an existing external cortex crate
- **Effort**: ~50 lines
- **Priority**: MEDIUM

### GAP-05: cortex-privacy modifications
- **Task**: 7.4
- **What**: Add emotional/attachment content patterns for ConvergenceAwareFilter
- **Crate**: Not in workspace — likely an existing external cortex crate
- **Effort**: ~100 lines
- **Priority**: MEDIUM

### GAP-06: cortex-multiagent modifications
- **Task**: 7.4
- **What**: ConsensusShield for multi-source validation
- **Crate**: Not in workspace — likely an existing external cortex crate
- **Effort**: ~150 lines
- **Priority**: MEDIUM

### GAP-07: cortex-napi modifications
- **Task**: 7.4
- **What**: Convergence API bindings (TypeScript types via ts-rs, NAPI functions)
- **Crate**: Not in workspace — likely an existing external cortex crate
- **Effort**: ~200 lines
- **Priority**: LOW — only needed for Node.js consumers


### GAP-08: Baileys WhatsApp Bridge (NEW)
- **Task**: 5.7
- **What**: Node.js JSON-RPC stdin/stdout bridge to WhatsApp Web
- **Files needed**:
  - `extension/bridges/baileys-bridge/package.json` — baileys dependency
  - `extension/bridges/baileys-bridge/baileys-bridge.js` — JSON-RPC bridge script
- **How it works**: Spawned by WhatsAppAdapter on connect(). Health monitoring via heartbeat. Requires Node.js 18+. Restart up to 3 times on crash.
- **Effort**: ~200 lines (JS)
- **Priority**: MEDIUM — WhatsApp channel won't work without it

### GAP-09: CORP_POLICY.md (NEW)
- **Task**: 4.2, 3.4
- **What**: Root corporate policy document consumed by ghost-policy engine and ghost-identity CorpPolicyLoader
- **Location**: Root `CORP_POLICY.md` or `~/.ghost/CORP_POLICY.md`
- **Content**: Platform safety rules, tool restrictions, data handling policies, convergence override rules
- **Signed**: Must be Ed25519 signed — ghost-identity refuses to load unsigned/invalid
- **Effort**: ~100 lines (policy text) + signing tooling
- **Priority**: HIGH — ghost-policy and ghost-identity reference it directly

### GAP-10: `docs/` directory (NEW)
- **Task**: 7.5
- **Files needed**:
  - `docs/getting-started.md` — Installation, first agent setup, ghost.yml basics
  - `docs/configuration.md` — Full ghost.yml reference, env var substitution, profiles
  - `docs/skill-authoring.md` — Writing skills, YAML frontmatter, signing, WASM sandbox
  - `docs/channel-adapters.md` — Setting up Telegram, Discord, Slack, WhatsApp, WebSocket
  - `docs/convergence-safety.md` — How convergence monitoring works, intervention levels, tuning
  - `docs/architecture.md` — High-level architecture overview for contributors
- **Effort**: ~2000 lines (documentation)
- **Priority**: LOW — not blocking any code

### GAP-11: CI/CD Workflows (NEW)
- **Task**: 8.3
- **Files needed**:
  - `.github/workflows/ci.yml` — fmt, clippy, test, deny, npm lint
  - `.github/workflows/release.yml` — tagged release, cross-compile (4 targets), npm build
  - `.github/workflows/security-audit.yml` — daily cargo audit + cargo deny
  - `.github/workflows/benchmark.yml` — Criterion benchmarks on PR, fail on >10% regression
  - `.github/CODEOWNERS` — safety-critical path ownership
- **Effort**: ~400 lines (YAML)
- **Priority**: MEDIUM — needed before any real CI

### GAP-12: Root Config Files (NEW)
- **Task**: 8.3
- **Files needed**:
  - `deny.toml` — license allowlist (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib), advisory DB, duplicate detection
  - `rustfmt.toml` — edition=2021, max_width=100, use_field_init_shorthand=true
  - `clippy.toml` — cognitive-complexity-threshold, too-many-arguments-threshold
  - `SECURITY.md` — security policy, vulnerability disclosure, supported versions, contact
- **Effort**: ~150 lines
- **Priority**: MEDIUM

---

## 5. Missing Test Suites

### GAP-13: Adversarial Test Suites
- **Task**: 7.3
- **Req**: 32 AC7
- **Files needed**:
  - `tests/adversarial/unicode_bypass.rs` — zero-width chars, homoglyphs, RTL override, NFC/NFD against simulation boundary
  - `tests/adversarial/proposal_adversarial.rs` — max self-reference, scope expansion at boundary, re-proposal after rejection
  - `tests/adversarial/kill_switch_race.rs` — concurrent trigger delivery, dedup under load
  - `tests/adversarial/compaction_under_load.rs` — compaction with simultaneous message arrival
  - `tests/adversarial/credential_exfil_patterns.rs` — known credential patterns, encoding tricks, partial leaks
  - `tests/adversarial/convergence_manipulation.rs` — attempts to game scoring via crafted ITP events
- **Effort**: ~800 lines
- **Priority**: HIGH — safety-critical validation

### GAP-14: End-to-End Integration Tests
- **Task**: 8.1
- **What**: Cross-crate integration tests covering full lifecycles:
  - Full agent turn lifecycle (message → routing → gates → prompt → LLM → response → proposal → delivery → ITP → compaction)
  - Full kill switch chain (detection → trigger → eval → dedup → classify → execute → notify → audit)
  - Full convergence pipeline (ITP → monitor → signals → scoring → intervention → shared state → gateway → policy)
  - Full proposal lifecycle (output → extract → context → 7-dim validate → decide → commit/reject → feedback)
  - Full compaction lifecycle (threshold → snapshot → flush → compress → CompactionBlock → verify)
  - Full inter-agent messaging (compose → sign → dispatch → verify → deliver → ack)
  - Gateway bootstrap → degraded → recovery → healthy
  - Multi-agent scenarios (3 agents, convergence isolation, T6 cascade)
- **Effort**: ~1500 lines
- **Priority**: HIGH — validates the entire system works together

### GAP-15: Performance Benchmarks
- **Task**: 8.2
- **What**: Criterion benchmarks for critical paths:
  - Hash chain computation: 10K events/sec target
  - Convergence signal computation: 7 signals in <10ms
  - Composite scoring: <1ms per score
  - Proposal validation (7 dimensions): <50ms per proposal
  - Simulation boundary scan: <5ms per scan
  - Monitor event ingestion: 10K events/sec target
  - Prompt compilation (10 layers): <100ms
  - Kill switch check: <1μs (atomic read)
  - Message signing + verification: <1ms per message
  - MerkleTree proof generation: <10ms for 10K leaves
- **Effort**: ~600 lines
- **Priority**: MEDIUM — needed for CI regression detection

---

## 6. Cross-Crate Wiring Gaps

These are integration points where individual crates are implemented but the wiring between them may not be fully connected.

### GAP-16: ITP Flow (Agent Loop → Monitor)
- **What**: ghost-agent-loop emits ITP events via bounded channel → convergence-monitor ingests via unix socket
- **Status**: Both sides implemented independently. Need to verify:
  - ITPEmitter in ghost-agent-loop connects to monitor's unix socket
  - Event format matches between emitter and monitor's ingest parser
  - Bounded channel (capacity 1000) with try_send drop semantics
- **Risk**: LOW — both use itp-protocol types

### GAP-17: Kill Switch Chain (Gateway ↔ Agent Loop)
- **What**: ghost-gateway KillSwitch sets PLATFORM_KILLED AtomicBool → ghost-agent-loop checks at GATE 3
- **Status**: Both sides implemented. Need to verify:
  - Shared AtomicBool reference passed during bootstrap
  - SeqCst ordering on both sides
  - AutoTriggerEvaluator receives TriggerEvents from cortex-core safety module
- **Risk**: LOW — uses atomic flag pattern

### GAP-18: Proposal Lifecycle (Agent Loop → Validation → Storage)
- **What**: ProposalExtractor → ProposalRouter → ProposalValidator → cortex-storage
- **Status**: All components exist. Need to verify:
  - ProposalRouter assembles ProposalContext with all 10 fields
  - Atomic transaction: proposal INSERT + memory commit in same SQLite transaction
  - DenialFeedback appears in Layer L6 (convergence state), NOT Layer L8 (history)
  - Score caching with 30s TTL
- **Risk**: MEDIUM — complex multi-step flow

### GAP-19: Compaction Flow (Gateway → Agent Loop → Storage)
- **What**: SessionCompactor triggers at 70% → FlushExecutor (AgentRunner) → cortex-storage
- **Status**: FlushExecutor trait defined in ghost-agent-loop. Need to verify:
  - SessionCompactor in ghost-gateway uses FlushExecutor trait (not direct AgentRunner dep)
  - CompactionBlock never re-compressed
  - Per-type compression minimums enforced
  - Policy denials during flush don't increment CircuitBreaker
- **Risk**: MEDIUM — circular dependency break via trait

### GAP-20: Bootstrap + Degraded Mode (Gateway → Monitor)
- **What**: Gateway bootstrap checks monitor health → Degraded if unreachable → ITP buffer → Recovery
- **Status**: Gateway FSM and MonitorHealthChecker exist. Need to verify:
  - ITPBuffer disk-backed at ~/.ghost/sessions/buffer/
  - Recovery replays buffered events in batches of 100
  - Stale state conservative (retain last-known level, never fall to L0)
- **Risk**: MEDIUM — complex state machine

### GAP-21: Convergence State Publication (Monitor → Gateway)
- **What**: Monitor writes atomic JSON to ~/.ghost/data/convergence_state/{agent_id}.json → Gateway polls at 1s
- **Status**: StatePublisher in monitor exists. Need to verify:
  - Atomic write (temp file + rename)
  - Gateway's SessionBoundaryProxy reads from shared state file
  - Stale file handling (monitor crash → gateway retains last-known level)
- **Risk**: LOW — file-based IPC is simple

### GAP-22: Read-Only Pipeline → Prompt Compiler
- **What**: read-only-pipeline assembles AgentSnapshot → ghost-agent-loop PromptCompiler consumes at L6
- **Status**: Both exist. Need to verify:
  - Snapshot assembled ONCE pre-loop, never re-read mid-run (Hazard 1)
  - Filtering uses RAW composite score (not intervention level)
  - SnapshotFormatter output fits within L6 token budget (1000 tokens)
- **Risk**: LOW

### GAP-23: Inter-Agent Messaging Signing
- **What**: ghost-gateway MessageDispatcher signs with ghost-signing → cortex-crdt KeyRegistry verifies
- **Status**: Both signing systems exist independently. Need to verify:
  - Dual key registration during bootstrap (MessageDispatcher + cortex-crdt KeyRegistry)
  - canonical_bytes() deterministic across sender/receiver
  - 3-gate verification pipeline (signature → replay → policy)
- **Risk**: MEDIUM — dual registration is a subtle requirement

### GAP-24: Convergence Tightening → Policy → Agent Loop
- **What**: Intervention level flows from monitor → shared state → gateway → PolicyEngine → agent loop tool filtering
- **Status**: All components exist. Need to verify:
  - ConvergencePolicyTightener correctly restricts at each level (L2: reduce proactive, L3: session cap, L4: task-only)
  - Tool schema filtering in PromptCompiler L3 matches policy restrictions
  - Compaction flush exception (memory_write always permitted during flush)
- **Risk**: LOW — policy engine has clear priority chain


---

## 7. Correctness Properties Not Yet Tested (Req 41)

These 17 properties are defined in requirements.md and tasks.md Task 7.2. Each must be validated via proptest with 1000+ cases. None have dedicated property test files yet.

| # | Property | Crate(s) | Invariant |
|---|----------|----------|-----------|
| 1 | Kill monotonicity | ghost-gateway | Kill level never decreases without owner resume |
| 2 | Kill determinism | ghost-gateway | Same TriggerEvent sequence → same final state |
| 3 | Kill completeness | ghost-gateway | audit entries = trigger events |
| 4 | Kill consistency | ghost-gateway | PLATFORM_KILLED=true ↔ state=KillAll |
| 5 | Session serialization | ghost-gateway | At most 1 operation per session at any time |
| 6 | Message preservation | ghost-gateway | Messages during compaction enqueued, not dropped |
| 7 | Compaction isolation | ghost-gateway | Other sessions not blocked by one session's compaction |
| 8 | Cost completeness | ghost-gateway | Compaction flush cost tracked |
| 9 | Compaction atomicity | ghost-gateway | Complete fully or roll back |
| 10 | Audit-before-action | convergence-monitor | Score persisted before intervention trigger |
| 11 | Signing determinism | ghost-gateway | canonical_bytes identical on sender and receiver |
| 12 | Validation ordering | cortex-validation | D1-D4 before D5-D7 |
| 13 | Gateway transitions | ghost-gateway | Only valid transitions permitted |
| 14 | Signal range | cortex-convergence | All signals in [0.0, 1.0] |
| 15 | Tamper detection | cortex-temporal | Any byte modification → verify_chain fails |
| 16 | Convergence bounds | cortex-convergence | Score always in [0.0, 1.0] |
| 17 | Decay monotonicity | cortex-decay | Convergence factor always >= 1.0 |

Additional properties from A26 addenda:
- trigger_deduplication: same trigger within 60s suppressed
- state_persistence_roundtrip: kill_state.json write/read identical
- kill_all_stops_everything: after KILL_ALL, no agent operation succeeds
- quarantine_isolates_agent: quarantined agent cannot send/receive
- signing_roundtrip: sign then verify for all payload variants
- hash_chain_integrity: append then verify for arbitrary sequences
- compaction_token_reduction: post < pre tokens
- hash_algorithm_separation: ITP uses SHA-256, hash chains use blake3 — never confused

---

## 8. OpenClaw → GHOST Architectural Mapping

How GHOST maps to OpenClaw's architecture:

| OpenClaw Concept | GHOST Equivalent | Status |
|-----------------|------------------|--------|
| Gateway server (port 18789) | ghost-gateway (port 18789) | ✅ Implemented |
| Agent Runner | ghost-agent-loop AgentRunner | ✅ Implemented |
| Agentic Loop (recursive) | AgentRunner::run() with 5 gates | ✅ Implemented |
| SOUL.md / IDENTITY.md | ghost-identity SoulManager + IdentityManager | ✅ Implemented |
| MEMORY.md | cortex-core + cortex-storage (typed memories) | ✅ Implemented (upgraded to typed system) |
| Markdown-based skills | ghost-skills SkillRegistry + WASM sandbox | ✅ Implemented (upgraded to WASM) |
| Multi-channel (WhatsApp/Telegram/Discord/Slack) | ghost-channels (6 adapters) | ✅ Implemented |
| Session compaction | ghost-gateway SessionCompactor | ✅ Implemented (upgraded: 5-phase, per-type minimums) |
| Heartbeat | ghost-heartbeat HeartbeatEngine | ✅ Implemented (upgraded: convergence-aware frequency) |
| Cron jobs | ghost-heartbeat CronEngine | ✅ Implemented |
| Tool execution | ghost-agent-loop ToolRegistry + ToolExecutor | ✅ Implemented |
| Cost tracking | ghost-gateway CostTracker + SpendingCapEnforcer | ✅ Implemented |
| — (no equivalent) | Convergence monitoring (7 signals, 5 levels) | ✅ NEW in GHOST |
| — (no equivalent) | Proposal validation (7 dimensions) | ✅ NEW in GHOST |
| — (no equivalent) | Simulation boundary enforcement | ✅ NEW in GHOST |
| — (no equivalent) | Kill switch (3 levels, 7 auto-triggers) | ✅ NEW in GHOST |
| — (no equivalent) | Policy engine (Cedar-style, convergence tightening) | ✅ NEW in GHOST |
| — (no equivalent) | Hash chain tamper evidence | ✅ NEW in GHOST |
| — (no equivalent) | CRDT signed deltas + sybil resistance | ✅ NEW in GHOST |
| — (no equivalent) | Inter-agent messaging (signed, encrypted) | ✅ NEW in GHOST |
| — (no equivalent) | Browser extension (passive monitoring) | ✅ NEW in GHOST |
| — (no equivalent) | HTTPS proxy (passive monitoring) | ✅ NEW in GHOST |
| — (no equivalent) | Data export analyzer (5 platforms) | ✅ NEW in GHOST |
| — (no equivalent) | OpenClaw migration tool | ✅ NEW in GHOST |

**Key GHOST additions over OpenClaw**:
- Convergence safety system (monitor sidecar, 7 signals, 5-level interventions)
- Proposal lifecycle with 7-dimension validation gate
- Simulation boundary enforcement (emulation detection + reframing)
- Kill switch with 7 auto-triggers and 3 escalation levels
- Cedar-style policy engine with convergence tightening
- Tamper-evident storage (blake3 hash chains, Merkle trees, append-only triggers)
- CRDT with Ed25519 signed deltas and sybil resistance
- Signed inter-agent messaging with delegation state machine
- Browser extension for passive convergence monitoring across AI platforms
- HTTPS proxy for passive traffic analysis
- Data export analyzer for importing history from 5 platforms
- OpenClaw migration tool for seamless transition

---

## 9. Priority-Ordered Build Plan

### Tier 1: Immediate (blocks other work)
| Gap | Item | Effort | Why First |
|-----|------|--------|-----------|
| GAP-09 | CORP_POLICY.md | ~100 lines | ghost-policy and ghost-identity reference it directly |
| GAP-12 | Root config files (deny.toml, rustfmt.toml, clippy.toml, SECURITY.md) | ~150 lines | Needed for consistent formatting and CI |
| GAP-01 | cortex-test-fixtures crate | ~400 lines | Blocks all property testing |

### Tier 2: Safety-Critical (validates correctness)
| Gap | Item | Effort | Why |
|-----|------|--------|-----|
| GAP-13 | Adversarial test suites | ~800 lines | Validates safety-critical paths |
| Task 7.2 | Correctness properties (17 proptest suites) | ~1200 lines | Req 41 — formal invariant verification |
| GAP-14 | E2E integration tests | ~1500 lines | Validates cross-crate wiring |

### Tier 3: Infrastructure (enables CI/CD)
| Gap | Item | Effort | Why |
|-----|------|--------|-----|
| GAP-11 | CI/CD workflows | ~400 lines | Automated testing and release |
| GAP-15 | Performance benchmarks | ~600 lines | Regression detection |

### Tier 4: Feature Completeness
| Gap | Item | Effort | Why |
|-----|------|--------|-----|
| GAP-08 | Baileys WhatsApp bridge | ~200 lines | WhatsApp channel support |
| GAP-02 | ghost-mesh placeholder | ~100 lines | Phase 9 trait boundary |
| GAP-03-07 | Cortex crate modifications | ~700 lines | Convergence integration in existing cortex crates |

### Tier 5: Documentation
| Gap | Item | Effort | Why |
|-----|------|--------|-----|
| GAP-10 | docs/ directory | ~2000 lines | User-facing documentation |

---

## 10. Estimated Total Remaining Effort

| Category | Lines | Hours (est.) |
|----------|-------|-------------|
| Tier 1: Immediate | ~650 | 3-4h |
| Tier 2: Safety tests | ~3500 | 15-20h |
| Tier 3: CI/CD + benchmarks | ~1000 | 4-6h |
| Tier 4: Features | ~1000 | 4-6h |
| Tier 5: Documentation | ~2000 | 6-8h |
| **Total** | **~8150** | **~32-44h** |

The codebase is architecturally complete. The remaining work is primarily:
1. Testing infrastructure (test-fixtures, property tests, adversarial tests, e2e)
2. CI/CD and project hygiene (workflows, config files, CORP_POLICY)
3. Documentation
4. Minor feature gaps (Baileys bridge, cortex modifications, ghost-mesh placeholder)

---

## 11. Dependency Graph for Gaps

```
GAP-01 (test-fixtures)
  └─ blocks: Task 7.2 (correctness properties)
  └─ blocks: GAP-13 (adversarial tests, partially)
  └─ blocks: GAP-14 (e2e tests, partially)

GAP-09 (CORP_POLICY.md)
  └─ blocks: ghost-policy integration testing
  └─ blocks: ghost-identity CorpPolicyLoader testing

GAP-12 (root configs)
  └─ blocks: GAP-11 (CI/CD workflows reference deny.toml, rustfmt.toml)

GAP-11 (CI/CD)
  └─ blocks: GAP-15 (benchmark workflow)
  └─ depends on: GAP-12 (root configs)

GAP-13 (adversarial tests)
  └─ partially depends on: GAP-01 (test-fixtures for strategies)

GAP-14 (e2e tests)
  └─ depends on: GAP-09 (CORP_POLICY.md for policy tests)
  └─ partially depends on: GAP-01 (test-fixtures)

GAP-08 (Baileys bridge)
  └─ independent — Node.js, no Rust deps

GAP-02 (ghost-mesh)
  └─ independent — placeholder only

GAP-03-07 (cortex modifications)
  └─ independent of each other
  └─ GAP-03 (observability) may depend on metrics crate selection

GAP-10 (docs)
  └─ independent — can be written anytime
```

---

## 12. Cross-Crate Wiring Verification Checklist

These are the integration points that should be verified during e2e testing (GAP-14):

- [ ] ITP events flow from ghost-agent-loop → unix socket → convergence-monitor
- [ ] Kill switch AtomicBool shared between ghost-gateway and ghost-agent-loop
- [ ] TriggerEvent enum (cortex-core) received by AutoTriggerEvaluator (ghost-gateway)
- [ ] ProposalRouter assembles ProposalContext with all 10 fields
- [ ] Atomic transaction: proposal INSERT + memory commit in same SQLite tx
- [ ] DenialFeedback appears in L6 (convergence state), NOT L8 (history)
- [ ] FlushExecutor trait breaks circular dep between gateway and agent-loop
- [ ] CompactionBlock never re-compressed in subsequent passes
- [ ] StatePublisher atomic write (temp + rename) to shared state file
- [ ] Gateway polls shared state at 1s intervals
- [ ] Dual key registration during bootstrap (MessageDispatcher + cortex-crdt KeyRegistry)
- [ ] canonical_bytes() deterministic across sender/receiver
- [ ] ConvergencePolicyTightener restrictions match tool schema filtering in L3
- [ ] Compaction flush exception: memory_write always permitted during flush
- [ ] ITPBuffer disk-backed during Degraded mode, replayed during Recovery
- [ ] Snapshot assembled ONCE pre-loop, immutable for entire run
- [ ] Hash algorithm separation: SHA-256 for ITP content, blake3 for hash chains

---

## 13. Notes on External Cortex Crates

The following cortex crates are referenced in FILE_MAPPING.md and tasks.md but are NOT workspace members:

- `cortex-retrieval` — retrieval/scoring engine
- `cortex-session` — session management
- `cortex-privacy` — privacy filtering
- `cortex-multiagent` — multi-agent coordination
- `cortex-embeddings` — embedding computation
- `cortex-compression` — data compression
- `cortex-tokens` — token counting
- `cortex-causal` — causal reasoning
- `cortex-learning` — learning engine
- `cortex-consolidation` — memory consolidation
- `cortex-prediction` — prediction engine
- `cortex-reclassification` — memory reclassification
- `cortex-observability` — metrics/observability
- `cortex-cloud` — cloud sync
- `cortex-napi` — Node.js bindings
- `cortex-drift-bridge` — Drift ↔ Cortex bridge

These are likely part of the broader Cortex ecosystem that exists outside this workspace. GAP-03 through GAP-07 reference modifications to some of these crates. If they're external, those modifications would happen in their respective repositories. If they're being brought into this workspace, they'd need to be added as workspace members.

For the purposes of this gap analysis, we treat them as external dependencies that will be modified separately. The GHOST workspace's 25 crates are self-contained and functional without them — the modifications add convergence awareness to the broader Cortex ecosystem.

---

*End of gap analysis. Total gaps identified: 15 (GAP-01 through GAP-15) plus 9 cross-crate wiring verification points (GAP-16 through GAP-24) and 17 correctness properties.*