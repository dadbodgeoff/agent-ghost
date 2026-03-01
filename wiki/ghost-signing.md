# ghost-signing

> Ed25519 signing primitives — the cryptographic bedrock of the entire GHOST platform.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 0 (Leaf) |
| Type | Library |
| Location | `crates/ghost-signing/` |
| Workspace deps | **None** — zero `ghost-*` or `cortex-*` dependencies |
| External deps | `ed25519-dalek` 2.x, `zeroize` 1.x, `rand` 0.8, `serde` 1.x |
| Modules | `keypair`, `signer`, `verifier` |
| Public API | `generate_keypair()`, `sign()`, `verify()`, `SigningKey`, `VerifyingKey`, `Signature` |
| Test coverage | Unit tests, property-based tests (1000 cases per property), leaf-crate audit |
| Downstream consumers | `ghost-identity`, `ghost-mesh`, `ghost-skills`, `ghost-kill-gates`, `ghost-gateway`, `ghost-integration-tests`, `cortex-test-fixtures` |

---

## Why This Crate Exists

Every security-critical operation in GHOST traces back to Ed25519 signatures:

- **Agent identity** — Each agent has an Ed25519 keypair managed by `ghost-identity`. The public key IS the agent's identity.
- **Inter-agent communication** — All messages in the `ghost-mesh` network are signed. A message without a valid signature is dropped.
- **Kill gate audit chains** — `ghost-kill-gates` signs every gate state transition. The hash chain is cryptographically bound to the signing key.
- **Skill manifests** — `ghost-skills` verifies skill package signatures before loading them into the WASM sandbox.
- **CRDT deltas** — While `cortex-crdt` uses `ed25519-dalek` directly (see [Architectural Decision: Why cortex-crdt Doesn't Use ghost-signing](#architectural-decision-why-cortex-crdt-doesnt-use-ghost-signing)), the signing algorithm is the same.

`ghost-signing` exists to provide a single, auditable, wrapper around `ed25519-dalek` that enforces:
1. Zeroize-on-drop for all private key material
2. A minimal, hard-to-misuse API surface
3. Serde support for key/signature serialization
4. A leaf-crate guarantee (no transitive dependency bloat)

---

## Module Breakdown

### `keypair.rs` — Key Generation and Lifecycle

This module defines the two core types and the keypair generation function.

#### `SigningKey` (Private Key Wrapper)

```rust
pub struct SigningKey {
    inner: ed25519_dalek::SigningKey,
}
```

**Key design decisions:**

1. **Newtype wrapper, not a type alias.** The `SigningKey` is a newtype around `ed25519_dalek::SigningKey`. This prevents downstream crates from accidentally using `ed25519-dalek` APIs directly, which could bypass the zeroize guarantees or create incompatible key formats.

2. **`pub(crate)` inner access.** The `inner()` method is `pub(crate)`, meaning only `signer.rs` and `verifier.rs` within this crate can access the raw `ed25519-dalek` key. No downstream consumer can reach into the wrapper.

3. **No `Clone` or `Copy`.** `SigningKey` deliberately does not implement `Clone` or `Copy`. Private keys should have a single owner. If you need the key in two places, you need to pass a reference — this makes key lifecycle explicit and auditable.

4. **No `Serialize`/`Deserialize`.** The signing key is intentionally not serializable. You cannot accidentally serialize a private key to JSON, a log file, or a network response. The only way to persist a signing key is through `ghost-secrets` (which encrypts at rest) or `ghost-identity` (which manages the lifecycle).

5. **Debug redaction.** The `Debug` impl shows only the public key, never the private key material:
   ```rust
   impl std::fmt::Debug for SigningKey {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           f.debug_struct("SigningKey")
               .field("public", &self.inner.verifying_key())
               .finish_non_exhaustive()
       }
   }
   ```
   This means `println!("{:?}", signing_key)` will show `SigningKey { public: VerifyingKey(...), .. }` — never the secret bytes. This is critical for log safety.

6. **Zeroize-on-drop via delegation.** There is no manual `Drop` impl on the wrapper. Instead, the crate relies on `ed25519-dalek`'s own `ZeroizeOnDrop` implementation (enabled by the `zeroize` feature flag). When the wrapper is dropped, Rust drops the `inner` field, which triggers `ed25519_dalek::SigningKey::drop()`, which overwrites the 32-byte secret seed with zeros. This is documented explicitly in the source comments to prevent future contributors from adding a redundant `Drop` impl.

#### `VerifyingKey` (Public Key Wrapper)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyingKey {
    inner: ed25519_dalek::VerifyingKey,
}
```

**Contrast with `SigningKey`:**

| Trait | `SigningKey` | `VerifyingKey` | Rationale |
|-------|-------------|----------------|-----------|
| `Clone` | ❌ | ✅ | Public keys are freely shareable |
| `Copy` | ❌ | ❌ | 32 bytes — Clone is fine, implicit Copy could hide allocations |
| `Serialize` | ❌ | ✅ | Public keys need to be stored, transmitted, embedded in AgentCards |
| `Deserialize` | ❌ | ✅ | Need to reconstruct from JSON, TOML, network payloads |
| `Debug` | Redacted | Full | No secret material to protect |
| `PartialEq`/`Eq` | ❌ | ✅ | Need to compare public keys for identity checks |

**Serialization format:** The `VerifyingKey` serializes via `ed25519-dalek`'s serde implementation, which uses the 32-byte compressed Edwards Y representation. This is the standard Ed25519 public key format.

**Manual byte conversion:** In addition to serde, the crate provides explicit `to_bytes()` → `[u8; 32]` and `from_bytes(&[u8; 32]) → Option<Self>` methods. The `from_bytes` method returns `Option` (not `Result`) because there's exactly one failure mode: the bytes don't represent a valid compressed Edwards Y point. The identity point `[0u8; 32]` is rejected by `ed25519-dalek` as a weak key.

#### `generate_keypair()` — Entropy Source

```rust
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let inner = ed25519_dalek::SigningKey::generate(&mut OsRng);
    // ...
}
```

**Why `OsRng` and not `ThreadRng`:**

The function uses `rand::rngs::OsRng` — the OS-provided cryptographically secure random number generator. On macOS this is `SecRandomCopyBytes`, on Linux it's `getrandom(2)`, on Windows it's `BCryptGenRandom`.

`OsRng` was chosen over `ThreadRng` (which uses ChaCha20) because:
- For key generation (a rare, high-stakes operation), the slight performance cost of a syscall is irrelevant
- `OsRng` has no internal state that could be compromised by a memory disclosure vulnerability
- It's the most conservative choice — if the OS CSPRNG is broken, you have bigger problems

**Return type:** Returns a tuple `(SigningKey, VerifyingKey)` rather than a struct. This is intentional — it forces the caller to destructure and name both keys, making it harder to accidentally discard the signing key or confuse the two.

---

### `signer.rs` — Signing Operations

#### `Signature` Type

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    inner: ed25519_dalek::Signature,
}
```

