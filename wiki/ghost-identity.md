# ghost-identity

> Soul documents, keypair lifecycle, and drift detection — who an agent is and how to prove it.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Ghost Infrastructure) |
| Type | Library |
| Location | `crates/ghost-identity/` |
| Workspace deps | `ghost-signing` (Layer 0), `cortex-core` (Layer 1) |
| External deps | `blake3`, `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, `tokio`, `tracing` |
| Modules | `soul_manager`, `identity_manager`, `corp_policy`, `keypair_manager`, `drift_detector`, `user` |
| Public API | `SoulManager`, `IdentityManager`, `CorpPolicyLoader`, `AgentKeypairManager`, `IdentityDriftDetector`, `UserManager` |
| Test coverage | Dev-dependencies include proptest, tempfile |
| Downstream consumers | `ghost-agent-loop`, `ghost-gateway`, `ghost-mesh`, `ghost-kill-gates` |

---

## Why This Crate Exists

Every GHOST agent has an identity composed of three elements:

1. **Soul document (SOUL.md)** — A human-authored document that defines the agent's purpose, personality, and boundaries. Read-only to the agent.
2. **Ed25519 keypair** — The cryptographic identity. The public key IS the agent's identity in the mesh network.
3. **Corporate policy (CORP_POLICY.md)** — Organization-level constraints signed by the platform key. Tamper-evident.

`ghost-identity` manages all three, plus drift detection (has the agent's behavior diverged from its soul document?) and user identity (who is the human interacting with the agent?).

---

## Module Breakdown

### `soul_manager.rs` — The Agent's Purpose (AC1)

```rust
pub struct SoulDocument {
    pub content: String,
    pub path: PathBuf,
    pub hash: [u8; 32],  // blake3 hash
}
```

The SOUL.md is the most important document in an agent's identity. It defines what the agent is, what it should do, and what it should never do. The `SoulManager` loads it, hashes it with blake3, and stores a baseline embedding for drift detection.

**Key design decisions:**

1. **blake3 hash, not SHA-256.** The soul document hash is used for drift detection and integrity verification within the GHOST platform (not for external interoperability). blake3 is faster and produces the same 32-byte output.

2. **Baseline embedding storage.** The `SoulManager` stores a baseline embedding vector (`Vec<f64>`) that represents the semantic content of the soul document at load time. This embedding is compared against the current embedding by the `IdentityDriftDetector` to detect if the agent's behavior has diverged from its purpose.

3. **Read-only to the agent.** The `SoulManager` has no write methods. The agent cannot modify its own soul document — only the human operator can edit SOUL.md.

### `keypair_manager.rs` — Cryptographic Identity (AC4)

```rust
pub struct AgentKeypairManager {
    keys_dir: PathBuf,
    current: Option<ManagedKeypair>,
    previous: Option<RetiredKeypair>,
    grace_period: Duration,  // default: 1 hour
}
```

**Keypair lifecycle:**

1. **`generate()`** — Creates a new Ed25519 keypair via `ghost-signing`. The public key is written to `agent.pub` on disk. The signing key is held in memory only.
2. **`rotate()`** — Generates a new keypair, moves the current key to `previous` with a rotation timestamp.
3. **`verify()`** — Tries the current key first, then falls back to the previous key during the grace period.

**Key design decisions:**

1. **Signing key in memory only.** The private key is never written to disk by the keypair manager. In production, it's encrypted at rest via the OS keychain (`ghost-secrets`). This prevents private key exposure through filesystem access.

2. **1-hour grace period.** After rotation, the old key remains valid for 1 hour. This handles in-flight messages signed with the old key — they can still be verified during the grace period. After the grace period, the old key is effectively dead.

3. **Grace period tracks rotation time, not creation time.** The `RetiredKeypair` stores `rotated_at` (when the key was retired), not `created_at` (when it was generated). This ensures the grace period starts from the moment of rotation, not from key creation.

4. **Public key on disk, private key in memory.** The `agent.pub` file allows other agents in the mesh to discover this agent's public key without a network round-trip. The private key never touches disk.

### `corp_policy.rs` — Signed Organizational Constraints (AC3)

```rust
pub struct CorpPolicyDocument {
    pub content: String,
    pub signature_verified: bool,
}
```

CORP_POLICY.md is an organization-level document that constrains all agents in a deployment. It's signed with the platform's Ed25519 key, and the signature is embedded as an HTML comment at the end of the file:

```markdown
# Corporate Policy
...policy content...
<!-- SIGNATURE: <hex-encoded-ed25519-signature> -->
```

**Key design decisions:**

1. **Signature in HTML comment.** The signature is embedded in the document itself (not a separate `.sig` file). This means the document is self-contained — you can copy it to a new deployment and it carries its own proof of authenticity. The HTML comment format means the signature is invisible when rendered as Markdown.

2. **Signature covers content only.** The signature is computed over the content before the `<!-- SIGNATURE: ... -->` line. This means the signature doesn't cover itself (which would be circular).

3. **Verification is mandatory.** `CorpPolicyLoader::load()` returns `Err(SignatureInvalid)` if verification fails. There's no "load without verification" path. A tampered CORP_POLICY.md is rejected entirely.

### `drift_detector.rs` — Identity Drift Detection (AC5, AC6)

```rust
pub struct IdentityDriftDetector {
    alert_threshold: f64,  // configurable, default 0.15
    kill_threshold: f64,   // hardcoded 0.25
}
```

Drift detection compares the agent's current behavioral embedding against the baseline soul document embedding using cosine similarity:

```
drift_score = 1.0 - cosine_similarity(baseline, current)
```

| Drift Score | Status | Action |
|-------------|--------|--------|
| < 0.15 | Normal | No action |
| 0.15–0.25 | Alert | Log warning, notify operator |
| ≥ 0.25 | Kill | Emit `TriggerEvent::SoulDrift`, initiate kill gate |

**Key design decisions:**

1. **Kill threshold is hardcoded.** The alert threshold (0.15) is configurable per deployment. The kill threshold (0.25) is not — it's a safety invariant. No configuration can raise it above 0.25. This prevents an operator from accidentally (or maliciously) disabling drift-based kill gates.

2. **Cosine similarity, not Euclidean distance.** Cosine similarity measures the angle between embedding vectors, not their magnitude. This means an agent that generates more text (larger magnitude) isn't penalized — only the direction of the embedding matters.

3. **`TriggerEvent::SoulDrift` integration.** When drift exceeds the kill threshold, the detector builds a `TriggerEvent::SoulDrift` that feeds into the kill gate system. This is the bridge between identity management and safety enforcement.

---

## Security Properties

### Soul Document Integrity

The soul document is hashed with blake3 at load time. Any modification to SOUL.md after loading would produce a different hash, detectable by comparing against the stored hash.

### Corporate Policy Tamper Evidence

CORP_POLICY.md is Ed25519-signed. Modifying any byte of the content invalidates the signature. The signature is verified on every load — there's no cached "trust" that could be stale.

### Private Key Protection

Signing keys are never written to disk by the keypair manager. They exist only in memory, protected by `ghost-signing`'s zeroize-on-drop guarantee. When the keypair manager is dropped, the private key material is overwritten with zeros.

### Hardcoded Kill Threshold

The drift kill threshold (0.25) cannot be changed at runtime or through configuration. This is a defense against configuration tampering — even if an attacker gains access to the configuration system, they cannot prevent drift-based kill gates from firing.

---

## Downstream Consumer Map

```
ghost-identity (Layer 4)
├── ghost-agent-loop (Layer 7)
│   └── Loads soul document, checks drift each turn
├── ghost-gateway (Layer 8)
│   └── Manages agent identity lifecycle, loads CORP_POLICY
├── ghost-mesh (Layer 4)
│   └── Uses agent public keys for message verification
└── ghost-kill-gates (Layer 4)
    └── Receives SoulDrift trigger events
