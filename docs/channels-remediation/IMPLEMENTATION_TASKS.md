# ADE Channels Implementation Tasks

## Objective

Execute the channels remediation defined in `MASTER_SPEC.md` without partial wiring, placeholder behavior, or split ownership.

## Non-Negotiable Rules

- do not keep dual authority between `ghost.yml`, SQLite, and registry state
- do not mutate channel state directly in API handlers once `ChannelManager` exists
- do not preserve `channel_type` as the sole routing key
- do not ship a fake `/settings/channels` view
- do not expose unsupported channel types in UI or API
- do not mark reconnect successful without runtime proof
- keep the tree releasable after every merged phase

## Critical Order

1. define the authoritative model
2. add storage support
3. implement `ChannelManager`
4. fix registry binding semantics
5. reconcile boot config into durable state
6. move API mutations onto `ChannelManager`
7. update SDK and WebSocket contracts
8. consolidate dashboard surfaces
9. wire the wizard
10. harden tests and rollout gates

## Task List

## T0. Freeze the broken seams

Purpose:

- stop new channel mutations from increasing drift during the remediation

Actions:

- document that channel runtime behavior is under active remediation
- reject new direct registry mutation patterns for channel bindings
- reject new dashboard channel surfaces

Done when:

- reviewers have one stated model to defend during the remediation

## T1. Define the authoritative channel model

Actions:

- add shared `ChannelType` and `ChannelStatus` representations
- add `routing_key` and `source` to the durable model
- define adapter-specific routing-key derivation rules

Done when:

- there is one durable record model used by gateway, SDK, and dashboard

Required checks:

- code search finds no independent channel-type lists except explicit adapter registry declarations

## T2. Add schema support and reconciliation safety

Actions:

- add storage migration for `routing_key` and `source`
- add uniqueness enforcement or equivalent application-layer guard
- add backfill logic for existing rows

Done when:

- existing databases upgrade safely
- ambiguous legacy state is surfaced, not silently normalized

Required tests:

- storage and migration tests from `TEST_DOCTRINE.md`

## T3. Implement `ChannelManager`

Actions:

- add gateway-owned channel lifecycle service
- move live start/stop/reconnect logic behind it
- centralize status persistence and runtime error mapping

Done when:

- API and bootstrap can no longer mutate runtime channel ownership without `ChannelManager`

Required tests:

- runtime and integration tests for create/delete/reconnect

## T4. Fix registry semantics

Actions:

- add explicit bind/unbind methods
- remove raw mutation of `channel_bindings` on registered agents where practical
- ensure reverse lookup updates atomically with forward state

Done when:

- no boot or API path relies on `channel_bindings.push(...)`

Required tests:

- reverse-index invariant tests

## T5. Reconcile boot config

Actions:

- import `ghost.yml` channels into durable state on boot
- mark drift explicitly
- activate runtime from durable rows after reconciliation

Done when:

- config-declared channels appear in ADE and route correctly after boot

Required tests:

- boot import and restart preservation tests

## T6. Refactor channel API

Actions:

- route create, reconnect, delete, and inject through `ChannelManager`
- enrich `GET /api/channels` payload with agent display data and routing metadata
- validate supported channel types and configs

Done when:

- channel API reflects real runtime effects

Required tests:

- gateway integration tests
- OpenAPI/SDK parity checks

## T7. Add channel WebSocket events and SDK support

Actions:

- extend gateway WebSocket events
- extend SDK unions and wrappers
- refresh generated types if needed

Done when:

- dashboard can refresh channels on channel events rather than generic agent events

Required tests:

- WebSocket payload tests
- SDK contract tests

## T8. Consolidate dashboard surfaces

Actions:

- make `/channels` canonical
- redirect or wrap `/settings/channels`
- update command palette
- remove contradictory copy

Done when:

- there is one visible channel truth in ADE

Required tests:

- dashboard route and command-palette tests

## T9. Wire agent creation flow

Actions:

- align wizard channel list with supported adapters
- provision channels after agent creation
- handle partial failure explicitly

Done when:

- a selected channel in the wizard results in a real authoritative channel binding

Required tests:

- dashboard workflow test
- integration test if backend composite mutation is added

## T10. Final hardening and release gates

Actions:

- remove obsolete assumptions and comments
- validate restart behavior
- validate drift handling
- update operator documentation if necessary

Done when:

- all acceptance criteria in `MASTER_SPEC.md` are met

## Merge Gates

Every PR in this program must satisfy:

- scoped tests for the touched layer
- no new direct mutation of channel rows or registry bindings outside the accepted ownership path
- no new unsupported type exposure
- no fake dashboard data model