**Design decisions:**

1. **`Clone` + `Serialize`/`Deserialize`.** Signatures are inert data — 64 bytes with no secret material. They need to be stored in databases, transmitted over the network, and embedded in signed messages.

2. **`from_bytes` returns `Option`, not `Result`.** The only validation is length checking (must be exactly 64 bytes). There's no complex error hierarchy to justify a `Result` type. This keeps the API minimal.

3. **`PartialEq`/`Eq` implemented.** Signatures need to be compared for deduplication and testing. The comparison delegates to `ed25519-dalek`'s implementation.

#### `sign()` Function

```rust
pub fn sign(data: &[u8], key: &SigningKey) -> Signature {
    Signature {
        inner: key.inner().sign(data),
    }
}
```

**Critical properties:**

1. **Deterministic.** Ed25519 signing is deterministic — the same key + message always produces the same signature. This is not a design choice by `ghost-signing`; it's inherent to Ed25519 (RFC 8032). The test suite explicitly verifies this property.

2. **No length limit.** The function accepts `&[u8]` of any length. Ed25519 internally hashes the message with SHA-512 before signing, so there's no practical size limit. The test suite exercises payloads from 0 bytes to 1 MB.

3. **Borrows the key.** Takes `&SigningKey`, not `SigningKey`. This is critical — signing should never consume the key. Multiple signatures with the same key is the normal use case.

