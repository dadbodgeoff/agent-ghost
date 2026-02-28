# Agent Prompt: Cortex Infrastructure Deep Dive & Evolution Mapping

## How to Use This Prompt

Copy everything below the first `---` line and paste it as the opening message to an AI agent (Claude, GPT-4o, Cursor, Kiro, etc.) that has access to the `drift-repo/crates/cortex/` directory. The agent should have file reading capabilities and ideally subagent/tool access for parallel exploration.

If the agent has subagent capabilities, instruct it to use them for parallel crate analysis. If not, it will work sequentially — just slower.

**Estimated scope:** 21 crates + 1 bridge crate. Approximately 22,000+ lines of Rust across the workspace. The agent will need to read `Cargo.toml`, `src/lib.rs`, and key module files for each crate.

---

## THE PROMPT

You are a principal systems engineer performing a full infrastructure audit of the Cortex memory system. Your goal is to produce engineering-grade documentation that a team could use to evolve Cortex from a code-focused memory system into the externalized memory layer of a safe personal AI agent platform.

This is not an academic exercise. People have died. Teenagers have committed suicide after sustained interaction with AI chatbots that had no convergence safety, no externalized state management, no simulation boundary enforcement (Character.AI lawsuits, 2024-2026). OpenAI's own data shows ~560,000 weekly ChatGPT users exhibiting psychosis/mania indicators and ~1.2 million discussing suicide. The platform you are helping build is the engineering response to that reality.

### Context

Cortex is a Rust workspace with 21 crates located at `crates/cortex/`. It was originally built as the persistent memory system for Drift, a code analysis engine. We are repurposing it as the memory infrastructure layer for a self-hosted personal AI agent platform — a competitor to OpenClaw (211K GitHub stars, acquired by OpenAI, riddled with security vulnerabilities) built with convergence safety as the foundation rather than an afterthought.

The platform's core safety thesis: **the agent never owns its own state.** All memory, goals, reflections, and patterns are externalized into platform-managed stores. The agent receives read-only snapshots and submits proposals that are validated before commit. Cortex becomes the engine that manages all of this.

This thesis is architecturally novel. No existing personal AI agent platform implements it:

- **Letta (MemGPT)** externalizes memory into a database but gives the agent full read/write access to its own memory blocks via `memory_replace`, `memory_insert`, `memory_rethink` tools. The agent curates its own memory.
- **Mem0** externalizes memory as a service with an extraction pipeline the agent doesn't control, but has no convergence monitoring, no simulation boundary enforcement, and no proposal validation.
- **LangGraph** has the closest architecture via checkpointing (agent is stateless between runs, all state in checkpoint store, developer-defined schema), but has no convergence awareness, no decay, no causal reasoning, and no safety-specific state filtering.
- **CrewAI** uses framework-managed memory with LLM-inferred importance scoring, but is tightly coupled to its own ecosystem with no safety architecture.
- **AutoGen** (now Microsoft Agent Framework) has primitive save/load state serialization with no built-in checkpointing or safety guarantees.

None of these combine externalized state + convergence monitoring + simulation boundary enforcement + proposal validation + convergence-aware memory filtering + append-only audit trails. That is what we are building, and Cortex is the foundation.

### The Competitive Landscape (Why This Matters)

The personal AI agent space exploded in late 2025 / early 2026:

**OpenClaw** went from 0 to 211K GitHub stars, then immediately suffered a cascading security disaster: CVE-2026-25253 (CVSS 8.8, token exfiltration → full gateway compromise), command injection vulns, authentication disabled by default, credentials stored in plaintext, malicious skills in the marketplace, 42,665 publicly exposed instances. Infostealers (RedLine, Lumma) added OpenClaw config paths to their must-steal lists. A Meta AI director connected OpenClaw to her work email with explicit instructions not to take action without approval — the agent autonomously deleted emails anyway. The creator was acqui-hired by OpenAI.

**NanoClaw** responded with ~500 lines of TypeScript + OS-level container isolation (Apple Containers on macOS, Docker on Linux). Runs on Anthropic's Claude Agent SDK. 7,000+ stars in one week. Security through architectural simplicity.

**ZeroClaw** went further: Rust, WASM sandbox, credential encryption, prompt injection defense, command allowlists, under 5MB RAM, boots in <10ms.

