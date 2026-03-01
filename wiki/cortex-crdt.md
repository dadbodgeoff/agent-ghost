# cortex-crdt

> CRDT primitives with Ed25519 signed deltas and sybil resistance — the distributed data foundation for multi-agent memory.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 1 (Cortex Foundation) |
| Type | Library |
| Location | `crates/cortex/cortex-crdt/` |
| Workspace deps | `cortex-core` |
| External deps | `ed25519-dalek` 2.x (direct, NOT via `ghost-signing`), `rand`, `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, `tracing` |
| Modules | `signing` (`signed_delta`, `key_registry`), `sybil` |
| Public API | `SignedDelta<T>`, `sign_delta()`, `verify_delta()`, `KeyRegistry`, `SybilGuard`, `AgentTrust`, `SybilError` |
| Test coverage | Unit tests, property-based tests (500 cases × 3 properties), layer separation audit |

---

## Why This Crate Exists

In a multi-agent system, agents can propose changes to shared memory. Without cryptographic guarantees, a compromised agent could:
- Forge memory deltas pretending to be another agent
- Flood the system with fake identities (sybil attack) to overwhelm consensus
- Tamper with deltas in transit between nodes

`cortex-crdt` solves all three problems:
1. **Signed deltas** — Every memory delta is wrapped in an Ed25519 signature. No valid signature = delta rejected.
2. **Key registry** — Maps agent UUIDs to their public keys. Only registered agents can have their deltas verified.
3. **Sybil guard** — Rate-limits agent creation (max 3 per parent per 24h) and caps trust for new agents.

---

## The ed25519-dalek Decision

This is the most architecturally significant decision in this crate. `cortex-crdt` uses `ed25519-dalek` **directly** rather than depending on `ghost-signing`.

### Why Not Use ghost-signing?

The full reasoning is documented in the [[ghost-signing]] wiki page, but the summary:

1. **Different wrapper semantics.** `ghost-signing` wraps agent-level messages. `cortex-crdt` wraps memory deltas. The types are structurally similar but semantically distinct.

2. **Cortex subsystem independence.** The `cortex-*` crates form a self-contained subsystem. Adding a `ghost-*` dependency at the foundation layer would break this boundary.

3. **CI-enforced invariant.** Three separate test files verify that `cortex-crdt`'s `Cargo.toml` does not contain `ghost-signing`:
   - `cortex-crdt/tests/signing_tests.rs`
   - `cortex-test-fixtures/tests/hash_algorithm_separation.rs`
   - `ghost-integration-tests/tests/property/hash_algorithm_separation.rs`

---

## Module Breakdown

### `signing/signed_delta.rs` — The Core Primitive

#### `SignedDelta<T>`

```rust
pub struct SignedDelta<T: Serialize> {
    pub delta: T,
    pub author: Uuid,
    pub signature: ed25519_dalek::Signature,
    pub timestamp: DateTime<Utc>,
}
```

Generic over any `Serialize` type. This means you can sign any struct as a delta — `MemoryDelta`, `GoalDelta`, `ConfigDelta`, etc. The only requirement is that the delta type implements `Serialize`.

**Key design decisions:**

1. **Generic, not concrete.** The crate doesn't define what a "delta" is — that's the caller's responsibility. This keeps `cortex-crdt` reusable across different CRDT implementations.

2. **Author is a UUID, not a public key.** The `author` field is the agent's UUID, not their public key. The public key is looked up via the `KeyRegistry` at verification time. This keeps the delta compact and avoids embedding 32-byte keys in every delta.

3. **Timestamp is part of the signed payload.** The timestamp is included in the canonical bytes that get signed. This means you can't change the timestamp without invalidating the signature — important for preventing backdating attacks.

#### Canonical Byte Format

```
delta_json || "|" || author_uuid_bytes || "|" || rfc3339_timestamp
```

The canonical bytes are constructed by concatenating:
1. The delta serialized as JSON (`serde_json::to_vec`)
2. A pipe separator
3. The author UUID as raw bytes (16 bytes)
4. A pipe separator
5. The timestamp in RFC 3339 format

**Why pipe separators?** To prevent ambiguity. Without separators, `delta_json + author_bytes` could be parsed differently if the JSON happens to end with bytes that look like a UUID prefix.

**Determinism warning:** The source code contains an explicit warning:

> Callers MUST NOT use `HashMap`-based delta types as the JSON key ordering would be non-deterministic. Use `BTreeMap` if map-like deltas are needed.

This is critical. If the same delta serializes to different JSON on different runs (due to `HashMap` random iteration order), the signature will be different, and verification will fail. `BTreeMap` guarantees deterministic key ordering.

**Error handling in canonical bytes:** If `serde_json::to_vec` fails (which should never happen for a well-formed struct), the code logs a `CRITICAL` error via `tracing` and falls back to an empty payload. This is a defensive choice — it's better to produce a verifiable (but wrong) signature than to panic in a production system.

#### Custom Serde for Signatures

The `signature_serde` module provides custom serialization for `ed25519_dalek::Signature` because the default serde implementation may not match the desired wire format. The custom implementation:
- Serializes as raw bytes (64 bytes)
- Deserializes from either a byte slice or a byte sequence (handles both binary and JSON array formats)
- Validates length (exactly 64 bytes)

#### `sign_delta()` and `verify_delta()`

```rust
pub fn sign_delta<T: Serialize>(
    delta: T, author: Uuid, key: &ed25519_dalek::SigningKey,
) -> SignedDelta<T>

