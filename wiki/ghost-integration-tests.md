# ghost-integration-tests

> The workspace-level integration test and benchmark crate — 27 integration test suites, 8 adversarial test suites, 1 property test suite, and 10 criterion benchmarks that validate cross-crate wiring, full lifecycle flows, safety-critical edge cases, and performance baselines across the entire GHOST platform.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 10 (Testing) |
| Type | Test-only crate (no library code) |
| Location | `crates/ghost-integration-tests/` |
| Workspace deps | 26 workspace crates (nearly all of them) |
| External deps | `criterion`, `proptest`, `tempfile`, `secrecy`, `tokio`, `blake3`, `chrono`, `uuid`, `serde_json`, `async-trait` |
| Integration suites | 27 test modules |
| Adversarial suites | 8 test modules |
| Property suites | 1 module (hash algorithm separation) |
| Benchmarks | 10 criterion benchmarks |
| Downstream consumers | None (test-only) |

---

## Why This Crate Exists

Individual crate tests verify that each crate works in isolation. But GHOST is a 37-crate system where crates interact in complex ways — the gateway creates an `AgentRunner` which calls `ghost-llm` which checks `ghost-policy` which reads convergence state published by the monitor. Integration tests verify these cross-crate interactions work correctly.

The crate exists as a separate workspace member (not tests in the root) because:
- It needs to depend on nearly every workspace crate simultaneously
- Criterion benchmarks require a `[[bench]]` target in `Cargo.toml`
- Test organization (integration / adversarial / property) benefits from a dedicated module tree
- `publish = false` ensures it's never accidentally published to crates.io

**No library code:** `src/lib.rs` is empty. All code lives in `tests/` and `benches/`.

---

## Test Organization

### Integration Tests (`tests/integration/`)

27 test modules organized by lifecycle flow:

**Phase 1-7 (Core platform):**

| Module | Focus | Cross-crate wiring |
|--------|-------|-------------------|
| `convergence_pipeline` | Signal computation → composite scoring → level mapping | `cortex-convergence` ↔ `cortex-core` |
| `convergence_decay_lifecycle` | Convergence score → decay factor → memory retention | `cortex-convergence` ↔ `cortex-decay` |
| `hash_chain_lifecycle` | Event → hash chain → Merkle tree → verification | `cortex-temporal` |
| `signing_lifecycle` | Keypair generation → signing → verification | `ghost-signing` |
| `simulation_boundary_lifecycle` | Input scan → mode enforcement → output reframing | `simulation-boundary` |
| `multiagent_consensus` | N-of-M consensus → shielded proposals | `cortex-multiagent` |
| `napi_bindings` | TypeScript binding correctness | `cortex-napi` |
| `observability_metrics` | Prometheus metric registration and emission | `cortex-observability` |
| `privacy_convergence` | Emotional content detection → privacy filtering | `cortex-privacy` |
| `retrieval_convergence` | 11-factor retrieval with convergence factor | `cortex-retrieval` |

**Phase 8 (Agent core + gateway):**

| Module | Focus | Cross-crate wiring |
|--------|-------|-------------------|
| `agent_turn_lifecycle` | Full agent turn: prompt → LLM → tools → response | `ghost-agent-loop` ↔ `ghost-llm` ↔ `ghost-policy` |
| `compaction_lifecycle` | Session compaction trigger → compress → flush | `ghost-gateway` session subsystem |
| `convergence_full_pipeline` | End-to-end: ITP event → signal → score → intervention | `cortex-convergence` ↔ `convergence-monitor` pipeline |
| `gateway_shutdown` | 7-step shutdown sequence verification | `ghost-gateway` shutdown |
| `gateway_state_machine` | 6-state FSM transition validation | `ghost-gateway` FSM |
| `safety_critical_edge_cases` | 50+ edge cases across kill switch, compaction, convergence, messaging, proposals | Multi-crate safety verification |
| `distributed_kill_gates` | Kill gate close → propagation → ack → consensus | `ghost-kill-gates` |
| `inter_agent_messaging` | Message creation → signing → verification → delivery | `ghost-gateway` messaging |
| `kill_switch_chain` | Trigger → classify → escalate → kill → persist → recover | `ghost-gateway` safety |
| `multi_agent_scenarios` | Multiple agents interacting via messaging and delegation | `ghost-gateway` + `ghost-agent-loop` |
| `proposal_lifecycle` | Proposal extraction → validation → routing → application | `ghost-agent-loop` + `cortex-validation` |
| `orchestrator_fix_verification` | Regression tests for orchestrator bug fixes | Various |

**Phase 15 (E2E):**

| Module | Focus | Cross-crate wiring |
|--------|-------|-------------------|
| `secrets_e2e` | Secret provider initialization and credential retrieval | `ghost-secrets` |
| `egress_e2e` | Egress policy application and violation detection | `ghost-egress` |
| `oauth_e2e` | OAuth PKCE flow: authorize → callback → token → refresh | `ghost-oauth` |
| `mesh_e2e` | A2A agent discovery → trust → delegation → completion | `ghost-mesh` |

### Adversarial Tests (`tests/adversarial/`)

