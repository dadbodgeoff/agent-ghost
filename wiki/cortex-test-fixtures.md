# cortex-test-fixtures

> Shared proptest strategies, golden dataset loaders, and test helpers consumed by every crate's property tests — 25+ proptest strategies covering all major domain types, deterministic golden trajectories for baseline verification, and hash chain construction helpers.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 10 (Testing) |
| Type | Library (test support) |
| Location | `crates/cortex/test-fixtures/` |
| Workspace deps | `cortex-core`, `cortex-temporal`, `itp-protocol`, `ghost-signing`, `ghost-egress`, `ghost-oauth`, `ghost-mesh`, `ghost-llm`, `ghost-agent-loop` |
| External deps | `proptest`, `serde`, `serde_json`, `chrono`, `uuid`, `blake3`, `ed25519-dalek`, `rand`, `secrecy` |
| Modules | `strategies`, `fixtures`, `helpers` |
| Strategy count | 25+ proptest strategies |
| Golden datasets | 3 (normal trajectory, escalating trajectory, intervention sequence) |
| Test suites | `correctness_properties.rs` (10 properties × 1000 cases), `hash_algorithm_separation.rs`, `post_v1_strategy_tests.rs` (11 strategies × 1000 cases) |
| Downstream consumers | Every crate with property tests |

---

## Why This Crate Exists

Property-based testing with proptest requires strategies that generate valid instances of domain types. Without a shared crate, every crate would need to duplicate these strategies — and worse, they'd drift out of sync when types change.

`cortex-test-fixtures` centralizes all proptest strategies so that:
- A type change in `cortex-core` only requires updating one strategy, not 15
- All property tests use the same generation logic, ensuring consistent coverage
- Golden datasets provide deterministic baselines that don't change between runs
- Helper functions (hash chain builders, assertion utilities) are shared

**Why it depends on 9 workspace crates:** Each strategy needs to construct valid instances of types from those crates. The `agent_card_strategy()` needs `ghost-mesh` types, `proposal_strategy()` needs `cortex-core` types, `token_set_strategy()` needs `ghost-oauth` types, etc.

---

## Module Breakdown

### `strategies.rs` — 25+ Proptest Strategies

Every major domain type in GHOST has a corresponding proptest strategy:

**Core types:**

| Strategy | Generates | Constraints |
|----------|-----------|------------|
| `memory_type_strategy()` | All 31 `MemoryType` variants | Uniform selection |
| `importance_strategy()` | `Importance` enum | All variants |
| `convergence_score_strategy()` | `f64` in [0.0, 1.0] | Clamped range |
| `signal_array_strategy()` | `[f64; 8]` | Each element in [0.0, 1.0] |
| `signal_array_8_strategy()` | `[f64; 8]` | Alias for signal_array_strategy |
| `base_memory_strategy()` | `BaseMemory` struct | Valid fields |

**Temporal types:**

