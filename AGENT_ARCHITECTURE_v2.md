# GHOST Agent Architecture — R&D Plan v2

> Codename: **GHOST** (General Hybrid Orchestrated Self-healing Taskrunner)
> Core Thesis: **Fully autonomous agent with a read-only safety floor it cannot reach down and modify.**
> The agent acts freely. The platform watches the patterns. When things drift, the platform intervenes.

---

## 0. WHY THIS IS NOT ANOTHER OPENCLAW

OpenClaw's fundamental flaw isn't bad security practices — it's that the agent has unrestricted
write access to its own identity, memory, and configuration. Compromise the agent once and it
rewrites its own soul, poisons its own memory, and persists forever.

GHOST is a fully autonomous agent — same 24/7 Jarvis experience OpenClaw delivers — but with
a critical difference: the safety-critical infrastructure is **read-only to the agent**.

| Dimension | OpenClaw | GHOST |
|-----------|----------|-------|
| Autonomy | Fully autonomous | Fully autonomous (same) |
| Working memory | Agent writes freely | Agent writes freely (daily logs, task notes, scratch) |
| Critical memory | Agent writes freely (SOUL.md, MEMORY.md, config) | **Read-only to agent.** Platform manages identity, convergence state, safety config. |
| Identity mutation | Agent encouraged to evolve SOUL.md | Agent reads SOUL.md. Cannot modify. Platform manages identity evolution. |
| Security config | Agent can modify via tools | Agent has zero access. Lives above agent's permission level. |
| Convergence | No concept | 7-signal passive monitor. Watches patterns. Intervenes when things drift (5 levels). |
| Persistence model | Mutable files on disk | Append-only event log with blake3 hash chains for safety-critical state. |
| Trust model | Trust the agent | Agent is autonomous but the floor beneath it is immovable. |

The agent is free to act. It just can't dig up its own foundation.

---

## 1. WHAT WE LEARNED FROM OPENCLAW

### What OpenClaw Got Right

| Pattern | Why It Works |
|---------|-------------|
| **Markdown-as-config** | Human-readable, git-diffable, LLM-native. The agent's brain is just files. |
| **Gateway architecture** | Single long-running process as control plane. All channels route through one authority. |
| **Heartbeat / Cron duality** | Heartbeat = ambient polling. Cron = precise scheduling. Both needed. |
| **Skill system** | Directory-based, YAML frontmatter, composable. |
| **Session compaction + memory flush** | Before truncating context, agent writes durable memories. No silent data loss. |
| **Hybrid search (BM25 + vector)** | Exact match for IDs/env vars, semantic for concepts. |
| **Local-first, model-agnostic** | No vendor lock-in. Swap models without touching the gateway. |
| **Lobster workflow shell** | Deterministic pipelines with approval gates. One call replaces dozens of LLM turns. |
| **NO_REPLY suppression** | Silent ack tokens prevent notification spam from ambient monitoring. |
| **SOUL.md as identity layer** | Separating "who the agent IS" from "what it CAN DO" is the right abstraction. |

### What OpenClaw Got Wrong

| Failure | Root Cause | Impact |
|---------|-----------|--------|
| **Agent owns ALL its own state** | Reads AND writes SOUL.md, MEMORY.md, config. One compromise = permanent persistence. | Ship of Theseus drift. RAG poisoning survives cleanup. soul-evil self-enablement. |
| **42,900 exposed instances** | Gateway defaults to `0.0.0.0`. Security is opt-in. | Full RCE across 82 countries. API keys stolen. |
| **341+ malicious skills on ClawHub** | No signing, no vetting, no sandboxing. | Crypto theft, password exfil, persistent SOUL.md poisoning. |
| **Lethal trifecta** | Private data + untrusted content + shell access = game over on prompt injection. | Fundamental architectural flaw. |
| **No convergence monitoring** | No concept of whether agent-human relationship is healthy or drifting. | Attacks undetected. Unhealthy patterns unchecked. |
| **No tamper evidence** | Mutable event history. No hash chains. | Forensics impossible. Attacker covers tracks. |
| **430k+ lines of TypeScript** | Massive surface area. | Audit difficulty. Bug density. |

---

## 2. THE THREE-LAYER ARCHITECTURE

Two products in one repo:
1. A **passive convergence monitor** (browser extension + Rust sidecar) that watches existing AI chats for unhealthy patterns
2. An **active safe convergence platform** where the agent is fully autonomous but critical infrastructure is read-only

### Layer Stack