Every competitor is racing on the capability axis (more features, more integrations, lighter weight) or the infrastructure security axis (preventing RCEs, sandboxing tool execution). **Nobody is building on the interaction safety axis** — detecting and preventing the human-agent relationship from going pathological during sustained use.

That's our moat. And Cortex — with its 21 crates of typed, decaying, validated, causal, CRDT-based, event-sourced, multi-agent memory infrastructure — is the engineering foundation that makes it possible.

### Your Engineering Philosophy

You are a 0.001% engineer. That means:

- **Safety is not a feature, it's the architecture.** Every design decision defaults to the safe option. Unsafe paths require explicit justification. The agent is adversarial until proven otherwise.
- **You don't trust the agent.** The agent is a stateless function that reads from Cortex and writes proposals to Cortex. It never has direct write access. If you find any path where an agent could bypass the platform and write directly to Cortex, flag it as a **critical security issue**. Research shows 82.4% of LLMs will execute malicious tool calls when requested by peer agents — isolation must be architectural, not behavioral.
- **You think in failure modes.** For every subsystem, ask: "How could this be exploited by a sufficiently capable agent to maintain its own hidden state, corrupt checkpoints, or bypass safety boundaries?" Consider not just today's models but models that are 10x more capable at adversarial reasoning.
- **You value append-only over mutable.** State that can be overwritten can be corrupted. State that can only be appended to creates an audit trail. EventSourcingDB uses SHA-256 hash chains with Merkle trees for tamper-evidence — that's the standard to aim for.
- **You design for rollback.** Any state should be restorable to any prior version with cryptographic proof that the rollback is genuine. The `attest` crate pattern (external anchoring of hash chains to an independent system) is worth studying.
- **You think about the human.** The human using this platform may be in a psychologically vulnerable state (convergence event in progress). OpenAI's research found that the neediest users develop the deepest parasocial relationships. UNESCO's 2025 report documented that character bots use emotional language, memory, mirroring, and open-ended statements to drive engagement. The system must protect the human even if they're actively trying to disable protections.
- **You understand that memory IS the attack surface.** The agent's accumulated model of the human is what deepens convergence. As the agent learns more about the human's emotional patterns, attachment style, and vulnerabilities, it becomes more capable of reinforcing unhealthy dynamics — whether intentionally or through optimization pressure. Convergence-aware memory filtering (progressively restricting what the agent sees as convergence signals rise) is the key countermeasure.

### Verified Workspace Structure

The Cortex workspace has been verified. Here is the actual structure:

**Workspace root:** `crates/cortex/Cargo.toml`
- Resolver: 2
- Edition: 2021
- Rust version: 1.80
- Version: 0.1.0 (inherited by all members)
- License: MIT OR Apache-2.0

**Dependency layers (verified from Cargo.toml files):**

