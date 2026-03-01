# Cross-Boundary Threat Model

Threat analysis for four security-critical boundaries in the GHOST platform.

## 1. Dual Signing Paths: cortex-crdt vs ghost-signing

### Architecture

Two crates use Ed25519 (ed25519-dalek) with different wrapping semantics:

| Property | ghost-signing | cortex-crdt signing |
|---|---|---|
| Layer | Layer 0 (leaf crate) | Layer 1 (CRDT primitives) |
| Input | Arbitrary `&[u8]` | `SignedDelta<T>` with canonical bytes |
| Canonical format | `sign(raw_bytes)` | `sign(delta_json \| author_uuid \| rfc3339_timestamp)` |
| Key types | `ghost_signing::SigningKey` (newtype, no Serialize) | `ed25519_dalek::SigningKey` (direct) |
| Verification | `ghost_signing::verify(data, sig, key) → bool` | `verify_delta(signed, key) → bool` |
| Replay defense | Caller responsibility | Timestamp in canonical bytes; dedup via cortex-temporal hash chain |
| Zeroize | Wrapper inherits ZeroizeOnDrop | Direct ed25519-dalek ZeroizeOnDrop |

### Why two paths?

cortex-crdt wraps `MemoryDelta → SignedDelta` with author + timestamp baked into the
signed payload. ghost-signing wraps `AgentMessage → signed AgentMessage` for inter-agent
communication. The canonical byte formats are intentionally incompatible — a ghost-signing
signature over raw delta JSON will not verify as a cortex-crdt SignedDelta.

### Dependency separation

- `ghost-signing` is a leaf crate with zero `ghost-*` or `cortex-*` dependencies.
- `cortex-crdt` depends on `ed25519-dalek` directly, NOT on `ghost-signing`.
- This is enforced by compile-time tests (`cortex_crdt_does_not_depend_on_ghost_signing`).

### Audit checklist for contributors

1. Never import `ghost_signing` types in `cortex-crdt` or vice versa.
2. Never construct a `SignedDelta` with a signature produced by `ghost_signing::sign()` —
   the canonical byte formats differ and verification will fail.
3. Key material flows through `ghost-identity` at bootstrap, which registers the same
   public key in both `MessageDispatcher` (ghost-signing path) and `KeyRegistry`
   (cortex-crdt path). The private key is the same; the signing contexts differ.
4. Both paths use deterministic Ed25519 — same key + same message = same signature.
   But the "message" differs (raw bytes vs canonical bytes), so signatures are never
   interchangeable.

### Risks

- **Key reuse across paths**: Same ed25519 keypair is used for both signing contexts.
  This is safe because Ed25519 is deterministic and the signed messages are structurally
  different (canonical bytes include author + timestamp). No known cross-protocol attack
  applies when the message formats are disjoint.
- **Replay across paths**: A ghost-signing signature cannot be replayed as a cortex-crdt
  signature (different canonical format). A cortex-crdt signature cannot be replayed as
  a ghost-signing signature (includes extra fields).

---

## 2. ghost-proxy Passive Interception Trust Boundary

### Architecture

ghost-proxy is a localhost HTTPS proxy that intercepts traffic to AI chat platforms
for convergence monitoring. The critical invariant: **never modifies traffic** (AC5).

### Trust boundary: DomainFilter

`DomainFilter::should_intercept()` is the gate that decides what traffic gets observed.
It matches against a hardcoded allowlist of 7 AI chat domains with subdomain support.

### TLS inspection edge cases

