# cortex-temporal

> Hash chains and Merkle trees for tamper-evident event logs — the cryptographic integrity layer that makes every GHOST audit trail verifiable.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 1 (Cortex Foundation) |
| Type | Library |
| Location | `crates/cortex/cortex-temporal/` |
| Workspace deps | `cortex-core` |
| External deps | `blake3`, `chrono`, `serde`, `serde_json`, `thiserror`, `rusqlite` (optional, feature `sqlite`) |
| Modules | `hash_chain`, `anchoring` (`merkle`, `git_anchor`, `rfc3161`) |
| Public API | `compute_event_hash()`, `verify_chain()`, `verify_all_chains()`, `ChainEvent`, `ChainVerification`, `ChainError`, `MerkleTree`, `GitAnchor`, `AnchorRecord` |
| Hash algorithm | blake3 (NOT SHA-256 — see [[Cryptographic-Choices]]) |
| Test coverage | Unit tests, property-based tests (chain round-trip, tamper detection, Merkle proofs) |

---

## Why This Crate Exists

`cortex-storage` guarantees that records can't be modified through the application layer (append-only triggers). But what if someone modifies the SQLite file directly? That's where `cortex-temporal` comes in.

Every event in the system is linked to the previous event via a blake3 hash chain. If any event is modified, inserted, or deleted at the file level, the chain breaks and the tampering is detectable. The Merkle tree layer provides efficient inclusion proofs and periodic anchoring to external systems (git commits, RFC 3161 timestamps).

The separation between `cortex-storage` (stores hashes) and `cortex-temporal` (computes and verifies hashes) is deliberate — the storage layer doesn't need to know about hash algorithms, and the temporal layer doesn't need to know about SQLite.

---

## Hash Chain

### The Core Algorithm

```
event_hash = blake3(event_type || "|" || delta_json || "|" || actor_id
                     || "|" || recorded_at || "|" || previous_hash)
```

Each event's hash includes:
1. The event type (string)
2. The delta payload (JSON string)
3. The actor who created it (string)
4. The timestamp (string)
5. The previous event's hash (32 bytes)