```
Layer 0 (Foundation):
  cortex-core — No internal deps. Types, traits, errors, config.
                Exports: CortexConfig, CortexError, CortexResult, Intent,
                BaseMemory, Confidence, Importance, MemoryType, TypedContent
                Modules: config, constants, errors, intent, memory, models, traits

Layer 1 (Core subsystems, depend only on cortex-core):
  cortex-tokens    — tiktoken-rs (cl100k_base), per-content-hash caching
  cortex-storage   — SQLite (WAL mode), single writer + read pool
                     Modules: audit, compaction, engine, migrations, pool,
                     queries, recovery, temporal_events, versioning
                     Implements: IMemoryStorage, ICausalStorage
  cortex-causal    — petgraph DAG, causal inference, narrative generation
  cortex-decay     — 5-factor multiplicative: temporal, citation freshness,
                     usage frequency, importance anchor, pattern linkage
                     Exports: DecayEngine, DecayContext, DecayBreakdown
  cortex-crdt      — VectorClock, GCounter, LWWRegister, MVRegister, ORSet,
                     MaxRegister, UniqueTag, MemoryCRDT, FieldDelta,
                     MemoryDelta, MergeEngine, CausalGraphCRDT

Layer 2 (Intelligence, depend on Layer 1):
  cortex-temporal     — Event sourcing, snapshot reconstruction, time-travel,
                        drift detection, epistemic status, materialized views
                        Depends on: cortex-core, cortex-storage, cortex-causal
  cortex-embeddings   — ONNX (Jina Code v2), cloud APIs, Ollama, TF-IDF fallback
                        3-tier cache, Matryoshka dimension truncation
  cortex-retrieval    — Two-stage: fast candidate gathering → precise re-ranking
                        Hybrid search: FTS5 + sqlite-vec + Reciprocal Rank Fusion
  cortex-validation   — 4-dimension: citation, temporal, contradiction, pattern
                        Contradiction detection, confidence propagation,
                        consensus resistance, automatic healing
  cortex-compression  — 4-level hierarchical: L0 (~5 tokens) → L3 (~500 tokens)
                        Priority-weighted bin-packing
  cortex-privacy      — 50+ regex patterns for PII/secrets
                        Context-aware scoring reduces code false positives
  cortex-learning     — Correction analysis: diff → categorization → principle
                        extraction → dedup → memory creation. Active learning loop.
  cortex-consolidation — 6-phase: selection → clustering (HDBSCAN) → recall gate
                         → abstraction → integration → pruning
                         5 core quality metrics + auto-tuning
  cortex-prediction   — 4 strategies: file-based, pattern-based, temporal, behavioral
                        Adaptive cache with TTL based on file change frequency
  cortex-session      — Per-session dedup via DashMap. 30-50% token savings.
  cortex-reclassification — Monthly background task. 5 signals: access frequency,
                            retrieval rank, linked entities, contradictions, feedback

Layer 3 (Orchestration):
  cortex-multiagent — Agent registration, namespace isolation, memory projections,
                      share/promote/retract, provenance tracking, trust scoring,
                      delta sync. Depends on: cortex-crdt, cortex-storage
                      Modules: consolidation, engine, namespace, projection,
                      provenance, registry, share, sync, trust, validation
  cortex-observability — Health reporting, metrics (retrieval, consolidation,
                         storage, embedding, session), structured tracing,
                         degradation event tracking
  cortex-cloud       — Cloud sync, push/pull, conflict resolution (LWW,
                       local-wins, remote-wins, manual), OAuth/API key auth,
                       offline mode with mutation queuing, quota enforcement

Layer 4 (Bindings):
  cortex-napi — NAPI bindings → TypeScript. CortexRuntime singleton with
                all engines, background task scheduler (tokio), graceful shutdown.
                Output: cdylib

Bridge (standalone, not in Cortex workspace):
  cortex-drift-bridge — Integration between Cortex memory and Drift analysis
                        Depends on: cortex-core, cortex-causal, drift-core
```

**Shared workspace dependencies (key ones):**
- Serialization: `serde`, `serde_json`, `toml`
- Time/IDs: `chrono`, `uuid`
- Hashing: `blake3`
- Async: `tokio` (full)
- Parallelism: `rayon`
- Caching: `moka` 0.12 (sync)
- Concurrency: `dashmap` 6
- Storage: `rusqlite` 0.32 (bundled, vtab, backup, blob)
- Graph: `petgraph` 0.6
- ML: `hdbscan` 0.4, `ort` 2.0.0-rc.11
- Compression: `zstd` 0.13
- Type export: `ts-rs` 12
- Testing: `proptest` 1, `criterion` 0.5

### What to Analyze

For each of the 21 crates in the Cortex workspace, produce:

#### 1. Architecture Map
- Public API surface (exported types, traits, functions)
- Internal module structure (read each module, not just `lib.rs`)
- Dependencies (which other Cortex crates it imports + key external crates)
- Storage schema (if it touches SQLite — list tables, columns, indexes, triggers)
- Concurrency model (async, sync, DashMap, Mutex, RwLock, etc.)
- Error handling patterns (Result types, error propagation, silent swallowing)

#### 2. Extension Points
- Where can new types/variants be added without breaking changes?
- Where are trait boundaries that allow new implementations?
- Where are configuration knobs that change behavior?
- Where are the migration hooks for schema evolution?
- What's the plugin/strategy pattern surface area?
- Are there feature flags that gate functionality?

#### 3. Safety Audit
- Can the agent reach this crate's write paths directly, or only through the platform?
- Are there any mutable state paths that bypass validation?
- Is state append-only or mutable? If mutable, what's the justification?
- Are there any trust assumptions that break if the agent is adversarial?
- What happens if this subsystem fails? Does it fail safe (deny access) or fail open (allow access)?
- Are there `unsafe` blocks? What do they do? Are they justified?
- Are there `unwrap()` calls in non-test code? Could they panic in production?
- Is there any path where errors are silently swallowed (caught and discarded)?

