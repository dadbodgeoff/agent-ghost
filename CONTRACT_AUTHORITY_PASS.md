# Contract Authority Pass

Date: 2026-03-07

Purpose:
- decide the recommended source-of-truth model before remediation
- classify mounted route families and SDK exports by support level
- identify blockers to a one-shot parity fix

This is a recommendation pass, not a claim that product policy has already
been formally ratified.

## Recommended Authority Model

Use three classes only:

1. `Canonical Public Contract`
   - mounted route or protocol is intentionally public
   - semantics are server-owned
   - must have accurate docs
   - REST endpoints must be covered by OpenAPI unless explicitly exempted by protocol class
   - SDK root export may expose these directly

2. `Public Convenience Layer`
   - client-side wrapper or alias over canonical server APIs
   - not the authoritative semantic contract
   - must be labeled as compatibility/convenience
   - must not be confused with generated schema-backed contract

3. `Internal / Experimental / Transport`
   - not part of the app REST contract
   - may still be public in a protocol sense, but must live under separate documentation
   - should not silently appear as stable schema-backed app API

## Current Measured State

As of this pass:

- mounted routes discovered by parity check: `119`
- documented OpenAPI paths: `53`
- undocumented mounted routes after policy exclusions: `62`

Command:

```bash
python3 scripts/check_openapi_parity.py
```

## Recommended Route Family Classification

| Family | Representative routes | Dashboard / SDK use | Recommended class | Reason |
| --- | --- | --- | --- | --- |
| Health | `/api/health`, `/api/ready` | yes / yes | Canonical Public Contract | foundational availability contract |
| Auth & Session Identity | `/api/auth/login`, `/api/auth/refresh`, `/api/auth/logout`, `/api/auth/session` | yes / yes | Canonical Public Contract | required for every client |
| Agents | `/api/agents`, `/api/agents/{id}` | yes / yes | Canonical Public Contract | core product surface |
| Convergence | `/api/convergence/scores` | yes / yes | Canonical Public Contract | first-class product behavior |
| Goals / Proposals | `/api/goals`, `/api/goals/{id}`, approve/reject | yes / yes | Canonical Public Contract | server-owned semantics |
| Memory | `/api/memory`, `/api/memory/search`, archive/unarchive | yes / yes | Canonical Public Contract | core product surface |
| Runtime Sessions | `/api/sessions`, events, bookmarks, branch, heartbeat | yes / yes | Canonical Public Contract | dashboard depends on it |
| Studio Sessions & Messaging | `/api/studio/sessions`, `/api/studio/run`, message stream/recover | yes / yes | Canonical Public Contract | active interactive UX surface |
| Workflows | `/api/workflows`, execute, executions, resume | yes / yes | Canonical Public Contract | explicit feature area |
| Audit | `/api/audit`, export, aggregation | yes / yes | Canonical Public Contract | security and compliance surface |
| Safety | `/api/safety/status`, pause/resume/quarantine/kill-all | yes / yes | Canonical Public Contract | high-stakes admin surface |
| Costs | `/api/costs` | yes / yes | Canonical Public Contract | public operator-facing surface |
| Search | `/api/search` | yes / yes | Canonical Public Contract | dashboard depends on it |
| Profiles | `/api/profiles`, assign profile | yes / yes | Canonical Public Contract | visible operator workflow |
| Channels | `/api/channels`, reconnect, inject | yes / yes | Canonical Public Contract | dashboard depends on it |
| Skills Management | `/api/skills`, install/uninstall | yes / yes | Canonical Public Contract | visible product feature |
| Skill Execution | `/api/skills/{name}/execute` | SDK route family present, no dashboard use observed | Canonical Public Contract if retained | server owns execution semantics; either document or remove |
| Traces | `/api/traces/{session_id}` | yes / yes | Canonical Public Contract | observability UI depends on it |
| OAuth | providers/connect/callback/connections/execute | yes / yes | Canonical Public Contract | dashboard settings depend on it |
| Push | `/api/push/vapid-key`, subscribe, unsubscribe | yes / yes | Canonical Public Contract | shipped dashboard behavior |
| Webhooks | `/api/webhooks` family | yes / yes | Canonical Public Contract | admin UX depends on it |
| Backups | `/api/admin/backups`, `/api/admin/backup`, restore/export | yes / yes | Canonical Public Contract | admin UX depends on it |
| Provider Keys | `/api/admin/provider-keys` family | yes / yes | Canonical Public Contract | dashboard settings depend on it |
| PC Control | status/actions/allowed-apps/blocked-hotkeys/safe-zones | yes / yes | Canonical Public Contract | shipped feature area |
| State / Integrity | `/api/state/crdt/{agent_id}`, `/api/integrity/chain/{agent_id}` | yes / yes | Canonical Public Contract | dashboard agent detail depends on it |
| A2A Task API | `/api/a2a/discover`, `/api/a2a/tasks` | yes / yes | Canonical Public Contract | explicit feature area with UI |
| Mesh Visualization | `/api/mesh/trust-graph`, consensus, delegations | yes / yes | Canonical Public Contract | orchestration UI depends on it |
| WebSocket app events | `/api/ws` | yes / yes | Internal / Experimental / Transport | public protocol, but not OpenAPI REST surface |
| Mesh transport ingress | `/.well-known/agent.json`, `/a2a` | no dashboard / no root SDK | Internal / Experimental / Transport | separate protocol surface, not app REST |
| Marketplace | `/api/marketplace/*` | no dashboard / no root SDK | Internal / Experimental / Transport | mounted but not yet established as public app contract |
| Agent chat direct API | `/api/agent/chat`, `/api/agent/chat/stream` | no dashboard / no root SDK | Internal / Experimental / Transport | runner-facing or future surface; not yet part of stable public contract |
| Safety checks registration | `/api/safety/checks` | no dashboard / no root SDK | Internal / Experimental / Transport | extension/admin plumbing, not established public contract |

