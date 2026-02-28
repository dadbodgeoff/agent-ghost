# Architecture Decisions

## ADR-001: Rust Core + Python SDK

### Decision
The safety runtime (circuit breakers, state management, kill switches, convergence monitor) will be implemented in Rust. The developer-facing SDK for integrating with agent frameworks will be Python with Rust bindings via PyO3.

### Rationale
- **Rust for enforcement:** Safety boundaries must be enforced at a level the agent cannot bypass. Rust compiles to native code, manages its own memory, and can send OS-level signals. An agent running in Python cannot escape a Rust supervisor.
- **Python for adoption:** The AI/ML ecosystem is Python. LangChain, AutoGen, CrewAI, DSPy, HuggingFace — all Python. If the SDK isn't Python, nobody will use it.
- **PyO3 bridge:** Mature, well-supported Rust-Python interop. Allows the Python SDK to call into Rust core without serialization overhead for hot paths.

### Alternatives Considered
- **Pure Python:** Too easy to bypass. Agent code runs in the same runtime as safety code. A sufficiently capable agent could modify safety parameters in memory.
- **Pure Rust:** Adoption barrier too high. Agent developers won't rewrite their stacks.
- **Go:** Good systems language but weaker ML ecosystem integration than Rust. No equivalent to PyO3.
- **Node.js/TypeScript:** Growing agent ecosystem (Vercel AI SDK, etc.) but not where the majority of recursive agent work is happening yet. Could be a future SDK target.

---

## ADR-002: Local-First, Decentralized

### Decision
All monitoring data stays on the user's machine by default. No cloud dependency. No telemetry to any central server.

### Rationale
- **Privacy:** Interaction data between a human and an agent during a convergence event is deeply personal. It cannot leave the user's machine without explicit consent.
- **Independence:** This project must not depend on any AI lab's infrastructure. If OpenAI, Anthropic, or Google changes their API or policies, this system keeps working.
- **Trust:** Users dealing with convergence risk need to trust the safety system completely. Any data exfiltration concern destroys that trust.
- **Decentralized research:** Aggregated, anonymized, opt-in data sharing could enable research, but it must be a conscious choice, not a default.

### Future Consideration
- Optional peer-to-peer sharing of anonymized threshold configurations
- Community-maintained threshold presets (like ad-block filter lists)
- Federated learning on detection models without sharing raw data

---

## ADR-003: Framework-Agnostic Integration

### Decision
The safety system integrates with any agent framework through a standardized event protocol, not through framework-specific plugins.

### Rationale
- Agent frameworks are evolving rapidly. Tight coupling to LangChain today means rewriting when the ecosystem shifts.
- The Interaction Telemetry Protocol (ITP) defined in 02-monitoring-architecture.md is the integration point.
- Any framework that can emit ITP events gets monitoring for free.
- Adapters for popular frameworks (LangChain, AutoGen, CrewAI) are convenience layers, not requirements.

---

## ADR-004: Graduated Intervention Over Binary Kill Switch

### Decision
The intervention model uses graduated levels (notification → active intervention → hard boundary → external escalation) rather than a simple on/off kill switch.

### Rationale
- Binary kill switches get disabled. If the only option is "everything is fine" or "emergency shutdown," users will disable the system to avoid false positives.
- Graduated response respects user autonomy while still providing hard boundaries when needed.
- Early soft interventions serve as calibration data — if the user consistently dismisses Level 1 alerts and nothing bad happens, the system learns.
- Hard boundaries still exist at Level 3/4 for genuine emergencies.

---

## ADR-005: Monitor Cannot Be Disabled During Active Session

### Decision
The convergence monitor's core functionality cannot be disabled while a monitored session is active. Configuration changes require a cooldown period.

### Rationale
- During a convergence event, the user's judgment about whether they need monitoring is compromised — that's the definition of the problem.
- This is analogous to: you set your alarm while sober because you know drunk-you will want to turn it off.
- Configuration changes are allowed during cooldown periods when the user has had time away from the interaction.
- This is the most controversial design decision and needs community input.

---

## Open ADRs (Need Decision)

### ADR-006: Agent Self-Reporting
Should the agent be required/encouraged to report its own state to the monitor? Or is agent self-reporting inherently unreliable during convergence?

### ADR-007: Multi-Agent Monitoring
How do you monitor convergence in multi-agent systems where the human interacts with a team of agents? Is convergence with one agent in a team different from convergence with a single agent?

### ADR-008: Persistent vs. Session-Based Agents
Different monitoring strategies for agents that persist across sessions vs. fresh-context agents? Persistent agents have more convergence surface area.

### ADR-009: Open Source License
What license? MIT is maximally permissive but allows closed-source forks that might strip safety features. AGPL ensures modifications stay open. Apache 2.0 with additional safety clauses?