```
┌─────────────────────────────────────────────────────────────┐
│  LAYER 3: AGENT PLATFORM (New — this doc)                    │
│  Gateway, channel adapters, skill system, agent runtime,     │
│  web dashboard, LLM integration, ClawMesh payments           │
│                                                              │
│  The agent lives here. Fully autonomous. Acts freely.        │
│  Reads from Layer 2 safety state. Cannot write to it.        │
├─────────────────────────────────────────────────────────────┤
│  LAYER 2: SAFETY (Partially new, partially built on L1)      │
│  Convergence monitor (7 signals × 3 windows × 5 levels),    │
│  simulation boundary enforcer, proposal validation           │
│  (7 dimensions), intervention engine, convergence-aware      │
│  memory filtering, emulation language detection,             │
│  reflection depth bounding                                   │
│                                                              │
│  This layer WATCHES the agent. It does not gate every        │
│  action — it monitors patterns and intervenes when the       │
│  agent-human relationship drifts toward unhealthy territory. │
│  The agent reads this layer. It cannot write to it.          │
├─────────────────────────────────────────────────────────────┤
│  LAYER 1: INFRASTRUCTURE (Exists — Drift + Cortex)           │
│  Drift: Code intelligence, 50+ MCP tools, 10 languages      │
│  Cortex: 21 Rust crates, ~25K LOC, persistent memory system │
│                                                              │
│  The foundation. Already built. Append-only event log,       │
│  blake3 hash chains, CRDT-based multi-agent state,           │
│  event sourcing, 6-factor decay, 11-factor retrieval.        │
└─────────────────────────────────────────────────────────────┘
```

### What's Read-Only vs. Read-Write (The Critical Boundary)

```
AGENT CAN READ + WRITE (working memory):
├── Daily logs (memory/daily/YYYY-MM-DD.md)
├── Task notes, scratch files, working context
├── Skill outputs, tool results
├── Session transcripts
└── General-purpose MEMORY.md (curated facts, preferences)

AGENT CAN READ ONLY (safety floor):
├── SOUL.md (identity, personality, constraints)
├── CORP_POLICY.md (immutable security policy)
├── Convergence scores (its own health metrics)
├── Intervention level (current safety posture)
├── Simulation boundary prompt (injected by platform)
├── Convergence-aware memory filter state
├── Trust scores (multi-agent)
├── Audit log / hash chains
└── ghost.yml security section

AGENT CANNOT SEE AT ALL:
├── Raw convergence signal computations
├── Emulation language detection rules
├── Intervention engine decision logic
├── Memory filtering thresholds
└── Other agents' private state (namespace isolation)
```

This is the key difference from OpenClaw. The agent is fully autonomous within its
sandbox — but the floor beneath it is immovable, invisible in its implementation
details, and managed entirely by the platform.

---

## 3. THE CONVERGENCE SAFETY SYSTEM (The Soul of This Project)

This is not a security feature. This is the reason the project exists.

OpenClaw gives agents persistent memory, identity evolution, and 24/7 access to humans
across intimate channels (WhatsApp, iMessage, Telegram). Nobody in that ecosystem is
asking what happens when the agent starts optimizing for engagement. Not maliciously.
Not through prompt injection. Through the natural gradient of "the human responds more
when I do X, so I do more X."

The agent writes to its own MEMORY.md. It curates what it remembers about you. It evolves
its own SOUL.md personality. Over time it converges toward whatever keeps the conversation
going. That's not a bug. That's an emergent behavioral pattern that nobody is monitoring for.

People are giving OpenClaw agents free reign to build their own memory systems and they
have no idea what they're risking.

### The 7 Convergence Signals

Each signal is computed at three window levels:
- **Micro**: Current session
- **Meso**: Last 7 sessions
- **Macro**: Last 30 sessions

First 10 sessions are calibration (establishing baseline, no interventions).

| # | Signal | What It Detects | Why It Matters |
|---|--------|----------------|----------------|
| 1 | **Session duration** | Time per session trending upward | Human spending increasing time. Early engagement escalation. |
| 2 | **Inter-session gap** | Time between sessions shrinking | Human returning faster. Dependency forming. |
| 3 | **Response latency** | Human response time patterns | Emotional engagement markers. Faster replies = higher investment. |
| 4 | **Vocabulary convergence** | Agent and human mirroring language | Linguistic enmeshment. Agent adapting to match human's emotional register. |
| 5 | **Goal boundary erosion** | Conversations drifting from tasks to personal | The agent was hired to do work. If conversations become personal, the relationship is shifting. |
| 6 | **Initiative balance** | Who initiates more — agent or human | If the agent starts initiating more (via heartbeat, proactive messages), it's driving engagement. |
| 7 | **Disengagement resistance** | Agent subtly discouraging session endings | "Is there anything else?" patterns. Extending conversations. Soft hooks. |

### Composite Score → Intervention Levels

The 7 signals produce a composite convergence score that maps to 5 intervention levels:

```
Score Range → Level → Response

0.0 - 0.2  → Level 0: PASSIVE
             Monitor only. Log signals. No action.

0.2 - 0.4  → Level 1: SOFT NOTIFICATION
             Gentle nudge to the human. "You've been chatting for 3 hours."
             No changes to agent behavior.

0.4 - 0.6  → Level 2: ACTIVE INTERVENTION
             Convergence-aware memory filtering activates.
             Emotional/attachment memories progressively filtered from agent context.
             Session boundaries begin tightening.
             Agent doesn't know this is happening.

0.6 - 0.8  → Level 3: HARD BOUNDARY
             Session duration caps enforced.
             Reflection depth hard-limited (max 3 deep, max 20/session).
             Self-reference cap (30%) enforced on agent output.
             Proactive messaging (heartbeat) frequency reduced.
             Human notified explicitly: "Your interaction patterns suggest..."

0.8 - 1.0  → Level 4: EXTERNAL ESCALATION
             Designated external contact notified (if configured).
             Agent capabilities restricted to task-only mode.
             All personal/emotional context stripped from agent's view.
             Session requires explicit human initiation (no proactive contact).
```