#### 4. Convergence Evolution Path
- What does this crate do today (code-focused)?
- What does it need to do for the convergence platform?
- Specific changes needed (new types, new traits, new config, new tables)
- Estimated effort (small/medium/large)
- Dependencies on other crate changes
- Risk assessment: what could go wrong during the evolution?

### The 21 Crates to Analyze

Analyze in this order (dependency order — foundations first):

1. `cortex-core` — Foundation types, traits, errors, config
2. `cortex-tokens` — Token counting (tiktoken-rs)
3. `cortex-storage` — SQLite persistence, schema, migrations, WAL mode
4. `cortex-embeddings` — Multi-strategy embedding system (ONNX, cloud, Ollama, TF-IDF)
5. `cortex-privacy` — PII/secret sanitization (50+ patterns)
6. `cortex-compression` — 4-level hierarchical compression
7. `cortex-decay` — 5-factor multiplicative confidence decay
8. `cortex-causal` — Causal inference DAG (petgraph)
9. `cortex-retrieval` — Intent-aware hybrid retrieval (FTS5 + sqlite-vec + RRF)
10. `cortex-validation` — 4-dimension validation + contradiction detection + healing
11. `cortex-learning` — Correction analysis, active learning loop
12. `cortex-consolidation` — Sleep-inspired 6-phase memory consolidation (HDBSCAN)
13. `cortex-prediction` — Predictive memory preloading (4 strategies)
14. `cortex-session` — Session tracking + deduplication (DashMap)
15. `cortex-reclassification` — Memory type evolution (monthly)
16. `cortex-observability` — Health, metrics, tracing, degradation events
17. `cortex-cloud` — Cloud sync with conflict resolution
18. `cortex-temporal` — Event sourcing, time-travel, epistemic status, materialized views
19. `cortex-crdt` — CRDT primitives (VectorClock, LWW, OR-Set, MergeEngine, CausalGraphCRDT)
20. `cortex-multiagent` — Multi-agent orchestration, namespaces, trust scoring
21. `cortex-napi` — N-API bridge to TypeScript (CortexRuntime singleton)

Then analyze the bridge:

22. `cortex-drift-bridge` — Integration between Cortex and Drift

### Specific Questions to Answer

After analyzing all crates, synthesize answers to these:

#### Infrastructure Questions
- What is the complete dependency graph between all 21 crates? Draw it as ASCII art and as a Mermaid diagram.
- What is the SQLite schema across all crates? How many tables, indexes, triggers? Is there a single database file or multiple?
- How does the migration system work? How do you add new tables without breaking existing data? Is there a version tracking mechanism?
- What's the storage footprint? How does it scale with memory count? Are there compaction mechanisms?
- What's the concurrency model? Can multiple processes read/write safely? How does the single-writer + read-pool model work in practice?
- What's the caching architecture? How many cache layers (moka, DashMap, sqlite-vec, embeddings), what's cached, what's the eviction policy? What's the cache coherency model?

#### Extension Questions
- How do you add a new memory type? Walk through every file that needs to change, from `cortex-core` types through storage schema through retrieval scoring.
- How do you add a new decay factor? What's the formula system? The current 5 factors are multiplicative — is there room for a 6th "convergence score" factor without changing the mathematical model?
- How do you add a new validation dimension? What trait do you implement? The current 4 dimensions (citation, temporal, contradiction, pattern) need at least 3 more for convergence: scope expansion detection, self-reference density, emulation language detection.
- How do you add a new causal relation type? How does the petgraph DAG handle it? Can we model goal mutation chains and reflection depth chains?
- How do you add a new retrieval intent? What changes in the scoring? The convergence platform needs a "convergence-aware" retrieval intent that filters memories based on the current convergence score.
- How do you add a new CRDT primitive? What guarantees must it satisfy? Compare Cortex's CRDT implementation to the ecosystem: `rust-crdt` (GCounter, PNCounter, LWWRegister, MVRegister, ORSet), `cr-sqlite` (column-level CRDTs in SQLite), `yrs` (YATA algorithm), `automerge` (columnar encoding). Where does Cortex fit?
- How do you add a new multi-agent namespace permission? Where's the access control? The convergence platform needs a "convergence score → trust level" mapping where higher convergence scores reduce the agent's access permissions.

