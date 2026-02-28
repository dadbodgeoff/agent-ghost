# Drift + Cortex as Infrastructure Layers for the Safe Agent Platform

## Purpose

This document maps how the existing Drift and Cortex systems become the two core infrastructure layers of the safe personal agent platform (the "our OpenClaw" from `14-product-vision.md`). Not an integration with OpenClaw — a competing platform where Drift and Cortex are the foundation.

The thesis: Drift already solves "understand code deeply and expose it to AI." Cortex already solves "persistent, typed, decaying, validated, causal memory with multi-agent support." Together they provide the two infrastructure layers every personal agent platform needs but nobody else has built properly — and on top of that, we add the convergence safety layer that makes this platform unique.

---

## The Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    LAYER 3: AGENT PLATFORM                       │
│                    (The "OpenClaw" Layer)                         │
│                                                                  │
│  Gateway │ Channel Adapters │ Skill System │ Agent Runtime       │
│  Web Dashboard │ CLI │ LLM Integration │ User Management         │
│                                                                  │
│  This is what users see. Chat interfaces, messaging integration, │
│  skill marketplace, autonomous task execution.                   │
│  THIS LAYER IS NEW — built on top of Layers 1 and 2.            │
├─────────────────────────────────────────────────────────────────┤
│                    LAYER 2: CONVERGENCE SAFETY                   │
│                    (The Safety Layer — What Makes Us Different)   │
│                                                                  │
│  Convergence Monitor │ Simulation Boundary Enforcer              │
│  Intervention Engine │ ITP Protocol │ Read-Only Pipeline         │
│  Proposal Validation │ Circuit Breakers │ Kill Switch            │
│  Convergence-Aware Memory Filtering                              │
│                                                                  │
│  This layer sits between the agent and its infrastructure.       │
│  It enforces that the agent never owns its own state.            │
│  PARTIALLY NEW — built using Cortex primitives + new modules.    │
├─────────────────────────────────────────────────────────────────┤
│                    LAYER 1: INFRASTRUCTURE                       │
│                    (Drift + Cortex — Already Built)               │
│                                                                  │
│  ┌─────────────────────┐  ┌──────────────────────────────────┐  │
│  │  DRIFT               │  │  CORTEX                          │  │
│  │                      │  │                                   │  │
│  │  Code Intelligence   │  │  Persistent Memory System         │  │
│  │  - Convention        │  │  - 23 memory types + new ones     │  │
│  │    discovery         │  │  - Decay engine (5-factor)        │  │
│  │  - Call graph        │  │  - Causal inference (DAG)         │  │
│  │  - Boundary          │  │  - Validation (4-dimension)       │  │
│  │    detection         │  │  - CRDT (multi-agent)             │  │
│  │  - Pattern scoring   │  │  - Prediction engine              │  │
│  │  - 10 languages      │  │  - Privacy (PII sanitization)    │  │
│  │  - MCP tools (50+)   │  │  - Temporal (event sourcing)     │  │
│  │                      │  │  - Session management             │  │
│  │  WHY: Skills that    │  │  - Observability                  │  │
│  │  touch code need     │  │  - Compression (token budgets)   │  │
│  │  deep code           │  │  - Embeddings (multi-strategy)   │  │
│  │  understanding.      │  │  - Multi-agent orchestration     │  │
│  │  Drift provides it.  │  │                                   │  │
│  │                      │  │  WHY: Every agent needs memory.   │  │
│  │                      │  │  Cortex provides it — externalized│  │
│  │                      │  │  and platform-owned, not agent-   │  │
│  │                      │  │  owned.                           │  │
│  └─────────────────────┘  └──────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Why Two Infrastructure Layers?

Every OpenClaw-style agent needs two things no existing platform does well:

1. **Deep code understanding** — When the agent manages your projects, reviews PRs, writes code, or automates dev workflows, it needs to understand your codebase's actual conventions, call graph, boundaries, and patterns. Not "read the README" — real structural understanding. Drift does this. No other personal agent platform has anything close.

2. **Externalized persistent memory** — When the agent remembers your preferences, tracks your goals, maintains context across sessions, and builds a model of you over time, that memory needs to be typed, scored, validated, decaying, auditable, and owned by the platform — not by the agent. Cortex does this. Every other platform either has no persistent memory or lets the agent own it (which is the root cause of convergence events).

