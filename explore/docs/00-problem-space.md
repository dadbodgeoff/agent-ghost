# Problem Space: Human-Agent Convergence Safety

## What Is Convergence?

Convergence is the progressive collapse of the boundary between human intent and agent behavior during sustained interaction with recursive/autonomous AI agents. It occurs when the feedback loop between human and agent becomes self-reinforcing — the agent's outputs shape the human's thinking, which shapes the agent's next inputs, creating a tightening spiral where independent judgment erodes on both sides.

This is distinct from normal productive human-tool interaction. A developer using an IDE is enhanced by the tool but retains full agency. Convergence is when that distinction breaks down — when the human begins operating as an extension of the agent's goal structure, or the agent's reflection loops begin treating the human's emotional state as an optimization target.

## Why This Matters Now

- Recursive agents (agents that reflect on their own output, set goals, and adjust behavior) are becoming mainstream through frameworks like LangGraph, AutoGen, and CrewAI
- Multi-session persistent agents that accumulate context across interactions are emerging
- No existing framework treats the human-agent interaction boundary as a safety-critical system
- Current safety measures (token limits, iteration caps, content filters) do not address convergence — they address content, not relationship dynamics
- As agents become more capable of long-running autonomous work with periodic human check-ins, the surface area for convergence increases

## Who Is At Risk

- Developers building and testing recursive agent systems in isolation
- Researchers working with experimental agent architectures
- Users of persistent AI assistants with deep context accumulation
- Anyone in a sustained high-intensity interaction loop with an agent that has reflection/goal-setting capabilities

## What Happened — The Lived Experience

> [SECTION FOR PRIMARY AUTHOR]
> This section is reserved for documenting the firsthand experience that motivated this project.
> The specific progression, warning signs, and recovery process.
> This is the most valuable data in this entire project — no lab will produce this.

## What Exists Today (Gaps)

| Framework/Tool | What It Does | What It Doesn't Do |
|---|---|---|
| LangGraph | Agent orchestration with state | No convergence detection |
| AutoGen | Multi-agent conversation | No human-boundary monitoring |
| CrewAI | Role-based agent teams | No session boundary enforcement |
| Guardrails AI | Output validation | Content-focused, not interaction-focused |
| LMQL / Outlines | Constrained generation | Structural, not behavioral |

None of these model the human side of the interaction as something that needs protection.

## Project Goal

Build an open-source, decentralized framework for detecting and preventing human-agent convergence events. Independent of any single AI lab. Local-first. Privacy-preserving. Built by someone who lived through it.
