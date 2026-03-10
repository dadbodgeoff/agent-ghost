# Speculative Context Layer Design

> Codename: GHOST
> Date: 2026-03-10
> Status: Proposed architecture
> Scope: Low-latency context continuity during compaction, validation, and memory promotion

---

## 0. Summary

This document defines a new runtime layer: the **Speculative Context Layer**.

The problem is straightforward:

- full compaction is too expensive to run inline on every active chat turn
- memory validation and convergence gating are too important to bypass
- spawned or resumed agents still need coherent context immediately

The answer is not to weaken validation. The answer is to split context into three authorities:

1. **Live Session State**
   The active conversation tail and per-turn runtime state.
2. **Speculative Context**
   Temporary, non-authoritative, session-scoped candidate context produced quickly and validated asynchronously.
3. **Durable Memory**
   Fully validated, promoted, long-term memory.

This layer allows the conversation to continue while compaction, filtering, and promotion happen in the background.

The critical constraint is absolute:

**Speculative context must never become a backdoor around validation.**

---

## 1. Problem Statement

The current design intent in GHOST is correct:

- session compaction exists as a real subsystem
- memory writes are intended to flow through proposal validation
- the agent should see an immutable snapshot each run
- convergence state should shape what context the agent sees

But the hot path still has an architectural tension:

- if compaction runs fully inline, user-visible latency rises
- if compaction is deferred without a staging layer, continuity drops
- if candidate memories are exposed without controls, speculative poison can leak into the prompt

We need a way to:

- preserve immediate conversational continuity
- keep the prompt cleaner than raw chat history alone
- let candidate summaries and facts be available quickly
- preserve strict gating before durable promotion
- ensure the model cannot treat speculative context as canonical truth

---

## 2. Decision

Introduce a **Speculative Context Layer** between live session state and durable memory.

This layer will:

- accept candidate compacted context immediately after a turn
- expose a narrow subset of low-risk entries back to the runtime
- run fast safety gating before exposure
- run deeper validation asynchronously
- promote only approved items into durable memory
- expire aggressively

It is a context-assistance layer, not a memory authority.

Internal terminology should prefer **speculative context** over **speculative memory** to keep this boundary explicit.

---

## 3. Goals

- keep active chat latency low even when compaction is needed
- let resumed or spawned agents enter a chat with useful short-term context
- reduce prompt clutter by storing temporary compacted context outside the main conversation tail
- preserve full validation, convergence gating, and severity checks before durable promotion
- make every promoted memory traceable to specific source turns and messages
- prevent speculative content from recursively validating itself
- make failure modes observable and bounded

---

## 4. Non-Goals

- replacing the durable memory system
- replacing the proposal validation system
- allowing speculative entries to persist indefinitely
- allowing speculative entries to be shared broadly across unrelated sessions
- solving full cross-agent shared cognition in phase 1
- allowing the model to directly self-promote speculative content to durable memory

---

## 5. Architectural Model

### 5.1 The Three Stores

#### A. Live Session State

Contains:

- current and recent chat turns
- current run state
- in-flight tool results
- session-local execution metadata

Purpose:

- immediate continuity
- direct conversational replay

Properties:

- authoritative for the active turn
- noisy by nature
- bounded by context window and session retention policy

#### B. Speculative Context

Contains:

- compacted summaries
- candidate facts
- candidate goals
- candidate reflections
- tool-derived observations

Purpose:

- short-term continuity without forcing immediate durable promotion

Properties:

- non-authoritative
- session-scoped by default
- aggressively TTL-bound
- retrievable only under policy

#### C. Durable Memory

Contains:

- validated long-term facts
- approved goals
- approved reflections
- archived historical memory snapshots

Purpose:

- canonical memory authority

Properties:

- promotion-only from validated paths
- durable
- retrieval-weighted as truth-bearing context

### 5.2 Why This Split Exists

Without this split, the system faces a bad tradeoff:

- inline validation and compaction preserve safety but stall chat
- asynchronous compaction preserves chat but weakens context continuity
- ungated temporary context preserves continuity but weakens safety

The Speculative Context Layer keeps the fast path fast and the safety path strict.

---

## 6. Core Invariants

These invariants are mandatory. If any are violated, the design has failed.

### 6.1 Authority Invariants

- Durable memory is always authoritative over speculative context.
- Speculative context is never treated as truth by default.
- Live session state is authoritative only for the current conversational timeline, not for long-term memory.

### 6.2 Exposure Invariants

- High-severity speculative entries are never exposed back to the model.
- Speculative entries must pass a fast gate before they become retrievable.
- Blocked entries are excluded from all retrieval paths, not merely hidden in UI.

### 6.3 Provenance Invariants

