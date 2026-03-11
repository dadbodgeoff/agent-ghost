# ADE Channels Agent Handoff

Start here only after reading:

1. `MASTER_SPEC.md`
2. `ARCHITECTURE_AND_CONTRACT.md`
3. `MIGRATION_AND_CUTOVER.md`
4. `TEST_DOCTRINE.md`
5. `IMPLEMENTATION_TASKS.md`

## Mission

Rebuild the ADE channels system so that:

- the dashboard, SDK, gateway API, runtime registry, and boot path all operate one channel model
- channel mutations affect real runtime ownership, not just database rows
- restart preserves the same effective channel set shown in ADE
- agent creation can provision real channel bindings

## Decisions Already Made

Do not re-open these unless the code makes them impossible:

- SQLite is the authoritative durable source of truth for channels
- `ghost.yml` is a boot import/reconciliation source, not the post-boot runtime authority
- `/channels` is the canonical UI surface
- `/settings/channels` must redirect or reuse `/channels`
- supported builtin channel types for this milestone are `cli`, `websocket`, `telegram`, `discord`, `slack`, and `whatsapp`
- `webhook` is out of scope until a real adapter exists
- routing must not resolve by `channel_type` alone
- channel lifecycle must be owned by a gateway `ChannelManager`

## Deliverables

You must leave behind:

- migrated durable channel model with `routing_key` and `source`
- working `ChannelManager`
- corrected registry bind/unbind semantics
- boot reconciliation from `ghost.yml`
- API mutations routed through `ChannelManager`
- channel WebSocket lifecycle events
- SDK type and wrapper updates
- canonical dashboard channels experience
- wizard channel provisioning
- tests covering the invariant set in `TEST_DOCTRINE.md`

## Likely Files To Touch

- `crates/ghost-gateway/src/api/channels.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/agents/registry.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/config.rs`
- `crates/cortex/cortex-storage/src/migrations/*`
- `packages/sdk/src/channels.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`
- `dashboard/src/routes/channels/+page.svelte`
- `dashboard/src/routes/settings/channels/+page.svelte`
- `dashboard/src/routes/agents/new/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`

## Execution Order

1. add schema and shared channel model changes
2. implement `ChannelManager`
3. fix registry binding semantics
4. add boot reconciliation
5. move channel API onto `ChannelManager`
6. add WebSocket and SDK updates
7. consolidate dashboard surfaces
8. wire wizard provisioning
9. land and run the full test set

## Hard Requirements

- no raw `channel_bindings.push(...)` in live wiring paths
- no create/delete/reconnect route may directly mutate durable rows and stop there
- no unsupported channel type may pass validation
- no dashboard channel view may derive its rows from agents
- no route may report reconnect success without runtime proof
- no partial wizard success may be silently presented as complete success

## Acceptance Checklist

Before calling the work done, verify:

- boot-imported channels appear in `GET /api/channels`
- ADE `/channels` shows the same effective set as runtime after restart
- channel injection resolves deterministically to the expected agent
- deleting a channel removes runtime routing immediately
- reconnect actually re-establishes runtime ownership
- command palette lands on the canonical channels surface
- wizard-created channels are present and live after agent creation
- channel lifecycle WebSocket events reach the dashboard

## Minimum Verification Commands

- `cargo test --workspace`
- `pnpm --filter @ghost/sdk test`
- `pnpm --filter dashboard test`

If one of these is too broad during development, you may run narrower local subsets, but the full relevant suite must pass before the work is complete.

## Forbidden Shortcuts

- do not patch the UI only
- do not patch the DB only
- do not patch bootstrap only
- do not claim success because the list page renders rows
- do not leave old contradictory routes in place
- do not add TODO-backed adapter validation or placeholder runtime hooks

## Definition Of Done

The work is done only when the channels system behaves as one cohesive control plane across boot, runtime, persistence, API, SDK, and ADE surfaces, with automated tests proving the critical invariants.