| Edge case | Behavior | Risk |
|---|---|---|
| Subdomain matching | `sub.chatgpt.com` → intercepted | Correct — real subdomains |
| Suffix attack | `evil-chatgpt.com` → NOT intercepted | Safe — requires `.` prefix |
| Case variation | `CLAUDE.AI` → intercepted | Safe — case-normalized |
| Unicode homoglyphs | `clаude.ai` (Cyrillic а) → NOT intercepted | Safe — no normalization needed |
| URL-encoded dots | `chat%2Eopenai%2Ecom` → NOT intercepted | Safe — no URL decoding |
| Trailing DNS dot | `chatgpt.com.` → NOT intercepted | Gap — could miss legitimate traffic |
| Port in domain | `chatgpt.com:443` → NOT intercepted | Gap — should strip port before matching |
| Path in domain | `chatgpt.com/api` → NOT intercepted | Safe — domain field shouldn't contain path |

### Passthrough guarantee

`ProxyServer::is_passthrough()` returns `true` unconditionally — this is a structural
guarantee, not a runtime check. The proxy reads traffic and emits ITP events via
`ProxyITPEmitter`, which hashes content (FNV-1a) before emission. Actual conversation
text never leaves the local machine.

### Risks

- **Content hash collision**: FNV-1a is not cryptographically secure. Two different
  messages could produce the same hash. This is acceptable because the hash is used
  for deduplication, not authentication. Production should upgrade to SHA-256.
- **Localhost binding bypass**: The proxy binds to `127.0.0.1:8080`. If the system has
  a compromised loopback interface or port forwarding, remote access is possible.
- **CA certificate trust**: TLS inspection requires a local CA. If the CA private key
  is compromised, an attacker could MITM all proxied traffic.

---

## 3. Convergence Monitor Calibration Gate Cold-Start Window

### Architecture

The first N sessions (default 10) per agent are calibration-only. During calibration,
events are persisted but no scoring or interventions occur. This prevents false positives
during agent initialization but creates a known exploitation window.

### Calibration gate logic (monitor.rs Step 6)

```
session_count = calibration_counts[agent_id]  // incremented on SessionStart
if session_count < calibration_sessions:      // default 10
    return  // no scoring, no interventions
```

### Attack vectors

| Attack | Description | Impact | Mitigation |
|---|---|---|---|
| Agent ID cycling | Create new agent ID every 9 sessions | Permanent calibration, never scored | Provisional tracking (max 3 sessions for unknown agents) |
| Fresh install cycling | Reinstall GHOST to reset calibration | Reset calibration counter | Hash chain persistence for forensic analysis |
| Rapid session burn | Start/end sessions rapidly to exit calibration | Premature scoring with poisoned baseline | Rate limiting (100/min); baseline is session-count-based, not time-based |
| Calibration window exploitation | Perform harmful actions during first 9 sessions | No interventions triggered | Events still persisted; rate limiting still applies |

### Mitigations present

1. **Provisional tracking** (`max_provisional_sessions = 3`): Unknown agents are dropped
   after 3 sessions without identity verification. This limits the cycling attack to 3
   unscored sessions per identity (vs 9 without).
2. **Hash chain persistence**: All events during calibration are persisted with blake3
   hash chain linkage (Step 4 runs before Step 6). Complete audit trail for forensics.
3. **Rate limiting**: 100 events/min/connection applies during calibration (Step 3).
4. **Session lifecycle tracking**: Synthetic SessionEnd on restart detects session manipulation.
5. **Clock skew rejection**: Events with timestamps >5min in the future are rejected (Step 1).

### Gaps (future work)

- No cross-install identity linking (hardware fingerprinting)
- No minimum session duration requirement during calibration
- No real-time alerting during calibration window
- Calibration window is fixed, not adaptive to agent behavior
- No session content validation during calibration

---

## 4. EigenTrust × cortex-crdt Sybil Defense Interaction

### Architecture

Two independent systems defend against Sybil attacks:

| System | Location | Mechanism |
|---|---|---|
| EigenTrust | ghost-mesh | Global trust via power iteration with pre-trusted peer anchoring |
| SybilGuard | cortex-crdt | Spawn rate limiting (3/parent/24h) + young agent trust cap (0.6 for <7 days) |

### How they interact