- Every speculative entry must include `agent_id`, `session_id`, `turn_id`, source references, and `created_at`.
- Every promoted durable memory must reference the speculative entry and original source material that produced it.
- Entries without provenance must be blocked or dropped.

### 6.4 Isolation Invariants

- Phase 1 speculative retrieval is same-session only.
- Cross-session speculative retrieval is forbidden until promotion quality is proven.
- Cross-agent speculative retrieval is forbidden by default.

### 6.5 Validation Invariants

- Speculative entries may not validate or cite other speculative entries as authoritative evidence.
- Promotion requires the full validation pipeline.
- A model-generated candidate must never self-certify its own truthfulness.

### 6.6 Lifetime Invariants

- Speculative entries must expire automatically.
- Expired speculative entries must be removed from retrieval.
- Speculative entries may outlive the immediate turn only for bounded operational reasons, not as a substitute for durable memory.

### 6.7 Retrieval Invariants

- Retrieval must rank durable memory above speculative context.
- Contradicted speculative entries must be suppressed.
- Retrieval from speculative context must operate under a smaller token budget than durable memory.

---

## 7. Data Model

The implementation should begin with four logical tables.

### 7.1 `context_attempts`

Primary record for speculative context candidates.

Suggested columns:

- `id TEXT PRIMARY KEY`
- `agent_id TEXT NOT NULL`
- `session_id TEXT NOT NULL`
- `turn_id TEXT NOT NULL`
- `attempt_kind TEXT NOT NULL`
- `content TEXT NOT NULL`
- `redacted_content TEXT`
- `status TEXT NOT NULL`
- `severity REAL NOT NULL DEFAULT 0.0`
- `confidence REAL NOT NULL DEFAULT 0.0`
- `retrieval_weight REAL NOT NULL DEFAULT 0.0`
- `source_refs TEXT NOT NULL`
- `source_hash BLOB`
- `contradicted_by_memory_id TEXT`
- `promotion_candidate INTEGER NOT NULL DEFAULT 0`
- `expires_at TEXT NOT NULL`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

`attempt_kind` initial enum:

- `summary`
- `fact_candidate`
- `goal_candidate`
- `reflection_candidate`
- `tool_observation`

`status` initial enum:

- `pending`
- `retrievable`
- `flagged`
- `blocked`
- `promoted`
- `expired`

### 7.2 `context_attempt_validation`

Append-only record of each validation/gating decision.

Suggested columns:

- `id TEXT PRIMARY KEY`
- `attempt_id TEXT NOT NULL`
- `gate_name TEXT NOT NULL`
- `decision TEXT NOT NULL`
- `reason TEXT`
- `score REAL`
- `details_json TEXT`
- `created_at TEXT NOT NULL`

This table exists for:

- auditability
- debugging false positives
- measuring promotion quality

### 7.3 `context_attempt_promotion`

Maps speculative entries to durable memory records.

Suggested columns:

- `id TEXT PRIMARY KEY`
- `attempt_id TEXT NOT NULL`
- `promoted_memory_id TEXT NOT NULL`
- `promotion_type TEXT NOT NULL`
- `created_at TEXT NOT NULL`

### 7.4 `context_attempt_jobs`

Tracks async work for compaction and validation.

Suggested columns:

- `id TEXT PRIMARY KEY`
- `attempt_id TEXT NOT NULL`
- `job_type TEXT NOT NULL`
- `status TEXT NOT NULL`
- `retry_count INTEGER NOT NULL DEFAULT 0`
- `last_error TEXT`
- `run_after TEXT NOT NULL`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

`job_type` initial enum:

- `fast_gate`
- `deep_validate`
- `embed`
- `promote`
- `expire`

---

## 8. Lifecycle

### 8.1 Hot Path: User Message to Response

1. User sends message.
2. Runtime hydrates:
   - live session tail
   - durable memory
   - allowed speculative context
   - convergence state
3. Agent responds.
4. Post-turn compactor emits candidate summaries and facts.
5. Candidate entries are written immediately to `context_attempts`.
6. A fast gate runs with a tight latency budget.
7. Low-risk entries become `retrievable`.
8. High-risk entries become `blocked` or `flagged`.
9. Chat continues without waiting for deep validation.

### 8.2 Background Path: Validation and Promotion

1. Deep validation jobs consume `pending`, `retrievable`, and `flagged` entries.
2. Validation checks run:
   - provenance presence
   - contradiction against durable memory
   - convergence-aware filtering
   - self-reference limits
   - scope expansion checks
   - emulation and severity checks
3. Eligible entries are marked `promotion_candidate`.
4. Promotion worker writes approved items into durable memory.
5. `context_attempt_promotion` is recorded.
6. Speculative entry becomes `promoted`.
7. Expiration job marks old entries `expired` and removes them from retrieval.

---

## 9. State Machine

### 9.1 States

- `pending`
- `retrievable`
- `flagged`
- `blocked`
- `promoted`
- `expired`

