# ADE Security Remediation: Execution Tracker

Status: initialized on March 11, 2026.

Use this file as the execution ledger. Update package status only after
verification, not after coding.

## Status Keys

- `Ready`
- `In Progress`
- `Code Complete`
- `Verified`
- `Accepted`
- `Blocked`

## Work Packages

| ID | Work package | Depends on | Primary files | Status | Evidence required |
| --- | --- | --- | --- | --- | --- |
| `WP-01` | Freeze canonical safety-status contract | none | `api/safety.rs`, `sdk/safety.ts`, generated types | `Ready` | contract diff + tests |
| `WP-02` | Freeze canonical audit filter semantics | none | `query_engine.rs`, `sdk/audit.ts` | `Ready` | query/export parity tests |
| `WP-03` | Freeze canonical security vocabularies | `WP-01`, `WP-02` | gateway + dashboard shared constants/docs | `Ready` | vocab reference + UI parity |
| `WP-04` | Build security route orchestration layer | `WP-01`, `WP-02` | `security/+page.svelte`, security stores | `Ready` | live-refresh verification |
| `WP-05` | Add section-level error and partial states | `WP-04` | `security/+page.svelte` | `Ready` | degraded-path UI tests |
| `WP-06` | Surface per-agent interventions | `WP-01`, `WP-04` | `security/+page.svelte` | `Ready` | rendered-state proof |
| `WP-07` | Centralize auth session state for shell gating | none | auth session store, `+layout.svelte` | `Ready` | auth-state tests |
| `WP-08` | Gate `kill-all` page action | `WP-07` | `security/+page.svelte` | `Ready` | role-based UI tests |
| `WP-09` | Gate `kill-all` shell shortcut | `WP-07` | `+layout.svelte` | `Ready` | shortcut registration tests |
| `WP-10` | Gate sandbox review decision actions | `WP-07` | `security/+page.svelte` | `Ready` | capability-based UI tests |
| `WP-11` | Align command-palette privileged actions | `WP-07` | `CommandPalette.svelte` | `Ready` | affordance parity proof |
| `WP-12` | Replace page-local filter vocabulary | `WP-02`, `WP-03` | `FilterBar.svelte`, `security/+page.svelte` | `Ready` | filter correctness tests |
| `WP-13` | Align timeline severity rendering | `WP-03` | `AuditTimeline.svelte` | `Ready` | severity rendering proof |
| `WP-14` | Make export use active filter state | `WP-02`, `WP-12` | `security/+page.svelte`, `sdk/audit.ts` | `Ready` | export parity tests |
| `WP-15` | Add gateway contract tests | `WP-01`, `WP-02` | `crates/ghost-gateway/tests/` | `Ready` | passing tests |
| `WP-16` | Add SDK parity tests | `WP-01`, `WP-02` | `packages/sdk/src/__tests__/` | `Ready` | passing tests |
| `WP-17` | Add dashboard end-to-end tests | `WP-04` through `WP-14` | `dashboard/tests/` | `Ready` | passing Playwright tests |
| `WP-18` | Build final signoff packet | `WP-15`, `WP-16`, `WP-17` | docs + closeout summary | `Ready` | completed packet |

## Acceptance Rule

A work package cannot be marked `Accepted` unless:

- the package-specific evidence exists
- downstream drift checks pass
- no open regression is introduced in a dependency already marked `Accepted`
