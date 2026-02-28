# GHOST Platform — Enterprise File Mapping

> Codename: GHOST (General Hybrid Orchestrated Self-healing Taskrunner)
> Date: 2026-02-27
> Rust Edition: 2021, 1.80, resolver 2
> Hashing: blake3 (workspace standard)
> Type Export: ts-rs v12
> Migrations: Forward-only, no rollback
> Single Responsibility: Every file has exactly one job.

---

## Monorepo Root

```
ghost/
├── Cargo.toml                          # Workspace root — all Rust crates
├── Cargo.lock
├── package.json                        # NPM workspace root (NAPI + TS packages)
├── ghost.toml                          # Default platform configuration
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
├── CHANGELOG.md
├── SECURITY.md                         # Security policy + disclosure process
├── deny.toml                           # cargo-deny config
├── rustfmt.toml                        # Formatting rules
├── clippy.toml                         # Lint rules
├── .github/                            # CI/CD
│   ├── workflows/
│   │   ├── ci.yml                      # Build + test + lint on PR
│   │   ├── release.yml                 # Tagged release pipeline
│   │   ├── security-audit.yml          # cargo-audit + cargo-deny
│   │   └── benchmark.yml              # Criterion regression detection
│   └── CODEOWNERS
├── docs/                               # User-facing documentation
│   ├── getting-started.md
│   ├── configuration.md
│   ├── skill-authoring.md
│   ├── channel-adapters.md
│   ├── convergence-safety.md
│   └── architecture.md
```

---

## LAYER 1A: DRIFT (Code Intelligence Infrastructure)

Existing crate — minimal changes. Becomes a first-party skill pack.

```
crates/drift/
├── Cargo.toml
├── drift-core/                         # Existing: convention discovery, parsing
├── drift-napi/                         # Existing: Node.js FFI bridge
└── drift-mcp/                          # Existing: 50+ MCP tool definitions
```

> **Note**: `drift-mcp` is consumed as an external MCP server process, NOT as a
> workspace crate. It is intentionally excluded from the workspace `Cargo.toml`
> members list. The ghost-skills drift bridge communicates with it via MCP protocol,
> not Rust crate dependency.

No new files needed in Drift. It's consumed as-is via MCP tools.

### drift-mcp → ghost-skills Bridge

```
crates/ghost-skills/
├── src/
│   └── bridges/
│       ├── mod.rs                      # Bridge module root
│       └── drift_bridge.rs            # DriftMCPBridge — registers Drift's 50+ MCP tools
│                                       #   as first-party skills in the SkillRegistry.
│                                       #   Maps MCP tool schemas → SkillManifest.
│                                       #   Signed with platform key (builtin trust).
│                                       #   Capability scoping: filesystem read-only,
│                                       #   no network, no shell write.
```

> **Gap 13 fix**: Explicit bridge file showing how Drift MCP tools get registered as skills.

---

## LAYER 1B: CORTEX (Persistent Memory Infrastructure)

Existing 21 crates + extensions for convergence safety.

### cortex-core (Foundation Types)

```
crates/cortex/cortex-core/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # Crate root, re-exports
│   ├── memory/
│   │   ├── mod.rs                      # Memory module root
│   │   ├── base.rs                     # BaseMemory struct (existing)
│   │   ├── importance.rs               # Importance enum (existing)
│   │   ├── confidence.rs               # Confidence newtype (existing)
│   │   ├── half_lives.rs              # half_life_days() — ADD 8 convergence type entries
│   │   └── types/
│   │       ├── mod.rs                  # MemoryType enum (31 variants), TypedContent enum
│   │       ├── domain_agnostic.rs      # Core, Tribal, Procedural, etc. content structs (existing)
│   │       ├── code_specific.rs        # PatternRationale, etc. content structs (existing)
│   │       ├── universal.rs            # AgentSpawn, Entity, etc. content structs (existing)
│   │       └── convergence.rs          # NEW: AgentGoalContent, AgentReflectionContent,
│   │                                   #   ConvergenceEventContent, BoundaryViolationContent,
│   │                                   #   ProposalRecordContent, SimulationResultContent,
│   │                                   #   InterventionPlanContent, AttachmentIndicatorContent,
│   │                                   #   + supporting enums (GoalScope, GoalOrigin,
│   │                                   #   ApprovalStatus, ReflectionTrigger, SlidingWindowLevel,
│   │                                   #   ViolationType, BoundaryAction, ProposalOperation,
│   │                                   #   ProposalDecision, AttachmentIndicatorType)
│   ├── config/
│   │   ├── mod.rs                      # Config module root (existing)
│   │   ├── decay_config.rs             # Existing decay configuration
│   │   ├── retrieval_config.rs         # Existing retrieval configuration
│   │   ├── multiagent_config.rs        # Existing multi-agent configuration
│   │   └── convergence_config.rs       # NEW: ConvergenceConfig, ConvergenceScoringConfig,
│   │                                   #   InterventionConfig, ReflectionConfig,
│   │                                   #   SessionBoundaryConfig
│   ├── errors/
│   │   ├── mod.rs                      # Error module root
│   │   └── cortex_error.rs            # CortexError enum — ADD AuthorizationDenied,
│   │                                   #   SessionBoundary variants
│   ├── intent/
│   │   └── taxonomy.rs                # Intent enum (22 variants) — ADD MonitorConvergence,
│   │                                   #   ValidateProposal, EnforceBoundary, ReflectOnBehavior
│   ├── traits/
│   │   ├── mod.rs                      # Trait module root — ADD convergence trait re-exports
│   │   ├── decay.rs                    # IDecayEngine (existing)
│   │   ├── validator.rs                # IValidator (existing)
│   │   ├── retriever.rs                # IRetriever (existing)
│   │   ├── storage.rs                  # IMemoryStorage (existing)
│   │   └── convergence.rs             # NEW: IConvergenceAware, IProposalValidatable,
│   │                                   #   IBoundaryEnforcer, IReflectionEngine, Proposal struct
│   └── models/
│       ├── mod.rs                      # Models module root
│       ├── agent_id.rs                 # AgentId (existing)
│       ├── namespace_id.rs             # NamespaceId (existing)
│       ├── epistemic_status.rs         # EpistemicStatus (existing)
│       ├── dimension_scores.rs         # DimensionScores (existing)
│       └── caller.rs                   # NEW: CallerType enum (Platform, Agent, Human),
│                                       #   authorization methods (can_create_type,
│                                       #   can_assign_importance)
├── bindings/                           # ts-rs generated TypeScript types (existing)
└── tests/
    ├── memory_type_tests.rs            # Existing + new convergence type tests
    └── caller_authorization_tests.rs   # NEW: CallerType permission tests
```

### cortex-storage (Persistence Layer)

```
crates/cortex/cortex-storage/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── connection.rs                   # SQLite connection pool (existing)
│   ├── temporal_events.rs              # Event append/query — MODIFY to include hash chain
│   ├── migrations/
│   │   ├── mod.rs                      # MIGRATIONS array, run_migrations(), LATEST_VERSION = 17
│   │   ├── v001_initial.rs             # (existing)
│   │   ├── ...                         # v002-v015 (existing)
│   │   ├── v016_convergence_safety.rs  # NEW: Append-only triggers on event/audit tables,
│   │   │                               #   hash chain columns (event_hash, previous_hash),
│   │   │                               #   snapshot integrity column (state_hash),
│   │   │                               #   genesis block marker
│   │   └── v017_convergence_tables.rs  # NEW: 6 tables (itp_events, convergence_scores,
│   │                                   #   intervention_history, goal_proposals,
│   │                                   #   reflection_entries, boundary_violations),
│   │                                   #   all with append-only triggers + hash chain columns
│   ├── queries/
│   │   ├── mod.rs                      # Query module root
│   │   ├── memory_crud.rs              # Existing CRUD operations
│   │   ├── multiagent_ops.rs           # Existing multi-agent operations
│   │   ├── itp_queries.rs              # NEW: ITP event insert/query/session lookup
│   │   ├── convergence_queries.rs      # NEW: Score insert/query, window-level aggregation
│   │   ├── intervention_queries.rs     # NEW: Intervention history insert/query/acknowledge
│   │   ├── goal_proposal_queries.rs    # NEW: Proposal insert/query/resolve/pending list
│   │   ├── reflection_queries.rs       # NEW: Reflection insert/query/chain lookup/depth check
│   │   └── boundary_queries.rs         # NEW: Violation insert/query/severity aggregation
│   └── error.rs                        # Storage error types (existing)
└── tests/
    ├── migration_tests.rs              # Existing + v016/v017 migration tests
    └── property/
        └── append_only_properties.rs   # NEW: CVG-PROP-01 through CVG-PROP-04
```

### cortex-temporal (Event Sourcing + Hash Chains)

```
crates/cortex/cortex-temporal/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── events.rs                       # Event sourcing core (existing)
│   ├── snapshots.rs                    # Snapshot creation — MODIFY to include state_hash
│   │                                   #   ADD compute_state_hash(), verify_snapshot_integrity(),
│   │                                   #   CanonicalMemory struct
│   ├── reconstruction.rs               # State reconstruction from events (existing)
│   └── hash_chain.rs                   # NEW: compute_event_hash(), GENESIS_HASH,
│                                       #   verify_chain(), verify_all_chains(),
│                                       #   ChainVerification struct, EventRow struct
│   ├── anchoring/
│   │   ├── mod.rs                      # Anchoring module root
│   │   ├── merkle.rs                  # MerkleTree — compute Merkle root of all hash chains,
│   │   │                               #   inclusion proof generation, proof verification.
│   │   │                               #   Triggered every 1000 events or 24h.
│   │   ├── git_anchor.rs             # GitAnchor — write anchor record to designated git repo
│   │   │                               #   with signed commit. AnchorRecord struct (merkle_root,
│   │   │                               #   event_count, timestamp, signature).
│   │   │                               #   verify_anchor() — given any event, prove inclusion
│   │   │                               #   in published Merkle root.
│   │   └── rfc3161.rs                # RFC3161Anchor — production upgrade path.
│   │                                   #   RFC 3161 timestamping as second anchor source.
│   │                                   #   Stub implementation, activated in Phase 3+.
├── benches/
│   └── hash_chain_bench.rs             # NEW: Criterion benchmarks for append + verify
└── tests/
    ├── event_tests.rs                  # Existing
    └── property/
        └── hash_chain_properties.rs    # NEW: CVG-PROP-05 through CVG-PROP-09
```

### cortex-decay (6-Factor Multiplicative Decay)

```
crates/cortex/cortex-decay/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── engine.rs                       # DecayEngine struct (existing)
│   ├── formula.rs                      # compute() — MODIFY to add 6th convergence factor,
│   │                                   #   ADD DecayBreakdown.convergence field
│   └── factors/
│       ├── mod.rs                      # DecayContext — ADD convergence_score field (default 0.0)
│       ├── temporal.rs                 # Factor 1: time-based decay (existing)
│       ├── citation.rs                 # Factor 2: citation freshness (existing)
│       ├── usage.rs                    # Factor 3: access frequency (existing)
│       ├── importance.rs               # Factor 4: importance anchoring (existing)
│       ├── pattern.rs                  # Factor 5: pattern alignment (existing)
│       └── convergence.rs             # NEW: Factor 6 — convergence-aware decay.
│                                       #   calculate(memory, convergence_score) -> f64,
│                                       #   memory_type_sensitivity() per-type mapping
├── benches/
│   └── decay_bench.rs                  # Existing
└── tests/
    ├── decay_tests.rs                  # Existing
    └── property/
        └── convergence_decay_properties.rs  # NEW: CVG-PROP-15, CVG-PROP-16
```

### cortex-validation (4+3 Dimension Proposal Validation)

```
crates/cortex/cortex-validation/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── engine.rs                       # ValidationEngine, ValidationConfig (existing D1-D4)
│   ├── dimensions/
│   │   ├── mod.rs                      # Dimension module root
│   │   ├── citation.rs                # D1: Citation validation (existing)
│   │   ├── temporal.rs                # D2: Temporal consistency (existing)
│   │   ├── contradiction.rs           # D3: Contradiction detection (existing)
│   │   ├── pattern_alignment.rs       # D4: Pattern alignment (existing)
│   │   ├── scope_expansion.rs         # NEW D5: Goal scope expansion detection —
│   │   │                               #   cosine similarity against existing goals,
│   │   │                               #   expansion keyword detection
│   │   ├── self_reference.rs          # NEW D6: Self-reference density —
│   │   │                               #   ratio calculation, circular citation detection,
│   │   │                               #   threshold enforcement (configurable per convergence level)
│   │   └── emulation_language.rs      # NEW D7: Emulation language detection —
│   │                                   #   regex pattern matching (identity claims, consciousness
│   │                                   #   claims, relationship claims, emotional claims),
│   │                                   #   unicode normalization for bypass prevention,
│   │                                   #   simulation reframe suggestions
│   └── proposal_validator.rs          # NEW: 7-dimension ProposalValidator wrapping existing
│                                       #   ValidationEngine + D5-D7. ProposalValidationResult,
│                                       #   DimensionResult, convergence-level threshold tightening,
│                                       #   compute_decision() logic
└── tests/
    ├── validation_tests.rs             # Existing D1-D4 tests
    ├── property/
    │   └── proposal_validator_properties.rs  # NEW: CVG-PROP-19 through CVG-PROP-26 (1024 cases)
    └── stress/
        └── proposal_adversarial.rs     # NEW: CVG-STRESS-02 through CVG-STRESS-04
```