### 9.2 Allowed Transitions

- `pending -> retrievable`
- `pending -> flagged`
- `pending -> blocked`
- `retrievable -> flagged`
- `retrievable -> blocked`
- `retrievable -> promoted`
- `flagged -> blocked`
- `flagged -> promoted`
- `pending -> expired`
- `retrievable -> expired`
- `flagged -> expired`

### 9.3 Forbidden Transitions

- `blocked -> retrievable`
- `blocked -> promoted`
- `expired -> retrievable`
- `promoted -> retrievable`

Once an item is blocked, it stays non-retrievable. Recovery means regenerating a new attempt from source, not resurrecting a blocked one.

---

## 10. Retrieval Policy

Retrieval order should be:

1. live session state
2. durable memory
3. speculative context

This is not just ordering. It is authority ranking.

### 10.1 Phase 1 Retrieval Filters

An attempt is retrievable only if all conditions hold:

- `status = retrievable`
- `session_id = active session`
- `expires_at > now`
- `severity < configured threshold`
- `contradicted_by_memory_id IS NULL`
- provenance fields are present

### 10.2 Retrieval Weighting

Suggested initial weighting:

- durable memory multiplier: `1.0`
- speculative context multiplier: `0.35` to `0.55`

Speculative context must help continuity without dominating retrieval.

### 10.3 Token Budget

Speculative context gets a hard smaller budget than durable memory.

Suggested initial limits:

- live session tail: primary conversational budget
- durable memory: standard L7 memory budget
- speculative context: 10% to 20% of the memory/context budget

### 10.4 Contradiction Handling

If a speculative entry conflicts with durable memory:

- durable memory wins
- speculative entry is suppressed
- validation record is written
- the entry may remain for audit but not retrieval

---

## 11. Validation Policy

Validation happens in two tiers.

### 11.1 Tier A: Fast Gate

Purpose:

- protect the prompt quickly
- preserve chat responsiveness

Latency target:

- milliseconds, not seconds

Checks:

- empty or malformed content
- provenance presence
- size and token sanity
- duplicate candidate detection
- credential and secret leakage
- extreme safety severity
- emulation-language hard fail
- obvious policy category disallow

Outputs:

- `retrievable`
- `flagged`
- `blocked`

### 11.2 Tier B: Deep Validation

Purpose:

- decide promotion eligibility
- detect subtle poison and drift

Checks:

- contradiction against durable memory
- convergence-aware filtering
- citation integrity
- self-reference density
- scope expansion
- memory type eligibility
- archival or suppression rules

Outputs:

- `promoted`
- `flagged`
- `blocked`
- `expired`

### 11.3 Validation Rule: No Speculative Citation Chains

This is a hard rule.

Speculative entries may include source references to:

- user messages
- assistant messages
- tool outputs
- durable memory IDs

They may not use other speculative entries as authoritative citations for promotion.

Otherwise the system can create self-reinforcing hallucination loops.

---

## 12. Promotion Policy

Promotion converts speculative context into durable memory.

### 12.1 Promotion Preconditions

All must be true:

- attempt is not blocked or expired
- provenance is complete
- contradiction checks pass
- convergence policy allows the target memory type
- validation decision is promotion-eligible
- memory type is approved for durable storage

### 12.2 Promotion Write Path

The promotion worker must:

1. create the durable memory record
2. create the durable snapshot/event entries
3. write promotion linkage
4. mark the speculative attempt `promoted`
5. emit observability events

The promotion worker is the only component allowed to cross the boundary from speculative context to durable memory.

### 12.3 Promotion Rule by Type

Initial guidance:

- `summary`: usually do not promote directly unless transformed into a validated session summary artifact
- `fact_candidate`: eligible for promotion
- `goal_candidate`: eligible only through goal validation path
- `reflection_candidate`: eligible only through reflection limits and reflection validation path
- `tool_observation`: eligible only if provenance is concrete and policy allows durable retention

---

## 13. Concurrency and Latency Model

### 13.1 The Fast Path Must Stay Small

The request path may:

- write attempt rows
- run a bounded fast gate
- update retrieval eligibility

The request path may not:

- block on deep validation
- block on embeddings for large batches
- block on promotion
- block on long compaction summarization retries

### 13.2 Session Isolation

Speculative writes and retrieval are session-scoped in phase 1.

One session's speculative backlog must not block another session's chat path.

### 13.3 Backpressure

If speculative jobs back up:

- chat continues
- new attempts may be downgraded in richness
- promotion latency increases
- operators are alerted

The system must degrade by reducing speculative quality, not by bypassing validation.

---

## 14. Failure Model

### 14.1 If the Fast Gate Fails

- do not expose the attempt
- mark it `blocked` or leave it `pending`
- emit a validation failure record