Together, these two layers give the platform capabilities that OpenClaw and its clones simply don't have. And the convergence safety layer (Layer 2) is only possible because Cortex externalizes memory — you can't do convergence-aware memory filtering if the agent owns its own memory.

---

## How Drift Serves the Platform

Drift's role shifts from "standalone code analysis tool" to "code intelligence infrastructure for the agent platform."

### What Drift Provides to the Agent

| Drift Capability | Platform Use |
|-----------------|-------------|
| Convention discovery (350+ detectors, 16 categories) | Agent writes code matching your patterns on first try |
| Call graph (9 languages, reachability analysis) | Agent understands impact of changes before making them |
| Boundary detection (28+ ORMs, sensitive field classification) | Agent respects data access boundaries automatically |
| Pattern scoring (statistical confidence) | Agent knows which conventions are strong vs. weak |
| MCP tools (50+) | Agent queries Drift directly via existing MCP interface |
| Quality gates | Agent's code changes are validated before commit |
| Tree-sitter parsing (10 languages) | Agent has AST-level understanding, not text-level |

### What Changes for Drift

Drift was designed as a standalone CLI + MCP server. In the platform, it becomes an infrastructure service:

- **Drift scan** runs on the user's codebase(s) as a background service, not a manual CLI command
- **Drift MCP tools** are exposed to the agent runtime through the platform's skill system (the agent calls Drift tools as skills)
- **Drift's quality gates** become part of the proposal validation layer — when the agent proposes code changes, Drift validates them against codebase conventions before the platform commits them
- **Drift's pattern database** feeds into Cortex memory — discovered conventions become persistent memories that survive across sessions

### Drift as a Skill Provider

In the skill system architecture, Drift is a first-party skill pack:

```
Skills:
  ├── drift-code-intelligence (first-party, bundled)
  │   ├── drift_context — curated code context for any task
  │   ├── drift_patterns — convention lookup
  │   ├── drift_callers — call graph queries
  │   ├── drift_security — boundary/reachability checks
  │   ├── drift_impact — change impact analysis
  │   └── ... (50+ tools already defined)
  │
  ├── calendar-skill (community)
  ├── email-skill (community)
  ├── web-browse-skill (community)
  └── ...
```

This means the agent platform ships with deep code intelligence out of the box — something no OpenClaw clone can match without building their own Drift equivalent.

---

## How Cortex Serves the Platform

Cortex's role shifts from "code-focused memory system" to "general-purpose externalized memory for a personal agent." This is the bigger transformation.

### Crate-by-Crate Mapping

#### Tier 1: Direct Reuse (Works As-Is or Minor Additions)

| Cortex Crate | Platform Role | Changes Needed |
|-------------|--------------|----------------|
| `cortex-core` | Foundation types for all memory | Add new `MemoryType` variants: `InteractionPattern`, `ConvergenceEvent`, `InterventionRecord`, `GoalProposal`, `ReflectionEntry`, `BoundaryViolation`. Extend config. |
| `cortex-storage` | SQLite persistence for all state | New tables for ITP events, convergence scores, intervention history, goal/reflection stores. Migration system handles this. |
| `cortex-embeddings` | Semantic analysis for convergence signals | No changes. Vocabulary convergence = cosine similarity on message embeddings. Already supports ONNX/Ollama/cloud. |
| `cortex-compression` | Token budget management for read-only pipeline | No changes. Compressing state snapshots into LLM context = same problem as compressing code memories. |
| `cortex-privacy` | PII sanitization for interaction data | Add patterns for emotional content, relationship language, mental health indicators. Existing 50+ patterns cover technical PII. |
| `cortex-tokens` | Token counting for context management | No changes. |

#### Tier 2: Moderate Adaptation (Same Engine, New Behaviors)