4. **Infallible.** Returns `Signature` directly, not `Result<Signature>`. Ed25519 signing cannot fail given a valid key and any byte slice. The type system enforces this — you can't construct an invalid `SigningKey` through the public API.

---

### `verifier.rs` — Verification

```rust
pub fn verify(data: &[u8], sig: &Signature, key: &VerifyingKey) -> bool {
    key.inner().verify(data, sig.inner()).is_ok()
}
```

**Design decisions:**

1. **Returns `bool`, not `Result`.** Verification either succeeds or fails. There's no useful error information — a failed verification could be due to a wrong key, tampered data, or a malformed signature, but the caller's response is the same in all cases: reject. Returning `Result` would tempt callers to match on error variants and potentially handle them differently, which is a security anti-pattern.

2. **Constant-time comparison.** The `ed25519-dalek` `verify()` method uses constant-time comparison internally. This prevents timing side-channel attacks where an attacker could determine how many bytes of the signature matched by measuring verification time.

3. **Never panics.** The function is guaranteed to never panic. Malformed inputs (wrong key, corrupted signature) return `false`. This is explicitly tested with adversarial inputs: all-zero signatures, all-zero keys, truncated data.

---

## Architectural Decision: Why cortex-crdt Doesn't Use ghost-signing

This is one of the most important architectural decisions in the codebase and deserves detailed explanation.

`cortex-crdt` (Layer 1) needs Ed25519 signatures for its `SignedDelta` type. It uses `ed25519-dalek` **directly** rather than depending on `ghost-signing` (Layer 0). This seems counterintuitive — why duplicate the dependency?

### The Reasoning

1. **Different wrapper semantics.** `ghost-signing` wraps agent-level operations: "Agent A signed message M." `cortex-crdt` wraps memory-level operations: "Memory delta D was signed by author X." The types are structurally similar but semantically distinct. Forcing them through the same wrapper would either:
   - Require `ghost-signing` to know about CRDT concepts (violating its leaf-crate status), or
   - Require `cortex-crdt` to depend on `ghost-signing` and then immediately unwrap to get at `ed25519-dalek` types (pointless indirection)

2. **Layer separation enforcement.** `cortex-crdt` is Layer 1. `ghost-signing` is Layer 0. While Layer 1 *can* depend on Layer 0, the GHOST architecture enforces that `cortex-*` crates form a self-contained subsystem. Adding `ghost-signing` as a dependency would create a cross-subsystem dependency at the foundation layer, making it harder to extract the Cortex subsystem as an independent library in the future.

3. **Compile-time enforcement.** Multiple tests across the codebase verify this constraint:
   - `cortex-crdt/tests/signing_tests.rs` — asserts `Cargo.toml` does not contain `ghost-signing`
   - `cortex-test-fixtures/tests/hash_algorithm_separation.rs` — same assertion
   - `ghost-integration-tests/tests/property/hash_algorithm_separation.rs` — same assertion
   - `tests/property/hash_algorithm_separation.rs` — workspace-level assertion

   This is **test-enforced architecture** — the layer boundary is not just a convention, it's a CI-verified invariant.

---

## Security Properties

### Zeroize Guarantee

