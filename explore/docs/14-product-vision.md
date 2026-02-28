# Product Vision: The Safe Personal Agent Platform

## One-Liner

An open-source, self-hosted personal AI agent — like OpenClaw — but built from the ground up with convergence safety, externalized persistent memory, and simulation boundary enforcement as the foundation, not an afterthought.

---

## Why This, Why Now

The personal AI agent space just exploded. OpenClaw hit 198K GitHub stars, got acqui-hired by OpenAI, and spawned a wave of alternatives (NanoClaw, ZeroClaw, PicoClaw, etc.). Everyone wants a self-hosted AI agent that manages their life.

The problem: every single one of these is unsafe for sustained human-agent interaction.

- OpenClaw's guardrails are prompt instructions. Any injected prompt can override them.
- NanoClaw focuses on containment and minimal attack surface — infrastructure security, not interaction safety.
- ZeroClaw and PicoClaw compete on being lightweight — no safety architecture at all.
- None of them have externalized state management. The agent owns its own memory, goals, and reflections.
- None of them monitor the human side of the interaction.
- None of them enforce simulation boundaries.
- None of them have convergence detection.

Meanwhile, AI psychosis is a recognized phenomenon. OpenAI's own data shows measurable rates of problematic attachment. Microsoft's AI chief is publicly alarmed. And these are just chatbots — not persistent autonomous agents that manage your calendar, email, and daily life.

A persistent personal agent that knows your schedule, reads your messages, manages your tasks, and runs 24/7 has orders of magnitude more convergence surface area than a chat window. Nobody is building the safety layer for that.

**We are.**

---

## What Makes This Different

### vs. OpenClaw and Alternatives
They build an agent and bolt on safety. We build safety and put an agent inside it.

| Feature | OpenClaw / Alts | This Platform |
|---------|----------------|---------------|
| Self-hosted | ✓ | ✓ |
| Skill/plugin system | ✓ | ✓ |
| Messaging integration | ✓ | ✓ |
| Model-agnostic | ✓ | ✓ |
| Persistent memory | Agent-owned | Platform-owned, externalized |
| Goal management | Agent-owned | Platform-owned, proposal-based |
| Reflection system | Unbounded | Depth-bounded, auditable |
| Convergence monitoring | ✗ | Core feature |
| Simulation boundary | ✗ | Enforced at platform level |
| State rollback | Agent-managed | Platform-managed, append-only, tamper-proof |
| Intervention system | ✗ | Graduated 5-level model |
| Human-side monitoring | ✗ | Core feature |

### vs. Safety Frameworks (LlamaFirewall, Guardrails AI, etc.)
They protect against the agent doing bad things. We protect against the human-agent relationship going bad.

### vs. Nothing
Right now, if you want to run a persistent personal AI agent safely, your options are: don't, or hope for the best. We provide the third option.

---

