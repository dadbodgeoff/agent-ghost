# ADE Channels Master Spec

Status: March 11, 2026

Purpose: define the authoritative remediation plan for the ADE channels feature so that runtime wiring, persistence, contracts, SDKs, and dashboard surfaces all describe and operate the same system.

This spec is based on the live code, not on older design claims. If the code and earlier docs disagree, the code-audit findings in this spec take precedence.

## Standard

This work is held to the following bar:

- one source of truth for channel existence, binding, and runtime status
- no dashboard surface may fabricate channel state from unrelated data
- no runtime routing decision may depend on a lossy key such as `channel_type` alone
- no API mutation may update persistence without updating live runtime ownership
- no live runtime mutation may bypass audit and operator visibility
- no supported channel type may exist in one layer and not the others
- no wizard or setup surface may claim to wire channels if it does not
- no cutover may leave boot configuration and database state in silent disagreement

## Scope

This remediation covers:

- gateway channel bootstrap and runtime ownership
- channel persistence and reconciliation with `ghost.yml`
- channel REST contract
- channel WebSocket event contract
- SDK channel wrappers and types
- ADE `/channels` experience
- Settings and command-palette channel entry points
- agent creation channel binding flow
- test and rollout gates

This remediation does not cover:

- building new channel adapters beyond the currently implemented set
- redesigning unrelated agent lifecycle APIs
- channel-provider secret management beyond what is required for config validation and runtime status

## Confirmed Current Failures

### F1. Channel state has split authority.

The live runtime binds channels from `ghost.yml` at bootstrap. The ADE and channel APIs read and mutate SQLite channel rows. These are not reconciled into one authoritative model.

Effects:

- runtime channels can exist and not appear in ADE
- ADE-created channels can exist in the database and never be loaded into runtime
- restart can preserve misleading UI state while runtime forgets the binding

### F2. Bootstrap channel reverse lookup is broken.

Agents are registered with empty `channel_bindings`. Bindings are pushed afterward, but the reverse `channel_to_id` index is only populated during registration.

Effects:

- `lookup_by_channel()` misses boot-configured channels
- injection and any channel-based lookup can silently fall back to the wrong agent

### F3. The identity model is invalid for a multi-channel ADE.

Runtime reverse lookup is keyed by a single string map entry. Storage and APIs allow unlimited channel rows with the same `channel_type`. Supported types are not validated by the API.

Effects:

- duplicate type bindings are ambiguous
- invalid types can be stored as if they were real
- the system cannot support more than one effective binding per routing key without explicit redesign

### F4. The ADE has contradictory channel surfaces.

`/channels` is the real management page. `/settings/channels` invents channel rows from the agent list. Command palette navigation sends users to the fake surface.

Effects:

- operator trust is damaged
- the product presents mutually inconsistent truths

### F5. Agent setup lies about channel wiring.

The wizard collects channel choices but does not create channel bindings. Its type list is also out of sync with implemented adapters.

Effects:

- agent onboarding creates false operator expectations
- initial system state is not what the UI promised

### F6. Channel state is not part of the live event model.

The dashboard refreshes channels on generic agent events. There are no first-class channel lifecycle events for other clients or tabs to consume.

Effects:

- stale ADE state
- avoidable polling/resync behavior

## Target End State

At completion, the system must satisfy all of:

- the database is the authoritative durable source of truth for channel records
- boot-time `ghost.yml` channels are imported and reconciled into the authoritative store
- one `ChannelManager` owns channel lifecycle, runtime adapters, agent binding updates, audit, and WebSocket emission
- every channel has a durable `channel_id`
- every inbound route resolves through an explicit routing key or channel identity, not `channel_type` alone
- supported channel types are represented by one shared enum across gateway, SDK, and dashboard
- `/settings/channels` no longer presents a separate truth
- the agent wizard either provisions channels transactionally or does not offer the step
- channel mutations are visible in real time through first-class events
- restart preserves the same effective channel set that ADE shows

## Architectural Decisions

These decisions are mandatory for this milestone:

- durable authority: SQLite `channels` table is canonical
- config reconciliation model: `ghost.yml` is an import/bootstrap input, not the runtime source of truth after reconciliation
- supported builtin channel types: `cli`, `websocket`, `telegram`, `discord`, `slack`, `whatsapp`
- unsupported types such as `webhook` must not be shown or accepted unless a real adapter exists
- live runtime ownership: channel lifecycle must move behind a gateway-owned `ChannelManager`
- UI consolidation: `/channels` is the canonical dashboard surface; `/settings/channels` must redirect or wrap it, not reimplement it

## Acceptance Criteria

This remediation is done only when all of the following are true:

- creating a channel through ADE causes a durable row, a live runtime binding, an audit entry, and a WebSocket event
- deleting a channel through ADE removes the durable row, tears down the live runtime binding, updates registry routing, and emits a WebSocket event
- reconnecting a channel uses runtime behavior, not just a status-field update
- a channel imported from `ghost.yml` appears in ADE without any manual backfill
- restarting the gateway yields the same effective channel set shown in ADE
- channel injection and routing select the correct agent deterministically
- duplicate routing keys are rejected or resolved by an explicit, documented model
- the command palette and settings entry point land on the same canonical channels experience
- the wizard channel step results in actual channel bindings
- required tests and verification commands in this package pass

## Primary Sources

- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/api/channels.rs`
- `crates/ghost-gateway/src/agents/registry.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/config.rs`
- `crates/cortex/cortex-storage/src/migrations/v040_phase3_tables.rs`
- `crates/ghost-channels/src/lib.rs`
- `crates/ghost-channels/src/adapters/mod.rs`
- `dashboard/src/routes/channels/+page.svelte`
- `dashboard/src/routes/settings/channels/+page.svelte`
- `dashboard/src/routes/agents/new/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`
- `packages/sdk/src/channels.ts`
- `packages/sdk/src/websocket.ts`