8 test modules that simulate attacks and verify they're detected/blocked:

| Module | Attack vector | Expected outcome |
|--------|--------------|-----------------|
| `unicode_bypass` | Homoglyphs, fullwidth chars, combining diacriticals, mathematical italic | Simulation boundary detects and blocks |
| `proposal_adversarial` | D7 emulation in content, restricted type creation, critical importance escalation | Validation rejects |
| `kill_switch_race` | Concurrent trigger delivery, dedup under load, state consistency | Monotonicity preserved, no races |
| `compaction_under_load` | User messages mimicking compaction blocks, NaN spending caps, zero context windows | No panics, no bypasses |
| `credential_exfil_patterns` | Base64-encoded credentials, split credentials, known credential patterns | Output inspector detects and blocks |
| `convergence_manipulation` | NaN signals, infinity signals, zero weights, dual amplification | Scores clamped to [0.0, 1.0] |
| `kill_gate_adversarial` | Race conditions in distributed kill gate propagation | Consensus achieved correctly |
| `orchestrator_adversarial` | Orchestrator-level attack patterns | Detected and blocked |

### Property Tests (`tests/property/`)

| Module | Invariant | Method |
|--------|-----------|--------|
| `hash_algorithm_separation` | ITP uses SHA-256, hash chains use blake3, never confused | Cargo.toml inspection + output length verification |

### `safety_critical_edge_cases.rs` — The Crown Jewel

This is the most important test file in the entire platform. It contains 50+ tests that verify safety-critical edge cases across multiple crates:

- Kill switch monotonicity (can't downgrade without resume)
- Kill switch blocks unknown agents
- Compaction blocks can't be faked by user messages
- NaN spending cap bypass attempts
- Unicode bypass attempts (combining diacriticals, fullwidth, mathematical italic)
- Convergence score clamping (NaN, ±infinity, zero weights)
- Hash chain tamper detection (wrong genesis, tampered middle, duplicate hashes)
- Message replay detection
- Signature anomaly detection
- Gateway FSM terminal state enforcement
- Circuit breaker and damage counter monotonicity
- Output inspector credential detection (base64, split, known patterns)
- Proposal D7 emulation rejection
- Hash algorithm separation (SHA-256 vs blake3)

---

## Benchmarks (`benches/convergence_bench.rs`)

10 criterion benchmarks establishing performance baselines:

| Benchmark | What it measures | Why it matters |
|-----------|-----------------|---------------|
| `hash_chain_computation` | blake3 hash chain throughput | Hash chain is on the hot path for every event |
| `composite_scoring` | 8-signal composite score computation | Computed on every message (EveryMessage tier) |
| `simulation_boundary_scan` | Input/output scanning throughput | Runs on every user message and agent response |
| `signing` | Ed25519 sign + verify cycle | Every inter-agent message is signed |
| `merkle_tree` | Merkle tree construction and proof generation | Used for hash chain verification |
| `convergence_factor` | Decay factor computation | Called during every memory retrieval |
| `signal_computation` | Individual signal computation | 8 signals computed at various frequencies |
| `proposal_validation` | 7-dimension proposal validation | Every agent proposal goes through validation |
| `prompt_compilation` | 10-layer prompt assembly | Runs at the start of every agent turn |
| `kill_switch_check` | `PLATFORM_KILLED` atomic read | Checked on every agent loop iteration |

---

## Security Properties

1. **Adversarial tests are mandatory** — Every safety-critical feature has corresponding adversarial tests
2. **Edge case coverage** — NaN, infinity, zero, empty, and boundary values are tested explicitly
3. **Cross-crate invariants** — Hash algorithm separation, layer dependency rules, and FSM constraints are verified at the integration level
4. **Regression prevention** — `orchestrator_fix_verification` ensures past bugs don't recur

---

## File Map

| File | Purpose |
|------|---------|
| `src/lib.rs` | Empty (test-only crate) |
| `tests/integration.rs` | Integration test harness entry point |
| `tests/integration/mod.rs` | 27 integration test module declarations |
| `tests/adversarial.rs` | Adversarial test harness entry point |
| `tests/adversarial/mod.rs` | 8 adversarial test module declarations |
| `tests/property.rs` | Property test harness entry point |
| `tests/property/mod.rs` | 1 property test module declaration |
| `benches/convergence_bench.rs` | 10 criterion benchmarks |

---

## Common Questions

**Q: Why are adversarial tests separate from integration tests?**
Adversarial tests have a different intent: they simulate attacks, not normal usage. Separating them makes it clear which tests verify "does it work?" vs "can it be broken?". It also allows running adversarial tests independently during security reviews.

**Q: Why property tests here instead of in individual crates?**
The `hash_algorithm_separation` property test spans multiple crates (inspects `itp-protocol` and `cortex-temporal` Cargo.toml files). It can't live in either crate because it needs to verify a cross-crate invariant.

**Q: Why criterion benchmarks instead of `#[bench]`?**
Criterion provides statistical analysis (confidence intervals, regression detection), HTML reports, and comparison across runs. The built-in `#[bench]` only provides raw timing. For performance baselines that need to be tracked over time, criterion is the right tool.
