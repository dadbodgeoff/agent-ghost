# Proposal Lineage And Decision State Machine Design

## Status

- Author: Codex
- Date: 2026-03-07
- Scope: goal proposal storage, gateway decision routes, agent-loop proposal persistence, dashboard and SDK contract, timeout and supersession handling
- Depends on: [REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md)

## Executive Summary

The current proposal system has the right idea but the wrong canonical shape.

The domain already models richer outcomes such as `HumanReviewRequired`, `TimedOut`, and `Superseded`. The canonical storage does not. Today `goal_proposals` is one mutable row with `decision`, `resolved_at`, and `resolver`, and the gateway approves or rejects by proposal ID alone. Supersession is remembered in process memory by `goal_text`, not in storage.

That means the system cannot reliably answer the only question that matters for a human decision:

> what exact proposal, against what exact reviewed revision, in what exact lineage, was the human approving or rejecting?

This design fixes that by making three changes:

1. split immutable proposal facts from append-only lifecycle transitions
2. separate validation outcome from lifecycle state
3. require human decisions to be bound to expected state, subject lineage, and reviewed revision

The result is a canonical decision path that is replay-safe, stale-safe, and restart-safe.

## Current State Audit

### Repo observations

- `ProposalDecision` already includes `AutoApproved`, `AutoRejected`, `HumanReviewRequired`, `ApprovedWithFlags`, `TimedOut`, and `Superseded`.
- `goal_proposals` stores only one mutable `decision` field plus `resolved_at` and `resolver`.
- `resolve_proposal()` updates one row with `WHERE id = ?1 AND resolved_at IS NULL`.
- `approve_goal()` and `reject_goal()` accept only a proposal ID from the path and no expected revision or lineage data.
- the agent loop persists proposals once and does not persist supersession lineage
- `ProposalRouter::check_superseding()` supersedes by `content.goal_text` in memory only
- goal proposal identity is inconsistent even in fixtures and tests: some payloads use `goal_text`, some use `goal`

### What this means

The current model conflates three different things into one column:

- validation outcome
- human review lifecycle
- final terminal result

That creates four failure modes:

- retry-after-commit and stale approval look the same
- restart loses supersession lineage that only existed in memory
- text-similar goals can be incorrectly treated as the same logical subject
- old clients can approve a proposal without proving they reviewed the current version

## Design Principles

1. Proposal content is immutable once created.
2. Proposal lifecycle is append-only.
3. Validation outcome and review outcome are separate concepts.
4. Human decisions must be bound to the reviewed subject revision.
5. Supersession must exist in storage, not only in process memory.
6. Gateway transitions must be transactional and idempotent.
7. Legacy data may be backfilled, but historical certainty must not be fabricated.

## Target Model

### Concept split

The system should track three separate concepts:

- immutable proposal facts
- lifecycle transitions for that proposal
- canonical subject lineage and current revision

### Immutable proposal facts

A proposal row captures what was proposed and what the proposer reviewed.

It should never be mutated after insert.

### Lifecycle transitions

A transition row captures how proposal state changed:

- created as pending review
- auto-applied
- auto-rejected
- approved by human
- rejected by human
- superseded by a newer proposal
- timed out

The transition log is append-only.

### Subject lineage

Goal changes need a stable subject identity. Text is not enough.

Every proposal must belong to a canonical subject lineage identified by:

- `subject_type`
- `subject_key`
- `lineage_id`
- `reviewed_revision`

For goal changes, `subject_key` is the stable identity of the goal being changed. It must not be raw `goal_text`.

## State Machine

### Validation disposition

Validation disposition is stored independently from lifecycle state.

Allowed values:

- `auto_apply`
- `auto_reject`
- `human_review_required`

`ApprovedWithFlags` maps to:

- `validation_disposition = auto_apply`
- `validation_flags != []`

### Lifecycle state

Allowed lifecycle states:

- `pending_review`
- `auto_applied`
- `auto_rejected`
- `approved`
- `rejected`
- `superseded`
- `timed_out`

### Allowed transitions

Creation paths:

- new proposal -> `pending_review`
- new proposal -> `auto_applied`
- new proposal -> `auto_rejected`