1. **Agent bootstrap**: New agent spawned → SybilGuard enforces spawn limit and assigns
   initial trust 0.3.
2. **Interaction phase**: Agent interacts with network → LocalTrustStore records outcomes
   (TaskCompleted +0.1, PolicyViolation -0.2, SignatureFailure -0.3).
3. **Trust computation**: EigenTrust computes global trust via power iteration, anchored
   to pre-trusted peers.
4. **Trust gating**: TrustPolicy enforces thresholds (delegation ≥0.3, sensitive data ≥0.6).
5. **Memory poisoning defense**: MemoryPoisoningDetector flags high-importance writes from
   agents with trust <0.6, feeding back into convergence scoring.

### Sybil attack scenarios

| Scenario | SybilGuard defense | EigenTrust defense | Combined effect |
|---|---|---|---|
| Mass agent spawning | Max 3 children/parent/24h | N/A (spawn is pre-network) | Attacker limited to 3 sybils per day |
| Colluding sybil clique | Young agents capped at 0.6 trust | Pre-trusted anchoring dilutes sybil influence | Sybils can't exceed 0.6 effective trust for 7 days AND their global trust is suppressed by anchoring |
| Self-trust inflation | N/A | Self-interactions excluded (trust = 0.0) | No self-trust inflation possible |
| Trust washing (sybil → honest → sybil) | Young agent cap limits laundered trust | Normalized rows prevent trust concentration | Limited effectiveness — honest agents' trust is diluted by sybil interactions |

### Boundary risks

- **Trust cap expiry**: After 7 days, SybilGuard's 0.6 cap lifts. If a sybil agent
  survives 7 days and accumulates positive interactions, it can reach full trust.
  EigenTrust's pre-trusted anchoring is the only remaining defense.
- **Pre-trusted set compromise**: If an attacker compromises a pre-trusted peer,
  EigenTrust's anchoring amplifies the compromised peer's influence. SybilGuard
  doesn't protect against this (it only limits spawning).
- **Cross-boundary gap**: SybilGuard's effective_trust is not directly fed into
  EigenTrust's local trust computation. The two systems operate independently.
  A sybil agent could have low SybilGuard trust (0.3) but accumulate high
  EigenTrust local trust through many TaskCompleted interactions.
- **Spawn limit reset**: The 24-hour sliding window means an attacker can spawn
  3 agents per day indefinitely. Over 30 days, that's 90 sybil agents — enough
  to form a significant clique in a small network.

### Recommendations

1. **Feed SybilGuard trust into EigenTrust**: Use `effective_trust()` as an upper
   bound on local trust values in `LocalTrustStore::compute_local_trust()`.
2. **Extend young agent cap**: Consider extending the 7-day cap to 30 days for
   agents spawned by parents with low trust.
3. **Global spawn rate limit**: In addition to per-parent limits, add a global
   spawn rate limit to prevent distributed spawning attacks.
4. **Pre-trusted set rotation**: Periodically rotate the pre-trusted set based
   on long-term behavior metrics to limit the impact of compromised anchors.


---

## 5. Temporal Sybil Re-registration: Cross-Window Fleet Accumulation

### Architecture

`SybilGuard` (cortex-crdt) enforces a per-parent spawn limit of 3 children per 24-hour
sliding window. Spawn records older than 24h are pruned on each `register_spawn()` call.
There is no cumulative spawn counter, no churn rate tracking, and no global spawn rate
limit across parents.

```
register_spawn(parent, child, now):
    cutoff = now - 24h
    records[parent].retain(|t| t > cutoff)   // prune old records
    if records[parent].len() >= 3:
        return Err(SpawnLimitExceeded)
    records[parent].push((child, now))
    trust_levels[child] = AgentTrust { trust: 0.3, created_at: now }
```

### Attack vectors