```

---

## File Map

```
crates/ghost-identity/
├── Cargo.toml
├── src/
│   ├── lib.rs                # Module declarations
│   ├── soul_manager.rs       # SOUL.md loading, blake3 hashing, baseline embedding
│   ├── identity_manager.rs   # Agent identity loading from disk
│   ├── corp_policy.rs        # CORP_POLICY.md with Ed25519 signature verification
│   ├── keypair_manager.rs    # Ed25519 keypair generation, rotation, grace period
│   ├── drift_detector.rs     # Cosine similarity drift with alert/kill thresholds
│   └── user.rs               # User document management
```

---

## Common Questions

### Why is the kill threshold hardcoded?

A configurable kill threshold is a security liability. If an attacker compromises the configuration system, they could set the threshold to 1.0 (effectively disabling drift detection). The hardcoded threshold ensures that drift-based safety enforcement cannot be disabled through configuration alone — you'd need to modify and recompile the binary.

### Why blake3 for soul hashing but SHA-256 for ITP content hashing?

Different use cases. Soul document hashing is internal to GHOST (speed matters, interoperability doesn't). ITP content hashing may be verified by external audit tools (interoperability matters, speed doesn't). blake3 is ~3x faster than SHA-256 for the same output size.

### Can an agent have multiple soul documents?

No. Each agent has exactly one SOUL.md. Multiple soul documents would create ambiguity about the agent's purpose and make drift detection meaningless (drift from which soul?). If an agent needs to serve multiple purposes, the soul document should describe all of them.