## Recommended SDK Classification

| SDK surface | Recommended class | Notes |
| --- | --- | --- |
| `GhostClient` core namespaces backed directly by mounted routes | Canonical Public Contract | acceptable root export if backing route family is canonical and documented |
| `GhostWebSocket` | Internal / Experimental / Transport | public protocol client, but should be documented as websocket/protocol contract, not OpenAPI-derived REST surface |
| generated `paths/components/operations` from `generated-types.ts` | Canonical Public Contract | only if parity work closes drift and the project keeps OpenAPI canonical |
| `ApprovalsAPI` | Public Convenience Layer | semantic alias over goals/proposals; must be demoted or formalized |
| `SessionsAPI` | Canonical Public Contract | naming is generic but route backing is direct studio session contract |
| `ChatAPI` | Canonical Public Contract | direct wrapper over studio message and SSE endpoints |

## One Confirmed Broken Surface

### `MemoryAPI.graph()` is exported and used by the dashboard, but no gateway route is mounted

Evidence:
- SDK calls `GET /api/memory/graph`
- dashboard memory graph page calls `client.memory.graph()`
- no mounted route or handler path for `/api/memory/graph` was found in the gateway source

References:
- `packages/sdk/src/memory.ts`
- `dashboard/src/routes/memory/graph/+page.svelte`

Implication:
- this is not just documentation drift
- this is an unsupported exported client operation and likely a broken UI path

Required action:
- either implement `/api/memory/graph`
- or remove/demote `memory.graph()` and the dashboard page

## One-Shot Remediation Prerequisites

A true one-shot contract fix is only realistic if these decisions are accepted first:

1. OpenAPI remains the canonical REST contract.
2. WebSocket and mesh transport are documented as separate protocol contracts, not forced into OpenAPI parity.
3. Marketplace, direct agent chat, and safety-check registration remain non-canonical until intentionally promoted.
4. `ApprovalsAPI` is either:
   - formally server-owned and implemented as a real approval domain, or
   - demoted as a compatibility alias over goals
5. Broken exported surfaces like `memory.graph()` are resolved before parity is declared complete.

Without these decisions, parity work can still be mechanically correct while
remaining semantically dishonest.

## Recommended Remediation Order

### Phase 1: Formalize authority

- adopt this classification model or an adjusted version
- record the decision in repo docs
- mark REST vs protocol vs convenience explicitly

### Phase 2: Fix unsupported/broken surfaces

- resolve `memory.graph()`
- decide `ApprovalsAPI`

### Phase 3: Close REST parity for canonical families

First-wave canonical parity should cover:
- auth/session
- agents
- convergence
- goals
- memory
- runtime sessions
- studio
- workflows
- audit
- safety
- costs
- search
- profiles
- channels
- traces
- OAuth
- push
- webhooks
- provider keys
- backups
- PC control
- state/integrity
- A2A
- mesh visualization

### Phase 4: Separate protocol docs

- websocket contract doc
- mesh transport contract doc

### Phase 5: Demote or clearly label convenience surfaces

- `ApprovalsAPI`
- any future client-side semantic adapters

## Exit Criteria

This pass is complete when:

- every route family has an explicit authority class
- every SDK export family has an explicit authority class
- broken exported surfaces are identified
- parity work has a bounded target instead of an implied “document everything mounted”