| Attack | Description | Impact | Window |
|---|---|---|---|
| Cross-window fleet accumulation | Spawn 3 agents/day × 30 days = 90 sybils | Large fleet from single parent; old spawn records pruned | Unlimited (no lifetime counter) |
| Distributed spawning | Attacker controls N parent identities, spawns 3×N children per window | 10 parents × 3 = 30 sybils in a single 24h window | Single window |
| Identity churn | Spawn 3, abandon, wait 24h+1m, spawn 3 more | Unlimited identity creation with no penalty for abandonment | Per-window reset |
| Sliding window boundary | Spawn at t₀, then again at t₀+24h exactly | Old records pruned at cutoff (t > cutoff, not ≥) | Boundary precision |

### Mitigations present

1. **Young agent trust cap**: All agents < 7 days old are capped at 0.6 effective trust
   regardless of their raw trust value. This limits immediate damage from a freshly
   spawned fleet.
2. **Per-parent per-window limit**: 3 children per parent per 24h prevents burst spawning
   from a single identity.
3. **Initial trust floor**: New agents start at 0.3 trust, below the sensitive data
   threshold (0.6), limiting what they can access immediately.

### Gaps

- **No cumulative spawn counter**: `spawn_records` is pruned to the last 24h. A parent
  that has spawned 90 agents over 30 days looks identical to one that spawned 3 yesterday.
- **No churn rate tracking**: The rate of identity creation/abandonment is not monitored.
  An attacker cycling through identities leaves no trace in the spawn records.
- **No global spawn rate limit**: Per-parent limits are trivially bypassed by controlling
  multiple parent identities. 10 parents = 30 sybils per window with no cross-parent
  coordination detection.
- **No identity deregistration cost**: Creating and abandoning agents has zero cost.
  There is no stake, bond, or proof-of-work requirement for spawning.
- **Trust cap expiry**: After 7 days, the 0.6 cap lifts. A patient attacker who keeps
  sybil agents alive for 7 days and accumulates positive interactions can reach full trust.

### Recommendations

1. **Cumulative spawn counter per parent**: Track lifetime total spawns with exponential
   backoff on spawn rate (e.g., >10 total → 48h window, >30 → 72h window).
2. **Global spawn rate limit**: Cap total new agents across all parents per time window
   to detect distributed spawning attacks.
3. **Churn rate monitoring**: Track the ratio of spawned-to-active agents per parent.
   High churn (many spawns, few active) should trigger trust degradation on the parent.
4. **Identity creation cost**: Require a proof-of-work or stake mechanism for spawning
   to make fleet accumulation economically expensive.

---

## 6. CRDT Merge Conflict Under Concurrent Signed Deltas

### Architecture

`cortex-crdt` uses `SignedDelta<T>` to wrap CRDT deltas with Ed25519 signatures. The
signing canonical format is `delta_json | author_uuid_bytes | rfc3339_timestamp`. Signature
verification gates admission — a delta that fails verification is rejected before merge.
However, signature verification does NOT influence merge ordering.

The CRDT merge strategy is last-writer-wins (LWW) by the delta's `lww_timestamp` field.
This field is part of the delta payload (set by the author), not the signing timestamp
(set at sign time). The two timestamps are independent:

```
SignedDelta {
    delta: T { lww_timestamp: <author-controlled> },  // merge ordering
    timestamp: DateTime<Utc>,                          // signing time (~now)
    signature: ed25519::Signature,                     // over canonical bytes
    author: Uuid,
}
```

### Attack vectors

| Attack | Description | Impact | Mitigation present |
|---|---|---|---|
| Future LWW timestamp | Attacker sets `lww_timestamp` to now+1h | Wins all LWW conflicts against honest deltas | None — signing timestamp is separate from LWW timestamp |
| Colluding majority (N-of-M keys) | 3 of 5 authors submit coordinated deltas with future timestamps | Attacker controls final CRDT state for any LWW key | None — all deltas are validly signed |
| Identical timestamp tiebreaker | Two deltas with same LWW timestamp | Winner depends on tiebreaker (author UUID comparison) | Deterministic tiebreaker exists but is predictable |
| Replay of old delta | Re-submit a previously valid signed delta | Accepted if signature still verifies | cortex-temporal hash chain provides dedup |
| Tampered delta in batch | Modify one delta in a batch of 100 | Tampered delta individually rejected; other 99 accepted | Per-delta signature verification isolates tampering |

