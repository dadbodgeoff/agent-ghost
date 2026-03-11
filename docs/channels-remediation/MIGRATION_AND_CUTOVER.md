# ADE Channels Migration And Cutover

## Objective

Move from the current split model to a database-authoritative channel system without leaving hidden drift between runtime, ADE, and boot config.

## Chosen Migration Model

This milestone uses:

- database-authoritative channel records
- boot-time import and reconciliation from `ghost.yml`
- no permanent dual-write between `ghost.yml` and SQLite at runtime

Rationale:

- ADE needs runtime-visible mutable state
- the current API already writes SQLite
- the current runtime already reads config
- the only coherent direction is to make config an input into the durable store, not a competing source of truth

## Schema Changes

Add or evolve the `channels` table to support:

- `routing_key TEXT NOT NULL`
- `source TEXT NOT NULL DEFAULT 'operator_created'`
- optional uniqueness protection on `routing_key`
- any adapter health metadata needed for operator visibility

Guardrails:

- backfill existing rows deterministically
- refuse migration if irreconcilable duplicate routing keys are detected

## Reconciliation Algorithm

On boot:

1. Read `ghost.yml` channel entries.
2. Normalize each entry into the authoritative record shape.
3. Compute its `routing_key`.
4. For each normalized config channel:
   - if a durable record with the same `routing_key` exists, reconcile mutable fields only where policy allows
   - if none exists, insert a new record with `source = imported_config`
5. Mark any durable imported-config rows no longer present in config as drifted, not silently deleted
6. Load runtime ownership from the reconciled durable set

Required policy choice for this milestone:

- operator-created rows remain authoritative once created
- config-imported rows may update their config-derived fields on boot
- deleting a config row from `ghost.yml` does not hard-delete the durable record automatically; it becomes drifted and operator-visible

## Handling Existing Bad State

The migrator must detect and handle:

- duplicate rows that resolve to the same `routing_key`
- rows with unsupported `channel_type`
- rows bound to missing agents
- rows created by ADE that were never loaded into runtime

Disposition rules:

- unsupported type: mark `error` with explicit operator-visible message and exclude from runtime activation
- missing agent: mark `error` and exclude from runtime activation
- duplicate routing key: fail migration or quarantine all but one according to an explicit deterministic rule; do not silently pick a winner

## UI Cutover

During the same milestone:

- replace `/settings/channels` with a redirect or wrapper to `/channels`
- update command palette to use `/channels`
- remove any text claiming channels are created automatically with agents unless that becomes true via the new wizard wiring

## Wizard Cutover

The wizard cutover must be atomic from the user perspective:

- create agent
- create selected channels
- if channel creation fails, show explicit failure and recovery path

Allowed implementation for this milestone:

- sequential API calls from the dashboard with explicit rollback or recovery UX

Preferred implementation:

- a backend composite mutation that creates the agent and requested channels together

## Rollout Sequence

1. Land schema support and `ChannelManager`.
2. Land bootstrap reconciliation and registry fixes.
3. Land API refactor to use `ChannelManager`.
4. Land SDK and WebSocket contract changes.
5. Land dashboard consolidation and wizard wiring.
6. Enable redirect from `/settings/channels`.
7. Run restart and drift validation before release.

## Rollback

Rollback is allowed only if:

- migrations remain backward-compatible enough to keep channel rows readable
- the previous runtime can ignore added columns safely

If rollback would restore the old split-authority bug while leaving newly created rows in place, release must be blocked rather than rolled back casually.

## Release Checklist

- boot with `ghost.yml` channels imports correctly
- channels page shows imported rows
- restart preserves the same effective set
- command palette and settings route converge
- wizard provisioning is real
- duplicate routing keys are rejected
- operator audit log records create, reconnect, and delete
