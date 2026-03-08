# Production Audit Remediation

## Objective

Close the verified production-risk gaps in idempotency, crash recovery, workflow durability, backup integrity, messaging authenticity, distributed kill-gate coordination, skill-catalog mutation safety, and persisted-state/schema contracts.

## Scope

Primary code paths:
- `crates/ghost-backup`
- `crates/ghost-gateway/src/messaging`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/workflows.rs`
- `crates/ghost-gateway/src/api/skills.rs`
- `crates/ghost-gateway/src/api/skill_execute.rs`
- `crates/ghost-gateway/src/skill_catalog`
- `crates/cortex/cortex-storage`
- `crates/ghost-kill-gates`
- `crates/ghost-gateway/src/safety`

Related contracts:
- operation journal
- live execution records
- stream event log
- workflow execution persistence
- migration/schema verification
- distributed kill-gate quorum/resume

## Constraints

- Fail closed on persistence, auth, ownership, schema, and replay uncertainty.
- No placeholders in production paths for encryption, signatures, or quorum trust.
- No silent defaulting for persisted recovery state.
- No replayable API or stream event may be emitted before durable persistence succeeds.
- Mixed-version deploys must have an explicit compatibility plan or a temporary feature gate.
- Every remediation task must land with adversarial tests, not only happy-path tests.

## Findings Summary

1. Operation-journal lease takeover is unsafe: stale owners can still commit after takeover and overwrite the winning result.
2. Agent chat stream replay durability is false in the live SSE path: event persistence writes through read-only connections and failures are silently ignored.
3. Workflow durability/recovery is largely fake: resume reruns from scratch, persist failures are ignored, corrupt JSON silently defaults, and unknown/skipped nodes can still produce `"completed"`.
4. Backup export/import ships security placeholders and restore integrity holes: XOR “encryption”, unsigned archives, path traversal restore, extra-entry restore, non-atomic restore, and no SQLite-consistent snapshot.
5. Inter-agent messaging authenticity/confidentiality is fake: unsigned messages are accepted, “encryption” is plaintext pass-through, and key registration is only claimed in comments/logs.
6. Distributed kill-gate quorum is unauthenticated and fail-open: any UUID can count as a resume vote, and the bridge still contains transport placeholders.
7. Messaging replay prevention has an hourly-reset replay hole that re-admits recent messages across the reset boundary.
8. Skill-catalog install/uninstall/execute routes mutate state without the operation journal, durable provenance, or retry-safe contracts.
9. Persisted recovery state and schema verification are under-specified: recovery-critical JSON is unversioned and migration “schema verified” only proves table existence for several critical tables.

## Phased Implementation Plan

### Phase 0: Containment

Goal: stop claiming guarantees the code does not provide.

1. Gate or disable unsafe resume/replay surfaces until they are truthful.
   Dependencies: none.
   Tasks:
   - Return `409`/`501` for workflow resume until real step-level resume exists.
   - Return explicit recovery-required/error on stream persistence failure instead of silently continuing.
   - Disable distributed kill-gate resume unless authenticated cluster membership is enforced.
   Acceptance criteria:
   - `/api/workflows/:id/resume/:execution_id` no longer reruns from scratch under the name “resume”.
   - SSE replay contract is either real or explicitly refused.
   - Distributed resume cannot reopen the gate with unauthenticated votes.

### Phase 1: Ownership, Idempotency, and Crash-Recovery Correctness

Goal: exactly-once semantics under retries, contention, slow execution, and restart.

2. Harden `operation_journal` ownership and lease semantics.
   Dependencies: Phase 0.
   Tasks:
   - Add durable owner token / lease epoch to `operation_journal`.
   - Make takeover, commit, and abort compare-and-set on `(id, owner_token, status='in_progress')`.
   - Add lease renewal/heartbeat for long-running mutations.
   - Treat ownership loss as a hard error; stale owners must not commit.
   Acceptance criteria:
   - A worker that loses ownership cannot commit or delete the row.
   - A long-running request that is still healthy renews its lease and is not spuriously taken over.
   - Concurrent retries produce one committed response body and one authoritative audit trail.

3. Make agent chat stream replay durable and fail closed.
   Dependencies: task 2.
   Tasks:
   - Persist replayable stream events with a write-capable connection.
   - Persist before emitting replayable SSE events or broadcasting replay-relevant websocket sequence numbers.
   - Record durable terminal/recovery-required state for the stream route.
   - Remove `unwrap_or(0)` sequence-number fallback for persisted stream events.
   Acceptance criteria:
   - Restart replay returns `stream_start`, all persisted text/tool events, and terminal event without provider re-execution.
   - Persistence failure produces an explicit recovery-required contract, not silent live-only delivery.
   - No live event with replay semantics is emitted without durable storage success.

4. Replace fake workflow durability with real resumable execution state.
   Dependencies: tasks 2-3.
   Tasks:
   - Introduce typed/versioned execution records with step-level durable progress and outcome state.
   - Link workflow execution state to operation-journal ownership.
   - Resume from the last durably committed step only; do not rerun completed side effects.
   - Route workflow agent/tool execution through the same runtime-safety and provider construction path as live chat/studio execution.
   - Treat unknown node types, missing providers, skipped critical nodes, and state-persist failures as execution failure, not `"completed"`.
   Acceptance criteria:
   - Crash after step N resumes at step N+1 when safe, or returns explicit manual-recovery requirement when not safe.
   - Duplicate retry does not rerun completed steps or duplicate side effects.
   - Workflow status cannot be `"completed"` if any required node was skipped, unknown, or semantically invalid.

### Phase 2: Security Truthfulness

Goal: no fake crypto, no fake signatures, no unauthenticated coordination.

5. Replace backup placeholders with authenticated, crash-safe backup/restore.
   Dependencies: none.
   Tasks:
   - Define backup format v2 with explicit versioning and authenticated encryption.
   - Refuse empty-passphrase/unsigned production backups.
   - Validate exact manifest/data set equality.
   - Constrain restore paths to a staged subtree and atomically swap into place.
   - Use SQLite backup API or equivalent consistent snapshot for live DB state.
   Acceptance criteria:
   - Tampered archive fails before restore.
   - Path traversal and extra archive entries are rejected.
   - Restore either completes fully or leaves the original state untouched.

6. Make inter-agent messaging actually authenticated and confidential.
   Dependencies: none.
   Tasks:
   - Require valid signatures for accepted messages; reject empty signatures.
   - Implement real key registration/loading instead of bootstrap-only log messages.
   - Replace plaintext “encryption” with AEAD or remove the encrypted flag until real crypto exists.
   - Bind replay protection to authenticated sender identity.
   Acceptance criteria:
   - Unsigned, forged, and tampered messages are rejected.
   - “Encrypted” messages are not plaintext and fail to decrypt on tamper.
   - Bootstrap/runtime key registry is exercised by tests, not only comments.

7. Fix distributed kill-gate quorum and transport trust.
   Dependencies: none.
   Tasks:
   - Enforce cluster membership validation for acks and resume votes.
   - Authenticate relay messages and bind votes to known node identities.
   - Remove or gate code paths that only build relay messages without transport delivery.
   - Define partition/rejoin behavior and chain verification requirements for resume.
   Acceptance criteria:
   - Fake node IDs cannot reach quorum.
   - Duplicate votes and unknown peers do not affect quorum.
   - Resume requires authenticated votes from current cluster members only.

8. Close the dispatcher replay hole.
   Dependencies: task 6.
   Tasks:
   - Replace hourly wholesale nonce reset with timestamped eviction bounded by replay window.
   - Separate replay cache cleanup from rate-limit counter cleanup.
   Acceptance criteria:
   - A message accepted immediately before the cleanup boundary is still rejected on replay immediately after the boundary if it remains inside the replay window.

### Phase 3: Contracted Mutation Surfaces and Persisted-State Compatibility

Goal: retry-safe APIs, versioned recovery state, and migrations that actually verify the contract.

9. Put skill-catalog mutations/execution under the same mutation contract as other write paths.
   Dependencies: task 2.
   Tasks:
   - Require operation context for install/uninstall/execute routes.
   - Journal and audit skill-catalog mutations and write-capable executions.
   - Define which skills require client-generated ids or explicit idempotency keys for side effects.
   Acceptance criteria:
   - Retried install/uninstall requests replay the same response instead of mutating twice.
   - Retried write-capable skill execution is exactly-once or explicitly rejected as non-idempotent.
   - Audit records reconstruct actor, request, operation, and idempotency provenance.

10. Version persisted recovery state and strengthen schema verification.
    Dependencies: tasks 2-4.
    Tasks:
    - Add `state_version` or typed columns for `workflow_executions` and `live_execution_records`.
    - Reject unknown state versions/shapes instead of silently defaulting.
    - Expand schema contract checks for `workflows`, `stream_event_log`, `workflow_executions`, `operation_journal`, and `live_execution_records` to verify required columns, indexes, and constraints.
    - Make migration receipts truthful: “schema verified” must mean the recovery contract is actually present.
    Acceptance criteria:
    - Corrupt or unknown persisted state fails closed with explicit diagnostics.
    - A database missing a critical recovery column/index fails schema verification.
    - Mixed-version fixtures either migrate cleanly or are explicitly rejected with actionable errors.

### Phase 4: Rollout and Verification

Goal: land high-risk fixes without corrupting existing state or breaking mixed-version deployments.

11. Execute staged rollout with migration and recovery drills.
    Dependencies: tasks 2-10.
    Tasks:
    - Add forward-compatible migrations first, then flip enforcement behind feature flags.
    - Run canary with metrics on takeover conflicts, replay failures, recovery-required transitions, backup verify failures, signature reject counts, and quorum vote rejects.
    - Perform crash/restart drills and mixed-version upgrade tests before broad rollout.
    Acceptance criteria:
    - No canary duplicate execution under forced retry/timeout scenarios.
    - Recovery drills produce deterministic post-restart state.
    - Feature flags allow rapid fail-closed rollback of new contracts if verification fails.

## Test Matrix

- Operation journal:
  - stale owner commit after takeover
  - lease renewal during >30s execution
  - crash after side effect before journal commit
  - crash after journal commit before HTTP response
  - concurrent retries from two workers
- Agent chat stream:
  - read-only/write failure during event append
  - crash after `stream_start`, mid-text, mid-tool event, and before terminal event
  - replay after restart with no provider re-execution
  - websocket sequence numbers remain monotonic and persisted
- Workflows:
  - crash between steps
  - resume from prior execution id
  - unknown node type
  - missing API key/provider
  - corrupt stored JSON
  - mixed-version stored state fixture
- Backup:
  - tampered ciphertext
  - wrong passphrase
  - extra data entries
  - `../` traversal path
  - restore interrupted mid-run
  - live SQLite/WAL writes during backup
- Messaging:
  - unsigned message
  - wrong-key signature
  - ciphertext tamper
  - replay across cleanup boundary
  - future-dated and expired messages
- Kill gate:
  - fake node IDs
  - duplicate vote from same node
  - unknown peer ack
  - partition then rejoin
  - resume with stale membership view
- Skill catalog:
  - duplicate install retry
  - duplicate uninstall retry
  - write-capable execute retry
  - audit provenance presence
- Migrations/schema:
  - missing critical columns/indexes
  - upgrade from pre-v051/v052/v053 states
  - migration receipt emitted only after full contract verification

## Migration and Rollout Notes

- `operation_journal` changes require additive migration first. Do not remove legacy columns until all writers compare-and-set on the new ownership fields.
- `workflow_executions` and `live_execution_records` need explicit state versioning. Old rows must either be upgraded in place or marked non-resumable.
- Backup format change should be versioned. Keep legacy import behind an explicit compatibility flag and never restore legacy archives silently.
- Distributed kill-gate resume must remain disabled unless authenticated cluster membership is configured and verified.
- Skill route enforcement can roll out in two steps:
  - accept missing idempotency headers with warnings for existing clients
  - switch to hard enforcement once clients are updated

## Open Questions

- Is `ghost-backup` currently reachable in production deployments or only via operator tooling? The code must still be fixed before treating exported archives as trustworthy.
- Do any external clients rely on the current broken behavior of workflow “resume”? If yes, they need a compatibility notice before the route is changed or gated.
- What is the authoritative cluster-membership source for distributed kill-gate quorum? The current code has no trusted membership binding.
- Which skill executions are intended to be replay-safe, and which require caller-supplied ids for exactly-once writes?

## Do Not Regress

- No stale owner may commit after lease takeover.
- No replayable stream event may be emitted before durable append succeeds.
- No workflow may report `"completed"` when a required node failed, was skipped, or was unknown.
- No backup restore may write outside the staged ghost root.
- No unsigned or forged inter-agent message may be accepted.
- No unauthenticated node ID may count toward kill-gate quorum.
- No mutating skill route may bypass operation journal and mutation audit.
- No recovery-critical persisted JSON may silently default on parse/version mismatch.
- No migration receipt may claim schema verification without verifying the recovery-critical columns and indexes.