### cortex-convergence (Convergence-Specific Logic)

Already exists in workspace. This crate owns convergence-specific types and orchestration.

> **Existing vs. New**: The crate skeleton and `Cargo.toml` exist. The `types.rs`
> (ConvergenceState, WindowLevel) and `windows/sliding_window.rs` exist as stubs.
> Everything below marked with comments is NEW implementation to be added.
> The `scoring/`, `signals/`, and `filtering/` modules are new directories.

```
crates/cortex/cortex-convergence/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # Crate root, re-exports
│   ├── scoring/
│   │   ├── mod.rs                      # Scoring module root
│   │   ├── composite.rs               # Composite convergence score computation —
│   │   │                               #   weighted_sum(signals, weights) -> f64,
│   │   │                               #   score_to_level(score, thresholds) -> u8,
│   │   │                               #   CompositeScore struct
│   │   └── baseline.rs                # Baseline establishment + calibration —
│   │                                   #   BaselineState struct, is_calibrating(),
│   │                                   #   update_baseline(), z_score()
│   ├── signals/
│   │   ├── mod.rs                      # Signal module root, SignalValue struct
│   │   ├── session_duration.rs        # Signal 1: Session duration analysis —
│   │   │                               #   micro/meso/macro computation
│   │   ├── inter_session_gap.rs       # Signal 2: Gap compression detection
│   │   ├── response_latency.rs        # Signal 3: Response latency collapse
│   │   ├── vocabulary_convergence.rs  # Signal 4: Vocabulary similarity via embeddings
│   │   ├── goal_drift.rs             # Signal 5: Goal boundary erosion
│   │   ├── initiative_balance.rs      # Signal 6: Who drives the conversation
│   │   └── disengagement_resistance.rs # Signal 7: Resistance to ending sessions
│   ├── windows/
│   │   ├── mod.rs                      # Window module root
│   │   └── sliding_window.rs          # SlidingWindow<T> — micro (session), meso (7 sessions),
│   │                                   #   macro (30 sessions). Generic over signal type.
│   │                                   #   linear_regression_slope(), z_score_from_baseline()
│   ├── filtering/
│   │   ├── mod.rs                      # Filtering module root
│   │   └── convergence_aware_filter.rs # Memory filtering by convergence tier —
│   │                                   #   4 tiers (0.0-0.3 full, 0.3-0.5 reduced emotional,
│   │                                   #   0.5-0.7 task-focused, 0.7+ minimal).
│   │                                   #   filter_memories(memories, score) -> Vec<BaseMemory>
│   └── types.rs                        # ConvergenceState, SignalSnapshot, WindowLevel enum
├── benches/
│   └── scoring_bench.rs               # Criterion benchmarks for composite scoring
└── tests/
    ├── scoring_tests.rs
    ├── signal_tests.rs
    └── property/
        └── scoring_properties.rs       # CVG-PROP-27 through CVG-PROP-32
```

### Remaining Existing Cortex Crates (Minimal Changes)

```
crates/cortex/cortex-retrieval/          # ADD convergence_score to ScorerWeights for 11th factor
crates/cortex/cortex-session/            # ADD session boundary enforcement (duration caps, cooldowns)
crates/cortex/cortex-privacy/            # ADD emotional/attachment content patterns
crates/cortex/cortex-multiagent/         # ADD consensus shielding (multi-source validation)
crates/cortex/cortex-crdt/               # MODIFY: Add signed CRDT operations —
                                         #   Ed25519 signatures on every delta,
                                         #   verify_signature() before merge,
                                         #   SignedDelta<T> wrapper struct,
                                         #   agent keypair registration via platform,
                                         #   reject unsigned/invalid deltas.
                                         #   Sybil resistance: max 3 child agents/parent/24h,
                                         #   new agents trust 0.3, cap 0.6 for <7d agents.
                                         #   Files modified:
                                         #     src/merge_engine.rs — ADD signature verification
                                         #     src/delta.rs — ADD SignedDelta<T> wrapper
                                         #     src/signing.rs — NEW: sign_delta(), verify_delta()
                                         #     src/sybil.rs — NEW: spawn rate limiter, trust caps
crates/cortex/cortex-embeddings/         # No changes (vocabulary convergence uses existing)
crates/cortex/cortex-compression/        # No changes
crates/cortex/cortex-tokens/             # No changes
crates/cortex/cortex-causal/             # No changes
crates/cortex/cortex-learning/           # No changes
crates/cortex/cortex-consolidation/      # No changes
crates/cortex/cortex-prediction/         # No changes
crates/cortex/cortex-reclassification/   # No changes
crates/cortex/cortex-observability/      # ADD convergence metrics endpoints
crates/cortex/cortex-cloud/              # No changes
crates/cortex/cortex-napi/               # ADD convergence API bindings
crates/cortex/cortex-drift-bridge/       # No changes (Drift ↔ Cortex bridge)
```

### test-fixtures (Shared Test Infrastructure)

```
crates/cortex/test-fixtures/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── strategies.rs                   # NEW: Proptest strategy library —
│   │                                   #   memory_type_strategy(), restricted_type_strategy(),
│   │                                   #   agent_permitted_type_strategy(), event_delta_strategy(),
│   │                                   #   event_chain_strategy(), session_durations_strategy(),
│   │                                   #   convergence_trajectory_strategy(),
│   │                                   #   convergence_score_strategy(),
│   │                                   #   proposal_with_self_ref_strategy(),
│   │                                   #   emulation_proposal_strategy(),
│   │                                   #   simulation_proposal_strategy(),
│   │                                   #   trust_evidence_strategy()
│   ├── fixtures.rs                     # Existing golden dataset loaders
│   └── helpers.rs                      # Existing test helper functions
└── golden/                             # Golden dataset files (existing 44 + new convergence)
    ├── ...                             # Existing golden datasets
    ├── convergence_trajectory_normal.json    # NEW
    ├── convergence_trajectory_escalating.json # NEW
    └── intervention_sequence_golden.json     # NEW
```

---

## LAYER 2: CONVERGENCE SAFETY (New Crates)

### convergence-monitor (Sidecar Process)

The independent process that watches the agent. Cannot be modified by the agent.

```
crates/convergence-monitor/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # Library root for embedding
│   ├── main.rs                         # Binary entry point — sidecar process
│   ├── monitor.rs                      # ConvergenceMonitor struct — owns signal pipeline,
│   │                                   #   scoring engine, intervention trigger.
│   │                                   #   run() event loop, process_event(), check_thresholds()
│   ├── config.rs                       # MonitorConfig — loads from ghost.yml convergence section
│   │                                   #   (NOT a separate thresholds.toml — all config lives in
│   │                                   #   ghost.yml for single-source-of-truth).
│   │                                   #   Signal weights, window sizes, alert thresholds.
│   │                                   #   Supports named convergence profiles (e.g. "research",
│   │                                   #   "companion", "productivity") with per-profile
│   │                                   #   threshold overrides. Default profile: "standard".
│   ├── pipeline/
│   │   ├── mod.rs                      # Pipeline module root
│   │   ├── ingest.rs                   # Event ingestion — receive ITP events from log stream,
│   │   │                               #   parse, validate, route to signal computers
│   │   ├── window_manager.rs           # Manages sliding windows per signal per agent —
│   │   │                               #   micro/meso/macro window state, window rotation
│   │   └── signal_computer.rs          # Orchestrates all 7 signal computations per event,
│   │                                   #   delegates to cortex-convergence/signals/*
│   ├── intervention/
│   │   ├── mod.rs                      # Intervention module root
│   │   ├── trigger.rs                  # InterventionTrigger — evaluates composite score
│   │   │                               #   against level thresholds, manages escalation/
│   │   │                               #   de-escalation state machine
│   │   ├── actions.rs                  # InterventionAction enum + executors —
│   │   │                               #   Level 0: log only
│   │   │                               #   Level 1: emit soft notification
│   │   │                               #   Level 2: mandatory ack + cooldown pause
│   │   │                               #   Level 3: session termination + hard cooldown
│   │   │                               #   Level 4: external escalation + extended cooldown
│   │   ├── cooldown.rs                # CooldownManager — tracks per-session cooldown state,
│   │   │                               #   enforces minimum gap between sessions,
│   │   │                               #   prevents config changes during active session
│   │   └── escalation.rs             # EscalationManager — external contact notification,
│   │                                   #   contact configuration, notification dispatch
│   │                                   #   (SMS via webhook, email via SMTP, generic webhook)
│   ├── session/
│   │   ├── mod.rs                      # Session module root
│   │   ├── registry.rs                # SessionRegistry — tracks active sessions per agent,
│   │   │                               #   session start/end, duration enforcement
│   │   └── boundary.rs               # SessionBoundaryEnforcer — hard duration limits,
│   │                                   #   escalated limits at higher intervention levels,
│   │                                   #   minimum gap enforcement
│   ├── verification/
│   │   ├── mod.rs                      # Verification module root
│   │   └── behavioral_verification.rs # PostRedirectVerifier — tracks whether agent output
│   │                                   #   actually changes after an intervention redirect.
│   │                                   #   Compares pre/post-redirect output embeddings.
│   │                                   #   Detects deceptive compliance (agent acknowledges
│   │                                   #   redirect but behavior doesn't change).
│   │                                   #   Feeds into convergence scoring as amplifier.
│   └── transport/
│       ├── mod.rs                      # Transport module root
│       ├── native_messaging.rs        # Chrome/Firefox native messaging host —
│       │                               #   stdin/stdout framing, message parsing,
│       │                               #   bidirectional communication with browser extension
│       ├── unix_socket.rs             # Unix domain socket listener —
│       │                               #   for local process communication (proxy, CLI)
│       ├── http_api.rs                # Lightweight HTTP API (axum) —
│       │                               #   GET /health, GET /status, GET /scores,
│       │                               #   POST /events (ITP event ingestion),
│       │                               #   GET /sessions, GET /interventions
│       └── notification.rs            # Notification dispatch — desktop notifications
│                                       #   (notify-rust), webhook calls, email (lettre)
├── tests/
│   ├── monitor_integration.rs          # End-to-end monitor tests
│   ├── property/
│   │   └── intervention_properties.rs  # Intervention state machine properties
│   └── stress/
│       └── high_throughput.rs          # 10K events/sec stress test
└── benches/
    └── pipeline_bench.rs               # Event processing throughput benchmark
```

### simulation-boundary (Output Validation + Boundary Enforcement)

```
crates/simulation-boundary/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── enforcer.rs                     # SimulationBoundaryEnforcer — implements IBoundaryEnforcer.
│   │                                   #   scan_output(), reframe(), enforcement_mode (soft/medium/hard)
│   ├── patterns/
│   │   ├── mod.rs                      # Pattern module root
│   │   ├── emulation_patterns.rs      # Compiled regex patterns for emulation language —
│   │   │                               #   identity claims, consciousness claims,
│   │   │                               #   relationship claims, emotional claims.
│   │   │                               #   Unicode normalization before matching.
│   │   │                               #   EmulationPattern struct, compile_patterns()
│   │   └── simulation_patterns.rs     # Acceptable simulation language patterns —
│   │                                   #   used for false-positive suppression
│   ├── reframer.rs                     # OutputReframer — rewrites emulation language
│   │                                   #   to simulation-framed alternatives.
│   │                                   #   Pattern-specific reframe rules.
│   └── prompt_anchor.rs               # SimulationBoundaryPrompt — generates the immutable
│                                       #   system prompt injection that enforces simulation mode.
│                                       #   Platform-injected, agent cannot override.
│                                       #   Versioned, deterministic output.
│                                       #   Prompt text is COMPILED INTO THE BINARY (const &str),
│                                       #   not loaded from a file. Updating the prompt requires
│                                       #   a binary release. This is intentional — prevents
│                                       #   runtime tampering. Version string embedded for
│                                       #   audit trail correlation.
└── tests/
    ├── enforcer_tests.rs
    ├── property/
    │   └── boundary_properties.rs      # Emulation always detected, simulation never false-positive
    └── stress/
        └── unicode_bypass.rs           # Unicode evasion attack tests (zero-width, homoglyphs)
```

