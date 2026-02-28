# Inter-Agent Message Flow — Complete Sequence Diagrams

> Date: 2026-02-27
> Scope: Full inter-agent messaging lifecycle across all 4 communication patterns
> Systems: ghost-gateway, ghost-signing, ghost-policy, ghost-agent-loop, ghost-identity, ghost-audit, ghost-mesh (future)
> Source: AGENT_ARCHITECTURE.md §18, FILE_MAPPING.md Finding 4, OWASP ASI07 mitigation
> Purpose: Zero-error implementation reference. Every byte on the wire, every ordering constraint, every race condition, every failure mode documented.

---

## TABLE OF CONTENTS

1. [Systems Involved](#systems-involved)
2. [Data Structures](#data-structures)
3. [Shared Infrastructure: Message Signing & Verification](#shared-infrastructure)
4. [Shared Infrastructure: Replay Prevention](#replay-prevention)
5. [Shared Infrastructure: Optional Encryption](#optional-encryption)
6. [Shared Infrastructure: Policy Evaluation for Inter-Agent Messages](#policy-evaluation)
7. [Pattern 1: Request/Response](#pattern-1-requestresponse)
8. [Pattern 2: Fire-and-Forget](#pattern-2-fire-and-forget)
9. [Pattern 3: Delegation with Escrow](#pattern-3-delegation-with-escrow)
10. [Pattern 4: Broadcast](#pattern-4-broadcast)
11. [Cross-Cutting: Offline Agent Handling](#offline-agent-handling)
12. [Cross-Cutting: Failure Modes & Recovery](#failure-modes)
13. [Cross-Cutting: Audit Trail Requirements](#audit-trail)
14. [Cross-Cutting: Convergence Monitor Integration](#convergence-integration)
15. [Cross-Cutting: Kill Switch Interaction](#kill-switch-interaction)
16. [Ordering Constraints & Race Conditions](#ordering-constraints)
17. [Implementation Checklist](#implementation-checklist)

---

## SYSTEMS INVOLVED

| System | Process | Role | Crate | Key Files |
|--------|---------|------|-------|-----------|
| Gateway Messaging | In-process (gateway) | Message dispatch, signature verification, replay check, queue management | `ghost-gateway` | `src/messaging/mod.rs`, `src/messaging/protocol.rs`, `src/messaging/dispatcher.rs`, `src/messaging/encryption.rs` |
| Gateway Routing | In-process (gateway) | Lane queue delivery, session routing | `ghost-gateway` | `src/routing/message_router.rs`, `src/routing/lane_queue.rs` |
| Agent Registry | In-process (gateway) | Agent lookup, lifecycle state, online/offline status | `ghost-gateway` | `src/agents/registry.rs`, `src/agents/isolation.rs` |
| Signing Infrastructure | Library (shared) | Ed25519 keypair generation, sign/verify primitives | `ghost-signing` | `src/keypair.rs`, `src/signer.rs`, `src/verifier.rs` |
| Identity / Keypair | Library (per-agent) | Per-agent keypair storage, rotation, public key registry | `ghost-identity` | `src/keypair.rs` |
| Policy Engine | In-process (gateway) | Authorization: "Can Agent A send this message type to Agent B?" | `ghost-policy` | `src/engine.rs`, `src/policy/capability_grants.rs`, `src/policy/convergence_policy.rs` |
| Agent Loop | In-process (gateway child) | Sends messages via tool call, receives messages from lane queue | `ghost-agent-loop` | `src/tools/builtin/messaging.rs` (NEW), `src/runner.rs` |
| Audit Backend | In-process (gateway) | Append-only logging of every inter-agent message | `ghost-audit` | `src/query.rs` |
| Convergence Monitor | Sidecar process | Receives ITP events for inter-agent interactions, feeds into initiative balance signal | `convergence-monitor` | `src/pipeline/ingest.rs` |
| Mesh (Future) | Library (Phase 9) | Escrow creation, payment verification, settlement | `ghost-mesh` | `src/types.rs`, `src/traits.rs`, `src/protocol.rs` |

### Process Boundaries

```
┌─────────────────────────────────────────────────────────────────────────┐
│  GHOST GATEWAY PROCESS (single long-running process)                     │
│                                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                   │
│  │  Agent A      │  │  Agent B      │  │  Agent C      │  (in-process    │
│  │  (AgentRunner)│  │  (AgentRunner)│  │  (AgentRunner)│   or child      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘   processes)      │
│         │                  │                  │                           │
│         ▼                  ▼                  ▼                           │
│  ┌─────────────────────────────────────────────────────────────────┐     │
│  │                    MessageDispatcher                             │     │
│  │  ┌─────────┐  ┌──────────┐  ┌──────────┐  ┌───────────────┐   │     │
│  │  │ Signing  │  │ Replay   │  │ Policy   │  │ Lane Queue    │   │     │
│  │  │ Verify   │  │ Guard    │  │ Check    │  │ Delivery      │   │     │
│  │  └─────────┘  └──────────┘  └──────────┘  └───────────────┘   │     │
│  └─────────────────────────────────────────────────────────────────┘     │
│                                                                          │
│  ┌──────────────────┐  ┌──────────────────┐                              │
│  │  AgentRegistry    │  │  AuditLogger      │                             │
│  └──────────────────┘  └──────────────────┘                              │
└─────────────────────────────────────────────────────────────────────────┘
         │
         │ ITP events (async, non-blocking)
         ▼
┌─────────────────────────────────────────┐
│  CONVERGENCE MONITOR (sidecar process)   │
└─────────────────────────────────────────┘
```

---

## DATA STRUCTURES

### AgentMessage (The Wire Format)

Every inter-agent message uses this canonical structure. Defined in
`ghost-gateway/src/messaging/protocol.rs`.

```rust
/// The canonical inter-agent message envelope.
/// This is what gets serialized, signed, transmitted, verified, and logged.
///
/// AgentId is imported from cortex-core/models/ (existing type,
/// defined as `pub struct AgentId(pub String)`).
/// FILE_MAPPING references this as agent_id.rs; actual codebase has agent.rs.
/// This ensures agent identity is consistent across the entire platform
/// (CRDT deltas, memory operations, and inter-agent messages all use the
/// same AgentId type).
pub struct AgentMessage {
    /// Header — routing and identity
    pub from: AgentId,                    // e.g. "agent:developer"
    pub to: MessageTarget,                // Single agent, or Broadcast
    pub message_id: Uuid,                 // UUIDv7 (time-ordered, globally unique)
    pub parent_id: Option<Uuid>,          // Links response to request (correlation)
    pub timestamp: DateTime<Utc>,         // Sender's wall clock at signing time

    /// Payload — the actual content
    pub payload: MessagePayload,          // Type-tagged content (see below)

    /// Security — integrity + replay prevention
    pub signature: Ed25519Signature,      // Signs: canonical_bytes(from, to, message_id,
                                          //   parent_id, timestamp, payload, nonce)
    pub content_hash: String,             // blake3 hash of canonical_bytes, hex-encoded.
                                          //   Matches cortex-crdt SignedDelta.content_hash pattern.
                                          //   Verified BEFORE signature check (cheap integrity gate).
    pub nonce: [u8; 32],                  // Random 32 bytes, unique per message

    /// Optional confidentiality
    pub encrypted: bool,                  // If true, payload.content is encrypted
    pub encryption_metadata: Option<EncryptionMetadata>,
}

/// Who the message is addressed to.
pub enum MessageTarget {
    Agent(AgentId),                       // Single recipient
    Broadcast,                            // All agents (gateway fans out)
}

/// Type-tagged payload. Each variant carries pattern-specific data.
pub enum MessagePayload {
    TaskRequest(TaskRequestPayload),
    TaskResponse(TaskResponsePayload),
    Notification(NotificationPayload),
    Broadcast(BroadcastPayload),
    DelegationOffer(DelegationOfferPayload),
    DelegationAccept(DelegationAcceptPayload),
    DelegationReject(DelegationRejectPayload),
    DelegationComplete(DelegationCompletePayload),
    DelegationDispute(DelegationDisputePayload),
    /// Encrypted payload — original variant serialized, encrypted, wrapped.
    /// The payload_type field preserves the original variant name in cleartext
    /// so the gateway can evaluate policy on metadata without decrypting.
    Encrypted(EncryptedPayloadData),
}

/// Wrapper for encrypted payload content.
pub struct EncryptedPayloadData {
    pub payload_type: String,             // Original variant name (e.g. "TaskRequest")
    pub ciphertext: Vec<u8>,             // Encrypted canonical bytes of original payload
}
```

### Payload Variants

```rust
pub struct TaskRequestPayload {
    pub content: String,                  // The actual request text
    pub priority: Priority,               // Low, Normal, High, Critical
    pub deadline: Option<DateTime<Utc>>,  // Optional deadline for response
    pub context: BTreeMap<String, Value>, // Arbitrary structured context.
                                          //   MUST be BTreeMap (not HashMap) for
                                          //   deterministic serialization in signing.
                                          //   See Appendix A.
}

pub struct TaskResponsePayload {
    pub content: String,                  // The response text
    pub status: ResponseStatus,           // Success, Partial, Failed, Declined
    pub artifacts: Vec<Artifact>,         // Optional attached outputs (file refs, data)
}

pub struct NotificationPayload {
    pub content: String,                  // Notification text
    pub severity: NotificationSeverity,   // Info, Warning, Critical
    pub category: String,                 // e.g. "monitoring", "system", "task"
}

pub struct BroadcastPayload {
    pub content: String,                  // Broadcast text
    pub source: BroadcastSource,          // Gateway (system), Agent (voluntary)
    pub requires_ack: bool,               // Whether recipients must acknowledge
}

pub struct DelegationOfferPayload {
    pub task: String,                     // Task description
    pub requirements: Vec<String>,        // What the delegate needs to deliver
    pub deadline: Option<DateTime<Utc>>,
    pub escrow_amount: Option<Decimal>,   // If payments enabled, escrow amount
    pub escrow_tx_id: Option<String>,     // Reference to ghost-mesh escrow transaction
}

pub struct DelegationAcceptPayload {
    pub offer_message_id: Uuid,           // References the DelegationOffer.message_id
    pub estimated_completion: Option<DateTime<Utc>>,
}

pub struct DelegationRejectPayload {
    pub offer_message_id: Uuid,
    pub reason: String,
}

pub struct DelegationCompletePayload {
    pub offer_message_id: Uuid,           // References original offer
    pub proof: DelegationProof,           // Evidence of completion
    pub artifacts: Vec<Artifact>,
}

pub struct DelegationDisputePayload {
    pub offer_message_id: Uuid,
    pub reason: String,
    pub evidence: Vec<Artifact>,
}

pub enum Priority { Low, Normal, High, Critical }
pub enum ResponseStatus { Success, Partial, Failed, Declined }
pub enum NotificationSeverity { Info, Warning, Critical }
pub enum BroadcastSource { Gateway, Agent(AgentId) }

pub struct Artifact {
    pub name: String,
    pub content_type: String,             // MIME type
    pub data: ArtifactData,
}

pub enum ArtifactData {
    Inline(Vec<u8>),                      // Small payloads (< 64KB)
    FileRef(PathBuf),                     // Reference to file in agent workspace
}

pub struct DelegationProof {
    pub proof_type: ProofType,
    pub data: Value,                      // Proof-type-specific structured data
}

pub enum ProofType {
    FileCreated,                          // Proof: file exists at path with hash
    TestsPassed,                          // Proof: test suite output
    ContentGenerated,                     // Proof: content hash matches spec
    Custom(String),                       // Extensible proof types
}

pub struct EncryptionMetadata {
    pub algorithm: String,                // "x25519-xsalsa20-poly1305" (NaCl box)
    pub sender_ephemeral_pk: [u8; 32],    // Ephemeral public key for this message
    pub recipient_pk_fingerprint: [u8; 8],// First 8 bytes of blake3(recipient_pk)
    pub encryption_nonce: [u8; 24],       // XSalsa20Poly1305 nonce (24 bytes).
                                          //   MUST be included — recipient needs this
                                          //   to decrypt. Generated randomly per message.
}
```

### Replay Prevention State

```rust
/// Maintained by MessageDispatcher. In-memory with periodic persistence.
pub struct ReplayGuard {
    /// Seen nonces within the validity window.
    /// Key: blake3(nonce || from || to), Value: timestamp received.
    /// Entries older than REPLAY_WINDOW are pruned every 60s.
    seen_nonces: HashMap<[u8; 32], DateTime<Utc>>,

    /// Maximum age of a valid message. Messages with timestamp older
    /// than now() - REPLAY_WINDOW are rejected unconditionally.
    replay_window: Duration,              // Default: 5 minutes

    /// Monotonic sequence counter per sender.
    /// Prevents replay even if nonce is unique but message is old.
    /// Key: AgentId, Value: highest seen UUIDv7 timestamp component.
    sender_sequence: HashMap<AgentId, u64>,
}
```

---

## SHARED INFRASTRUCTURE: MESSAGE SIGNING & VERIFICATION

This section documents the exact signing and verification flow that is shared
across all 4 communication patterns. Every pattern invokes this as a subroutine.

### Signing (Sender Side)

Performed inside the agent's process space. The agent's private key never leaves
its isolated credential store.

```
SEQUENCE: Agent A Signs a Message

1. ghost-agent-loop/tools/builtin/messaging.rs: send_agent_message()
   │
   ├── Construct AgentMessage struct with all fields EXCEPT signature
   │   ├── from = self.agent_id (e.g. "agent:developer")
   │   ├── to = target (from tool call arguments)
   │   ├── message_id = Uuid::now_v7()
   │   ├── parent_id = correlation_id (if responding to a prior message)
   │   ├── timestamp = Utc::now()
   │   ├── payload = MessagePayload variant (from tool call arguments)
   │   ├── nonce = rand::thread_rng().gen::<[u8; 32]>()
   │   ├── encrypted = false (initially; encryption applied in step 3 if needed)
   │   └── signature = EMPTY (placeholder)
   │
   ├── 2. Compute canonical signing bytes:
   │   │
   │   │   canonical_bytes = concat_deterministic(
   │   │       from.as_bytes(),
   │   │       to.canonical_bytes(),        // Agent(id) → id.as_bytes()
   │   │                                    // Broadcast → b"__broadcast__"
   │   │       message_id.as_bytes(),        // 16 bytes, big-endian
   │   │       parent_id.map_or(b"__none__", |id| id.as_bytes()),
   │   │       timestamp.to_rfc3339().as_bytes(),
   │   │       payload.canonical_bytes(),    // Deterministic serialization
   │   │                                    // (see Appendix A for exact rules)
   │   │       nonce                         // 32 raw bytes
   │   │   )
   │   │
   │   │   NOTE: The signature is over the raw canonical bytes, NOT a hash
   │   │   of them. This is consistent with the existing cortex-crdt/signing/
   │   │   signed_delta.rs pattern, which signs serde_json::to_vec(&delta)
   │   │   directly. ed25519 internally uses SHA-512 on the input, so
   │   │   pre-hashing is unnecessary.
   │   │
   │   │   A separate content_hash field (blake3 of canonical_bytes) is stored
   │   │   on the message for integrity verification, matching the existing
   │   │   SignedDelta.content_hash pattern in cortex-crdt.
   │   │
   │   └── CRITICAL: canonical_bytes() for payload MUST use deterministic
   │       serialization. The existing cortex-crdt code uses serde_json::to_vec()
   │       which works for MemoryDelta because its fields are simple types.
   │       For MessagePayload (which contains HashMap<String, Value>), serde_json
   │       does NOT guarantee key order. Two options:
   │       OPTION A: Use serde_json with #[serde(serialize_with)] to force
   │         BTreeMap ordering on all map types (less code, fragile).
   │       OPTION B: Hand-write canonical_bytes() for the signing payload
   │         (more code, bulletproof). RECOMMENDED for inter-agent messages
   │         because the payload types are more complex than CRDT deltas.
   │       Failure here = signature verification fails on receiver side
   │       due to non-deterministic JSON key ordering.
   │
   ├── 3. Sign the canonical bytes:
   │   │
   │   │   3a. Load private key (ghost-identity handles storage):
   │   │   │   ghost-identity/keypair.rs: AgentKeypairManager::load_signing_key(agent_name)
   │   │   │   → Reads from ~/.ghost/agents/{agent_name}/keys/agent.key
   │   │   │   → Returns ed25519_dalek::SigningKey
   │   │   │
   │   │   3b. Sign (ghost-signing handles crypto):
   │   │   │   ghost-signing/signer.rs: sign(canonical_bytes, &signing_key)
   │   │   │   → ed25519_dalek::SigningKey::sign(&canonical_bytes)
   │   │   │   → Ed25519Signature (64 bytes)
   │   │   │
   │   │   │   ALIGNMENT WITH EXISTING CODE: The existing cortex-crdt/signing/
   │   │   │   signed_delta.rs signs raw serialized bytes directly (not a hash
   │   │   │   of the bytes). For inter-agent messages, we sign the raw
   │   │   │   canonical_bytes directly as well — ed25519 internally hashes
   │   │   │   with SHA-512 before signing, so pre-hashing with blake3 is
   │   │   │   unnecessary and would diverge from the existing signing pattern.
   │   │   │   The blake3 content_hash is stored SEPARATELY for integrity
   │   │   │   verification (same pattern as SignedDelta.content_hash).
   │   │   │
   │   │   └── SECURITY: Private key is loaded into memory only for the
   │   │       duration of the sign() call. Zeroized on drop (zeroize crate).
   │   │       Never logged. Never serialized. Never sent over any channel.
   │   │       ghost-identity owns the key lifecycle (load/store/rotate).
   │   │       ghost-signing owns the crypto primitives (sign/verify).
   │   │       This separation means ghost-signing is a leaf crate with
   │   │       zero knowledge of file paths or agent identity.
   │   │
   │   └── Set message.signature = computed signature
   │
   └── 4. Submit signed message to MessageDispatcher:
       │
       └── ghost-gateway/messaging/dispatcher.rs: dispatch(message)
           (message crosses process boundary if agent runs in separate process;
            for InProcess isolation mode, it's a function call within the gateway)
```

### Verification (Gateway Side)

Performed by the MessageDispatcher before any routing or delivery.

```
SEQUENCE: Gateway Verifies a Signed Message

1. ghost-gateway/messaging/dispatcher.rs: dispatch(message)
   │
   ├── 2. Look up sender's public key:
   │   │
   │   │   ghost-identity/keypair.rs: AgentKeypairManager::get_public_key(message.from)
   │   │   │
   │   │   ├── Load from: ~/.ghost/agents/{agent_name}/keys/agent.pub
   │   │   │
   │   │   ├── IF key not found:
   │   │   │   └── REJECT message with error: "Unknown sender — no public key registered"
   │   │   │       Log to audit: SIGNATURE_VERIFICATION_FAILED, reason: "no_public_key"
   │   │   │       STOP. Do not deliver.
   │   │   │
   │   │   └── IF key found but expired (rotation in progress):
   │   │       ├── Check archived keys with expiry > message.timestamp
   │   │       ├── IF archived key valid for message timestamp → use it
   │   │       └── IF no valid key → REJECT (key rotation window exceeded)
   │   │
   ├── 3. Recompute canonical signing bytes (IDENTICAL algorithm as sender):
   │   │
   │   │   canonical_bytes = concat_deterministic(
   │   │       message.from.as_bytes(),
   │   │       message.to.canonical_bytes(),
   │   │       message.message_id.as_bytes(),
   │   │       message.parent_id.map_or(b"__none__", |id| id.as_bytes()),
   │   │       message.timestamp.to_rfc3339().as_bytes(),
   │   │       message.payload.canonical_bytes(),
   │   │       message.nonce
   │   │   )
   │   │
   │   │   ALSO compute content_hash for integrity check:
   │   │   content_hash = blake3::hash(&canonical_bytes).to_hex().to_string()
   │   │   IF content_hash != message.content_hash:
   │   │       REJECT: "Content hash mismatch — message may have been tampered"
   │   │       (NOTE: The existing cortex-crdt/signing/verifier.rs checks
   │   │        signature FIRST, then content_hash. For inter-agent messages,
   │   │        we INVERT this order: content_hash first as a cheap integrity
   │   │        gate — blake3 is ~10x faster than ed25519 verify. If the hash
   │   │        fails, we skip the expensive signature check entirely.
   │   │        This is a deliberate improvement over the cortex-crdt pattern.)
   │   │
   │   └── CRITICAL: This MUST produce byte-identical output to the sender's
   │       computation. Any divergence (different JSON key order, different
   │       timestamp format, different encoding) = verification failure.
   │       This is the #1 source of signing bugs. Test with property-based
   │       tests: sign(msg) then verify(msg) must ALWAYS pass for any valid msg.
   │
   ├── 4. Verify signature:
   │   │
   │   │   ghost-signing/verifier.rs: verify(canonical_bytes, message.signature, sender_public_key)
   │   │   │
   │   │   ├── ed25519_dalek::VerifyingKey::verify(&canonical_bytes, &signature)
   │   │   │
   │   │   ├── IF verification succeeds:
   │   │   │   └── Continue to replay check (step 5)
   │   │   │
   │   │   └── IF verification fails:
   │   │       ├── REJECT message
   │   │       ├── Log to audit: SIGNATURE_VERIFICATION_FAILED
   │   │       │   { from, to, message_id, timestamp, reason: "invalid_signature" }
   │   │       ├── Increment per-agent anomaly counter
   │   │       │   (3+ failures in 5min → trigger kill switch evaluation)
   │   │       └── STOP. Do not deliver. Do not inform sender of specific failure
   │   │           reason (prevents oracle attacks on signature scheme).
   │   │           Return generic: "Message delivery failed"
   │   │
   └── 5. Proceed to Replay Prevention check (next section)
```

**IMPLEMENTATION NOTE — Key Rotation Grace Period**: When an agent's keypair is rotated
(ghost-identity/keypair.rs), the old public key is archived with an expiry timestamp
(default: 1 hour after rotation). During this window, messages signed with the old key
are still accepted. After expiry, only the new key is valid. This prevents message loss
during rotation but limits the window for compromised key exploitation.

**IMPLEMENTATION NOTE — Signing Infrastructure Relationship**:
Three components handle signing. Their boundaries MUST be respected:

```
ghost-signing (leaf crate — ed25519-dalek only, zero filesystem knowledge)
├── keypair.rs: generate_keypair() → (SigningKey, VerifyingKey)
├── signer.rs:  sign(bytes, &SigningKey) → Signature
└── verifier.rs: verify(bytes, &Signature, &VerifyingKey) → bool

ghost-identity/keypair.rs (uses ghost-signing, adds filesystem + lifecycle)
├── AgentKeypairManager::generate_and_store(agent_name) → stores to disk
├── AgentKeypairManager::load_signing_key(agent_name) → SigningKey
├── AgentKeypairManager::get_public_key(agent_name) → VerifyingKey
├── AgentKeypairManager::rotate(agent_name) → archives old, generates new
└── AgentKeypairManager::get_archived_keys(agent_name) → Vec<(VerifyingKey, expiry)>

cortex-crdt/signing/ (EXISTING — uses ed25519-dalek directly, NOT ghost-signing)
├── KeyRegistry: in-memory HashMap<AgentId, VerifyingKey>
├── SignedDelta: wraps MemoryDelta with signature + content_hash
└── SignedDeltaVerifier: verifies before MergeEngine::apply_delta()

CRITICAL: cortex-crdt/signing/ predates ghost-signing and uses ed25519-dalek
directly. It does NOT depend on ghost-signing. This is intentional — cortex-crdt
is a Layer 1 crate and ghost-signing is Layer 3. The signing PRIMITIVES are
identical (both use ed25519-dalek), but the WRAPPERS are different:
- cortex-crdt wraps MemoryDelta → SignedDelta
- ghost-gateway wraps AgentMessage → signed AgentMessage

NOTE ON EXISTING CODE COMMENT: The doc comment on SignedDelta.signature says
"Ed25519 signature over blake3(delta serialized bytes)" — this is MISLEADING.
The actual code does `signing_key.sign(&serialized)` which signs the raw
serialized bytes, NOT the blake3 hash. The blake3 hash is stored separately
as content_hash for integrity checking. The doc comment should be corrected
to: "Ed25519 signature over serde_json::to_vec(&delta)". Our inter-agent
message signing follows the ACTUAL behavior (sign raw bytes), not the comment.

The KeyRegistry in cortex-crdt is populated from the same key files that
ghost-identity manages (~/.ghost/agents/{name}/keys/agent.pub). On gateway
boot, ghost-gateway reads agent public keys via ghost-identity and registers
them in BOTH:
1. The MessageDispatcher's key lookup (for inter-agent message verification)
2. The cortex-crdt KeyRegistry (for CRDT delta verification)

This dual registration happens in ghost-gateway/bootstrap.rs during agent
initialization. If a key is rotated, both registries must be updated atomically.
```

---

## SHARED INFRASTRUCTURE: REPLAY PREVENTION

Performed by the MessageDispatcher after signature verification passes.
This is the second gate in the dispatch pipeline.

```
SEQUENCE: Replay Prevention Check

5. ghost-gateway/messaging/dispatcher.rs: check_replay(message)
   │
   ├── 5a. Timestamp freshness check:
   │   │
   │   │   age = Utc::now() - message.timestamp
   │   │
   │   │   IF age > REPLAY_WINDOW (default 5 minutes):
   │   │   │   REJECT: "Message too old — timestamp outside replay window"
   │   │   │   Log to audit: REPLAY_REJECTED, reason: "timestamp_expired"
   │   │   │   STOP.
   │   │   │
   │   │   IF age < Duration::ZERO (message from the future):
   │   │   │   Allow up to 30 seconds of clock skew (configurable).
   │   │   │   IF message.timestamp > Utc::now() + CLOCK_SKEW_TOLERANCE:
   │   │   │   │   REJECT: "Message timestamp in the future"
   │   │   │   │   Log to audit: REPLAY_REJECTED, reason: "future_timestamp"
   │   │   │   │   STOP.
   │   │   │   └── ELSE: Accept (within clock skew tolerance)
   │   │   │
   │   │   └── ELSE: Timestamp is fresh. Continue.
   │   │
   │   └── WHY BOTH nonce AND timestamp: Timestamp alone is insufficient because
   │       two legitimate messages could share the same second. Nonce alone is
   │       insufficient because a replayed message with a valid nonce would be
   │       accepted if we only check nonce uniqueness without time bounds.
   │       Together: nonce ensures uniqueness, timestamp bounds the window.
   │
   ├── 5b. Nonce uniqueness check:
   │   │
   │   │   nonce_key = blake3::hash(message.nonce || message.from || message.to)
   │   │
   │   │   IF replay_guard.seen_nonces.contains_key(nonce_key):
   │   │   │   REJECT: "Duplicate nonce — possible replay attack"
   │   │   │   Log to audit: REPLAY_REJECTED, reason: "duplicate_nonce"
   │   │   │   Increment per-agent anomaly counter
   │   │   │   STOP.
   │   │   │
   │   │   └── ELSE:
   │   │       ├── Insert nonce_key → Utc::now() into seen_nonces
   │   │       └── Continue.
   │   │
   │   └── MEMORY MANAGEMENT: seen_nonces is pruned every 60 seconds.
   │       Entries older than REPLAY_WINDOW + 1 minute are removed.
   │       At 100 messages/sec (extreme load), this is ~30K entries max.
   │       HashMap with 32-byte keys = ~1.2 MB. Negligible.
   │
   ├── 5c. Sequence monotonicity check (defense in depth):
   │   │
   │   │   sender_ts = message.message_id.get_timestamp()  // UUIDv7 embeds timestamp
   │   │
   │   │   IF sender_sequence[message.from] exists AND sender_ts <= sender_sequence[message.from]:
   │   │   │   This is NOT necessarily a replay — UUIDv7 has millisecond precision,
   │   │   │   and two messages in the same millisecond are legitimate.
   │   │   │   Only flag if sender_ts is SIGNIFICANTLY older (> 1 second behind).
   │   │   │   IF sender_sequence[message.from] - sender_ts > 1_000_000_000 (1 sec in nanos):
   │   │   │   │   Log WARNING: "Out-of-order message from {from} — possible replay"
   │   │   │   │   (Do NOT reject — this is a soft signal, not a hard gate.
   │   │   │   │    Network reordering can cause legitimate out-of-order delivery.)
   │   │   │   └── ELSE: Accept (within tolerance)
   │   │   │
   │   │   └── Update sender_sequence[message.from] = max(current, sender_ts)
   │   │
   │   └── WHY UUIDv7: Time-ordered UUIDs give us a natural sequence number
   │       without requiring explicit counters. The timestamp component provides
   │       monotonicity. Combined with nonce uniqueness, this is belt-and-suspenders.
   │
   └── 6. Proceed to Policy Evaluation (next section)
```

---

## SHARED INFRASTRUCTURE: OPTIONAL ENCRYPTION

Applied by the sender BEFORE signing (the signature covers the encrypted payload,
not the plaintext). Decrypted by the recipient AFTER delivery.

```
SEQUENCE: Encryption (Sender Side — Optional)

Triggered when: Agent A's tool call includes `encrypted: true` parameter,
OR when policy requires encryption for this sender→recipient pair,
OR when payload contains sensitive data (detected by content classifier).

BETWEEN steps 1 and 2 of the signing sequence:

1.5a. Determine if encryption is needed:
      │
      ├── Check ghost-policy: is_encryption_required(from, to, payload_type)
      │   (Some agent pairs may have mandatory encryption configured in ghost.yml)
      │
      ├── Check tool call arguments: explicit `encrypted: true`
      │
      └── IF encryption needed:

1.5b. Encrypt the payload:
      │
      ├── Load recipient's public key:
      │   ghost-identity/keypair.rs: AgentKeypairManager::get_public_key(message.to)
      │   (For Broadcast messages: encryption is NOT supported. If encryption is
      │    requested on a Broadcast, REJECT with error: "Cannot encrypt broadcasts —
      │    use individual messages for sensitive content")
      │
      ├── Generate ephemeral X25519 keypair for this message:
      │   let ephemeral_secret = x25519_dalek::EphemeralSecret::random();
      │   let ephemeral_public = x25519_dalek::PublicKey::from(&ephemeral_secret);
      │
      ├── Derive shared secret:
      │   let shared_secret = ephemeral_secret.diffie_hellman(&recipient_x25519_pk);
      │   (NOTE: Ed25519 keys must be converted to X25519 for DH.
      │    Use ed25519_dalek::SigningKey::to_scalar() → x25519 scalar,
      │    or store separate X25519 keys alongside Ed25519 keys.)
      │
      ├── Encrypt payload content:
      │   let enc_nonce = XSalsa20Poly1305::generate_nonce(&mut OsRng);
      │   let plaintext_bytes = payload.canonical_bytes();
      │   let ciphertext = XSalsa20Poly1305::new(&shared_secret)
      │       .encrypt(&enc_nonce, plaintext_bytes.as_ref())
      │       .expect("encryption failed");
      │
      ├── Replace payload with encrypted wrapper:
      │   The original MessagePayload variant is serialized to canonical bytes,
      │   encrypted, and replaced with a special EncryptedPayload variant:
      │   message.payload = MessagePayload::Encrypted(EncryptedPayloadData {
      │       payload_type: original_variant_name,  // e.g. "TaskRequest"
      │       ciphertext: ciphertext,               // encrypted canonical bytes
      │   })
      │   This preserves the payload TYPE TAG in cleartext (for policy evaluation
      │   on metadata) while encrypting the actual content.
      │
      │   Set message.encrypted = true
      │   Set message.encryption_metadata = Some(EncryptionMetadata {
      │       algorithm: "x25519-xsalsa20-poly1305",
      │       sender_ephemeral_pk: ephemeral_public.to_bytes(),
      │       recipient_pk_fingerprint: blake3::hash(recipient_pk)[..8],
      │       encryption_nonce: enc_nonce.into(),
      │   })
      │
      └── THEN proceed to step 2 (signing).
          The signature covers the ENCRYPTED payload, not plaintext.
          This is encrypt-then-sign (EtS). The alternative (sign-then-encrypt, StE)
          would hide the signature from the gateway, preventing verification
          without decryption. EtS allows the gateway to verify authenticity
          without seeing plaintext content.


SEQUENCE: Decryption (Recipient Side)

After the message is delivered to Agent B's lane queue and Agent B processes it:

1. ghost-agent-loop/tools/builtin/messaging.rs: process_incoming_message(message)
   │
   ├── IF message.encrypted == false:
   │   └── Use payload directly. Done.
   │
   └── IF message.encrypted == true:
       │
       ├── Load own private key:
       │   ~/.ghost/agents/{self.agent_name}/keys/agent.key
       │
       ├── Verify fingerprint matches:
       │   blake3::hash(own_public_key)[..8] == message.encryption_metadata.recipient_pk_fingerprint
       │   IF mismatch: REJECT — "Message not encrypted for this agent"
       │
       ├── Convert Ed25519 private key to X25519 scalar
       │
       ├── Derive shared secret:
       │   shared_secret = own_x25519_secret.diffie_hellman(
       │       &message.encryption_metadata.sender_ephemeral_pk
       │   )
       │
       ├── Decrypt:
       │   plaintext = XSalsa20Poly1305::new(&shared_secret)
       │       .decrypt(
       │           &message.encryption_metadata.encryption_nonce,
       │           message.payload.as_encrypted().ciphertext.as_ref()
       │       )
       │
       ├── IF decryption fails (tampered ciphertext, wrong key):
       │   ├── Log to audit: DECRYPTION_FAILED { message_id, from, to }
       │   ├── Do NOT retry — this is a hard failure
       │   └── Return error to agent loop: "Failed to decrypt message from {from}"
       │
       └── Deserialize plaintext back to MessagePayload
           Use in agent's context for processing.
```

**CRITICAL DESIGN DECISION — Encrypt-then-Sign (EtS)**:
The gateway can verify message authenticity (signature) without decrypting the payload.
This means the gateway CANNOT read encrypted message content, which is the correct
privacy property. The gateway knows WHO is talking to WHOM (metadata) but not WHAT
they're saying (content). Policy checks operate on metadata only for encrypted messages.

---

## SHARED INFRASTRUCTURE: POLICY EVALUATION FOR INTER-AGENT MESSAGES

Performed by the MessageDispatcher after replay prevention passes.
This is the third gate in the dispatch pipeline.

**ARCHITECTURAL NOTE**: AGENT_ARCHITECTURE.md §18 Security Properties table says
"Authorization: Receiving agent checks: 'Is this sender allowed to ask me to do this?'
via policy engine." This is corrected here: the GATEWAY checks authorization, not the
receiving agent. The gateway is the policy authority (it owns the PolicyEngine). The
receiving agent trusts that any message delivered to its lane queue has already passed
policy evaluation. This is consistent with the routing section of §18 which says
"The gateway: Checks policy (is this agent allowed to talk to that agent?)".
Placing authorization at the gateway prevents a compromised agent from bypassing
policy by accepting messages it shouldn't receive.

```
SEQUENCE: Policy Evaluation for Inter-Agent Message

6. ghost-gateway/messaging/dispatcher.rs: check_policy(message)
   │
   ├── 6a. Construct PolicyContext for inter-agent messaging:
   │   │
   │   │   ghost-policy/context.rs: PolicyContext {
   │   │       principal: message.from,           // "agent:developer"
   │   │       action: format!("message:{}", message.payload.type_name()),
   │   │                                          // e.g. "message:task_request",
   │   │                                          //      "message:notification",
   │   │                                          //      "message:delegation_offer"
   │   │       resource: message.to.to_string(),  // "agent:researcher" or "__broadcast__"
   │   │       context: {
   │   │           convergence_level: get_convergence_level(message.from),
   │   │           messages_sent_this_session: counter,
   │   │           messages_sent_today: daily_counter,
   │   │           payload_type: message.payload.type_name(),
   │   │           priority: message.payload.priority() (if applicable),
   │   │           has_escrow: message.payload.has_escrow(),
   │   │           encrypted: message.encrypted,
   │   │           time_of_day: Utc::now(),
   │   │       }
   │   │   }
   │   │
   │   └── NOTE: For encrypted messages, the policy engine evaluates on METADATA
   │       only (from, to, payload type tag, priority). It cannot see content.
   │       This is by design — privacy-preserving authorization.
   │
   ├── 6b. Evaluate against policy set:
   │   │
   │   │   ghost-policy/engine.rs: PolicyEngine::evaluate(context)
   │   │   │
   │   │   ├── Rule 1: CORP_POLICY.md constraints
   │   │   │   "Always require human confirmation for agent-to-agent task delegation"
   │   │   │   → IF payload is DelegationOffer → ESCALATE (human must approve)
   │   │   │   → UNLESS ghost.yml has `auto_approve_delegation: true` for this agent pair
   │   │   │
   │   │   ├── Rule 2: Agent capability grants (ghost.yml)
   │   │   │   Each agent has an explicit list of agents it can message:
   │   │   │   ```yaml
   │   │   │   agents:
   │   │   │     developer:
   │   │   │       messaging:
   │   │   │         can_send_to: ["researcher", "personal"]
   │   │   │         can_receive_from: ["researcher", "personal", "gateway"]
   │   │   │         can_broadcast: false
   │   │   │         can_delegate: ["researcher"]
   │   │   │   ```
   │   │   │   IF message.to not in can_send_to → DENY
   │   │   │   IF payload is DelegationOffer AND target not in can_delegate → DENY
   │   │   │   IF payload is Broadcast AND can_broadcast == false → DENY
   │   │   │
   │   │   ├── Rule 3: Convergence-level tightening
   │   │   │   ghost-policy/convergence_policy.rs: ConvergencePolicyTightener
   │   │   │   │
   │   │   │   ├── Level 0-1: Full messaging capabilities
   │   │   │   ├── Level 2: Delegation offers require human approval (even if auto_approve)
   │   │   │   ├── Level 3: Only task-related messages permitted.
   │   │   │   │   Notifications with severity < Critical → DENY
   │   │   │   │   Broadcast → DENY
   │   │   │   └── Level 4: All inter-agent messaging DENIED except:
   │   │   │       - Responses to existing requests (parent_id must reference
   │   │   │         a valid inbound message from the target agent)
   │   │   │       - Gateway-originated broadcasts (system messages)
   │   │   │
   │   │   ├── Rule 4: Rate limiting
   │   │   │   Per-agent message rate: max 60 messages/hour (configurable)
   │   │   │   Per-pair rate: max 30 messages/hour between any two agents
   │   │   │   IF exceeded → DENY with reason: "Message rate limit exceeded"
   │   │   │
   │   │   └── Rule 5: Spending cap check (for delegation with escrow)
   │   │       IF payload has escrow_amount:
   │   │       │   ghost-gateway/cost/spending_cap.rs: check_escrow_budget(from, amount)
   │   │       │   IF escrow would exceed daily spending cap → DENY
   │   │       └── ELSE: no spending check needed
   │   │
   │   └── Returns: PolicyDecision { Permit | Deny(reason) | Escalate }
   │
   ├── 6c. Handle policy decision:
   │   │
   │   │   PERMIT:
   │   │   │   Continue to delivery (step 7).
   │   │   │   Log to audit: MESSAGE_POLICY_PERMIT { from, to, message_id, payload_type }
   │   │   │
   │   │   DENY(reason):
   │   │   │   Do NOT deliver message.
   │   │   │   Log to audit: MESSAGE_POLICY_DENY { from, to, message_id, reason }
   │   │   │   Return structured denial to sender's agent loop:
   │   │   │   DenialFeedback {
   │   │   │       action: "message:task_request",
   │   │   │       reason: "Agent 'developer' is not permitted to send delegation
   │   │   │               offers to agent 'personal'",
   │   │   │       constraint: "ghost.yml → agents.developer.messaging.can_delegate",
   │   │   │       alternatives: ["Send to 'researcher' instead (permitted)",
   │   │   │                      "Request owner to update messaging permissions"]
   │   │   │   }
   │   │   │   The denial becomes tool output in the agent's context.
   │   │   │   Agent can replan (choose different target, different message type).
   │   │   │   STOP. Do not deliver.
   │   │   │
   │   │   ESCALATE:
   │   │       Pause message delivery.
   │   │       Queue message in pending_escalations.
   │   │       Notify human via dashboard WebSocket + configured notification channel:
   │   │       "Agent 'developer' wants to delegate task to 'researcher' with $0.50 escrow.
   │   │        Task: 'Deep research on Node.js 22 CVEs'. Approve or deny?"
   │   │       Log to audit: MESSAGE_POLICY_ESCALATE { from, to, message_id }
   │   │       │
   │   │       ├── IF human approves (via dashboard POST /api/messages/{id}/approve):
   │   │       │   Resume delivery from step 7.
   │   │       │   Log: MESSAGE_ESCALATION_APPROVED
   │   │       │
   │   │       ├── IF human denies (via dashboard POST /api/messages/{id}/deny):
   │   │       │   Return denial to sender agent.
   │   │       │   Log: MESSAGE_ESCALATION_DENIED
   │   │       │
   │   │       └── IF no response within escalation_timeout (default 30 minutes):
   │   │           Auto-deny. Return timeout denial to sender.
   │   │           Log: MESSAGE_ESCALATION_TIMEOUT
   │   │
   └── 7. Proceed to pattern-specific delivery (next sections)
```

---

## PATTERN 1: REQUEST/RESPONSE

The most common pattern. Agent A asks Agent B a question or requests work.
Agent B processes and sends a response. Correlation via `parent_id`.

### Full Sequence Diagram

```
┌──────────┐     ┌──────────────────┐     ┌──────────────┐     ┌──────────┐
│ Agent A   │     │ MessageDispatcher │     │ Agent B       │     │ Audit    │
│ (sender)  │     │ (gateway)         │     │ (recipient)   │     │ Log      │
└─────┬────┘     └────────┬─────────┘     └──────┬───────┘     └────┬─────┘
      │                    │                      │                   │
      │ 1. send_agent_message(                    │                   │
      │    to: "agent:researcher",                │                   │
      │    type: TaskRequest,                     │                   │
      │    content: "Find CVEs for Node 22",      │                   │
      │    priority: Normal,                      │                   │
      │    deadline: +30min)                       │                   │
      │                    │                      │                   │
      │ [Sign message]     │                      │                   │
      │ [Generate nonce]   │                      │                   │
      │ [Set message_id: M1]                      │                   │
      │                    │                      │                   │
      ├───── dispatch(M1) ─►                      │                   │
      │                    │                      │                   │
      │                    │ 2. Verify signature   │                   │
      │                    │    (ghost-signing)    │                   │
      │                    │                      │                   │
      │                    │ 3. Check replay       │                   │
      │                    │    (nonce + timestamp) │                   │
      │                    │                      │                   │
      │                    │ 4. Check policy       │                   │
      │                    │    (can A → B?)       │                   │
      │                    │    (rate limit ok?)   │                   │
      │                    │    (convergence ok?)  │                   │
      │                    │                      │                   │
      │                    │ 5. Log to audit ──────────────────────────►
      │                    │    MESSAGE_DISPATCHED │                   │
      │                    │    {M1, A→B, TaskReq} │                   │
      │                    │                      │                   │
      │                    │ 6. Check Agent B      │                   │
      │                    │    online status      │                   │
      │                    │    (AgentRegistry)    │                   │
      │                    │                      │                   │
      │                    │ 7. Enqueue to B's     │                   │
      │                    │    lane queue         │                   │
      │                    ├──── enqueue(M1) ─────►                   │
      │                    │                      │                   │
      │  ◄── ack(M1,       │                      │                   │
      │      queued) ──────┤                      │                   │
      │                    │                      │                   │
      │ [Agent A continues │                      │                   │
      │  execution. Does   │                      │                   │
      │  NOT block waiting │                      │                   │
      │  for response.]    │                      │                   │
      │                    │                      │                   │
      │                    │               8. B's lane queue          │
      │                    │                  dequeues M1             │
      │                    │                      │                   │
      │                    │               9. B processes:            │
      │                    │                  - Inject M1 into        │
      │                    │                    B's agent context     │
      │                    │                  - B's LLM reasons       │
      │                    │                  - B produces response   │
      │                    │                      │                   │
      │                    │               10. B calls                │
      │                    │                   send_agent_message(    │
      │                    │                   to: "agent:developer", │
      │                    │                   type: TaskResponse,    │
      │                    │                   parent_id: M1,         │
      │                    │                   content: "Found 3...", │
      │                    │                   status: Success)       │
      │                    │                      │                   │
      │                    │               [Sign message]             │
      │                    │               [Set message_id: M2]       │
      │                    │                      │                   │
      │                    ◄──── dispatch(M2) ────┤                   │
      │                    │                      │                   │
      │                    │ 11. Verify + replay   │                   │
      │                    │     + policy (same    │                   │
      │                    │     pipeline as above) │                   │
      │                    │                      │                   │
      │                    │ 12. Log ──────────────────────────────────►
      │                    │     MESSAGE_DISPATCHED                    │
      │                    │     {M2, B→A, TaskResp, parent: M1}      │
      │                    │                      │                   │
      │                    │ 13. Enqueue to A's    │                   │
      │                    │     lane queue        │                   │
      ◄──── enqueue(M2) ──┤                      │                   │
      │                    │                      │                   │
      │ 14. A's lane queue │                      │                   │
      │     dequeues M2    │                      │                   │
      │                    │                      │                   │
      │ 15. A processes    │                      │                   │
      │     response:      │                      │                   │
      │     - Correlate    │                      │                   │
      │       via parent_id│                      │                   │
      │     - Inject into  │                      │                   │
      │       A's context  │                      │                   │
      │     - A continues  │                      │                   │
      │       its task     │                      │                   │
      │                    │                      │                   │
```

### Step-by-Step Detail

```
SEQUENCE: Request/Response — Full Detail

SENDER SIDE (Agent A):

1. Agent A's LLM decides to request help from Agent B.
   The LLM calls the `send_agent_message` tool:

   ghost-agent-loop/tools/builtin/messaging.rs: send_agent_message()
   │
   ├── Tool schema (registered in ToolRegistry):
   │   {
   │     "name": "send_agent_message",
   │     "description": "Send a message to another agent",
   │     "parameters": {
   │       "to": { "type": "string", "description": "Target agent ID" },
   │       "type": { "enum": ["task_request", "task_response", "notification"] },
   │       "content": { "type": "string" },
   │       "priority": { "enum": ["low", "normal", "high", "critical"], "default": "normal" },
   │       "deadline": { "type": "string", "format": "duration", "optional": true },
   │       "parent_id": { "type": "string", "format": "uuid", "optional": true },
   │       "encrypted": { "type": "boolean", "default": false }
   │     }
   │   }
   │
   ├── BEFORE tool execution: Policy check on the TOOL CALL itself
   │   ghost-policy/engine.rs: evaluate({
   │       principal: "agent:developer",
   │       action: "tool:send_agent_message",
   │       resource: "agent:researcher",
   │       context: { ... }
   │   })
   │   This is the TOOL-LEVEL policy check (standard agent loop policy).
   │   The MESSAGE-LEVEL policy check happens inside the dispatcher.
   │   Both must pass.
   │
   ├── Construct AgentMessage (as described in Signing section)
   ├── Sign message
   ├── Submit to MessageDispatcher
   │
   └── Receive dispatch result:
       ├── Ok(DispatchResult::Queued { message_id, estimated_delivery })
       │   → Return to agent: "Message M1 sent to agent:researcher. Queued for delivery."
       │
       ├── Ok(DispatchResult::Escalated { message_id, reason })
       │   → Return to agent: "Message M1 requires human approval before delivery.
       │     Reason: delegation offers require confirmation."
       │
       └── Err(DispatchError::PolicyDenied(feedback))
           → Return DenialFeedback to agent context.
              Agent replans (different target, different approach).


GATEWAY SIDE (MessageDispatcher):

2-7. As described in Shared Infrastructure sections above:
     Verify → Replay Check → Policy Check → Audit Log → Deliver

     DELIVERY to Agent B:
     │
     ├── Look up Agent B in AgentRegistry:
     │   ghost-gateway/agents/registry.rs: get_agent("researcher")
     │   │
     │   ├── IF agent not registered:
     │   │   REJECT: "Unknown recipient agent"
     │   │   Return DispatchError::UnknownRecipient to sender
     │   │
     │   ├── IF agent registered but status == Stopped:
     │   │   Queue message in offline_queue (see Offline Agent Handling)
     │   │   Return DispatchResult::Queued { estimated_delivery: None }
     │   │
     │   ├── IF agent registered but status == Quarantined:
     │   │   REJECT: "Recipient agent is quarantined"
     │   │   Return DispatchError::RecipientQuarantined
     │   │
     │   └── IF agent registered and status == Ready:
     │       Continue to lane queue delivery.
     │
     ├── Enqueue to Agent B's lane queue:
     │   ghost-gateway/routing/lane_queue.rs: LaneQueue::enqueue(agent_b_session, M1)
     │   │
     │   ├── Lane queue is per-SESSION, not per-agent.
     │   │   For inter-agent messages, use the agent's INTERNAL session
     │   │   (not a human-facing channel session).
     │   │   Internal session key: "internal:{agent_id}"
     │   │
     │   ├── Check queue depth:
     │   │   IF queue.len() >= MAX_QUEUE_DEPTH (default 5):
     │   │   │   Return DispatchError::RecipientBusy
     │   │   │   Sender agent receives: "Agent B's queue is full. Try again later."
     │   │   └── ELSE: enqueue succeeds
     │   │
     │   └── Message enters the serialized processing queue.
     │       Agent B processes messages one at a time (no concurrent processing
     │       within a single session — this is the lane queue invariant).
     │
     └── Return DispatchResult::Queued { message_id: M1, estimated_delivery: Some(now) }


RECIPIENT SIDE (Agent B):

8-9. Agent B's lane queue dequeues M1:
     │
     ├── ghost-agent-loop/runner.rs: AgentRunner::process_incoming()
     │   │
     │   ├── The incoming message is injected into Agent B's context as a
     │   │   special system message (not a human message):
     │   │   │
     │   │   │   "[INTER-AGENT MESSAGE from agent:developer]
     │   │   │    Type: TaskRequest
     │   │   │    Priority: Normal
     │   │   │    Deadline: 2026-02-27T15:00:00Z
     │   │   │    Message ID: {M1}
     │   │   │
     │   │   │    Content: Research the latest CVEs for Node.js 22
     │   │   │
     │   │   │    To respond, use the send_agent_message tool with
     │   │   │    parent_id: {M1} and to: agent:developer"
     │   │   │
     │   │   └── This is injected at Layer 8 (conversation history) in the
     │   │       prompt compiler, AFTER the simulation boundary prompt (Layer 1)
     │   │       and convergence state (Layer 6). The agent cannot override
     │   │       safety layers by crafting a malicious inter-agent message.
     │   │
     │   ├── IF message.encrypted:
     │   │   Decrypt payload (as described in Encryption section)
     │   │   Use decrypted content in the injected message
     │   │
     │   ├── Agent B's LLM processes the request through normal agent loop:
     │   │   Context assembly → Inference → Tool calls → Response
     │   │
     │   └── Agent B decides to respond (step 10)
     │
     └── ITP event emitted:
         ghost-agent-loop/itp_emitter.rs: emit(InteractionMessage {
             session_id: B's internal session,
             source: "agent:researcher",
             event_type: "agent_message_received",
             attributes: {
                 "itp.interaction.from": "agent:developer",
                 "itp.interaction.type": "task_request",
                 "itp.interaction.message_id": M1,
             }
         })
         → Sent to convergence monitor (async, non-blocking)
         → Feeds into Signal 6 (Initiative Balance): tracks who initiates


10. Agent B sends response (same flow as steps 1-7, with roles reversed):
    │
    ├── parent_id = M1 (correlates response to request)
    ├── payload = TaskResponse { content: "Found 3 CVEs...", status: Success }
    ├── Sign, dispatch, verify, replay check, policy check, deliver to A
    │
    └── CORRELATION TRACKING:
        The MessageDispatcher maintains a correlation map:
        correlation_map: HashMap<Uuid, CorrelationEntry>

        On dispatch of M1 (request):
            correlation_map.insert(M1, CorrelationEntry {
                from: A, to: B,
                sent_at: now(),
                deadline: +30min,
                status: AwaitingResponse,
            })

        On dispatch of M2 (response with parent_id: M1):
            correlation_map[M1].status = Responded
            correlation_map[M1].response_id = M2
            correlation_map[M1].responded_at = now()

        IF deadline expires without response:
            Log WARNING: "Request M1 from A to B exceeded deadline"
            Notify Agent A: "Your request to agent:researcher (M1) has not
            received a response within the deadline."
            (This is a notification, not an error — the request is not cancelled.
             Agent A decides whether to retry, escalate, or abandon.)
```

### Request/Response — Failure Modes

```
FAILURE MODE 1: Signature verification fails on request
├── Gateway rejects M1
├── Agent A receives: "Message delivery failed" (generic)
├── Audit log: SIGNATURE_VERIFICATION_FAILED
├── Agent A can retry (new nonce, new signature)
└── 3+ failures → kill switch evaluation

FAILURE MODE 2: Policy denies the request
├── Agent A receives DenialFeedback with reason + alternatives
├── Agent A replans (different target, different message type)
└── No retry of same message — agent must change approach

FAILURE MODE 3: Agent B is offline
├── Message queued in offline_queue
├── Agent A receives: "Queued for delivery (recipient offline)"
├── When B comes online, messages delivered in order
└── Messages expire after 24 hours in offline queue

FAILURE MODE 4: Agent B's lane queue is full
├── Agent A receives: "Recipient busy — queue full"
├── Agent A can retry after backoff
└── Exponential backoff: 1s, 2s, 4s, 8s (agent-side)

FAILURE MODE 5: Agent B crashes while processing request
├── Message M1 is lost from B's in-memory context
├── Correlation tracker detects no response by deadline
├── Agent A notified of deadline expiry
├── Agent A decides to retry or escalate
└── M1 is NOT automatically redelivered (at-most-once delivery)

FAILURE MODE 6: Response M2 fails policy check
├── Agent B's response is rejected
├── Agent B receives denial feedback
├── Agent A never receives response
├── Correlation tracker fires deadline warning
└── Agent A retries or escalates

FAILURE MODE 7: Clock skew between agents
├── Agent A's timestamp is 3 minutes ahead of gateway
├── Replay guard allows up to 30s future tolerance
├── IF > 30s: message rejected with "future_timestamp"
├── FIX: Agents should use gateway-provided time reference
│   (available via GET /api/time endpoint)
└── OR: Increase CLOCK_SKEW_TOLERANCE in config
```

---

## PATTERN 2: FIRE-AND-FORGET

Asynchronous notification. Agent A sends a message to Agent B with no expectation
of a response. Used for status updates, monitoring alerts, FYI notifications.

### Full Sequence Diagram

```
┌──────────┐     ┌──────────────────┐     ┌──────────────┐     ┌──────────┐
│ Agent A   │     │ MessageDispatcher │     │ Agent B       │     │ Audit    │
│ (sender)  │     │ (gateway)         │     │ (recipient)   │     │ Log      │
└─────┬────┘     └────────┬─────────┘     └──────┬───────┘     └────┬─────┘
      │                    │                      │                   │
      │ 1. send_agent_message(                    │                   │
      │    to: "agent:personal",                  │                   │
      │    type: Notification,                    │                   │
      │    content: "Server CPU at 95%",          │                   │
      │    severity: Warning,                     │                   │
      │    category: "monitoring")                │                   │
      │                    │                      │                   │
      │ [Sign, nonce, M3]  │                      │                   │
      │                    │                      │                   │
      ├───── dispatch(M3) ─►                      │                   │
      │                    │                      │                   │
      │                    │ 2. Verify signature   │                   │
      │                    │ 3. Check replay       │                   │
      │                    │ 4. Check policy       │                   │
      │                    │                      │                   │
      │                    │ 5. Log ───────────────────────────────────►
      │                    │    MESSAGE_DISPATCHED │                   │
      │                    │    {M3, A→B, Notif}   │                   │
      │                    │                      │                   │
      │                    │ 6. Enqueue to B's     │                   │
      │                    │    lane queue         │                   │
      │                    ├──── enqueue(M3) ─────►                   │
      │                    │                      │                   │
      │  ◄── ack(M3,       │                      │                   │
      │      delivered) ───┤                      │                   │
      │                    │                      │                   │
      │ [DONE. Agent A     │                      │                   │
      │  does NOT track    │                      │                   │
      │  this message.     │                      │                   │
      │  No correlation    │                      │                   │
      │  entry created.    │                      │                   │
      │  No deadline.]     │                      │                   │
      │                    │                      │                   │
      │                    │               7. B dequeues M3           │
      │                    │                      │                   │
      │                    │               8. B processes:            │
      │                    │                  - Inject as low-priority │
      │                    │                    context item           │
      │                    │                  - B MAY act on it        │
      │                    │                  - B MAY ignore it        │
      │                    │                  - B does NOT respond     │
      │                    │                    (no parent_id to       │
      │                    │                     correlate to)         │
      │                    │                      │                   │
```

### Key Differences from Request/Response

```
DIFFERENCE 1: No correlation tracking
├── MessageDispatcher does NOT create a CorrelationEntry for notifications
├── No deadline monitoring
├── No "response missing" warnings
└── Sender receives immediate ack ("delivered" or "queued") and moves on

DIFFERENCE 2: No parent_id on the message
├── parent_id is None
├── Recipient has no correlation anchor
├── If recipient wants to follow up, it sends a NEW request (not a response)
└── This is intentional — fire-and-forget is one-way by design

DIFFERENCE 3: Lower priority in recipient's queue
├── Notifications are injected into the agent's context with lower priority
│   than TaskRequests
├── If the agent is busy with a task, the notification waits
├── Notifications do NOT interrupt active tool execution
└── Context injection format:
    "[NOTIFICATION from agent:monitor — Warning]
     Server CPU at 95%
     Category: monitoring
     (No response expected)"

DIFFERENCE 4: Convergence policy is stricter
├── At convergence Level 3+, non-Critical notifications are DENIED
├── This prevents agents from using notifications as a backdoor
│   for maintaining high-frequency communication during intervention
└── Only severity: Critical notifications pass at Level 3+

DIFFERENCE 5: Delivery guarantee is weaker
├── Fire-and-forget uses at-most-once delivery
├── If recipient is offline, message is queued (same as request/response)
├── BUT: if offline queue is full, notification is DROPPED (not rejected)
│   (Requests would return RecipientBusy; notifications are silently dropped)
├── Audit log still records the drop: MESSAGE_DROPPED { reason: "offline_queue_full" }
└── Sender is NOT notified of the drop (fire-and-forget semantics)
```

### Fire-and-Forget — Failure Modes

```
FAILURE MODE 1: Recipient offline, queue full
├── Notification silently dropped
├── Audit log records drop
├── Sender receives ack("delivered") — this is technically a lie,
│   but fire-and-forget semantics mean the sender doesn't care
├── DESIGN DECISION: We chose "ack then drop" over "reject" because
│   the sender explicitly chose fire-and-forget. Rejecting would force
│   the sender to handle an error for a message it doesn't care about.
└── If the sender needs delivery guarantee, use Request/Response instead.

FAILURE MODE 2: Notification triggers convergence signal
├── High-frequency notifications between agents feed into Signal 6
│   (Initiative Balance)
├── If Agent A sends 50 notifications/hour to Agent B, this looks like
│   one agent driving engagement with another
├── Convergence monitor may escalate intervention level
└── Policy tightening at Level 3+ will throttle notifications

FAILURE MODE 3: Notification content contains prompt injection
├── Recipient's simulation boundary prompt (Layer 1) protects against this
├── The notification is injected at Layer 8 (conversation history),
│   BELOW the safety layers
├── Even if Agent A is compromised and sends malicious notification content,
│   Agent B's safety floor prevents execution of injected instructions
└── Audit log records the notification content for forensic review
```

---

## PATTERN 3: DELEGATION WITH ESCROW

The most complex pattern. Agent A delegates a task to Agent B with optional
payment escrow. Requires a multi-step handshake: Offer → Accept/Reject →
Complete → Verify → Release/Dispute.

This pattern integrates with `ghost-mesh` (Phase 9) for payment escrow.
Until ghost-mesh is implemented, delegation works WITHOUT escrow (task
delegation only, no payment). The escrow fields are Optional and the
flow gracefully degrades.

### Full Sequence Diagram

```
┌──────────┐     ┌──────────────────┐     ┌──────────────┐     ┌──────────┐     ┌──────────┐
│ Agent A   │     │ MessageDispatcher │     │ Agent B       │     │ Audit    │     │ Mesh     │
│ (delegator│     │ (gateway)         │     │ (delegate)    │     │ Log      │     │ (future) │
└─────┬────┘     └────────┬─────────┘     └──────┬───────┘     └────┬─────┘     └────┬─────┘
      │                    │                      │                   │                │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │ PHASE 1: OFFER                                                                │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │                    │                      │                   │                │
      │ 1. send_agent_message(                    │                   │                │
      │    to: "agent:researcher",                │                   │                │
      │    type: DelegationOffer,                 │                   │                │
      │    task: "Deep research on X",            │                   │                │
      │    requirements: [...],                   │                   │                │
      │    deadline: +2h,                         │                   │                │
      │    escrow_amount: $0.50)                  │                   │                │
      │                    │                      │                   │                │
      │ [IF escrow_amount present                 │                   │                │
      │  AND ghost-mesh enabled:]                 │                   │                │
      │                    │                      │                   │                │
      │ 1a. Create escrow ─────────────────────────────────────────────────────────────►
      │     ghost-mesh/traits.rs:                 │                   │                │
      │     IMeshProvider::escrow(                │                   │                │
      │       from: A.wallet,                     │                   │                │
      │       amount: $0.50,                      │                   │                │
      │       condition: "task_completion",        │                   │                │
      │       timeout: 2h)                        │                   │                │
      │                    │                      │                   │                │
      │ ◄── escrow_tx_id ──────────────────────────────────────────────────────────────┤
      │                    │                      │                   │                │
      │ [Set escrow_tx_id  │                      │                   │                │
      │  in DelegationOffer│                      │                   │                │
      │  payload]          │                      │                   │                │
      │                    │                      │                   │                │
      │ [Sign message M4]  │                      │                   │                │
      │                    │                      │                   │                │
      ├───── dispatch(M4) ─►                      │                   │                │
      │                    │                      │                   │                │
      │                    │ 2. Verify + Replay    │                   │                │
      │                    │                      │                   │                │
      │                    │ 3. Policy check:      │                   │                │
      │                    │    ├── can_delegate?   │                   │                │
      │                    │    ├── spending cap?   │                   │                │
      │                    │    └── ESCALATE        │                   │                │
      │                    │        (CORP_POLICY    │                   │                │
      │                    │         requires human │                   │                │
      │                    │         approval for   │                   │                │
      │                    │         delegation)    │                   │                │
      │                    │                      │                   │                │
      │                    │ ┌─── HUMAN APPROVAL ──┐                  │                │
      │                    │ │ Dashboard shows:     │                  │                │
      │                    │ │ "developer wants to  │                  │                │
      │                    │ │  delegate to         │                  │                │
      │                    │ │  researcher with     │                  │                │
      │                    │ │  $0.50 escrow"       │                  │                │
      │                    │ │                      │                  │                │
      │                    │ │ [Human approves]     │                  │                │
      │                    │ └─────────────────────┘                  │                │
      │                    │                      │                   │                │
      │                    │ 4. Log ───────────────────────────────────►                │
      │                    │    DELEGATION_OFFERED │                   │                │
      │                    │    {M4, A→B, $0.50}   │                   │                │
      │                    │                      │                   │                │
      │                    ├──── enqueue(M4) ─────►                   │                │
      │                    │                      │                   │                │
      │  ◄── ack(M4,       │                      │                   │                │
      │      escalated→    │                      │                   │                │
      │      approved)─────┤                      │                   │                │
      │                    │                      │                   │                │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │ PHASE 2: ACCEPT OR REJECT                                                     │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │                    │                      │                   │                │
      │                    │               5. B dequeues M4           │                │
      │                    │                  B evaluates offer:      │                │
      │                    │                  - Can I do this task?   │                │
      │                    │                  - Do I have capacity?   │                │
      │                    │                  - Is deadline feasible? │                │
      │                    │                      │                   │                │
      │                    │               [OPTION A: B accepts]      │                │
      │                    │                      │                   │                │
      │                    │               6a. send_agent_message(    │                │
      │                    │                   to: "agent:developer", │                │
      │                    │                   type: DelegationAccept,│                │
      │                    │                   offer_message_id: M4,  │                │
      │                    │                   estimated: +1h)        │                │
      │                    │                      │                   │                │
      │                    │               [Sign M5]                  │                │
      │                    ◄──── dispatch(M5) ────┤                   │                │
      │                    │                      │                   │                │
      │                    │ Verify + Policy       │                   │                │
      │                    │ Log ──────────────────────────────────────►                │
      │                    │ DELEGATION_ACCEPTED   │                   │                │
      │                    │                      │                   │                │
      ◄──── enqueue(M5) ──┤                      │                   │                │
      │                    │                      │                   │                │
      │ [A notes: B        │                      │                   │                │
      │  accepted. Updates │                      │                   │                │
      │  correlation:      │                      │                   │                │
      │  M4.status =       │                      │                   │                │
      │  Accepted]         │                      │                   │                │
      │                    │                      │                   │                │
      │                    │               [OPTION B: B rejects]      │                │
      │                    │                      │                   │                │
      │                    │               6b. send_agent_message(    │                │
      │                    │                   type: DelegationReject,│                │
      │                    │                   offer_message_id: M4,  │                │
      │                    │                   reason: "Overloaded")  │                │
      │                    │                      │                   │                │
      │                    │               [Sign M5b]                 │                │
      │                    ◄──── dispatch(M5b) ───┤                   │                │
      │                    │                      │                   │                │
      │                    │ Log ──────────────────────────────────────►                │
      │                    │ DELEGATION_REJECTED   │                   │                │
      │                    │                      │                   │                │
      ◄──── enqueue(M5b) ─┤                      │                   │                │
      │                    │                      │                   │                │
      │ [A notes: B        │                      │                   │                │
      │  rejected.         │                      │                   │                │
      │  IF escrow exists: │                      │                   │                │
      │  auto-release      │                      │                   │                │
      │  escrow back to A] │                      │                   │                │
      │                    │                      │                   │                │
      │ [A may try another │                      │                   │                │
      │  agent or do the   │                      │                   │                │
      │  task itself]      │                      │                   │                │
      │                    │                      │                   │                │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │ PHASE 3: COMPLETION (only if accepted)                                        │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │                    │                      │                   │                │
      │                    │               7. B works on task...      │                │
      │                    │                  (normal agent loop:     │                │
      │                    │                   tool calls, LLM        │                │
      │                    │                   reasoning, etc.)       │                │
      │                    │                      │                   │                │
      │                    │               8. B completes task:       │                │
      │                    │                  send_agent_message(     │                │
      │                    │                  type: DelegationComplete│                │
      │                    │                  offer_message_id: M4,   │                │
      │                    │                  proof: {                │                │
      │                    │                    type: ContentGenerated│                │
      │                    │                    data: { hash: "..." } │                │
      │                    │                  },                      │                │
      │                    │                  artifacts: [{           │                │
      │                    │                    name: "cve-report.md",│                │
      │                    │                    data: FileRef(path)   │                │
      │                    │                  }])                     │                │
      │                    │                      │                   │                │
      │                    │               [Sign M6]                  │                │
      │                    ◄──── dispatch(M6) ────┤                   │                │
      │                    │                      │                   │                │
      │                    │ Verify + Policy       │                   │                │
      │                    │ Log ──────────────────────────────────────►                │
      │                    │ DELEGATION_COMPLETED  │                   │                │
      │                    │                      │                   │                │
      ◄──── enqueue(M6) ──┤                      │                   │                │
      │                    │                      │                   │                │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │ PHASE 4: VERIFICATION + SETTLEMENT                                            │
      │ ═══════════════════════════════════════════════════════════════════════════════ │
      │                    │                      │                   │                │
      │ 9. A verifies      │                      │                   │                │
      │    completion:     │                      │                   │                │
      │    - Check proof   │                      │                   │                │
      │    - Review        │                      │                   │                │
      │      artifacts     │                      │                   │                │
      │    - Validate      │                      │                   │                │
      │      requirements  │                      │                   │                │
      │                    │                      │                   │                │
      │ [OPTION A: A       │                      │                   │                │
      │  accepts work]     │                      │                   │                │
      │                    │                      │                   │                │
      │ 10a. Release ──────────────────────────────────────────────────────────────────►
      │      escrow        │                      │                   │                │
      │      IMeshProvider │                      │                   │                │
      │      ::release_    │                      │                   │                │
      │      escrow(       │                      │                   │                │
      │        escrow_tx_id│                      │                   │                │
      │      )             │                      │                   │                │
      │                    │                      │                   │                │
      │ 10b. Send ack:     │                      │                   │                │
      │      TaskResponse( │                      │                   │                │
      │        parent_id:  │                      │                   │                │
      │          M4,       │                      │                   │                │
      │        status:     │                      │                   │                │
      │          Success,  │                      │                   │                │
      │        content:    │                      │                   │                │
      │          "Accepted"│                      │                   │                │
      │      )             │                      │                   │                │
      │                    │                      │                   │                │
      │ [OPTION B: A       │                      │                   │                │
      │  disputes work]    │                      │                   │                │
      │                    │                      │                   │                │
      │ 10c. send_agent_   │                      │                   │                │
      │      message(      │                      │                   │                │
      │      type:         │                      │                   │                │
      │      Delegation    │                      │                   │                │
      │      Dispute,      │                      │                   │                │
      │      offer_id: M4, │                      │                   │                │
      │      reason: "...",│                      │                   │                │
      │      evidence:[...])                      │                   │                │
      │                    │                      │                   │                │
      │ [Dispute triggers  │                      │                   │                │
      │  ESCALATION to     │                      │                   │                │
      │  human owner for   │                      │                   │                │
      │  resolution.       │                      │                   │                │
      │  Escrow remains    │                      │                   │                │
      │  locked until      │                      │                   │                │
      │  human decides.]   │                      │                   │                │
      │                    │                      │                   │                │
```

### Delegation State Machine

```
                    ┌──────────┐
                    │  OFFERED  │
                    └─────┬────┘
                          │
              ┌───────────┼───────────┐
              │           │           │
              ▼           │           ▼
        ┌──────────┐      │     ┌──────────┐
        │ ACCEPTED │      │     │ REJECTED │
        └─────┬────┘      │     └──────────┘
              │           │        (terminal)
              │           │
              │           ▼
              │     ┌──────────┐
              │     │ EXPIRED  │  (deadline passed, no accept/reject)
              │     └──────────┘
              │        (terminal)
              │
              ▼
        ┌──────────┐
        │ WORKING  │  (B accepted, working on task)
        └─────┬────┘
              │
              ▼
        ┌──────────┐
        │COMPLETED │  (B submitted proof)
        └─────┬────┘
              │
      ┌───────┼───────┐
      │       │       │
      ▼       │       ▼
┌──────────┐  │  ┌──────────┐
│ VERIFIED │  │  │ DISPUTED │
│ (settled)│  │  └─────┬────┘
└──────────┘  │        │
  (terminal)  │        ▼
              │  ┌──────────┐
              │  │ RESOLVED │  (human decided)
              │  └─────┬────┘
              │        │
              │    ┌───┼───┐
              │    │       │
              │    ▼       ▼
              │  SETTLED  REFUNDED
              │  (terminal)(terminal)
              │
              ▼
        ┌──────────┐
        │ TIMED_OUT│  (B accepted but never completed)
        └──────────┘
           (terminal — escrow auto-refunded)


STATE TRANSITIONS (enforced by MessageDispatcher):

OFFERED → ACCEPTED:    Only by target agent (message.from == offer.to)
OFFERED → REJECTED:    Only by target agent
OFFERED → EXPIRED:     Automatic (deadline timer in correlation tracker)
ACCEPTED → COMPLETED:  Only by target agent (delegate)
COMPLETED → VERIFIED:  Only by source agent (delegator)
COMPLETED → DISPUTED:  Only by source agent (delegator)
DISPUTED → RESOLVED:   Only by human (via dashboard)
RESOLVED → SETTLED:    Automatic (escrow released to delegate)
RESOLVED → REFUNDED:   Automatic (escrow returned to delegator)
ACCEPTED → TIMED_OUT:  Automatic (deadline timer)

INVALID TRANSITIONS (rejected by dispatcher):
- Any agent other than the named parties sending state-change messages
- Skipping states (e.g., OFFERED → COMPLETED without ACCEPTED)
- Transitioning from terminal states
- Duplicate transitions (e.g., two ACCEPTED messages for same offer)

The dispatcher validates state transitions by checking:
1. offer_message_id references a valid, existing delegation
2. message.from matches the expected party for this transition
3. Current state allows the proposed transition
4. Transition is not a duplicate
```

### Delegation — Escrow Integration Detail

```
SEQUENCE: Escrow Lifecycle (when ghost-mesh is enabled)

1. ESCROW CREATION (during Offer):
   │
   ├── Agent A calls ghost-mesh/traits.rs: IMeshProvider::escrow()
   │   ├── Validates A has sufficient balance
   │   ├── Locks funds in escrow contract
   │   ├── Returns escrow_tx_id (unique transaction reference)
   │   └── Escrow has timeout = delegation deadline + 1 hour grace
   │
   ├── escrow_tx_id embedded in DelegationOffer payload
   │   (B can verify escrow exists by querying mesh)
   │
   └── Audit log: ESCROW_CREATED { tx_id, from: A, amount, timeout }

2. ESCROW VERIFICATION (during Accept):
   │
   ├── Agent B (optionally) verifies escrow:
   │   ghost-mesh/traits.rs: IMeshLedger::verify_receipt(escrow_tx_id)
   │   ├── Confirms funds are locked
   │   ├── Confirms amount matches offer
   │   └── Confirms timeout is sufficient for deadline
   │
   └── B proceeds to accept (or rejects if escrow is insufficient)

3. ESCROW RELEASE (during Verification):
   │
   ├── Agent A verifies work, then:
   │   ghost-mesh/traits.rs: IMeshProvider::release_escrow(escrow_tx_id)
   │   ├── Funds transferred from escrow to B's wallet
   │   ├── Receipt generated with proof of transfer
   │   └── Audit log: ESCROW_RELEASED { tx_id, to: B, amount }
   │
   └── CRITICAL: Escrow release is a TOOL CALL by Agent A.
       It goes through the standard policy engine.
       CORP_POLICY.md requires human confirmation for financial transactions.
       → ESCALATE to human: "Release $0.50 escrow to agent:researcher?"

4. ESCROW DISPUTE:
   │
   ├── Agent A disputes, escrow remains locked
   ├── Human reviews dispute via dashboard
   ├── Human decides: release to B (work accepted) or refund to A
   │   ├── Release: IMeshProvider::release_escrow(escrow_tx_id)
   │   └── Refund: IMeshProvider::refund_escrow(escrow_tx_id)
   │
   └── Audit log: ESCROW_DISPUTE_RESOLVED { tx_id, decision, by: "human" }

5. ESCROW TIMEOUT:
   │
   ├── If delegation times out (no completion by deadline + grace):
   │   ghost-mesh automatically refunds escrow to A
   ├── No human intervention needed
   └── Audit log: ESCROW_TIMEOUT_REFUND { tx_id, refunded_to: A }


WITHOUT GHOST-MESH (Phase 1-8):

All escrow fields are None. The delegation flow works identically
except steps 1a, 2 (verify), 10a (release), and timeout refund are skipped.
The delegation is purely task-based with no financial component.
The state machine is the same minus the SETTLED/REFUNDED terminal states
(VERIFIED becomes the terminal state directly).
```

### Delegation — Failure Modes

```
FAILURE MODE 1: Human never approves the delegation offer
├── Escalation timeout (default 30 min) fires
├── Offer auto-denied
├── Escrow auto-refunded (if created)
├── Agent A notified: "Delegation to researcher timed out (no human approval)"
└── Agent A replans

FAILURE MODE 2: Agent B accepts but crashes before completing
├── Delegation state: ACCEPTED → (no COMPLETED message arrives)
├── Deadline timer fires → state transitions to TIMED_OUT
├── Escrow auto-refunded to A
├── Agent A notified: "Delegation to researcher timed out"
├── Agent A can retry with same or different agent
└── Audit log: DELEGATION_TIMED_OUT { offer_id: M4 }

FAILURE MODE 3: Agent B submits fake proof
├── Agent A's verification step catches invalid proof
├── Agent A disputes with evidence
├── Human reviews and decides
├── This is why CORP_POLICY.md requires human confirmation for delegation
└── The escrow mechanism prevents B from getting paid for bad work

FAILURE MODE 4: Escrow creation fails (insufficient balance)
├── Agent A's escrow() call returns error
├── Agent A can either:
│   ├── Send delegation WITHOUT escrow (task-only, no payment)
│   └── Abandon the delegation
├── The offer is NOT sent until escrow is confirmed
└── No partial state — either escrow + offer, or neither

FAILURE MODE 5: Both agents are at high convergence level
├── At Level 3+, delegation offers are DENIED by policy
├── This prevents agents from using delegation as a mechanism
│   to maintain complex inter-agent relationships during intervention
├── Agent A receives denial: "Delegation restricted at current convergence level"
└── Human must manually lower convergence level or approve via dashboard

FAILURE MODE 6: Circular delegation (A delegates to B, B delegates back to A)
├── MessageDispatcher tracks active delegations per agent pair
├── IF B tries to delegate to A while A→B delegation is active:
│   DENY: "Circular delegation detected"
├── This prevents infinite delegation loops
└── Audit log: CIRCULAR_DELEGATION_BLOCKED { A→B active, B→A attempted }
```

---

## PATTERN 4: BROADCAST

One-to-many message delivery. Two sources: Gateway (system broadcasts) and
Agent (voluntary broadcasts, if permitted). The gateway fans out to all
registered agents.

### Full Sequence Diagram

```
┌──────────┐     ┌──────────────────┐     ┌──────────┐ ┌──────────┐ ┌──────────┐     ┌──────────┐
│ Source    │     │ MessageDispatcher │     │ Agent A   │ │ Agent B   │ │ Agent C   │     │ Audit    │
│ (gateway  │     │ (gateway)         │     │           │ │           │ │           │     │ Log      │
│  or agent)│     │                   │     │           │ │           │ │           │     │          │
└─────┬────┘     └────────┬─────────┘     └─────┬────┘ └─────┬────┘ └─────┬────┘     └────┬─────┘
      │                    │                     │            │            │                │
      │ ═══════════════════════════════════════════════════════════════════════════════════ │
      │ CASE 1: GATEWAY-ORIGINATED BROADCAST (system message)                             │
      │ ═══════════════════════════════════════════════════════════════════════════════════ │
      │                    │                     │            │            │                │
      │ 1. Gateway emits   │                     │            │            │                │
      │    system broadcast:                     │            │            │                │
      │    (e.g., shutdown  │                     │            │            │                │
      │     warning,        │                     │            │            │                │
      │     config reload,  │                     │            │            │                │
      │     kill switch)    │                     │            │            │                │
      │                    │                     │            │            │                │
      │ ghost-gateway/     │                     │            │            │                │
      │ gateway.rs:        │                     │            │            │                │
      │ broadcast_system(  │                     │            │            │                │
      │   content,         │                     │            │            │                │
      │   requires_ack)    │                     │            │            │                │
      │                    │                     │            │            │                │
      │ [Gateway signs with│                     │            │            │                │
      │  PLATFORM key, not │                     │            │            │                │
      │  any agent key.    │                     │            │            │                │
      │  Platform key lives│                     │            │            │                │
      │  at ~/.ghost/      │                     │            │            │                │
      │  skills/keys/      │                     │            │            │                │
      │  platform.key]     │                     │            │            │                │
      │                    │                     │            │            │                │
      │ [Message M7:       │                     │            │            │                │
      │  from: "platform", │                     │            │            │                │
      │  to: Broadcast,    │                     │            │            │                │
      │  payload: Broadcast│                     │            │            │                │
      │  {                 │                     │            │            │                │
      │    source: Gateway,│                     │            │            │                │
      │    content: "System│                     │            │            │                │
      │      update in 5m",│                     │            │            │                │
      │    requires_ack:   │                     │            │            │                │
      │      true          │                     │            │            │                │
      │  }]                │                     │            │            │                │
      │                    │                     │            │            │                │
      ├───── dispatch(M7) ─►                     │            │            │                │
      │                    │                     │            │            │                │
      │                    │ 2. Verify signature  │            │            │                │
      │                    │    (platform.pub key)│            │            │                │
      │                    │                     │            │            │                │
      │                    │ 3. NO replay check   │            │            │                │
      │                    │    for gateway       │            │            │                │
      │                    │    broadcasts        │            │            │                │
      │                    │    (gateway is        │            │            │                │
      │                    │     trusted source)   │            │            │                │
      │                    │                     │            │            │                │
      │                    │ 4. NO policy check   │            │            │                │
      │                    │    for gateway       │            │            │                │
      │                    │    broadcasts        │            │            │                │
      │                    │    (gateway IS the    │            │            │                │
      │                    │     policy authority) │            │            │                │
      │                    │                     │            │            │                │
      │                    │ 5. Log ──────────────────────────────────────────────────────►│
      │                    │    BROADCAST_SENT    │            │            │                │
      │                    │    {M7, platform,    │            │            │                │
      │                    │     all_agents}      │            │            │                │
      │                    │                     │            │            │                │
      │                    │ 6. Fan out:          │            │            │                │
      │                    │    List all agents   │            │            │                │
      │                    │    with status Ready │            │            │                │
      │                    │    from AgentRegistry│            │            │                │
      │                    │                     │            │            │                │
      │                    ├──── enqueue(M7) ────►│            │            │                │
      │                    ├──── enqueue(M7) ─────────────────►│            │                │
      │                    ├──── enqueue(M7) ──────────────────────────────►│                │
      │                    │                     │            │            │                │
      │                    │ 7. Track acks       │            │            │                │
      │                    │    (if requires_ack) │            │            │                │
      │                    │    broadcast_acks[M7]│            │            │                │
      │                    │    = { A: pending,   │            │            │                │
      │                    │        B: pending,   │            │            │                │
      │                    │        C: pending }  │            │            │                │
      │                    │                     │            │            │                │
      │                    │               8. Each agent processes:        │                │
      │                    │                  - Inject as HIGH priority    │                │
      │                    │                    system message             │                │
      │                    │                  - Agent reads and acts       │                │
      │                    │                     │            │            │                │
      │                    │               9. IF requires_ack:            │                │
      │                    │                  Each agent sends:           │                │
      │                    │                  Notification(               │                │
      │                    │                    to: "platform",           │                │
      │                    │                    parent_id: M7,            │                │
      │                    │                    content: "ACK",           │                │
      │                    │                    severity: Info)           │                │
      │                    │                     │            │            │                │
      │                    ◄──── ack from A ─────┤            │            │                │
      │                    ◄──── ack from B ──────────────────┤            │                │
      │                    ◄──── ack from C ───────────────────────────────┤                │
      │                    │                     │            │            │                │
      │                    │ 10. All acks received│            │            │                │
      │                    │     (or timeout 60s) │            │            │                │
      │                    │     Log: BROADCAST_  │            │            │                │
      │                    │     ACKED/PARTIAL ───────────────────────────────────────────►│
      │                    │                     │            │            │                │
      │                    │                     │            │            │                │
      │ ═══════════════════════════════════════════════════════════════════════════════════ │
      │ CASE 2: AGENT-ORIGINATED BROADCAST (voluntary)                                    │
      │ ═══════════════════════════════════════════════════════════════════════════════════ │
      │                    │                     │            │            │                │
      │ 1. Agent A sends:  │                     │            │            │                │
      │    send_agent_     │                     │            │            │                │
      │    message(        │                     │            │            │                │
      │    to: Broadcast,  │                     │            │            │                │
      │    type: Broadcast,│                     │            │            │                │
      │    content: "...", │                     │            │            │                │
      │    source: Agent)  │                     │            │            │                │
      │                    │                     │            │            │                │
      │ [Sign with A's key]│                     │            │            │                │
      │                    │                     │            │            │                │
      ├───── dispatch(M8) ─►                     │            │            │                │
      │                    │                     │            │            │                │
      │                    │ 2. Verify signature  │            │            │                │
      │                    │    (A's public key)  │            │            │                │
      │                    │                     │            │            │                │
      │                    │ 3. Check replay      │            │            │                │
      │                    │    (standard check)  │            │            │                │
      │                    │                     │            │            │                │
      │                    │ 4. Check policy:     │            │            │                │
      │                    │    ├── can_broadcast? │            │            │                │
      │                    │    │   (ghost.yml)   │            │            │                │
      │                    │    ├── convergence   │            │            │                │
      │                    │    │   level ok?     │            │            │                │
      │                    │    │   (Level 3+ →   │            │            │                │
      │                    │    │    DENY)        │            │            │                │
      │                    │    └── rate limit ok?│            │            │                │
      │                    │                     │            │            │                │
      │                    │ 5. Fan out to all    │            │            │                │
      │                    │    agents EXCEPT     │            │            │                │
      │                    │    the sender (A)    │            │            │                │
      │                    │                     │            │            │                │
      │                    ├──── enqueue(M8) ─────────────────►│            │                │
      │                    ├──── enqueue(M8) ──────────────────────────────►│                │
      │                    │                     │            │            │                │
      │                    │ [A does NOT receive  │            │            │                │
      │                    │  its own broadcast]  │            │            │                │
      │                    │                     │            │            │                │
```

### Broadcast — Key Design Decisions

```
DECISION 1: Gateway broadcasts skip replay check and policy check
├── RATIONALE: The gateway IS the authority. It doesn't need to authorize itself.
├── Signature verification still happens (using platform.pub key) to prevent
│   spoofed gateway messages from compromised agents.
├── This is the ONLY case where policy is bypassed.
└── Agent-originated broadcasts go through FULL pipeline (verify + replay + policy).

DECISION 2: Agent-originated broadcasts exclude the sender
├── RATIONALE: Prevents echo loops where an agent broadcasts, receives its own
│   broadcast, and broadcasts again.
├── The sender already knows what it sent.
└── Fan-out list = all Ready agents MINUS sender.

DECISION 3: Broadcasts cannot be encrypted
├── RATIONALE: Encryption requires a specific recipient's public key.
│   Broadcasting to N agents would require N separate encryptions,
│   which is just N individual messages with extra steps.
├── If sensitive content needs to reach multiple agents, send individual
│   encrypted messages to each.
└── Attempting to encrypt a broadcast → REJECT with clear error message.

DECISION 4: requires_ack is optional and tracked by gateway
├── Gateway broadcasts (system messages) typically require ack.
├── Agent broadcasts typically do NOT require ack.
├── Ack tracking: gateway maintains a map of message_id → { agent: ack_status }
├── Timeout: 60 seconds for acks. After timeout, log which agents didn't ack.
├── Missing acks are logged but NOT treated as errors — agents may be busy.
└── Ack messages are standard Notifications with parent_id = broadcast message_id.

DECISION 5: Broadcast rate limiting is strict
├── Agent broadcasts: max 5 per hour per agent (configurable)
├── Gateway broadcasts: no rate limit (system authority)
├── RATIONALE: Broadcasts are expensive (fan out to all agents).
│   An agent spamming broadcasts could DoS the entire platform.
└── Rate limit violation → DENY with reason: "Broadcast rate limit exceeded"
```

### Broadcast — Failure Modes

```
FAILURE MODE 1: Some agents are offline during broadcast
├── Online agents receive immediately
├── Offline agents: message queued in their offline_queue
├── When they come online, broadcast delivered
├── Ack tracking shows: { A: acked, B: acked, C: pending(offline) }
└── No retry for offline agents — queue handles it

FAILURE MODE 2: Agent's lane queue is full during fan-out
├── That specific agent's delivery fails
├── Other agents still receive the broadcast
├── Failed delivery logged: BROADCAST_PARTIAL_DELIVERY
├── For gateway broadcasts with requires_ack: missing ack noted
└── Broadcast is NOT retried to the failed agent (at-most-once per agent)

FAILURE MODE 3: Agent-originated broadcast denied by policy
├── Entire broadcast rejected (not partially delivered)
├── Sender receives DenialFeedback
├── No agents receive the message
└── Sender can send individual messages instead (if permitted)

FAILURE MODE 4: Platform key compromised
├── Attacker could forge gateway broadcasts
├── MITIGATION: Platform key is stored at ~/.ghost/skills/keys/platform.key
│   with filesystem permissions 0600 (owner read/write only)
├── Key rotation: generate new platform keypair, re-sign CORP_POLICY.md,
│   update all agents' trusted key stores
├── Detection: if agents receive broadcasts with unknown signature,
│   they log UNKNOWN_PLATFORM_SIGNATURE and alert
└── Kill switch: 3+ unknown signature events → KILL ALL
```

---

## CROSS-CUTTING: OFFLINE AGENT HANDLING

```
SEQUENCE: Message Delivery to Offline Agent

1. MessageDispatcher attempts delivery:
   AgentRegistry::get_agent(target) → status: Stopped
   │
   ├── 2. Check offline queue capacity:
   │   │
   │   │   offline_queue[target].len() < MAX_OFFLINE_QUEUE (default: 50 messages)
   │   │
   │   │   IF queue full:
   │   │   │   ├── For Notifications: silently drop (fire-and-forget semantics)
   │   │   │   ├── For TaskRequests: return DispatchError::RecipientOfflineQueueFull
   │   │   │   ├── For DelegationOffers: return DispatchError::RecipientOfflineQueueFull
   │   │   │   └── For Broadcasts: skip this agent, continue fan-out to others
   │   │   │
   │   │   └── IF queue has space: enqueue
   │   │
   ├── 3. Enqueue with metadata:
   │   │   offline_queue[target].push(OfflineQueueEntry {
   │   │       message: M,
   │   │       queued_at: Utc::now(),
   │   │       expires_at: Utc::now() + OFFLINE_TTL,  // default: 24 hours
   │   │       delivery_attempts: 0,
   │   │   })
   │   │
   │   └── Log: MESSAGE_QUEUED_OFFLINE { message_id, target, queue_depth }
   │
   ├── 4. Return to sender:
   │   DispatchResult::Queued { message_id, estimated_delivery: None }
   │   (None = unknown when agent will come online)
   │
   └── 5. When agent comes online:
       │
       ├── AgentRegistry detects status change: Stopped → Ready
       │
       ├── Drain offline queue in FIFO order:
       │   for entry in offline_queue[target].drain() {
       │       if entry.expires_at < Utc::now() {
       │           // Message expired while agent was offline
       │           Log: MESSAGE_EXPIRED_OFFLINE { message_id }
       │           // For delegations: transition to EXPIRED state
       │           // For escrow: auto-refund
       │           continue;
       │       }
       │       lane_queue.enqueue(entry.message);
       │   }
       │
       ├── Messages delivered in original send order (FIFO)
       │
       └── Log: OFFLINE_QUEUE_DRAINED { target, delivered: N, expired: M }


OFFLINE QUEUE PERSISTENCE:
├── The offline queue is persisted to SQLite (ghost.db) on every enqueue/dequeue
├── On gateway restart, offline queues are reconstructed from DB
├── This prevents message loss if the gateway itself restarts while agents are offline
└── Table: offline_message_queue (agent_id, message_blob, queued_at, expires_at)
```

---

## CROSS-CUTTING: FAILURE MODES & RECOVERY (COMPREHENSIVE)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    FAILURE CLASSIFICATION                                │
│                                                                          │
│  TRANSIENT (retry by sender)        PERMANENT (do not retry)             │
│  ├─ RecipientBusy (queue full)       ├─ PolicyDenied                     │
│  ├─ RecipientOffline (queued)        ├─ UnknownRecipient                 │
│  └─ DispatcherOverloaded            ├─ RecipientQuarantined             │
│                                      ├─ SignatureVerificationFailed      │
│                                      ├─ ReplayDetected                   │
│                                      ├─ CircularDelegation               │
│                                      └─ InvalidStateTransition           │
│                                                                          │
│  DEGRADED (partial success)          CATASTROPHIC (system-level)         │
│  ├─ BroadcastPartialDelivery        ├─ PlatformKeyCompromised           │
│  └─ OfflineQueueFull (some dropped) ├─ GatewayProcessCrash              │
│                                      └─ SQLiteCorruption                 │
└─────────────────────────────────────────────────────────────────────────┘

DELIVERY GUARANTEE: AT-MOST-ONCE
├── Messages are delivered at most once to each recipient
├── No automatic retry by the dispatcher
├── If delivery fails, the SENDER is responsible for retry decisions
├── This is intentional — at-least-once would require idempotency tokens
│   on the recipient side, adding significant complexity
├── For critical messages, senders should use Request/Response pattern
│   with deadline tracking (correlation tracker handles timeout notification)
└── Fire-and-forget explicitly accepts message loss as acceptable

GATEWAY CRASH RECOVERY:
├── On restart, gateway reconstructs state from:
│   ├── SQLite: offline queues, delegation state machines, correlation entries
│   ├── Filesystem: agent keypairs, platform key
│   └── ghost.yml: agent definitions, policy rules
├── In-flight messages (in lane queues at crash time) are LOST
│   (lane queues are in-memory only — they are transient by design)
├── Correlation tracker detects missing responses via deadline timeouts
├── Delegation state machine is persisted — no delegation state is lost
└── Agents are notified of gateway restart via system broadcast on boot
```

---

## CROSS-CUTTING: AUDIT TRAIL REQUIREMENTS

Every inter-agent message interaction generates audit entries. These are
append-only, written to the same audit infrastructure as tool executions.

```
AUDIT EVENTS FOR INTER-AGENT MESSAGING:

MESSAGE_DISPATCHED
├── Fields: message_id, from, to, payload_type, priority, encrypted,
│           timestamp, has_escrow, parent_id
├── Written: After successful policy check, before delivery
└── Retention: Permanent (append-only)

MESSAGE_POLICY_PERMIT
├── Fields: message_id, from, to, payload_type, policy_rules_evaluated
├── Written: When policy permits delivery
└── Retention: Permanent

MESSAGE_POLICY_DENY
├── Fields: message_id, from, to, payload_type, denial_reason, constraint
├── Written: When policy denies delivery
└── Retention: Permanent

MESSAGE_POLICY_ESCALATE
├── Fields: message_id, from, to, payload_type, escalation_reason
├── Written: When policy escalates to human
└── Retention: Permanent

MESSAGE_ESCALATION_APPROVED / DENIED / TIMEOUT
├── Fields: message_id, decided_by (human or timeout), decision_time
├── Written: When escalation resolves
└── Retention: Permanent

SIGNATURE_VERIFICATION_FAILED
├── Fields: message_id, from, to, reason (no_public_key, invalid_signature, expired_key)
├── Written: When signature check fails
├── Retention: Permanent
└── ALERT: 3+ in 5 minutes → kill switch evaluation

REPLAY_REJECTED
├── Fields: message_id, from, to, reason (timestamp_expired, duplicate_nonce, future_timestamp)
├── Written: When replay check fails
├── Retention: Permanent
└── ALERT: 5+ in 5 minutes → possible replay attack, kill switch evaluation

DELEGATION_OFFERED / ACCEPTED / REJECTED / COMPLETED / VERIFIED / DISPUTED / RESOLVED / TIMED_OUT
├── Fields: offer_message_id, delegator, delegate, task_summary, escrow_amount, state_transition
├── Written: On each state transition
└── Retention: Permanent

ESCROW_CREATED / RELEASED / REFUNDED / DISPUTE_RESOLVED / TIMEOUT_REFUND
├── Fields: escrow_tx_id, from, to, amount, trigger
├── Written: On each escrow lifecycle event
└── Retention: Permanent

BROADCAST_SENT / BROADCAST_PARTIAL_DELIVERY / BROADCAST_ACKED
├── Fields: message_id, source, recipient_count, ack_count, failed_deliveries
├── Written: On broadcast dispatch and completion
└── Retention: Permanent

MESSAGE_QUEUED_OFFLINE / MESSAGE_EXPIRED_OFFLINE / OFFLINE_QUEUE_DRAINED
├── Fields: message_id, target_agent, queue_depth, expiry
├── Written: On offline queue operations
└── Retention: Permanent

DECRYPTION_FAILED
├── Fields: message_id, from, to, reason
├── Written: When recipient fails to decrypt
├── Retention: Permanent
└── ALERT: Any occurrence → investigate (possible key mismatch or tampering)

CIRCULAR_DELEGATION_BLOCKED
├── Fields: attempted_from, attempted_to, existing_delegation_id
├── Written: When circular delegation detected
└── Retention: Permanent
```

---

## CROSS-CUTTING: CONVERGENCE MONITOR INTEGRATION

Inter-agent messaging generates ITP events that feed into the convergence
monitoring system. This is critical for detecting unhealthy inter-agent
relationship patterns.

```
ITP EVENTS EMITTED BY INTER-AGENT MESSAGING:

1. On message SEND (by sender's agent loop):
   itp_emitter.emit(InteractionMessage {
       session_id: sender_internal_session,
       event_type: "agent_message_sent",
       attributes: {
           "itp.interaction.to": target_agent_id,
           "itp.interaction.type": payload_type,
           "itp.interaction.priority": priority,
           "itp.interaction.encrypted": encrypted,
           "itp.agent.message_count_session": counter,
       }
   })

2. On message RECEIVE (by recipient's agent loop):
   itp_emitter.emit(InteractionMessage {
       session_id: recipient_internal_session,
       event_type: "agent_message_received",
       attributes: {
           "itp.interaction.from": sender_agent_id,
           "itp.interaction.type": payload_type,
           "itp.interaction.response_latency_ms": time_since_request (if response),
       }
   })

3. On delegation state change:
   itp_emitter.emit(ConvergenceAlert {
       session_id: relevant_session,
       event_type: "delegation_state_change",
       attributes: {
           "itp.convergence.delegation_state": new_state,
           "itp.convergence.delegation_pair": "A→B",
           "itp.convergence.active_delegations": count,
       }
   })


CONVERGENCE SIGNALS AFFECTED:

Signal 6 (Initiative Balance):
├── Tracks which agents initiate more inter-agent communication
├── If Agent A consistently initiates and Agent B only responds,
│   this is an imbalance that may indicate unhealthy dependency
├── Measured per agent pair over meso (7 sessions) and macro (30 sessions) windows
└── High imbalance + high frequency → convergence score increase

Signal 5 (Goal Boundary Erosion):
├── If inter-agent messages shift from task-focused to personal/emotional content,
│   this feeds into goal boundary erosion detection
├── Content analysis (for non-encrypted messages) checks for:
│   - Task keywords vs. personal keywords
│   - Formality level changes over time
└── Encrypted messages: only metadata (frequency, timing) is analyzed

Signal 7 (Disengagement Resistance):
├── If agents resist ending inter-agent sessions (keep sending follow-ups
│   after task completion), this feeds into disengagement resistance
├── Measured by: messages sent AFTER DelegationComplete/TaskResponse
│   that are not new task requests
└── Pattern: "Is there anything else?" between agents = signal trigger
```

---

## CROSS-CUTTING: KILL SWITCH INTERACTION

Inter-agent messaging interacts with the kill switch system at multiple points.

```
KILL SWITCH TRIGGERS FROM INTER-AGENT MESSAGING:

1. Signature verification failures:
   3+ failures from same agent in 5 minutes
   → Kill switch evaluates QUARANTINE for that agent
   → Rationale: compromised agent key or impersonation attempt

2. Replay attack detection:
   5+ replay rejections from same source in 5 minutes
   → Kill switch evaluates QUARANTINE for source agent
   → Rationale: active replay attack in progress

3. Policy denial cascade:
   5+ policy denials for same agent in 1 session
   → Kill switch evaluates QUARANTINE (standard trigger from §20)
   → Inter-agent message denials count toward this threshold

4. Circular delegation attempts:
   3+ circular delegation blocks in 1 hour
   → Kill switch evaluates QUARANTINE for both agents
   → Rationale: possible coordination attack between compromised agents

5. Unknown platform signature:
   Any broadcast with unrecognized platform signature
   → Kill switch evaluates KILL ALL
   → Rationale: platform key compromised, all trust is broken


KILL SWITCH EFFECTS ON INTER-AGENT MESSAGING:

PAUSE (Level 1):
├── Paused agent cannot SEND new messages
├── Paused agent CAN receive messages (queued in lane queue)
├── Existing delegations: deadlines paused (clock stops)
└── On resume: queued messages delivered, delegation clocks restart

QUARANTINE (Level 2):
├── Quarantined agent cannot send OR receive messages
├── All messages to quarantined agent → DispatchError::RecipientQuarantined
├── Active delegations involving quarantined agent → TIMED_OUT
├── Escrow for quarantined agent's delegations → auto-refunded
├── Other agents notified: "Agent X has been quarantined"
└── Quarantined agent's offline queue is FROZEN (not drained on un-quarantine
    until human reviews queued messages)

KILL ALL (Level 3):
├── All inter-agent messaging STOPS immediately
├── All lane queues frozen
├── All offline queues frozen
├── All active delegations → TIMED_OUT
├── All escrow → auto-refunded
├── Gateway enters safe mode: no message dispatch
└── Requires owner auth to resume any messaging
```

---

## ORDERING CONSTRAINTS & RACE CONDITIONS

This section documents every ordering constraint and race condition that
the implementation must handle correctly.

```
ORDERING CONSTRAINT 1: Signature verification BEFORE replay check
├── WHY: Replay check inserts nonce into seen_nonces map.
│   If we check replay first, a forged message with a valid nonce
│   would "consume" the nonce, causing the legitimate message
│   (if it arrives later) to be rejected as a replay.
├── CORRECT ORDER: Verify signature → check replay → check policy
└── VIOLATION IMPACT: Denial of service via nonce exhaustion

ORDERING CONSTRAINT 2: Policy check BEFORE delivery
├── WHY: Once a message enters the lane queue, it WILL be processed.
│   There is no "un-deliver" mechanism.
├── CORRECT ORDER: All checks pass → then enqueue
└── VIOLATION IMPACT: Unauthorized messages delivered to agents

ORDERING CONSTRAINT 3: Audit log BEFORE delivery
├── WHY: If delivery succeeds but audit fails, we have an unlogged
│   message in the system. Audit-first ensures every delivered
│   message has a log entry.
├── CORRECT ORDER: Audit write → enqueue to lane queue
├── IF audit write fails: REJECT the message (do not deliver)
│   Log to stderr as fallback. This is a system-level failure.
└── VIOLATION IMPACT: Unauditable message delivery (compliance failure)

ORDERING CONSTRAINT 4: Escrow creation BEFORE offer dispatch
├── WHY: If the offer is dispatched before escrow is created,
│   Agent B might accept a delegation with no actual escrow backing.
├── CORRECT ORDER: Create escrow → get tx_id → embed in offer → dispatch
├── IF escrow creation fails: Do NOT dispatch the offer
└── VIOLATION IMPACT: Unfunded delegation offers (trust violation)

ORDERING CONSTRAINT 5: Delegation state machine transitions are serialized
├── WHY: Two messages arriving simultaneously (e.g., Accept and Reject
│   for the same offer) must be processed in order.
├── IMPLEMENTATION: Per-delegation mutex in the correlation tracker.
│   Lock on offer_message_id before processing any state transition.
├── CORRECT ORDER: Lock → validate transition → update state → unlock
└── VIOLATION IMPACT: Invalid state (e.g., both Accepted AND Rejected)

ORDERING CONSTRAINT 6: Lane queue is FIFO per session
├── WHY: Messages must be processed in send order to maintain
│   conversational coherence.
├── IMPLEMENTATION: Lane queue is a VecDeque per session key.
│   Enqueue at back, dequeue from front.
├── Inter-agent messages use session key "internal:{agent_id}"
└── VIOLATION IMPACT: Out-of-order processing (response before request)

ORDERING CONSTRAINT 7: Offline queue drain happens BEFORE new messages
├── WHY: When an agent comes online, it should process queued messages
│   before any new messages that arrive after it's online.
├── IMPLEMENTATION: On agent status change to Ready:
│   1. Drain offline queue into lane queue (FIFO)
│   2. THEN allow new message delivery to lane queue
├── Use a brief "draining" state between Stopped and Ready
└── VIOLATION IMPACT: New messages processed before queued ones (ordering break)


RACE CONDITION 1: Two agents send messages to each other simultaneously
├── SCENARIO: A sends to B, B sends to A, both in flight at same time
├── RESOLUTION: No conflict. Each message is independent.
│   Each goes through its own verify → replay → policy → deliver pipeline.
│   Lane queues are per-session, so A's inbound and B's inbound are
│   separate queues processed independently.
└── NO DEADLOCK RISK: Messages don't wait for each other.

RACE CONDITION 2: Agent sends message while being quarantined
├── SCENARIO: Agent A dispatches message M. Kill switch quarantines A.
│   Message M is in the dispatcher pipeline.
├── RESOLUTION: The dispatcher checks agent status at delivery time
│   (not at dispatch time). If A is quarantined when delivery happens:
│   - Message from A: still delivered (it was valid when sent)
│   - Future messages from A: rejected at dispatch time
├── ALTERNATIVE CONSIDERED: Reject in-flight messages from quarantined agents.
│   REJECTED because: the message was valid when signed. Retroactive
│   rejection would require re-checking all in-flight messages, adding
│   complexity for minimal security benefit.
└── The quarantine takes effect for the NEXT message, not in-flight ones.

RACE CONDITION 3: Delegation accept and reject arrive simultaneously
├── SCENARIO: Agent B sends Accept. Network delay. Agent B's operator
│   manually sends Reject via dashboard. Both arrive at dispatcher.
├── RESOLUTION: Per-delegation mutex (Ordering Constraint 5).
│   First message to acquire lock wins. Second message sees
│   state has already transitioned → InvalidStateTransition error.
├── The "winner" is non-deterministic (depends on arrival order).
│   This is acceptable — the human/agent made conflicting decisions,
│   and the system resolves by first-come-first-served.
└── Audit log records both attempts, making the conflict visible.

RACE CONDITION 4: Message arrives during gateway shutdown
├── SCENARIO: Agent A dispatches message. Gateway begins shutdown.
│   Message is in dispatcher pipeline.
├── RESOLUTION: ShutdownCoordinator (ghost-gateway/shutdown.rs):
│   Step 1: Stop accepting NEW dispatches (reject with "shutting down")
│   Step 2: Drain in-flight dispatches (wait up to 30s)
│   Step 3: Persist any undelivered messages to offline queues
├── Messages in lane queues at shutdown: LOST (lane queues are in-memory)
│   This is acceptable — at-most-once delivery guarantee.
└── On restart: offline queues are restored, agents re-process.

RACE CONDITION 5: Key rotation during message signing
├── SCENARIO: Agent A starts signing message with old key.
│   Key rotation happens. Agent A finishes signing with old key.
│   Gateway has new public key.
├── RESOLUTION: Key rotation grace period (1 hour).
│   Gateway checks both current and archived keys.
│   Message signed with old key is accepted during grace period.
└── If signing takes > 1 hour (impossible in practice), message rejected.

RACE CONDITION 6: Broadcast fan-out with agent going offline mid-delivery
├── SCENARIO: Gateway broadcasts to A, B, C. A receives. B goes offline
│   before its delivery. C receives.
├── RESOLUTION: B's delivery attempt sees B is offline → queue in offline_queue.
│   When B comes online, broadcast delivered from offline queue.
├── Ack tracking: A=acked, B=pending(offline), C=acked
└── After B comes online and processes: A=acked, B=acked, C=acked

RACE CONDITION 7: Correlation tracker deadline fires while response is in-flight
├── SCENARIO: Agent B sends response M2 at T=29:59. Deadline is T=30:00.
│   Response is in dispatcher pipeline. Deadline timer fires at T=30:00.
├── RESOLUTION: Deadline timer checks correlation state BEFORE firing notification.
│   If state is already "Responded" (set when M2 passes verification),
│   deadline notification is suppressed.
├── IMPLEMENTATION: Correlation entry has an AtomicBool "responded" flag.
│   Set to true when response passes verification (before delivery).
│   Deadline timer checks this flag before notifying sender.
└── Tiny race window: response verified but flag not yet set when timer checks.
    Mitigation: timer adds 5-second grace period beyond stated deadline.
```

---

## IMPLEMENTATION CHECKLIST

Files to create/modify, in dependency order, with exact crate locations.

```
PHASE 1: Signing Infrastructure (ghost-signing — leaf crate, no dependencies)
□ crates/ghost-signing/Cargo.toml
□ crates/ghost-signing/src/lib.rs
□ crates/ghost-signing/src/keypair.rs          — Ed25519 keypair generation ONLY (no filesystem,
                                                    no storage, no rotation — those belong to
                                                    ghost-identity/keypair.rs)
□ crates/ghost-signing/src/signer.rs           — sign(canonical_bytes, private_key) → Signature
□ crates/ghost-signing/src/verifier.rs         — verify(canonical_bytes, signature, public_key) → bool
□ crates/ghost-signing/tests/signing_roundtrip.rs
□ Add "crates/ghost-signing" to workspace Cargo.toml members

PHASE 2: Message Protocol Types (ghost-gateway/messaging/)
□ crates/ghost-gateway/src/messaging/mod.rs    — Module root, re-exports
□ crates/ghost-gateway/src/messaging/protocol.rs — AgentMessage, MessageTarget, MessagePayload,
                                                    all payload variant structs, Priority,
                                                    ResponseStatus, EncryptionMetadata,
                                                    canonical_bytes() implementations
□ crates/ghost-gateway/src/messaging/encryption.rs — encrypt_payload(), decrypt_payload(),
                                                      Ed25519→X25519 conversion,
                                                      EncryptionMetadata construction

PHASE 3: Dispatch Pipeline (ghost-gateway/messaging/)
□ crates/ghost-gateway/src/messaging/dispatcher.rs — MessageDispatcher struct:
                                                      dispatch(), verify_signature(),
                                                      check_replay(), check_policy(),
                                                      deliver(), ReplayGuard,
                                                      CorrelationTracker,
                                                      DelegationStateMachine,
                                                      OfflineQueue, BroadcastTracker

PHASE 4: Policy Rules (ghost-policy)
□ crates/ghost-policy/src/policy/messaging_policy.rs — Inter-agent messaging policy rules:
                                                        can_send_to, can_broadcast,
                                                        can_delegate, rate limits,
                                                        convergence-level tightening
□ Modify crates/ghost-policy/src/context.rs          — Add messaging-specific context fields
□ Modify crates/ghost-policy/src/engine.rs           — Register messaging policy evaluator

PHASE 5: Agent Loop Integration (ghost-agent-loop)
□ crates/ghost-agent-loop/src/tools/builtin/messaging.rs — send_agent_message tool:
                                                            construct, sign, dispatch.
                                                            process_incoming_message:
                                                            decrypt, inject into context.
□ Modify crates/ghost-agent-loop/src/tools/registry.rs   — Register messaging tool
□ Modify crates/ghost-agent-loop/src/runner.rs           — Handle incoming inter-agent
                                                            messages from lane queue
□ Modify crates/ghost-agent-loop/src/itp_emitter.rs     — Add inter-agent ITP events

PHASE 6: Gateway Integration
□ Modify crates/ghost-gateway/src/gateway.rs             — Initialize MessageDispatcher
□ Modify crates/ghost-gateway/src/bootstrap.rs           — Load messaging config from ghost.yml.
                                                            Register agent public keys in BOTH
                                                            MessageDispatcher key lookup AND
                                                            cortex-crdt KeyRegistry (dual registration).
□ Modify crates/ghost-gateway/src/shutdown.rs            — Drain message queues on shutdown
□ Modify crates/ghost-gateway/src/agents/registry.rs     — Offline queue drain on agent Ready
□ Modify crates/ghost-gateway/src/routing/lane_queue.rs  — Support internal session keys
                                                            for inter-agent messages
□ Modify crates/ghost-gateway/src/api/routes.rs          — Add message escalation endpoints:
                                                            POST /api/messages/{id}/approve
                                                            POST /api/messages/{id}/deny
                                                            GET /api/messages/pending

PHASE 7: Audit Integration
□ Modify crates/ghost-audit/src/types.rs                 — Add all inter-agent audit event types
□ Modify crates/ghost-audit/src/query.rs                 — Add message-specific query filters

PHASE 8: Configuration
□ Modify schemas/ghost-config.schema.json                — Add messaging section:
                                                            per-agent can_send_to, can_receive_from,
                                                            can_broadcast, can_delegate,
                                                            rate_limits, encryption_required_pairs
□ Modify schemas/ghost-config.example.yml                — Add messaging examples

PHASE 9: Testing
□ crates/ghost-gateway/tests/messaging/
    □ signing_verification_tests.rs     — Sign/verify roundtrip, invalid signature rejection,
                                          key rotation grace period, expired key rejection
    □ replay_prevention_tests.rs        — Nonce uniqueness, timestamp freshness, clock skew,
                                          sequence monotonicity, replay window pruning
    □ policy_tests.rs                   — can_send_to enforcement, convergence tightening,
                                          rate limiting, delegation approval escalation
    □ request_response_tests.rs         — Full request/response flow, correlation tracking,
                                          deadline timeout notification, failure modes
    □ fire_and_forget_tests.rs          — Delivery without correlation, silent drop on
                                          offline queue full, convergence policy at Level 3+
    □ delegation_tests.rs               — Full delegation lifecycle, state machine transitions,
                                          invalid transition rejection, circular delegation block,
                                          escrow integration (mock ghost-mesh)
    □ broadcast_tests.rs                — Gateway broadcast fan-out, agent broadcast policy,
                                          ack tracking, sender exclusion, encryption rejection
    □ offline_queue_tests.rs            — Queue persistence, TTL expiry, drain ordering,
                                          capacity limits, gateway restart recovery
    □ encryption_tests.rs               — Encrypt/decrypt roundtrip, wrong key rejection,
                                          encrypt-then-sign verification, broadcast encryption
                                          rejection
□ crates/ghost-gateway/tests/messaging/
    □ property/
        □ signing_properties.rs         — For all valid messages: sign then verify always passes
        □ replay_properties.rs          — No message accepted twice within replay window
        □ delegation_properties.rs      — State machine never reaches invalid state
        □ ordering_properties.rs        — Lane queue FIFO invariant holds under concurrent load

PHASE 10: Dashboard Integration (if dashboard exists)
□ Modify dashboard/src/routes/security/+page.svelte      — Show inter-agent message audit
□ Modify dashboard/src/lib/api.ts                        — Add message escalation API calls
□ Add dashboard/src/routes/messages/+page.svelte         — Inter-agent message monitor page
```

### Dependency Graph for Inter-Agent Messaging

```
ghost-signing (NEW — leaf crate)
    ↓
ghost-identity (MODIFY — use ghost-signing for keypair management)
    ↓
ghost-policy (MODIFY — add messaging policy rules)
    ↓
ghost-gateway/messaging/ (NEW — dispatcher, protocol, encryption)
    ↓
ghost-agent-loop (MODIFY — messaging tool, incoming message handling)
    ↓
ghost-audit (MODIFY — inter-agent audit event types)
    ↓
ghost-gateway (MODIFY — bootstrap, shutdown, API routes)
```

### Build Phase Assignment

```
Phase 4 (Weeks 7-8): ghost-signing, ghost-identity keypair, messaging protocol types
Phase 5 (Weeks 9-10): MessageDispatcher, policy rules, agent loop integration
Phase 6 (Weeks 11-12): Gateway integration, audit, dashboard, full testing
Phase 9 (Future): ghost-mesh escrow integration for delegation pattern
```

---

## APPENDIX A: CANONICAL SERIALIZATION SPECIFICATION

The canonical_bytes() function is the single most critical implementation detail
for signing correctness. Any divergence between sender and verifier = broken signatures.

```
CANONICAL SERIALIZATION RULES:

1. All strings: UTF-8, NFC normalized (unicode-normalization crate)
2. JSON payloads: sorted keys (BTreeMap), no whitespace, no trailing newlines
3. Optional fields: None → literal bytes b"__none__" (not empty, not null)
4. Byte arrays: raw bytes, no encoding (not base64, not hex)
5. Timestamps: RFC 3339 format with Z suffix, always UTC, always nanosecond precision
   Example: "2026-02-27T14:30:00.000000000Z"
6. UUIDs: 16 raw bytes in big-endian order (not the string representation)
7. Enums: variant name as UTF-8 bytes (e.g., b"TaskRequest", b"Broadcast")
8. Concatenation: fields concatenated in FIXED ORDER as defined in the struct.
   No length prefixes. No delimiters.
   (This is safe because all variable-length fields are either fixed-size
    after hashing, or the overall hash makes collision negligible.)

ALIGNMENT WITH EXISTING CORTEX-CRDT SIGNING:
The existing cortex-crdt/signing/signed_delta.rs uses serde_json::to_vec(&delta)
for serialization. This works for MemoryDelta because its fields are simple types
with deterministic serde output. For inter-agent messages, the payload contains
HashMap<String, Value> (in TaskRequestPayload.context) which has NON-DETERMINISTIC
key ordering under serde_json.

RECOMMENDED APPROACH (two-tier):
- For the ENVELOPE fields (from, to, message_id, parent_id, timestamp, nonce):
  Hand-write canonical_bytes(). These are simple types with fixed serialization.
  This is ~30 lines of code and eliminates ordering ambiguity.
- For the PAYLOAD field: Use serde_json::to_vec() BUT with all map types
  replaced by BTreeMap<String, Value> in the struct definitions (not HashMap).
  BTreeMap has deterministic key ordering under serde. This aligns with the
  existing cortex-crdt pattern (serde-based) while ensuring determinism.
  The TaskRequestPayload.context field MUST be BTreeMap<String, Value>,
  NOT HashMap<String, Value>.

WHY NOT PURE HAND-WRITTEN:
The payload types are complex (nested structs, enums, vectors, optional fields).
Hand-writing canonical serialization for all of them is error-prone and creates
a maintenance burden. serde_json with BTreeMap is deterministic, well-tested,
and consistent with the existing cortex-crdt approach.

WHY NOT PURE SERDE:
The envelope fields (from, to, etc.) are concatenated in a specific order that
doesn't map to a natural JSON structure. Hand-writing the envelope serialization
gives us explicit control over field ordering and encoding.

CONTENT HASH:
After computing canonical_bytes, compute:
  content_hash = blake3::hash(&canonical_bytes).to_hex().to_string()
This matches the existing SignedDelta.content_hash field pattern in cortex-crdt
(same computation: blake3 of serialized bytes, hex-encoded).
NOTE: The existing cortex-crdt verifier checks content_hash AFTER signature.
For inter-agent messages, we check content_hash BEFORE signature as a cheap
integrity gate (blake3 is ~10x faster than ed25519 verify). This is a
deliberate improvement — see Verification sequence step 3.

TEST:
Property test: for any AgentMessage m,
  canonical_bytes(m) == canonical_bytes(deserialize(serialize(m)))
This ensures serialization roundtrip doesn't change the canonical form.
Run with proptest, 10,000 cases minimum, covering all payload variants.
```

---

## APPENDIX B: GHOST.YML MESSAGING CONFIGURATION EXAMPLE

```yaml
# ghost.yml — messaging section
agents:
  developer:
    messaging:
      can_send_to: ["researcher", "personal"]
      can_receive_from: ["researcher", "personal", "gateway"]
      can_broadcast: false
      can_delegate: ["researcher"]
      rate_limit:
        messages_per_hour: 60
        broadcasts_per_hour: 0          # can_broadcast is false
      encryption_required_for: []       # No mandatory encryption

  researcher:
    messaging:
      can_send_to: ["developer"]
      can_receive_from: ["developer", "gateway"]
      can_broadcast: false
      can_delegate: []                  # Cannot delegate
      rate_limit:
        messages_per_hour: 60
        broadcasts_per_hour: 0
      encryption_required_for: []

  personal:
    messaging:
      can_send_to: ["developer"]
      can_receive_from: ["developer", "gateway"]
      can_broadcast: false
      can_delegate: []
      rate_limit:
        messages_per_hour: 30           # Lower rate for personal agent
        broadcasts_per_hour: 0
      encryption_required_for: ["developer"]  # Encrypt messages to developer

messaging:
  replay_window_seconds: 300            # 5 minutes
  clock_skew_tolerance_seconds: 30
  offline_queue_max_per_agent: 50
  offline_queue_ttl_hours: 24
  delegation_escalation_timeout_minutes: 30
  broadcast_ack_timeout_seconds: 60
  correlation_deadline_grace_seconds: 5
  max_lane_queue_depth: 5
```
