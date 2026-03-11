# ADE Security Remediation: Traceability Matrix

Status: draft for implementation orchestration on March 11, 2026.

This document maps findings to requirements, code surfaces, and verification.

| Finding / risk | Requirement | Code surfaces | Verification |
| --- | --- | --- | --- |
| kill state rendered as normal despite real non-normal backend state | UI must render canonical kill state without string-to-number inference | `api/safety.rs`, `sdk/safety.ts`, `security/+page.svelte` | gateway contract tests, SDK tests, dashboard render tests |
| filter UI returns false empty states | query and export must share canonical filter semantics; filter options must be backend-valid | `query_engine.rs`, `sdk/audit.ts`, `FilterBar.svelte`, `security/+page.svelte` | query tests, export tests, Playwright filter tests |
| sandbox review fetch failure shown as empty state | each section must distinguish error from empty | `security/+page.svelte`, orchestration store | degraded-path Playwright tests |
| audit pane goes stale after security events | security websocket events must refresh all impacted evidence sections | `security/+page.svelte`, security stores, websocket store integration | websocket refresh tests |
| privileged controls shown to unauthorized principals | no security action affordance without role/capability gating | `+layout.svelte`, `security/+page.svelte`, `CommandPalette.svelte` | role/capability UI tests |
| SDK/gateway contract drift | SDK and gateway fields must match canonical contract | `api/safety.rs`, `sdk/safety.ts`, generated types | contract parity tests |
| shell/page authorization drift | shell shortcuts and page buttons must expose the same privilege envelope | `+layout.svelte`, `security/+page.svelte`, auth state store | shell parity tests + Playwright tests |
| query/export evidence mismatch | exported evidence must reflect active filter state | `sdk/audit.ts`, `security/+page.svelte` | export parity tests |

## Traceability Rule

No finding is considered closed until:

- the requirement has been implemented
- the mapped verification has passed
- the corresponding evidence is recorded in the signoff packet