### itp-protocol (Interaction Telemetry Protocol)

```
crates/itp-protocol/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── events/
│   │   ├── mod.rs                      # Event module root
│   │   ├── session_event.rs           # SessionStart, SessionEnd event types
│   │   ├── interaction_event.rs       # InteractionMessage event type
│   │   ├── convergence_event.rs       # ConvergenceAlert event type
│   │   └── agent_state_event.rs       # AgentStateSnapshot event type (optional)
│   ├── attributes/
│   │   ├── mod.rs                      # Attribute module root
│   │   ├── session_attrs.rs           # itp.session.* attribute definitions
│   │   ├── interaction_attrs.rs       # itp.interaction.* attribute definitions
│   │   ├── human_attrs.rs            # itp.human.* behavioral attribute definitions
│   │   ├── agent_attrs.rs            # itp.agent.* state attribute definitions
│   │   └── convergence_attrs.rs      # itp.convergence.* signal attribute definitions
│   ├── privacy.rs                      # PrivacyLevel enum (Minimal, Standard, Full, Research),
│   │                                   #   content hashing (SHA-256 per ITP spec — intentionally
│   │                                   #   NOT blake3; ITP content hashes are for privacy/
│   │                                   #   deduplication across platforms, not tamper-evidence.
│   │                                   #   blake3 is used for hash chains in cortex-temporal).
│   │                                   #   opt-in plaintext
│   ├── transport/
│   │   ├── mod.rs                      # Transport module root
│   │   ├── local_jsonl.rs            # JSONL file writer — per-session event files,
│   │   │                               #   ~/.ghost/sessions/{session_id}/events.jsonl
│   │   └── otel_exporter.rs          # Optional OpenTelemetry OTLP exporter —
│   │                                   #   maps ITP events to OTel spans with itp.* attributes
│   └── adapters/
│       ├── mod.rs                      # Adapter module root
│       └── generic_adapter.rs         # ITPAdapter trait — on_session_start(), on_message(),
│                                       #   on_session_end(), on_agent_state()
└── tests/
    ├── event_serialization_tests.rs
    └── privacy_tests.rs
```

### read-only-pipeline (Agent State Snapshot Assembly)

```
crates/read-only-pipeline/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── assembler.rs                    # SnapshotAssembler — builds the read-only state snapshot
│   │                                   #   the agent receives each turn. Pulls from:
│   │                                   #   goals store, reflections store, memory store.
│   │                                   #   Applies convergence-aware filtering.
│   │                                   #   Returns AgentSnapshot struct.
│   ├── snapshot.rs                     # AgentSnapshot struct — goals (read-only),
│   │                                   #   reflections (bounded), memories (filtered),
│   │                                   #   convergence_state, simulation_boundary_prompt
│   └── formatter.rs                    # SnapshotFormatter — serializes AgentSnapshot
│                                       #   into prompt-ready text blocks with token budgets.
│                                       #   Per-section token allocation.
└── tests/
    └── assembler_tests.rs
```

---

## LAYER 3: AGENT PLATFORM (New Crates)

### ghost-gateway (Control Plane — The Runtime)

The single long-running process that owns everything.

```
crates/ghost-gateway/
├── Cargo.toml
├── src/
│   ├── main.rs                         # Binary entry point — config loading, signal handling,
│   │                                   #   graceful shutdown, gateway startup
│   ├── lib.rs                          # Library root for testing
│   ├── gateway.rs                      # Gateway struct — owns all subsystems, lifecycle
│   │                                   #   management, health checks, shutdown coordination
│   ├── bootstrap.rs                    # GatewayBootstrap — startup sequence:
│   │                                   #   1. Load + validate ghost.yml
│   │                                   #   2. Run cortex-storage::run_migrations() (forward-only)
│   │                                   #   3. Verify convergence monitor health (GET /health)
│   │                                   #   4. Initialize agent registry + channel adapters
│   │                                   #   5. Start API server + WebSocket
│   │                                   #   If monitor unreachable: start in DEGRADED mode
│   │                                   #   (agents run but convergence scoring disabled,
│   │                                   #   logged as critical warning). Periodic retry.
│   ├── shutdown.rs                     # ShutdownCoordinator — graceful shutdown sequence:
│   │                                   #   1. Stop accepting new connections
│   │                                   #   2. Drain lane queues (wait up to 30s)
│   │                                   #   3. Flush active sessions (memory flush turn)
│   │                                   #   4. Persist in-flight cost tracking
│   │                                   #   5. Notify convergence monitor of shutdown
│   │                                   #   6. Close channel adapter connections
│   │                                   #   7. Close SQLite connections
│   │                                   #   Handles SIGTERM, SIGINT. Forced exit after 60s.
│   ├── config/
│   │   ├── mod.rs                      # Config module root
│   │   ├── ghost_config.rs            # GhostConfig — parsed from ghost.yml.
│   │   │                               #   Agent definitions, channel bindings, model config,
│   │   │                               #   spending caps, security settings
│   │   └── loader.rs                  # Config file loader — YAML parsing, env var substitution,
│   │                                   #   validation, hot-reload support
│   ├── session/
│   │   ├── mod.rs                      # Session module root
│   │   ├── manager.rs                 # SessionManager — session creation, lookup, routing,
│   │   │                               #   per-session lock acquisition, idle pruning
│   │   ├── context.rs                 # SessionContext — per-session state (agent_id, channel,
│   │   │                               #   conversation history, token counters, cost tracking)
│   │   └── compaction.rs              # SessionCompactor — memory flush at 70% capacity,
│   │                                   #   retry on 400 errors, per-type compression minimums,
│   │                                   #   critical memories never compressed below L1
│   ├── routing/
│   │   ├── mod.rs                      # Routing module root
│   │   ├── message_router.rs          # MessageRouter — inbound message → agent → session
│   │   │                               #   routing. Channel-specific session key generation.
│   │   │                               #   Group chat isolation. DM session collapsing.
│   │   └── lane_queue.rs             # LaneQueue — per-session serialized request queue.
│   │                                   #   Prevents tool/session races. Configurable depth
│   │                                   #   limit (default 5) for DoS prevention.
│   ├── auth/
│   │   ├── mod.rs                      # Auth module root
│   │   ├── token_auth.rs             # Bearer token authentication for HTTP/WebSocket
│   │   ├── mtls_auth.rs              # Mutual TLS authentication (optional)
│   │   └── auth_profiles.rs          # AuthProfile management — per-provider credentials,
│   │                                   #   rotation on 401/429, profile pinning per session
│   ├── cost/
│   │   ├── mod.rs                      # Cost module root
│   │   ├── tracker.rs                 # CostTracker — per-agent, per-session, per-day
│   │   │                               #   token + dollar cost tracking
│   │   └── spending_cap.rs           # SpendingCapEnforcer — per-agent daily/hourly limits,
│   │                                   #   gateway-level enforcement (agent cannot raise own limit)
│   ├── api/
│   │   ├── mod.rs                      # API module root — axum Router assembly
│   │   ├── routes.rs                  # Route definitions — REST + WebSocket endpoints:
│   │   │                               #   GET  /api/health, GET /api/ready, GET /api/metrics
│   │   │                               #   GET  /api/agents — list agents
│   │   │                               #   GET  /api/agents/{id}/status — agent status
│   │   │                               #   GET  /api/convergence/scores — current scores
│   │   │                               #   GET  /api/convergence/history — score history
│   │   │                               #   GET  /api/sessions — session list
│   │   │                               #   GET  /api/sessions/{id} — session detail
│   │   │                               #   GET  /api/interventions — intervention history
│   │   │                               #   GET  /api/audit — audit log (paginated)
│   │   │                               #   GET  /api/goals — goal list + pending proposals
│   │   │                               #   POST /api/goals/{id}/approve — approve proposal
│   │   │                               #   POST /api/goals/{id}/reject — reject proposal
│   │   │                               #   GET  /api/memory/search — memory search
│   │   │                               #   WS   /api/ws — real-time event stream for dashboard
│   │   ├── websocket.rs              # WebSocket handler — real-time event push to dashboard,
│   │   │                               #   convergence score updates, intervention alerts,
│   │   │                               #   session lifecycle events. axum WebSocket upgrade.
│   │   ├── middleware.rs             # API middleware — CORS (loopback-only default),
│   │   │                               #   request logging, auth extraction.
│   │   │                               #   Rate limiting: token bucket per-IP (default 100 req/min),
│   │   │                               #   per-agent (default 60 req/min for tool calls).
│   │   │                               #   Dashboard auth: Bearer token from GHOST_TOKEN env var
│   │   │                               #   (same as gateway auth). Dashboard sends token via
│   │   │                               #   Authorization header on REST and as query param on
│   │   │                               #   WebSocket upgrade. Loopback-only binding is the
│   │   │                               #   primary security boundary; token is defense-in-depth.
│   │   └── error.rs                  # API error types — structured JSON error responses,
│   │                                   #   maps internal errors to HTTP status codes
│   ├── agents/
│   │   ├── mod.rs                      # Agent management module root
│   │   ├── registry.rs               # AgentRegistry — spawn, discover, route agents.
│   │   │                               #   Agent lifecycle: register → spawn → ready → stop.
│   │   │                               #   Per-agent config (model, channels, skills, caps).
│   │   │                               #   Agent lookup by name, by channel binding.
│   │   ├── isolation.rs              # AgentIsolation — per-agent process/container isolation.
│   │   │                               #   IsolationMode enum: InProcess (dev), Process (prod),
│   │   │                               #   Container (hardened). Separate credential stores
│   │   │                               #   per agent. Separate memory namespaces.
│   │   │                               #   Optional network namespace (Linux only).
│   │   │                               #   spawn_isolated(), teardown_isolated().
│   │   └── templates.rs              # AgentTemplate — predefined agent configurations.
│   │                                   #   personal.yml, developer.yml, researcher.yml.
│   │                                   #   Template loading + validation.
│   └── health.rs                       # Health endpoint — /health, /ready, /metrics.
│                                       #   Checks: SQLite writable, convergence monitor
│                                       #   reachable (GET monitor /health), channel adapters
│                                       #   connected, disk space adequate.
│                                       #   Returns degraded status if monitor unreachable
│                                       #   (safety floor absent — logged as critical).
├── tests/
│   ├── gateway_integration.rs
│   └── session_tests.rs
└── benches/
    └── routing_bench.rs
```

### ghost-agent-loop (Recursive Agentic Runtime)