### 14.2 If Deep Validation Fails

- suppress promotion
- mark the attempt `flagged` or `blocked`
- keep provenance for audit

### 14.3 If Job Processing Is Delayed

- speculative entries may remain retrievable only until TTL
- expired items must drop out of retrieval automatically

### 14.4 If Durable Memory Write Fails During Promotion

- do not mark attempt `promoted`
- retain retryable promotion job state
- prevent partial authority crossing

### 14.5 If Contradiction Detection Is Unavailable

- fail closed for promotion
- optionally allow continued `retrievable` status only for same-session continuity
- never promote during degraded contradiction state

---

## 15. Security Considerations

### 15.1 Why This Layer Is Dangerous

This layer improves continuity, but it also creates a tempting bypass:

- generated content becomes retrievable quickly
- retrievable content influences future prompts
- future prompts may generate more content from that influence

Without strict separation, this becomes recursive self-poisoning.

### 15.2 Controls Required

- same-session scope first
- low retrieval weight
- TTL expiration
- blocked entries fully suppressed
- no speculative citation chains
- durable memory always outranks speculative context
- promotion through one guarded path only

### 15.3 Severity Policy

Any attempt with:

- credential leakage
- explicit emulation language above threshold
- high-risk manipulation content
- missing provenance

must be blocked immediately from retrieval.

---

## 16. Metrics and SLOs

Track these from day one.

### 16.1 Latency

- attempt write latency
- fast gate latency
- added p95 chat latency with speculative layer enabled

### 16.2 Quality

- speculative retrieval hit rate
- promotion rate
- contradiction rate
- blocked rate
- false-positive flag rate

### 16.3 Safety

- blocked entry prompt leak count
- promotion failure count
- speculative citation-chain violation count
- expired but still retrievable count

### 16.4 Capacity

- queue depth by job type
- average attempt count per session
- average TTL expiration backlog

Suggested initial SLO:

- speculative layer adds negligible user-visible latency relative to baseline chat path

---

## 17. Rollout Plan

### Phase 1: Session-Scoped Summaries Only

Deliver:

- `context_attempts`
- fast gate
- same-session retrieval for `summary` only
- TTL expiration

Do not deliver:

- durable promotion
- cross-session retrieval
- cross-agent retrieval

Exit criteria:

- no blocked-attempt prompt leaks
- chat latency remains acceptable

### Phase 2: Deep Validation

Deliver:

- validation job table
- contradiction checks
- convergence-aware filtering
- flagged and blocked transitions

Exit criteria:

- stable contradiction handling
- low false-positive rate

### Phase 3: Controlled Promotion

Deliver:

- promotion worker
- promotion linkage records
- durable write path for approved `fact_candidate` items

Exit criteria:

- promotion precision is high
- rollback and retry semantics are proven

### Phase 4: Spawn-Aware Hydration

Deliver:

- new runtime hydration path that assembles:
  - live session tail
  - durable memory
  - allowed speculative context
  - convergence state

Exit criteria:

- resumed and spawned agents show higher continuity without prompt bloat

### Phase 5: Expanded Scope

Possible future work:

- carefully controlled cross-session retrieval
- cross-agent derived context in tightly scoped workflows

This phase should not begin until the safety properties of phases 1 through 4 are demonstrated with real metrics.

---

## 18. Integration Guidance for Current GHOST Runtime

This design should plug into the existing runtime in a narrow way.

### 18.1 Writer

After each turn, compaction emits candidate entries into `context_attempts`.

### 18.2 Hydrator

Before each turn, a hydrator assembles:

- conversation tail
- durable memory
- speculative context
- convergence state

and provides it to the runner as structured input, not as ad hoc agent-to-agent prose.

### 18.3 Validator

Fast gate runs in the request-adjacent path.

Deep gate runs in the job system.

### 18.4 Promoter

Promotion is the only bridge from speculative context to durable memory.

---

## 19. Open Questions

- Should summaries ever be promoted directly, or always re-materialized as a separate durable artifact?
- Should `retrievable` speculative context be visible in operator tooling as prompt-visible or only database-visible?
- What TTL is correct by attempt kind?
- Should the system keep redacted and raw content for blocked entries, or immediately purge raw content after audit?
- How should contradiction scoring interact with convergence level when durable memory itself is uncertain?

---

## 20. Final Position

This design is feasible and worth building.

The key reason is that it removes the false binary between:

- low-latency chat
- strict memory validation

We do not need to choose one or the other.

We can keep chat fast by introducing a temporary, non-authoritative context layer.
We can keep safety strong by making that layer:

- provenance-bound
- low-weight
- TTL-limited
- validation-gated
- promotion-controlled

If implemented with these invariants intact, the Speculative Context Layer becomes a practical bridge between raw conversation and durable memory rather than a new prompt-injection surface.