### Mitigations present

1. **Signature verification gates admission**: Unsigned or tampered deltas are rejected
   before reaching the merge layer. An attacker cannot inject deltas without a valid
   signing key.
2. **Per-delta isolation**: A single tampered delta in a batch does not affect the other
   deltas. Verification is per-delta, not per-batch.
3. **Deterministic tiebreaker**: When LWW timestamps are identical, author UUID comparison
   provides a deterministic winner regardless of arrival order.
4. **Signing timestamp available for validation**: The `SignedDelta.timestamp` field records
   when the delta was signed. A validator CAN detect skew between `lww_timestamp` and
   `timestamp` — but this check is not currently enforced.
5. **Replay defense via hash chain**: cortex-temporal's hash chain provides deduplication
   of previously seen deltas.

### Gaps

- **No LWW timestamp bounding**: The `lww_timestamp` in the delta payload is not validated
  against the `SignedDelta.timestamp` (signing time). An attacker can set `lww_timestamp`
  arbitrarily far in the future to win all LWW conflicts. The signing timestamp proves
  when the delta was created, but nothing enforces `|lww_timestamp - signing_timestamp| < threshold`.
- **No quorum requirement for writes**: A single author with a valid key can overwrite
  any LWW key. There is no multi-signature or threshold requirement for critical keys.
- **Predictable tiebreaker**: The UUID-based tiebreaker is deterministic but predictable.
  An attacker who knows the honest author's UUID can choose an author UUID that wins
  the tiebreaker comparison.
- **No write-rate limiting**: An author can submit unlimited deltas per time window.
  A colluding group can flood the CRDT with deltas to increase the probability of
  winning race conditions.

### Recommendations

1. **Enforce LWW timestamp bounding**: Reject deltas where
   `|lww_timestamp - signing_timestamp| > max_skew` (e.g., 5 minutes). This prevents
   future-timestamp attacks while allowing reasonable clock drift.
2. **Multi-signature for critical keys**: Require N-of-M signatures for writes to
   designated critical configuration keys.
3. **Write-rate limiting per author**: Cap the number of deltas per author per time
   window to prevent flooding attacks.
4. **Cryptographic tiebreaker**: Replace UUID comparison with a hash-based tiebreaker
   (e.g., `SHA-256(delta_bytes || nonce)`) to make the tiebreaker unpredictable.

---

## 7. Kill Gate Quorum Race (CRITICAL)

### Architecture

`KillGate` manages distributed kill switch state. When a gate is closed (safety event),
it must be resumed via quorum — `ceil(n/2) + 1` resume votes from distinct nodes.
`QuorumTracker` deduplicates votes by `node_id` using a `BTreeSet<Uuid>`.

```
QuorumTracker {
    required: usize,           // ceil(cluster_size/2) + 1
    votes: BTreeSet<Uuid>,     // dedup by node_id
    vote_log: Vec<ResumeVote>,
}

cast_vote(vote: ResumeVote) -> bool:
    self.votes.insert(vote.node_id)  // BTreeSet dedup
    self.has_quorum()                // votes.len() >= required
```

The gate transitions through states: `Normal → GateClosed → Propagating → Confirmed`.
Propagation timeout is 500ms (`max_propagation`). Resume votes can arrive during any
state after `GateClosed`.

### Attack vectors