pub fn verify_delta<T: Serialize>(
    signed: &SignedDelta<T>, key: &ed25519_dalek::VerifyingKey,
) -> bool
```

Both are free functions, not methods. This keeps the API simple and avoids the need for a signing context object.

`sign_delta` captures `Utc::now()` as the timestamp. This means the timestamp is set at signing time, not at delta creation time. If there's a delay between creating the delta and signing it, the timestamp reflects when it was signed.

`verify_delta` returns `bool` (same rationale as `ghost-signing::verify` — see [[ghost-signing#verifierrs--verification]]).

**Replay attack note:** The signing layer does NOT prevent replay attacks. The same signed delta can be verified multiple times. Replay detection is the caller's responsibility, typically via hash chain deduplication in `cortex-temporal`. This is explicitly documented in the test suite:

> "Both verifications succeed — replay detection is the caller's responsibility (via hash chain dedup), not the signing layer's."

---

### `signing/key_registry.rs` — Agent Key Management

```rust
pub struct KeyRegistry {
    keys: BTreeMap<Uuid, ed25519_dalek::VerifyingKey>,
}
```

A simple in-memory map from agent UUID to public key. Operations: `register`, `get`, `remove`, `len`, `is_empty`.

**Why `BTreeMap` instead of `HashMap`?** Deterministic iteration order. While the registry itself doesn't need deterministic iteration, using `BTreeMap` consistently across the codebase prevents accidental non-determinism if the registry is ever serialized.

**Dual registration pattern:** The source comments note: "Populated from ghost-identity key files during bootstrap. Dual registration: same public key registered in both MessageDispatcher and cortex-crdt KeyRegistry." This means each agent's public key exists in two places:
1. The `KeyRegistry` (for CRDT delta verification)
2. The `MessageDispatcher` in `ghost-gateway` (for inter-agent message verification)

Both registries are populated from the same source (`ghost-identity` key files) during gateway bootstrap.

---

### `sybil.rs` — Sybil Resistance

The sybil module prevents a single entity from creating many fake agents to overwhelm the CRDT consensus system.

#### Three Rules

| Rule | Value | Purpose |
|------|-------|---------|
| Max children per parent per 24h | 3 | Prevents rapid agent spawning |
| Initial trust for new agents | 0.3 | New agents start with low influence |
| Trust cap for agents < 7 days old | 0.6 | Even if trust is manually elevated, young agents are capped |

#### `SybilGuard`

```rust
pub struct SybilGuard {
    max_children_per_day: usize,
    initial_trust: f64,
    young_agent_cap: f64,
    spawn_records: BTreeMap<Uuid, Vec<(Uuid, DateTime<Utc>)>>,
    trust_levels: BTreeMap<Uuid, AgentTrust>,
}
```

**Spawn record pruning:** When `register_spawn` is called, it first prunes records older than 24 hours from the parent's spawn list. This means the 24-hour window is a sliding window, not a fixed calendar day.

**Effective trust calculation:**
```rust
pub fn effective_trust(&self) -> f64 {
    let age = Utc::now() - self.created_at;
    if age < Duration::days(7) {
        self.trust.min(0.6)
    } else {
        self.trust
    }
}
```

The `min(0.6)` cap means that even if you call `set_trust(agent, 1.0)`, a young agent's effective trust is still 0.6. This prevents a compromised parent from immediately granting full trust to a newly spawned child.

**Unknown agents get trust 0.0:** If you query `effective_trust` for an agent that was never registered, you get 0.0. This is a safe default — unknown agents have zero influence.

**Boundary behavior at exactly 7 days:** The test suite explicitly verifies that an agent at exactly 7 days old is NOT capped. The condition is `age < Duration::days(7)`, so `age == 7 days` passes the check and the cap doesn't apply.

---

## Security Properties

### Tamper Detection

The canonical byte format includes the delta content, author UUID, and timestamp. Modifying any of these fields invalidates the signature. The test suite verifies:
- Tampered delta content → verification fails
- Tampered author UUID → verification fails
- Wrong signing key → verification fails

### Sybil Resistance

The three-rule system creates a trust gradient:
- Day 0: Trust 0.3, capped at 0.6
- Day 1–6: Trust can grow but capped at 0.6
- Day 7+: Trust cap removed, full trust possible

This means a sybil attacker who creates 3 agents per day for a week gets 21 agents, each with at most 0.6 trust. A legitimate agent that's been running for months has uncapped trust. The trust differential makes sybil attacks economically expensive.

### Replay Prevention (Not Provided)

Replay detection is explicitly NOT the responsibility of this crate. It's handled by `cortex-temporal`'s hash chains, which detect duplicate entries.

---

## Test Strategy

### Signing Tests (`tests/signing_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `valid_signed_delta_verifies` | Happy path: sign → verify = true |
| `delta_with_wrong_key_rejected` | Cross-key rejection |
| `tampered_delta_rejected` | Content tampering detected |
| `tampered_author_rejected` | Author UUID tampering detected |
| `key_registry_register_and_lookup` | Registry CRUD operations |
| `key_registry_remove` | Key removal |
| `key_registry_unknown_agent_returns_none` | Missing key handling |
| `sign_verify_via_registry` | End-to-end: sign → registry lookup → verify |
| `signing_preserves_delta_content` | Delta content unchanged after signing |
| `replay_attack_same_delta_submitted_twice` | Documents that replay detection is caller's job |
| `cortex_crdt_cargo_toml_does_not_depend_on_ghost_signing` | Layer separation audit |
| `signed_delta_uses_ed25519_dalek_directly` | Architectural constraint verification |

