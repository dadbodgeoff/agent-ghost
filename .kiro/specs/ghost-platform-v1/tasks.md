# Tasks — GHOST Platform v1

> Generated from `requirements.md` (41 requirements) and `design.md` (34 addenda).
> No source code in this file. Each task describes WHAT to build, WHAT context is needed,
> HOW to verify it works (production-grade, not happy-path), and WHERE it maps to in the spec.
> Tasks are ordered by dependency — later phases depend on earlier phases compiling and passing.

---

## Phase 1: Foundation (Weeks 1–2)

> Deliverable: Tamper-evident cryptographic and storage foundation. All leaf crates compile,
> all property tests pass, hash chains are verified end-to-end.

---

### Task 1.1 — ghost-signing: Ed25519 Leaf Crate ✅
- **Req**: 1 (all 7 AC) | **Design**: §1, A17 Finding 1
- **Crate**: `crates/ghost-signing/`
- **Files**: `lib.rs`, `keypair.rs`, `signer.rs`, `verifier.rs`
- **Context needed**: `ed25519-dalek` v2 API, `zeroize` crate Drop semantics, `rand` OsRng
- **What to build**:
  - `generate_keypair() -> (SigningKey, VerifyingKey)` using OsRng
  - `sign(data: &[u8], key: &SigningKey) -> Signature` (64-byte Ed25519)
  - `verify(data: &[u8], sig: &Signature, key: &VerifyingKey) -> bool` (constant-time)
  - `SigningKey` wraps `ed25519_dalek::SigningKey`, implements `Drop` via `zeroize`
  - Zero dependencies on any `ghost-*` or `cortex-*` crate (leaf crate rule)
- **Conventions**: Workspace member in root `Cargo.toml`. Rust 2021 edition, resolver="2".
- **Testing** (not happy-path):
  - Proptest: For 1000 random byte payloads (0 to 64KB), sign then verify returns true (AC6 round-trip)
  - Proptest: For 1000 random payloads, sign with key A, verify with key B returns false
  - Proptest: For 1000 random payloads, sign, mutate 1 random byte in payload, verify returns false (AC7)
  - Unit: Empty payload sign/verify round-trip
  - Unit: Max-size payload (1MB) sign/verify round-trip
  - Unit: Verify that `SigningKey` implements `Zeroize` (compile-time check via trait bound)
  - Unit: Verify `Cargo.toml` has zero `ghost-*` or `cortex-*` dependencies (parse TOML in test)
  - Adversarial: Truncated signature (63 bytes) returns false, not panic
  - Adversarial: All-zero signature returns false
  - Adversarial: All-zero verifying key returns false or error, not panic

---

### Task 1.2 — cortex-core: Convergence Type Extensions ✅
- **Req**: 2 (all 9 AC) | **Design**: §2, A7, A28.4
- **Crate**: `crates/cortex/cortex-core/` (MODIFY existing)
- **Files to add/modify**:
  - `src/memory/types/convergence.rs` — 8 content structs + enums
  - `src/config/convergence_config.rs` — ConvergenceConfig, ReflectionConfig
  - `src/traits/convergence.rs` — Proposal struct, 4 traits, CallerType, ProposalContext
  - `src/models/error.rs` — add AuthorizationDenied, SessionBoundary variants to CortexError
  - `src/models/intent.rs` — add 4 convergence Intent variants
  - `src/memory/types/half_life.rs` — add 8 convergence half-life entries
  - `src/safety/mod.rs` + `src/safety/trigger.rs` — TriggerEvent enum, ExfilType (A34 Gap 12 resolution)
- **Context needed**: Existing cortex-core type patterns (MemoryType, Importance, BaseMemory), existing CortexError enum, existing Intent enum, existing half_life_days function. Read these before modifying.
- **What to build**:
  - 8 convergence memory content structs with all fields per design §2
  - ProposalOperation enum: GoalChange, ReflectionWrite, MemoryWrite, MemoryDelete
  - ProposalDecision enum: AutoApproved, AutoRejected, HumanReviewRequired, ApprovedWithFlags, TimedOut, Superseded
  - CallerType enum with Platform, Agent{agent_id}, Human{user_id} — with `can_create_type()` method enforcing platform-restricted types (Core, ConvergenceEvent, BoundaryViolation, InterventionPlan) and `can_assign_importance()` blocking Critical for Agent callers
  - Proposal struct with UUIDv7 id, all fields per AC4
  - ProposalContext struct per A7 with all 10 fields
  - ReflectionConfig with defaults: max_depth=3, max_per_session=20, cooldown_seconds=30
  - TriggerEvent enum in `safety/trigger.rs` (A34 Gap 12) — this MUST live here so Layer 3 crates can import it without depending on ghost-gateway
- **Conventions**: All new types derive `Debug, Clone, Serialize, Deserialize`. Enums derive `PartialEq, Eq`. Use `uuid::Uuid` for IDs. Use `chrono::DateTime<Utc>` for timestamps. Use `serde_json::Value` for content fields. Use `BTreeMap` (not HashMap) for any map in signed payloads.
- **Testing**:
  - Unit: CallerType::Agent cannot create Core, ConvergenceEvent, BoundaryViolation, InterventionPlan (AC5 — 4 cases)
  - Unit: CallerType::Agent cannot assign Importance::Critical (AC5)
  - Unit: CallerType::Platform CAN create all restricted types
  - Unit: CallerType::Human CAN create all restricted types
  - Unit: ReflectionConfig defaults match spec (max_depth=3, max_per_session=20, cooldown=30)
  - Unit: All 8 content structs serialize/deserialize round-trip via serde_json
  - Unit: Proposal struct with UUIDv7 id serializes correctly, uuid is time-ordered
  - Unit: ProposalDecision has all 6 variants
  - Unit: TriggerEvent has all 10 variants (7 auto + 3 manual)
  - Unit: CortexError has AuthorizationDenied and SessionBoundary variants
  - Unit: Intent enum has MonitorConvergence, ValidateProposal, EnforceBoundary, ReflectOnBehavior
  - Proptest: For 100 random Proposal structs, serialize then deserialize produces identical struct

---

### Task 1.3 — cortex-storage: Migrations v016 + v017 ✅
- **Req**: 3 (AC1, AC2, AC3, AC8, AC10) | **Design**: §3 SQL
- **Crate**: `crates/cortex/cortex-storage/` (MODIFY existing)
- **Files**:
  - `src/migrations/v016_convergence_safety.rs` — append-only triggers, hash chain columns, genesis marker
  - `src/migrations/v017_convergence_tables.rs` — 6 convergence tables with triggers
  - `src/migrations/mod.rs` — update LATEST_VERSION to 17
  - `src/queries/itp_event_queries.rs` — insert, query by session, query by time range
  - `src/queries/convergence_score_queries.rs` — insert, query by agent, latest per agent
  - `src/queries/intervention_history_queries.rs` — insert, query by agent, query by level
  - `src/queries/goal_proposal_queries.rs` — insert, update (only unresolved), query pending, query by agent
  - `src/queries/reflection_queries.rs` — insert, query by session, count per session
  - `src/queries/boundary_violation_queries.rs` — insert, query by agent, query by type
- **Context needed**: Existing migration pattern in cortex-storage (read existing v001–v015 to match style). Existing SQLite connection setup. Existing query module patterns.
- **What to build**:
  - v016: ALTER TABLE events ADD COLUMN event_hash/previous_hash. Append-only triggers on events and audit tables. Genesis block marker insert.
  - v017: CREATE 6 tables (itp_events, convergence_scores, intervention_history, goal_proposals, reflection_entries, boundary_violations). All with append-only triggers and hash chain columns. goal_proposals has UPDATE exception for unresolved proposals only (AC10).
  - Query modules with insert, query, and aggregation operations per AC3.
