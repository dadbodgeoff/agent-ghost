# ADE Channels Architecture And Contract

## Architectural Summary

The current system mixes three truths:

- boot config in `ghost.yml`
- durable rows in SQLite
- transient bindings in the in-memory agent registry

That is the core failure. The corrected design is:

- SQLite channel records are the only durable source of truth
- boot config is imported into that store during reconciliation
- a gateway-owned `ChannelManager` projects durable records into live runtime adapters and registry bindings
- ADE, SDK, CLI, and runtime all read the same channel model

## Core Domain Model

Every channel record must have:

- `id`: durable channel identity
- `channel_type`: one of the supported builtin adapter types
- `agent_id`: durable agent identity
- `routing_key`: explicit inbound routing identity
- `status`: runtime-derived lifecycle state
- `status_message`: latest operator-facing detail
- `config`: validated channel-specific configuration blob
- `source`: `imported_config` or `operator_created`
- `last_message_at`
- `message_count`
- `created_at`
- `updated_at`

### Supported Channel Types

This milestone supports exactly:

- `cli`
- `websocket`
- `telegram`
- `discord`
- `slack`
- `whatsapp`

The gateway, SDK, and dashboard must share one generated or centrally declared type definition for this list.

## Identity And Routing Rules

### Rule 1: `channel_id` is durable instance identity.

Operator actions such as reconnect and delete are addressed by `channel_id`.

### Rule 2: `channel_type` is classification, not routing identity.

It describes what adapter implementation to use. It must not be the only lookup key for inbound message routing.

### Rule 3: `routing_key` is the runtime lookup key.

Examples:

- `cli:default`
- `websocket:127.0.0.1:18791`
- `slack:T12345`
- `telegram:bot:<token-fingerprint>`
- `discord:guild:123456789`
- `whatsapp:bridge:default`

The exact scheme may vary by adapter, but it must be explicit, durable enough for reconciliation, and unique among active channel records.

### Rule 4: uniqueness is enforced.

If two channel records would resolve to the same `routing_key`, the second one must be rejected.

## ChannelManager

`ChannelManager` is the new authority layer between API/bootstrap and runtime.

Responsibilities:

- load and reconcile durable channel records
- import channels from `ghost.yml`
- validate channel type and config shape
- compute and enforce `routing_key`
- start, stop, reconnect, and inspect live adapters
- update the agent registry routing map
- persist runtime-visible status changes
- write audit entries for operator mutations
- emit WebSocket channel lifecycle events

Forbidden behavior:

- API handlers mutating channel rows directly with ad hoc SQL for create/delete/reconnect
- bootstrap mutating `RegisteredAgent.channel_bindings` directly
- dashboard deriving channels from the agent list

## Registry Ownership Model

The agent registry should not own ad hoc channel mutation.

Required changes:

- add registry methods such as `bind_channel(routing_key, agent_id)` and `unbind_channel(routing_key)`
- stop exposing raw mutation patterns that let callers push to `channel_bindings` without updating reverse indexes
- keep agent-facing `channel_bindings` as a derived reflection of the same routing truth used for lookup

## REST Contract

The channel API should be shaped around the authoritative model.

### GET `/api/channels`

Returns:

- channel identity
- type
- agent identity and agent name
- routing key
- status
- status message
- config summary suitable for operators
- timestamps
- counts
- source

### POST `/api/channels`

Behavior:

- validates `channel_type`
- validates `agent_id`
- validates adapter-specific config
- computes `routing_key`
- rejects duplicate routing key
- persists the row
- creates live runtime binding through `ChannelManager`
- emits audit + WebSocket events

### POST `/api/channels/{id}/reconnect`

Behavior:

- asks `ChannelManager` to tear down and re-establish the live adapter
- updates status from real runtime outcome
- does not merely rewrite `status = connected`

### DELETE `/api/channels/{id}`

Behavior:

- asks `ChannelManager` to stop live runtime ownership first
- removes registry binding
- deletes or tombstones the durable record
- emits audit + WebSocket events

### POST `/api/channels/{id}/inject`

Preferred behavior for this milestone:

- injection resolves by `channel_id`, not by `channel_type`
- optional legacy `type` injection path may remain temporarily but must resolve through authoritative routing keys and be explicitly marked operator-debug only

## WebSocket Contract

Add explicit events:

- `ChannelCreated`
- `ChannelUpdated`
- `ChannelDeleted`
- `ChannelStatusChanged`

Each must include:

- `channel_id`
- `channel_type`
- `agent_id`
- `routing_key`
- `status`
- `status_message`
- `updated_at`

The dashboard channels page must subscribe to channel events directly rather than infer state from `AgentStateChange`.

## SDK Contract

The SDK must:

- expose shared `ChannelType` and `ChannelStatus` types
- expose the full `ChannelInfo` shape returned by the API
- expose channel lifecycle methods that match real runtime behavior
- expose channel WebSocket event types in the typed union

No hand-written SDK type should quietly fork from generated types without an explicit exception record.

## Dashboard Contract

### Canonical surface

`/channels` is the single management surface.

### Settings surface

`/settings/channels` must redirect to `/channels` or render the same page shell with no independent data model.

### Command palette

Command-palette navigation must land on `/channels`.

### Wizard

The agent wizard must:

- show only supported channel types
- provision channel bindings after successful agent creation
- fail the overall workflow if channel provisioning fails, or surface explicit partial-success recovery UI

## Runtime Flows

### Boot Flow

1. Run migrations.
2. Reconcile/import `ghost.yml` channels into durable records.
3. Load authoritative channel rows.
4. For each active record, validate, compute routing key, and start runtime ownership.
5. Bind registry routing from authoritative records.
6. Emit health state for any channel that cannot start.

### Create Flow

1. Validate request.
2. Begin transaction.
3. Insert durable row.
4. Commit durable mutation.
5. Ask `ChannelManager` to activate runtime ownership.
6. Persist resulting status.
7. Emit audit + WebSocket event.

### Delete Flow

1. Resolve channel by id.
2. Ask `ChannelManager` to stop runtime ownership.
3. Remove registry binding.
4. Delete or tombstone durable row.
5. Emit audit + WebSocket event.

### Reconnect Flow

1. Resolve durable row.
2. Stop runtime ownership if active.
3. Recreate adapter from durable config.
4. Rebind routing.
5. Persist real outcome.
6. Emit audit + WebSocket event.

## File Targets

Likely implementation files:

- `crates/ghost-gateway/src/api/channels.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/agents/registry.rs`
- `crates/ghost-gateway/src/config.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/cortex/cortex-storage/src/migrations/*`
- `packages/sdk/src/channels.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`
- `dashboard/src/routes/channels/+page.svelte`
- `dashboard/src/routes/settings/channels/+page.svelte`
- `dashboard/src/routes/agents/new/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`