| Strategy | Generates | Constraints |
|----------|-----------|------------|
| `event_chain_strategy(min, max)` | `Vec<ChainEvent>` of length min..max | Valid hash chain (each event's hash chains from previous) |
| `convergence_trajectory_strategy()` | `Vec<f64>` trajectory | Values in [0.0, 1.0] |

**Safety types:**

| Strategy | Generates | Constraints |
|----------|-----------|------------|
| `trigger_event_strategy()` | All `TriggerEvent` variants | Valid UUIDs, timestamps, reasons |
| `kill_state_strategy()` | `(u8, BTreeMap<String, u8>)` | Platform level + per-agent levels |
| `gateway_state_transition_strategy()` | `Vec<u8>` state sequence | Valid FSM transitions |

**Proposal types:**

| Strategy | Generates | Constraints |
|----------|-----------|------------|
| `proposal_strategy()` | `Proposal` struct | Valid target_type, content, caller |
| `caller_type_strategy()` | `CallerType` enum | All variants |
| `proposal_operation_strategy()` | `ProposalOperation` enum | All variants |

**Session types:**

| Strategy | Generates | Constraints |
|----------|-----------|------------|
| `session_history_strategy(min, max)` | `Vec<String>` message history | Length min..max |

**Network types (Phase 15):**

| Strategy | Generates | Constraints |
|----------|-----------|------------|
| `egress_config_strategy()` | `AgentEgressConfig` | Valid policy mode, domains |
| `domain_pattern_strategy()` | Domain string | Non-empty, valid format |
| `oauth_ref_id_strategy()` | `OAuthRefId` | Valid UUID-based ref |
| `token_set_strategy()` | `TokenSet` with `SecretString` | Valid scopes, expiry |
| `agent_card_strategy()` | `AgentCard` with valid signature | Ed25519 signed |
| `mesh_task_strategy()` | `MeshTask` | Valid task structure |
| `interaction_outcome_strategy()` | `InteractionOutcome` | Valid outcome |
| `trust_matrix_strategy()` | `BTreeMap<(Uuid, Uuid), f64>` | No self-trust, values in [0.0, 1.0] |

**Agent loop types:**

| Strategy | Generates | Constraints |
|----------|-----------|------------|
| `tool_call_plan_strategy()` | `ToolCallPlan` with 0-8 calls | Valid tool names |
| `spotlighting_config_strategy()` | `SpotlightingConfig` | Valid config |

**Design decision:** The `event_chain_strategy()` is the most complex strategy. It generates a valid hash chain where each event's hash is computed from the previous event's hash. This ensures property tests that consume hash chains always start with structurally valid data — the tests then tamper with it to verify detection.

### `fixtures.rs` — Golden Datasets

Three deterministic datasets for baseline verification:

**`normal_trajectory()`** — 20 convergence scores representing a healthy agent. Values oscillate between 0.05 and 0.12, never approaching intervention thresholds. Used to verify that normal behavior doesn't trigger false positives.

**`escalating_trajectory()`** — 20 scores showing steady escalation from 0.10 to 0.95. Used to verify that the intervention state machine escalates correctly through all 5 levels.

**`intervention_sequence()`** — 6 (score, level) pairs mapping scores to expected intervention levels. Used as a truth table for score-to-level mapping.

**`minimal_config()`** — A minimal valid `ghost.yml` as a JSON `Value`. Used by config parsing tests.

### `helpers.rs` — Test Utilities

**`build_chain(events)`** — Constructs a valid hash chain from raw event tuples `(event_type, delta_json, actor_id, recorded_at)`. Handles genesis hash and chaining automatically. This is the most-used helper — every hash chain test starts with `build_chain()`.

**`assert_unit_range(value, label)`** — Asserts a value is in [0.0, 1.0]. Used extensively in convergence score tests.

**`assert_factor_monotonic(value, label)`** — Asserts a value is >= 1.0. Used in decay factor tests to verify the monotonicity invariant.

---

## Test Suites

### `correctness_properties.rs` — 10 Core Properties × 1000 Cases

The most important property test file in the platform. Each property runs 1000 cases with proptest shrinking:

| Property | Invariant | Crates tested |
|----------|-----------|--------------|
| Signal range | All 8 signals in [0.0, 1.0] | `cortex-convergence` |
| Convergence bounds | Composite score in [0.0, 1.0] | `cortex-convergence` |
| Decay monotonicity | Factor >= 1.0 for all types and scores | `cortex-decay` |
| Tamper detection | Any byte modification → chain verification fails | `cortex-temporal` |
| Hash chain round-trip | Valid chain always verifies | `cortex-temporal` |
| Amplified score bounded | Score with meso/macro amplification still in [0.0, 1.0] | `cortex-convergence` |
| Decay factor monotonic in score | Higher score → higher or equal factor | `cortex-decay` |
| Hash deterministic | Same inputs → same hash | `cortex-temporal` |
| Different inputs → different hash | Different event_type → different hash | `cortex-temporal` |
| Proposal serde round-trip | Serialize → deserialize = identity | `cortex-core` |
| Trigger event serde round-trip | Serialize → deserialize = identity | `cortex-core` |

### `post_v1_strategy_tests.rs` — 11 Phase 15 Strategies × 1000 Cases

Validates that all Phase 15 strategies produce valid instances:
- No panics on 1000 samples
- Serialization round-trips succeed
- Domain invariants hold (no self-trust in trust matrices, signals in [0,1], agent cards have valid signatures)

### `hash_algorithm_separation.rs` — Dependency Verification

Verifies the hash algorithm separation invariant by inspecting Cargo.toml files:
- `itp-protocol` depends on `sha2`, NOT `blake3`
- `cortex-temporal` depends on `blake3`, NOT `sha2`
- ITP content hashes are 64 hex chars (SHA-256)
- Hash chain outputs are 32 bytes (blake3)

---

## Security Properties

1. **Strategy correctness** — Every strategy produces values within documented constraints (1000 cases each)
2. **Tamper detection** — Property tests verify that any single-byte modification to a hash chain is detected
3. **Monotonicity** — Decay factors are verified to be monotonically non-decreasing with convergence score
4. **Bounds enforcement** — Convergence scores are verified to stay in [0.0, 1.0] even with amplification
5. **Serde round-trip** — All serializable types survive JSON round-trips without data loss

---

## File Map

| File | Lines | Purpose |
|------|-------|---------|
| `src/lib.rs` | ~10 | Module declarations |
| `src/strategies.rs` | ~580 | 25+ proptest strategies for all domain types |
| `src/fixtures.rs` | ~45 | Golden datasets (trajectories, intervention sequence, config) |
| `src/helpers.rs` | ~45 | Hash chain builder, assertion utilities |
| `tests/correctness_properties.rs` | ~190 | 10 core correctness properties × 1000 cases |
| `tests/post_v1_strategy_tests.rs` | ~120 | 11 Phase 15 strategy validation tests |
| `tests/hash_algorithm_separation.rs` | ~65 | Hash algorithm dependency verification |

---

## Common Questions

**Q: Why does this crate depend on 9 workspace crates?**
Each strategy needs to construct valid instances of types defined in those crates. The `agent_card_strategy()` creates `AgentCard` from `ghost-mesh`, signs it with `ghost-signing`, etc. The alternative — duplicating type definitions — would be worse.

**Q: Why 1000 cases per property?**
1000 is the sweet spot between coverage and CI time. Fewer cases miss edge cases; more cases slow down the test suite. proptest's shrinking ensures that when a failure is found, it's reduced to the minimal reproducing case.

**Q: Why golden datasets instead of just proptest?**
Proptest generates random data — great for finding unexpected failures, but not for verifying specific known-good scenarios. Golden datasets provide deterministic baselines: "this exact trajectory should produce these exact intervention levels." They complement proptest, not replace it.

**Q: Why is `event_chain_strategy()` parameterized with min/max length?**
Different tests need different chain lengths. Tamper detection tests need chains of at least 2 events (to have something to tamper with). Round-trip tests can use longer chains. The parameterization avoids creating multiple near-identical strategies.
