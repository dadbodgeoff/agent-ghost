# ADE Security Remediation: Target Architecture

Status: draft for implementation handoff on March 11, 2026.

## Architecture Goal

The ADE Security surface must behave as one system, not a page with a few API
calls.

That means:

- one canonical safety-status contract
- one canonical audit filter model
- one permission model used by shell and page affordances
- one live-refresh model for security events

## End-State Components

### A. Auth-aware shell state

The shell must hold the current authenticated principal for UI gating.

Required properties:

- sourced from `GET /api/auth/session`
- cached centrally for page and shell use
- exposes base role and any relevant capabilities if available
- drives visibility and enabled state for privileged controls

Consumers:

- `/security`
- global keyboard shortcuts
- command palette actions

### B. Canonical security store or route-local controller

The Security surface must be backed by a single orchestration layer that owns:

- safety status
- sandbox review list
- audit query state
- active filters
- section-level load/error state
- websocket resync behavior

This may be a dedicated store or a disciplined route-local controller, but it
must prevent data refresh behavior from diverging across subsections.

### C. Canonical safety status contract

The frontend must not infer semantics from backend debug strings.

Target payload requirements:

- `platform_level_code: number`
- `platform_level_name: string`
- `platform_killed: boolean`
- `per_agent`
- `activated_at`
- `trigger`
- `convergence_protection`
- `distributed_kill`
- optional `available_actions`

If backward compatibility is needed, the old fields may temporarily coexist, but
all frontend rendering must move to the canonical fields.

### D. Canonical audit filter contract

Filter semantics must be explicit and shared.

Required properties:

- event types originate from backend-defined vocabulary
- severities originate from backend-defined vocabulary
- multi-select semantics are supported intentionally or removed
- export consumes the same filter state used by on-screen results

### E. Permission-aware action model

Every surfaced action must pass through the same policy rules.

Actions in scope:

- `kill-all`
- sandbox review approve
- sandbox review reject
- any future pause, quarantine, or resume controls if added to this surface

Required behavior:

- hidden or disabled before click if unauthorized
- clearly explained if disabled
- no shell shortcut registration for unauthorized principals

### F. Live evidence refresh model

Relevant websocket events must refresh all impacted security data.

Event handling rules:

- `KillSwitchActivation` refreshes safety status and audit evidence
- `InterventionChange` refreshes safety status and audit evidence
- `SandboxReviewRequested` refreshes reviews and audit evidence
- `SandboxReviewResolved` refreshes reviews and audit evidence
- `Resync` triggers a full security re-fetch

## Required Security Page Surface

The page should expose the following coherent areas.

### 1. Safety Overview

Shows:

- platform kill level
- platform killed boolean
- activation timestamp
- trigger
- distributed kill status
- convergence protection status

### 2. Per-Agent Interventions

Shows:

- paused agents
- quarantined agents
- trigger and activation time per agent

This is required because backend state already tracks it and the current page
does not surface it.

### 3. Sandbox Reviews

Shows:

- pending, approved, rejected, expired
- tool, agent, reason, mode, timestamps
- action controls only for authorized principals
- explicit error state for fetch failure

### 4. Audit Evidence

Shows:

- current filter state
- current result set
- canonical event types and severities
- export of the current filtered set
- accurate severity rendering

## System Data Flow

```text
Auth Session
  -> shell auth state
  -> role/capability gating for page actions, shortcuts, command palette

Gateway REST
  -> canonical safety status
  -> canonical audit query/export
  -> sandbox review list

WebSocket Events
  -> security orchestration layer
  -> targeted refresh or full resync
  -> page sections stay in sync

SDK Types
  -> generated/maintained from canonical contract
  -> dashboard consumes typed fields only
```

## Design Principles

- Truth before convenience: if a subsystem fails, show failure.
- Backend authority over vocabulary: event/severity labels come from the
  contract, not page-local guesses.
- Fail closed on privilege: missing permission data disables or hides.
- Cohesion over local patching: shell, shortcut, and page behavior must align.
- Observable degradation: partial subsystem failure is explicitly represented.