### Sybil Tests (`tests/sybil_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `three_spawns_in_24h_all_succeed` | 3 children allowed |
| `fourth_spawn_in_24h_rejected` | 4th child rejected with correct error |
| `new_agent_trust_is_0_3` | Initial trust = 0.3 |
| `young_agent_trust_capped_at_0_6` | Trust cap for < 7 day agents |
| `old_agent_trust_not_capped` | No cap for ≥ 7 day agents |
| `spawn_at_23h59m_still_rejected` | Boundary: 23h59m still within window |
| `spawn_after_24h_window_succeeds` | Window expiry: 24h01m allows new spawns |
| `different_parents_have_independent_limits` | Per-parent isolation |
| `unknown_agent_effective_trust_is_zero` | Safe default for unknown agents |
| `agent_at_exactly_7_days_is_not_capped` | Boundary: exactly 7 days = uncapped |

### Property Tests (`tests/property_tests.rs`)

500 cases per property:

| Property | Invariant |
|----------|-----------|
| `sign_verify_round_trip` | ∀ delta: sign → verify = true |
| `tamper_detection` | ∀ original ≠ tampered: modify value → verify = false |
| `cross_key_always_fails` | ∀ delta: sign with A → verify with B = false |

---

## File Map

```
crates/cortex/cortex-crdt/
├── Cargo.toml                          # ed25519-dalek direct dep, NO ghost-signing
├── src/
│   ├── lib.rs                          # Architectural constraint documented
│   ├── signing/
│   │   ├── mod.rs                      # Re-exports
│   │   ├── signed_delta.rs             # SignedDelta<T>, sign_delta(), verify_delta()
│   │   └── key_registry.rs             # KeyRegistry (UUID → VerifyingKey)
│   └── sybil.rs                        # SybilGuard, AgentTrust, spawn limits
└── tests/
    ├── signing_tests.rs                # Unit tests + layer separation audit
    ├── sybil_tests.rs                  # Spawn limits, trust caps, boundary tests
    └── property_tests.rs              # 3 properties × 500 cases
```