The pipe separators prevent field boundary ambiguity (same rationale as `cortex-crdt`'s canonical bytes).

### Genesis Hash

```rust
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];
```

Every chain starts from the all-zeros genesis hash. The first event in any chain has `previous_hash = GENESIS_HASH`. This is a well-known constant — there's no secret involved.

### Why blake3, Not SHA-256

The GHOST platform uses two hash algorithms:
- **blake3** — For hash chains, Merkle trees, and content integrity (used here)
- **SHA-256** — For ITP privacy hashing only (in `itp-protocol`)

blake3 was chosen for hash chains because:
- 2-3x faster than SHA-256 on modern hardware
- Built-in parallelism (SIMD + multithreading for large inputs)
- 32-byte output (same as SHA-256)
- No length extension attacks (unlike SHA-256)
- Designed by the same team as BLAKE2 (widely trusted)

SHA-256 is used in `itp-protocol` for privacy hashing because it's the industry standard for content hashing in telemetry systems, and interoperability with external systems matters more than speed there.

### `ChainEvent` Structure

```rust
pub struct ChainEvent {
    pub event_type: String,
    pub delta_json: String,
    pub actor_id: String,
    pub recorded_at: String,
    pub event_hash: [u8; 32],
    pub previous_hash: [u8; 32],
}
```

Fixed-size `[u8; 32]` arrays for hashes, not `Vec<u8>`. This prevents accidental truncation or extension and makes equality comparison constant-time.

### Chain Verification

```rust
pub fn verify_chain(events: &[ChainEvent]) -> ChainVerification
```

The verification algorithm:
1. Check for duplicate `event_hash` values (detects copy-paste attacks)
2. For each event, verify `previous_hash` matches the previous event's `event_hash`
3. For each event, recompute the hash and verify it matches the stored `event_hash`

Returns a `ChainVerification` struct with:
- `total_events` — How many events were in the chain
- `verified_events` — How many passed verification before the first failure
- `is_valid` — Whether the entire chain is valid
- `error` — The specific error if invalid

Three error types:
| Error | Meaning |
|-------|---------|
| `BrokenLink { index }` | Event at `index` has a `previous_hash` that doesn't match the previous event's `event_hash`. Indicates an insertion, deletion, or reordering. |
| `HashMismatch { index }` | Event at `index` has an `event_hash` that doesn't match the recomputed hash. Indicates content modification. |
| `DuplicateHash { first, second }` | Two events have the same `event_hash`. Indicates a copy-paste or replay attack. |

**Empty chains are valid.** An empty event list returns `is_valid: true` with 0 events. This is the correct behavior — there's nothing to verify.

### SQLite Integration (Feature-Gated)

```rust
#[cfg(feature = "sqlite")]
pub fn verify_all_chains(conn: &Connection) -> Result<Vec<ChainVerification>, CortexError>
```

When the `sqlite` feature is enabled, `verify_all_chains` queries all distinct `memory_id` values from `memory_events`, loads each chain, and verifies it. Returns only broken chains (empty vec = all valid).

This is feature-gated because:
- Not all consumers need SQLite (the hash chain algorithm is useful standalone)
- Avoids pulling in `rusqlite` for crates that only need hash computation
- The `sqlite` feature is enabled in dev-dependencies for testing

---

## Merkle Trees

### Purpose

Hash chains provide sequential integrity (each event links to the previous). Merkle trees provide:
1. **Efficient inclusion proofs** — Prove that a specific event is part of the chain without revealing the entire chain (O(log n) proof size)
2. **Anchoring** — The Merkle root is a single 32-byte value that summarizes the entire chain. This root can be anchored to external systems for independent verification.

### Construction

```rust
impl MerkleTree {
    pub fn from_chain(chain_hashes: &[[u8; 32]]) -> Self
}
```

Standard binary Merkle tree construction:
1. Leaves are the `event_hash` values from the chain
2. If the number of leaves is odd, the last leaf is duplicated (standard padding)
3. Each pair of nodes is hashed together: `blake3(left || right)`
4. Repeat until a single root remains

**Internal node storage:** The tree stores all levels (`nodes: Vec<Vec<[u8; 32]>>`) for proof generation. This trades memory for proof generation speed — you don't need to rebuild the tree to generate a proof.

### Inclusion Proofs

```rust
pub fn inclusion_proof(&self, leaf_index: usize) -> Vec<[u8; 32]>
pub fn verify_proof(root: &[u8; 32], leaf: &[u8; 32], proof: &[[u8; 32]], leaf_index: usize) -> bool
```

An inclusion proof is a list of sibling hashes along the path from the leaf to the root. To verify:
1. Start with the leaf hash
2. For each sibling in the proof, hash the pair (order determined by the leaf index's bit at each level)
3. The final hash should equal the root

Proof size is O(log₂ n) — for 1000 events, the proof is ~10 hashes (320 bytes).

`verify_proof` is a static method — it doesn't need the full tree, just the root, leaf, proof, and index. This means proofs can be verified by anyone with the root hash, without access to the full tree or database.

### Anchoring Schedule

The source comments note: "Triggered every 1000 events or 24 hours (AC9)." The Merkle tree is rebuilt and anchored periodically, not on every event. This balances integrity guarantees with performance.

---

## Anchoring Backends

### Git Anchor (Stub)

```rust
pub struct GitAnchor;
```

Anchors a Merkle root to a git commit. Currently a Phase 1 stub — the `anchor()` method returns a placeholder `AnchorRecord` with `git_commit_hash: None`. Full implementation planned for Phase 3.

The idea: commit the Merkle root to a git repository. Anyone with access to the repo can verify that the hash chain existed at the time of the commit. Git's own hash chain (commit history) provides an independent tamper-evident timeline.

### RFC 3161 Anchor (Stub)

```rust
pub struct RFC3161Anchor;
```

RFC 3161 defines a Trusted Timestamp Authority (TSA) protocol. The anchor would submit the Merkle root to a TSA and receive a signed timestamp proving the root existed at a specific time. Currently returns `Err("RFC 3161 anchoring not yet implemented")`.

---

## Security Properties

### Tamper Detection Guarantees

| Attack | Detection |
|--------|-----------|
| Modify event content | `HashMismatch` — recomputed hash doesn't match stored hash |
| Delete an event | `BrokenLink` — next event's `previous_hash` doesn't match |
| Insert an event | `BrokenLink` — chain continuity broken |
| Reorder events | `BrokenLink` — `previous_hash` links broken |
| Copy-paste an event | `DuplicateHash` — same `event_hash` appears twice |
| Modify the SQLite file directly | All of the above — the chain is verified from the data, not from SQLite metadata |

### What This Does NOT Protect Against

- **Truncation from the end** — If the last N events are deleted, the remaining chain is still valid. This is detectable via Merkle anchoring (the anchored root won't match the truncated chain).
- **Complete chain replacement** — If the entire chain is replaced with a new valid chain, the hash chain alone can't detect it. Merkle anchoring to external systems (git, RFC 3161) prevents this.

---

## Test Strategy

### Unit Tests (`tests/hash_chain_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `genesis_hash_is_all_zeros` | Constant correctness |
| `single_event_chain_verifies` | Minimal valid chain |
| `empty_chain_is_valid` | Empty = valid |
| `compute_event_hash_is_deterministic` | Same inputs = same hash |
| `different_event_type_produces_different_hash` | Hash sensitivity |
| `valid_chain_of_100_verifies` | Longer chain integrity |
| `tampered_event_hash_detected` | Content modification detected |
| `tampered_previous_hash_detected` | Link modification detected |
| `duplicate_event_hash_detected` | Copy-paste attack detected |
| `merkle_single_leaf_root_equals_leaf` | Degenerate tree |
| `merkle_two_leaves_inclusion_proof` | Minimal proof |
| `merkle_1000_leaves_random_proof` | Large tree, multiple proof indices |
| `merkle_wrong_root_returns_false` | Invalid root rejected |
| `merkle_wrong_leaf_returns_false` | Invalid leaf rejected |
| `merkle_empty_chain` | Empty tree = zero root |

### Property Tests (inline `proptests` module)

| Property | Invariant |
|----------|-----------|
| `chain_round_trip` | ∀ len ∈ [1, 200]: make_chain(len) → verify_chain = valid |
| `tamper_single_byte_detected` | ∀ chain, ∀ tamper position: modify delta → verify = invalid |
| `tamper_previous_hash_detected` | ∀ chain, ∀ position > 0: flip previous_hash byte → verify = invalid |
| `merkle_inclusion_proof_round_trip` | ∀ len ∈ [1, 200]: first and last leaf proofs verify |

---

## File Map

```
crates/cortex/cortex-temporal/
├── Cargo.toml                          # blake3, optional rusqlite (feature: sqlite)
├── src/
│   ├── lib.rs                          # Module declarations, blake3 note
│   ├── hash_chain.rs                   # ChainEvent, compute_event_hash, verify_chain, verify_all_chains
│   └── anchoring/
│       ├── mod.rs                      # Module declarations
│       ├── merkle.rs                   # MerkleTree: from_chain, inclusion_proof, verify_proof
│       ├── git_anchor.rs              # GitAnchor stub (Phase 1)
│       └── rfc3161.rs                 # RFC3161Anchor stub
└── tests/
    └── hash_chain_tests.rs            # Unit tests + property tests
```