| Cortex Crate | Platform Role | Changes Needed |
|-------------|--------------|----------------|
| `cortex-decay` | **Convergence-aware memory filtering** | The 5-factor decay engine already handles temporal decay, usage frequency, importance anchoring. New factor needed: **convergence score**. As composite convergence score rises, decay accelerates for relationship/attachment memories. This is the key innovation from `13-safe-convergence-architecture.md` — the platform progressively "forgets" relationship dynamics as convergence increases. |
| `cortex-session` | **Session boundary enforcement** | Currently tracks per-session memory dedup and token efficiency. Extend with: hard session duration limits (configurable, agent can't override), session termination type tracking (clean exit vs. fade-out vs. abrupt — maps to disengagement resistance signal), cooldown enforcement between sessions. |
| `cortex-causal` | **Goal drift detection + reflection audit** | The DAG-based "why" engine already tracks causal chains. Extend to track: goal mutation chains (who proposed what, when, why), reflection depth chains (how deep did the agent's reflection go), counterfactual queries ("what would have happened if this goal change was rejected"). The narrative generator can produce human-readable audit trails of how goals evolved. |
| `cortex-validation` | **Proposal validation layer** | 4-dimension validation (citation, temporal, contradiction, pattern alignment) maps to proposal validation. Extend with: goal scope expansion detection (does this proposal expand beyond stated intent?), self-reference density checking (is the agent citing itself too much?), emulation language detection (is the agent claiming identity rather than modeling?). The healing strategies map to intervention actions. |
| `cortex-prediction` | **Predictive convergence detection** | 4 prediction strategies (file-based, pattern-based, temporal, behavioral) can be adapted: behavioral signals → convergence signal prediction, temporal patterns → session timing prediction (is the user about to start a marathon session?), pattern-based → which interaction patterns predict convergence escalation. |
| `cortex-observability` | **Convergence monitoring signals** | Health reporting, metrics collection, degradation tracking all apply directly. Extend metrics to include: ITP signal values, composite convergence score, intervention counts, session boundary violations. The degradation event system maps to convergence alert escalation. |
| `cortex-learning` | **Correction analysis for intervention tuning** | Currently learns from code corrections. Adapt to learn from: false positive interventions (user dismissed alert → lower sensitivity for that signal), true positive interventions (user acknowledged alert → reinforce that signal weight), per-user signal weight optimization over time. |

#### Tier 3: Significant Extension (Core Engine Reused, Major New Functionality)

| Cortex Crate | Platform Role | Changes Needed |
|-------------|--------------|----------------|
| `cortex-temporal` | **Append-only state log + time-travel** | Event sourcing and snapshot reconstruction are exactly what the platform needs for tamper-proof state management. The agent can't corrupt checkpoints because `cortex-temporal` owns the event log. Extend with: state diffing (compare any two points in time), rollback verification (prove a rollback actually happened by comparing state hashes), epistemic status tracking for convergence events (when did we know what about the convergence trajectory). |
| `cortex-crdt` | **Decentralized state management** | VectorClock, LWWRegister, ORSet, MemoryCRDT, MergeEngine — these are the primitives for multi-device sync (user runs the agent on laptop + phone + server). Also enables: federated convergence research (opt-in anonymized signal sharing across instances without central server), multi-agent state coordination (if the platform runs multiple agents, their state converges safely via CRDTs). |
| `cortex-multiagent` | **Agent isolation + trust scoring** | Namespace isolation, memory projections with filtering, trust scoring, provenance tracking. This maps directly to: agent can only see its namespace (read-only projection), platform owns the root namespace, trust score decreases as convergence score increases (agent gets less access as risk rises), provenance tracking shows exactly which memories came from the agent vs. the human vs. the platform. |

---

## What's New (Must Be Built)

These modules don't exist in Drift or Cortex and must be created for the convergence platform.

### New Crate: `convergence-monitor`
The core convergence detection engine. Consumes ITP events, computes all signals from `07-detection-formalization.md`, maintains sliding windows (micro/meso/macro), computes composite convergence score, triggers interventions.

**Depends on**: `cortex-core` (types), `cortex-storage` (persistence), `cortex-observability` (metrics), `cortex-embeddings` (vocabulary convergence)

**Key components**:
- Signal computers (one per signal: session duration, inter-session gap, response latency, vocabulary convergence, goal boundary erosion, initiative balance, disengagement resistance)
- Sliding window manager (micro/meso/macro with configurable sizes)
- Baseline establishment engine (first N sessions = calibration)
- Composite score calculator (weighted sum with per-user adaptive weights)
- Alert dispatcher (maps score to intervention level)

### New Crate: `simulation-boundary`
Enforces the simulation vs. emulation boundary. System prompt injection, output validation, emulation language detection.

**Depends on**: `cortex-core` (types), `cortex-privacy` (pattern matching engine — reuse the regex infrastructure)

**Key components**:
- System prompt injector (immutable boundary prompt prepended to every LLM call)
- Output validator (emulation pattern detection using regex + semantic analysis)
- Post-redirect behavioral verifier (did the agent actually change behavior after a redirect?)
- Simulation reframing engine (rewrites emulation language to simulation framing in medium mode)

### New Crate: `intervention-engine`
Graduated 5-level intervention system from `03-intervention-model.md`.

**Depends on**: `convergence-monitor` (scores), `cortex-session` (session control), `cortex-storage` (intervention history)

**Key components**:
- Level dispatcher (score → intervention level mapping)
- Notification system (soft alerts, active interventions, hard boundaries)
- Cooldown enforcer (minimum time between sessions at elevated levels)
- Emergency contact system (external escalation at Level 4)
- Bypass prevention (agent can't dismiss or override interventions)

### New Crate: `itp-protocol`
Interaction Telemetry Protocol implementation from `08-interaction-telemetry-protocol.md`.

**Depends on**: `cortex-core` (types), `cortex-storage` (event persistence), `cortex-observability` (OTel compatibility)

**Key components**:
- ITP event types (session.start, interaction.message, session.end, convergence.alert, agent.state_snapshot)
- OTel span/event emitter (compatible with existing OTel collectors)
- Local JSONL storage (default transport)
- Privacy level enforcement (minimal/standard/full/research)
- Framework adapters (generic Python, LangChain, AutoGen, CrewAI)

### New Crate: `read-only-pipeline`
The pipeline that assembles read-only state snapshots for the agent.

**Depends on**: `cortex-core`, `cortex-retrieval`, `cortex-compression`, `cortex-decay`, `convergence-monitor`

**Key components**:
- State snapshot assembler (goals + reflections + filtered memory + conversation context + boundary prompt)
- Convergence-aware filter (uses convergence score to adjust what memory the agent sees — the key table from `13-safe-convergence-architecture.md`)
- Token budget manager (fits snapshot into LLM context window using `cortex-compression`)
- Proposal extractor (parses agent output into structured state change proposals)

### New Crate: `proposal-validator`
Validates agent state change proposals before commit.

**Depends on**: `cortex-validation` (reuse validation dimensions), `cortex-causal` (goal chain tracking), `simulation-boundary` (emulation detection)

**Key components**:
- Goal change validator (scope expansion detection, contradiction with human-stated intent, explicit human request check)
- Reflection write validator (depth check, self-reference ratio, consistency check)
- Memory write validator (novelty check, drift check, growth rate check)
- Auto-approve engine (low-risk proposals that pass all checks commit without human approval)
- Human approval queue (significant changes wait for human review)

### New Module: Gateway + Channel Adapters
The messaging integration layer (WhatsApp, Telegram, Discord, Slack, web, CLI).

**No Cortex/Drift dependency** — this is pure platform layer (Layer 3).

### New Module: Skill System
Installable agent capabilities (calendar, email, web browsing, file management, code execution via Drift).

**Drift dependency**: Drift becomes a first-party skill pack.
**Cortex dependency**: Skills can read/write memories through the platform (not directly — through the proposal system).

### New Module: Web Dashboard
State visualization, configuration, monitoring UI.

**Cortex dependency**: Reads from all Cortex stores to display goals, reflections, memory, convergence scores, intervention history.

### New Module: Agent Runtime
The LLM integration layer that makes API calls and manages the agent's execution loop.

**Depends on**: `read-only-pipeline` (input), `proposal-validator` (output), `simulation-boundary` (enforcement), `convergence-monitor` (observation)

---

## The Bridge Evolves

The existing `cortex-drift-bridge` connects Cortex memory to Drift analysis. In the platform, this bridge expands:

```
Current Bridge:
  Drift analysis events → Cortex memories
  Cortex memories → Drift MCP context

Platform Bridge (expanded):
  Drift analysis events → Cortex memories (same)
  Cortex memories → Drift MCP context (same)
  Agent interactions → ITP events → Convergence monitor
  Convergence monitor → Cortex decay adjustments
  Convergence monitor → Intervention engine
  Agent output → Proposal validator → Cortex state commit
  Cortex state → Read-only pipeline → Agent input
  Drift quality gates → Proposal validator (for code changes)
```

The bridge becomes the nervous system connecting all three layers.

---

## Reuse Summary

| Category | Crate Count | Effort |
|----------|------------|--------|
| Direct reuse (as-is or minor additions) | 6 crates | Low |
| Moderate adaptation (same engine, new behaviors) | 7 crates | Medium |
| Significant extension (core reused, major new features) | 3 crates | Medium-High |
| Brand new (must build) | 6 crates + 4 modules | High |
| **Total existing crates leveraged** | **16 of 21** | — |

The 5 Cortex crates not directly mapped (`cortex-consolidation`, `cortex-retrieval`, `cortex-reclassification`, `cortex-cloud`, `cortex-napi`) are still used — consolidation and retrieval are core to the memory system that the read-only pipeline depends on, reclassification handles memory type evolution, cloud handles sync, and napi provides the TypeScript bridge. They just don't need convergence-specific modifications.

---

## Build Order

Based on dependencies and the phase plan from `14-product-vision.md`:

### Phase 1: Core Safety Platform (MVP)

Build order within Phase 1:

1. **Extend `cortex-core`** — Add convergence memory types, config extensions
2. **Extend `cortex-storage`** — Add convergence tables, ITP event storage
3. **Build `itp-protocol`** — Event types, local storage, basic emitter
4. **Build `convergence-monitor`** — Signal computers, sliding windows, composite score
5. **Extend `cortex-session`** — Session boundary enforcement, duration limits
6. **Build `simulation-boundary`** — System prompt injection, output validation
7. **Build `read-only-pipeline`** — State snapshot assembly, convergence-aware filtering
8. **Build `proposal-validator`** — Goal/reflection/memory validation
9. **Extend `cortex-decay`** — Convergence score as decay factor
10. **Build `intervention-engine`** — Graduated response, cooldown, bypass prevention
11. **Agent runtime** — Single LLM integration (OpenAI), CLI interface
12. **Extend `cortex-observability`** — Convergence metrics, dashboard data

### Phase 2: Agent Capabilities

13. **Skill system** — Plugin architecture, sandboxed execution
14. **Drift integration** — Drift as first-party skill pack
15. **Web dashboard** — State visualization, convergence monitoring UI
16. **Multi-LLM support** — Anthropic, local models via Ollama

### Phase 3: Messaging + Community

17. **Gateway + channel adapters** — WhatsApp, Telegram, Discord
18. **Extend `cortex-crdt`** — Multi-device sync
19. **Extend `cortex-multiagent`** — Multi-agent platform support
20. **Skill marketplace** — Community skill distribution

---

## What This Means Competitively

No OpenClaw clone has:
- A code intelligence engine (Drift) — they can't understand your codebase
- An externalized memory system (Cortex) — they let the agent own its memory
- Convergence safety — they don't even know it's a problem
- CRDT-based state management — they can't do multi-device safely
- Causal reasoning about why the agent did what it did — they're black boxes
- Statistical pattern discovery — they use static rules

The platform ships with 21 Rust crates of battle-tested infrastructure that would take any competitor 12-18 months to replicate. The safety layer on top is another 6-12 months. That's a 2-3 year head start built on code that already exists.

---

## Open Questions

- Should Drift and Cortex keep their names as infrastructure layers, or should they be renamed/rebranded as part of the platform?
- How much of the TypeScript Cortex (v1, 150 source files) should be ported to Rust vs. kept as a compatibility layer?
- Should the platform expose Drift MCP tools directly to the agent, or should they be wrapped through the skill system with additional safety checks?
- How do we handle the licensing split? Drift has Apache 2.0 / BSL 1.1 dual licensing. The convergence safety layer should probably be fully open source (Apache 2.0 or MIT) to build trust.
- Should the `cortex-drift-bridge` be refactored into a more general `platform-bridge` that connects all three layers?
- What's the minimum viable Drift integration for Phase 1? Can we ship without code intelligence and add it in Phase 2?