Human review paths:

- `pending_review` -> `approved`
- `pending_review` -> `rejected`

System paths:

- `pending_review` -> `superseded`
- `pending_review` -> `timed_out`

Terminal rules:

- `auto_applied`, `auto_rejected`, `approved`, `rejected`, `superseded`, and `timed_out` are terminal
- terminal states cannot transition further
- exactly one terminal state may exist for one proposal

## Storage Design

### Table 1: `goal_proposals_v2`

This is the immutable fact table.

```sql
CREATE TABLE goal_proposals_v2 (
    id                    TEXT PRIMARY KEY,
    lineage_id            TEXT NOT NULL,
    subject_type          TEXT NOT NULL,
    subject_key           TEXT NOT NULL,
    reviewed_revision     TEXT NOT NULL,
    proposer_type         TEXT NOT NULL,
    proposer_id           TEXT,
    agent_id              TEXT NOT NULL,
    session_id            TEXT NOT NULL,
    operation             TEXT NOT NULL,
    target_type           TEXT NOT NULL,
    content               TEXT NOT NULL,
    cited_memory_ids      TEXT NOT NULL DEFAULT '[]',
    validation_disposition TEXT NOT NULL,
    validation_flags      TEXT NOT NULL DEFAULT '[]',
    validation_scores     TEXT NOT NULL DEFAULT '{}',
    denial_reason         TEXT,
    supersedes_proposal_id TEXT,
    operation_id          TEXT,
    request_id            TEXT,
    created_at            TEXT NOT NULL,
    event_hash            BLOB NOT NULL,
    previous_hash         BLOB NOT NULL
);

CREATE INDEX idx_goal_proposals_v2_lineage
    ON goal_proposals_v2(lineage_id, created_at);

CREATE INDEX idx_goal_proposals_v2_subject
    ON goal_proposals_v2(subject_type, subject_key, created_at);
```

Important notes:

- `reviewed_revision` is the revision token of the subject the proposer claims to have reviewed
- `supersedes_proposal_id` links a newer proposal to the pending one it replaced
- `operation_id` and `request_id` come from the operation identity layer

### Table 2: `goal_proposal_transitions`

This is the append-only lifecycle log.

```sql
CREATE TABLE goal_proposal_transitions (
    id                    TEXT PRIMARY KEY,
    proposal_id           TEXT NOT NULL,
    lineage_id            TEXT NOT NULL,
    from_state            TEXT,
    to_state              TEXT NOT NULL,
    actor_type            TEXT NOT NULL,
    actor_id              TEXT,
    reason_code           TEXT,
    rationale             TEXT,
    expected_state        TEXT,
    expected_revision     TEXT,
    operation_id          TEXT,
    request_id            TEXT,
    idempotency_key       TEXT,
    created_at            TEXT NOT NULL
);

CREATE INDEX idx_goal_proposal_transitions_proposal
    ON goal_proposal_transitions(proposal_id, created_at);

CREATE INDEX idx_goal_proposal_transitions_lineage
    ON goal_proposal_transitions(lineage_id, created_at);

CREATE UNIQUE INDEX idx_goal_proposal_single_terminal
    ON goal_proposal_transitions(proposal_id)
    WHERE to_state IN (
        'auto_applied',
        'auto_rejected',
        'approved',
        'rejected',
        'superseded',
        'timed_out'
    );
```

Important notes:

- this table records who caused the transition and under what expected preconditions
- terminal uniqueness is enforced in storage, not only in handler code

### Table 3: `goal_lineage_heads`

This is a transactional projection for concurrency control and fast reads.

It is not the source of truth.

```sql
CREATE TABLE goal_lineage_heads (
    subject_type          TEXT NOT NULL,
    subject_key           TEXT NOT NULL,
    lineage_id            TEXT NOT NULL,
    head_proposal_id      TEXT NOT NULL,
    head_state            TEXT NOT NULL,
    current_revision      TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    PRIMARY KEY (subject_type, subject_key)
);
```

This table allows the gateway to answer quickly:

- what proposal is currently authoritative for this lineage?
- what revision is current?
- is there still a pending review head?