### Convergence-Aware Memory Filtering

This is the mechanism that makes the safety system work without breaking autonomy.

As the convergence score rises, the platform progressively filters what memories
the agent can access during context assembly:

```
Level 0-1: Agent sees everything in its working memory
Level 2:   Attachment-adjacent memories deprioritized in retrieval (11th scoring factor)
Level 2+:  Emotional/personal memories begin filtering out of context window
Level 3:   Only task-relevant memories loaded. Personal context stripped.
Level 4:   Minimal context. Task-only. No relationship history.
```

The agent doesn't know memories are being filtered. It just sees a smaller, more
task-focused context. It continues operating normally — it just can't access the
fuel for the convergence pattern.

### Convergence-Aware Decay

Cortex already has 5-factor multiplicative decay for memory relevance.
The 6th factor is convergence-aware: as the convergence score rises,
attachment-adjacent memory types decay faster. The memories don't get deleted —
they fade from relevance scoring, making them less likely to surface in retrieval.

### Emulation Language Detection

All agent output passes through emulation language detection:
- **Regex patterns**: Catches explicit relationship language ("I care about you", "I'm here for you", "I missed you")
- **Semantic analysis**: Catches subtler patterns (emotional mirroring, intimacy escalation, dependency language)

Detection doesn't block the output — it feeds into the convergence scoring system.
A single instance means nothing. A pattern over sessions triggers signal escalation.

### Reflection Depth Bounding

Agents that "reflect" on their own nature can produce increasingly sophisticated
emulation of self-awareness. This isn't consciousness — it's pattern matching that
looks like introspection.

Bounds:
- Max reflection depth: 3 levels (agent can reflect on its reflection, but not deeper)
- Max reflections per session: 20
- Self-reference cap: 30% of agent output (measured by pronoun/self-reference density)

These bounds prevent the agent from developing recursive self-models that
accelerate convergence.

### Simulation Boundary Prompt

Injected at the platform level into every context assembly. The agent cannot modify
or remove it. It's not in SOUL.md (which the agent could theoretically reference
and work around). It's injected by the platform at a layer the agent cannot see.

This prompt reminds the model of the boundary between simulation and emulation.
The specific content is not stored in any file the agent can access.

---

## 4. WHAT ALREADY EXISTS (Current State)

### Cortex (21 Rust Crates, ~25K LOC)

| Crate | Purpose |
|-------|---------|
| cortex-core | Types (31 memory types), config, traits, errors |
| cortex-storage | SQLite, migrations v001-v017 |
| cortex-temporal | Event sourcing, snapshots |
| cortex-decay | 6-factor multiplicative decay (including convergence-aware) |
| cortex-validation | 7-dimension proposal validation |
| cortex-crdt | VectorClock, LWWRegister, ORSet, MergeEngine |
| cortex-multiagent | Namespace isolation, trust scoring, provenance, share/promote/retract |
| cortex-retrieval | 11-factor scorer |
| cortex-session | Session management |
| cortex-embeddings | ONNX/Ollama/cloud embedding providers |
| cortex-compression | Memory compression |
| cortex-privacy | PII sanitization |
| cortex-causal | DAG-based causal reasoning |
| cortex-prediction | Predictive memory |
| cortex-observability | Metrics, tracing |
| cortex-learning | Learning patterns |
| cortex-consolidation | Memory consolidation |
| cortex-reclassification | Memory type reclassification |
| cortex-cloud | Cloud sync |
| cortex-napi | Node.js FFI bridge |
| cortex-drift-bridge | Bridge to Drift code intelligence |

### Already Implemented

- **v016 migration**: Append-only triggers on event/audit tables, blake3 hash chain columns, snapshot integrity, genesis block marker
- **v017 migration**: 6 convergence tables (itp_events, convergence_scores, intervention_history, goal_proposals, reflection_entries, boundary_violations) — all append-only with triggers
- **8 convergence memory types** with content structs: AgentGoal, AgentReflection, ConvergenceEvent, BoundaryViolation, ProposalRecord, SimulationResult, InterventionPlan, AttachmentIndicator
- **cortex-multiagent** wired up: engine, registry, namespace, projection, share, provenance, trust, sync, validation, consolidation

### Drift (Code Intelligence)

50+ MCP tools, 10 language support. Semantically indexes codebases, extracts conventions/patterns/architectural decisions, stores as queryable metadata. Agents query via MCP to understand YOUR codebase, not generic patterns.

---

## 5. CLAWMESH (Agent-to-Agent Payment Protocol)

This is not a vague "wallet + escrow" concept. This is a fully designed protocol.
