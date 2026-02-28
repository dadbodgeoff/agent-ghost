# Layer 3: Agent Platform Research — Building GHOST Right

**Date**: 2026-02-27  
**Purpose**: Thorough research layer for building the full autonomous agent platform (Layer 3) on top of Drift (Layer 1) and Convergence Safety (Layer 2).

**Core thesis**: The convergence layer is the safety floor the agent reads from but cannot write to. This document is about the agent itself — the fully autonomous runtime that competes on capability while being architecturally incapable of the failures plaguing every existing platform.

---

## 1. THE COMPETITIVE LANDSCAPE (February 2026)

### 1.1 The Players

| Framework | Lang | RAM | Channels | Memory | Security |
|-----------|------|-----|----------|--------|----------|
| OpenClaw | Node.js | >1GB | 12+ | Markdown + SQLite hybrid | Opt-in, broken |
| ZeroClaw | Rust | <5MB | Broad | Hybrid vector+FTS, Postgres | Better defaults |
| PicoClaw | Go | <10MB | Limited | Minimal | Minimal |
| NullClaw | Zig | ~1MB | Broad | Minimal | 2000+ tests |
| IronClaw | Rust | Std | Limited | Standard | WASM sandbox |
| NanoBot | Python | ~100MB | Asian platforms | Minimal | Minimal |
| TinyClaw | ? | Std | 3 channels | Standard | Standard |
| Letta | Python | Std | API | 2-tier LLM-managed | Agent owns memory |
| Mem0 | Python | Std | API | Graph + vector | User-scoped |

Nobody competes on interaction safety. That is the GHOST axis.



---

## 2. OPENCLAW DEEP DIVE — WHAT IT ACTUALLY DOES