## Why this shape is better than mutating `goal_proposals`

The current update-in-place model has a structural problem: it loses the distinction between the original proposal facts and the later human or system action that resolved it.

An append-only transition model gives the system:

- immutable original evidence
- durable supersession and timeout history
- one place to attach operation identity
- easier incident forensics
- stronger invariant enforcement

## Subject Identity And Lineage

### Requirement

`goal_text` is not a durable identity.

Two different proposals can share text. One proposal can also revise wording without changing the subject. Text-based supersession is therefore not strong enough.

### New contract

Each proposal must carry or be assigned:

- `subject_type`
- `subject_key`
- `lineage_id`
- `reviewed_revision`

### For goal changes

Best-case target contract:

```json
{
  "operation": "GoalChange",
  "target_type": "AgentGoal",
  "content": {
    "subject_key": "goal:agent:123:primary",
    "reviewed_revision": "rev-42",
    "goal_text": "learn Rust"
  }
}
```

Compatibility fallback:

- if old clients only send `goal_text`, the gateway may derive a temporary legacy `subject_key`
- legacy derived keys must be marked as low-confidence
- phase 3 should reject reviewable goal changes that do not provide a stable subject reference

## Human Decision API

### New request body

The current `POST /approve` and `POST /reject` shape is too weak because it sends only a proposal ID.

Target request body:

```json
{
  "expected_state": "pending_review",
  "expected_lineage_id": "ln_123",
  "expected_subject_key": "goal:agent:123:primary",
  "expected_reviewed_revision": "rev-42",
  "rationale": "optional human note"
}
```

The action itself can still be represented by the route:

- `POST /api/goals/:id/approve`
- `POST /api/goals/:id/reject`

Or by a unified route:

- `POST /api/goals/:id/decision`

Either approach is fine. The important part is that the gateway receives the expected preconditions.

### Gateway transition rules

Approve or reject is allowed only if all are true:

- the proposal exists
- current lifecycle state is `pending_review`
- the proposal is still the active lineage head
- request `expected_lineage_id` matches stored lineage
- request `expected_subject_key` matches stored subject
- request `expected_reviewed_revision` matches stored reviewed revision
- operation identity and idempotency checks pass

If any check fails, the gateway returns a stale or conflict error, not a generic resolve failure.

## Transaction Design

All proposal creation and decision transitions should run inside `BEGIN IMMEDIATE` transactions.

### Human approve or reject

Transaction flow:

1. load proposal facts by ID
2. load current lineage head by `subject_type + subject_key`
3. load current proposal state
4. validate expected state, lineage, subject, and reviewed revision
5. insert terminal transition row
6. update lineage head projection if needed
7. emit websocket or webhook event after commit

### New proposal on same lineage

Transaction flow:

1. determine `subject_type`, `subject_key`, `lineage_id`, and `reviewed_revision`
2. load current lineage head
3. insert immutable proposal row
4. if current head is `pending_review`, insert `superseded` transition for old head
5. insert initial transition for new proposal:
   - `pending_review`
   - or terminal auto state
6. update lineage head projection
7. commit

This makes supersession canonical and race-safe.

## Timeout Design

Timeout is a lifecycle transition, not a field update.

The timeout worker should:

1. find proposals whose current state is `pending_review`
2. verify they are still lineage head or still unresolved
3. insert `timed_out` transition
4. update lineage projection if needed

Timeout must not overwrite a proposal that was already superseded or resolved by another transaction.

## Read Model

The gateway should expose proposal detail using derived current state.

### Read fields to add

- `lineage_id`
- `subject_type`
- `subject_key`
- `reviewed_revision`
- `validation_disposition`
- `current_state`
- `supersedes_proposal_id`
- `resolved_by_actor`
- `resolved_operation_id`
- `transition_history` on detail endpoint

### Compatibility mapping

For existing clients, `decision` can remain as a derived compatibility field:

- `approved` for `approved`
- `rejected` for `rejected`
- `HumanReviewRequired` for `pending_review`
- `Superseded` for `superseded`
- `TimedOut` for `timed_out`
- `AutoApproved` or `ApprovedWithFlags` for `auto_applied`
- `AutoRejected` for `auto_rejected`

