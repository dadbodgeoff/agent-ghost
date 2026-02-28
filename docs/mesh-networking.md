# Mesh Networking

GHOST agents can discover, delegate to, and collaborate with other agents via
the `ghost-mesh` crate. The protocol is compatible with Google's Agent-to-Agent
(A2A) specification while adding GHOST-specific safety extensions.

## Agent Discovery

Each agent publishes an `AgentCard` at `/.well-known/agent.json`:

```json
{
  "name": "code-reviewer",
  "description": "Reviews code for quality and security issues",
  "capabilities": ["code-review", "security-audit"],
  "endpoint_url": "http://localhost:8081",
  "public_key": "<base64 Ed25519 public key>",
  "trust_score": 0.75,
  "signature": "<Ed25519 signature over canonical bytes>"
}
```

Cards are signed with Ed25519 (via `ghost-signing`). Consumers verify the
signature before trusting any card data.

## Task Delegation

### Lifecycle

```
Submitted → Working → Completed
                   → Failed
                   → InputRequired → Working → ...
         → Canceled (from any non-terminal state)
```

### Delegation Depth

Each hop increments `delegation_depth`. The platform enforces a maximum depth
(default 3) to prevent unbounded delegation chains.

```
Agent A → Agent B → Agent C → Agent D  (depth 3, allowed)
Agent A → Agent B → Agent C → Agent D → Agent E  (depth 4, rejected)
```

## Trust Scoring (EigenTrust)

Trust is computed using the EigenTrust algorithm over interaction history:

| Outcome | Trust Delta |
|---------|------------|
| TaskCompleted | +0.10 |
| TaskFailed | -0.05 |
| PolicyViolation | -0.20 |
| SignatureFailure | -0.30 |
| Timeout | -0.02 |

New agents start with trust 0.0. Delegation requires minimum trust (configurable,
default 0.3). Trust is local per-agent — each agent maintains its own trust view.

## Safety

### Cascade Circuit Breakers

If a delegated agent's convergence score spikes, the delegating agent's cascade
breaker trips, preventing further delegation to that agent.

### Memory Poisoning Defense

Delegated tasks that produce suspicious memory writes (high self-reference density,
scope expansion beyond the task) are flagged and rejected before merge.

### Sybil Resistance

Inherited from `cortex-crdt`: max 3 child agents per parent per 24h, new agents
start at trust 0.3, trust capped at 0.6 for agents less than 7 days old.

## A2A Compatibility

GHOST agents serve standard A2A endpoints:

- `POST /` — JSON-RPC 2.0 (tasks/send, tasks/get, tasks/cancel)
- `GET /.well-known/agent.json` — Agent card

Non-GHOST A2A clients can interact with GHOST agents using the standard protocol.
GHOST-specific extensions (trust, convergence, signed cards) are optional.