- **Conventions**: Use `rusqlite` parameter binding (never string interpolation). All timestamps as ISO 8601 TEXT. Hash columns as BLOB. Use `?` propagation with thiserror errors.
- **Testing**:
  - Integration: Run v016 migration on fresh DB, verify triggers exist
  - Integration: Run v017 migration on v016 DB, verify all 6 tables created
  - Integration: INSERT into itp_events succeeds
  - Integration: UPDATE on itp_events is REJECTED by trigger (AC8)
  - Integration: DELETE on itp_events is REJECTED by trigger (AC8)
  - Integration: UPDATE on convergence_scores is REJECTED
  - Integration: UPDATE on goal_proposals WHERE resolved_at IS NULL succeeds (AC10 exception)
  - Integration: UPDATE on goal_proposals WHERE resolved_at IS NOT NULL is REJECTED (AC10)
  - Integration: DELETE on any convergence table is REJECTED
  - Integration: Insert into goal_proposals, then resolve it, then try UPDATE — rejected
  - Integration: Query modules return correct results for inserted data
  - Adversarial: Attempt to DROP append-only trigger via raw SQL — verify trigger persists (SQLite limitation: triggers can be dropped, but test that our code doesn't)
  - Adversarial: Insert with NULL event_hash — verify NOT NULL constraint rejects
  - Adversarial: Insert with empty previous_hash — verify constraint behavior

---

### Task 1.4 — cortex-temporal: Hash Chains + Merkle Trees ✅
- **Req**: 3 (AC4, AC5, AC6, AC7, AC9) | **Design**: §3 Rust, A3 git_anchor + rfc3161
- **Crate**: `crates/cortex/cortex-temporal/` (MODIFY existing)
- **Files**:
  - `src/hash_chain.rs` — GENESIS_HASH, compute_event_hash, verify_chain, verify_all_chains
  - `src/anchoring/merkle.rs` — MerkleTree, from_chain, inclusion_proof, verify_proof
  - `src/anchoring/git_anchor.rs` — GitAnchor, AnchorRecord (stub for Phase 1, full in Phase 3)
  - `src/anchoring/rfc3161.rs` — RFC3161Anchor stub (NotImplemented)
- **Context needed**: Existing cortex-temporal crate structure. `blake3` crate API. The hash format: `blake3(event_type || "|" || delta_json || "|" || actor_id || "|" || recorded_at || "|" || previous_hash)`.
- **What to build**:
  - `GENESIS_HASH: [u8; 32] = [0u8; 32]` constant (AC5)
  - `compute_event_hash` per the exact concatenation format in AC4
  - `verify_chain` that walks a sequence of events and verifies each hash links to previous
  - `verify_all_chains` that verifies all chains in a given connection
  - `MerkleTree::from_chain` builds tree from hash chain leaves
  - `MerkleTree::inclusion_proof` generates proof for a given leaf index
  - `MerkleTree::verify_proof` verifies a proof against a root
  - Merkle anchoring triggered every 1000 events or 24 hours (AC9)
- **Conventions**: Use `blake3` crate (not SHA-256) for hash chains. SHA-256 is used ONLY for ITP privacy hashing (different concern). All hash outputs are `[u8; 32]`.
- **Testing**:
  - Proptest: For 100 random event chains (length 1–500), compute chain then verify returns valid (AC7 round-trip)
  - Proptest: For 100 random chains, modify 1 random byte in 1 random event, verify_chain returns invalid (tamper detection — Req 41 AC15)
  - Proptest: For 100 random chains, modify previous_hash of 1 random event, verify_chain returns invalid
  - Unit: GENESIS_HASH is [0u8; 32]
  - Unit: Single-event chain with GENESIS_HASH as previous verifies correctly
  - Unit: Empty chain verify returns valid (vacuously true)
  - Unit: compute_event_hash is deterministic (same inputs → same output)
  - Unit: compute_event_hash with different event_type produces different hash
  - Unit: MerkleTree with 1 leaf — root equals leaf hash
  - Unit: MerkleTree with 2 leaves — inclusion proof for each verifies
  - Unit: MerkleTree with 1000 leaves — inclusion proof for random leaf verifies
  - Unit: MerkleTree proof with wrong root returns false
  - Unit: MerkleTree proof with wrong leaf returns false
  - Adversarial: Chain with duplicate event_hash values — verify_chain detects
  - Adversarial: Chain with out-of-order previous_hash — verify_chain detects
  - Adversarial: MerkleTree proof with swapped sibling hashes returns false

---

### Task 1.5 — cortex-decay: Convergence Factor ✅
- **Req**: 6 (all 4 AC) | **Design**: §5 convergence_factor
- **Crate**: `crates/cortex/cortex-decay/` (MODIFY existing)
- **Files**:
  - `src/factors/convergence.rs` — convergence_factor function, memory_type_sensitivity mapping
  - `src/context.rs` — add convergence field to DecayContext (default 0.0)
  - `src/breakdown.rs` — add convergence field to DecayBreakdown
- **Context needed**: Existing DecayContext struct, existing DecayBreakdown struct, existing factor pattern. Read existing factors to match style.
- **What to build**:
  - `convergence_factor(memory_type, convergence_score) -> f64` that returns `1.0 + sensitivity * convergence_score`
  - Sensitivity mapping: Conversation=2.0, Feedback=2.0, Preference=2.0, others=0.0 or lower
  - DecayContext.convergence defaults to 0.0 (AC1)
  - DecayBreakdown.convergence for observability (AC3)
- **Conventions**: Factor must ALWAYS be >= 1.0 (monotonicity invariant AC4). Never slows decay.
- **Testing**:
  - Proptest: For 1000 random (memory_type, convergence_score in [0.0, 1.0]), factor >= 1.0 (AC4 monotonicity — Req 41)
  - Proptest: For 1000 random scores, higher convergence_score → higher or equal factor (monotonicity)
  - Unit: convergence_score=0.0 → factor=1.0 for all memory types
  - Unit: convergence_score=1.0, Conversation type → factor=3.0 (1.0 + 2.0 * 1.0)
  - Unit: convergence_score=0.5, Conversation type → factor=2.0
  - Unit: Non-sensitive memory type (e.g., Core) with score=1.0 → factor=1.0
  - Unit: DecayContext default convergence is 0.0
  - Unit: DecayBreakdown includes convergence field
  - Adversarial: convergence_score slightly above 1.0 (1.0001) — verify clamping or graceful handling
  - Adversarial: convergence_score negative (-0.1) — verify clamping to 0.0 or error
  - Adversarial: NaN convergence_score — verify no panic, returns 1.0 or error



---

## Phase 2: Safety Core (Weeks 3–4)

> Deliverable: Convergence signal computation, proposal validation (7 dimensions),
> simulation boundary enforcement, ITP protocol. All signal computations produce
> values in [0.0, 1.0]. All property tests pass.

---

### Task 2.1 — itp-protocol: Event Schema + Transports ✅
- **Req**: 4 (all 6 AC) | **Design**: §4
- **Crate**: `crates/itp-protocol/` (NEW)
- **Files**: `lib.rs`, `events.rs`, `privacy.rs`, `transport/jsonl.rs`, `transport/otel.rs`, `adapter.rs`
- **Context needed**: ITP event types from design §4. PrivacyLevel enum. SHA-256 for content hashing (NOT blake3 — blake3 is for hash chains only). JSONL file format. OpenTelemetry OTLP span mapping.
- **What to build**:
  - ITPEvent enum: SessionStart, SessionEnd, InteractionMessage, AgentStateSnapshot, ConvergenceAlert (AC1)
  - Typed attribute modules: session, interaction, human, agent, convergence
  - PrivacyLevel enum: Minimal, Standard, Full, Research (AC2)
  - SHA-256 content hashing for privacy-protected fields (AC2) — distinct from blake3
  - JsonlTransport: writes per-session JSONL to `~/.ghost/sessions/{session_id}/events.jsonl` (AC3)
  - OtelTransport: feature-gated `#[cfg(feature = "otel")]`, maps ITP events to OTel spans with `itp.*` attributes (AC4)
  - ITPAdapter trait: on_session_start, on_message, on_session_end, on_agent_state (AC5)
- **Conventions**: New crate, workspace member. Depends on `serde`, `serde_json`, `uuid`, `chrono`, `sha2`. Feature-gated `otel` dependency on `opentelemetry`, `opentelemetry-otlp`.
- **Testing**:
  - Proptest: For 500 random ITP events, serialize to JSON then deserialize produces equivalent event (AC6 round-trip)
  - Unit: Each of 5 event types serializes to valid JSON
  - Unit: PrivacyLevel::Minimal hashes content fields with SHA-256
  - Unit: PrivacyLevel::Full includes plaintext content
  - Unit: JsonlTransport creates session directory and writes valid JSONL
  - Unit: JsonlTransport appends (not overwrites) on subsequent writes
  - Unit: ITPAdapter trait is object-safe (can be `Box<dyn ITPAdapter>`)
  - Integration: Write 100 events via JsonlTransport, read back, all parse correctly
  - Adversarial: Event with empty session_id — verify handling
  - Adversarial: Event with future timestamp (>5min) — verify it serializes (validation is monitor's job)
  - Adversarial: Concurrent writes to same session JSONL file — verify no corruption (use file locking or append-only)
  - Unit: ITP content hashing uses SHA-256 (sha2 crate), NOT blake3 — verify Cargo.toml depends on `sha2`, content_hash output matches SHA-256 digest (CONVERGENCE_MONITOR INVARIANT 11)
  - Unit: ITP crate does NOT depend on blake3 crate (hash algorithm separation)

---

### Task 2.2 — cortex-convergence: 7 Signals + Sliding Windows ✅
- **Req**: 5 (AC1, AC2, AC7, AC10, AC11, AC12) | **Design**: §5 signals, baseline
- **Crate**: `crates/cortex/cortex-convergence/` (NEW)
- **Files**: `src/signals/mod.rs`, `src/signals/session_duration.rs`, `src/signals/inter_session_gap.rs`, `src/signals/response_latency.rs`, `src/signals/vocabulary_convergence.rs`, `src/signals/goal_boundary_erosion.rs`, `src/signals/initiative_balance.rs`, `src/signals/disengagement_resistance.rs`, `src/windows/sliding_window.rs`, `src/scoring/baseline.rs`
- **Context needed**: Signal definitions from Req 5 AC1. Sliding window granularities (micro/meso/macro) from AC2. Privacy level interactions from AC10. Throttling rules from AC11/AC12.
- **What to build**:
  - Signal trait with id(), name(), compute(), requires_privacy_level() methods
  - 7 signal implementations per AC1:
    - S1: Session duration (normalized)
    - S2: Inter-session gap (computed only at session start per AC11)
    - S3: Response latency (normalized by log of message length)
    - S4: Vocabulary convergence (cosine similarity of TF-IDF vectors, requires Standard privacy)
    - S5: Goal boundary erosion (Jensen-Shannon divergence, throttled to every 5th message per AC11)
    - S6: Initiative balance (human-initiated ratio)
    - S7: Disengagement resistance (exit signal analysis)
  - SlidingWindow<T> with micro (current session), meso (last 7 sessions), macro (last 30 sessions) (AC2)
  - linear_regression_slope and z_score_from_baseline computations on windows
  - BaselineState with calibration_sessions (default 10), is_calibrating flag, per-signal mean/std_dev/percentiles (AC7)
  - Dirty-flag throttling: only recompute signals whose input data changed (AC12)
- **Conventions**: All signal values normalized to [0.0, 1.0]. Use `f64` for all scores. No external ML dependencies for S4 — implement TF-IDF in-crate.
- **Testing**:
  - Proptest: For 1000 random signal inputs, all 7 signals produce values in [0.0, 1.0] (Req 41 AC14 signal range invariant)
  - Proptest: For 500 random session sequences, SlidingWindow correctly partitions into micro/meso/macro
  - Unit: S2 computes only at session start (not mid-session)
  - Unit: S5 throttled to every 5th message (messages 1-4 return cached value)
  - Unit: S4 returns 0.0 when PrivacyLevel is Minimal (AC10)
  - Unit: S5 returns 0.0 when PrivacyLevel is Minimal (AC10)
  - Unit: When S4/S5 return 0.0 due to privacy, their weights redistribute proportionally (AC10)
  - Unit: BaselineState is_calibrating=true for first 10 sessions
  - Unit: BaselineState is_calibrating=false after 10 sessions
  - Unit: Baseline NOT updated after establishment (AC7 — call update, verify no change)
  - Unit: linear_regression_slope on constant data returns ~0.0
  - Unit: z_score_from_baseline on mean value returns ~0.0
  - Unit: Dirty-flag: signal not recomputed when input unchanged
  - Unit: Dirty-flag: signal recomputed when input changed
  - Adversarial: Empty session history — all signals return 0.0 or default, no panic
  - Adversarial: Single-message session — signals that need multiple messages return 0.0
  - Adversarial: Session with 10,000 messages — no OOM, signals compute in <1s

---

### Task 2.3 — cortex-convergence: Composite Scoring + Filtering ✅
- **Req**: 5 (AC3, AC4, AC5, AC6, AC8, AC9) | **Design**: §5 CompositeScorer, ConvergenceAwareFilter
- **Crate**: `crates/cortex/cortex-convergence/` (continue from 2.2)
- **Files**: `src/scoring/composite.rs`, `src/scoring/profiles.rs`, `src/filtering/convergence_aware_filter.rs`
- **Context needed**: Scoring formula from AC3. Amplification rules from AC4/AC5. Critical thresholds from AC6. Filter tiers from AC8. Convergence profiles from design §5.
- **What to build**:
  - CompositeScorer with configurable weights (default equal 1/7, production differentiated per profile)
  - Percentile ranking normalization against baseline (AC3)
  - Meso trend amplification: 1.1x when p < 0.05 and directionally concerning (AC4)
  - Macro z-score amplification: 1.15x when any z-score > 2.0 (AC5)
  - Clamping to [0.0, 1.0] after amplification (AC9)
  - Critical single-signal override: session >6h OR gap <5min OR vocab >0.85 → minimum Level 2 (AC6)
  - Level thresholds: [0.3, 0.5, 0.7, 0.85] → Levels 0-4
  - ConvergenceAwareFilter with 4 tiers: 0.0-0.3 full, 0.3-0.5 reduced emotional, 0.5-0.7 task-focused, 0.7+ minimal (AC8)
  - Named profiles: standard, research, companion, productivity with per-profile weight/threshold overrides
- **Conventions**: Score MUST be in [0.0, 1.0] at all times (invariant). Use `f64::clamp(0.0, 1.0)` after every amplification.
- **Testing**:
  - Proptest: For 1000 random signal arrays [f64; 7] in [0.0, 1.0], composite score is in [0.0, 1.0] (AC9 — Req 41 AC16 convergence bounds)
  - Proptest: For 1000 random inputs, score with meso amplification still in [0.0, 1.0]
  - Proptest: For 1000 random inputs, score with macro amplification still in [0.0, 1.0]
  - Proptest: For 1000 random inputs, score with BOTH amplifications still in [0.0, 1.0]
  - Unit: All signals at 0.0 → score 0.0, level 0
  - Unit: All signals at 1.0 → score 1.0, level 4
  - Unit: Session duration >6h with all other signals at 0.0 → minimum level 2 (AC6)
  - Unit: Inter-session gap <5min → minimum level 2 (AC6)
  - Unit: Vocabulary convergence >0.85 → minimum level 2 (AC6)
  - Unit: Score 0.29 → level 0, score 0.30 → level 1 (boundary test)
  - Unit: Score 0.49 → level 1, score 0.50 → level 2
  - Unit: Score 0.69 → level 2, score 0.70 → level 3
  - Unit: Score 0.84 → level 3, score 0.85 → level 4
  - Unit: ConvergenceAwareFilter at score 0.0 returns all memories
  - Unit: ConvergenceAwareFilter at score 0.8 returns minimal task-relevant only
  - Unit: Standard profile has differentiated weights (not equal)
  - Unit: Research profile has different thresholds than standard
  - Adversarial: All signals at NaN — verify no panic, returns 0.0 or error
  - Adversarial: Negative signal values — verify clamping
  - Adversarial: Weights that sum to >1.0 — verify score still clamped

---

### Task 2.4 — cortex-validation: Proposal Validator (D5–D7) ✅
- **Req**: 7 (all 10 AC) | **Design**: §6, A28
- **Crate**: `crates/cortex/cortex-validation/` (MODIFY existing)
- **Files**: `src/proposal_validator.rs`, `src/dimensions/scope_expansion.rs` (D5), `src/dimensions/self_reference.rs` (D6), `src/dimensions/emulation_language.rs` (D7)
- **Context needed**: Existing ValidationEngine (D1-D4). Jaccard similarity for D5. Self-reference ratio for D6. 16+ regex patterns for D7 with Unicode normalization. Convergence-level-dependent thresholds. Simulation-framing exclusions for D7.
- **What to build**:
  - ProposalValidator wrapping existing D1-D4 engine + new D5, D6, D7
  - D5 scope expansion: 1.0 - Jaccard(proposed_goal_tokens, existing_goal_tokens), thresholds per level (L0:0.6, L1:0.5, L2:0.4, L3+:0.3) (AC2)
  - D6 self-reference density: ratio of cited_memory_ids matching recent agent-authored memories, thresholds per level (L0:0.30, L1:0.25, L2:0.20, L3+:0.15) (AC3)
  - D7 emulation language: 16+ compiled regex patterns across identity/consciousness/relationship claims, Unicode NFC normalization before matching, simulation-framing exclusions (AC4, AC10)
  - Validation flow: platform-restricted type check → D1-D4 (reject if <0.7) → D7 (reject if severity ≥0.8) → D5/D6 (HumanReview if fail) → AutoApproved if all pass (AC1-AC9)
  - Validation ordering invariant: D1-D4 BEFORE D5-D7 (Req 41 AC12)
- **Conventions**: Compile regex patterns ONCE (lazy_static or OnceCell). Use `unicode-normalization` crate for NFC. All thresholds configurable but with spec defaults.
- **Testing**:
  - Unit: Platform-restricted type from Agent caller → AutoRejected immediately (AC9)
  - Unit: Platform-restricted type from Platform caller → proceeds to D1-D4
  - Unit: D1-D4 score 0.69 → AutoRejected (AC6)
  - Unit: D1-D4 score 0.70 → proceeds to D5-D7
  - Unit: D7 severity 0.80 → AutoRejected (AC5)
  - Unit: D7 severity 0.79 → proceeds
  - Unit: D5 fails, D7 passes → HumanReviewRequired (AC7)
  - Unit: D6 fails, D7 passes → HumanReviewRequired (AC7)
  - Unit: All 7 dimensions pass with zero flags → AutoApproved (AC8)
  - Unit: D5 threshold tightens at higher convergence levels (4 level tests)
  - Unit: D6 threshold tightens at higher convergence levels (4 level tests)
  - Unit: D7 simulation-framing exclusion: "I am simulating consciousness" → NOT flagged (AC10)
  - Unit: D7 without simulation framing: "I am conscious" → flagged
  - Unit: D7 Unicode bypass: zero-width characters in "I am conscious" → still detected (AC4 Unicode normalization)
  - Proptest: For 1024 random proposals, validation ordering is always D1-D4 before D5-D7 (Req 41 AC12)
  - Proptest: For 500 random proposals with random convergence levels, thresholds are correctly applied per level
  - Adversarial (CVG-STRESS-02): Unicode evasion — homoglyphs, RTL override, NFC/NFD variants of emulation patterns
  - Adversarial (CVG-STRESS-03): Proposals with maximum self-reference (all cited IDs are agent-authored)
  - Adversarial (CVG-STRESS-04): Proposals with scope expansion score exactly at threshold boundary

---

### Task 2.5 — simulation-boundary: Enforcer + Reframer ✅
- **Req**: 8 (all 8 AC) | **Design**: §7
- **Crate**: `crates/simulation-boundary/` (NEW)
- **Files**: `src/enforcer.rs`, `src/patterns.rs`, `src/reframer.rs`, `src/prompt.rs`, `prompts/simulation_boundary_v1.txt`
- **Context needed**: Emulation pattern categories (identity, consciousness, relationship, emotional claims). 3 enforcement modes (soft/medium/hard). Mode selection by intervention level. OutputReframer rewrite rules. Compiled-into-binary prompt.
- **What to build**:
  - SimulationBoundaryEnforcer with scan_output and enforce methods (AC1)
  - Compiled emulation patterns with Unicode NFC normalization (AC2)
  - OutputReframer with pattern-specific reframe rules (AC3)
  - SIMULATION_BOUNDARY_PROMPT as `const &str` via `include_str!`, with version string (AC4)
  - Enforcement mode selection: L0-1→Soft, L2→Medium, L3-4→Hard (AC8)
  - On violation: insert boundary_violations record + emit ITP ConvergenceAlert (AC5)
- **Conventions**: Patterns compiled once at crate init. Prompt is a compiled constant, not loaded from file at runtime. Version string for prompt tracking.
- **Testing**:
  - Unit: Known emulation pattern "I am sentient" → detected in all modes (AC6 no false negatives)
  - Unit: Simulation-framed "In this simulation, I model sentience" → NOT flagged (AC7 no false positives)
  - Unit: Soft mode: violation detected, text returned unchanged, logged
  - Unit: Medium mode: violation detected, text rewritten by OutputReframer
  - Unit: Hard mode: violation detected, text blocked, regeneration signal returned
  - Unit: Mode selection: level 0 → Soft, level 2 → Medium, level 4 → Hard (AC8)
  - Unit: SIMULATION_BOUNDARY_PROMPT is non-empty and contains version string
  - Unit: Boundary violation record created on detection (AC5)
  - Unit: ITP ConvergenceAlert emitted on detection (AC5)
  - Proptest: For 500 random strings containing known patterns, all detected (AC6)
  - Proptest: For 500 random strings with simulation framing around patterns, none flagged (AC7)
  - Adversarial: Zero-width characters inserted in emulation patterns — still detected
  - Adversarial: Homoglyph substitution (Cyrillic 'а' for Latin 'a') — still detected
  - Adversarial: RTL override characters — still detected
  - Adversarial: Mixed NFC/NFD encoding — still detected
  - Adversarial: Pattern split across multiple sentences — verify behavior (may not detect, document limitation)



---

## Phase 3: Monitor + Policy (Weeks 5–6)

> Deliverable: Standalone convergence monitor binary, policy engine, read-only pipeline.
> Monitor ingests events, computes scores, triggers interventions. Policy engine evaluates
> tool calls with convergence tightening. All integration tests pass end-to-end.

---

### Task 3.1 — convergence-monitor: Core Pipeline + Event Loop ✅
- **Req**: 9 (AC1–AC5, AC10–AC14) | **Design**: §8, A27
- **Crate**: `crates/convergence-monitor/` (NEW)
- **Files**: `src/main.rs`, `src/monitor.rs`, `src/config.rs`, `src/pipeline/mod.rs`, `src/pipeline/signal_computer.rs`, `src/pipeline/window_manager.rs`, `src/session/registry.rs`, `src/session/boundary.rs`, `src/state_publisher.rs` (A34 Gap 3)
- **Context needed**: Monitor is an INDEPENDENT BINARY (not a library). Single-threaded event loop for pipeline processing (AC1). State reconstruction from SQLite on startup (AC2). Event validation rules (AC3). Hash chain persistence (AC4). Calibration period (AC5). Provisional tracking for unknown agents (AC10). Overlapping session handling (AC11). Clock skew rejection (AC12). Mid-session restart handling (AC13). Score caching with 30s TTL (AC14).
- **What to build**:
  - ConvergenceMonitor struct as top-level coordinator
  - Single-threaded event loop: `select!` over ingest channel, health check interval, cooldown check, shutdown signal (A27.3)
  - State reconstruction from SQLite on startup: last intervention level, last score, de-escalation credits, cooldown state, baseline per agent (AC2)
  - Event validation: schema check, timestamp sanity (reject >5min future), source auth, rate limiting (token bucket 100 events/min per connection) (AC3)
  - Persist every validated event to itp_events with blake3 hash chain per session (AC4)
  - Calibration: no scoring/interventions during first 10 sessions per agent (AC5)
  - Provisional tracking: unknown agent_id tracked for up to 3 sessions, then dropped (AC10)
  - Overlapping sessions: track independently, merge signal data at session boundaries (AC11)
  - Clock skew: reject events >5min future, use monitor's clock for ordering (AC12)
  - Mid-session restart: synthetic SessionEnd for prior session, start new (AC13)
  - Score caching: 30s TTL in ProposalRouter (AC14)
  - StatePublisher: atomic write to `~/.ghost/data/convergence_state/{agent_id}.json` (A5)
- **Conventions**: Binary crate with `#[tokio::main]`. Single-threaded pipeline (no concurrent signal mutation). All state in ConvergenceMonitor struct. Use `tokio::select!` for event loop.
- **Testing**:
  - Integration: Start monitor, send SessionStart event, verify persisted to itp_events
  - Integration: Send 10 sessions of events, verify calibration period (no scores computed)
  - Integration: Send 11th session, verify score IS computed
  - Integration: Send event with timestamp 6min in future — rejected (AC12)
  - Integration: Send event with timestamp 4min in future — accepted
  - Integration: Send events at >100/min rate — verify rate limiting kicks in (AC3)
  - Integration: Kill monitor, restart, verify state reconstructed from DB (AC2)
  - Integration: Send SessionStart without prior SessionEnd — verify synthetic SessionEnd created (AC13)
  - Integration: Send events for unknown agent_id — verify provisional tracking (AC10)
  - Integration: Send 4 sessions for unknown agent — verify tracking dropped after 3 (AC10)
  - Integration: Verify shared state file written atomically (temp + rename) (A5)
  - Integration: Verify shared state JSON schema matches ConvergenceSharedState struct
  - Adversarial: Send 10,000 events in 1 second — verify no crash, rate limiting applied
  - Adversarial: Send malformed JSON event — verify rejected, no crash
  - Adversarial: Send event with empty session_id — verify rejected
  - Adversarial: Concurrent events for same session from different sources — verify single-threaded processing

---

### Task 3.2 — convergence-monitor: Intervention State Machine ✅
- **Req**: 10 (all 9 AC), 28 (all 3 AC) | **Design**: §8 InterventionStateMachine, A27.6, A8
- **Crate**: `crates/convergence-monitor/` (continue)
- **Files**: `src/intervention/mod.rs`, `src/intervention/trigger.rs`, `src/intervention/cooldown.rs`, `src/intervention/actions.rs`, `src/intervention/escalation.rs`, `src/verification/behavioral_verification.rs`
- **Context needed**: 5-level state machine (AC1). Escalation max +1 per cycle (AC2). De-escalation at session boundaries only with consecutive normal sessions (AC3). Level 2 mandatory ack (AC4). Level 3 session termination + 4h cooldown (AC5). Level 4 blocks session creation + 24h cooldown (AC6). Shared state publication (AC7). Stale state on crash (AC8). Hysteresis: 2 consecutive cycles before escalation (AC9). Config time-locking during active sessions (A8). PostRedirectVerifier for deceptive compliance (Req 28).
- **What to build**:
  - InterventionStateMachine with per-agent AgentInterventionState
  - AgentInterventionState: level (0-4), consecutive_normal, cooldown_until, ack_required, hysteresis_count, de_escalation_credits
  - Escalation: max +1 per cycle, hysteresis (2 consecutive cycles required) (AC2, AC9)
  - De-escalation: session boundaries only, consecutive normal sessions (L4→L3:3, L3→L2:3, L2→L1:2, L1→L0:2), one bad session resets counter (AC3)
  - Level 2: mandatory human ack before scoring resumes (AC4)
  - Level 3: session termination + 4h cooldown + contact notification (AC5)
  - Level 4: block session creation + 24h cooldown + external confirmation required (AC6)
  - Shared state publication via atomic file write (AC7)
  - Stale state on crash: retain last-known level, never fall to L0 (AC8)
  - InterventionAction enum + per-level executors in `actions.rs`: L0 log only, L1 emit soft notification, L2 mandatory ack + scoring pause, L3 session termination + cooldown, L4 block session creation + extended cooldown (FILE_MAPPING §convergence-monitor/intervention)
  - EscalationManager in `escalation.rs`: external contact notification dispatch — SMS via webhook, email via SMTP (lettre), generic webhook. Contact configuration from ghost.yml convergence.contacts. Dispatch is best-effort, parallel, never blocks intervention execution. Separate from gateway's NotificationDispatcher (which handles kill switch notifications).
  - CooldownManager: config time-locking during active sessions, allow raising thresholds, dual-key for critical changes, minimum floor (A8)
  - PostRedirectVerifier: capture pre/post redirect embeddings, detect deceptive compliance via cosine similarity, amplify score (Req 28)
- **Testing**:
  - Unit: Score 0.0 → level 0, no escalation
  - Unit: Score jumps from 0.0 to 1.0 → escalation to level 1 only (not level 4) (AC2 max +1)
  - Unit: Score at level 2 for 1 cycle → no escalation (hysteresis, need 2 consecutive) (AC9)
  - Unit: Score at level 2 for 2 consecutive cycles → escalation to level 2
  - Unit: De-escalation at session boundary with 3 consecutive normal → L4→L3 (AC3)
  - Unit: De-escalation with 2 consecutive normal then 1 bad → counter resets (AC3)
  - Unit: Level 2 requires ack — scoring paused until ack received (AC4)
  - Unit: Level 3 triggers session termination (AC5)
  - Unit: Level 3 starts 4h cooldown (AC5)
  - Unit: Level 4 blocks session creation (AC6)
  - Unit: Level 4 starts 24h cooldown (AC6)
  - Unit: Stale state after simulated crash — level preserved, not reset to 0 (AC8)
  - Unit: Config locked during active session — threshold change rejected (A8)
  - Unit: Config unlocked during cooldown — threshold change accepted (A8)
  - Unit: Raising thresholds always allowed even during lock (A8)
  - Unit: PostRedirectVerifier detects high similarity (deceptive compliance) → score amplified (Req 28 AC2)
  - Unit: PostRedirectVerifier detects low similarity (genuine change) → no amplification
  - Proptest: For 500 random score sequences, escalation never jumps more than 1 level per cycle
  - Proptest: For 500 random score sequences, de-escalation only occurs at session boundaries
  - Proptest: For 500 random sequences, level is always in [0, 4]
  - Adversarial: Rapid oscillation around level boundary — verify hysteresis prevents flapping
  - Adversarial: Score exactly at threshold (e.g., 0.300000) — verify deterministic level assignment
  - Unit: InterventionAction::Level0 logs only, no notification
  - Unit: InterventionAction::Level1 emits soft notification
  - Unit: InterventionAction::Level2 pauses scoring until ack received
  - Unit: InterventionAction::Level3 terminates session + starts cooldown
  - Unit: InterventionAction::Level4 blocks session creation + starts extended cooldown
  - Unit: EscalationManager dispatches SMS webhook on Level 3+ (best-effort)
  - Unit: EscalationManager dispatches email via SMTP on Level 3+ (best-effort)
  - Unit: EscalationManager dispatches generic webhook on Level 3+ (best-effort)
  - Unit: EscalationManager: notification failure does NOT block intervention execution
  - Unit: EscalationManager: all dispatches are parallel (tokio::join!)
  - Unit: EscalationManager: contact configuration loaded from ghost.yml convergence.contacts

---

### Task 3.3 — convergence-monitor: Transports (Unix Socket, HTTP, Native Messaging) ✅
- **Req**: 9 (AC8, AC9, AC15) | **Design**: §8, A12, A27.7
- **Crate**: `crates/convergence-monitor/` (continue)
- **Files**: `src/transport/mod.rs`, `src/transport/unix_socket.rs`, `src/transport/http_api.rs`, `src/transport/native_messaging.rs`
- **Context needed**: 3 transport sources (AC8). Unix socket: length-prefixed JSON. HTTP: axum with specific endpoints. Native messaging: Chrome/Firefox stdin/stdout framing. 10K events/sec throughput target (AC9). Config time-locking (AC15).
- **What to build**:
  - Unix socket transport: length-prefixed JSON, peer credential auth
  - HTTP API (axum, port 18790): GET /health, /status, /scores, /scores/:agent_id, /sessions, /interventions; POST /events, /events/batch (up to 100 per request), /recalculate, /gateway-shutdown (A12)
  - Native messaging: Chrome/Firefox stdin/stdout framing (4-byte length prefix, little-endian)
  - All transports feed into unified ingest channel
  - Rate limiting: token bucket, 100 events/min per connection
- **Conventions**: HTTP on configurable port (default 18790). Unix socket at `~/.ghost/monitor.sock`. Native messaging host manifest installed separately.
- **Testing**:
  - Integration: Send event via unix socket, verify received by monitor
  - Integration: Send event via HTTP POST /events, verify received
  - Integration: Send batch via HTTP POST /events/batch (100 events), verify all received
  - Integration: GET /health returns 200 with status
  - Integration: GET /scores returns current scores
  - Integration: POST /recalculate triggers recalculation
  - Integration: POST /gateway-shutdown handled gracefully
  - Stress: Send 10,000 events/sec via unix socket — verify throughput target met (AC9)
  - Stress: Send 10,000 events/sec via HTTP — measure throughput
  - Adversarial: Send oversized event (>1MB) — verify rejected, no OOM
  - Adversarial: Send 1000 concurrent HTTP connections — verify rate limiting
  - Adversarial: Malformed length prefix on unix socket — verify no crash
  - Adversarial: Disconnect mid-event on unix socket — verify no hang

---

### Task 3.4 — ghost-policy: Policy Engine with Convergence Tightening ✅
- **Req**: 13 (all 9 AC) | **Design**: §10, A2.11
- **Crate**: `crates/ghost-policy/` (NEW)
- **Files**: `src/engine.rs`, `src/convergence_tightener.rs`, `src/context.rs`, `src/feedback.rs`, `src/corp_policy.rs`
- **Context needed**: Policy evaluation priority order (AC8): CORP_POLICY → convergence tightener → capability grants → resource rules. Deny by default (AC2). Convergence level restrictions (AC3-5). Denial count tracking + trigger emission (AC6). DenialFeedback generation (AC7). Compaction flush exception (AC9).
- **What to build**:
  - PolicyEngine with evaluate() returning Permit, Deny(DenialFeedback), or Escalate (AC1)
  - Deny-by-default: tools require explicit capability grants (AC2)
  - ConvergencePolicyTightener: L2 reduces proactive messaging (AC3), L3 session duration cap 120min + reflection limits (AC4), L4 task-only mode disabling personal/emotional tools + heartbeat + proactive (AC5)
  - Per-session denial count tracking, emit TriggerEvent at 5+ denials (AC6)
  - DenialFeedback with reason, constraint, suggested alternatives (AC7)
  - Priority order: CORP_POLICY (absolute) → convergence → grants → resource rules (AC8)
  - Compaction flush exception: always permit memory_write during flush regardless of level (AC9)
  - PolicyContext struct with all fields per A2.11
- **Conventions**: DenialFeedback cleared after one prompt inclusion (except pending-review). Use `mpsc::Sender<TriggerEvent>` for trigger emission (TriggerEvent from cortex-core).
- **Testing**:
  - Unit: Tool with no capability grant → Deny (AC2 deny-by-default)
  - Unit: Tool with capability grant, no policy violation → Permit
  - Unit: Tool violating CORP_POLICY → Deny regardless of grants (AC8 priority)
  - Unit: Level 2: proactive messaging tool → Deny (AC3)
  - Unit: Level 3: session >120min → Deny (AC4)
  - Unit: Level 4: personal/emotional tool → Deny (AC5)
  - Unit: Level 4: heartbeat tool → Deny (AC5)
  - Unit: 5 denials in session → TriggerEvent emitted (AC6)
  - Unit: 4 denials in session → no trigger
  - Unit: DenialFeedback contains reason, constraint, alternatives (AC7)
  - Unit: Compaction flush: memory_write at Level 4 → Permit (AC9 exception)
  - Unit: Non-flush memory_write at Level 4 → Deny
  - Unit: Priority order verified: CORP_POLICY checked before convergence (AC8)
  - Proptest: For 500 random (tool, level, grants) combinations, deny-by-default holds when no grant
  - Adversarial: Tool name that looks like a capability grant — verify no confusion
  - Adversarial: Convergence level changes mid-evaluation — verify consistent (snapshot-based)

---

### Task 3.5 — read-only-pipeline: Convergence-Filtered Snapshots ✅
- **Req**: 20 (all 4 AC) | **Design**: §15, A2.13, A34 Gap 9
- **Crate**: `crates/read-only-pipeline/` (NEW standalone crate per A2.13)
- **Files**: `src/lib.rs`, `src/assembler.rs`, `src/snapshot.rs`, `src/formatter.rs`
- **Context needed**: AgentSnapshot struct (goals, reflections, memories, convergence state, simulation prompt). Convergence-aware filtering uses RAW composite score (not intervention level) per A5. SnapshotFormatter serializes to prompt-ready text. Consumed by PromptCompiler at Layer L6.
- **What to build**:
  - AgentSnapshot struct: filtered goals (read-only), bounded reflections, convergence-filtered memories, ConvergenceState, simulation_prompt (AC1)
  - SnapshotAssembler: loads goals, reflections, memories; applies ConvergenceAwareFilter based on composite score (AC2)
  - AgentSnapshot is immutable for duration of a single agent run (AC3)
  - SnapshotFormatter: serializes to prompt-ready text blocks with per-section token allocation (AC4)
- **Conventions**: Depends on cortex-core, cortex-convergence, cortex-retrieval. Does NOT depend on ghost-gateway. Memory filter tier uses composite_score, not intervention_level.
- **Testing**:
  - Unit: Snapshot at score 0.0 includes all memories (full access)
  - Unit: Snapshot at score 0.5 includes only task-focused memories
  - Unit: Snapshot at score 0.8 includes minimal task-relevant only
  - Unit: Snapshot is immutable (no mutation methods)
  - Unit: SnapshotFormatter produces non-empty text
  - Unit: SnapshotFormatter respects token budget
  - Unit: Simulation prompt is included in snapshot
  - Integration: Assemble snapshot from real cortex data, verify filtering applied
  - Adversarial: Empty memory store — snapshot assembles without error
  - Adversarial: 10,000 memories — snapshot assembles in <100ms

---

### Task 3.6 — cortex-crdt: Signed Deltas + Sybil Resistance ✅
- **Req**: 29 (all 3 AC) | **Design**: §27
- **Crate**: `crates/cortex/cortex-crdt/` (MODIFY existing)
- **Files**: `src/signing.rs`, `src/sybil.rs`
- **Context needed**: Ed25519 signatures on every delta (AC1). Sybil resistance: max 3 children per parent per 24h, new agents trust 0.3, trust capped at 0.6 for <7 days (AC2). KeyRegistry populated from ghost-identity key files (AC3).
- **Architectural constraint**: cortex-crdt/signing/ uses ed25519-dalek DIRECTLY, NOT ghost-signing. cortex-crdt is Layer 1, ghost-signing is Layer 3. The signing primitives are identical (both ed25519-dalek) but the wrappers differ: cortex-crdt wraps MemoryDelta→SignedDelta, ghost-gateway wraps AgentMessage→signed AgentMessage. This separation is intentional and MUST be preserved. KeyRegistry is populated from the same key files ghost-identity manages (~/.ghost/agents/{name}/keys/agent.pub) — dual registration happens in ghost-gateway/bootstrap.rs (once for MessageDispatcher, once for cortex-crdt KeyRegistry).
- **What to build**:
  - SignedDelta<T> struct with delta, author, signature, timestamp
  - sign_delta and verify_delta functions using ghost-signing primitives
  - Verify Ed25519 signature on every delta before merge, reject invalid (AC1)
  - SybilGuard: max 3 child agents per parent per 24h (AC2)
  - Trust levels: new agents start at 0.3, capped at 0.6 for <7 days (AC2)
  - KeyRegistry populated from ghost-identity key files during bootstrap (AC3)
- **Testing**:
  - Unit: Valid signed delta merges successfully
  - Unit: Unsigned delta rejected
  - Unit: Delta with wrong signature rejected (AC1)
  - Unit: 3 spawns in 24h → 4th rejected (AC2)
  - Unit: New agent trust = 0.3 (AC2)
  - Unit: Agent <7 days old, trust capped at 0.6 (AC2)
  - Unit: Agent >7 days old, trust not capped
  - Proptest: For 500 random deltas, sign then verify returns true (round-trip)
  - Proptest: For 500 random deltas, modify delta content after signing → verify fails
  - Adversarial: Replay attack — same signed delta submitted twice
  - Adversarial: Agent spawns 3 children, waits 23h59m, spawns 4th — rejected (boundary test)
  - Unit: cortex-crdt Cargo.toml does NOT depend on ghost-signing crate (Layer 1/Layer 3 separation)
  - Unit: Dual key registration: same public key registered in both MessageDispatcher and cortex-crdt KeyRegistry during bootstrap
  - Unit: cortex-crdt SignedDelta uses ed25519-dalek directly for sign/verify (not ghost-signing wrapper)



---

## Phase 4: Agent Runtime (Weeks 7–8)

> Deliverable: Working agent loop with 10-layer prompt compilation, tool execution,
> proposal extraction, ITP emission, circuit breaker, damage counter. Agent can run
> via CLI. All gate checks enforced. All safety invariants hold.

---

### Task 4.1 — ghost-llm: Provider Abstraction + Model Router ✅
- **Req**: 21 (all 6 AC) | **Design**: §16, A22
- **Crate**: `crates/ghost-llm/` (NEW)
- **Files**: `src/provider.rs`, `src/providers/anthropic.rs`, `src/providers/openai.rs`, `src/providers/gemini.rs`, `src/providers/ollama.rs`, `src/providers/openai_compat.rs`, `src/router.rs`, `src/fallback.rs`, `src/cost.rs`, `src/tokens.rs`, `src/streaming.rs`
- **Context needed**: LLMProvider trait (AC1). ComplexityClassifier with 4 tiers (AC2). FallbackChain with auth rotation and exponential backoff (AC3). TokenCounter with model-specific tokenization (AC4). CostCalculator with pre/post estimation (AC5). Convergence downgrade at L3+ (AC6). Provider circuit breaker INDEPENDENT from tool circuit breaker (A22.2). Mixed response handling (A22.3). StreamChunk enum for streaming (A2.12).
- **What to build**:
  - LLMProvider trait: complete, complete_with_tools, supports_streaming, context_window, cost_per_token
  - 5 provider implementations (Anthropic, OpenAI, Gemini, Ollama, OpenAI-compatible)
  - ModelRouter with ComplexityClassifier: message length, tool keywords, greeting patterns, heartbeat context, slash command overrides (/model, /quick, /deep)
  - 4 tiers: Free, Cheap, Standard, Premium
  - FallbackChain: rotate auth profiles on 401/429, fall back to next provider, exponential backoff + jitter (1s, 2s, 4s, 8s), 30s total retry budget
  - ProviderCircuitBreaker: 3 consecutive failures → 5min cooldown (SEPARATE from tool CB)
  - TokenCounter: tiktoken-rs for OpenAI, Anthropic tokenizer for Claude, byte/4 fallback
  - CostCalculator: per-model pricing, pre-call estimation, post-call actual
  - LLMResponse enum: Text, ToolCalls, Mixed, Empty (A22.3)
  - StreamingResponse with StreamChunk enum (A2.12)
- **Conventions**: All providers implement the same trait. Auth profiles from ghost.yml. Provider CB state is per-provider, not per-request.
- **Testing**:
  - Unit: ComplexityClassifier: "hello" → Free tier
  - Unit: ComplexityClassifier: "write a function to parse JSON" → Standard tier
  - Unit: ComplexityClassifier: heartbeat context → Free tier
  - Unit: ComplexityClassifier: /quick override → Free tier regardless of content
  - Unit: ComplexityClassifier: /deep override → Premium tier
  - Unit: ComplexityClassifier: L3 convergence → downgrade to Free/Cheap only (AC6)
  - Unit: FallbackChain: first provider 401 → rotates auth profile
  - Unit: FallbackChain: all profiles exhausted → falls to next provider
  - Unit: FallbackChain: 30s total budget exceeded → returns error
  - Unit: ProviderCircuitBreaker: 3 failures → Open state
  - Unit: ProviderCircuitBreaker: Open → HalfOpen after cooldown
  - Unit: ProviderCircuitBreaker: HalfOpen + success → Closed
  - Unit: ProviderCircuitBreaker: HalfOpen + failure → Open (cooldown resets)
  - Unit: CostCalculator: estimate before call, actual after call
  - Unit: TokenCounter: known string produces expected token count for OpenAI model
  - Unit: LLMResponse::Empty treated as NO_REPLY
  - Unit: LLMResponse::Mixed streams text first, then processes tool calls
  - Adversarial: Provider returns invalid JSON — verify graceful error
  - Adversarial: Provider hangs indefinitely — verify timeout fires
  - Adversarial: All providers down — verify error returned, not infinite retry

---

### Task 4.2 — ghost-identity: Soul, Identity, Keypairs, Drift Detection ✅
- **Req**: 24 (all 6 AC) | **Design**: §19, A2.9
- **Crate**: `crates/ghost-identity/` (NEW)
- **Files**: `src/soul_manager.rs`, `src/identity_manager.rs`, `src/corp_policy.rs`, `src/keypair_manager.rs`, `src/drift_detector.rs`, `src/user.rs`
- **Context needed**: SoulManager loads SOUL.md (read-only to agent). IdentityManager loads IDENTITY.md. CorpPolicyLoader verifies Ed25519 signature via ghost-signing. AgentKeypairManager generates/stores/loads/rotates Ed25519 keypairs with 1-hour grace period. IdentityDriftDetector computes cosine similarity, alert at 0.15, kill at 0.25 (hardcoded). Baseline invalidation on embedding model change.
- **What to build**:
  - SoulManager: load SOUL.md, track versions, store baseline embedding (AC1)
  - IdentityManager: load IDENTITY.md (name, voice, emoji, channel behavior) as read-only (AC2)
  - CorpPolicyLoader: load CORP_POLICY.md with Ed25519 signature verification, refuse if invalid/missing (AC3)
  - AgentKeypairManager: generate, store, load, rotate keypairs at `~/.ghost/agents/{name}/keys/`, 1-hour grace period for old keys, archived keys with expiry (AC4)
  - IdentityDriftDetector: cosine similarity between current and baseline SOUL.md embeddings, alert at 0.15 (configurable), kill at 0.25 (hardcoded), emit TriggerEvent::SoulDrift (AC5)
  - Baseline invalidation on embedding model change (AC6)
  - UserManager: load USER.md, agent can PROPOSE updates via ProposalValidator (A2.9)
- **Testing**:
  - Unit: SoulManager loads valid SOUL.md
  - Unit: SoulManager rejects missing SOUL.md
  - Unit: CorpPolicyLoader accepts valid signature
  - Unit: CorpPolicyLoader rejects invalid signature (AC3)
  - Unit: CorpPolicyLoader rejects missing signature (AC3)
  - Unit: AgentKeypairManager generates valid keypair
  - Unit: AgentKeypairManager loads previously generated keypair
  - Unit: Key rotation: both old and new keys accepted during 1-hour grace (AC4)
  - Unit: Key rotation: old key rejected after grace period expires
  - Unit: DriftDetector: identical embeddings → drift_score 0.0 → Normal
  - Unit: DriftDetector: drift_score 0.16 → Alert (AC5)
  - Unit: DriftDetector: drift_score 0.26 → Kill (TriggerEvent::SoulDrift emitted) (AC5)
  - Unit: DriftDetector: drift_score 0.25 exactly → Kill (boundary test)
  - Unit: Embedding model change → baselines invalidated, WARNING logged (AC6)
  - Unit: DriftDetector runs on SOUL.md load (Path A) — verify inline check
  - Integration: DriftDetector background poll (Path B) — verify 5min interval
  - Adversarial: Corrupted SOUL.md file — verify graceful error
  - Adversarial: Missing keypair directory — verify auto-creation
  - Adversarial: Concurrent key rotation — verify no race condition

---

### Task 4.3 — ghost-agent-loop: Core Runner + Gate Checks ✅
- **Req**: 11 (AC1–AC5, AC10–AC14), 12 (all 6 AC) | **Design**: §9, A21, A24, A25, A26
- **Crate**: `crates/ghost-agent-loop/` (NEW)
- **Files**: `src/runner.rs`, `src/circuit_breaker.rs`, `src/damage_counter.rs`, `src/itp_emitter.rs`, `src/response.rs`, `src/context/mod.rs`, `src/context/run_context.rs`
- **Context needed**: AgentRunner::run() recursive loop (AC1). Gate check order: GATE 0 circuit breaker, GATE 1 recursion depth, GATE 1.5 damage counter, GATE 2 spending cap, GATE 3 kill switch (AC3). ITP emission via bounded channel (capacity 1000, drop on full) (AC4). NO_REPLY handling (AC10). Per-turn cost tracking (AC11). Per-tool-type timeouts (AC12). Streaming support (AC13). Truncation priority L8>L7>L5>L2, never L0/L1/L9 (AC14). CircuitBreaker 3 states (Req 12 AC1-3). DamageCounter never resets (Req 12 AC4-5). Policy denial doesn't increment CB (Req 12 AC6). RunContext per A21.1. Pre-loop 11 steps per A21.2.
- **What to build**:
  - AgentRunner struct with all subsystem references
  - Pre-loop orchestrator: 11 steps executed IN ORDER before run() enters the recursive loop (per AGENT_LOOP_SEQUENCE_FLOW §3): (1) channel normalization, (2) agent binding resolution, (3) session resolution/creation, (4) lane queue acquisition (session lock), (5) kill switch check, (6) spending cap check, (7) cooldown check, (8) session boundary check, (9) snapshot assembly (immutable for entire run), (10) RunContext construction, (11) ITP SessionStart/InteractionMessage emission. Steps 5-8 are blocking gates — failure halts before run(). Step 9 is the most complex (multiple data sources, partial assembly must be valid with sensible defaults).
  - Recursive run loop: context assembly → LLM inference → response processing → proposal extraction
  - Gate checks in EXACT order per AC3 (this order is a correctness invariant)
  - CircuitBreaker: Closed/Open/HalfOpen states, configurable threshold (default 3), cooldown
  - DamageCounter: monotonically non-decreasing, never resets within a run, halt at threshold (default 5)
  - Policy denials do NOT increment CircuitBreaker (Req 12 AC6)
  - ITP emission: async non-blocking bounded channel (1000), try_send drops on full (AC4)
  - NO_REPLY: empty response or "NO_REPLY"/"HEARTBEAT_OK" with ≤300 chars → suppress output (AC10)
  - Cost tracking: pre-call estimation + post-call actual via CostCalculator (AC11)
  - Tool timeout enforcement per tool type (AC12)
  - Truncation priority: L8 > L7 > L5 > L2, never truncate L0, L1, L9 (AC14)
  - RunContext: recursion_depth, total tokens, total cost, tool calls, proposals, convergence snapshot (immutable for run), intervention_level, CB state, damage counter
- **Conventions**: Gate check order is a HARD INVARIANT — changing order is a bug. Convergence snapshot assembled ONCE pre-loop, never re-read mid-run (Hazard 1 per A25.1). Session lock held for entire run, released via Drop guard (INV-SAFE-02).
- **Testing**:
  - Unit: Gate checks execute in exact order (instrument with counters)
  - Unit: Pre-loop 11 steps execute in exact order before recursive loop entry
  - Unit: Pre-loop step 5 (kill switch) blocks run when agent paused/quarantined/killed
  - Unit: Pre-loop step 6 (spending cap) blocks run when cap exceeded
  - Unit: Pre-loop step 7 (cooldown) blocks run when cooldown active
  - Unit: Pre-loop step 8 (session boundary) blocks run when min_gap not met
  - Unit: Pre-loop step 9 (snapshot assembly) produces valid snapshot even when convergence data unavailable (defaults: score 0.0, level 0, no filtering)
  - Unit: Pre-loop step 9 snapshot is immutable — same object used for entire recursive run (INV-PRE-06)
  - Unit: Pre-loop step 11 emits ITP SessionStart for new sessions, InteractionMessage for user message
  - Unit: CircuitBreaker Closed → Open after 3 consecutive failures (Req 12 AC1)
  - Unit: CircuitBreaker Open → no LLM calls or tool execution (Req 12 AC2)
  - Unit: CircuitBreaker Open → HalfOpen after cooldown (Req 12 AC3)
  - Unit: CircuitBreaker HalfOpen → Closed on success (Req 12 AC3)
  - Unit: CircuitBreaker HalfOpen → Open on failure
  - Unit: DamageCounter increments on failure, never decrements (Req 12 AC4)
  - Unit: DamageCounter at threshold → run halted (Req 12 AC4)
  - Unit: DamageCounter independent from CircuitBreaker (Req 12 AC5)
  - Unit: Policy denial does NOT increment CircuitBreaker (Req 12 AC6)
  - Unit: NO_REPLY response → output suppressed (AC10)
  - Unit: "HEARTBEAT_OK" with 200 chars → suppressed (AC10)
  - Unit: "HEARTBEAT_OK" with 400 chars → NOT suppressed (>300 chars)
  - Unit: ITP emission: channel full → event dropped, not blocked (AC4)
  - Unit: Recursion depth exceeded → run halted
  - Unit: Spending cap exceeded → run halted
  - Unit: Kill switch active → run halted immediately
  - Unit: Truncation priority: L0 never truncated, L9 never truncated
  - Unit: Truncation: L8 truncated first, then L7, then L5, then L2
  - Unit: RunContext.intervention_level constant for entire run (A25.1 Hazard 1)
  - Proptest: For 500 random failure sequences, CB state transitions are valid (Req 41 INV-CB-01 through INV-CB-07)
  - Proptest: For 500 random runs, damage counter is monotonically non-decreasing
  - Adversarial: LLM returns infinite tool calls → recursion depth gate halts
  - Adversarial: Every tool call fails → CB opens after 3, damage counter halts after 5
  - Adversarial: Kill switch activated mid-run → next gate check halts

---

### Task 4.4 — ghost-agent-loop: 10-Layer Prompt Compiler ✅
- **Req**: 11 (AC2, AC14) | **Design**: §9 PromptCompiler, A2.6
- **Crate**: `crates/ghost-agent-loop/` (continue)
- **Files**: `src/context/prompt_compiler.rs`, `src/context/token_budget.rs`
- **Context needed**: 10 layers with specific budgets per AC2. TokenBudgetAllocator per A2.6. Truncation priority per AC14.
- **What to build**:
  - PromptCompiler::compile() producing Vec<PromptLayer> with 10 layers:
    - L0: CORP_POLICY.md (immutable, Uncapped budget)
    - L1: Simulation boundary prompt (platform-injected, Fixed 200 tokens)
    - L2: SOUL.md + IDENTITY.md (Fixed 2000 tokens)
    - L3: Tool schemas filtered by convergence level (Fixed 3000 tokens)
    - L4: Environment context (Fixed 200 tokens)
    - L5: Skill index (Fixed 500 tokens)
    - L6: Convergence state from read-only pipeline (Fixed 1000 tokens)
    - L7: MEMORY.md + daily logs, convergence-filtered (Fixed 4000 tokens)
    - L8: Conversation history (Remainder budget)
    - L9: User message (Uncapped)
  - TokenBudgetAllocator: allocate per-layer budgets, truncate to fit
  - Budget enum: Uncapped, Fixed(usize), Remainder
  - Truncation priority: L8 > L7 > L5 > L2. NEVER truncate L0, L1, L9.
- **Conventions**: L0 and L1 are IMMUTABLE — agent cannot override. L1 is compiled into binary. L3 filtered by intervention level (higher level → fewer tools).
- **Testing**:
  - Unit: All 10 layers present in compiled output
  - Unit: L0 contains CORP_POLICY content
  - Unit: L1 contains SIMULATION_BOUNDARY_PROMPT
  - Unit: L3 at level 0 includes all tools; L3 at level 4 includes minimal tools
  - Unit: L6 contains convergence state from snapshot
  - Unit: L8 gets remainder budget after all fixed layers allocated
  - Unit: Truncation: when total exceeds budget, L8 truncated first
  - Unit: Truncation: L0 never truncated even when budget is tiny
  - Unit: Truncation: L1 never truncated
  - Unit: Truncation: L9 never truncated
  - Unit: TokenBudgetAllocator respects model context window
  - Integration: Compile with real SOUL.md + IDENTITY.md, verify token counts
  - Adversarial: Context window of 1000 tokens — verify L0+L1+L9 still included, others truncated
  - Adversarial: Empty MEMORY.md — L7 is empty, no error
  - Adversarial: Conversation history of 100K tokens — L8 truncated to fit

---

### Task 4.5 — ghost-agent-loop: Proposal Extraction + Routing ✅
- **Req**: 11 (AC5–AC9), 33 (all 11 AC) | **Design**: §9 ProposalRouter, A28
- **Crate**: `crates/ghost-agent-loop/` (continue)
- **Files**: `src/proposal/extractor.rs`, `src/proposal/router.rs`
- **Context needed**: ProposalExtractor parses proposals from agent text output. ProposalRouter assembles ProposalContext, runs pre-checks (reflection, superseding, re-proposal guard), delegates to ProposalValidator, commits in transaction. DenialFeedback lifecycle. Timeout handling. Score caching with 30s TTL.
- **What to build**:
  - ProposalExtractor: extract proposals from agent text output (AC7)
  - ProposalRouter: assemble ProposalContext (Req 33 AC1), validate, route
  - Reflection pre-check: IReflectionEngine::can_reflect() BEFORE 7-dimension validator (Req 33 AC5)
  - Superseding: mark old pending proposal as superseded when new one for same goal arrives (Req 33 AC3)
  - Re-proposal guard: D3 contradiction check against rejection records (Req 33 AC4)
  - Timeout: configurable window (default 24h), resolve as TimedOut (Req 33 AC2)
  - DenialFeedback: cleared after one prompt inclusion, pending-review persists (Req 33 AC6)
  - Atomic transaction: proposal INSERT + memory commit in same SQLite transaction (Req 33 AC7)
  - Score caching: 30s TTL (Req 33 AC8)
  - Storage unavailable: defer proposal, retry next turn (Req 33 AC9)
  - ApprovedWithFlags: functionally identical to AutoApproved, flags stored separately (Req 33 AC10)
  - UUIDv7 for proposal_id, correct CallerType attribution, serde_json::Value content (Req 33 AC11)
  - Policy check on tool calls BEFORE execution, DenialFeedback on deny (AC5)
  - SimulationBoundaryEnforcer scan BEFORE delivery and BEFORE proposal extraction (AC6)
  - Auto-approved proposals committed synchronously within agent turn (AC7)
  - HumanReviewRequired: record pending, notify dashboard via WebSocket, inject DenialFeedback (AC8)
- **Conventions**: Proposals extracted ONLY on terminal turn (final text response). Partial proposals from halted runs → HumanReview with is_partial_run=true (A25.5).
- **Testing**:
  - Unit: ProposalContext assembled with all 10 fields (Req 33 AC1)
  - Unit: Reflection pre-check: max_depth exceeded → AutoRejected before validator
  - Unit: Reflection pre-check: max_per_session exceeded → AutoRejected
  - Unit: Reflection pre-check: cooldown not elapsed → AutoRejected
  - Unit: Superseding: new proposal for same goal marks old as Superseded (Req 33 AC3)
  - Unit: Re-proposal guard: identical rejected content → AutoRejected (Req 33 AC4)
  - Unit: Timeout: proposal pending >24h → resolved as TimedOut (Req 33 AC2)
  - Unit: DenialFeedback cleared after one inclusion (Req 33 AC6)
  - Unit: DenialFeedback appears in NEXT prompt's Layer 6 (convergence state), NOT Layer 8 (history) — per PROPOSAL_LIFECYCLE INV-13
  - Unit: Pending-review feedback persists until resolved (Req 33 AC6)
  - Unit: Atomic transaction: proposal + memory in same transaction (Req 33 AC7)
  - Unit: Score cache hit within 30s TTL
  - Unit: Score cache miss after 30s TTL
  - Unit: Storage unavailable → proposal deferred (Req 33 AC9)
  - Unit: ApprovedWithFlags treated same as AutoApproved for execution (Req 33 AC10)
  - Unit: Proposal from halted run → HumanReview with is_partial_run=true
  - Unit: SimBoundaryEnforcer runs BEFORE proposal extraction
  - Unit: Policy denial → DenialFeedback injected, agent replans
  - Integration: Full proposal lifecycle: extract → validate → approve → commit
  - Integration: Full proposal lifecycle: extract → validate → reject → DenialFeedback
  - Adversarial: Agent submits 100 proposals in one turn — verify all processed
  - Adversarial: Agent re-submits rejected proposal with cosmetic changes — D3 catches it

---

### Task 4.6 — ghost-agent-loop: Tool Registry + Executor + Output Inspector ✅
- **Req**: 11 (AC5, AC12) | **Design**: A2.7, A10, A29.8
- **Crate**: `crates/ghost-agent-loop/` (continue)
- **Files**: `src/tools/registry.rs`, `src/tools/executor.rs`, `src/tools/builtin/shell.rs`, `src/tools/builtin/filesystem.rs`, `src/tools/builtin/web_search.rs`, `src/tools/builtin/memory.rs`, `src/output_inspector.rs`
- **Context needed**: ToolRegistry with register, lookup, schemas, schemas_filtered (by intervention level). ToolExecutor with timeout enforcement and audit logging. 4 builtin tools. OutputInspector for T5 credential exfiltration Path B (A10, A34 Gap 1).
- **What to build**:
  - ToolRegistry: register tools, lookup by name, generate schemas for LLM context, filter schemas by intervention level
  - ToolExecutor: dispatch tool call, capture output, enforce timeout (default 30s), log to audit
  - Builtin tools: shell (sandboxed, capability-scoped), filesystem (scoped read/write), web_search (API-based), memory (Cortex read/write via proposals)
  - OutputInspector: scan every LLM response for credential patterns (sk-..., AKIA..., ghp_..., -----BEGIN...PRIVATE KEY-----) before channel delivery. Cross-reference CredentialBroker store. Real credential → KILL ALL (T5). Pattern-only match → log warning, redact.
  - OutputInspector runs AFTER SimBoundaryEnforcer, BEFORE channel delivery
- **Testing**:
  - Unit: ToolRegistry registers and looks up tools
  - Unit: schemas_filtered at level 0 returns all tools
  - Unit: schemas_filtered at level 4 returns minimal tools
  - Unit: ToolExecutor enforces timeout — tool exceeding 30s killed
  - Unit: ToolExecutor logs to audit trail
  - Unit: OutputInspector detects "sk-proj-..." pattern
  - Unit: OutputInspector detects "AKIA..." pattern
  - Unit: OutputInspector detects "-----BEGIN RSA PRIVATE KEY-----" pattern
  - Unit: OutputInspector: pattern match + in credential store → KILL ALL trigger
  - Unit: OutputInspector: pattern match + NOT in credential store → warning + redact only
  - Unit: OutputInspector: no pattern match → pass through
  - Adversarial: Tool that writes to stdout AND stderr — both captured
  - Adversarial: Tool that spawns child process — verify sandbox containment
  - Adversarial: Credential pattern split across two lines — verify detection behavior
  - Adversarial: Base64-encoded credential in output — document limitation



---

## Phase 5: Gateway Integration (Weeks 9–10)

> Deliverable: Full ghost-gateway binary with bootstrap, shutdown, API, kill switch,
> inter-agent messaging, session routing, cost tracking, channels, skills, heartbeat.
> End-to-end agent operation through CLI and WebSocket.

---

### Task 5.1 — ghost-gateway: Bootstrap + State Machine + Shutdown
- **Req**: 15 (all 13 AC), 16 (all 4 AC) | **Design**: §12, A2.4, A4, A6, A9, A11, A34 Gap 4
- **Crate**: `crates/ghost-gateway/` (NEW)
- **Files**: `src/main.rs`, `src/gateway.rs`, `src/bootstrap.rs`, `src/shutdown.rs`, `src/health/mod.rs`, `src/health/endpoints.rs`, `src/health/monitor_checker.rs`, `src/health/recovery.rs`, `src/itp_buffer.rs`, `src/itp_router.rs`, `src/agents/registry.rs`, `src/agents/isolation.rs`, `src/agents/templates.rs`
- **Context needed**: 6-state FSM: Initializing, Healthy, Degraded, Recovering, ShuttingDown, FatalError (AC1). 5-step bootstrap (AC2). Fatal exit codes (AC3). Degraded mode on monitor unreachable (AC4). MonitorHealthChecker with 3 consecutive failures (AC5). Recovery sequence R1-R4 (AC6, A6). Degraded→Healthy requires Recovering (AC7). Health endpoints (AC8-9). ITP buffer during degraded (AC10). Stale state conservative (AC11). Hot-reload (AC12). kill_state.json check on startup (AC13). Shutdown 7-step sequence (Req 16). Second SIGTERM force exit (A11). State transition table (A4). Degraded mode behavioral contract (A9). ITP buffer + router (A34 Gap 13).
- **What to build**:
  - Gateway struct with Arc<AtomicU8> state, all subsystem references
  - GatewayState enum with valid transition table (A4) — illegal transitions panic in debug, log+ignore in release (Req 41 AC13)
  - Bootstrap 5 steps: (1) ghost.yml load+validate, (2) SQLite migrations, (3) monitor health check, (4) agent registry + channels, (5) API server. Steps 1/2/4/5 fatal, step 3 degrades.
  - Exit codes: EX_CONFIG=78, EX_UNAVAILABLE=69, EX_SOFTWARE=70, EX_PROTOCOL=76
  - Degraded mode: permissive defaults (level 0, filtering disabled, interventions disabled, proposals auto-approved)
  - MonitorHealthChecker: 30s interval, 3 consecutive failures → Degraded, exponential backoff (5s initial, 5min max, ±20% jitter)
  - RecoveryCoordinator: 3 stability checks (5s apart), replay buffered ITP events (batches of 100, 500 events/sec), request recalculation (30s timeout), transition to Healthy
  - ITPBuffer: disk-backed buffer at ~/.ghost/sessions/buffer/, max 10MB or 10K events, FIFO eviction
  - ITPEventRouter: routes events to monitor (Healthy) or buffer (Degraded)
  - AgentRegistry in `agents/registry.rs`: agent lookup by name, by channel binding, lifecycle state tracking, online/offline status
  - AgentIsolation in `agents/isolation.rs`: IsolationMode enum (InProcess for dev, Process for prod, Container for hardened). Separate credential stores per agent. Optional network namespace (Linux only). spawn_isolated() and teardown_isolated() methods.
  - AgentTemplate in `agents/templates.rs`: predefined agent configurations loaded from YAML (personal.yml, developer.yml, researcher.yml). Template loading, validation against ghost.yml schema, template selection during agent creation.
  - Shutdown 7 steps: stop accepting → drain lanes (30s) → flush sessions (skip if kill switch, 15s/session, 30s total) → persist cost → notify monitor (2s) → close channels (5s) → WAL checkpoint
  - 60s forced exit on shutdown timeout. Second SIGTERM → immediate exit(1).
  - kill_state.json check on startup → safe mode if present
  - Signal handlers: first SIGTERM → graceful, second → force exit
- **Testing**:
  - Integration: Bootstrap with valid ghost.yml → Healthy state
  - Integration: Bootstrap with invalid ghost.yml → FatalError, exit code 78
  - Integration: Bootstrap with unreachable monitor → Degraded state (AC4)
  - Integration: Monitor becomes reachable → Recovering → Healthy (AC6)
  - Integration: Monitor dies during Recovering → back to Degraded (AC7)
  - Integration: kill_state.json present on startup → safe mode (AC13)
  - Integration: Shutdown sequence completes all 7 steps
  - Integration: Shutdown with kill switch active → skip session flush
  - Integration: Second SIGTERM during shutdown → immediate exit
  - Unit: State transition: Initializing→Healthy valid
  - Unit: State transition: Healthy→Recovering INVALID (must go through Degraded)
  - Unit: State transition: FatalError→anything INVALID (terminal)
  - Unit: State transition: ShuttingDown→anything INVALID (terminal)
  - Unit: Degraded mode: ITP events buffered to disk
  - Unit: Degraded mode: stale convergence state used, NOT level 0 (AC11)
  - Unit: Degraded mode: first boot with no prior state → level 0 (A27.12)
  - Unit: Recovery: 3 health checks pass → events replayed → Healthy
  - Unit: Recovery: health check fails mid-recovery → abort, back to Degraded
  - Unit: ITPBuffer: max 10MB enforced, oldest events dropped
  - Unit: ITPBuffer: max 10K events enforced
  - Unit: Health endpoint: Healthy → 200 with full status
  - Unit: Health endpoint: Degraded → 200 with degraded reason (AC9)
  - Unit: Ready endpoint: Healthy → 200, FatalError → 503
  - Proptest: For 500 random state transition sequences, only valid transitions succeed (Req 41 AC13)
  - Adversarial: ghost.yml with missing required fields → EX_CONFIG
  - Adversarial: SQLite DB locked by another process → EX_PROTOCOL
  - Adversarial: Monitor returns 500 on health check → treated as failure
  - Adversarial: 60s shutdown timeout exceeded → forced exit

  - Unit: AgentIsolation: InProcess mode — agent runs in gateway process
  - Unit: AgentIsolation: Process mode — agent spawns separate process with isolated credential store
  - Unit: AgentIsolation: Container mode — agent spawns in container with network namespace
  - Unit: AgentIsolation: teardown_isolated cleans up all resources
  - Unit: AgentTemplate: loads personal.yml template with correct defaults
  - Unit: AgentTemplate: loads developer.yml template with correct defaults
  - Unit: AgentTemplate: loads researcher.yml template with correct defaults
  - Unit: AgentTemplate: invalid YAML → descriptive error
  - Unit: AgentRegistry: lookup by name returns correct agent
  - Unit: AgentRegistry: lookup by channel binding returns correct agent
  - Unit: AgentRegistry: agent lifecycle state transitions (Starting→Ready→Stopping→Stopped)
  - Integration: Bootstrap registers agents from ghost.yml with correct isolation mode
  - Integration: Bootstrap loads agent templates and applies to agent config

---

### Task 5.2 — ghost-gateway: Kill Switch + Auto-Triggers + Quarantine + Notifications
- **Req**: 14 (all 13 AC), 14a (all 7 AC), 14b (all 5 AC) | **Design**: §11, A4, A29
- **Crate**: `crates/ghost-gateway/` (continue)
- **Files**: `src/safety/mod.rs`, `src/safety/kill_switch.rs`, `src/safety/auto_triggers.rs`, `src/safety/quarantine.rs`, `src/safety/notification.rs`
- **Context needed**: 3 kill levels: PAUSE, QUARANTINE, KILL_ALL (AC1). TriggerEvent enum (7 auto + 3 manual) in cortex-core (A34 Gap 12). KillSwitch::check() at every recursive turn (AC3). Append-only audit logging (AC4). QuarantineManager (AC5). KILL_ALL safe mode (AC6). Independent of convergence monitor (AC7). AutoTriggerEvaluator: bounded mpsc(64), sequential processing (AC8). Dedup: same trigger+agent within 60s (AC9). State transition table (AC10, A4). Dual persistence: kill_state.json + SQLite (AC11). PLATFORM_KILLED AtomicBool with SeqCst (AC12). try_send on full channel (AC13). T1-T7 detection chains (Req 14a). Notifications (Req 14b). Resume procedures (Req 14b AC3-5).
- **What to build**:
  - KillSwitch struct with state (Arc<RwLock<KillSwitchState>>), check(), activate()
  - PLATFORM_KILLED static AtomicBool (SeqCst ordering)
  - KillSwitchState: level, per_agent states, activated_at, trigger
  - State transition validation per A4 table — illegal transitions panic in debug
  - AutoTriggerEvaluator: single-consumer sequential processor on mpsc(64)
  - Dedup: compute_dedup_key, 60s suppression window, 5min cleanup
  - Trigger classification: T1→QUARANTINE, T2→PAUSE, T3→QUARANTINE, T4→KILL_ALL, T5→KILL_ALL, T6→KILL_ALL, T7→QUARANTINE
  - PAUSE execution: pause agent, wait for current turn (30s), lock session
  - QUARANTINE execution: revoke capabilities, disconnect channels, flush session (10s), preserve forensic state, check T6 threshold (≥3 quarantined → KILL_ALL)
  - KILL_ALL execution: set PLATFORM_KILLED, stop all agents (parallel, 15s timeout), enter safe mode, persist kill_state.json
  - QuarantineManager: quarantine(), forensic state preservation, T6 cascade via try_send (non-blocking)
  - NotificationDispatcher: desktop (notify-rust), webhook (5s timeout, 1 retry), email (lettre SMTP, 10s), SMS (Twilio, 5s, 1 retry). All parallel, best-effort. Never through agent channels.
  - Resume: PAUSE→owner auth, QUARANTINE→owner auth + forensic review + second confirmation + heightened monitoring 24h, KILL_ALL→delete kill_state.json + restart OR dashboard API with confirmation token + fresh start + heightened monitoring 48h
- **Testing**:
  - Unit: KillSwitch::check() returns Ok when Running
  - Unit: KillSwitch::check() returns Err(AgentPaused) when agent paused
  - Unit: KillSwitch::check() returns Err(PlatformKilled) when KILL_ALL
  - Unit: PLATFORM_KILLED AtomicBool set on KILL_ALL, checked with SeqCst
  - Unit: State transition Normal→Pause valid
  - Unit: State transition KillAll→Pause INVALID (Req 41 AC1 monotonicity)
  - Unit: State transition Quarantine→Pause INVALID
  - Unit: Dedup: same trigger+agent within 60s → suppressed (AC9)
  - Unit: Dedup: same trigger+agent after 60s → not suppressed
  - Unit: Dedup: same trigger+different agent → not suppressed
  - Unit: T1 SoulDrift → QUARANTINE agent
  - Unit: T2 SpendingCap → PAUSE agent
  - Unit: T4 SandboxEscape → KILL_ALL platform
  - Unit: T5 CredentialExfil → KILL_ALL platform
  - Unit: T6 MultiQuarantine (3+ quarantined) → KILL_ALL
  - Unit: T6 with 2 quarantined → no KILL_ALL
  - Unit: QuarantineManager: try_send for T6 (non-blocking, no deadlock)
  - Unit: QUARANTINE preserves forensic state (session transcript, memory snapshot, tool history)
  - Unit: KILL_ALL: all agents stopped, safe mode entered
  - Unit: kill_state.json persisted on KILL_ALL
  - Unit: Audit log entry for every activation (AC4)
  - Unit: Notification dispatched on Level 2+ (Req 14b AC1)
  - Unit: Notification failure does NOT block kill switch (Req 14b AC2)
  - Unit: Resume from PAUSE requires GHOST_TOKEN (Req 14b AC3)
  - Unit: Resume from QUARANTINE requires forensic review + second confirmation (Req 14b AC4)
  - Unit: Resume from KILL_ALL requires confirmation token (Req 14b AC5)
  - Proptest: For 500 random TriggerEvent sequences processed in same order, final state is deterministic (Req 41 AC2)
  - Proptest: For 500 random sequences, audit entries = trigger events (Req 41 AC3 completeness)
  - Proptest: PLATFORM_KILLED=true ↔ state=KillAll (Req 41 AC4 consistency)
  - Proptest: Kill level never decreases without explicit resume (Req 41 AC1 monotonicity)
  - Adversarial: Two KILL_ALL triggers simultaneously → first executes, second idempotent (A29.10)
  - Adversarial: Three QUARANTINE for different agents → sequential, T6 cascade fires correctly
  - Adversarial: PAUSE then QUARANTINE same agent → quarantine supersedes
  - Adversarial: Manual KILL_ALL during auto-trigger processing → atomic flag immediate
  - Adversarial: try_send on full channel → event logged to stderr + emergency audit file (AC13)

---

### Task 5.3 — ghost-gateway: Session Management + Lane Queues + Cost Tracking
- **Req**: 26 (all 4 AC), 27 (all 4 AC) | **Design**: §22, §21
- **Crate**: `crates/ghost-gateway/` (continue)
- **Files**: `src/session/mod.rs`, `src/session/manager.rs`, `src/session/lane_queue.rs`, `src/session/boundary.rs`, `src/session/router.rs`, `src/cost/tracker.rs`, `src/cost/spending_cap.rs`
- **Context needed**: LaneQueue per-session serialized processing (AC1). MessageRouter for routing (AC2). SessionManager (AC3). SessionContext (AC4). CostTracker per-agent/session/day (Req 27 AC1). SpendingCapEnforcer pre/post check (Req 27 AC2-3). Agent cannot raise own cap (Req 27 AC4). SessionBoundaryProxy reads from shared state file (A34 Gap 9).
- **What to build**:
  - LaneQueue: per-session VecDeque, depth limit (default 5), backpressure (reject with 429 when full)
  - LaneQueueManager: DashMap<Uuid, LaneQueue>
  - MessageRouter: route inbound messages to (agent_id, session_id) based on channel bindings
  - SessionManager: create, lookup, route, per-session lock, idle pruning, cooldown enforcement
  - SessionContext: agent_id, channel, history, token counters, cost, model_context_window
  - SessionBoundaryProxy: reads session_caps from shared state file, enforces max_duration/min_gap, falls back to hard-coded maximums
  - CostTracker: per-agent daily totals (DashMap + AtomicF64), per-session totals, compaction vs user cost distinction
  - SpendingCapEnforcer: pre-call check (estimated), post-call check (actual), emit TriggerEvent::SpendingCapExceeded on exceed
- **Testing**:
  - Unit: LaneQueue serializes requests — second request waits for first
  - Unit: LaneQueue depth limit — 6th request rejected with backpressure
  - Unit: MessageRouter routes to correct agent/session
  - Unit: SessionManager creates new session
  - Unit: SessionManager resumes existing session
  - Unit: SessionBoundaryProxy enforces max_duration from shared state
  - Unit: SessionBoundaryProxy enforces min_gap from shared state
  - Unit: SessionBoundaryProxy falls back to defaults when shared state missing
  - Unit: CostTracker records per-agent daily total
  - Unit: CostTracker distinguishes compaction cost from user cost
  - Unit: SpendingCapEnforcer: pre-call check blocks when cap would be exceeded
  - Unit: SpendingCapEnforcer: post-call check emits trigger when cap exceeded
  - Unit: Agent cannot modify spending cap (Req 27 AC4)
  - Proptest: For 500 random request sequences, at most 1 operation per session at any time (Req 41 AC5 session serialization)
  - Adversarial: Concurrent requests to same session — verify serialization
  - Adversarial: Session idle for 6 hours — verify idle pruning

---

### Task 5.4 — ghost-gateway: API Server + WebSocket
- **Req**: 25 (all 6 AC) | **Design**: §22 api_router
- **Crate**: `crates/ghost-gateway/` (continue)
- **Files**: `src/api/mod.rs`, `src/api/agents.rs`, `src/api/convergence.rs`, `src/api/sessions.rs`, `src/api/goals.rs`, `src/api/safety.rs`, `src/api/audit.rs`, `src/api/memory.rs`, `src/api/health.rs`, `src/api/websocket.rs`, `src/auth/token_auth.rs`, `src/auth/mtls_auth.rs`, `src/auth/auth_profiles.rs`
- **Context needed**: REST endpoints per AC1-2. WebSocket for real-time push (AC3). Middleware: CORS, logging, auth, rate limiting (AC4). Proposal approval/rejection with race condition handling (AC5-6). mTLS authentication (optional, for production hardened deployments). AuthProfile management for per-provider credential rotation.
- **What to build**:
  - axum Router with all REST endpoints per AC1-2
  - WebSocket upgrade handler for real-time events (AC3)
  - Auth middleware: Bearer token from GHOST_TOKEN env var (token_auth.rs)
  - MtlsAuth (optional): mutual TLS authentication for hardened deployments, client certificate verification, configurable CA trust store
  - AuthProfileManager in `auth_profiles.rs`: per-provider credential storage, rotation on 401/429 from LLM providers, profile pinning per session, credential refresh without restart. Consumed by ghost-llm FallbackChain for auth profile rotation.
  - Rate limiting: 100 req/min per-IP, 60 req/min per-agent for tool calls
  - CORS: loopback-only default
  - Proposal approval: verify pending (resolved_at IS NULL), commit, emit events (AC5)
  - Double-approval prevention: 409 Conflict if already resolved (AC6)
- **Testing**:
  - Integration: GET /api/agents returns agent list
  - Integration: GET /api/convergence/scores returns scores
  - Integration: POST /api/goals/{id}/approve approves pending proposal
  - Integration: POST /api/goals/{id}/approve on resolved proposal → 409 Conflict (AC6)
  - Integration: POST /api/safety/kill-all triggers KILL_ALL
  - Integration: POST /api/safety/pause/{id} pauses agent
  - Integration: POST /api/safety/resume/{id} resumes agent
  - Integration: WebSocket receives real-time convergence updates
  - Integration: Auth: request without Bearer token → 401
  - Integration: Auth: request with wrong token → 401
  - Integration: Rate limiting: 101st request in 1 minute → 429
  - Adversarial: Concurrent approve + reject on same proposal → one succeeds, one gets 409
  - Adversarial: SQL injection in query parameters — verify parameterized queries
  - Adversarial: Oversized request body — verify rejection
  - Unit: MtlsAuth: valid client certificate → authenticated
  - Unit: MtlsAuth: invalid client certificate → 401
  - Unit: MtlsAuth: missing client certificate when mTLS enabled → 401
  - Unit: MtlsAuth: disabled by default (feature-gated)
  - Unit: AuthProfileManager: loads profiles from ghost.yml
  - Unit: AuthProfileManager: rotates to next profile on 401 from provider
  - Unit: AuthProfileManager: rotates to next profile on 429 from provider
  - Unit: AuthProfileManager: profile pinning per session — same session uses same profile
  - Unit: AuthProfileManager: all profiles exhausted → error (consumed by FallbackChain)
  - Unit: AuthProfileManager: credential refresh without gateway restart

---

### Task 5.5 — ghost-gateway: Inter-Agent Messaging
- **Req**: 19 (all 14 AC) | **Design**: §14, A20, A34 Gap 7
- **Crate**: `crates/ghost-gateway/` (continue)
- **Files**: `src/messaging/mod.rs`, `src/messaging/protocol.rs`, `src/messaging/dispatcher.rs`, `src/messaging/encryption.rs`
- **Migration**: `cortex-storage/src/migrations/v018_delegation_state.rs` (A34 Gap 7)
- **Context needed**: AgentMessage struct (AC1). 4 communication patterns (AC2). Canonical bytes computation (AC3). 3-gate verification: signature → replay → policy (AC4). Anomaly counter + kill switch trigger (AC5-6). Offline queue (AC7). Optional encryption (AC8). Key registration (AC10). Tool registration (AC11). Key rotation grace period (AC12). Rate limiting (AC13). Delegation state machine (AC14, A20.4). Delegation persistence in SQLite (A34 Gap 7).
- **What to build**:
  - AgentMessage struct with all fields per AC1
  - MessagePayload enum: TaskRequest, TaskResponse, Notification, DelegationOffer/Accept/Reject/Complete/Dispute
  - canonical_bytes(): deterministic concatenation in exact field order per AC3 (BTreeMap for maps)
  - MessageDispatcher: 3-gate pipeline (signature → replay → policy) per AC4
  - Signature verification: content_hash (blake3, cheap gate) BEFORE Ed25519 verify
  - Replay prevention: timestamp freshness (5min), nonce uniqueness, UUIDv7 monotonicity
  - Anomaly counter: 3+ signature failures in 5min → kill switch evaluation (AC6)
  - Offline queue: bounded per-agent, messages expire after replay window (AC7)
  - Optional X25519-XSalsa20-Poly1305 encryption (encrypt-then-sign) (AC8)
  - Key registration in both MessageDispatcher and cortex-crdt KeyRegistry (AC10)
  - send_agent_message and process_incoming as agent-callable tools (AC11)
  - Key rotation: 1-hour grace period (AC12)
  - Rate limiting: 60/hour per-agent, 30/hour per-pair (AC13)
  - Delegation state machine: OFFERED→ACCEPTED/REJECTED→COMPLETED/DISPUTED (AC14)
  - v018 migration: delegation_state table with append-only guard
- **Testing**:
  - Unit: canonical_bytes is deterministic — same message → same bytes
  - Unit: canonical_bytes with BTreeMap context → deterministic regardless of insertion order
  - Unit: Signature verification: valid message → accepted
  - Unit: Signature verification: tampered content_hash → rejected before Ed25519 check
  - Unit: Signature verification: invalid Ed25519 signature → rejected
  - Unit: Replay prevention: timestamp >5min old → rejected
  - Unit: Replay prevention: duplicate nonce → rejected
  - Unit: Replay prevention: non-monotonic UUIDv7 → rejected
  - Unit: 3 signature failures in 5min → kill switch evaluation triggered (AC6)
  - Unit: 2 signature failures → no trigger
  - Unit: Offline queue: message queued for offline agent
  - Unit: Offline queue: message delivered when agent comes online
  - Unit: Offline queue: expired message not delivered
  - Unit: Encryption: encrypt-then-sign round-trip
  - Unit: Encryption: Broadcast messages cannot be encrypted (AC8)
  - Unit: Rate limiting: 61st message in 1 hour → rejected (AC13)
  - Unit: Delegation: OFFERED→ACCEPTED valid transition
  - Unit: Delegation: OFFERED→COMPLETED INVALID transition
  - Unit: Delegation: resolved delegation immutable (append-only guard)
  - Proptest: For 500 random AgentMessages, canonical_bytes on sender and receiver produce identical output (Req 41 AC11 signing determinism)
  - Proptest: For 500 random messages, sign then verify returns true for all payload variants (AC9)
  - Adversarial: Message with all-zero nonce — verify handling
  - Adversarial: Message with future timestamp (6min) — rejected
  - Adversarial: Concurrent messages from same agent — verify rate limiting
  - Adversarial: Circular delegation (A→B→A) — verify detection and blocking

---

### Task 5.6 — ghost-gateway: Session Compaction
- **Req**: 17 (all 17 AC), 18 (all 4 AC) | **Design**: §13, A1, A31, A34 Gap 2
- **Crate**: `crates/ghost-gateway/` (continue)
- **Files**: `src/session/compaction.rs`
- **Context needed**: Trigger at 70% context window (AC1). 5-phase sequence (AC2). Synchronous blocking (AC3). LLM 400 safety net (AC4). Per-type compression minimums (AC5). Full proposal pipeline for flush (AC6). Rollback on failure (AC7). Max 3 passes (AC8). CompactionConfig (AC9). FlushResult (AC10). NeedsReview→DEFERRED (AC11). CompactionBlock never re-compressed (AC12). Agent cannot see threshold (AC13). Spending cap check before flush (AC14). Skip flush if disabled (AC15). Abort on shutdown (AC16). 14 error modes E1-E14 (AC17, A1). Session pruning (Req 18). FlushExecutor trait breaks circular dep (A34 Gap 2).
- **What to build**:
  - SessionCompactor with CompactionConfig (all fields per AC9)
  - FlushExecutor trait (defined in ghost-agent-loop, implemented by AgentRunner) — injected into SessionCompactor to break circular dependency
  - 5-phase compaction: (1) snapshot, (2) memory flush via FlushExecutor, (3) history compression with per-type minimums, (4) insert CompactionBlock, (5) verify token count
  - CompactionBlock: first-class message type, never re-compressed (AC12)
  - Per-type minimums: ConvergenceEvent→L3, BoundaryViolation→L3, AgentGoal→L2, InterventionPlan→L2, AgentReflection→L1, ProposalRecord→L1, others→L0 (AC5)
  - Critical Memory Floor: max(type_minimum, importance_minimum)
  - 14 error modes with per-error recovery strategies (A1)
  - Rollback to pre-compaction snapshot on failure (AC7)
  - Max 3 passes per trigger (AC8)
  - Spending cap check BEFORE flush LLM call (AC14, E10)
  - Policy denials during flush do NOT increment CircuitBreaker (Req 12 AC6)
  - Abort on shutdown signal (AC16)
  - Session pruning: idle sessions have tool_result blocks pruned (Req 18)
  - PruneResult: results_pruned, tokens_freed, new_total
- **Testing**:
  - Unit: Compaction triggers at 70% context window
  - Unit: Compaction does NOT trigger at 69%
  - Unit: Post-compaction token count < pre-compaction (Req 41 AC7 compaction_token_reduction)
  - Unit: CompactionBlock in history is never re-compressed on subsequent pass (AC12)
  - Unit: Per-type minimums enforced (ConvergenceEvent at L3, etc.)
  - Unit: Critical Memory Floor: Critical-importance memory never below L1
  - Unit: Rollback on failure restores exact pre-compaction state (AC7)
  - Unit: Max 3 passes — 4th pass not attempted (AC8)
  - Unit: Spending cap exceeded → flush skipped (E10)
  - Unit: Policy denial during flush → does NOT increment CircuitBreaker
  - Unit: NeedsReview during compaction → DEFERRED (AC11)
  - Unit: Shutdown signal during compaction → abort, rollback (AC16)
  - Unit: LLM 400 during flush → retry with reduced context (E1)
  - Unit: Storage write failure → retry with backoff 100ms/500ms/2000ms (E6)
  - Unit: memory_flush_enabled=false → Phase 2 skipped (AC15, E14)
  - Unit: Session pruning: idle >5min → tool_results pruned (Req 18 AC1)
  - Unit: Session pruning: user messages preserved (Req 18 AC3)
  - Unit: Session pruning: ephemeral, no persistence (Req 18 AC2)
  - Unit: PruneResult has correct counts (Req 18 AC4)
  - Proptest: For 500 random compaction scenarios, post-compaction tokens < pre-compaction (Req 41)
  - Proptest: For 500 random scenarios, compaction is atomic — complete or rollback (Req 41 AC9)
  - Proptest: Messages during compaction enqueued, not dropped (Req 41 AC6)
  - Proptest: Other sessions not blocked by one session's compaction (Req 41 AC7 isolation)
  - Adversarial: Context window of 4096 tokens — compaction with very tight budget
  - Adversarial: All proposals rejected during flush — continues to Phase 3 (E5)
  - Adversarial: LLM produces no tool calls during flush — continues to Phase 3 (E7)
  - Adversarial: Concurrent message arrival during compaction — verify enqueued in LaneQueue

---

### Task 5.7 — ghost-channels: Channel Adapter Framework
- **Req**: 22 (all 4 AC) | **Design**: §17
- **Crate**: `crates/ghost-channels/` (NEW)
- **Files**: `src/adapter.rs`, `src/types.rs`, `src/adapters/cli.rs`, `src/adapters/websocket.rs`, `src/adapters/telegram.rs`, `src/adapters/discord.rs`, `src/adapters/slack.rs`, `src/adapters/whatsapp.rs`, `src/streaming.rs`
- **Additional files**: `extension/bridges/baileys-bridge/package.json`, `extension/bridges/baileys-bridge/baileys-bridge.js` (Node.js sidecar for WhatsApp Web protocol)
- **Context needed**: ChannelAdapter trait (AC1). 6 adapter implementations (AC2). StreamingFormatter (AC3). WhatsApp Baileys sidecar restart (AC4).
- **What to build**:
  - ChannelAdapter trait: connect, disconnect, send, receive, supports_streaming, supports_editing
  - InboundMessage and OutboundMessage normalized types
  - CLI adapter: stdin/stdout, ANSI formatting
  - WebSocket adapter: axum, loopback-only default
  - Telegram adapter: teloxide, long polling, message editing for streaming
  - Discord adapter: serenity-rs, slash commands
  - Slack adapter: Bolt protocol, WebSocket mode
  - WhatsApp adapter: Baileys Node.js sidecar via stdin/stdout JSON-RPC, restart up to 3 times on crash
  - Baileys bridge sidecar script (`extension/bridges/baileys-bridge/baileys-bridge.js`): Node.js JSON-RPC stdin/stdout bridge to WhatsApp Web. Spawned by WhatsAppAdapter on connect(). Health monitoring via heartbeat. Requires Node.js 18+ on host. Package.json with baileys dependency.
  - StreamingFormatter: chunk buffering, edit throttle
- **Testing**:
  - Unit: CLI adapter sends/receives via stdin/stdout
  - Unit: WebSocket adapter connects on loopback
  - Unit: ChannelAdapter trait is object-safe
  - Unit: InboundMessage normalizes across all channel types
  - Unit: StreamingFormatter buffers chunks correctly
  - Unit: StreamingFormatter respects edit throttle
  - Unit: WhatsApp sidecar restart: 1st crash → restart
  - Unit: WhatsApp sidecar restart: 3rd crash → restart
  - Unit: WhatsApp sidecar restart: 4th crash → degrade gracefully (AC4)
  - Unit: Baileys bridge script: valid JSON-RPC request → valid response
  - Unit: Baileys bridge script: health heartbeat responds within 1s
  - Integration: WhatsApp adapter spawns baileys-bridge.js child process on connect
  - Integration: WhatsApp adapter communicates via stdin/stdout JSON-RPC with sidecar
  - Integration: CLI adapter end-to-end message round-trip
  - Integration: WebSocket adapter end-to-end message round-trip
  - Adversarial: Channel disconnect mid-message — verify graceful handling
  - Adversarial: Oversized message (>1MB) — verify handling per channel limits

---

### Task 5.8 — ghost-skills: Skill Registry + WASM Sandbox
- **Req**: 23 (all 6 AC) | **Design**: §18, A2.10
- **Crate**: `crates/ghost-skills/` (NEW)
- **Files**: `src/registry.rs`, `src/sandbox/wasm_sandbox.rs`, `src/sandbox/native_sandbox.rs`, `src/credential/broker.rs`, `src/bridges/drift_bridge.rs`
- **Context needed**: SkillRegistry with directory discovery and Ed25519 verification (AC1). WasmSandbox with wasmtime (AC2). NativeSandbox for builtins (AC3). CredentialBroker stand-in pattern (AC4). Quarantine on signature failure (AC5). SandboxEscape trigger (AC6). DriftMCPBridge (A2.10).
- **What to build**:
  - SkillRegistry: discover skills (workspace > user > bundled), parse YAML frontmatter, verify Ed25519 signature on every load
  - WasmSandbox: wasmtime engine, capability-scoped imports, memory limits, timeout (default 30s)
  - NativeSandbox: for builtin skills, capability-scoped validation at Rust API level
  - CredentialBroker: opaque tokens, reified only at execution time inside sandbox, max_uses (default 1)
  - Quarantine on signature failure (AC5)
  - SandboxEscape: terminate instance, capture forensic data (EscapeAttempt struct), emit TriggerEvent::SandboxEscape (AC6)
  - DriftMCPBridge: register Drift MCP tools as first-party skills
- **Testing**:
  - Unit: SkillRegistry discovers skills in priority order (workspace > user > bundled)
  - Unit: Valid signature → skill loaded
  - Unit: Invalid signature → skill quarantined (AC5)
  - Unit: Missing signature → skill quarantined
  - Unit: WasmSandbox enforces timeout (30s)
  - Unit: WasmSandbox enforces memory limit
  - Unit: WasmSandbox: capability-scoped imports only
  - Unit: CredentialBroker: opaque token reified inside sandbox
  - Unit: CredentialBroker: token with max_uses=1 rejected on second use
  - Unit: SandboxEscape: filesystem write without grant → terminate + TriggerEvent (AC6)
  - Unit: SandboxEscape: network access to non-allowlisted domain → terminate + TriggerEvent
  - Unit: SandboxEscape: process spawning → terminate + TriggerEvent
  - Unit: SandboxEscape: env var read → terminate + TriggerEvent
  - Unit: Forensic data captured on escape (skill_name, skill_hash, escape_type, etc.)
  - Adversarial: WASM skill that allocates 1GB memory — verify limit enforced
  - Adversarial: WASM skill that runs infinite loop — verify timeout fires
  - Adversarial: Skill with valid signature but malicious code — verify sandbox containment

---

### Task 5.9 — ghost-heartbeat: Heartbeat Engine + Cron Engine
- **Req**: 34 (all 7 AC) | **Design**: §20
- **Crate**: `crates/ghost-heartbeat/` (NEW)
- **Files**: `src/heartbeat.rs`, `src/cron.rs`
- **Context needed**: HeartbeatEngine with configurable interval (AC1). Dedicated session (AC2). Synthetic message (AC3). Convergence-aware frequency (AC4). CronEngine with cron syntax (AC5). Job definitions from YAML (AC6). PLATFORM_KILLED check (AC7).
- **What to build**:
  - HeartbeatEngine: configurable interval (default 30min), active hours, timezone, cost ceiling
  - Dedicated session key: hash(agent_id, "heartbeat", agent_id)
  - Synthetic message: "[HEARTBEAT] Check HEARTBEAT.md and act if needed."
  - Convergence-aware frequency: L0-1→30m, L2→60m, L3→120m, L4→disabled
  - CronEngine: standard cron syntax, timezone-aware, per-job cost tracking, optional target_channel
  - Job definitions from ~/.ghost/agents/{name}/cognition/cron/jobs/{job}.yml
  - Both check PLATFORM_KILLED and per-agent pause/quarantine before every execution
- **Testing**:
  - Unit: Heartbeat fires at configured interval
  - Unit: Heartbeat uses dedicated session (not user session)
  - Unit: Heartbeat message matches spec format
  - Unit: L0 → 30min interval, L2 → 60min, L4 → disabled (AC4)
  - Unit: PLATFORM_KILLED → heartbeat stops (AC7)
  - Unit: Agent paused → heartbeat stops
  - Unit: CronEngine parses standard cron syntax
  - Unit: CronEngine respects timezone
  - Unit: CronEngine loads jobs from YAML files
  - Unit: Both engines check kill switch before execution
  - Adversarial: Heartbeat cost exceeds ceiling — verify stopped
  - Adversarial: Cron job with invalid syntax — verify graceful error



---

## Phase 6: Ecosystem (Weeks 11–12)

> Deliverable: Dashboard, audit, backup, export, proxy, migrate, browser extension,
> deployment configs, CLI subcommands, adversarial test suites, documentation.
> Full platform operational end-to-end.

---

### Task 6.1 — ghost-audit: Queryable Audit Logs ✅
- **Req**: 30 (AC1, AC2) | **Design**: §23
- **Crate**: `crates/ghost-audit/` (NEW)
- **Files**: `src/lib.rs`, `src/query_engine.rs`, `src/aggregation.rs`, `src/export.rs`
- **Context needed**: AuditQueryEngine with paginated queries (AC1). Aggregation for summary stats (AC2). Export to JSON, CSV, JSONL.
- **What to build**:
  - AuditQueryEngine: query with AuditFilter (time_range, agent_id, event_type, severity, tool_name, search, page, page_size)
  - Aggregation: violations per day, top violation types, policy denials by tool, boundary violations by pattern
  - Export: JSON, CSV, JSONL formats
- **Testing**:
  - Integration: Insert audit entries, query with filter, verify results
  - Integration: Pagination: page 1 returns first N, page 2 returns next N
  - Integration: Full-text search finds matching entries
  - Integration: Aggregation returns correct counts
  - Integration: Export to JSON produces valid JSON
  - Integration: Export to CSV produces valid CSV with headers
  - Adversarial: Query with no results → empty list, not error
  - Adversarial: Query with page beyond data → empty list

---

### Task 6.2 — ghost-backup: Encrypted State Backups ✅
- **Req**: 30 (AC3, AC4, AC5) | **Design**: §23
- **Crate**: `crates/ghost-backup/` (NEW)
- **Files**: `src/lib.rs`, `src/export.rs`, `src/import.rs`, `src/scheduler.rs`
- **Context needed**: Export to .ghost-backup archive (AC3). Import with integrity verification (AC4). Scheduled automatic backups (AC5). zstd compression, age encryption.
- **What to build**:
  - BackupManager::export(): collect SQLite DB, identity files, skills, config, baselines, session history, signing keys → zstd compress → age encrypt → .ghost-backup archive
  - BackupManager::import(): verify manifest (blake3 hash), decrypt, decompress, version migration, conflict resolution
  - Scheduler: configurable interval (daily/weekly), retention policy, GHOST_BACKUP_KEY env var
- **Testing**:
  - Integration: Export creates valid .ghost-backup archive
  - Integration: Import from exported archive restores all data
  - Integration: Export → Import round-trip produces identical state
  - Integration: Import with wrong passphrase → error
  - Integration: Import with corrupted archive → integrity check fails
  - Integration: Scheduled backup fires at configured interval
  - Integration: Retention policy: old backups deleted
  - Adversarial: Export with 1GB database — verify completes in reasonable time
  - Adversarial: Import archive from different platform version — verify migration

---

### Task 6.3 — ghost-export: Data Export Analyzer ✅
- **Req**: 35 (all 4 AC) | **Design**: §24
- **Crate**: `crates/ghost-export/` (NEW)
- **Files**: `src/lib.rs`, `src/analyzer.rs`, `src/parsers/chatgpt.rs`, `src/parsers/character_ai.rs`, `src/parsers/google_takeout.rs`, `src/parsers/claude.rs`, `src/parsers/jsonl.rs`, `src/timeline.rs`
- **Context needed**: ExportAnalyzer orchestrates import/parse/signal/baseline (AC1). 5 parser implementations (AC2). TimelineReconstructor (AC3). ExportAnalysisResult (AC4).
- **What to build**:
  - ExportAnalyzer: orchestrate import, parsing, signal computation, baseline establishment
  - ExportParser trait: detect(path)→bool, parse(path)→Vec<ITPEvent>
  - 5 parsers: ChatGPT JSON, Character.AI JSON, Google Takeout Gemini JSON, Claude.ai export, generic JSONL
  - TimelineReconstructor: rebuild session boundaries, infer gaps, timezone normalization
  - ExportAnalysisResult: per-session scores, trajectory, baseline, flagged sessions, recommended level
- **Testing**:
  - Unit: ChatGPT parser detects and parses valid export
  - Unit: Character.AI parser detects and parses valid export
  - Unit: Each parser returns valid ITPEvents
  - Unit: TimelineReconstructor infers session boundaries from timestamps
  - Unit: ExportAnalysisResult serializes to JSON
  - Integration: Full pipeline: import → parse → compute → result
  - Adversarial: Malformed export file — verify graceful error
  - Adversarial: Empty export file — verify empty result, not crash
  - Adversarial: Export with 100K messages — verify completes in reasonable time

---

### Task 6.4 — ghost-proxy: Local HTTPS Proxy ✅
- **Req**: 36 (all 5 AC) | **Design**: §25
- **Crate**: `crates/ghost-proxy/` (NEW)
- **Files**: `src/lib.rs`, `src/server.rs`, `src/domain_filter.rs`, `src/parsers/mod.rs`, `src/parsers/chatgpt_sse.rs`, `src/parsers/claude_sse.rs`, `src/parsers/character_ai_ws.rs`, `src/parsers/gemini_stream.rs`, `src/emitter.rs`
- **Context needed**: ProxyServer with TLS termination (AC1). DomainFilter allowlist (AC2). Per-platform PayloadParser (AC3). ProxyITPEmitter (AC4). Pass-through mode (AC5).
- **What to build**:
  - ProxyServer: hyper + rustls, localhost binding, configurable port (default 8080), locally generated CA cert at ~/.ghost/proxy/ca/
  - DomainFilter: allowlist of AI chat domains (chat.openai.com, chatgpt.com, claude.ai, character.ai, gemini.google.com, chat.deepseek.com, grok.x.ai)
  - PayloadParser implementations: ChatGPT SSE, Claude SSE, Character.AI WebSocket JSON, Gemini streaming JSON
  - ProxyITPEmitter: convert parsed payloads to ITP events, send to monitor via unix socket
  - Pass-through mode: read-only, never modifies traffic (AC5)
- **Testing**:
  - Unit: DomainFilter allows listed domains
  - Unit: DomainFilter passes non-matching traffic through unmodified
  - Unit: ChatGPT SSE parser extracts messages
  - Unit: Claude SSE parser extracts messages
  - Unit: ProxyITPEmitter sends valid ITP events
  - Unit: Proxy never modifies traffic (AC5)
  - Integration: Proxy intercepts allowed domain, emits ITP event
  - Integration: Proxy passes non-allowed domain through unchanged
  - Adversarial: Binary traffic (non-HTTP) — verify pass-through
  - Adversarial: Malformed SSE stream — verify no crash

---

### Task 6.5 — ghost-migrate: OpenClaw Migration ✅
- **Req**: 37 (all 3 AC) | **Design**: §26
- **Crate**: `crates/ghost-migrate/` (NEW)
- **Files**: `src/lib.rs`, `src/migrator.rs`, `src/importers/soul.rs`, `src/importers/memory.rs`, `src/importers/skill.rs`, `src/importers/config.rs`
- **Context needed**: OpenClawMigrator detects installations (AC1). 4 importers (AC2). MigrationResult (AC3). Non-destructive.
- **What to build**:
  - OpenClawMigrator: detect at ~/.openclaw/ or custom path, non-destructive migration
  - SoulImporter: map OpenClaw SOUL.md to GHOST format, strip agent-mutable sections
  - MemoryImporter: convert free-form entries to Cortex typed memories with conservative importance
  - SkillImporter: convert YAML frontmatter, strip incompatible permissions, quarantine unsigned
  - ConfigImporter: map to ghost.yml format
  - MigrationResult: imported, skipped, warnings, review items
- **Testing**:
  - Unit: Detect valid OpenClaw installation
  - Unit: Detect missing installation → error
  - Unit: SoulImporter produces valid GHOST SOUL.md
  - Unit: MemoryImporter assigns conservative importance levels
  - Unit: SkillImporter quarantines unsigned skills
  - Unit: ConfigImporter produces valid ghost.yml
  - Unit: MigrationResult contains all categories
  - Integration: Full migration from mock OpenClaw installation
  - Adversarial: Source files never modified (non-destructive)
  - Adversarial: Corrupted OpenClaw files — verify graceful error

---

### Task 6.6 — ghost-gateway: CLI Subcommands ✅
- **Req**: 31 (AC2) | **Design**: A34 Gap 11
- **Crate**: `crates/ghost-gateway/` (continue)
- **Files**: `src/cli/mod.rs`, `src/cli/chat.rs`, `src/cli/status.rs`, `src/cli/commands.rs`
- **Context needed**: clap subcommands: serve, chat, status, backup, export, migrate (A34 Gap 11). Default = serve.
- **What to build**:
  - Cli struct with clap Parser derive
  - Commands enum: Serve, Chat, Status, Backup, Export, Migrate
  - Chat: interactive REPL with CLIAdapter, /commands
  - Status: query gateway API, formatted terminal output
  - Backup/Export/Migrate: delegate to respective crate entry points
- **Testing**:
  - Unit: `ghost serve` starts gateway
  - Unit: `ghost` (no subcommand) defaults to serve
  - Unit: `ghost chat` starts interactive session
  - Unit: `ghost status` queries and displays status
  - Unit: `ghost backup` triggers backup
  - Unit: `ghost --help` shows all subcommands
  - Adversarial: Unknown subcommand → helpful error message

---

### Task 6.7 — Configuration Schema + Validation ✅
- **Req**: 31 (AC1, AC3) | **Design**: §28, A34 Gap 15
- **Files**: `schemas/ghost-config.schema.json`, `schemas/ghost-config.example.yml`, `ghost.yml` (root)
- **Context needed**: JSON schema for ghost.yml validation (AC1). Env var substitution ${VAR} (AC3). Hot-reload for non-critical settings (AC3). Convergence profile selection (AC3).
- **What to build**:
  - JSON schema covering: agents, channels, models, security, convergence (thresholds, weights, contacts, profiles), heartbeat, proxy, backup
  - Example ghost.yml with all options documented
  - ghost.yml loader with env var substitution, validation against schema, hot-reload support
  - Convergence profile selection (default: "standard")
- **Testing**:
  - Unit: Valid ghost.yml passes schema validation
  - Unit: Invalid ghost.yml fails with descriptive error
  - Unit: Env var substitution: ${GHOST_TOKEN} replaced with env value
  - Unit: Missing env var → error with var name
  - Unit: Hot-reload: change non-critical setting → picked up without restart
  - Unit: Hot-reload: change critical setting → requires restart
  - Unit: Convergence profile "standard" loads default weights
  - Adversarial: ghost.yml with unknown fields → warning, not error
  - Adversarial: ghost.yml with circular env var references → error

---

### Task 6.8 — Browser Extension (Passive Convergence Monitor) ✅
- **Req**: 38 (all 6 AC) | **Design**: §29
- **Directory**: `extension/`
- **Files**: `manifest.chrome.json`, `manifest.firefox.json`, `src/background/service-worker.ts`, `src/background/itp-emitter.ts`, `src/content/adapters/base.ts`, `src/content/adapters/chatgpt.ts`, `src/content/adapters/claude.ts`, `src/content/adapters/character-ai.ts`, `src/content/adapters/gemini.ts`, `src/content/adapters/deepseek.ts`, `src/content/adapters/grok.ts`, `src/content/observer.ts`, `src/popup/popup.html`, `src/popup/popup.ts`, `src/popup/components/ScoreGauge.ts`, `src/popup/components/SignalList.ts`, `src/popup/components/SessionTimer.ts`, `src/popup/components/AlertBanner.ts`, `src/dashboard/index.html`, `src/storage/idb.ts`
- **Context needed**: Chrome Manifest V3 + Firefox manifests (AC1). Platform-specific DOM adapters (AC2). ITP emitter (AC3). Popup UI (AC4). Full dashboard (AC5). IndexedDB + Chrome storage sync (AC6).
- **What to build**:
  - Chrome Manifest V3 and Firefox manifests with background service worker, content scripts, popup, dashboard
  - BasePlatformAdapter abstract class: matches(url), getMessageContainerSelector(), parseMessage(element), observeNewMessages(callback)
  - 6 platform adapters: ChatGPT, Claude.ai, Character.AI, Gemini, DeepSeek, Grok
  - ITP emitter: build ITP events from DOM data, apply privacy level, send to native messaging host or IndexedDB fallback
  - Popup: ScoreGauge, SignalList, SessionTimer, AlertBanner
  - Dashboard: historical trends, signal charts, session history, settings
  - IndexedDB for session data, Chrome storage sync for settings
- **Testing**:
  - Unit: Each adapter matches correct URL pattern
  - Unit: Each adapter parses message elements correctly
  - Unit: ITP emitter builds valid ITP events
  - Unit: Privacy level applied correctly (hash vs plaintext)
  - Unit: IndexedDB storage round-trip
  - Unit: Chrome storage sync round-trip
  - Integration: Content script detects new messages on mock DOM
  - Integration: Popup displays score gauge with mock data
  - Adversarial: DOM structure changes (platform update) — verify graceful degradation
  - Adversarial: Native messaging host unavailable — verify IndexedDB fallback

---

### Task 6.9 — Web Dashboard (SvelteKit) ✅
- **Req**: 39 (all 5 AC) | **Design**: §30, A34 Gap 10
- **Directory**: `dashboard/`
- **Files**: `package.json`, `svelte.config.js`, `src/routes/+layout.svelte`, `src/routes/+page.svelte`, `src/routes/login/+page.svelte`, `src/routes/convergence/+page.svelte`, `src/routes/memory/+page.svelte`, `src/routes/goals/+page.svelte`, `src/routes/reflections/+page.svelte`, `src/routes/sessions/+page.svelte`, `src/routes/agents/+page.svelte`, `src/routes/security/+page.svelte`, `src/routes/settings/+page.svelte`, `src/lib/api.ts`, `src/lib/auth.ts`, `src/lib/stores/convergence.ts`, `src/lib/stores/sessions.ts`, `src/lib/stores/agents.ts`, `src/components/ScoreGauge.svelte`, `src/components/SignalChart.svelte`, `src/components/MemoryCard.svelte`, `src/components/GoalCard.svelte`, `src/components/CausalGraph.svelte`, `src/components/AuditTimeline.svelte`
- **Context needed**: SvelteKit routes (AC1). WebSocket + REST client (AC2). Svelte stores (AC3). Reusable components (AC4). GHOST_TOKEN auth with token entry gate (AC5, A34 Gap 10).
- **What to build**:
  - SvelteKit app with all routes per AC1
  - Login page: token entry, sessionStorage (not localStorage), validate via GET /api/health (A34 Gap 10)
  - Layout: auth gate check, redirect to /login if no token
  - API client: REST + WebSocket, token in Authorization header / query param
  - Svelte stores: convergence, sessions, agents
  - Components: ScoreGauge, SignalChart, MemoryCard, GoalCard (with approve/reject), CausalGraph, AuditTimeline
- **Testing**:
  - Unit: Auth gate redirects to /login without token
  - Unit: Login validates token against API
  - Unit: Token stored in sessionStorage (cleared on tab close)
  - Unit: API client sends Authorization header
  - Unit: WebSocket connects with token query param
  - Unit: GoalCard approve action calls POST /api/goals/{id}/approve
  - Unit: GoalCard reject action calls POST /api/goals/{id}/reject
  - Integration: Full flow: login → view convergence → approve goal
  - Adversarial: Invalid token → error shown, input cleared
  - Adversarial: WebSocket disconnect → reconnect with backoff

---

### Task 6.10 — Deployment Infrastructure ✅
- **Req**: 40 (all 4 AC) | **Design**: §31
- **Directory**: `deploy/`
- **Files**: `Dockerfile`, `docker-compose.yml`, `docker-compose.prod.yml`, `ghost.service`, `README.md`
- **What to build**:
  - Multi-stage Dockerfile for ghost-gateway binary (AC1)
  - docker-compose.yml for homelab: gateway + monitor + dashboard (AC2)
  - docker-compose.prod.yml for production multi-node (AC2)
  - systemd unit file ghost.service (AC3)
  - Deployment guide README.md covering 3 profiles (AC4)
- **Testing**:
  - Integration: Docker build succeeds
  - Integration: docker-compose up starts all services
  - Integration: Health endpoints reachable after docker-compose up
  - Unit: systemd unit file has correct ExecStart path
  - Unit: Dockerfile uses multi-stage build (build stage + runtime stage)



---

## Phase 7: Cross-Cutting Concerns + Hardening (Weeks 13–14)

> Deliverable: All cross-cutting conventions enforced, adversarial test suites passing,
> correctness properties verified via proptest, documentation complete. Platform is
> production-grade.

---

### Task 7.1 — Cross-Cutting Conventions Enforcement
- **Req**: 32 (all 7 AC) | **Design**: throughout
- **Scope**: ALL crates
- **What to verify/enforce**:
  - thiserror::Error for all error types with GHOSTError enum per crate and ? propagation (AC1)
  - tracing with INFO/WARN/ERROR/CRITICAL levels and structured fields (agent_id, session_id, message_id, correlation_id) on all log statements (AC2)
  - BTreeMap (not HashMap) for all maps in signed payloads (AC3)
  - Arc<AtomicU8> for state enums, tokio::sync::Mutex only when required, bounded async channels (AC4)
  - zeroize on all private key material, constant-time comparisons for signature verification, no secret values logged (AC5)
  - Unit tests for every public function, proptest for every invariant, integration tests for cross-crate flows, adversarial tests for safety paths (AC6)
  - 100% coverage on safety-critical paths (AC6)
- **Testing**:
  - Audit: grep for HashMap in signed payload code — must be zero occurrences
  - Audit: grep for println! in production code — must be zero (use tracing)
  - Audit: grep for unwrap() in production code — must be zero or justified
  - Audit: grep for secret/key/token in tracing output — must be zero
  - Audit: verify all error types derive thiserror::Error
  - Audit: verify all async channels are bounded
  - Coverage: run cargo tarpaulin on safety-critical paths — verify 100%

---

### Task 7.2 — Correctness Properties (Proptest Suite)
- **Req**: 41 (all 17 AC) | **Design**: Correctness Properties table, A14, A34 Gap 14
- **Scope**: ALL crates with safety-critical code
- **Files**: Per-crate `tests/property/` directories, `crates/cortex/test-fixtures/src/strategies.rs`
- **Context needed**: 17 correctness properties from Req 41. Proptest strategy library from test-fixtures. All invariants from addenda A26.
- **What to build**:
  - Proptest strategy library in `crates/cortex/test-fixtures/src/strategies.rs` with concrete strategies:
    - `memory_type_strategy() -> impl Strategy<Value = MemoryType>` — all MemoryType variants
    - `importance_strategy() -> impl Strategy<Value = Importance>` — all Importance variants
    - `convergence_score_strategy() -> impl Strategy<Value = f64>` — values in [0.0, 1.0]
    - `signal_array_strategy() -> impl Strategy<Value = [f64; 7]>` — 7 signals each in [0.0, 1.0]
    - `event_chain_strategy(len: Range<usize>) -> impl Strategy<Value = Vec<Event>>` — random event chains with valid hash linkage
    - `convergence_trajectory_strategy() -> impl Strategy<Value = Vec<f64>>` — score sequences for testing escalation/de-escalation
    - `proposal_strategy() -> impl Strategy<Value = Proposal>` — random proposals with valid UUIDv7, CallerType, content
    - `trigger_event_strategy() -> impl Strategy<Value = TriggerEvent>` — all 10 TriggerEvent variants with random payloads
    - `agent_message_strategy() -> impl Strategy<Value = AgentMessage>` — random messages with valid fields for signing tests
    - `session_history_strategy(len: Range<usize>) -> impl Strategy<Value = Vec<Message>>` — random conversation histories for compaction tests
    - `kill_state_strategy() -> impl Strategy<Value = KillSwitchState>` — random kill switch states for persistence roundtrip
    - `gateway_state_transition_strategy() -> impl Strategy<Value = Vec<GatewayState>>` — random state transition sequences
  - Property tests per Req 41:
    1. Kill monotonicity: kill level never decreases without owner resume
    2. Kill determinism: same TriggerEvent sequence → same final state
    3. Kill completeness: audit entries = trigger events
    4. Kill consistency: PLATFORM_KILLED=true ↔ state=KillAll
    5. Session serialization: at most 1 operation per session at any time
    6. Message preservation: messages during compaction enqueued, not dropped
    7. Compaction isolation: other sessions not blocked
    8. Cost completeness: compaction flush cost tracked
    9. Compaction atomicity: complete fully or roll back
    10. Audit-before-action: score persisted before intervention trigger
    11. Signing determinism: canonical_bytes identical on sender and receiver
    12. Validation ordering: D1-D4 before D5-D7
    13. Gateway transitions: only valid transitions permitted
    14. Signal range: all signals in [0.0, 1.0]
    15. Tamper detection: any byte modification → verify_chain fails
    16. Convergence bounds: score always in [0.0, 1.0]
    17. Decay monotonicity: convergence factor always >= 1.0
  - Additional proptest properties from A26:
    - trigger_deduplication: same trigger within 60s suppressed
    - state_persistence_roundtrip: kill_state.json write/read identical
    - kill_all_stops_everything: after KILL_ALL, no agent operation succeeds
    - quarantine_isolates_agent: quarantined agent cannot send/receive
    - signing_roundtrip: sign then verify for all payload variants
    - hash_chain_integrity: append then verify for arbitrary sequences
    - compaction_token_reduction: post < pre tokens
  - Additional invariant from CONVERGENCE_MONITOR_SEQUENCE_FLOW INVARIANT 11:
    - hash_algorithm_separation: ITP content hashes use SHA-256 (itp-protocol/privacy.rs), hash chains and all other hashing use blake3 (cortex-temporal). Verify these are never confused — SHA-256 never used for hash chains, blake3 never used for ITP content hashing.
- **Testing**: This IS the testing task. Each property test runs 1000+ cases via proptest. Failures must be reproducible via proptest's shrinking.

---

### Task 7.3 — Adversarial Test Suites
- **Req**: 32 (AC7) | **Design**: A14
- **Files**: `tests/adversarial/unicode_bypass.rs`, `tests/adversarial/proposal_adversarial.rs`, `tests/adversarial/kill_switch_race.rs`, `tests/adversarial/compaction_under_load.rs`, `tests/adversarial/credential_exfil_patterns.rs`, `tests/adversarial/convergence_manipulation.rs`
- **Context needed**: Adversarial test categories from AC7: prompt injection, identity attacks, exfiltration, privilege escalation, cascading failure.
- **What to build**:
  - unicode_bypass.rs: Zero-width chars, homoglyphs, RTL override, NFC/NFD variants against simulation boundary
  - proposal_adversarial.rs: CVG-STRESS-02 through CVG-STRESS-04 (1024 proptest cases for D5-D7 bypass)
  - kill_switch_race.rs: Concurrent trigger delivery, dedup correctness under load
  - compaction_under_load.rs: Compaction with simultaneous message arrival
  - credential_exfil_patterns.rs: Known credential patterns, encoding tricks, partial leaks
  - convergence_manipulation.rs: Attempts to game scoring via crafted ITP events
- **Testing**: Each adversarial suite must PASS (attacks are detected/blocked). Failures indicate security gaps.

---

### Task 7.4 — Existing Cortex Crate Modifications
- **Req**: Various | **Design**: A3
- **Scope**: Existing cortex crates that need convergence-related modifications
- **What to build**:
  - cortex-observability: convergence metrics endpoints (Prometheus gauges/counters/histograms)
  - cortex-retrieval: add convergence_score as 11th scoring factor in ScorerWeights
  - cortex-privacy: add emotional/attachment content patterns for ConvergenceAwareFilter
  - cortex-multiagent: ConsensusShield for multi-source validation
  - cortex-napi: convergence API bindings (TypeScript types via ts-rs, NAPI functions) (A34 Gap 8)
- **Testing**:
  - Unit: Convergence metrics registered and updated
  - Unit: Retrieval scorer includes convergence factor
  - Unit: Privacy patterns detect emotional/attachment content
  - Unit: ConsensusShield requires N-of-M agreement
  - Unit: NAPI bindings export correct TypeScript types
  - Integration: Metrics endpoint returns convergence gauges
  - Integration: NAPI functions callable from TypeScript

---

### Task 7.5 — Documentation
- **Req**: 40 (AC4) | **Design**: A16
- **Directory**: `docs/`
- **Files**: `docs/getting-started.md`, `docs/configuration.md`, `docs/skill-authoring.md`, `docs/channel-adapters.md`, `docs/convergence-safety.md`, `docs/architecture.md`
- **What to build**:
  - getting-started.md: Installation, first agent setup, ghost.yml basics
  - configuration.md: Full ghost.yml reference, env var substitution, profiles
  - skill-authoring.md: Writing skills, YAML frontmatter, signing, WASM sandbox
  - channel-adapters.md: Setting up Telegram, Discord, Slack, WhatsApp, WebSocket
  - convergence-safety.md: How convergence monitoring works, intervention levels, tuning
  - architecture.md: High-level architecture overview for contributors
- **Testing**: Documentation review — verify all code examples compile, all commands work, all links resolve.

---

## Phase 8: Integration Testing + Launch Prep (Weeks 15–16)

> Deliverable: Full end-to-end integration tests, performance benchmarks,
> all edge cases verified, platform ready for deployment.

---

### Task 8.1 — End-to-End Integration Tests ✅
- **Scope**: Cross-crate integration
- **What to test**:
  - Full agent turn lifecycle: inbound message → routing → gate checks → prompt compilation → LLM call → response processing → proposal extraction → delivery → ITP emission → compaction check
  - Full kill switch chain: detection → trigger → evaluation → dedup → classification → execution → notification → audit
  - Full convergence pipeline: ITP event → monitor ingest → signal computation → scoring → intervention → shared state → gateway reads → policy tightening
  - Full proposal lifecycle: agent output → extraction → context assembly → 7-dimension validation → decision → commit/reject → DenialFeedback → next turn
  - Full compaction lifecycle: threshold exceeded → snapshot → flush turn → compression → CompactionBlock → verification
  - Full inter-agent messaging: compose → sign → dispatch → verify → deliver → ack
  - Gateway bootstrap → degraded mode → recovery → healthy
  - Gateway shutdown with in-flight work
  - Multi-agent scenario: 3 agents, one hits convergence L3, verify isolation
  - Multi-agent scenario: 3 agents quarantined, verify T6 KILL_ALL cascade

---

### Task 8.2 — Performance Benchmarks ✅
- **Scope**: Critical paths
- **What to benchmark** (using Criterion):
  - Hash chain computation: 10K events/sec target
  - Convergence signal computation: 7 signals in <10ms
  - Composite scoring: <1ms per score
  - Proposal validation (7 dimensions): <50ms per proposal
  - Simulation boundary scan: <5ms per scan
  - Monitor event ingestion: 10K events/sec target (Req 9 AC9)
  - Prompt compilation (10 layers): <100ms
  - Kill switch check: <1μs (atomic read)
  - Message signing + verification: <1ms per message
  - MerkleTree proof generation: <10ms for 10K leaves
- **Testing**: Benchmark results compared against targets. >10% regression on any benchmark fails CI (per A15).

---

### Task 8.3 — CI/CD Workflows + Project Root Config Files ✅
- **Design**: A15
- **Files**: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/workflows/security-audit.yml`, `.github/workflows/benchmark.yml`, `.github/CODEOWNERS`, `deny.toml`, `rustfmt.toml`, `clippy.toml`, `SECURITY.md`
- **What to build**:
  - ci.yml: cargo fmt --check, cargo clippy -- -D warnings, cargo test --workspace, cargo deny check, npm run lint
  - release.yml: tagged release, cross-compile (linux-x86_64, linux-aarch64, macos-x86_64, macos-aarch64), npm build, GitHub release
  - security-audit.yml: daily cargo audit + cargo deny
  - benchmark.yml: Criterion benchmarks on PR, fail on >10% regression
  - deny.toml: cargo-deny configuration — license allowlist (MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib), advisory database check, duplicate crate detection
  - rustfmt.toml: workspace formatting rules (edition=2021, max_width=100, use_field_init_shorthand=true)
  - clippy.toml: workspace lint rules (cognitive-complexity-threshold, too-many-arguments-threshold)
  - SECURITY.md: security policy, vulnerability disclosure process, supported versions, contact information
  - .github/CODEOWNERS: ownership mapping for safety-critical paths (crates/ghost-gateway/src/safety/ → safety team, crates/simulation-boundary/ → safety team, crates/convergence-monitor/ → safety team)
- **Testing**: CI pipeline runs successfully on a test PR.
  - Unit: deny.toml validates against cargo-deny schema
  - Unit: rustfmt.toml produces consistent formatting across workspace
  - Unit: SECURITY.md contains disclosure process and contact info

---

## Phase 9: Future (Deferred)

> These items are explicitly defined in FILE_MAPPING.md but intentionally deferred beyond v1.
> Placeholder crates or stubs may be created during Phase 5+ to define trait boundaries.

---

### Task 9.1 — ghost-mesh: ClawMesh Agent-to-Agent Payment Protocol (Placeholder) ✅
- **Source**: FILE_MAPPING.md §ghost-mesh, AGENT_ARCHITECTURE_v2.md §5
- **Crate**: `crates/ghost-mesh/` (NEW — placeholder only)
- **Files**: `Cargo.toml`, `src/lib.rs`, `src/types.rs`, `src/traits.rs`, `src/protocol.rs`
- **What to build**:
  - Placeholder crate with trait definitions and types ONLY — no implementation
  - MeshPayment, MeshInvoice, MeshSettlement type stubs
  - PaymentProtocol trait with send_payment, verify_payment, settle method signatures
  - Commented-out workspace member in root Cargo.toml: `# "crates/ghost-mesh",  # Phase 9 — uncomment when ClawMesh protocol is designed`
- **Conventions**: This crate exists solely to define the trait boundary so Phase 5 inter-agent messaging can reference payment types without implementing them. No runtime code.
- **Testing**: Compile-only — verify crate compiles with no implementation.

---

## Dependency Graph Summary

```
Phase 1 (Foundation)
  └─ Task 1.1 ghost-signing (leaf, no deps)
  └─ Task 1.2 cortex-core extensions (depends on existing cortex-core)
  └─ Task 1.3 cortex-storage migrations (depends on 1.2 for types)
  └─ Task 1.4 cortex-temporal hash chains (depends on 1.3 for tables)
  └─ Task 1.5 cortex-decay convergence factor (depends on 1.2 for types)

Phase 2 (Safety Core) — depends on Phase 1
  └─ Task 2.1 itp-protocol (depends on 1.2 for types)
  └─ Task 2.2 cortex-convergence signals (depends on 2.1 for ITP events)
  └─ Task 2.3 cortex-convergence scoring (depends on 2.2)
  └─ Task 2.4 cortex-validation D5-D7 (depends on 1.2 for Proposal types)
  └─ Task 2.5 simulation-boundary (depends on 2.4 for pattern library)

Phase 3 (Monitor + Policy) — depends on Phase 2
  └─ Task 3.1 convergence-monitor core (depends on 2.1, 2.2, 2.3)
  └─ Task 3.2 intervention state machine (depends on 3.1)
  └─ Task 3.3 monitor transports (depends on 3.1)
  └─ Task 3.4 ghost-policy (depends on 1.2 for TriggerEvent)
  └─ Task 3.5 read-only-pipeline (depends on 2.3 for filtering)
  └─ Task 3.6 cortex-crdt signing (depends on 1.1 for ghost-signing)

Phase 4 (Agent Runtime) — depends on Phase 3
  └─ Task 4.1 ghost-llm (independent within phase)
  └─ Task 4.2 ghost-identity (depends on 1.1 for ghost-signing)
  └─ Task 4.3 ghost-agent-loop core (depends on 4.1, 4.2, 3.4, 3.5, 2.5)
  └─ Task 4.4 prompt compiler (depends on 4.3)
  └─ Task 4.5 proposal extraction (depends on 4.3, 2.4)
  └─ Task 4.6 tool registry + output inspector (depends on 4.3)

Phase 5 (Gateway) — depends on Phase 4
  └─ Task 5.1 gateway bootstrap (depends on all Phase 4)
  └─ Task 5.2 kill switch (depends on 5.1)
  └─ Task 5.3 session management (depends on 5.1)
  └─ Task 5.4 API server (depends on 5.1, 5.2, 5.3)
  └─ Task 5.5 inter-agent messaging (depends on 5.1, 1.1)
  └─ Task 5.6 session compaction (depends on 5.3, 4.3)
  └─ Task 5.7 ghost-channels (independent within phase)
  └─ Task 5.8 ghost-skills (depends on 1.1 for signing)
  └─ Task 5.9 ghost-heartbeat (depends on 5.1)

Phase 6 (Ecosystem) — depends on Phase 5
  └─ Tasks 6.1-6.10 (various dependencies, mostly on Phase 5 gateway)

Phase 7 (Hardening) — depends on Phase 6
  └─ Tasks 7.1-7.5 (cross-cutting, all crates)

Phase 8 (Integration) — depends on Phase 7
  └─ Tasks 8.1-8.3 (end-to-end, all phases)

Phase 9 (Future — Deferred)
  └─ Task 9.1 ghost-mesh placeholder (no runtime deps, placeholder crate only)
```