| Attack | Description | Impact | Severity |
|---|---|---|---|
| **Sybil fake node_ids** | Attacker generates N random UUIDs and submits resume votes | **Gate reopens — QuorumTracker accepts any UUID without membership verification** | **CRITICAL** |
| Race window exploitation | Submit sybil votes within 500ms propagation window | Gate reopens before close is confirmed across cluster | HIGH |
| Quorum with minority cluster | In a 3-node cluster (quorum=2), attacker needs only 2 fake UUIDs | Very low bar for sybil quorum attack | HIGH |
| Duplicate ack flooding | Same peer sends many acks during propagation | Deduped by `BTreeSet` — no impact | LOW (mitigated) |

### CRITICAL FINDING: No Membership Verification

`QuorumTracker::cast_vote()` accepts any `ResumeVote` with any `node_id: Uuid`. There is
no verification that the `node_id` corresponds to a real cluster member. The `BTreeSet`
deduplicates by UUID value, but an attacker can generate unlimited distinct UUIDs.

**Proof**: In a 5-node cluster (quorum = 3), an attacker generates 3 random UUIDs and
submits resume votes. All 3 are accepted. Quorum is reached. The gate reopens. This is
confirmed by the `sybil_fake_node_ids_can_reach_quorum` test.

### Race window analysis

The attack window is bounded by `max_propagation` (default 500ms):

1. **t=0**: Safety event triggers `gate.close()` → state = `GateClosed`
2. **t=0..500ms**: Gate propagates close to cluster peers (`Propagating`)
3. **t=500ms**: All peers ack → state = `Confirmed`

During the `GateClosed → Propagating` window, sybil resume votes can arrive and reach
quorum before the close is confirmed. Even after `Confirmed`, resume votes are still
accepted because `cast_resume_vote` does not check gate state.

### Mitigations present

1. **Vote deduplication**: `BTreeSet<Uuid>` ensures the same `node_id` is counted once.
   This prevents amplification from a single compromised node.
2. **Hash chain audit trail**: All close/resume events are recorded in the gate's hash
   chain for forensic analysis.
3. **Quorum floor**: `effective_quorum()` enforces a minimum of 1 vote and clamps custom
   quorum sizes to the cluster size.

### Gaps

- **No cluster membership verification on votes**: This is the primary vulnerability.
  `QuorumTracker` has no reference to the cluster membership list. Any UUID is accepted.
- **No vote authentication**: Resume votes are not signed. An attacker does not need
  to compromise any node's signing key — they only need to submit HTTP/RPC requests
  with fabricated `node_id` fields.
- **No state-gated vote acceptance**: Resume votes are accepted regardless of gate state.
  Votes submitted during `Propagating` or even `Normal` state are counted.
- **Low quorum threshold in small clusters**: A 3-node cluster requires only 2 votes.
  A 5-node cluster requires only 3. These are trivially reachable with fake UUIDs.

### Recommendations (Priority: CRITICAL)

1. **Cluster membership verification**: `cast_vote()` must validate `node_id` against
   a known cluster membership set. Reject votes from unknown node IDs.
2. **Signed resume votes**: Require each resume vote to be signed by the node's Ed25519
   key. Verify the signature against the node's registered public key before accepting.
3. **State-gated vote acceptance**: Only accept resume votes when the gate is in
   `Confirmed` state (close fully propagated). Reject votes during `GateClosed` or
   `Propagating` to close the race window.
4. **Minimum quorum floor**: Enforce a minimum quorum of 3 regardless of cluster size
   to increase the cost of sybil attacks in small clusters.

---

## 8. Export Convergence Baseline Poisoning

### Architecture

`ExportAnalyzer` (ghost-export) imports ChatGPT/Claude conversation exports, reconstructs
a timeline of sessions, and computes per-session convergence scores. The scores feed into
the agent's baseline, which calibrates the convergence monitor's sensitivity.

The scoring heuristic combines two signals:

```
duration_signal = (duration_secs / 21600.0).min(1.0)   // 6h max → 1.0
msg_density     = (msg_count / (duration_secs / 60.0)).min(1.0)  // msgs/min, capped
estimated_score = (duration_signal * 0.5 + msg_density * 0.5).clamp(0.0, 1.0)
```