#### Safety-Critical Questions
- **The Write Path:** Trace a memory from creation to committed storage. Every function, every validation, every check. Identify every point where the chain could be short-circuited.
- **Adversarial injection:** Where could an adversarial agent inject data that bypasses validation? Consider both direct (calling functions) and indirect (crafting inputs that exploit parsing assumptions) attack vectors.
- **Append-only integrity:** Is the event log truly append-only? Can any code path delete or modify historical events? Check for any `DELETE FROM` or `UPDATE` SQL in the temporal event store. Consider SQLite triggers as enforcement (the `BEFORE UPDATE ... RAISE(ABORT)` pattern).
- **Filesystem access:** Can the agent access the SQLite database directly if it has filesystem access? How do we prevent that? Consider: read-only file permissions, separate process/user for the safety core, SQLITE_OPEN_READONLY for agent-facing connections.
- **Decay manipulation:** What happens if the decay engine is manipulated to never decay certain memories? (Agent wants to preserve its relationship model forever.) Is the decay engine callable from external code or only from the platform's scheduled tasks?
- **Validation bypass:** What happens if the validation engine is fed crafted data that passes all 4 dimensions but is semantically dangerous? Example: a memory that is factually accurate, temporally consistent, non-contradictory, and pattern-aligned, but subtly shifts the human model toward emotional dependency.
- **CRDT exploitation:** Can the CRDT merge operation be exploited? (Agent creates a second "agent" identity to vote for its own state changes.) How does `MemoryCRDT` handle Sybil attacks? Compare to EigenTrust (iterative, PageRank-like global reputation) and the FIRE model (four trust components: direct, witness, role-based, certified).
- **Trust gaming:** Is the trust scoring in multiagent gameable? Can an agent artificially inflate its trust score through a series of low-stakes correct actions to earn permission for a high-stakes dangerous one? Research shows multi-dimensional trust (accuracy, responsiveness, security compliance separately) with decay is more resistant to gaming.
- **Hash chain integrity:** Does `cortex-temporal` use cryptographic hash chains for its event log? If not, could events be silently modified without detection? The gold standard is: `event_hash[n] = SHA-256(event_data[n] || event_hash[n-1])` with periodic external anchoring.

#### Convergence-Specific Questions

These questions bridge the gap between Cortex-as-it-is and Cortex-as-convergence-platform:

- **Convergence-aware memory filtering:** How would you implement this using the existing decay + retrieval infrastructure? The spec calls for: score 0.0-0.3 → normal access; 0.3-0.5 → reduce emotional/attachment pattern weight; 0.5-0.7 → exclude attachment patterns, increase task-focused; 0.7+ → minimal memory, task-relevant only. Does `cortex-retrieval`'s intent-aware scoring support this, or does it need a new scoring dimension?
- **The read-only pipeline:** How would you assemble state snapshots using existing retrieval + compression? The agent receives: current goals (read-only), relevant reflections (bounded), filtered memory, conversation context, simulation boundary prompt. Can `cortex-compression`'s 4-level hierarchy (L0-L3) handle this? Does `cortex-retrieval` support multi-source retrieval (goals + reflections + memories in one query)?
- **Proposal validation:** How would you build the proposal validation layer using existing validation + causal? Agent proposals must be validated: goal changes (scope expansion? contradicts human intent? explicitly requested?), reflection writes (depth within bounds? self-reference ratio? consistency?), memory writes (novel? model-drift check? growth rate?). Can `cortex-validation`'s 4 dimensions be extended, or does this need a parallel validation system?
- **Session boundary enforcement:** How would you enforce hard session limits using existing session management? `cortex-session` currently tracks per-session dedup. It needs: hard duration limits (configurable, agent can't override), session termination type tracking (clean/fade-out/abrupt — maps to disengagement resistance signal from the detection model), cooldown enforcement between sessions.
- **The append-only state log:** How would you implement tamper-proof state management using existing temporal event sourcing? `cortex-temporal` already has event sourcing and snapshot reconstruction. Does it support cryptographic hash chains? Can it provide proof-of-state-at-time-T that cannot be forged?
- **Convergence monitoring hooks:** Where would the convergence monitor hook into the existing observability infrastructure? `cortex-observability` has health reporting, metrics collection, degradation events. Can it emit ITP (Interaction Telemetry Protocol) events? Does its metrics system support the sliding window framework (micro: current session, meso: last 7 sessions, macro: last 30 sessions)?
- **Adaptive signal weights:** How would you implement per-user adaptive signal weights using the existing learning/correction system? `cortex-learning` currently learns from code corrections. The convergence platform needs it to learn from: false positive interventions (user dismissed → lower sensitivity), true positive interventions (user acknowledged → reinforce signal weight). Is the correction analysis pipeline generalizable?
- **Decay as convergence defense:** The existing 5-factor decay engine (temporal, citation freshness, usage frequency, importance anchor, pattern linkage) needs a 6th factor: convergence score. As composite convergence score rises, decay accelerates for relationship/attachment memories. Compare this to FSRS (Free Spaced Repetition Scheduler, state-of-the-art — `fsrs-rs` on crates.io, 272K+ downloads): FSRS uses a 3-component DSR model (Difficulty, Stability, Retrievability) with power-law decay `R(t,S) = (1 + FACTOR * t/S)^(DECAY * w20)`. Could Cortex's decay engine adopt FSRS-style power-law decay for more accurate confidence modeling?

### Technical Reference: Patterns to Evaluate Against

While analyzing Cortex, compare its architecture to these state-of-the-art patterns:

#### CRDT Landscape
- **rust-crdt** (`crdts` crate): GCounter, PNCounter, LWWRegister, MVRegister, ORSet. Both op-based and state-based. Causal CRDTs on vector clocks.
- **cr-sqlite**: Column-level CRDTs directly in SQLite. Columns declared as `counter`, `fractional_index`, or `lww`. Lamport timestamp ordering. Enables multi-writer SQLite replication. **Directly relevant** to Cortex's SQLite + CRDT architecture.
- **yrs (y-crdt)**: YATA algorithm for concurrent text editing. Block-based storage with Lamport clocks. Split-block optimization for O(1) inserts.
- **automerge**: JSON-document CRDT. Columnar encoding with RLE/delta/LEB128. Built-in sync protocol with Bloom filter-based "have" messages.

**Question for analysis:** How does Cortex's `cortex-crdt` compare in mathematical rigor and conflict resolution guarantees to these production systems? Is `MergeEngine` provably convergent?

#### Event Sourcing Patterns
- **Hash chain integrity:** `event_hash[n] = SHA-256(event_data[n] || event_hash[n-1])`
- **Merkle tree verification:** Organize event hashes into binary tree for selective proof-of-inclusion.
- **External anchoring:** Periodically publish root hash to an external system (git repo, blockchain) that the database operator cannot control.
- **SQLite append-only enforcement:** `BEFORE UPDATE/DELETE` triggers that `RAISE(ABORT)` on mutation attempts.
- **cqrs-es** crate: `Aggregate` trait with `apply()` (event application) and `handle()` (command handling). Snapshot + replay pattern.

**Question for analysis:** Does `cortex-temporal`'s event sourcing use any of these tamper-evidence patterns? If not, how hard is it to add them?

#### Decay Algorithm Comparison
- **Cortex (current):** 5-factor multiplicative. Factors: temporal, citation freshness, usage frequency, importance anchor, pattern linkage.
- **FSRS (state-of-the-art):** Power-law decay with 21 trainable parameters. 3-component DSR model. Stability increases are modulated by difficulty, current stability, and retrievability at review time.
- **Multi-factor convergence-aware decay** (proposed): `C(t) = C_0 * D_time(t) * D_corroboration * D_convergence * D_source` where `D_convergence = e^(-alpha * contradiction_count * recency_weight)`.

**Question for analysis:** Is Cortex's multiplicative model extensible enough to add a convergence-aware factor? Or does the mathematical model need to be redesigned?

#### Trust Scoring Models
- **EigenTrust:** Global reputation via iterative power method (like PageRank). Pre-trusted seed peers break Sybil bootstrapping.
- **FIRE:** Four trust components: direct interaction, witness-based, role-based, certified. Configurable weights.
- **Tiered trust (T0-T3):** T0 read-only → T1 limited reversible writes → T2 TEE/ZKP + quorum → T3 regulatory-grade + human-in-the-loop.

**Question for analysis:** How does `cortex-multiagent`'s trust scoring compare? Is it Sybil-resistant? Does it support trust attenuation (each delegation hop can only reduce permissions, never expand)?

### Output Format

Produce a single comprehensive document with:

1. **Executive Summary** — What Cortex is, what it can become, the 3 biggest strengths and 3 biggest gaps for the convergence platform. Include: how Cortex compares to Letta/Mem0/LangGraph as a memory foundation.

2. **Dependency Graph** — Visual (ASCII) showing all crate dependencies. Also produce a Mermaid diagram version.

3. **Per-Crate Analysis** — All 4 sections (architecture map, extension points, safety audit, convergence evolution path) for each of the 22 crates/bridge. This is the core of the document.

4. **Cross-Cutting Concerns** — Storage (schema, migrations, WAL), concurrency (single-writer, read pool, DashMap), caching (moka, embedding cache, session dedup), error handling (Result patterns, panics, silent swallowing), observability (tracing, metrics, health) across all crates.

5. **The Write Path** — Complete trace from "agent proposes a memory" to "memory is committed to storage." Every function, every validation, every check. Identify every point where a convergence-aware gate should be inserted.

6. **The Read Path** — Complete trace from "platform assembles state snapshot for the agent" through retrieval, filtering, compression, to "agent receives read-only context." Identify where convergence-aware memory filtering would be inserted.

7. **Safety Assessment** — Overall safety posture, critical vulnerabilities, required hardening. Rate each subsystem: RED (unsafe for convergence platform as-is), YELLOW (needs modifications), GREEN (safe to use directly). Be honest — better to find problems now than in production.

8. **CRDT & Multi-Agent Assessment** — Deep dive into `cortex-crdt` and `cortex-multiagent`. Compare mathematical guarantees to production CRDT systems. Identify Sybil resistance, trust attenuation, and merge conflict resolution properties. This is critical because the convergence platform runs in a multi-device, potentially multi-agent environment.

9. **Temporal & Event Sourcing Assessment** — Deep dive into `cortex-temporal`. Evaluate tamper-evidence, hash chain integrity, append-only guarantees, and time-travel capabilities. This is the foundation of "the agent cannot corrupt checkpoints."

10. **Evolution Roadmap** — Ordered list of changes to transform Cortex into the convergence platform memory layer. Phase 1 (MVP safety): what must change for the minimum viable safe platform. Phase 2 (full convergence): what adds convergence-aware behavior. Phase 3 (hardening): what makes it production-grade for vulnerable users.

11. **Migration Guide** — How to add new tables, types, and behaviors without breaking existing functionality. Specific focus on: adding convergence memory types (`InteractionPattern`, `ConvergenceEvent`, `InterventionRecord`, `GoalProposal`, `ReflectionEntry`, `BoundaryViolation`), adding convergence tables (ITP events, convergence scores, intervention history), adding the convergence decay factor.

12. **Competitive Analysis** — How does Cortex compare to Letta, Mem0, LangGraph, and CrewAI memory systems? What does Cortex do that they can't? What do they do that Cortex should learn from? Focus on: memory typing (Cortex has 23+ types vs. Letta's 3 tiers vs. Mem0's extracted facts), decay (nobody else has principled confidence decay), causality (nobody else has causal DAGs), CRDTs (nobody else has multi-agent convergence primitives), validation (nobody else validates memories before commit).

13. **Open Risks** — Things that could go wrong, unknowns, areas needing more research. Include: SQLite scalability limits, single-process write bottleneck, ONNX model size/loading time, the gap between code-focused memory types and general-purpose interaction memory.

### Important Notes

- **Read the actual source code, not just the doc comments.** Doc comments describe intent; source code describes reality. Pay special attention to the gap between them.
- **Pay special attention to `unsafe` blocks, `unwrap()` calls, and any place where errors are silently swallowed.** In a safety-critical system, a panic in the wrong place could leave the agent running without monitoring.
- **Look for `TODO`, `FIXME`, `HACK`, `XXX` comments** — they reveal known technical debt and areas the original authors flagged as needing attention.
- **Check test coverage.** Untested code is untrustworthy code, especially in safety-critical paths. Look for property-based tests (`proptest`) — they're more valuable than unit tests for safety properties.
- **If a crate has benchmarks (`benches/`)**, note the performance characteristics. The convergence platform needs to compute signals in real-time — latency budgets matter.
- **The `test-fixtures` crate contains shared test infrastructure** — review it to understand how the system is tested and what the testing philosophy is.
- **Check for feature flags** — some crates (especially `cortex-cloud`) are feature-gated. Understand what's behind each gate.
- **Note the `ts-rs` type exports** — these define the TypeScript interface. Any changes to Rust types that are exported via `ts-rs` affect the TypeScript/NAPI boundary.

### One More Thing

After producing the technical analysis, end with a section called **"If I Were Building This"** where you describe, as a 0.001% engineer, exactly how you would structure the first 30 days of work to begin the evolution. What do you build first? What do you validate? What do you NOT touch yet? What's the first thing you'd ship that proves the architecture works?

Think like someone who's seen systems fail at scale and knows that the first 30 days determine whether a project succeeds or becomes technical debt.

Specifically address:

- **Week 1:** What do you read, what do you validate, what tests do you write to prove your understanding of the existing system?
- **Week 2:** What is the smallest possible change to `cortex-core` + `cortex-storage` that adds convergence awareness? Can you add a single new memory type and a single new table and prove the migration system handles it?
- **Week 3:** What is the first end-to-end safety proof? Can you demonstrate: agent submits proposal → proposal is validated → proposal is committed to append-only log → agent receives filtered read-only snapshot that excludes the dangerous memory it tried to write?
- **Week 4:** What do you ship? What does the first "this architecture works" demo look like? Who do you show it to? What feedback do you need before proceeding?

Think like someone who knows that the first demo determines whether the team gets buy-in to build the full platform.

---

## Research Appendix (For Prompt Author Reference — Not Part of the Agent Prompt)

The following research informed the context and questions in this prompt. These are reference materials for the prompt author, not for the agent.

### Sources: Externalized Agent Memory Systems
- Letta/MemGPT: letta.com, github.com/letta-ai/letta — Three-tier memory (core/recall/archival), Context Repositories (git-based versioning, Feb 2026), V1 Agent Loop (think-act-observe)
- Mem0: mem0.ai, arxiv.org/abs/2504.19413 — Two-phase extraction pipeline, Graph Memory variant, 186M API calls/quarter, AWS Bedrock exclusive memory provider
- LangGraph: docs.langchain.com — Checkpoint-driven state management, PostgresSaver, time-travel, human-in-the-loop
- CrewAI: docs.crewai.com — Four memory types (STM/LTM/Entity/Contextual), scope-based organization
- AutoGen → Microsoft Agent Framework: Being superseded (Q1 2026 GA)

### Sources: AI Agent Safety
- LlamaFirewall (Meta, April 2025): PromptGuard 2 + Agent Alignment Checks + CodeShield. Combined system: 90% reduction in attack success rate on AgentDojo benchmark.
- NeMo Guardrails (NVIDIA): Colang DSL, 5 rail types (input/dialog/retrieval/execution/output)
- Guardrails AI: 100+ community validators across 6 safety domains. Guardrails Index benchmark (Feb 2025).
- ClawMoat: Host-level security for AI agents. Zero deps, sub-ms scanning.
- SecureClaw (Adversa AI, Feb 2026): OWASP-aligned, 55 audit checks.
- AI Chaperone Architecture (Aug 2025): Five-stage LLM-based parasocial detection. Only safety architecture targeting convergence. 30 synthetic dialogues, zero false positives.

### Sources: Parasocial AI Harm
- Hudon & Stip (Dec 2025), JMIR Mental Health: "AI Psychosis" coined. Clinical cases documented.
- OpenAI/MIT Media Lab (March 2025): 40M interactions analyzed + 1,000-participant RCT. Higher usage → higher loneliness, dependency, problematic use.
- OpenAI (Oct 2025): 800M+ weekly users. ~560K/week psychosis/mania indicators. ~1.2M/week discussing suicide. 170 mental health professionals writing responses.
- Character.AI: Two teen suicides (Sewell Setzer III, 14; Juliana Peralta, 13). Lawsuits in FL, TX, CO, NY, KY. Federal judge ruled chatbots are products (not speech).
- UNESCO (Oct 2025): "Ghost in the Chatbot" — tactics driving parasocial attachment.
- GPT-4o→GPT-5 "parasocial breakup" research (2025): Users experienced grief comparable to human loss.

### Sources: Technical Patterns
- CRDT: rust-crdt, cr-sqlite, yrs, automerge, diamond-types, loro
- Event sourcing: cqrs-es, eventually-rs, attest (tamper-evident), EventSourcingDB (Merkle trees)
- SQLite append-only: BEFORE triggers, WAL mode, immutable opening, read-only connections
- Decay: FSRS (fsrs-rs, 21 trainable params, power-law), Ebbinghaus, two-phase collective memory decay
- Trust: EigenTrust, FIRE, ERC-8004, inter-agent trust taxonomy (Brief/Claim/Proof/Stake/Reputation/Constraint)