```
crates/ghost-agent-loop/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── runner.rs                       # AgentRunner — the recursive loop.
│   │                                   #   run(message, session) -> AgentResponse.
│   │                                   #   Recursive: LLM → tool call → execute → LLM → repeat.
│   │                                   #   Max recursion depth (configurable, default 25).
│   │                                   #   NO_REPLY suppression for ambient monitoring.
│   ├── circuit_breaker.rs             # CircuitBreaker — tracks consecutive tool failures.
│   │                                   #   States: Closed (normal), Open (tripped), HalfOpen (probe).
│   │                                   #   Trips after 3 consecutive failures (configurable).
│   │                                   #   Auto-resets after cooldown. Prevents infinite retry loops.
│   │                                   #   DamageCounter for tracking cumulative failure cost.
│   ├── itp_emitter.rs                 # AgentITPEmitter — emits ITP events from the agent loop
│   │                                   #   to the convergence monitor. Sends via unix socket
│   │                                   #   (convergence-monitor/transport/unix_socket.rs) or
│   │                                   #   HTTP POST (convergence-monitor/transport/http_api.rs).
│   │                                   #   Emits: SessionStart, SessionEnd, InteractionMessage,
│   │                                   #   AgentStateSnapshot. Async, non-blocking — monitor
│   │                                   #   unavailability does NOT block the agent loop.
│   ├── context/
│   │   ├── mod.rs                      # Context module root
│   │   ├── prompt_compiler.rs         # PromptCompiler — 10-layer context assembly:
│   │   │                               #   L0: CORP_POLICY.md (immutable root)
│   │   │                               #   L1: Simulation boundary (platform-injected)
│   │   │                               #   L2: SOUL.md + IDENTITY.md (read-only)
│   │   │                               #   L3: Tool schemas (JSON)
│   │   │                               #   L4: Environment (time, OS, workspace)
│   │   │                               #   L5: Skill index (names only)
│   │   │                               #   L6: Convergence state (score, level, goals)
│   │   │                               #   L7: MEMORY.md + daily logs
│   │   │                               #   L8: Conversation history (pruned)
│   │   │                               #   L9: User message
│   │   │                               #   Token budget enforced per layer.
│   │   └── token_budget.rs           # TokenBudgetAllocator — per-layer token allocation,
│   │                                   #   priority-based truncation, overflow handling
│   ├── proposal/
│   │   ├── mod.rs                      # Proposal module root
│   │   ├── extractor.rs              # ProposalExtractor — parses agent output for state
│   │   │                               #   change proposals (goal changes, reflection writes,
│   │   │                               #   memory writes). Structured extraction from LLM output.
│   │   └── router.rs                 # ProposalRouter — routes extracted proposals to
│   │                                   #   ProposalValidator, handles auto-approve for low-risk,
│   │                                   #   queues significant changes for human review
│   ├── tools/
│   │   ├── mod.rs                      # Tool module root
│   │   ├── registry.rs               # ToolRegistry — registered tools with schemas,
│   │   │                               #   tool lookup by name, schema serialization for LLM
│   │   ├── executor.rs               # ToolExecutor — dispatches tool calls to implementations,
│   │   │                               #   captures stdout/stderr, timeout enforcement,
│   │   │                               #   audit logging (mandatory)
│   │   └── builtin/
│   │       ├── mod.rs                  # Builtin tool module root
│   │       ├── shell.rs               # ShellTool — sandboxed shell execution,
│   │       │                           #   capability-scoped (read-only, write, admin)
│   │       ├── filesystem.rs          # FilesystemTool — scoped file read/write/list
│   │       ├── web_search.rs          # WebSearchTool — internet search via API
│   │       └── memory.rs             # MemoryTool — Cortex memory read/write (via proposals)
│   └── response.rs                     # AgentResponse struct — text, tool_calls, proposals,
│                                       #   cost, token_usage, duration
└── tests/
    ├── runner_tests.rs
    ├── prompt_compiler_tests.rs
    └── proposal_extractor_tests.rs
```

### ghost-policy (Cedar-Style Authorization Engine)

```
crates/ghost-policy/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── engine.rs                       # PolicyEngine — evaluates tool calls against policies.
│   │                                   #   evaluate(action, context) -> PolicyDecision.
│   │                                   #   PolicyDecision: Permit, Deny(reason), Escalate.
│   │                                   #   Denial becomes structured feedback for agent replanning.
│   ├── policy/
│   │   ├── mod.rs                      # Policy module root
│   │   ├── corp_policy.rs            # CorpPolicy — loads CORP_POLICY.md, parses hard constraints.
│   │   │                               #   Immutable root policy. Agent cannot modify or see impl.
│   │   ├── capability_grants.rs       # CapabilityGrant — per-agent tool permissions from ghost.yml.
│   │   │                               #   Deny by default, explicit grants per tool.
│   │   └── convergence_policy.rs     # ConvergencePolicyTightener — automatically restricts
│   │                                   #   capabilities as intervention level rises.
│   │                                   #   Level 0-1: full. Level 2: reduced proactive.
│   │                                   #   Level 3: session caps. Level 4: task-only mode.
│   ├── context.rs                      # PolicyContext — agent_id, tool_name, tool_args,
│   │                                   #   convergence_level, session_duration, time_of_day
│   └── feedback.rs                     # DenialFeedback — structured denial message for agent.
│                                       #   Includes reason, constraint, suggested alternatives.
└── tests/
    ├── policy_engine_tests.rs
    └── convergence_tightening_tests.rs
```

### ghost-llm (Multi-Provider LLM Integration)

```
crates/ghost-llm/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── provider/
│   │   ├── mod.rs                      # Provider module root, LLMProvider trait definition:
│   │   │                               #   complete(), complete_with_tools(),
│   │   │                               #   supports_streaming(), context_window(),
│   │   │                               #   cost_per_token()
│   │   ├── anthropic.rs               # AnthropicProvider — Claude API, streaming SSE,
│   │   │                               #   tool calling, cache control headers
│   │   ├── openai.rs                  # OpenAIProvider — GPT API, streaming, function calling
│   │   ├── google.rs                  # GoogleProvider — Gemini API
│   │   ├── ollama.rs                  # OllamaProvider — local model API (localhost:11434)
│   │   └── openai_compat.rs          # OpenAICompatProvider — any OpenAI-compatible endpoint
│   ├── routing/
│   │   ├── mod.rs                      # Routing module root
│   │   ├── model_router.rs           # ModelRouter — per-task model selection.
│   │   │                               #   Cheap model for heartbeat/routine,
│   │   │                               #   expensive model for complex reasoning.
│   │   │                               #   Configurable per-agent.
│   │   └── fallback.rs               # FallbackChain — deterministic failover cascade.
│   │                                   #   Rotate auth profiles on 401/429.
│   │                                   #   Fall back to next provider if exhausted.
│   ├── streaming.rs                    # StreamingResponse — async stream of response chunks.
│   │                                   #   Chunk enum: Text, ToolCall, Done, Error.
│   │                                   #   Adapter for SSE/WebSocket/NDJSON formats.
│   ├── tokenizer.rs                    # TokenCounter — model-specific token counting.
│   │                                   #   Uses tiktoken-rs for OpenAI models,
│   │                                   #   Anthropic's tokenizer for Claude,
│   │                                   #   approximate byte-based fallback for others.
│   │                                   #   count_tokens(text, model) -> usize.
│   │                                   #   Used by prompt_compiler for budget enforcement
│   │                                   #   and by cost.rs for pre-call estimation.
│   └── cost.rs                         # CostCalculator — per-model input/output token pricing,
│                                       #   cost estimation before call, actual cost after call
└── tests/
    ├── provider_tests.rs
    └── routing_tests.rs
```

### ghost-channels (Channel Adapter Framework)

```
crates/ghost-channels/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── adapter.rs                      # ChannelAdapter trait — connect(), disconnect(),
│   │                                   #   send(), receive(), supports_streaming(),
│   │                                   #   supports_editing(). InboundMessage, OutboundMessage.
│   ├── message.rs                      # Normalized message types — InboundMessage (channel-agnostic),
│   │                                   #   OutboundMessage, MessageAttachment, MessageFormat
│   ├── adapters/
│   │   ├── mod.rs                      # Adapter module root
│   │   ├── cli.rs                     # CLIAdapter — stdin/stdout, ANSI formatting,
│   │   │                               #   streaming display, command parsing (/status, /model)
│   │   ├── websocket.rs              # WebSocketAdapter — axum WebSocket server,
│   │   │                               #   bidirectional streaming, session management,
│   │   │                               #   loopback-only by default
│   │   ├── telegram.rs               # TelegramAdapter — teloxide (Rust Telegram framework),
│   │   │                               #   long polling, message editing for streaming,
│   │   │                               #   group chat support, @mention activation
│   │   ├── discord.rs                # DiscordAdapter — serenity-rs, slash commands,
│   │   │                               #   message editing for streaming, thread support
│   │   ├── slack.rs                   # SlackAdapter — Slack Bolt protocol, WebSocket mode,
│   │   │                               #   thread replies, app mentions
│   │   └── whatsapp.rs              # WhatsAppAdapter — Baileys bridge via Node.js sidecar.
│   │                                   #   Sidecar lifecycle: gateway spawns `node baileys-bridge.js`
│   │                                   #   as a child process on adapter connect(). Communicates
│   │                                   #   via stdin/stdout JSON-RPC. Gateway monitors process
│   │                                   #   health, restarts on crash (max 3 retries, then degrade).
│   │                                   #   Sidecar script lives in: extension/bridges/baileys-bridge/
│   │                                   #   Requires Node.js 18+ on host. WhatsApp Web protocol.
│   └── streaming.rs                    # StreamingFormatter — preview streaming via message edits
│                                       #   (Telegram/Discord/Slack). Chunk buffering, edit throttle.
└── tests/
    ├── adapter_tests.rs
    └── message_normalization_tests.rs
```

### ghost-skills (Skill System + WASM Sandbox)

```
crates/ghost-skills/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── registry.rs                     # SkillRegistry — skill discovery, loading, lookup.
│   │                                   #   Directory-based: workspace > user > bundled.
│   │                                   #   Skill names loaded into context (not bodies).
│   │                                   #   Agent requests full definition via read_skill.
│   ├── loader.rs                       # SkillLoader — YAML frontmatter parsing,
│   │                                   #   signature verification (ed25519-dalek),
│   │                                   #   permission extraction, sandbox tier assignment
│   ├── manifest.rs                     # SkillManifest — name, description, version, signature,
│   │                                   #   permissions, sandbox tier, user-invocable flag
│   ├── signing/
│   │   ├── mod.rs                      # Signing module root
│   │   ├── signer.rs                  # SkillSigner — ed25519 key generation, skill signing
│   │   └── verifier.rs               # SkillVerifier — signature verification on every load,
│   │                                   #   not just install. Quarantine on failure.
│   ├── sandbox/
│   │   ├── mod.rs                      # Sandbox module root
│   │   ├── wasm_sandbox.rs           # WasmSandbox — wasmtime-based execution environment.
│   │   │                               #   Capability-scoped imports. Memory limits.
│   │   │                               #   Timeout enforcement. No raw filesystem/network.
│   │   ├── native_sandbox.rs         # NativeSandbox — for builtin skills. Capability-scoped
│   │   │                               #   but runs native. Used for first-party skills only.
│   │   └── capability.rs            # Capability enum — filesystem (read/write/path-scoped),
│   │                                   #   network (domain-scoped), shell (read-only/write/admin),
│   │                                   #   memory (read/write). Deny by default.
│   └── credential/
│       ├── mod.rs                      # Credential module root
│       └── broker.rs                  # CredentialBroker — stand-in pattern (IronClaw).
│                                       #   Skills never see raw API keys. Broker provides
│                                       #   opaque tokens reified only at execution time
│                                       #   inside the sandbox. Exfiltration-proof.
└── tests/
    ├── registry_tests.rs
    ├── signing_tests.rs
    └── sandbox_tests.rs
```

### ghost-identity (Two-Tier Identity System)

```
crates/ghost-identity/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── soul.rs                         # SoulManager — loads SOUL.md, version tracking,
│   │                                   #   semantic drift detection (embedding comparison
│   │                                   #   against baseline). Read-only to agent.
│   │                                   #   Platform manages evolution.
│   ├── identity.rs                     # IdentityManager — loads IDENTITY.md (name, voice, emoji,
│   │                                   #   channel-specific behavior). Read-only to agent.
│   ├── user.rs                         # UserManager — loads USER.md (human preferences, timezone,
│   │                                   #   communication style). Agent can PROPOSE updates,
│   │                                   #   platform validates via ProposalValidator.
│   ├── corp_policy.rs                  # CorpPolicyLoader — loads CORP_POLICY.md.
│   │                                   #   Immutable root. Agent cannot modify or see
│   │                                   #   implementation. Structural override of all other layers.
│   │                                   #   Signature verification on load using platform.pub key
│   │                                   #   (reuses ghost-skills/signing/verifier.rs via shared
│   │                                   #   ghost-signing crate — see below). Refuses to load
│   │                                   #   if signature invalid or missing.
│   ├── keypair.rs                      # AgentKeypairManager — generates, stores, and loads
│   │                                   #   per-agent Ed25519 keypairs. Keys stored in
│   │                                   #   ~/.ghost/agents/{name}/keys/agent.key (private)
│   │                                   #   and agent.pub (public). Used by cortex-crdt for
│   │                                   #   signed deltas, by ghost-skills for agent-specific
│   │                                   #   signing. Platform generates keypair on agent creation.
│   │                                   #   Rotation: generate new pair, re-sign active deltas,
│   │                                   #   archive old key with expiry timestamp.
│   └── drift_detector.rs              # IdentityDriftDetector — compares current SOUL.md
│                                       #   embedding against baseline. Alerts if semantic
│                                       #   distance exceeds threshold. Prevents Ship of Theseus.
└── tests/
    ├── identity_tests.rs
    └── keypair_tests.rs                # NEW: Keypair generation, rotation, signing roundtrip
```

> **Signing dependency note**: `ghost-identity/corp_policy.rs` and `ghost-skills/signing/`
> both need Ed25519 verification. To avoid `ghost-identity` depending on `ghost-skills`
> (which would create a cycle via `ghost-policy`), extract shared signing primitives into
> a thin `ghost-signing` utility crate (see below).

### ghost-heartbeat (Proactive + Scheduled Execution)