When a `SigningKey` goes out of scope, the 32-byte secret seed is overwritten with zeros before the memory is deallocated. This is provided by `ed25519-dalek`'s `ZeroizeOnDrop` derive (enabled by the `zeroize` feature in `Cargo.toml`).

**What this protects against:**
- Memory dumps (core dumps, `/proc/pid/mem` reads) after the key is no longer needed
- Heap inspection by a co-tenant process in shared hosting
- Swap file forensics

**What this does NOT protect against:**
- A running process with access to the key's memory (the key is in plaintext while in use)
- Compiler optimizations that copy the key to a temporary location (mitigated by `zeroize`'s use of `volatile` writes, but not guaranteed on all platforms)
- Keys that were serialized to disk (which is why `SigningKey` doesn't implement `Serialize`)

### No Serialize on Private Keys

The `SigningKey` type deliberately omits `Serialize` and `Deserialize`. This is a compile-time guarantee that private key material cannot be accidentally written to:
- JSON API responses
- Log files (via `Debug` redaction)
- Database columns
- Network payloads

To persist a signing key, you must go through `ghost-secrets` (which encrypts at rest) or `ghost-identity`'s `AgentKeypairManager` (which manages the full lifecycle).

### Deterministic Signatures

Ed25519 produces deterministic signatures (same key + same message = same signature, always). This is a security feature, not a bug:
- No dependency on random number generation during signing (eliminates an entire class of RNG-related vulnerabilities)
- Signatures are reproducible for auditing
- No risk of nonce reuse (which would leak the private key in ECDSA — Ed25519 derives the nonce from the message)

---

## Downstream Consumer Map

```
ghost-signing (Layer 0)
├── ghost-identity (Layer 4)
│   └── Agent keypair generation, rotation, and storage
├── ghost-mesh (Layer 4)
│   └── Inter-agent message signing and verification
├── ghost-skills (Layer 5)
│   └── Skill manifest signature verification
├── ghost-kill-gates (Layer 4)
│   └── Gate state transition signing for audit chains
├── ghost-gateway (Layer 8)
│   └── VAPID key generation for push notifications
├── ghost-integration-tests (Layer 10)
│   └── Test infrastructure
└── cortex-test-fixtures (Layer 10)
    └── Proptest strategies for generating valid signing keys
```

Note: `cortex-crdt` is intentionally **absent** from this list. See [Architectural Decision](#architectural-decision-why-cortex-crdt-doesnt-use-ghost-signing) above.

---

## Test Strategy

### Unit Tests (`tests/signing_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `sign_verify_round_trip_empty_payload` | Empty byte slice signs and verifies correctly |
| `sign_verify_round_trip_small_payload` | Small payload round-trip |
| `sign_verify_round_trip_max_payload` | 1 MB payload round-trip (no size limit) |
| `verify_with_wrong_key_returns_false` | Cross-key rejection — sign with A, verify with B fails |
| `mutated_payload_fails_verification` | Case flip, appended null, and truncation all detected |
| `truncated_signature_63_bytes_returns_false` | 63-byte slice rejected at construction |
| `all_zero_signature_returns_false` | 64 zero bytes parse but fail verification |
| `all_zero_verifying_key_returns_false_or_none` | Identity point rejected (weak key defense) |
| `signing_is_deterministic` | Same key + same message = same signature |
| `verifying_key_bytes_round_trip` | `to_bytes()` → `from_bytes()` round-trip |
| `signing_key_is_zeroize_compatible` | Exercises the drop path for zeroize |
| `cargo_toml_has_no_ghost_or_cortex_dependencies` | **Leaf-crate invariant** — parses `Cargo.toml` and asserts zero internal deps |

### Property-Based Tests (`tests/property_tests.rs`)

All property tests run with **1000 cases** per property, with payloads from 0 to 64 KB.

| Property | Invariant |
|----------|-----------|
| `round_trip_holds_for_random_payloads` | ∀ data ∈ [0, 64KB]: sign(data, sk) → verify(data, sig, vk) = true |
| `cross_key_verification_fails` | ∀ data: sign(data, sk_a) → verify(data, sig, vk_b) = false |
| `single_byte_mutation_detected` | ∀ data, ∀ single-byte mutation: verify(mutated, sig, vk) = false |

The `single_byte_mutation_detected` property is particularly important — it guarantees that Ed25519 detects even the smallest possible change to the signed data. The test uses `proptest::sample::Index` to select a random byte position and ensures the mutation actually changes the value (wrapping add if the random flip value happens to equal the original).

---

## Dependency Rationale

| Dependency | Version | Why |
|------------|---------|-----|
| `ed25519-dalek` | 2.x | The de facto Rust Ed25519 implementation. Version 2 added `ZeroizeOnDrop` support and improved batch verification. Features: `serde` (key serialization), `rand_core` (OsRng compatibility), `zeroize` (secret key cleanup). |
| `zeroize` | 1.x | Provides the `Zeroize` and `ZeroizeOnDrop` traits. The `derive` feature enables `#[derive(Zeroize, ZeroizeOnDrop)]` on structs. Used transitively through `ed25519-dalek` but also listed as a direct dependency for documentation clarity. |
| `rand` | 0.8 | Provides `OsRng` for cryptographically secure key generation. Version 0.8 is the latest stable that's compatible with `ed25519-dalek` 2.x's `rand_core` requirement. |
| `serde` | 1.x | Serialization framework. Only `VerifyingKey` and `Signature` derive `Serialize`/`Deserialize`. `SigningKey` deliberately does not. |

### Dev Dependencies

| Dependency | Why |
|------------|-----|
| `proptest` | Property-based testing with 1000 random cases per property |
| `serde_json` | Testing JSON serialization round-trips |
| `toml` | Parsing `Cargo.toml` in the leaf-crate audit test |

---

## File Map

```
crates/ghost-signing/
├── Cargo.toml              # Dependency manifest — zero internal deps
├── src/
│   ├── lib.rs              # Public API re-exports
│   ├── keypair.rs          # SigningKey, VerifyingKey, generate_keypair()
│   ├── signer.rs           # Signature type, sign() function
│   └── verifier.rs         # verify() function
└── tests/
    ├── signing_tests.rs    # Unit tests + leaf-crate audit
    └── property_tests.rs   # Proptest: 3 properties × 1000 cases
```

---

## Common Questions

### Why not use `ring` instead of `ed25519-dalek`?

`ring` is an excellent cryptography library, but `ed25519-dalek` was chosen because:
- It provides `Serialize`/`Deserialize` derives out of the box (ring does not)
- It supports `ZeroizeOnDrop` natively (ring requires manual cleanup)
- The GHOST platform doesn't need ring's broader algorithm support (no TLS, no AES, no RSA)
- `ed25519-dalek` is a focused, single-algorithm crate — smaller audit surface

### Why is `zeroize` listed as both a direct and transitive dependency?

`ed25519-dalek` with the `zeroize` feature already pulls in `zeroize`. The direct dependency in `ghost-signing`'s `Cargo.toml` exists for documentation clarity — it makes the security guarantee visible at the manifest level without requiring readers to trace transitive dependencies.

### Can I use `ghost-signing` outside the GHOST platform?

Yes. It has zero internal dependencies and a minimal external dependency set. It's a standalone Ed25519 wrapper with good defaults. However, if you don't need the specific wrapper semantics (no-serialize signing key, debug redaction), using `ed25519-dalek` directly is equally valid.

### Why doesn't `SigningKey` implement `PartialEq`?

Comparing private keys is almost never the right thing to do. If you need to check "is this the same key?", compare the derived `VerifyingKey` instead. This prevents timing attacks on private key comparison and eliminates a class of bugs where code accidentally branches on private key equality.