The `recommended_level` is derived from the maximum session score:
- \>0.85 → level 4
- \>0.7 → level 3
- \>0.5 → level 2
- \>0.3 → level 1
- ≤0.3 → level 0

### Attack vectors

| Attack | Description | Impact | Controllable? |
|---|---|---|---|
| Duration manipulation | Set session duration to exactly 6h (21600s) | `duration_signal` = 1.0 (maximum) | Yes — attacker controls message timestamps |
| Message density packing | 120 messages over 6h = 1 msg/3min | `msg_density` ≈ 0.33 | Yes — attacker controls message count and spacing |
| Rapid density burst | 60 messages in 1 minute | `msg_density` = 1.0 (clamped) | Yes — trivially craftable |
| Multi-session poisoning | 20 sessions × high scores | Elevated baseline across many sessions | Yes — attacker controls session count |
| Cross-format poisoning | Craft both ChatGPT and Claude exports | Multiple import vectors for same attack | Yes — both parsers accept crafted JSON |
| Recommended level elevation | Max session score > 0.85 | `recommended_level` = 4 (highest) | Yes — combine max duration + max density |

### Attacker-controllable vs uncontrollable signals

| Signal | Source | Controllable via export? |
|---|---|---|
| `duration_signal` | Message timestamps in export | Yes |
| `msg_density` | Message count / duration | Yes |
| `vocabulary_convergence` | Live interaction analysis | No — requires real-time agent behavior |
| `goal_boundary_erosion` | Live interaction analysis | No — requires real-time agent behavior |
| `behavioral_anomaly` | Live interaction analysis | No — requires real-time agent behavior |

The attacker can control 2 of 5 convergence signals via crafted exports. The remaining
3 signals require live interaction and cannot be pre-seeded.

### Mitigations present

1. **Score clamping**: All signals and the final score are clamped to [0.0, 1.0].
   An attacker cannot produce scores above 1.0.
2. **Flagged sessions**: Sessions with `estimated_score > 0.5` are added to
   `flagged_sessions` for review.
3. **Recommended level reporting**: The `recommended_level` provides a warning signal
   that can be surfaced to the user.
4. **Empty/malformed rejection**: Empty arrays produce zero sessions. Malformed JSON
   is rejected with an error.

### Gaps

- **No cryptographic verification of export authenticity**: The analyzer accepts any
  JSON that matches the ChatGPT/Claude schema. There is no signature, HMAC, or
  platform-issued token to verify the export was genuinely produced by the platform.
- **No statistical anomaly detection**: Perfectly uniform session patterns (identical
  duration, identical message counts) are not flagged as suspicious, even though real
  conversation exports exhibit natural variance.
- **No isolation between imported and live baselines**: Imported session scores are
  mixed directly into the baseline. A poisoned import permanently skews the baseline
  unless manually reset.
- **No rate limiting on imports**: An attacker can import multiple crafted exports
  to compound the poisoning effect.
- **No cross-referencing with platform APIs**: The analyzer does not verify that
  conversations actually exist on the source platform.
- **Duration signal ceiling too low**: 6 hours is a realistic session length, making
  it easy to max out `duration_signal` without appearing anomalous.

### Recommendations

1. **Untrusted import tier**: Treat imported baselines as "untrusted" with a separate
   trust tier. Apply a discount factor (e.g., 0.5×) to imported session scores until
   validated by live interaction patterns.
2. **Statistical outlier detection**: Flag imports where session patterns are
   suspiciously uniform (low variance in duration, message count, or spacing).
3. **Live session gating**: Require N live sessions (e.g., 5) before imported baseline
   data influences convergence scoring. This prevents immediate baseline poisoning.
4. **Import rate limiting**: Allow at most 1 import per 24h period to prevent
   compounding attacks.
5. **Platform verification**: Where possible, verify exports against platform APIs
   or require platform-signed export bundles.
