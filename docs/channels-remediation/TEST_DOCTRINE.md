# ADE Channels Test Doctrine

## Objective

Build a test suite that proves channel correctness across persistence, runtime routing, SDK contract, and dashboard behavior.

The target is not broad coverage. The target is regression resistance against the exact classes of failure already present.

## Required Invariants

The suite must prove:

- ADE-visible channels equal runtime-authoritative channels after boot
- no channel binding exists in an agent without corresponding reverse lookup
- every active routing key resolves to exactly one agent
- unsupported channel types are rejected or surfaced as explicit errors
- create, reconnect, and delete affect both durable state and live runtime state
- restart preserves the same effective binding set
- the wizard channel step creates real bindings
- WebSocket channel events keep dashboards in sync

## Test Layers

### Storage and migration tests

Prove:

- schema migration adds required columns and constraints
- duplicate routing keys are detected
- boot import is idempotent
- unsupported legacy rows are surfaced correctly

Suggested tests:

- `channels_schema_adds_routing_key_and_source`
- `boot_import_of_config_channels_is_idempotent`
- `duplicate_routing_keys_fail_reconciliation`
- `unsupported_channel_type_is_marked_error`

### Registry and runtime tests

Prove:

- binding and unbinding update both forward and reverse views
- reconnect tears down and re-establishes runtime ownership
- channel lookup never falls back to the wrong agent when an explicit route exists

Suggested tests:

- `bind_channel_updates_reverse_index`
- `unbind_channel_removes_reverse_index`
- `reconnect_channel_rebinds_runtime_route`
- `inject_by_channel_id_routes_to_expected_agent`

### Gateway integration tests

Prove:

- `POST /api/channels` creates durable and live state
- `DELETE /api/channels/{id}` removes durable and live state
- `POST /api/channels/{id}/reconnect` uses runtime semantics
- restart reproduces the same channel set

Suggested tests:

- `create_channel_activates_runtime_binding`
- `delete_channel_deactivates_runtime_binding`
- `reconnect_channel_updates_status_from_runtime`
- `boot_reconciliation_imports_config_channels_into_list_api`
- `restart_preserves_effective_channel_set`

### WebSocket contract tests

Prove:

- channel lifecycle events exist
- payloads match generated types and dashboard expectations

Suggested tests:

- `channel_created_event_emitted`
- `channel_status_changed_event_emitted`
- `channel_deleted_event_emitted`
- `channel_ws_payload_matches_sdk_union`

### SDK tests

Prove:

- generated types and wrappers agree
- channel payload includes routing and agent display data

Suggested tests:

- `sdk_channel_types_match_openapi`
- `sdk_channel_events_match_ws_contract`

### Dashboard tests

Prove:

- `/channels` shows imported and created channels correctly
- `/settings/channels` redirects or reuses the same surface
- command palette lands on canonical channels page
- wizard-created agent provisions selected channels

Suggested tests:

- `channels_page_renders_authoritative_rows`
- `settings_channels_redirects_to_canonical_page`
- `command_palette_go_to_channels_uses_canonical_route`
- `agent_wizard_channel_selection_creates_bindings`

## Commands

The implementation is not done until the relevant suites pass. At minimum:

- `cargo test --workspace`
- `pnpm --filter @ghost/sdk test`
- `pnpm --filter dashboard test`

If the dashboard suite is too broad for iteration, the PR must still include focused coverage for the channel path and pass it in CI.

## Anti-Patterns

Do not accept:

- tests that assert only status codes while ignoring runtime effect
- tests that validate SQL row creation but not routing behavior
- dashboard tests that stub channel data in ways that bypass real contracts
- WebSocket tests that check only event names and not payload shape
- happy-path-only reconnect tests
