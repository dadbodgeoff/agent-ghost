# ADE Security Remediation: Contract Matrix

Status: draft for implementation handoff on March 11, 2026.

This document freezes the contracts and vocabularies that the remediation must
implement.

## 1. REST Contracts

| Surface | Current issue | Target contract | Owner |
| --- | --- | --- | --- |
| `GET /api/auth/session` | shell reads session but does not use it for security action gating | provide principal state to shell and security UI; minimum required field is `role`; capability field may be added if needed | gateway + dashboard |
| `GET /api/safety/status` | UI expects fields that do not align with current gateway payload and parses kill level unsafely | canonical fields: `platform_level_code`, `platform_level_name`, `platform_killed`, `per_agent`, `activated_at`, `trigger`, `convergence_protection`, `distributed_kill` | gateway + SDK + dashboard |
| `GET /api/safety/sandbox-reviews` | page masks failure as empty reviews | preserve payload shape, but frontend must represent fetch failure separately from empty result | dashboard |
| `POST /api/safety/sandbox-reviews/:id/approve` | UI exposes action to unauthorized users | action remains backend-enforced; frontend must gate affordance | dashboard |
| `POST /api/safety/sandbox-reviews/:id/reject` | UI exposes action to unauthorized users | action remains backend-enforced; frontend must gate affordance | dashboard |
| `POST /api/safety/kill-all` | surfaced too broadly in shell/UI | route stays superadmin-only; shell and UI must respect that | dashboard |
| `GET /api/audit` | filter semantics are exact-match but UI behaves like set filter | either change API to accept repeated values or reduce UI to exact-match semantics; chosen behavior must be explicit and shared | gateway + SDK + dashboard |
| `GET /api/audit/export` | export ignores on-screen filter state | export inputs must be the same filter model used by query | SDK + dashboard |

## 2. WebSocket Refresh Rules

| Event | Target refresh behavior |
| --- | --- |
| `KillSwitchActivation` | refresh safety overview, per-agent interventions, and audit evidence |
| `InterventionChange` | refresh safety overview, per-agent interventions, and audit evidence |
| `SandboxReviewRequested` | refresh sandbox reviews and audit evidence |
| `SandboxReviewResolved` | refresh sandbox reviews and audit evidence |
| `Resync` | full re-fetch of all Security route sections |

## 3. Authorization Matrix For Surfaced Actions

| UI action | Required backend auth | Frontend behavior |
| --- | --- | --- |
| `KILL ALL` button | `superadmin` | hidden or disabled for non-superadmins; no optimistic invocation |
| `killSwitch.activateAll` shortcut | `superadmin` | do not register when unauthorized |
| sandbox review approve | `admin+` or `operator + safety_review` | hidden or disabled when unauthorized |
| sandbox review reject | `admin+` or `operator + safety_review` | hidden or disabled when unauthorized |

## 4. Canonical Security Event Vocabulary

The Security page must not hard-code a narrow generic event list that omits live
security events.

Minimum audited security event types already present in the gateway include:

- `kill_all`
- `pause_agent`
- `resume_agent`
- `quarantine_agent`
- `forensic_review`
- `sandbox_review_requested`
- `sandbox_review_approved`
- `sandbox_review_rejected`
- `sandbox_review_expired`

Implementation rule:

- if the UI offers an event-type picker, the values must be sourced from a
  backend-owned list, backend aggregation, or a shared typed constant with a
  single owner

## 5. Canonical Severity Vocabulary

Current evidence already shows multiple live severities:

- `info`
- `warn`
- `high`
- `critical`
- `medium`
- `low`

The remediation must choose one of two paths.

### Path A: Preserve current backend vocabulary

- UI uses the exact gateway values
- `warn` stays `warn`
- filter controls reflect exact values

### Path B: Normalize vocabulary end-to-end

- gateway writes a canonical set
- SDK and dashboard adopt the same set
- migrations or compatibility logic handle old values

Mandatory rule:

- no page-local synonyms such as `warning` unless the backend also supports them

## 6. Section State Model

Each section of the Security page must represent:

- `loading`
- `loaded`
- `empty`
- `error`
- optional `partial`

This state must be independent per section:

- Safety overview
- Sandbox reviews
- Audit evidence

The route must not collapse one section’s failure into a global empty state for
the whole page.