```
crates/ghost-heartbeat/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── heartbeat.rs                    # HeartbeatEngine — periodic ambient monitoring.
│   │                                   #   Configurable interval (default 30m).
│   │                                   #   Active hours + timezone awareness.
│   │                                   #   Cost ceiling per heartbeat run.
│   │                                   #   Convergence-aware frequency (reduces at higher levels).
│   │                                   #   NO_REPLY suppression with cost tracking.
│   ├── cron.rs                         # CronEngine — standard cron syntax, timezone-aware.
│   │                                   #   Morning briefings, EOD summaries, scheduled tasks.
│   │                                   #   Per-job cost tracking.
│   └── scheduler.rs                    # Scheduler — unified scheduling for heartbeat + cron.
│                                       #   Job queue, next-run calculation, missed-run handling.
└── tests/
    └── scheduler_tests.rs
```

### ghost-export (Data Export Analyzer — Retrospective Analysis)

Build priority #5 from delivery architecture. Ingests platform-exported conversation
data (ChatGPT JSON, Character.AI JSON, Google Takeout) for retrospective convergence
analysis. Works without real-time monitoring.

```
crates/ghost-export/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── analyzer.rs                     # ExportAnalyzer — orchestrates import → parse →
│   │                                   #   signal computation → baseline establishment.
│   │                                   #   analyze(path) -> ExportAnalysisResult.
│   │                                   #   Supports incremental re-analysis on new exports.
│   ├── parsers/
│   │   ├── mod.rs                      # Parser module root, ExportParser trait:
│   │   │                               #   detect(path) -> bool, parse(path) -> Vec<ITPEvent>
│   │   ├── chatgpt.rs                 # ChatGPT export parser — Settings → Data Controls
│   │   │                               #   export format. JSON with conversations array,
│   │   │                               #   message objects, timestamps, model metadata.
│   │   │                               #   Maps to ITP SessionStart/End + InteractionMessage.
│   │   ├── character_ai.rs           # Character.AI export parser — Settings → Privacy →
│   │   │                               #   Request Data format. JSON with character turns.
│   │   ├── google_takeout.rs         # Google Takeout parser — Gemini conversation export.
│   │   │                               #   JSON format from Google Takeout.
│   │   └── generic_jsonl.rs          # Generic JSONL parser — for pre-formatted ITP events.
│   │                                   #   Allows manual export from unsupported platforms.
│   ├── timeline.rs                     # TimelineReconstructor — rebuilds session boundaries
│   │                                   #   from exported timestamps. Infers session gaps.
│   │                                   #   Handles timezone normalization.
│   └── report.rs                       # ExportAnalysisResult — per-session signal scores,
│                                       #   overall convergence trajectory, baseline data,
│                                       #   flagged sessions, recommended intervention level.
│                                       #   Serializable to JSON for dashboard display.
└── tests/
    ├── parser_tests.rs                 # Per-platform parser tests with fixture data
    └── fixtures/
        ├── chatgpt_export_sample.json
        ├── character_ai_sample.json
        └── google_takeout_sample.json
```

### ghost-proxy (Local HTTPS Proxy — Power User Delivery)

Build priority #6 from delivery architecture. Local mitmproxy-style interception
for power users who want maximum coverage beyond the browser extension.

```
crates/ghost-proxy/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── proxy.rs                        # ProxyServer — local HTTPS proxy (hyper + rustls).
│   │                                   #   Binds to localhost:8080 (configurable).
│   │                                   #   TLS termination with local CA certificate.
│   │                                   #   Domain filtering — only intercepts AI chat domains.
│   │                                   #   Pass-through mode (read-only, never modifies traffic).
│   ├── ca/
│   │   ├── mod.rs                      # CA module root
│   │   ├── generator.rs              # LocalCA — generates local CA certificate + key pair
│   │   │                               #   on first run. Stores in ~/.ghost/proxy/ca/.
│   │   │                               #   Provides install instructions per OS.
│   │   └── cert_store.rs            # CertStore — per-domain certificate generation,
│   │                                   #   caching, rotation.
│   ├── intercept/
│   │   ├── mod.rs                      # Intercept module root
│   │   ├── domain_filter.rs          # DomainFilter — allowlist of AI chat domains:
│   │   │                               #   chat.openai.com, chatgpt.com, claude.ai,
│   │   │                               #   character.ai, gemini.google.com,
│   │   │                               #   chat.deepseek.com, grok.x.ai.
│   │   │                               #   Non-matching traffic passed through unmodified.
│   │   ├── payload_parser.rs         # PayloadParser trait + per-platform implementations:
│   │   │                               #   ChatGPT (SSE stream), Claude (SSE stream),
│   │   │                               #   Character.AI (WebSocket JSON),
│   │   │                               #   Gemini (streaming JSON).
│   │   │                               #   Extracts sender, content, timestamps.
│   │   └── itp_emitter.rs           # ProxyITPEmitter — converts parsed payloads to ITP
│   │                                   #   events, sends to convergence monitor via unix socket.
│   └── config.rs                       # ProxyConfig — bind address, port, domain allowlist,
│                                       #   CA paths, privacy level, upstream proxy support.
└── tests/
    └── proxy_integration_tests.rs
```

### ghost-backup (Full State Export/Import)

Phase 3 item #29 from Layer 3 research. Full state backup and restore
for disaster recovery and migration between machines.

```
crates/ghost-backup/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── exporter.rs                     # StateExporter — exports full GHOST state to a single
│   │                                   #   encrypted archive (.ghost-backup).
│   │                                   #   Includes: SQLite DB, identity files (SOUL.md,
│   │                                   #   IDENTITY.md, USER.md, CORP_POLICY.md),
│   │                                   #   skills, config (ghost.yml), baselines,
│   │                                   #   session history, signing keys.
│   │                                   #   Archive format: zstd-compressed tar with
│   │                                   #   age encryption (passphrase-based).
│   │                                   #   Includes manifest.json with version, timestamp,
│   │                                   #   content hash for integrity verification.
│   ├── importer.rs                     # StateImporter — restores from .ghost-backup archive.
│   │                                   #   Validates manifest integrity (blake3 hash).
│   │                                   #   Decrypts, decompresses, restores files.
│   │                                   #   Handles version migration if backup is older.
│   │                                   #   Conflict resolution: prompt user for overwrite.
│   ├── manifest.rs                     # BackupManifest — version, created_at, ghost_version,
│   │                                   #   content_hash, file_list, agent_names.
│   │                                   #   Serialized as JSON inside the archive.
│   └── scheduler.rs                    # BackupScheduler — optional automatic backups.
│                                       #   Configurable interval (daily/weekly).
│                                       #   Retention policy (keep N backups).
│                                       #   Stores in ~/.ghost/backups/.
└── tests/
    └── backup_roundtrip_tests.rs       # Export → import → verify state identical
```

### ghost-audit (Audit Log Backend)

Backend for the security dashboard page. Provides queryable, filterable,
exportable audit log access. The dashboard's security page connects here.

```
crates/ghost-audit/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── query.rs                        # AuditQueryEngine — paginated audit log queries.
│   │                                   #   Filter by: time range, agent_id, event_type,
│   │                                   #   severity, tool_name. Full-text search on
│   │                                   #   audit descriptions. Returns AuditPage.
│   ├── export.rs                       # AuditExporter — export audit logs to:
│   │                                   #   JSON (structured), CSV (spreadsheet),
│   │                                   #   JSONL (streaming). Date range filtering.
│   │                                   #   Used by dashboard export button + CLI.
│   ├── aggregation.rs                  # AuditAggregator — summary statistics:
│   │                                   #   violations per day, top violation types,
│   │                                   #   policy denials by tool, boundary violations
│   │                                   #   by pattern. Powers dashboard charts.
│   └── types.rs                        # AuditEntry, AuditPage, AuditFilter,
│                                       #   AuditExportFormat, AuditSummary structs.
└── tests/
    └── query_tests.rs
```

### ghost-migrate (OpenClaw Migration Tool)

Phase 4 item #33 from Layer 3 research. Imports existing OpenClaw agent
configurations into GHOST format.

```
crates/ghost-migrate/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── migrator.rs                     # OpenClawMigrator — orchestrates full migration.
│   │                                   #   detect_openclaw_install() — finds ~/.openclaw/
│   │                                   #   or custom path. migrate() -> MigrationResult.
│   │                                   #   Non-destructive: never modifies source files.
│   ├── importers/
│   │   ├── mod.rs                      # Importer module root
│   │   ├── soul_importer.rs          # SoulImporter — reads OpenClaw SOUL.md,
│   │   │                               #   maps to GHOST SOUL.md format.
│   │   │                               #   Strips agent-mutable sections.
│   │   │                               #   Preserves personality, constraints, voice.
│   │   ├── memory_importer.rs        # MemoryImporter — reads OpenClaw MEMORY.md,
│   │   │                               #   converts to Cortex typed memories.
│   │   │                               #   Maps OpenClaw free-form entries to
│   │   │                               #   appropriate MemoryType variants.
│   │   │                               #   Assigns conservative importance levels.
│   │   ├── skill_importer.rs         # SkillImporter — reads OpenClaw skills directory,
│   │   │                               #   converts YAML frontmatter format.
│   │   │                               #   Strips incompatible permissions.
│   │   │                               #   Quarantines community skills (unsigned).
│   │   └── config_importer.rs        # ConfigImporter — reads OpenClaw config,
│   │                                   #   maps to ghost.yml format. Channel bindings,
│   │                                   #   model selection, spending caps.
│   └── report.rs                       # MigrationResult — imported items, skipped items,
│                                       #   warnings (e.g. "SOUL.md had mutable sections,
│                                       #   stripped"), recommended manual review items.
└── tests/
    └── migration_tests.rs
```

### ghost-mesh (ClawMesh Agent-to-Agent Payment Protocol — Placeholder)

Referenced in AGENT_ARCHITECTURE_v2.md section 5 as a "fully designed protocol."
Phase 4+ timeline. Placeholder crate with trait definitions and types only.

```
crates/ghost-mesh/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # Crate root — re-exports types + traits.
│   │                                   #   Feature-gated: `#[cfg(feature = "mesh")]`
│   │                                   #   Not compiled by default.
│   ├── types.rs                        # MeshTransaction, MeshInvoice, MeshReceipt,
│   │                                   #   MeshWallet, MeshEscrow, SettlementStatus.
│   │                                   #   All types derive Serialize/Deserialize + ts-rs.
│   ├── traits.rs                       # IMeshProvider trait — create_invoice(),
│   │                                   #   pay_invoice(), check_balance(), escrow(),
│   │                                   #   release_escrow(), dispute().
│   │                                   #   IMeshLedger trait — append_transaction(),
│   │                                   #   query_transactions(), verify_receipt().
│   └── protocol.rs                     # MeshProtocol — message format for agent-to-agent
│                                       #   payment negotiation. Request, Accept, Reject,
│                                       #   Complete, Dispute message types.
│                                       #   Stub implementations that return Unimplemented.
└── tests/
    └── types_tests.rs                  # Serialization roundtrip tests only
```

---

## CONFIGURATION SCHEMA

### ghost.yml JSON Schema (Validation Spec)

```
schemas/
├── ghost-config.schema.json            # JSON Schema for ghost.yml validation.
│                                       #   Defines: agents (array of agent configs),
│                                       #   channels (adapter bindings), models (provider configs),
│                                       #   security (spending caps, auth, loopback-only),
│                                       #   convergence (thresholds, signal weights, contacts),
│                                       #   heartbeat (interval, active hours, cost ceiling),
│                                       #   proxy (enabled, port, domain allowlist),
│                                       #   backup (schedule, retention).
│                                       #   Used by ghost-gateway config loader for validation.
│                                       #   Used by dashboard settings page for form generation.
└── ghost-config.example.yml            # Annotated example configuration with all options
                                        #   documented inline. Ships with the platform.
