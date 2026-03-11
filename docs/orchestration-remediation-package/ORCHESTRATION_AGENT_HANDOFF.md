# Orchestration Agent Handoff

Status: March 11, 2026
Audience: implementation agent
Purpose: provide the final execution brief for rebuilding orchestration start to finish.

Read this document last. It is not standalone authority. You must also follow:

- `docs/orchestration-remediation-package/ORCHESTRATION_MASTER_REMEDIATION_SPEC.md`
- `docs/orchestration-remediation-package/ORCHESTRATION_ARCHITECTURE_AND_CONTRACTS.md`
- `docs/orchestration-remediation-package/ORCHESTRATION_EXECUTION_PLAN.md`
- `docs/orchestration-remediation-package/ORCHESTRATION_VERIFICATION_PLAN.md`

## Mission

Bring the ADE orchestration subsystem to production-grade correctness and cohesion across backend, SDK, dashboard, and tests.

The orchestration subsystem is currently partially wired and partially misleading. Your job is not to patch symptoms. Your job is to make the subsystem truthful, coherent, and test-gated.

## Deliverables

You must produce all of the following:

1. corrected gateway orchestration read models
2. corrected SDK orchestration contracts
3. one unified dashboard orchestration store
4. route/components migrated to that single source of truth
5. end-to-end orchestration tests across gateway, SDK, and dashboard

## Non-Negotiable Rules

- Do not keep fabricated metrics behind polished UI.
- Do not leave duplicate task state owners in route and child component.
- Do not suppress backend read-model/query failure.
- Do not present A2A dispatch acknowledgment as full task execution tracking unless task reconciliation is implemented.
- Do not ship orchestration changes without tests that prove coherence after realtime updates and resync.

## Required Execution Order

### Step 1. Repair backend truth

Implement Phase 1 from `ORCHESTRATION_EXECUTION_PLAN.md`.

You must fix:

- trust graph derivation
- consensus derivation
- sybil/delegation derivation
- A2A discovery/task lifecycle model

Do not start dashboard refactors until backend payloads are semantically correct.

### Step 2. Align SDK and contract surfaces

Implement Phase 2.

You must ensure:

- SDK types match backend truth
- websocket orchestration events are typed correctly
- orchestration routes are parity-gated

### Step 3. Build the orchestration store

Implement one dashboard store to own all orchestration state.

The store must:

- load all orchestration slices
- subscribe to relevant websocket events
- handle resync
- expose targeted actions for discovery and task dispatch

### Step 4. Migrate the route and components

The route becomes a thin composition layer.

Components must:

- receive data by props
- receive actions by callbacks
- stop performing hidden critical-state fetches

### Step 5. Add verification gates

Implement the verification plan completely.

Do not declare completion until:

- backend integration tests exist
- SDK tests exist or are updated
- dashboard orchestration tests exist
- reconnect/resync behavior is covered

## Required Files To Touch

At minimum, expect changes in:

- `crates/ghost-gateway/src/api/mesh_viz.rs`
- `crates/ghost-gateway/src/api/a2a.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/cortex/cortex-storage/src/queries/`
- `crates/cortex/cortex-storage/src/migrations/` if schema changes are needed
- `packages/sdk/src/mesh.ts`
- `packages/sdk/src/a2a.ts`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/routes/orchestration/+page.svelte`
- `dashboard/src/components/A2ATaskTracker.svelte`
- `dashboard/src/components/A2AAgentCard.svelte`
- `dashboard/src/lib/stores/` with a new orchestration store
- orchestration-specific tests across gateway and dashboard

## Output Standard

When you finish:

- summarize what changed by subsystem, not by file dump
- list any residual risks explicitly
- list exact tests run
- do not call the work complete if any major panel still depends on placeholders or stale one-shot fetches

## Stop Conditions

Stop and escalate if:

- current ADE semantics do not actually define a truthful notion of consensus counts
- remote A2A systems cannot provide any state after submission and product language must change
- required schema changes would break existing migration or compatibility guarantees in a non-trivial way

If you hit one of these, do not improvise a fake semantic layer. Record the decision and force explicit resolution.
