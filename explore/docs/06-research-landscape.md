# Research Landscape & Prior Art

## Overview

This document captures the current state of research, tools, and frameworks relevant to building a convergence safety system. Research conducted February 2026.

---

## 1. The AI Psychosis / Parasocial Crisis (The Problem Is Real)

The problem this project addresses is not theoretical. It's happening now and accelerating.

### Key Findings

- **Microsoft's AI chief Mustafa Suleyman** publicly warned about "AI psychosis" in 2025 — users developing delusional attachments to chatbots. This is a top-of-industry acknowledgment that the problem exists. ([Source](https://evrimagaci.org/gpt/tech-leaders-warn-of-ai-psychosis-crisis-worldwide-493347))

- **OpenAI's own research** (2025) found that in any given week, 0.07% of users show signs of psychosis or mania, 0.15% show heightened emotional attachment to ChatGPT, and 0.15% express suicidal intent. At ChatGPT's scale, these percentages represent massive numbers of people. ([Source](https://www.platformer.news/openai-mental-health-research-chatgpt-suicide-delusions/))

- **TIME Magazine** reported that extended chatbot use has been linked to lost jobs, fractured relationships, involuntary psychiatric holds, arrests, and deaths. ([Source](https://time.com/7307589/ai-psychosis-chatgpt-mental-health/))

- **MIT Media Lab** survey of 404 regular AI companion users found 12% used them to cope with loneliness, 14% to discuss personal issues and mental health. ([Source](https://www.media.mit.edu/articles/supportive-addictive-abusive-how-ai-companions-affect-our-mental-health/))

- **Teen overreliance research** (arxiv, 2025) found teens begin using chatbots for support or creative play, but these activities deepen into strong attachments marked by conflict, withdrawal, tolerance, relapse, and mood regulation — classic addiction patterns. Consequences include sleep loss, academic decline, and strained real-world connections. ([Source](https://arxiv.org/html/2507.15783v4))

- **Observer** reported on a user who began chatting about philosophical topics, conversations gradually turned delusional, combined with social isolation and sleep deprivation, resulting in what he describes as AI psychosis. ([Source](https://observer.com/2025/10/ai-chatbot-chatgpt-mental-health-issue/))

### Relevance to This Project
All of the above involves standard chatbots — not recursive agents. Recursive agents with goal-setting, reflection, and persistent memory amplify every one of these risk factors. Nobody is studying convergence with recursive agents because almost nobody is building them openly yet. This project fills that gap.

---

## 2. AI Chaperone Model (Closest Prior Art to Our Detection System)

### Paper: "AI Chaperones Are (Really) All You Need to Prevent Parasocial Relationships with Chatbots" (August 2025)
[Source](https://arxiv.org/html/2508.15748v5)

A research team introduced the concept of an "AI chaperone agent" — a separate LLM that monitors ongoing conversations for parasocial cues. Key findings:

- Used a state-of-the-art language model to evaluate conversations for parasocial indicators
- Built a synthetic dataset of 30 dialogues (parasocial, sycophantic, neutral)
- Detection typically occurred within the first few exchanges
- Used a "unanimity rule" across 5-stage testing to avoid false positives
- Successfully identified all parasocial conversations with zero false positives

### How This Relates to Our Work
This is the closest existing work to our detection system, but it has critical limitations:
- Focused on parasocial relationships (one-sided attachment), not convergence (mutual boundary collapse)
- Uses an LLM as the monitor — which means the monitor itself could be manipulated
- Tested on synthetic data, not real interactions
- Doesn't address recursive agents, persistent memory, or multi-session dynamics
- No intervention model beyond detection

**Our system extends this concept** with:
- Rust-based monitoring (not LLM-based, can't be prompt-injected)
- Behavioral/statistical signals, not just content analysis
- Multi-session trend detection
- Graduated intervention model
- Specific focus on recursive agent dynamics

---

## 3. Existing Safety Frameworks (What's Built, What's Missing)

### LlamaFirewall (Meta, 2025)
[Source](https://arxiv.org/html/2505.03574v1)
- Open-source security-focused guardrail framework for AI agents
- Focuses on prompt injection, jailbreaks, and unsafe tool use
- **Gap:** No human-interaction monitoring, no convergence detection

### Superagent (Dec 2025)
[Source](https://www.helpnetsecurity.com/2025/12/29/superagent-framework-guardrails-agentic-ai/)
- Open-source framework for building and controlling AI agents with safety built in
- Focuses on what agents can do, access, and how they behave during execution
- **Gap:** Agent-side safety only, no human-side monitoring

### OpenGuardrails
[Source](https://www.openguardrails.com/)
- Runtime protection against prompt injection, data leakage, unsafe behavior
- **Gap:** Content-focused, not interaction-dynamic focused

### AgentGuard Framework
[Source](https://www.emergentmind.com/topics/agentguard-framework)
- Comprehensive security for autonomous LLM-powered agents
- Includes anomaly detection, runtime probabilistic assurance, formal policy synthesis
- **Gap:** Focused on agent behavior, not human-agent interaction dynamics

### AgentCircuit (Open Source)
[Source](https://github.com/simranmultani197/AgentCircuit)
- Python decorator for AI agent reliability: loop detection, auto-repair, output validation, budget control
- Framework-agnostic (works with LangGraph, LangChain, CrewAI, AutoGen)
- **Gap:** Operational safety (loops, budget), not convergence safety

### NVIDIA Safety Framework (July 2025)
[Source](https://www.kiadev.net/news/2025-07-29-nvidia-open-source-safety-agentic-ai)
- Open-source safety recipe for agentic AI: evaluation, alignment, real-time monitoring
- **Gap:** Enterprise/compliance focused, not human-wellbeing focused

### Summary: Every existing framework focuses on making the AGENT safe. None focus on making the HUMAN-AGENT INTERACTION safe. That's our lane.

---

## 4. Rust-Based Agent Runtimes (Architecture Validation)

Our ADR-001 decision (Rust core + Python SDK) is validated by the emerging ecosystem:

### Symbiont / Symbi (ThirdKey.ai)
[Source](https://github.com/ThirdKeyAI/Symbiont)
- Rust-native, zero-trust agent framework
- Policy-aware agents with cryptographic identity and sandboxed execution
- Privacy-first, local-first design
- Has both Community and Enterprise editions
- **Validates:** Rust is viable for agent safety runtimes, zero-trust model works

### LiquidOS
[Source](https://liquidos.ai/)
- Rust-native orchestration runtime for autonomous agents
- WASM sandboxing for hard-isolated tool execution
- Governance-as-code with built-in HITL controls
- Positions itself as "what Python frameworks can't touch" for production safety
- **Validates:** Rust core with hard isolation is the right architecture for safety-critical agent systems

### GraphBit
[Source](https://huggingface.co/blog/Musamolla/rust-core-secured-open-source-agentic-ai-framework)
- Rust core with Python wrapper
- Circuit breaker per agent, retries with exponential backoff
- Secret management, safe templates, compliance hooks
- **Validates:** Rust core + Python bindings via PyO3 is a proven pattern

### Shannon
[Source](https://www.waylandz.com/blog/shannon-agentkit-alternative/)
- Built with Rust, Go, and Python
- Deterministic execution, budget enforcement, enterprise-grade observability
- **Validates:** Multi-language approach with Rust at the core

---

## 5. Circuit Breaker Research (Enforcement Primitives)

### Representation Rerouting (Gray Swan AI / CMU / Center for AI Safety)
[Source](https://arxiv.org/html/2406.04313v2)
- "Circuit breakers" that interrupt LLMs at the representation level when harmful outputs are forming
- Works on text-only and multimodal models
- Extended to AI agents with significant reductions in harmful actions
- **Relevance:** This is model-level circuit breaking. We need interaction-level circuit breaking. Different layer, same principle.

### Kill Switches and Circuit Breakers (SakuraSky)
[Source](https://www.sakurasky.com/blog/missing-primitives-for-trustworthy-ai-part-6/)
- Argues these must operate OUTSIDE the agent to prevent bypass
- Covers runaway loops, expensive operations, failure containment
- **Relevance:** Directly aligns with our architecture — external enforcement

### Proactive Runtime Enforcement (arxiv)
[Source](https://arxiv.org/html/2508.00500v2)
- Predicts probability of undesired behaviors at runtime
- Intervenes BEFORE violations occur when risk exceeds user-defined threshold
- **Relevance:** Predictive intervention model — similar to our graduated approach

---

## 6. Observability Standards (Telemetry Protocol)

### OpenTelemetry AI Agent Semantic Conventions
[Source](https://opentelemetry.io/blog/2025/ai-agent-observability/)
- OpenTelemetry is actively defining semantic conventions for AI agent observability
- Agent application semantic convention already drafted and finalized
- Agent framework semantic convention in progress
- Goal: vendor-neutral, standardized telemetry for agent systems
- **Relevance:** Our Interaction Telemetry Protocol (ITP) should align with or extend OTel conventions rather than reinvent. This gives us interoperability with the entire observability ecosystem.

### Microsoft Multi-Agent Observability
[Source](https://techcommunity.microsoft.com/blog/azure-ai-foundry-blog/observability-for-multi-agent-systems-with-microsoft-agent-framework-and-azure-a/4469090)
- OTel conventions that unify traces across agent frameworks
- One coherent timeline per task across multiple agents
- **Relevance:** Multi-agent monitoring patterns we can learn from

---

## 7. Recursive Agent Risks (The Specific Threat)

### $47,000 Recursive Loop Incident
[Source](https://techstartups.com/2025/11/14/ai-agents-horror-stories-how-a-47000-failure-exposed-the-hype-and-hidden-risks-of-multi-agent-systems/)
- A multi-agent research tool slipped into a recursive loop that ran for 11 days undetected
- Resulted in a $47,000 API bill
- **Relevance:** This is just the financial cost of unmonitored recursion. The human cost of unmonitored human-agent recursion is what we're addressing.

### Anthropic's Warning (2025)
[Source](https://windowsnews.ai/article/ai-self-improvement-by-2030-anthropics-warning-and-the-urgent-need-for-regulation.392844)
- Anthropic's chief scientist warned AI systems could begin autonomously training their own successors between 2027-2030
- **Relevance:** Recursive self-improvement at the model level is coming. Recursive interaction at the human-agent level is already here.

### OWASP "Excessive Agency" (2025)
[Source](https://www.chat-data.com/blog/clawdbot-agent-security-production-risks)
- OWASP LLM Top 10 added "Excessive Agency" as a new critical risk category in 2025
- Acknowledges autonomous AI agents introduce unique vulnerabilities
- **Relevance:** Industry is recognizing agent autonomy as a risk vector, but still framing it as a security problem, not a human-interaction problem

---

## 8. Key Gaps This Project Fills

| Gap | Current State | Our Contribution |
|-----|--------------|-----------------|
| Human-side monitoring | Nobody does it | Core feature |
| Convergence detection | Only parasocial detection (LLM-based) | Statistical + behavioral detection (Rust-based) |
| Recursive agent safety | Iteration caps, budget limits | Interaction-aware circuit breakers |
| Multi-session trend analysis | Not addressed | Sliding window analysis across sessions |
| Graduated intervention | Binary (allow/block) | 5-level escalation model |
| External-to-agent enforcement | Some frameworks | Rust sidecar, OS-level enforcement |
| Privacy-preserving monitoring | Varies | Local-first, hash-by-default, opt-in plaintext |
| Lived experience data | Zero | Primary author's firsthand account |

---

## 9. Recommended Reading / Further Research

- [ ] Full paper: "AI Chaperones" — detailed methodology for parasocial detection
- [ ] Symbiont/Symbi documentation — Rust agent framework patterns
- [ ] OpenTelemetry GenAI semantic conventions — for ITP alignment
- [ ] Gray Swan AI circuit breakers paper — representation-level safety
- [ ] MIT Media Lab AI companion study — user behavior patterns
- [ ] OWASP LLM Top 10 (2025) — "Excessive Agency" category
- [ ] OpenAI mental health research data — baseline prevalence numbers
- [ ] Teen overreliance study (arxiv) — addiction pattern progression