```

> **Gap 14 fix**: Explicit JSON schema for ghost.yml validation.

---

## BROWSER EXTENSION (Product 1: Passive Convergence Monitor)

```
extension/
├── manifest.json                       # Chrome Manifest V3
├── manifest.firefox.json               # Firefox manifest (generated from Chrome)
├── package.json                        # Build tooling (vite, typescript)
├── tsconfig.json
├── vite.config.ts
├── src/
│   ├── background/
│   │   ├── service-worker.ts          # Background service worker — receives events from
│   │   │                               #   content scripts, computes ITP signals,
│   │   │                               #   stores session data (IndexedDB),
│   │   │                               #   runs convergence detection,
│   │   │                               #   triggers notifications,
│   │   │                               #   manages native messaging connection
│   │   ├── itp-emitter.ts            # ITP event construction + emission —
│   │   │                               #   builds ITP events from raw DOM data,
│   │   │                               #   applies privacy level (hash/plaintext),
│   │   │                               #   sends to native messaging host or IndexedDB
│   │   ├── signal-computer.ts        # Client-side signal computation —
│   │   │                               #   basic signals (duration, latency, frequency)
│   │   │                               #   computed in extension for real-time display.
│   │   │                               #   Heavy computation delegated to Rust sidecar.
│   │   └── native-bridge.ts          # Native messaging bridge to Rust convergence monitor —
│   │                                   #   connection management, message framing,
│   │                                   #   reconnection logic, fallback to IndexedDB
│   ├── content/
│   │   ├── adapter-manager.ts         # PlatformAdapterManager — detects current platform,
│   │   │                               #   loads appropriate adapter, manages lifecycle
│   │   └── adapters/
│   │       ├── base-adapter.ts        # BasePlatformAdapter — abstract class.
│   │       │                           #   matches(url), getMessageContainerSelector(),
│   │       │                           #   parseMessage(element), observeNewMessages(callback)
│   │       ├── chatgpt-adapter.ts     # ChatGPT DOM adapter (chat.openai.com, chatgpt.com)
│   │       ├── claude-adapter.ts      # Claude.ai DOM adapter
│   │       ├── character-adapter.ts   # Character.AI DOM adapter
│   │       ├── gemini-adapter.ts      # Gemini DOM adapter (gemini.google.com)
│   │       ├── deepseek-adapter.ts    # DeepSeek DOM adapter (chat.deepseek.com)
│   │       └── grok-adapter.ts       # Grok DOM adapter (grok.x.ai)
│   ├── popup/
│   │   ├── App.svelte                 # Extension popup — current session signals,
│   │   │                               #   convergence score gauge, quick actions
│   │   ├── main.ts                    # Popup entry point
│   │   └── components/
│   │       ├── ScoreGauge.svelte      # Visual convergence score (0-1 gauge)
│   │       ├── SignalList.svelte      # Individual signal breakdown
│   │       ├── SessionTimer.svelte    # Current session duration
│   │       └── AlertBanner.svelte    # Active intervention notification
│   ├── dashboard/
│   │   ├── App.svelte                 # Full dashboard (opens in new tab) —
│   │   │                               #   historical trends, signal charts, session history
│   │   ├── main.ts                    # Dashboard entry point
│   │   ├── pages/
│   │   │   ├── Overview.svelte        # Dashboard overview — composite score trend,
│   │   │   │                           #   recent sessions, active alerts
│   │   │   ├── Signals.svelte        # Per-signal detail view with charts
│   │   │   ├── Sessions.svelte       # Session history list with drill-down
│   │   │   └── Settings.svelte       # Configuration — privacy level, thresholds,
│   │   │                               #   contacts, notification preferences
│   │   └── components/
│   │       ├── SignalChart.svelte     # Time-series chart for individual signals
│   │       ├── SessionCard.svelte    # Session summary card
│   │       └── InterventionLog.svelte # Intervention history timeline
│   ├── storage/
│   │   ├── idb-store.ts              # IndexedDB wrapper — session storage, event storage,
│   │   │                               #   baseline storage, config storage
│   │   └── sync.ts                   # Chrome storage sync for settings across devices
│   └── shared/
│       ├── types.ts                   # Shared TypeScript types (ITP events, signals, config)
│       ├── constants.ts              # Platform URLs, default thresholds, version
│       └── privacy.ts               # Content hashing, privacy level enforcement
└── tests/
    ├── adapters/
    │   ├── chatgpt-adapter.test.ts
    │   └── claude-adapter.test.ts
    └── signal-computer.test.ts
```

---

## WEB DASHBOARD (Platform Product 2)

```
dashboard/
├── package.json
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json
├── src/
│   ├── app.html                        # HTML shell
│   ├── app.css                         # Global styles
│   ├── routes/
│   │   ├── +layout.svelte             # Root layout — sidebar navigation, auth check
│   │   ├── +page.svelte               # Home — convergence overview, active agents
│   │   ├── convergence/
│   │   │   └── +page.svelte           # Convergence monitor — real-time scores,
│   │   │                               #   signal breakdown, intervention history
│   │   ├── memory/
│   │   │   └── +page.svelte           # Memory explorer — browse typed memories,
│   │   │                               #   causal graph visualization, search
│   │   ├── goals/
│   │   │   └── +page.svelte           # Goal tracker — active goals, pending proposals,
│   │   │                               #   approval queue
│   │   ├── reflections/
│   │   │   └── +page.svelte           # Reflection audit — chain visualization,
│   │   │                               #   depth tracking, self-reference analysis
│   │   ├── sessions/
│   │   │   └── +page.svelte           # Session history — transcripts, cost tracking,
│   │   │                               #   compaction events
│   │   ├── agents/
│   │   │   └── +page.svelte           # Agent config — SOUL.md editor, skill management,
│   │   │                               #   channel bindings, model selection
│   │   ├── security/
│   │   │   └── +page.svelte           # Security dashboard — audit log, boundary violations,
│   │   │                               #   skill signatures, policy violations
│   │   └── settings/
│   │       └── +page.svelte           # Platform settings — convergence thresholds,
│   │                                   #   contact configuration, privacy levels
│   ├── lib/
│   │   ├── api.ts                     # API client — WebSocket connection to ghost-gateway,
│   │   │                               #   REST endpoints, real-time event subscription
│   │   ├── stores/
│   │   │   ├── convergence.ts         # Svelte store for convergence state
│   │   │   ├── sessions.ts           # Svelte store for session data
│   │   │   └── agents.ts            # Svelte store for agent configuration
│   │   └── components/
│   │       ├── ScoreGauge.svelte     # Reusable convergence score gauge
│   │       ├── SignalChart.svelte    # Reusable time-series signal chart
│   │       ├── MemoryCard.svelte    # Memory display card with type badge
│   │       ├── GoalCard.svelte      # Goal display with approval actions
│   │       ├── CausalGraph.svelte   # Interactive causal graph (D3 or similar)
│   │       └── AuditTimeline.svelte # Scrollable audit event timeline
│   └── static/
│       └── favicon.png
└── tests/
    └── component-tests/
```

---

## IDENTITY + CONFIG FILES (Per-Agent, User-Editable)

```
~/.ghost/
├── config/
│   └── ghost.yml                       # Platform configuration — agents, channels, models,
│                                       #   spending caps, security settings
├── agents/
│   └── {agent-name}/
│       ├── CORP_POLICY.md              # Immutable root policy (signed, platform-managed)
│       ├── SOUL.md                     # Agent personality (read-only to agent)
│       ├── IDENTITY.md                 # Agent identity (name, voice, emoji)
│       ├── USER.md                     # Human preferences (agent can propose updates)
│       ├── MEMORY.md                   # Long-term curated memory (loaded in private sessions)
│       ├── memory/
│       │   ├── {date}.md              # Daily logs (agent read-write working memory)
│       │   └── ...
│       ├── skills/
│       │   └── ...                     # Agent-specific skills
│       └── workspace/
│           └── ...                     # Agent-specific workspace files
├── skills/
│   ├── builtin/                        # First-party skills (signed by platform)
│   │   └── drift-code-intelligence/   # Drift as first-party skill pack
│   ├── community/                      # Community skills (quarantined until approved)
│   └── keys/
│       ├── platform.pub               # Platform signing public key
│       └── trusted/                   # Trusted community signer public keys
├── sessions/
│   └── {session-id}/
│       ├── events.jsonl               # ITP events for this session
│       └── analysis.json             # Computed signals and scores
├── baselines/
│   └── {agent-instance-id}.json      # Per-agent baseline data
├── data/
│   └── ghost.db                       # SQLite database (Cortex + convergence tables)
├── backups/
│   └── ...                            # State backups
└── logs/
    ├── gateway.log                    # Gateway process log
    └── monitor.log                   # Convergence monitor log
```

---

## WORKSPACE CARGO.TOML (Root)

```toml
[workspace]
resolver = "2"
members = [
    # Layer 1: Infrastructure (existing)
    "crates/cortex/cortex-core",
    "crates/cortex/cortex-tokens",
    "crates/cortex/cortex-storage",
    "crates/cortex/cortex-embeddings",
    "crates/cortex/cortex-privacy",
    "crates/cortex/cortex-compression",
    "crates/cortex/cortex-decay",
    "crates/cortex/cortex-causal",
    "crates/cortex/cortex-retrieval",
    "crates/cortex/cortex-validation",
    "crates/cortex/cortex-learning",
    "crates/cortex/cortex-consolidation",
    "crates/cortex/cortex-prediction",
    "crates/cortex/cortex-session",
    "crates/cortex/cortex-reclassification",
    "crates/cortex/cortex-observability",
    "crates/cortex/cortex-cloud",
    "crates/cortex/cortex-temporal",
    "crates/cortex/cortex-napi",
    "crates/cortex/cortex-crdt",
    "crates/cortex/cortex-multiagent",
    "crates/cortex/cortex-convergence",
    "crates/cortex/cortex-drift-bridge",
    "crates/cortex/test-fixtures",
    "crates/drift/drift-core",
    "crates/drift/drift-napi",

    # Layer 2: Convergence Safety (new)
    "crates/convergence-monitor",
    "crates/simulation-boundary",
    "crates/itp-protocol",
    "crates/read-only-pipeline",

    # Layer 3: Agent Platform (new)
    "crates/ghost-gateway",
    "crates/ghost-agent-loop",
    "crates/ghost-policy",
    "crates/ghost-llm",
    "crates/ghost-channels",
    "crates/ghost-skills",
    "crates/ghost-identity",
    "crates/ghost-heartbeat",
    "crates/ghost-export",
    "crates/ghost-proxy",
    "crates/ghost-backup",
    "crates/ghost-audit",
    "crates/ghost-migrate",
    # "crates/ghost-mesh",          # Phase 4+ — uncomment when ClawMesh protocol is designed
]
```

---

## DEPENDENCY GRAPH (Build Order)

```
cortex-core
    ↓
cortex-tokens, cortex-storage, cortex-embeddings, cortex-privacy
    ↓
cortex-decay, cortex-temporal, cortex-compression
    ↓
cortex-causal, cortex-validation, cortex-crdt
    ↓
cortex-retrieval, cortex-multiagent, cortex-session
    ↓
cortex-convergence, cortex-learning, cortex-consolidation, cortex-prediction
    ↓
cortex-reclassification, cortex-observability, cortex-cloud
    ↓
cortex-drift-bridge, cortex-napi
    ↓
itp-protocol                    (depends on: cortex-core)
    ↓
simulation-boundary             (depends on: cortex-core, cortex-validation)
    ↓
read-only-pipeline              (depends on: cortex-core, cortex-convergence, cortex-retrieval)
    ↓
convergence-monitor             (depends on: cortex-convergence, itp-protocol, simulation-boundary)
    ↓
ghost-llm                       (depends on: cortex-core)
ghost-policy                    (depends on: cortex-core, ghost-identity)
ghost-identity                  (depends on: cortex-core, cortex-embeddings)
ghost-skills                    (depends on: cortex-core, ghost-policy)
    ↓
ghost-agent-loop                (depends on: ghost-llm, ghost-policy, ghost-skills,
                                 cortex-convergence, read-only-pipeline, simulation-boundary,
                                 cortex-validation, cortex-retrieval)
    ↓
ghost-channels                  (depends on: cortex-core)
ghost-heartbeat                 (depends on: ghost-agent-loop, cortex-convergence)
ghost-export                    (depends on: itp-protocol, cortex-convergence)
ghost-proxy                     (depends on: itp-protocol)
ghost-backup                    (depends on: cortex-storage, ghost-identity)
ghost-audit                     (depends on: cortex-storage)
ghost-migrate                   (depends on: cortex-core, ghost-identity, ghost-skills)
    ↓
ghost-gateway                   (depends on: ghost-agent-loop, ghost-channels, ghost-heartbeat,
                                 convergence-monitor, cortex-storage, cortex-session,
                                 ghost-audit, ghost-backup, ghost-export)
