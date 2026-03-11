# ADE Security Remediation: Verification And Release Gates

Status: draft for implementation handoff on March 11, 2026.

This remediation is not complete when the UI looks better. It is complete only
when the following checks pass.

Final closeout evidence is recorded in `12_SIGNOFF_PACKET.md`.
Exact verification commands live in `13_VERIFICATION_COMMAND_RUNBOOK.md`.

## 1. Automated Test Requirements

### Gateway

Must cover:

- `GET /api/safety/status` canonical field presence and values
- audit query filter semantics
- audit export filter semantics
- authorization on:
  - `kill-all`
  - sandbox review approve
  - sandbox review reject

### SDK

Must cover:

- safety-status typing
- audit query parameter serialization
- audit export parameter serialization

### Dashboard

Must cover:

- Security page renders degraded state as degraded, not empty
- non-superadmin does not get active `kill-all` affordance
- non-reviewer does not get active sandbox review controls
- authorized reviewer does get sandbox review controls
- websocket-driven events refresh audit evidence
- export uses current filter state
- safety overview renders real kill states correctly

## 2. Manual Scenario Matrix

The implementer must run and record these scenarios.

1. Viewer session:
   `/security` should either be inaccessible or only show what policy allows.
2. Operator without `safety_review`:
   can view status and reviews but cannot approve/reject reviews or invoke
   `kill-all`.
3. Reviewer-capable operator:
   can approve/reject reviews but cannot invoke `kill-all`.
4. Superadmin:
   can access all surfaced actions.
5. Sandbox review endpoint failure:
   reviews section shows error state, not empty state.
6. Kill switch event:
   safety overview updates and audit evidence updates.
7. Resync event:
   all Security sections re-fetch correctly.
8. Filtered export:
   downloaded evidence matches the active filter set.

## 3. Release Gates

All gates are blocking.

- No known contract mismatch remains between gateway and SDK.
- No privileged shell action is exposed without auth gating.
- No Security subsection converts transport/auth failure into empty state.
- No canonical severity/event type is missing from UI rendering.
- No relevant websocket event leaves the audit pane stale.
- Playwright coverage exists for authorized, unauthorized, and degraded cases.

## 4. Evidence Required For Closeout

The implementing agent must leave behind:

- test output summary
- list of added/updated tests
- list of changed contracts
- note on any compatibility bridges introduced and whether they remain
- confirmation that shell, page, and export behavior are aligned

## 5. Failure Conditions

The remediation is not releasable if any of these remain true.

- `platform_level` still relies on string-to-number parsing in the UI
- filter UI offers values that the backend cannot match
- export ignores current filters
- unauthorized principals can still trigger privileged flows from shell or page
- Security page still shows “No sandbox reviews recorded” on fetch failure