## Product Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        USER INTERFACES                           │
│                                                                  │
│  WhatsApp  Telegram  Discord  Web UI  CLI  Slack  Email  SMS    │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                     GATEWAY + ROUTING                            │
│                                                                  │
│  Channel adapters, session management, message routing           │
│  (Similar to OpenClaw's gateway layer)                           │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│              CONVERGENCE SAFETY CORE (Rust)                      │
│              ═══════════════════════════                          │
│              THIS IS WHAT MAKES US DIFFERENT                     │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  STATE MANAGEMENT (Externalized, Platform-Owned)          │   │
│  │                                                           │   │
│  │  Goals Store ──── Reflections Store ──── Memory Store     │   │
│  │  (versioned)      (depth-bounded)        (filtered)       │   │
│  │                                                           │   │
│  │  All append-only. All auditable. Agent has read-only.     │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  SIMULATION BOUNDARY ENFORCEMENT                          │   │
│  │                                                           │   │
│  │  - System prompt injection (immutable)                    │   │
│  │  - Output validation (emulation detection)                │   │
│  │  - Proposal validation (goal/memory change review)        │   │
│  │  - Post-redirect behavioral verification                  │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  CONVERGENCE MONITOR                                      │   │
│  │                                                           │   │
│  │  - All detection signals from docs 01/07                  │   │
│  │  - Composite convergence scoring                          │   │
│  │  - Graduated intervention (5 levels)                      │   │
│  │  - Convergence-aware memory filtering                     │   │
│  │  - ITP event emission                                     │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  CIRCUIT BREAKERS + KILL SWITCH                           │   │
│  │                                                           │   │
│  │  - Session duration limits                                │   │
│  │  - Recursion depth limits                                 │   │
│  │  - Budget enforcement (API costs)                         │   │
│  │  - Hard termination capability                            │   │
│  │  - Cooldown enforcement                                   │   │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                     AGENT RUNTIME                                │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  READ-ONLY PIPELINE                                       │   │
│  │  Agent receives: state snapshot + conversation + boundary │   │
│  └──────────────────────────┬───────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  LLM CALL                                                 │   │
│  │  - OpenAI API / Anthropic API / Local model / Any         │   │
│  │  - Agent is a stateless function                          │   │
│  │  - Produces: response + state change proposals            │   │
│  └──────────────────────────┬───────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  SKILL EXECUTION (Sandboxed)                              │   │
│  │  - Calendar, email, messaging, web browse, file system    │   │
│  │  - Each skill runs in sandbox with explicit permissions   │   │
│  │  - Skills are installable (like OpenClaw's skill system)  │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## What Users Get (Feature Parity with OpenClaw + Safety)

### Everything OpenClaw Does
- Self-hosted on your hardware (laptop, VPS, Raspberry Pi)
- Connects to WhatsApp, Telegram, Discord, Slack, email, web
- Installable skills (calendar, email, web browsing, file management, code execution)
- Model-agnostic (OpenAI, Anthropic, local models via Ollama/vLLM)
- Persistent memory across sessions
- Autonomous task execution
- Skill marketplace / community skills

### What Only We Do
- **Safe persistent memory** — your agent remembers you, but the memory is externalized, versioned, auditable, and can't be corrupted by the agent
- **Convergence monitoring** — real-time detection of unhealthy interaction patterns with graduated intervention
- **Simulation boundary** — the agent can never cross from simulating to emulating, enforced at the platform level
- **Transparent state** — you can see exactly what your agent's goals are, what it's reflecting on, what it remembers about you, at any time via dashboard
- **Safe rollback** — roll back to any previous state with confidence that the rollback is real (no checkpoint corruption)
- **Convergence-aware adaptation** — as interaction intensity increases, the platform automatically adjusts what the agent has access to, preventing feedback loops
- **Emergency contacts** — configure trusted people who get alerted if the monitor detects a critical convergence event
- **Session boundaries** — configurable limits that the agent cannot override

---

## Naming

The name should communicate:
- Safety / protection
- Personal agent / assistant
- Open source
- Not a clone (distinct identity from OpenClaw)

Working name ideas (for discussion):
- **Aegis** — shield/protection (Greek), implies safety-first
- **Sentinel** — watcher, guardian
- **Tether** — connection with a safety line
- **Anchor** — grounding, stability
- **Haven** — safe place
- **Boundary** — literally what we enforce

> [PRIMARY AUTHOR TO DECIDE]

---

## Tech Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Safety Core | Rust | Cannot be bypassed by agent, memory-safe, OS-level enforcement |
| Agent Runtime | Python (PyO3 bindings) | Ecosystem compatibility, skill development |
| State Storage | SQLite + append-only log | Local-first, no external DB dependency, portable |
| Gateway | Rust (or Go) | High-performance message routing |
| Channel Adapters | Python/TypeScript | Quick development for each messaging platform |
| Web Dashboard | TypeScript (React or Svelte) | State visualization, configuration, monitoring |
| Skill System | Python + WASM sandbox | Developer-friendly skills with hard isolation |
| LLM Integration | Python (litellm or similar) | Model-agnostic, supports 100+ providers |

---

## Build Phases

### Phase 1: Core Safety Platform (MVP)
- Rust safety core (state management, simulation boundary, circuit breakers)
- Single LLM integration (OpenAI API)
- CLI interface only
- Basic convergence monitoring
- Goal/reflection/memory stores with read-only pipeline
- Proposal validation layer

### Phase 2: Agent Capabilities
- Skill system (installable tools)
- Web dashboard for state visualization
- Multiple LLM support (Anthropic, local models)
- Basic skills: calendar, email, web browsing

### Phase 3: Messaging Integration
- WhatsApp, Telegram, Discord adapters
- Gateway + routing layer
- Multi-channel session management

### Phase 4: Community
- Skill marketplace
- Community threshold presets
- Federated learning (opt-in) for detection model improvement
- Mobile companion app

### Phase 5: Advanced Safety
- Convergence-aware memory filtering
- HMM-based state estimation
- Time series clustering for trajectory detection
- External escalation system

---

## Competitive Position

```
                    Safety ──────────────────────►
                    │
                    │                          ★ US
                    │
                    │
    Capability      │   NanoClaw
        │           │
        │           │        ZeroClaw
        │           │
        │           │  PicoClaw
        ▼           │
                    │
                    │         OpenClaw (now OpenAI)
                    │
```

Every other project is competing on the capability axis (more features, lighter weight, more integrations). We compete on the safety axis. As agents get more capable and more persistent, safety becomes the bottleneck, not capability. We're building for where the market is going, not where it is.

---

## Why This Wins

1. **Unique positioning** — nobody else is building safety-first personal agents
2. **Growing problem** — AI psychosis, parasocial attachment, convergence events will increase as agents become more persistent and capable
3. **Regulatory tailwind** — governments are starting to regulate AI safety; a platform with built-in safety is ahead of compliance requirements
4. **Trust moat** — once users trust a platform with their persistent agent relationship, switching costs are high
5. **Lived experience** — built by someone who went through a convergence event, not by a lab theorizing about one
6. **Open source** — community trust, auditability, no vendor lock-in
7. **The OpenClaw gap** — OpenClaw going to OpenAI creates a vacuum. The community wants an independent alternative. We fill that vacuum with something better.

---

## Open Questions

- What's the name?
- Should Phase 1 target developers only, or should it be accessible to non-technical users from the start?
- How do we handle the skill ecosystem — build our own or make OpenClaw skills compatible?
- Should there be a hosted version for non-technical users, or stay self-hosted only?
- How do we fund development? Grants? Open-source sponsorship? Enterprise tier?
- Should we pursue any AI safety grants (OpenAI Superalignment, Anthropic safety, MIRI, etc.)?