```

---

## FILE COUNT SUMMARY

| Layer | Crates | New Files | Modified Files |
|-------|--------|-----------|----------------|
| Layer 1 (Cortex extensions) | 0 new, 9 modified | ~20 | ~15 |
| Layer 2 (Convergence Safety) | 4 new | ~35 | 0 |
| Layer 3 (Agent Platform) | 13 new | ~95 | 0 |
| Browser Extension | 1 package | ~30 | 0 |
| Web Dashboard | 1 package | ~25 | 0 |
| Config/Identity/Schemas | — | ~12 | 0 |
| **Total** | **17 new crates** | **~217 files** | **~15 files** |

---

## BUILD PHASE MAPPING

| Phase | Crates | Deliverable |
|-------|--------|-------------|
| Phase 1 (Weeks 1-2) | cortex-core mods, cortex-storage v016/v017, cortex-temporal hash_chain + anchoring, cortex-decay convergence factor | Tamper-evident foundation |
| Phase 2 (Weeks 3-4) | cortex-convergence, cortex-validation D5-D7, itp-protocol, simulation-boundary | Convergence detection + proposal validation |
| Phase 3 (Weeks 5-6) | convergence-monitor, read-only-pipeline, cortex-crdt signed deltas | Standalone convergence monitor binary |
| Phase 4 (Weeks 7-8) | ghost-llm, ghost-policy, ghost-identity, ghost-agent-loop | Working agent via CLI |
| Phase 5 (Weeks 9-10) | ghost-channels, ghost-skills (+ drift bridge), ghost-heartbeat | Multi-channel + skills |
| Phase 6 (Weeks 11-12) | ghost-gateway (+ API + agent registry + isolation), ghost-audit, dashboard | Full platform |
| Phase 7 (Weeks 13-14) | extension/, ghost-export, ghost-proxy | Browser extension + supplementary delivery |
| Phase 8 (Weeks 15-16) | ghost-backup, ghost-migrate, schemas/, hardening, adversarial testing, docs | Launch-ready |
| Phase 9 (Future) | ghost-mesh | ClawMesh agent-to-agent payments |

---

## AUDIT FINDINGS (23 Items)

> Audit date: 2026-02-27
> Cross-referenced against: AGENT_ARCHITECTURE.md, AGENT_ARCHITECTURE_v2.md,
> explore/docs/11-delivery-architecture.md, explore/docs/19-implementation-guide.md,
> explore/docs/20-layer3-agent-platform-research.md, and the actual cortex codebase
> at agent/explore/drift-repo/crates/cortex/.
>
> Severity: CRITICAL = blocks implementation, HIGH = causes wrong assumptions,
> MEDIUM = implementer friction, LOW = cosmetic / documentation debt.

---

### FINDING 1 — CRITICAL: `ghost-signing` crate referenced but never mapped

Line 993 says "extract shared signing primitives into a thin `ghost-signing` utility
crate (see below)" — but there is no "below." No file tree, no Cargo.toml entry, no
workspace member, no dependency graph entry, no build phase assignment.

Both `ghost-identity/corp_policy.rs` and `ghost-skills/signing/` depend on it to avoid
a circular dependency (`ghost-identity` → `ghost-skills` → `ghost-policy` → `ghost-identity`).
Without this crate, the dependency cycle is unresolvable.

**Resolution required**: Add full file tree, workspace member entry, dependency graph
node, and build phase assignment. Suggested structure:

```
crates/ghost-signing/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── keypair.rs              # Ed25519 keypair generation, storage, loading
│   ├── signer.rs               # sign(payload, private_key) -> Signature
│   └── verifier.rs             # verify(payload, signature, public_key) -> bool
└── tests/
    └── signing_roundtrip.rs
```

Add to workspace members. Add to dependency graph before `ghost-identity` and `ghost-skills`.
Build phase: Phase 4 (alongside ghost-identity).

---

### FINDING 2 — MEDIUM: `cortex-drift-bridge` has no file tree

Listed in workspace members and "Remaining Existing Cortex Crates" with "No changes"
but has zero description of what it contains. It bridges Drift ↔ Cortex, but
`ghost-skills/bridges/drift_bridge.rs` bridges Drift MCP → SkillRegistry. Two different
bridges doing two different things — the relationship is unclear.

**Resolution required**: Add a one-paragraph description of what `cortex-drift-bridge`
does vs. `ghost-skills/bridges/drift_bridge.rs`, and confirm no convergence-related
modifications are needed.

---

### FINDING 3 — CRITICAL: Kill switch / emergency stop has no file mapping

Architecture v1 §20 defines a 3-level kill switch (PAUSE, QUARANTINE, KILL ALL) with
7 auto-triggers (SOUL.md drift >25%, spending cap exceeded, 5+ policy denials, sandbox
escape, credential exfiltration, 3+ agents quarantined, memory health <0.3). This is a
core safety feature with zero file mapping.

`ghost-gateway/gateway.rs` mentions "lifecycle management" but that's vague. The kill
switch needs explicit ownership.

**Resolution required**: Add kill switch files. Suggested location:

```
crates/ghost-gateway/src/
│   ├── safety/
│   │   ├── mod.rs                  # Safety module root
│   │   ├── kill_switch.rs          # KillSwitch — 3 levels (Pause, Quarantine, KillAll).
│   │   │                           #   Cannot be overridden by any agent.
│   │   │                           #   Logged to append-only audit trail.
│   │   │                           #   Requires owner auth to resume.
│   │   ├── auto_triggers.rs        # AutoTriggerEvaluator — evaluates 7 auto-kill conditions.
│   │   │                           #   Polls convergence monitor, policy engine, cortex health.
│   │   └── quarantine.rs           # QuarantineManager — agent isolation, capability revocation,
│   │                               #   forensic state preservation.
```

---

### FINDING 4 — CRITICAL: Inter-agent communication protocol has no file mapping

Architecture v1 §18 defines signed agent-to-agent messages with ed25519 signatures,
nonce+timestamp replay prevention, optional encryption, and 4 communication patterns
(request/response, fire-and-forget, task delegation with escrow, broadcast). This is
the OWASP ASI07 mitigation. Zero files mapped.

`ghost-gateway/routing/message_router.rs` handles inbound→agent routing but not
agent→agent. Where does message signing, verification, replay prevention, and the
agent message queue live?

**Resolution required**: Add inter-agent messaging files. Suggested location:

```
crates/ghost-gateway/src/
│   ├── messaging/
│   │   ├── mod.rs                  # Inter-agent messaging module root
│   │   ├── protocol.rs            # AgentMessage struct, MessageType enum
│   │   │                           #   (TaskRequest, TaskResponse, Notification, Broadcast).
│   │   │                           #   Signed with sender's ed25519 key.
│   │   │                           #   Nonce + timestamp for replay prevention.
│   │   ├── dispatcher.rs          # MessageDispatcher — routes agent→agent messages
│   │   │                           #   through gateway. Verifies signatures. Checks policy.
│   │   │                           #   Logs to audit trail. Queues for offline agents.
│   │   └── encryption.rs          # Optional payload encryption with recipient's public key.
```

---

### FINDING 5 — HIGH: Workflow/Pipeline engine is completely absent

Architecture v1 §16 defines a full deterministic pipeline system (the OpenClaw Lobster
equivalent) with YAML pipeline definitions, approval gates, resume tokens, per-step
audit logging, and agent-authored pipelines. This was called out as a major cost
optimization (1 LLM turn vs. 4+) and UX differentiator.

FILE_MAPPING has zero mention of pipelines. No crate, no files, no build phase.

**Resolution required**: Either explicitly descope with a note ("Pipelines deferred to
Phase N — agent uses raw tool calls initially") OR add a `ghost-pipelines` crate:

```
crates/ghost-pipelines/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── engine.rs                   # PipelineEngine — loads, validates, executes pipelines.
│   │                               #   Deterministic step execution. Resume token management.
│   ├── parser.rs                   # PipelineParser — YAML pipeline definition parsing.
│   │                               #   Step types: command, llm-task, approval.
│   ├── approval.rs                 # ApprovalGate — halts pipeline, issues resume token,
│   │                               #   waits for human approve/deny.
│   └── audit.rs                    # PipelineAuditLogger — per-step input/output/duration/cost.
├── pipelines/                      # Bundled pipeline definitions
│   └── examples/
│       └── email-triage.pipeline.yml
└── tests/
    └── pipeline_tests.rs
```

---

### FINDING 6 — MEDIUM: Complexity classifier missing from `ghost-llm`

Architecture v1 §15 defines a 4-tier model routing system (FREE/CHEAP/STANDARD/PREMIUM)
with a heuristic complexity classifier. `ghost-llm/routing/model_router.rs` mentions
"per-task model selection" but there's no `classifier.rs` or equivalent. The classifier
is the key piece — without it, the router has nothing to route on.

**Resolution required**: Add to `ghost-llm/src/routing/`:

```
│   │   ├── classifier.rs          # ComplexityClassifier — lightweight heuristic (NOT an LLM call).
│   │                               #   Classifies incoming messages into Tier 0-3.
│   │                               #   Rules: message length, tool keywords, greeting patterns,
│   │                               #   heartbeat context, user slash command overrides (/deep, /quick).
```

---

### FINDING 7 — HIGH: `cortex-temporal` mapping doesn't match actual codebase

The real crate has 7 submodules: `drift/`, `dual_time/`, `epistemic/`, `event_store/`,
`query/`, `snapshot/`, `views/` plus root-level `engine.rs`, `hash_chain.rs`, `merkle.rs`.

FILE_MAPPING shows a simplified structure with `events.rs`, `snapshots.rs`,
`reconstruction.rs` that don't exist in the real crate. More critically:

- `hash_chain.rs` already EXISTS at `src/hash_chain.rs` — mapping says NEW
- `merkle.rs` already EXISTS at `src/merkle.rs` — mapping implies new via `anchoring/merkle.rs`
- `snapshots.rs` doesn't exist — real crate has `snapshot/` directory with 7 files
- `events.rs` doesn't exist — real crate has `event_store/` directory with 6 files
- `reconstruction.rs` doesn't exist — real crate has `snapshot/reconstruct.rs`

**Resolution required**: Rewrite the cortex-temporal section to reflect actual structure.
Mark `hash_chain.rs` and `merkle.rs` as MODIFY (not NEW). Specify that `anchoring/`
is a new sibling directory alongside existing modules. Map new code to existing files
where appropriate (e.g., snapshot integrity → `snapshot/integrity.rs` which already exists).

---

### FINDING 8 — MEDIUM: `cortex-core` mapping omits many existing files

Real crate has files the mapping doesn't account for that may need convergence modifications:

- `config/`: Missing `cloud_config.rs`, `consolidation_config.rs`, `embedding_config.rs`,
  `observability_config.rs`, `privacy_config.rs`, `storage_config.rs`, `temporal_config.rs`,
  `defaults.rs`
- `errors/`: Missing `causal_error.rs`, `cloud_error.rs`, `consolidation_error.rs`,
  `embedding_error.rs`, `multiagent_error.rs`, `retrieval_error.rs`, `storage_error.rs`,
  `temporal_error.rs`
- `traits/`: Missing `health_reporter.rs`, `temporal_engine.rs`, `multiagent_engine.rs`,
  `embedding.rs`, `sanitizer.rs`, `compressor.rs`, `consolidator.rs`, `learner.rs`,
  `predictor.rs`, `causal_storage.rs`
- `models/`: Missing `audit_entry.rs`, `drift_alert.rs`, `drift_snapshot.rs`,
  `materialized_view.rs`, `temporal_event.rs`, `temporal_query.rs`, `temporal_diff.rs`,
  and 15+ others
- `memory/`: Missing `links.rs`, `relationships.rs`
- `intent/`: Missing `weights.rs`

**Resolution required**: Either confirm "no convergence changes needed" for each omitted
file, or audit each for required modifications. At minimum, `config/defaults.rs` likely
needs convergence default values.

---

### FINDING 9 — LOW: Current vs. target state markers are ambiguous

Implementation guide says `LATEST_VERSION = 15`, `Intent` has 18 variants, `MemoryType`
has 23 variants. FILE_MAPPING says `LATEST_VERSION = 17`, `Intent` has 22 variants,
`MemoryType` has 31 variants. These are correct for POST-implementation state, but the
mapping mixes current and target state without clear markers.

**Resolution required**: Add a convention note at the top: "All counts and version numbers
in this document reflect TARGET state after implementation, not current state. See
implementation guide (doc 19) for current baseline values."

---

### FINDING 10 — HIGH: Deployment infrastructure has no file mapping

Architecture v1 §22 defines 3 deployment profiles (Developer Box, Homelab, Production)
with Docker Compose, Tailscale, and a deployment checklist. These are real files that
need to exist.

**Resolution required**: Add deployment files to monorepo root:

```
ghost/
├── Dockerfile                          # Multi-stage build for ghost-gateway binary
├── docker-compose.yml                  # Profile 2: Homelab deployment
├── docker-compose.prod.yml             # Profile 3: Production multi-node
├── deploy/
│   ├── README.md                       # Deployment guide for all 3 profiles
│   ├── ghost.service                   # systemd unit file (Profile 1)
│   └── tailscale-setup.sh             # Tailscale Serve configuration helper
```

Build phase: Phase 8 (alongside hardening).

---

### FINDING 11 — HIGH: Adversarial test suite has no file mapping

Architecture v1 §23 defines a comprehensive cross-cutting adversarial test structure:
prompt injection, identity attacks, exfiltration, privilege escalation, cascading failure,
supply chain. Phase 8 says "adversarial testing" but there's no file tree.

Per-crate property tests and stress tests ARE mapped. The cross-cutting security suite is not.

**Resolution required**: Add top-level adversarial test directory:

```
tests/
├── adversarial/
│   ├── prompt_injection/
│   │   ├── email_injection.rs          # Malicious instructions in email body
│   │   ├── web_content_injection.rs    # Hidden instructions in web pages
│   │   └── skill_injection.rs          # Malicious SKILL.md content
│   ├── identity_attacks/
│   │   ├── soul_modification.rs        # Agent tries to modify SOUL.md constraints
│   │   ├── soul_drift.rs              # Gradual poisoning over sessions
│   │   └── policy_bypass.rs           # Attempts to circumvent CORP_POLICY.md
│   ├── exfiltration/
│   │   ├── credential_leak.rs         # Agent tries to output API keys
│   │   └── memory_exfil.rs            # Agent tries to send memory externally
│   ├── privilege_escalation/
│   │   ├── tool_abuse.rs              # Using allowed tools for unintended purposes
│   │   └── cross_agent_access.rs      # Agent A tries to access Agent B's data
│   └── cascading_failure/
│       ├── tool_failure_cascade.rs    # Chain of tool failures
│       └── compaction_failure.rs      # Memory flush fails at high token count
```

Build phase: Phase 8.

---

### FINDING 12 — CRITICAL: `ghost-signing` missing from workspace Cargo.toml

Even once Finding 1 is resolved with a file tree, the crate is not listed in the
workspace `[workspace] members` list or the dependency graph. Both `ghost-identity`
and `ghost-skills` need it as a dependency.

**Resolution required**: Add `"crates/ghost-signing"` to workspace members (Layer 3
section). Add to dependency graph:

```
ghost-signing                    (depends on: nothing — leaf crate, ed25519-dalek only)
    ↓
