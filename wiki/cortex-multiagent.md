# cortex-multiagent

> N-of-M consensus shielding — no single agent can unilaterally change shared state.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-multiagent/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `serde`, `uuid` |
| Modules | `consensus` (single module) |
| Public API | `ConsensusShield`, `ConsensusConfig`, `ConsensusRound`, `ConsensusResult`, `Vote` |
| Test coverage | Unit tests (inline) |
| Downstream consumers | `ghost-mesh`, `ghost-agent-loop` |

---

## Why This Crate Exists

In a multi-agent GHOST deployment, agents can communicate and propose changes that affect other agents. Without consensus, a single compromised or converging agent could unilaterally modify shared state — changing goals, writing memories, or altering configurations that affect the entire agent network.

`cortex-multiagent` implements a consensus shield: a voting mechanism that requires N out of M agents to approve a cross-agent state change before it takes effect. This is the multi-agent equivalent of `cortex-validation`'s proposal validator — but instead of validating content, it validates agreement.

The default configuration requires 2-of-3 agreement with a 5-minute timeout.

---

## Module Breakdown

### `consensus.rs` — The Consensus Shield

#### Vote Types

```rust
pub enum Vote {
    Approve,
    Reject,
    Abstain,
}
```

Three-valued voting. `Abstain` is distinct from not voting — an abstaining agent has seen the proposal and chosen not to take a position. This matters for quorum calculation: an abstaining agent counts toward "votes received" but not toward approvals or rejections.

#### `ConsensusConfig`

```rust
pub struct ConsensusConfig {
    pub required_approvals: usize,  // N (default: 2)
    pub total_participants: usize,  // M (default: 3)
    pub timeout_seconds: u64,       // default: 300 (5 minutes)
}
```

**Key design decisions:**

1. **2-of-3 default.** The default requires 2 approvals from 3 participants. This is the simplest meaningful consensus — it tolerates 1 faulty/compromised agent while still requiring agreement. For larger deployments, N and M can be increased.

2. **Timeout as configuration, not enforcement.** The `timeout_seconds` field is stored but not enforced by the `ConsensusShield` itself. The caller (typically `ghost-mesh`) is responsible for checking timeouts and auto-rejecting stale rounds. This keeps the consensus logic pure (no clock dependency) and testable.

3. **`usize` for counts, not `u8`.** While GHOST deployments rarely exceed 10 agents, using `usize` avoids artificial limits and matches Rust's collection size conventions.

#### `ConsensusShield`

```rust
pub struct ConsensusShield {
    config: ConsensusConfig,
    rounds: BTreeMap<Uuid, ConsensusRound>,
}
```

**Lifecycle:**

1. **`start_round(proposal_id)`** — Creates a new `ConsensusRound` with empty votes
2. **`vote(proposal_id, agent_id, vote)`** — Records a vote and returns the current consensus state
3. **`evaluate(proposal_id)`** — Checks if consensus has been reached

**Consensus evaluation logic:**

```
if approvals >= required_approvals → Approved
if rejections > total_participants - required_approvals → Rejected
otherwise → Pending { approvals, rejections }
```

The rejection threshold is `total_participants - required_approvals`. In a 2-of-3 config, if 2 agents reject, it's impossible to reach 2 approvals (only 1 agent left), so the proposal is rejected immediately without waiting for the third vote.

**Key design decisions:**

1. **`BTreeMap` for rounds and votes.** Deterministic iteration order for debugging and audit trails. When inspecting consensus state, rounds and votes appear in a consistent order.

2. **Vote replacement.** If an agent votes twice on the same proposal, the second vote replaces the first (`BTreeMap::insert` overwrites). This allows agents to change their mind — useful if new information arrives during the voting period.

3. **Unknown proposal → Rejected.** Voting on a proposal that doesn't have an active round returns `Rejected`. This prevents late votes on expired rounds from being accepted.

4. **No cryptographic verification.** The `ConsensusShield` trusts that the `agent_id` in a vote is authentic. Cryptographic verification of vote authenticity happens at the `ghost-mesh` layer (which verifies Ed25519 signatures on all inter-agent messages). This separation keeps `cortex-multiagent` focused on consensus logic without depending on cryptographic primitives.

---

## Security Properties

### Byzantine Fault Tolerance

With 2-of-3 consensus, the system tolerates 1 Byzantine (malicious/compromised) agent. The compromised agent can vote `Approve` on malicious proposals, but it still needs 1 more approval from an honest agent. This is the minimum meaningful BFT threshold.

### No Self-Approval

The consensus shield doesn't prevent an agent from voting on its own proposal — that's enforced at the `ghost-mesh` layer. The shield is a pure voting mechanism; policy about who can vote is external.

### Early Rejection

The rejection check (`rejections > M - N`) allows early termination. In a 2-of-3 config, 2 rejections immediately reject the proposal without waiting for the third vote. This prevents a compromised agent from keeping a malicious proposal in `Pending` state indefinitely by never voting.

---

## Downstream Consumer Map

```
cortex-multiagent (Layer 2)
├── ghost-mesh (Layer 4)
│   └── Cross-agent proposal consensus in the A2A network
└── ghost-agent-loop (Layer 7)
    └── Multi-agent coordination for shared goal changes
```

---

## Test Strategy

### Inline Unit Tests

| Test | What It Verifies |
|------|-----------------|
| `consensus_requires_n_of_m_agreement` | 1 approval → Pending, 2 approvals → Approved |
| `consensus_rejected_when_too_many_rejections` | 2 rejections in 2-of-3 → Rejected |

---

## File Map

```
crates/cortex/cortex-multiagent/
├── Cargo.toml
├── src/
│   ├── lib.rs            # Re-exports ConsensusShield
│   └── consensus.rs      # N-of-M voting mechanism
```

---

## Common Questions

### Why not use a Raft or Paxos implementation?

Raft and Paxos solve distributed consensus for replicated state machines — they handle leader election, log replication, and network partitions. GHOST's consensus needs are simpler: "do N agents agree on this specific proposal?" There's no replicated log, no leader, and no need for total ordering. The simple N-of-M voting mechanism is sufficient and dramatically simpler to implement, test, and audit.

### Why is timeout not enforced internally?

Clock-dependent logic is hard to test and introduces non-determinism. By keeping timeout enforcement external, the `ConsensusShield` is a pure function of votes → result, which is trivially testable. The caller (`ghost-mesh`) already has timer infrastructure for network timeouts, so adding proposal timeout there is natural.

### What happens if an agent goes offline during a vote?

The proposal stays in `Pending` state until either enough votes arrive or the timeout expires (enforced by the caller). If the offline agent was needed for quorum, the proposal will be auto-rejected at timeout. This is the correct behavior — an unreachable agent should not be assumed to approve.