## SDK And Dashboard Changes

### SDK

`GoalsAPI.approve()` and `GoalsAPI.reject()` should gain an optional request body:

```ts
await client.goals.approve(id, {
  expectedState: 'pending_review',
  expectedLineageId,
  expectedSubjectKey,
  expectedReviewedRevision,
});
```

This should compose with the operation envelope from the idempotency design.

### Dashboard

The dashboard must stop approving based on a list item ID alone.

Before a human decision is sent, the UI should read:

- `lineage_id`
- `subject_key`
- `reviewed_revision`
- `current_state`

and send those values back in the approval or rejection request.

## Migration Strategy

### Important limitation

Historical supersession lineage cannot be fully reconstructed from the current schema because it was never durably stored.

Do not fabricate certainty during backfill.

### Migration approach

1. add new tables alongside `goal_proposals`
2. backfill immutable proposal rows from existing records
3. map current `decision` values into:
   - `validation_disposition`
   - initial and terminal transition rows
4. derive provisional `subject_key` from legacy content only where necessary
5. mark legacy-derived rows as inferred in metadata
6. move new writes to v2 tables
7. keep old reads working through compatibility views during rollout

### Mapping from old data

- `HumanReviewRequired` -> initial transition `pending_review`
- `AutoApproved` -> terminal transition `auto_applied`
- `ApprovedWithFlags` -> terminal transition `auto_applied` plus flags
- `AutoRejected` -> terminal transition `auto_rejected`
- `approved` -> terminal transition `approved`
- `rejected` -> terminal transition `rejected`
- `TimedOut` -> terminal transition `timed_out`
- `Superseded` -> terminal transition `superseded`

Rows backfilled from legacy data without durable lineage proof should not be used as strong evidence for stale-decision checks. Only new v2 rows get full guarantees.

## Rollout Plan

### Phase 0: Operation identity first

Implement the operation envelope design first.

Reason:

- the decision path should not be rebuilt without stable operation IDs and idempotency

### Phase 1: New tables and compatibility reads

- add `goal_proposals_v2`
- add `goal_proposal_transitions`
- add `goal_lineage_heads`
- expose derived read model while continuing to support old approval routes

### Phase 2: New write path

- write new proposals into v2 tables
- write approval and rejection through transition engine
- persist supersession transactionally

### Phase 3: Require decision preconditions

- require expected lineage, subject, and reviewed revision on human decisions
- reject approvals that arrive without enough context

### Phase 4: Remove mutable decision writes

- stop updating legacy `goal_proposals.decision`
- remove the unresolved-row update exception after migration is complete

## Test Plan

### Concurrency tests

- approve and reject race on the same pending proposal
- approve races with insertion of a superseding proposal
- timeout races with approve
- two superseding proposals arrive for the same lineage at once

### Replay tests

- retry approve after commit with same operation identity returns replayed success
- retry approve with same proposal ID but stale expected revision fails
- replay old decision after supersession fails

### Restart tests

- persist a pending proposal, restart, then submit a superseding proposal
- persist a superseded proposal, restart, then attempt stale approval

### Migration tests

- backfill old proposals with every legacy decision variant
- ensure inferred legacy subject keys are marked as such
- verify new reads stay compatible with old UI expectations

## Open Questions

### Where should `reviewed_revision` come from?

Best answer:

- from the canonical goal subject revision

Fallback answer:

- from the currently applied proposal lineage head until a richer goal entity exists

### Should the route remain `/approve` and `/reject`?

Either is acceptable.

My preference:

- keep `/approve` and `/reject` externally for ergonomics
- route both into one internal transition engine

### Should transitions be event-sourced for all mutable domains?

Eventually, maybe.

For now this should be limited to the proposal path, where stale human decisions create the clearest integrity risk.

## Bottom Line

The current repo has proposal validation logic, review UI, and approval endpoints, but it does not yet have a canonical decision state machine.

The missing design move is this:

- immutable proposal facts
- append-only lifecycle transitions
- stable subject lineage
- human decisions bound to reviewed revision and expected state

That is the design that turns proposal approval from "best effort conflict avoidance" into a defensible integrity boundary.