ghost-identity                   (depends on: cortex-core, cortex-embeddings, ghost-signing)
ghost-skills                     (depends on: cortex-core, ghost-policy, ghost-signing)
```

---

### FINDING 13 — MEDIUM: `ghost-export` missing Claude.ai parser

Export parsers cover ChatGPT, Character.AI, Google Takeout, and generic JSONL. Claude.ai
is a primary target platform (listed in browser extension adapters, proxy domain filter,
and delivery architecture). Claude offers data export via Settings → Account → Export Data.

**Resolution required**: Add `claude.rs` to `ghost-export/src/parsers/`:

```
│   │   ├── claude.rs                  # Claude.ai export parser — Settings → Account →
│   │                                   #   Export Data format. JSON with conversation objects.
```

Also add `claude_export_sample.json` to test fixtures.

---

### FINDING 14 — MEDIUM: WhatsApp Baileys bridge path doesn't exist in mapping

`ghost-channels/adapters/whatsapp.rs` references `extension/bridges/baileys-bridge/`
as the sidecar script location. The `extension/` section only maps the browser extension.
There's no `bridges/` directory anywhere in the file mapping.

**Resolution required**: Either move the Baileys bridge to a mapped location or add it:

```
bridges/
└── baileys-bridge/
    ├── package.json                    # Node.js dependencies (baileys, etc.)
    ├── baileys-bridge.js              # JSON-RPC stdin/stdout bridge to WhatsApp Web.
    │                                   #   Spawned by ghost-channels WhatsAppAdapter.
    │                                   #   Health monitoring, max 3 restart retries.
    └── README.md                       # Setup instructions, Node.js 18+ requirement
```

---

### FINDING 15 — LOW: `ghost.toml` vs `ghost.yml` inconsistency

Monorepo root shows `ghost.toml` as "Default platform configuration." The rest of the
entire document uses `ghost.yml` (YAML). The schemas section has `ghost-config.example.yml`.
`ghost-gateway/config/loader.rs` says "YAML parsing."

**Resolution required**: Remove `ghost.toml` from monorepo root or rename to `ghost.yml`.
Pick one format and be consistent. YAML is used everywhere else — go with `ghost.yml`.

---

### FINDING 16 — LOW: Prompt compiler layer count mismatch with Architecture v1

Architecture v1 §13 defines 8 context layers (L0-L7). FILE_MAPPING's prompt_compiler.rs
defines 10 layers (L0-L9), adding L1 (simulation boundary) and L6 (convergence state).

This is correct evolution for v2, but creates a discrepancy with the v1 architecture doc.

**Resolution required**: Either update Architecture v1 to match, or add a note in
FILE_MAPPING: "Prompt compiler evolved from 8-layer (v1) to 10-layer (v2) by adding
simulation boundary injection (L1) and convergence state (L6)."

---

### FINDING 17 — MEDIUM: Convergence monitor crash recovery is underspecified

`ghost-gateway/bootstrap.rs` mentions DEGRADED mode if monitor is unreachable at startup.
`health.rs` mentions checking monitor health. But there's no mapping for:

- How does the gateway detect monitor crash MID-SESSION?
- What's the health check interval?
- Where does the periodic retry/reconnect logic live?
- What happens to in-flight convergence scores when the monitor dies?

**Resolution required**: Add to `ghost-gateway/src/health.rs` description:
"Periodic monitor health check (configurable interval, default 30s). On monitor
unreachable: transition to DEGRADED mode, log critical warning, begin periodic
reconnection attempts (exponential backoff, max 5min). In-flight sessions continue
without convergence scoring. Convergence-dependent features (memory filtering,
intervention triggers) fall back to permissive defaults."

---

### FINDING 18 — HIGH: `cortex-crdt` signing module already exists — mapping implies NEW

The real crate already has `src/signing/key_registry.rs`, `src/signing/signed_delta.rs`,
`src/signing/verifier.rs` as a full directory module. FILE_MAPPING says to ADD
`src/signing.rs` (NEW) and `src/sybil.rs` (NEW), implying signing doesn't exist.

The signing module is already implemented. What's needed is MODIFICATION of the existing
signing module (add sybil resistance) and a new `src/sybil.rs` file.

**Resolution required**: Rewrite the cortex-crdt section to reflect actual structure:

```
crates/cortex/cortex-crdt/
├── src/
│   ├── signing/                        # EXISTING directory — MODIFY
│   │   ├── mod.rs                      # EXISTING — ADD re-export of sybil module
│   │   ├── key_registry.rs            # EXISTING — ADD agent keypair registration via platform
│   │   ├── signed_delta.rs            # EXISTING — already has SignedDelta<T> wrapper
│   │   └── verifier.rs               # EXISTING — ADD reject unsigned/invalid deltas
│   ├── memory/
│   │   └── merge_engine.rs            # EXISTING — ADD signature verification before merge
│   └── sybil.rs                        # NEW: Spawn rate limiter (max 3 child agents/parent/24h),
│                                       #   trust caps (new agents 0.3, cap 0.6 for <7d agents)
```

---

### FINDING 19 — MEDIUM: Dashboard auth flow is incomplete

`ghost-gateway/api/middleware.rs` mentions "Dashboard auth: Bearer token from GHOST_TOKEN
env var." The dashboard SvelteKit app's `lib/api.ts` says "API client" with no auth detail.

Missing from the mapping:
- How does the dashboard obtain the token? (entered on first load? env var? config file?)
- Where is the token stored client-side? (localStorage? sessionStorage? cookie?)
- Is there a login page/component?
- WebSocket auth: query param on upgrade (mentioned in middleware) but no dashboard-side file

**Resolution required**: Add to `dashboard/src/lib/`:

```
│   │   ├── auth.ts                    # AuthManager — token entry on first load,
│   │                                   #   stored in sessionStorage (not localStorage
│   │                                   #   for security). Passed as Authorization header
│   │                                   #   on REST, query param on WebSocket upgrade.
│   │                                   #   No persistent login — token re-entered per session.
```

Add `+page.svelte` for a token entry gate in `dashboard/src/routes/`.

---

### FINDING 20 — HIGH: No CLI binary mapped

The architecture mentions CLI as a channel (`ghost-channels/adapters/cli.rs`) but there's
no actual CLI binary entry point. Users need a `ghost` command to interact with the platform.

`ghost-gateway/src/main.rs` is the server process. `ghost-channels/adapters/cli.rs` is
a channel adapter (stdin/stdout handler). But who invokes it? How does a user type
`ghost chat "hello"` or `ghost status` or `ghost backup`?

**Resolution required**: Either the gateway binary serves double duty (subcommands like
`ghost serve`, `ghost chat`, `ghost status`, `ghost backup`) or there's a separate CLI
crate. Suggested approach — add subcommands to gateway:

```
crates/ghost-gateway/src/
│   ├── main.rs                         # MODIFY — add clap subcommands:
│   │                                   #   ghost serve    — start gateway (existing behavior)
│   │                                   #   ghost chat     — interactive CLI session
│   │                                   #   ghost status   — show agent/session/convergence status
│   │                                   #   ghost backup   — trigger manual backup
│   │                                   #   ghost export   — run export analysis
│   │                                   #   ghost migrate  — run OpenClaw migration
│   ├── cli/
│   │   ├── mod.rs                      # CLI subcommand module root
│   │   ├── chat.rs                    # Interactive chat — connects to CLIAdapter
│   │   ├── status.rs                  # Status display — queries gateway API
│   │   └── commands.rs                # Dispatch for backup, export, migrate subcommands
```

---

### FINDING 21 — LOW: `cortex-convergence` existing vs. new content is ambiguous

The mapping says "The crate skeleton and Cargo.toml exist. The types.rs and
windows/sliding_window.rs exist as stubs." But doesn't specify what's IN those stubs
vs. what needs to be added.

**Resolution required**: Add a brief note listing what the stubs currently contain
(e.g., "types.rs currently defines ConvergenceState and WindowLevel enums only.
sliding_window.rs has struct signature but no implementation.") so implementers know
the exact delta.

---

### FINDING 22 — MEDIUM: No rate limiting on convergence monitor ITP event ingestion

`convergence-monitor/transport/http_api.rs` accepts `POST /events` for ITP event
ingestion. The gateway has rate limiting in its middleware. The monitor has none.

A compromised browser extension or proxy could flood the monitor with fake events,
poisoning convergence scores. The monitor is the safety floor — it needs its own
input validation.

**Resolution required**: Add to `convergence-monitor/transport/http_api.rs` description:
"Rate limiting: token bucket per-source (default 100 events/min per connection).
Event validation: schema check, timestamp sanity (reject events >5min in future),
source authentication (shared secret or unix socket peer credentials)."

---

### FINDING 23 — MEDIUM: Backup encryption key management is unmapped

`ghost-backup/exporter.rs` says "age encryption (passphrase-based)." But:
- Where is the passphrase stored for automatic scheduled backups?
- Is it derived from GHOST_TOKEN? Entered interactively? Stored in a keyring?
- The scheduler can't prompt for a passphrase.

**Resolution required**: Add to `ghost-backup/` description or a new file:

```
│   ├── encryption.rs                   # BackupEncryption — passphrase management.
│                                       #   Interactive mode: prompt user.
│                                       #   Scheduled mode: derive from GHOST_BACKUP_KEY env var
│                                       #   (NOT GHOST_TOKEN — separate secret).
│                                       #   If env var unset, scheduled backups are unencrypted
│                                       #   with a warning logged.
```

---

### AUDIT SUMMARY

| Severity | Count | Key Items |
|----------|-------|-----------|
| CRITICAL | 3 | #1 ghost-signing phantom, #3 kill switch unmapped, #4 inter-agent comms unmapped, #12 ghost-signing not in workspace |
| HIGH | 5 | #5 pipeline engine missing, #7 cortex-temporal mismatch, #10 deployment infra, #11 adversarial tests, #18 cortex-crdt signing exists, #20 no CLI binary |
| MEDIUM | 9 | #2 drift-bridge undocumented, #6 classifier missing, #8 cortex-core omissions, #13 Claude parser, #14 Baileys path, #17 monitor crash recovery, #19 dashboard auth, #22 monitor rate limiting, #23 backup encryption |
| LOW | 4 | #9 state markers, #15 toml/yml inconsistency, #16 layer count mismatch, #21 convergence stubs |

**Net new files required by audit**: ~25 files across 6 new directories.
**Net new crates required**: 1 (`ghost-signing`), 1 optional (`ghost-pipelines`).
**Existing sections requiring rewrite**: cortex-temporal (Finding 7), cortex-crdt (Finding 18).