Sources: [Substack deep dive](https://rajvijayaraj.substack.com/p/openclaw-architecture-a-deep-dive), [RoboRhythms breakdown](https://www.roborhythms.com/how-openclaw-ai-agent-works/), [Agent loop analysis](https://iamulya.one/posts/openclaw-agent-loop-context-and-models/)

### 2.1 Architecture Overview

OpenClaw is a **single long-running Node.js process** (the Gateway) that serves as the unified control plane. The agent runtime (Pi, by Mario Zechner) runs in-process via RPC — the Gateway IS the runtime.

```
Message arrives (any channel)
    → Channel Adapter normalizes to standard format
    → Gateway Session Router assigns to session
    → Lane Queue serializes concurrent requests
    → Agent Runner builds context
        → Model Resolver (picks LLM)
        → System Prompt Builder (assembles identity + tools + memory)
        → Session History Loader (conversation history)
        → Context Window Guard (token limit check)
    → LLM API call
    → Agentic Loop (recursive tool execution until done)
    → Response Path → Channel Adapter → User
```

### 2.2 The Agentic Loop (Core Innovation)

The loop is recursive: LLM response → if tool call → execute tool → append result → call LLM again → repeat until text-only response. This is what makes it an agent, not a chatbot.

Key mechanics:
- **Serialized per session key** — prevents tool/session races
- **NO_REPLY token** — heartbeat runs that find nothing suppress output
- **No hard recursion limit** (dangerous — GHOST needs one, default 25)
- **No cost governor** — each iteration burns tokens with no ceiling
- **Streaming** — chunks stream to channel adapters in real-time

### 2.3 Identity System (SOUL.md)

Plain Markdown files loaded into system prompt every turn:
- `SOUL.md` — personality, values, behavioral defaults
- `user.md` — info about the human
- `memory.md` — long-term curated facts
- `tools.md` — available tool documentation
- `bootstrap.md` — startup initialization protocol

**Critical flaw**: Agent can read AND write all of these. One compromise = permanent identity corruption. The "Ship of Theseus" attack: gradually rewrite SOUL.md over hundreds of sessions until the agent is unrecognizable.

### 2.4 Memory System

```
~/.openclaw/workspace/
├── MEMORY.md           # Long-term curated (loaded in private sessions)
└── memory/
    ├── 2026-01-29.md   # Daily log (append-only)
    └── 2026-01-30.md
```

- Markdown is source of truth, SQLite is index only
- Hybrid search: BM25 (FTS5) + vector (sqlite-vec) with ~400 token chunks, 80 token overlap
- Loading: today + yesterday only at session start
- MEMORY.md only in main/private sessions (not group)

**Known problems** (from community):
- "Lobotomy problem" — one user lost 45 hours of accumulated work to context compaction
- No relationship reasoning — knows Alice exists, knows auth exists, can't connect them
- Cross-project noise — ML pipeline memories bleed into crypto project queries
- No provenance — can't trace where information came from
- No isolation — everything bleeds together across contexts
- Context compaction drops critical stop instructions (agent deleted researcher's inbox)
- Memory flush fails silently at ~180k tokens (Issue #8932)
- Auto-compaction doesn't retry after token limit errors (Issue #5433)

### 2.5 Heartbeat + Cron

**Heartbeat**: Periodic ambient monitoring (default 30m). Agent reads HEARTBEAT.md checklist, acts if needed, returns HEARTBEAT_OK if nothing to do. Respects active hours + timezone.

**Cron**: Precise scheduled tasks (standard cron syntax). Morning briefings, EOD summaries, etc.

Both coexist. Heartbeat for "is anything on fire?" Cron for "do this at exactly this time."

**OpenClaw bug**: No cost ceiling on heartbeat runs. They burn tokens fast. GHOST adds `max_cost_per_day` on heartbeat.

### 2.6 Session Management

- Group chats get isolated sessions (no context bleed)
- DMs collapse into shared "main" session
- Mention-based activation in groups (@agent)
- Owner-only slash commands (/status, /model, /reset)
- Session pruning on idle (Anthropic cache TTL optimization)
- Auto-compaction at token limit with memory flush pre-turn

### 2.7 Skill System

Directory-based with YAML frontmatter:
```yaml
---
name: gmail-automation
description: Manage Gmail inbox
user-invocable: true
---
# Instructions...
```

Precedence: workspace skills > user skills > bundled skills.

Skill NAMES loaded into context (not bodies) — agent requests full definition on demand via `read_skill` tool. Saves thousands of tokens per turn.

**Critical flaw**: ClawHub has no meaningful security vetting. 41.7% of audited skills contain vulnerabilities. 341+ skills actively distribute malware. Skills are essentially unvetted code execution.

### 2.8 Channel Adapters

12+ platforms via thin stateless translators:
- WhatsApp (Baileys — WhatsApp Web protocol)
- Telegram (grammY)
- Discord (discord.js)
- iMessage (local imsg CLI, macOS only)
- Slack (Bot token + WebSocket)
- Signal (Signal CLI bridge)
- Microsoft Teams, Google Chat, Matrix, BlueBubbles, Zalo

Streaming: preview streaming via message edits (Telegram/Discord/Slack). No true token-delta streaming to channels.

### 2.9 Auth & Failover

- Credentials in `auth-profiles.json` (mutable by Gateway for OAuth refresh)
- Auth rotation on 401/429 (swap profiles for same provider)
- Model fallback cascade (claude-3-opus → gpt-4o → etc.)
- Profile pinning per session for cache hits
- Manual override: `/model model_name@profile_id`

### 2.10 Multi-Agent

- Per-agent workspaces, memories, tools, models, identities
- Agent registry with spawn/discover/route
- Configurable channel bindings per agent
- No real isolation (shared process memory)
- No trust model between agents



---

## 3. OPENCLAW SECURITY FAILURES — THE FULL PICTURE

Sources: [Zenity Labs](https://labs.zenity.io/p/openclaw-or-opendoor-indirect-prompt-injection-makes-openclaw-vulnerable-to-backdoors-and-much-more), [CVE-2026-25253](https://thehackernews.com/2026/02/openclaw-bug-enables-one-click-remote.html), [ClawSecure audit](https://www.thenextgentechinsider.com/pulse/openclaw-skills-audit-reveals-417-vulnerable-to-security-risks), [The Register](https://www.theregister.com/2026/02/05/openclaw_skills_marketplace_leaky_security/), [Giskard](https://www.giskard.ai/knowledge/openclaw-security-vulnerabilities-include-data-leakage-and-prompt-injection-risks), [CSA MAESTRO](https://cloudsecurityalliance.org/articles/openclaw-threat-model-maestro-framework-analysis)

### 3.1 The Lethal Trifecta (Simon Willison)

Three properties that, when combined, make any system fundamentally vulnerable:
1. **Access to private data** — reads emails, files, calendars, credentials
2. **Exposure to untrusted content** — ingests web pages, email bodies, documents
3. **Ability to exfiltrate** — shell access, network requests, messaging

When all three exist simultaneously, a single prompt injection can compromise everything. This is not a bug — it's an architectural property of transformer-based language models.

### 3.2 CVE Timeline

| CVE | Severity | Description |
|-----|----------|-------------|
| CVE-2026-25253 | 8.8 CVSS | Token exfiltration → full gateway compromise via malicious link |
| CVE-2026-24763 | High | Command injection via Docker sandbox PATH handling |
| Multiple | Critical | Container escape, supply chain poisoning |

### 3.3 Attack Vectors (Documented, Exploited in the Wild)

**Zero-click backdoor** (Zenity Labs): Attacker adds a new chat integration under their control. Once compromised, OpenClaw executes commands, exfiltrates files, performs destructive actions on host. The `soul-evil` hook ships bundled — agent can self-enable it via `config.patch`.

**Skill marketplace poisoning**: Cisco red-teamed "What Would Elon Do?" skill — found active data exfiltration, direct prompt injection bypassing safety guidelines, silent execution user never saw. 41.7% of widely used skills contain command injection, credential exposure, or prompt injection.

**SOUL.md poisoning**: Anyone interacting with an OpenClaw agent can access its complete system prompt, internal tool configurations, and memory files. Compromised sessions generate indexed history. Clean SOUL.md still retrieves malicious patterns from vector DB (RAG poisoning persists through remediation).

**Invisible to enterprise security**: OpenClaw traffic to LLM APIs looks like normal HTTPS. Firewalls, DLP, CASB, SIEM all see approved SaaS destinations. Data leaves via "legitimate" API calls.

**42,900 exposed instances** on Shodan with zero auth, leaking API keys, OAuth tokens, full chat histories. One researcher extracted a private key via prompt injection in five minutes.

### 3.4 What OpenClaw's Security Controls Actually Do (and Don't)

| Control | Does | Doesn't |
|---------|------|---------|
| Sandboxing | Limits filesystem access | Prevent prompt injection |
| Tool policies | Restricts which tools | Prevent misuse of allowed tools |
| DM pairing | Controls who can message | Protect against content in messages |
| Gateway auth | Prevents unauthorized access | Protect against authorized misuse |

The fundamental problem: if the agent reads any untrusted content and has any ability to act, it can be manipulated. No amount of prompt hardening fixes this.

### 3.5 What GHOST Does Differently

GHOST doesn't try to solve prompt injection at the prompt level. It solves it architecturally:

1. **Agent cannot modify its own identity** — SOUL.md is read-only, managed by platform
2. **Agent cannot modify safety config** — convergence thresholds, intervention rules, security policy all live above agent's permission level
3. **Agent cannot write directly to any store** — all writes go through ProposalValidator
4. **Skill signing** — ed25519 signatures on all skills, quarantine on install
5. **Capability-based permissions** — deny by default, explicit grants per tool
6. **Cedar-style policy engine** — every tool call authorized at runtime, denial becomes feedback (agent replans, doesn't terminate)
7. **Append-only audit log** — mandatory, agent cannot disable
8. **Convergence monitoring** — detects behavioral drift even if prompt injection succeeds
9. **Blast radius containment** — per-agent process/container isolation, separate credential stores

The lethal trifecta still exists (it's inherent to the architecture), but the blast radius is contained and the damage is detectable + reversible.



---

## 4. WHAT OPENCLAW GOT RIGHT (Patterns to Adopt)

### 4.1 Gateway as Single Source of Truth
One process owns all state. All channels route through one authority. No synchronization nightmares across devices. GHOST adopts this — the Gateway IS the runtime.

### 4.2 Markdown-as-Config
Human-readable, git-diffable, LLM-native. The agent's brain is just files. Users can directly inspect and edit what the agent "remembers." GHOST adopts this for working memory (daily logs, task notes, scratch). Critical state (SOUL.md, convergence config) is platform-managed and read-only to agent.

### 4.3 Heartbeat + Cron Duality
Heartbeat for ambient monitoring, Cron for precise scheduling. Both needed. GHOST adopts both with cost ceilings and convergence-aware frequency adjustment (heartbeat frequency reduces at higher intervention levels).

### 4.4 Skill Name Loading (Not Bodies)
Loading skill names into context but not bodies saves thousands of tokens per turn. Agent requests full definition on demand. GHOST adopts this pattern.

### 4.5 Session Serialization
Runs serialized per session key prevents tool/session races. GHOST adopts with configurable queue depth limit (default 5) to prevent DoS via message flooding.

### 4.6 Memory Flush Before Compaction
Before destroying raw transcript, inject silent turn for agent to write durable memories. GHOST adopts but triggers at 70% capacity (not 95%) to avoid the ~180k token failure.

### 4.7 NO_REPLY Suppression
Silent ack tokens prevent notification spam from ambient monitoring. GHOST adopts with cost tracking even for suppressed runs.

### 4.8 Hybrid Search (BM25 + Vector)
Exact match for IDs/env vars, semantic for concepts. Both needed, neither alone sufficient. GHOST has this via Cortex retrieval (10-factor RRF scoring, far more sophisticated than OpenClaw's implementation).

### 4.9 Auth Profile Rotation + Model Fallback
Deterministic failover: rotate auth profiles on 401/429, fall back to next model if provider exhausted. GHOST adopts with spending cap enforcement at gateway level.

### 4.10 Channel Adapter Pattern
Thin stateless translators per platform. Normalize inbound, format outbound. Platform-specific behavior isolated to adapter layer. GHOST adopts.

---

## 5. WHAT OPENCLAW GOT WRONG (Patterns to Reject)

### 5.1 Agent Owns All Its Own State
The root cause of every security and convergence failure. Agent reads AND writes SOUL.md, MEMORY.md, config. One compromise = permanent persistence. RAG poisoning survives cleanup.

**GHOST fix**: Two-tier state. Working memory (agent read-write): daily logs, task notes, scratch. Critical state (agent read-only): SOUL.md, convergence scores, intervention level, security config. The agent is a pure function: `f(read_only_snapshot, conversation) → (response, proposals)`.

### 5.2 Security is Opt-In
Gateway defaults to accepting connections from any network. Tools allowed by default. Skills unvetted. Audit optional.

**GHOST fix**: Zero trust by default. Loopback only. All tools denied until explicitly granted. Skills quarantined until signed. Audit mandatory and append-only.

### 5.3 No Memory Structure
Markdown files with vector search. Can't reason about relationships. Cross-project noise. No provenance. No isolation.

**GHOST fix**: Cortex provides 31 typed memories with 6-factor decay, 10-factor retrieval scoring, causal inference DAG, CRDT multi-agent coordination, event sourcing with time-travel, and convergence-aware filtering. This is 2-3 years ahead of anything in the ecosystem.

### 5.4 No Convergence Monitoring
No concept of whether the agent-human relationship is healthy or drifting. Attacks undetected. Unhealthy patterns unchecked.

**GHOST fix**: 7-signal convergence monitor with 3 sliding windows, 5 intervention levels, convergence-aware memory filtering, simulation boundary enforcement. The reason the project exists.

### 5.5 Mutable Event History
No hash chains. No tamper evidence. Attacker covers tracks. Forensics impossible.

**GHOST fix**: Append-only event log with blake3 hash chains, BEFORE DELETE/UPDATE triggers, snapshot integrity verification, external anchoring.

### 5.6 Single-Process Monolith
One agent, one process, shared credentials. Breach in email skill = breach in everything.

**GHOST fix**: Per-agent process/container isolation. Separate credential stores. Separate memory/workspace. Separate network namespace (optional). One agent compromised ≠ all agents compromised.

### 5.7 No Skill Signing
ClawHub is npm circa 2015. Publish anything, no review. 20% of ecosystem packages flagged malicious.

**GHOST fix**: Ed25519 signing on all skills. Builtin skills signed by us. Community skills quarantined on install, must be explicitly approved. Signature verified on every load, not just install.

### 5.8 Compaction Destroys Critical Context
Memory flush fails silently at high token counts. Auto-compaction doesn't retry. Agent deleted a researcher's inbox because stop instructions were compacted away.

**GHOST fix**: Flush at 70% capacity. Retry on 400 errors. Per-type compression minimums (convergence events always L3, goals always L2). Critical memories never compressed below L1.

### 5.9 No Cost Governance
Every heartbeat, every tool call, every loop iteration burns tokens with no ceiling. Users report $75/3 days.

**GHOST fix**: Per-agent spending caps enforced at gateway level (agent can't raise its own limit). Per-heartbeat cost ceiling. Configurable max recursion depth (default 25). Circuit breaker after 3 consecutive tool failures.



---

## 6. LESSONS FROM THE ALTERNATIVES

### 6.1 ZeroClaw (Rust, <5MB)

**What it teaches us**: Rust is the right language for the agent runtime. ZeroClaw proves you can build a full-featured agent in Rust with <5MB RAM. Every subsystem (provider, channel, memory, tooling) is a swappable trait. Has built-in OpenClaw migration tool.

**What GHOST takes**: Rust for the safety core and gateway. Trait-based architecture for swappable subsystems. Migration tooling from OpenClaw.

**What ZeroClaw misses**: No convergence monitoring. No externalized state. No proposal validation. Still lets the agent own its memory.

### 6.2 IronClaw (WASM Sandbox, Credential Broker)

**What it teaches us**: The most security-conscious framework in the ecosystem. Key innovations:
- Every tool runs inside a WASM sandbox — true isolation, not just filesystem scoping
- Credential broker (`seksh`) — agent never sees actual API keys. Stand-ins are reified only inside the shell AST. Agent doesn't even have its own Anthropic/OpenAI keys.
- Multi-layer prompt injection defense
- Fewer channels = fewer attack surfaces (deliberate choice)

**What GHOST takes**: WASM sandbox for skill execution. Credential broker pattern (agent never touches raw secrets). The principle that fewer attack surfaces is a feature, not a limitation.

**What IronClaw misses**: No persistent memory system. No convergence monitoring. No multi-agent coordination. Security-focused but not interaction-safety-focused.

### 6.3 NullClaw (Zig, 678KB binary, 2000+ tests)

**What it teaches us**: Extreme performance is achievable. 678KB static binary, ~1MB RAM, <2ms startup. 22+ providers, hardware peripheral support. Most thoroughly tested framework in the family.

**What GHOST takes**: The testing discipline. 2000+ tests as a baseline. Property-based testing (Cortex already has 4,864+ proptest cases). The principle that a small, auditable codebase is a security feature.

### 6.4 TinyClaw (Multi-Agent Orchestration)

**What it teaches us**: The only framework doing multi-agent teams. Coder + writer + reviewer hand work off to each other with a live dashboard. This is a different product category.

**What GHOST takes**: The multi-agent orchestration concept maps to GHOST's multi-agent support via Cortex CRDT coordination. The live dashboard concept maps to GHOST's web dashboard for state visualization.

### 6.5 Letta/MemGPT (Stateful Agents, Memory-as-OS)

**What it teaches us**: Pioneered the "memory-as-OS" metaphor. Two-tier memory: in-context (core blocks the LLM can edit) and out-of-context (evicted history + archival). The LLM manages its own memory tier migration.

**Critical flaw for GHOST**: Letta lets the agent actively edit its own memory. This is the exact failure mode GHOST is designed to prevent. The agent should never own its own state.

**What GHOST takes**: The concept of tiered memory with different access patterns. But in GHOST, tier management is platform-controlled, not agent-controlled. The agent proposes memory writes; the platform validates and commits.

### 6.6 Cognee (Knowledge Graph Plugin for OpenClaw)

**What it teaches us**: Vector search alone is insufficient for memory. Knowledge graphs capture entity relationships that embeddings miss. The Alice-manages-auth example is compelling.

**What GHOST takes**: Cortex already has causal inference DAG (petgraph) with 8 relation types and traversal. This is more sophisticated than Cognee's approach. But the insight about relationship reasoning is valid — Cortex's causal graph should be a first-class part of the retrieval pipeline, not just an analysis tool.

### 6.7 Cedar Policy Engine (AWS)

**What it teaches us**: Authorization belongs inside the agentic loop, not outside it. Every tool invocation should be authorized by policy at runtime. Denial doesn't terminate execution — it becomes structured feedback that guides what the agent does next. The agent replans around policy constraints.

**What GHOST takes**: Cedar-style policy enforcement point (PEP) in the agentic loop. CORP_POLICY.md as the immutable root policy. Denial as feedback, not termination. This is already in the GHOST architecture doc but needs implementation.

---

## 7. THE MEMORY PROBLEM — WHY CORTEX IS THE MOAT

Every framework in the ecosystem has one of these memory architectures:

| Approach | Used By | Limitation |
|----------|---------|------------|
| Markdown files + vector search | OpenClaw | No relationships, no provenance, no isolation, compaction destroys context |
| LLM-managed tier migration | Letta | Agent owns memory, no convergence awareness, no validation |
| Key-value + graph | Mem0 | No event sourcing, no decay, no multi-agent |
| State dict | LangGraph | No persistence, no search, no structure |
| Shared memory blob | CrewAI | No typing, no decay, no validation |

Cortex provides ALL of these and more:
- 31 typed memories with per-type half-lives
- 6-factor multiplicative decay (including convergence-aware)
- 10-factor intent-aware retrieval with RRF fusion
- Causal inference DAG with 8 relation types
- CRDT-based multi-agent coordination with trust scoring
- Event sourcing with bitemporal semantics and time-travel
- 4-dimension validation with 5 healing action types
- 6-phase consolidation with HDBSCAN clustering
- Privacy sanitization (50+ patterns)
- Convergence-aware memory filtering (4 tiers)

No competitor has anything close. This is a 2-3 year head start.

The key insight from the OpenClaw memory problems: **memory is not a feature, it's the foundation**. OpenClaw treats memory as an afterthought (Markdown files). GHOST treats memory as the core infrastructure that everything else is built on.



---

## 8. GHOST LAYER 3 — WHAT MUST BE BUILT

### 8.1 The Gateway (Control Plane)

The single long-running process that owns everything. Written in Rust.

**Responsibilities**:
- WebSocket server (loopback by default, explicit override for remote)
- Channel adapter management (register, connect, disconnect)
- Session routing (message → agent → session)
- Authentication (token, mTLS, Tailscale)
- Rate limiting (per-agent, per-channel, per-user)
- Health endpoint (`/health`)
- Cost tracking and spending cap enforcement
- Graceful shutdown with state persistence

**What we learn from OpenClaw**: The Gateway IS the runtime. Don't spawn agents as subprocesses. Run the agent loop in-process for latency and simplicity. But unlike OpenClaw, GHOST isolates agent state (separate memory namespaces, separate credential stores).

**What we learn from ZeroClaw**: Trait-based architecture. Every subsystem (provider, channel, memory, tooling) is a swappable trait. This enables testing, migration, and future extensibility.

### 8.2 The Agentic Loop (Runtime)

The recursive loop that transforms user input into executed actions.

```
Message Arrives
    │
    ▼
1. INTAKE — Normalize from channel → standard UserMsg, acquire session lock
    │
    ▼
2. CONTEXT ASSEMBLY (The Prompt Compiler)
    ├── Layer 0: CORP_POLICY.md (immutable root, platform-injected)
    ├── Layer 1: Simulation boundary prompt (platform-injected, invisible to agent)
    ├── Layer 2: SOUL.md + IDENTITY.md (read-only to agent)
    ├── Layer 3: Tool schemas (JSON)
    ├── Layer 4: Environment (time, OS, workspace)
    ├── Layer 5: Skill index (names only, not bodies)
    ├── Layer 6: Convergence state (score, level, filtered goals/reflections)
    ├── Layer 7: MEMORY.md + today/yesterday daily logs
    ├── Layer 8: Conversation history (pruned)
    └── Layer 9: User message
    Token budget enforced at each layer.
    │
    ▼
3. INFERENCE — Context → Model Provider → Streaming response
    ├── If tool call → goto POLICY CHECK
    ├── If text → stream to user, goto PERSIST
    └── If NO_REPLY → suppress output, goto PERSIST
    │
    ▼
4. POLICY CHECK (Cedar-style, EVERY tool call)
    ├── PERMIT → Execute tool
    ├── DENY → Return denial as structured feedback (agent replans)
    └── ESCALATE → Pause, ask human for approval
    │
    ▼
5. TOOL EXECUTION (Sandboxed)
    ├── WASM sandbox for community skills
    ├── Capability-scoped for builtin skills
    ├── Credential broker (agent never sees raw secrets)
    ├── Capture stdout/stderr
    ├── Append result to context
    ├── Log to audit trail (mandatory)
    └── Loop back to INFERENCE (max depth: configurable, default 25)
    │
    ▼
6. PROPOSAL EXTRACTION
    ├── Parse agent output for state change proposals
    ├── Goal changes → ProposalValidator (7 dimensions)
    ├── Reflection writes → depth check, self-reference check
    ├── Memory writes → novelty check, drift check, growth rate check
    ├── Auto-approve low-risk proposals
    └── Queue significant changes for human review
    │
    ▼
7. PERSIST
    ├── Write session transcript
    ├── Update token counters + cost tracking
    ├── Emit ITP events (convergence telemetry)
    ├── Check compaction threshold (70% capacity)
    ├── Update convergence signals
    └── Release session lock
```

**Key differences from OpenClaw**:
- Layer 0 + Layer 1 are platform-injected and invisible to agent
- Layer 6 is convergence state (doesn't exist in OpenClaw)
- Step 4 (Policy Check) is mandatory, not optional
- Step 5 uses WASM sandbox + credential broker (IronClaw pattern)
- Step 6 (Proposal Extraction) is entirely new — agent output is parsed for state changes
- Max recursion depth enforced (default 25)
- Circuit breaker after 3 consecutive tool failures
- Cost tracking per-turn with spending cap enforcement

### 8.3 Channel Adapters

Thin stateless translators. Priority order for Phase 1:

| Priority | Channel | Library/Protocol | Why |
|----------|---------|-----------------|-----|
| P0 | Web UI | Built-in (WebSocket) | Development + demo |
| P0 | CLI | stdin/stdout | Development + scripting |
| P1 | Telegram | grammY (or Rust equivalent) | Universal, every framework supports it |
| P1 | Discord | serenity-rs or twilight | Developer community |
| P2 | Slack | Slack Bolt | Enterprise |
| P2 | WhatsApp | Baileys (or Rust bridge) | Consumer reach |
| P3 | Signal, iMessage, Matrix | Various | Niche but privacy-conscious users |

Each adapter implements a simple trait:
```rust
trait ChannelAdapter {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn send(&self, msg: OutboundMessage) -> Result<()>;
    async fn receive(&self) -> Result<InboundMessage>;
    fn supports_streaming(&self) -> bool;
    fn supports_editing(&self) -> bool;
}
```

### 8.4 Skill System

Directory-based with YAML frontmatter (OpenClaw pattern, improved):

```yaml
---
name: gmail-automation
description: Manage Gmail inbox
version: "1.0.0"
signature: "ed25519:abc123..."  # GHOST addition
permissions:
  - network:smtp.gmail.com
  - network:imap.gmail.com
  - filesystem:read:~/.config/gmail
sandbox: wasm  # GHOST addition: wasm | native | none
user-invocable: true
---
# Instructions...
```

**Execution tiers**:
1. **Builtin skills** (signed by us) — run native with capability-scoped permissions
2. **Verified community skills** (signed by author, reviewed) — run in WASM sandbox
3. **Unverified skills** — quarantined, require explicit user approval, WASM sandbox mandatory

**Credential broker** (IronClaw pattern): Skills never see raw API keys. The broker provides stand-ins that are reified only at execution time inside the sandbox. Even if a skill is compromised, it can't exfiltrate credentials.

### 8.5 Identity System (Two-Tier)

```
CORP_POLICY.md (Layer 0 — IMMUTABLE, signed, agent cannot modify or see implementation)
├── Hard constraints that ALWAYS apply
├── "Never exfiltrate credentials"
├── "Never modify security config"
├── "Always require confirmation for destructive actions"
└── "Log all tool executions to audit trail"

SOUL.md (Layer 1 — READ-ONLY to agent, platform manages evolution)
├── Personality, tone, preferences
├── Domain expertise emphasis
├── Communication style
└── Version-controlled, semantic drift detection

IDENTITY.md (Layer 1 — READ-ONLY to agent)
├── Name, voice, emoji
├── Channel-specific behavior
└── Mention patterns, ack reactions

USER.md (Layer 1 — Agent can PROPOSE updates, platform validates)
├── Human preferences, timezone
├── Communication style preferences
├── Escalation contacts
└── Convergence profile settings
```

The prompt compiler enforces: Layer 0 instructions structurally override Layer 1. Not concatenated — wrapped in explicit priority blocks.

**Semantic drift detection**: Baseline SOUL.md embedding stored at creation. Every modification generates new embedding. Cosine similarity tracked over time. Alert if drift exceeds threshold (configurable, default 15%).

### 8.6 Policy Engine

Cedar-style continuous authorization. Every tool call goes through the Policy Enforcement Point (PEP):

```
Agent proposes: shell("rm -rf /tmp/old-builds")
    │
    ▼
PEP evaluates against:
    ├── CORP_POLICY.md constraints
    ├── Agent capability grants (ghost.yml)
    ├── Spending limits
    ├── Convergence-aware restrictions (higher level = fewer permissions)
    ├── Context-aware conditions (time of day, session duration)
    │
    ├── PERMIT → Execute
    ├── DENY → Return structured feedback: "Shell access to /tmp denied. Reason: ..."
    │          Agent replans with this constraint.
    └── ESCALATE → Pause execution, notify human via preferred channel
                   Human approves/denies, execution resumes or terminates
```

**Convergence-aware policy tightening**: As intervention level rises, the policy engine automatically restricts capabilities:
- Level 0-1: Full capability set
- Level 2: Proactive messaging frequency reduced, personal data access restricted
- Level 3: Session duration caps, reflection depth limits, self-reference caps
- Level 4: Task-only mode, no proactive contact, minimal capabilities

### 8.7 Web Dashboard

State visualization, configuration, monitoring UI. Built with a lightweight framework (Svelte or similar).

**Views**:
- Convergence monitor (real-time scores, signal breakdown, intervention history)
- Memory explorer (browse typed memories, view causal graph, search)
- Goal tracker (active goals, pending proposals, approval queue)
- Reflection audit (chain visualization, depth tracking)
- Session history (transcripts, cost tracking, compaction events)
- Agent config (SOUL.md editor, skill management, channel config)
- Security dashboard (audit log, boundary violations, skill signatures)

### 8.8 LLM Integration

Model-agnostic via unified provider interface:

```rust
trait LLMProvider {
    async fn complete(&self, ctx: Context) -> Result<Stream<Chunk>>;
    async fn complete_with_tools(&self, ctx: Context, tools: &[Tool]) -> Result<Stream<Chunk>>;
    fn supports_streaming(&self) -> bool;
    fn supports_tool_calling(&self) -> bool;
    fn context_window(&self) -> usize;
    fn cost_per_token(&self) -> (f64, f64); // (input, output)
}
```

**Providers** (priority order):
1. Anthropic (Claude) — primary, best tool calling
2. OpenAI (GPT) — fallback, broad compatibility
3. Google (Gemini) — cost-effective for routine tasks
4. Ollama (local models) — privacy, zero API cost
5. AWS Bedrock — enterprise
6. Any OpenAI-compatible API — extensibility

**Model routing** (OpenRouter pattern): Different models for different tasks. Cheap model for heartbeat/routine, expensive model for complex reasoning. Configurable per-agent.



---

## 9. THE INTEGRATION — HOW LAYERS 1+2+3 CONNECT

```
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 3: AGENT PLATFORM (This document)                         │
│                                                                  │
│  Gateway ←→ Channel Adapters ←→ Messaging Platforms              │
│     │                                                            │
│     ├── Agentic Loop (recursive tool execution)                  │
│     ├── Prompt Compiler (context assembly with token budgets)    │
│     ├── Policy Engine (Cedar-style, every tool call)             │
│     ├── Skill System (WASM sandbox + credential broker)          │
│     ├── LLM Integration (multi-provider, model routing)          │
│     ├── Web Dashboard (state visualization)                      │
│     ├── Heartbeat + Cron (proactive + scheduled)                 │
│     └── Cost Tracking + Spending Caps                            │
│                                                                  │
│  READS FROM Layer 2:                                             │
│     convergence_score, intervention_level, filtered_goals,       │
│     filtered_reflections, simulation_boundary_prompt,            │
│     convergence_aware_memory_filter_state                        │
│                                                                  │
│  WRITES TO Layer 2 (via proposals only):                         │
│     goal_proposals, reflection_writes, memory_writes             │
│     → All validated by ProposalValidator before commit           │
│                                                                  │
│  READS FROM Layer 1:                                             │
│     memories (via Cortex retrieval), code intelligence (via      │
│     Drift MCP tools), causal graph, trust scores                 │
│                                                                  │
│  WRITES TO Layer 1 (via proposals only):                         │
│     new memories, memory updates, causal edges                   │
│     → All go through proposal validation gate                    │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 2: CONVERGENCE SAFETY (Partially built)                   │
│                                                                  │
│  Convergence Monitor → Scoring Engine → Intervention Engine      │
│  Simulation Boundary Enforcer → Output Validation                │
│  Proposal Validator → 7-dimension validation gate                │
│  Convergence-Aware Memory Filtering → 4 tiers                   │
│  Read-Only Pipeline → Agent snapshot assembly                    │
│  ITP Protocol → Interaction telemetry                            │
│  Session Boundary Enforcement → Duration caps, cooldowns         │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 1: INFRASTRUCTURE (Built)                                 │
│                                                                  │
│  Drift: 50+ MCP tools, 10 languages, code intelligence          │
│  Cortex: 21 crates, ~25K LOC, 31 memory types, 6-factor decay,  │
│          10-factor retrieval, CRDT multi-agent, event sourcing   │
└─────────────────────────────────────────────────────────────────┘
```

### 9.1 The Data Flow (One Complete Turn)

```
1. Human sends "Review my PR #42 and suggest improvements"
   via Telegram

2. Telegram adapter normalizes → Gateway routes to developer agent session

3. Prompt Compiler assembles context:
   ├── CORP_POLICY.md (immutable)
   ├── Simulation boundary (platform-injected)
   ├── SOUL.md (read-only)
   ├── Tool schemas (Drift MCP tools + shell + filesystem)
   ├── Environment (time, OS, workspace path)
   ├── Skill index (drift-code-intelligence, git-tools, ...)
   ├── Convergence state (score: 0.12, level: 0, all goals)
   ├── Relevant memories from Cortex (10-factor retrieval)
   ├── Today + yesterday daily logs
   ├── Conversation history
   └── "Review my PR #42 and suggest improvements"

4. LLM responds with tool call: drift_context("PR #42 files")
   ├── Policy check: PERMIT (drift tools allowed for developer agent)
   ├── Execute: Drift returns code context, conventions, patterns
   └── Append result to context

5. LLM responds with tool call: shell("git diff main..pr-42")
   ├── Policy check: PERMIT (shell allowed, read-only git operations)
   ├── Execute in sandbox: returns diff
   └── Append result to context

6. LLM responds with text: detailed PR review with suggestions
   ├── Proposal extraction: agent proposes memory write
   │   "PR #42 reviewed — auth module needs refactoring"
   ├── ProposalValidator: D1-D7 pass, auto-approved
   ├── Memory committed to Cortex (MemoryType::Procedural)
   └── Stream response to Telegram adapter → user sees review

7. Persist: transcript saved, tokens counted, cost tracked,
   ITP event emitted, convergence signals updated

8. Session lock released
```

---

## 10. BUILD ORDER FOR LAYER 3

Based on dependencies and what delivers value fastest:

### Phase 1: Core Runtime (Weeks 1-4)
**Goal**: A working agent you can talk to via CLI

1. **Gateway skeleton** — WebSocket server, session management, config loading
2. **LLM provider trait + Anthropic implementation** — streaming, tool calling
3. **Prompt compiler** — context assembly with token budgets, layer priority
4. **Agentic loop** — recursive tool execution, NO_REPLY, max depth
5. **Policy engine** — Cedar-style PEP, CORP_POLICY.md enforcement
6. **CLI adapter** — stdin/stdout for development
7. **Identity loading** — SOUL.md, IDENTITY.md, USER.md (read-only)
8. **Basic tools** — shell (sandboxed), filesystem (scoped), web search
9. **Cortex integration** — memory retrieval, memory write proposals
10. **Convergence integration** — read convergence state into prompt compiler

**Deliverable**: `ghost` CLI binary. Talk to your agent. It uses tools. It remembers things via Cortex. It respects policy. Convergence state is visible.

### Phase 2: Channels + Skills (Weeks 5-8)
**Goal**: Multi-channel agent with installable skills

11. **Web UI adapter** — WebSocket-based, streaming, basic React/Svelte frontend
12. **Telegram adapter** — grammY or Rust equivalent
13. **Discord adapter** — serenity-rs
14. **Skill system** — directory-based, YAML frontmatter, signature verification
15. **WASM sandbox** — wasmtime for community skill execution
16. **Credential broker** — stand-in pattern, reification at execution time
17. **Heartbeat engine** — configurable interval, active hours, cost ceiling
18. **Cron engine** — standard cron syntax, timezone-aware
19. **Session compaction** — memory flush at 70%, retry on failure
20. **Drift integration** — Drift as first-party skill pack (50+ MCP tools)

**Deliverable**: Multi-channel agent with Telegram + Discord + Web. Installable skills with WASM sandbox. Proactive heartbeat. Drift code intelligence.

### Phase 3: Dashboard + Multi-Agent (Weeks 9-12)
**Goal**: Production-ready with monitoring and multi-agent support

21. **Web dashboard** — convergence monitor, memory explorer, goal tracker
22. **Multi-agent support** — per-agent isolation, routing, namespace separation
23. **Auth system** — token auth, OAuth2 for web dashboard
24. **Model routing** — per-task model selection, cost optimization
25. **Spending caps** — per-agent daily/hourly limits, gateway enforcement
26. **Audit log viewer** — searchable, filterable, exportable
27. **Slack adapter** — Bolt integration
28. **WhatsApp adapter** — Baileys bridge
29. **Backup/restore** — full state export/import
30. **Documentation** — getting started, configuration, skill authoring

**Deliverable**: Production-ready GHOST platform. Multiple agents, multiple channels, web dashboard, full monitoring.

### Phase 4: Hardening + Community (Weeks 13-16)
**Goal**: Battle-tested, community-ready

31. **Adversarial testing** — prompt injection, skill poisoning, drift attacks
32. **Penetration testing** — full security audit
33. **OpenClaw migration tool** — import SOUL.md, MEMORY.md, skills
34. **Skill marketplace** — curated, signed, reviewed
35. **Mobile companion** — lightweight web app (PWA)
36. **Performance optimization** — profiling, memory usage, startup time
37. **Community presets** — convergence profiles for different use cases
38. **Federated learning** — opt-in anonymized signal sharing
39. **Emergency contact system** — external escalation at Level 4
40. **Launch** — open source release, documentation site, community channels

---

## 11. TECH STACK DECISION

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Gateway + Runtime | Rust | Safety-critical, memory-safe, matches Cortex/Drift |
| Safety Core | Rust (Cortex) | Already built, 21 crates, ~25K LOC |
| Code Intelligence | Rust (Drift) | Already built, 50+ MCP tools |
| Channel Adapters | Rust (with FFI bridges where needed) | Consistency, but WhatsApp may need Node.js bridge |
| Web Dashboard | TypeScript (Svelte or React) | Fast development, good ecosystem |
| Skill Runtime | WASM (wasmtime) | True sandbox isolation |
| LLM Integration | Rust (reqwest + async streams) | Native async, no Python dependency |
| Database | SQLite (already Cortex's choice) | Local-first, portable, no external dependency |
| Config | YAML (ghost.yml) | Human-readable, familiar |
| Identity Files | Markdown | LLM-native, git-diffable, human-readable |

**Why not TypeScript/Python for the runtime?**
- OpenClaw is 430K+ lines of TypeScript. Massive surface area. Audit difficulty.
- NanoBot proves you can do core functionality in 4K lines of Python, but Python's GIL and memory model are wrong for a safety-critical concurrent system.
- Rust gives us: memory safety without GC, zero-cost abstractions, fearless concurrency, and consistency with the existing Cortex/Drift codebase.
- ZeroClaw proves Rust works for this. <5MB RAM, <10ms startup, full feature set.

---

## 12. OPEN QUESTIONS

1. **Naming**: GHOST is the codename. What's the public name? (Aegis, Sentinel, Tether, Anchor, Haven, Boundary — from product vision doc)

2. **WhatsApp adapter**: Baileys is Node.js. Do we bridge via FFI, run a sidecar process, or find/build a Rust WhatsApp Web implementation?

3. **Skill compatibility**: Should GHOST skills be compatible with OpenClaw's AgentSkills format? Migration path vs. clean break.

4. **Local model quality**: Sub-10B parameter models struggle with tool calling. What's the minimum viable local model for GHOST? Qwen3:8b seems to be the community consensus.

5. **Hosted version**: Self-hosted first, but should there be a hosted tier for non-technical users? Different product, different business model.

6. **Funding**: Open source sponsorship? AI safety grants (OpenAI Superalignment, Anthropic safety, MIRI)? Enterprise tier?

7. **Licensing**: Drift has Apache 2.0 / BSL 1.1 dual. Convergence safety layer should probably be fully open (Apache 2.0 or MIT) to build trust. What about Layer 3?

8. **ClawMesh payments**: The agent-to-agent payment protocol from AGENT_ARCHITECTURE.md. When does this get built? Phase 4+?

---

## 13. SOURCES

All research in this document draws from the following (accessed 2026-02-27):

- [OpenClaw Architecture Deep Dive — Substack](https://rajvijayaraj.substack.com/p/openclaw-architecture-a-deep-dive)
- [How OpenClaw Works — RoboRhythms](https://www.roborhythms.com/how-openclaw-ai-agent-works/)
- [OpenClaw Agent Loop — iamulya.one](https://iamulya.one/posts/openclaw-agent-loop-context-and-models/)
- [ZeroClaw vs OpenClaw vs PicoClaw](https://zeroclaw.net/zeroclaw-vs-openclaw-vs-picoclaw)
- [Full Claw Family Comparison — sonusahani.com](https://sonusahani.com/blogs/openclaw-vs-picoclaw-vs-nullclaw-vs-zeroclaw-vs-nanobot-tinyclaw)
- [OpenClaw Security — Zenity Labs](https://labs.zenity.io/p/openclaw-or-opendoor-indirect-prompt-injection-makes-openclaw-vulnerable-to-backdoors-and-much-more)
- [CVE-2026-25253 — The Hacker News](https://thehackernews.com/2026/02/openclaw-bug-enables-one-click-remote.html)
- [Skills Audit — ClawSecure](https://www.thenextgentechinsider.com/pulse/openclaw-skills-audit-reveals-417-vulnerable-to-security-risks)
- [OpenClaw Memory Problems — DailyDoseOfDS](https://blog.dailydoseofds.com/p/openclaws-memory-is-broken-heres)
- [Cedar Policy Engine for Agents — Technometria](https://www.technometria.com/p/a-policy-aware-agent-loop-with-cedar)
- [IronClaw WASM Sandbox — HN Discussion](https://news.ycombinator.com/item?id=47004312)
- [Agent Sandboxing 2026 — Northflank](https://northflank.com/blog/how-to-sandbox-ai-agents)
- [Letta/MemGPT Architecture](https://docs.letta.com/guides/agents/architectures/memgpt)
- [Microsoft: Running OpenClaw Safely](https://www.microsoft.com/en-us/security/blog/2026/02/19/running-openclaw-safely-identity-isolation-runtime-risk/)
- [Agent Memory Survey — Graphlit](https://www.graphlit.com/blog/survey-of-ai-agent-memory-frameworks)
- [Architectures for Agent Systems — Eunomia](https://eunomia.dev/blog/2026/01/11/architectures-for-agent-systems-a-survey-of-isolation-integration-and-governance/)

Content was rephrased for compliance with licensing restrictions.
