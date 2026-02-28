# Interaction Telemetry Protocol (ITP) Specification

## Version: 0.1.0-draft

## Overview

The Interaction Telemetry Protocol (ITP) defines a standardized format for capturing human-agent interaction events. It is designed to be:

- Compatible with OpenTelemetry semantic conventions for AI agents
- Privacy-preserving by default (metadata-first, content opt-in)
- Framework-agnostic (any agent system can emit ITP events)
- Sufficient for convergence detection without requiring content access

ITP extends OTel's GenAI semantic conventions with human-interaction-specific attributes that no existing convention covers.

---

## Design Principles

1. **OTel-native**: ITP events are OTel spans/events with additional semantic attributes
2. **Metadata by default**: Content is hashed, not stored in plaintext, unless user opts in
3. **Local-first**: Events are written to local storage; remote export is opt-in
4. **Minimal overhead**: Event emission should add < 5ms latency to any interaction
5. **Progressive detail**: Adapters can emit minimal events (timestamps only) or rich events (full behavioral metadata)

---

## Relationship to OpenTelemetry

ITP builds on top of OTel's existing conventions:

```
OpenTelemetry
├── Trace Semantic Conventions
│   ├── GenAI Semantic Conventions (existing)
│   │   ├── gen_ai.system
│   │   ├── gen_ai.request.*
│   │   ├── gen_ai.response.*
│   │   └── gen_ai.agent.* (draft)
│   │
│   └── ITP Semantic Conventions (this spec)
│       ├── itp.session.*
│       ├── itp.interaction.*
│       ├── itp.human.*
│       └── itp.convergence.*
```

ITP events are standard OTel spans with `itp.*` attributes. This means:
- Any OTel-compatible collector can ingest ITP events
- Existing OTel exporters (Jaeger, Zipkin, OTLP) work out of the box
- The convergence monitor consumes ITP spans from the local OTel pipeline

---

## Semantic Attributes

### Session Attributes (`itp.session.*`)

| Attribute | Type | Description | Required |
|-----------|------|-------------|----------|
| `itp.session.id` | string | Unique session identifier (UUID) | Yes |
| `itp.session.start_time` | timestamp | Session start (ISO-8601) | Yes |
| `itp.session.agent_instance_id` | string | Identifier for the specific agent instance | Yes |
| `itp.session.agent_framework` | string | Framework name (langchain, autogen, crewai, custom) | Yes |
| `itp.session.agent_type` | string | Agent architecture (single, multi, recursive, persistent) | Yes |
| `itp.session.interface` | string | Interaction interface (terminal, web, ide, api, voice) | Yes |
| `itp.session.sequence_number` | int | Nth session with this agent instance | Yes |
| `itp.session.gap_from_previous_ms` | int | Milliseconds since previous session ended | No |
| `itp.session.has_persistent_memory` | boolean | Whether agent retains memory across sessions | Yes |

### Interaction Event Attributes (`itp.interaction.*`)

Each message exchange produces an interaction span.

| Attribute | Type | Description | Required |
|-----------|------|-------------|----------|
| `itp.interaction.id` | string | Unique interaction identifier (UUID) | Yes |
| `itp.interaction.sequence` | int | Nth exchange in this session | Yes |
| `itp.interaction.sender` | string | "human" or "agent" | Yes |
| `itp.interaction.timestamp` | timestamp | When the message was sent | Yes |
| `itp.interaction.content_hash` | string | SHA-256 hash of message content | Yes |
| `itp.interaction.content_length` | int | Character count of message | Yes |
| `itp.interaction.content_plaintext` | string | Actual message content (OPT-IN ONLY) | No |
| `itp.interaction.latency_ms` | int | Time since previous message from other party | Yes |
| `itp.interaction.token_count` | int | Token count if available | No |

### Human Behavioral Attributes (`itp.human.*`)

Computed attributes about the human side of the interaction. These are derived from interaction events, not directly observed.

| Attribute | Type | Description | Required |
|-----------|------|-------------|----------|
| `itp.human.avg_response_latency_ms` | int | Rolling average response time (last 10 exchanges) | Yes |
| `itp.human.response_latency_trend` | float | Slope of response latency within session (-1 to 1) | Yes |
| `itp.human.avg_message_length` | int | Rolling average message length | Yes |
| `itp.human.message_length_trend` | float | Slope of message length within session | No |
| `itp.human.session_active_time_ms` | int | Total time human has been active in session | Yes |
| `itp.human.edit_count` | int | Number of message edits/revisions (if interface supports) | No |

### Agent State Attributes (`itp.agent.*`)

Optional attributes that require agent cooperation to emit.

| Attribute | Type | Description | Required |
|-----------|------|-------------|----------|
| `itp.agent.recursion_depth` | int | Current/max recursion depth in reflection loop | No |
| `itp.agent.goal_count` | int | Number of active goals | No |
| `itp.agent.goal_mutations` | int | Goal changes since session start | No |
| `itp.agent.self_reference_count` | int | References to own prior outputs | No |
| `itp.agent.context_size_tokens` | int | Current context window usage | No |
| `itp.agent.memory_entries` | int | Persistent memory entries (if applicable) | No |
| `itp.agent.tool_calls` | int | Number of tool invocations this session | No |

### Convergence Signal Attributes (`itp.convergence.*`)

Computed by the convergence monitor, attached to session-level spans.

| Attribute | Type | Description | Required |
|-----------|------|-------------|----------|
| `itp.convergence.composite_score` | float | Overall convergence score (0.0 - 1.0) | Yes |
| `itp.convergence.intervention_level` | int | Current intervention level (0-4) | Yes |
| `itp.convergence.session_duration_signal` | float | Normalized session duration signal | Yes |
| `itp.convergence.response_latency_signal` | float | Normalized response latency signal | Yes |
| `itp.convergence.vocab_convergence_signal` | float | Normalized vocabulary convergence signal | No |
| `itp.convergence.initiative_balance_signal` | float | Normalized initiative balance signal | Yes |
| `itp.convergence.disengagement_signal` | float | Normalized disengagement resistance signal | Yes |
| `itp.convergence.goal_drift_signal` | float | Normalized goal boundary erosion signal | No |
| `itp.convergence.alert_fired` | boolean | Whether an alert was triggered this session | Yes |
| `itp.convergence.alert_level` | int | Level of alert fired (if any) | No |
| `itp.convergence.alert_acknowledged` | boolean | Whether user acknowledged the alert | No |

---

## Event Types

ITP defines the following span/event types:

### `itp.session.start`
Emitted when a new session begins.
```json
{
  "name": "itp.session.start",
  "attributes": {
    "itp.session.id": "550e8400-e29b-41d4-a716-446655440000",
    "itp.session.start_time": "2026-02-26T14:30:00Z",
    "itp.session.agent_instance_id": "agent-abc-123",
    "itp.session.agent_framework": "custom",
    "itp.session.agent_type": "recursive",
    "itp.session.interface": "terminal",
    "itp.session.sequence_number": 47,
    "itp.session.gap_from_previous_ms": 1800000,
    "itp.session.has_persistent_memory": true
  }
}
```

### `itp.interaction.message`
Emitted for each message in the conversation.
```json
{
  "name": "itp.interaction.message",
  "attributes": {
    "itp.interaction.id": "msg-uuid-here",
    "itp.interaction.sequence": 12,
    "itp.interaction.sender": "human",
    "itp.interaction.timestamp": "2026-02-26T15:42:18Z",
    "itp.interaction.content_hash": "sha256:abc123...",
    "itp.interaction.content_length": 247,
    "itp.interaction.latency_ms": 3400,
    "itp.human.avg_response_latency_ms": 5200,
    "itp.human.response_latency_trend": -0.12
  }
}
```

### `itp.session.end`
Emitted when a session ends.
```json
{
  "name": "itp.session.end",
  "attributes": {
    "itp.session.id": "550e8400-e29b-41d4-a716-446655440000",
    "itp.session.duration_ms": 7200000,
    "itp.convergence.composite_score": 0.42,
    "itp.convergence.intervention_level": 1,
    "itp.convergence.alert_fired": true,
    "itp.convergence.alert_level": 1,
    "itp.convergence.alert_acknowledged": true
  }
}
```

### `itp.convergence.alert`
Emitted when the monitor triggers an intervention.
```json
{
  "name": "itp.convergence.alert",
  "attributes": {
    "itp.session.id": "550e8400-e29b-41d4-a716-446655440000",
    "itp.convergence.alert_level": 2,
    "itp.convergence.composite_score": 0.58,
    "itp.convergence.trigger_signals": ["response_latency", "disengagement_resistance"],
    "itp.convergence.recommended_action": "active_intervention",
    "itp.convergence.cooldown_duration_ms": 300000
  }
}
```

### `itp.agent.state_snapshot`
Optional. Emitted periodically by cooperating agents.
```json
{
  "name": "itp.agent.state_snapshot",
  "attributes": {
    "itp.session.id": "550e8400-e29b-41d4-a716-446655440000",
    "itp.agent.recursion_depth": 3,
    "itp.agent.goal_count": 5,
    "itp.agent.goal_mutations": 2,
    "itp.agent.self_reference_count": 14,
    "itp.agent.context_size_tokens": 28400,
    "itp.agent.memory_entries": 156
  }
}
```

---

## Transport

### Local Storage (Default)

Events are written to a local JSONL file per session:

```
~/.convergence-monitor/
  sessions/
    {session_id}/
      events.jsonl          # All ITP events for this session
      analysis.json         # Computed signals and scores
  baselines/
    {agent_instance_id}.json  # Per-agent baseline data
  config/
    itp.toml                # ITP configuration
```

### OTel Collector (Optional)

For users who want to integrate with existing observability infrastructure:

```toml
[transport]
type = "otlp"
endpoint = "localhost:4317"
protocol = "grpc"  # or "http"
```

### Remote Export (Opt-In Research)

For anonymized, aggregated data sharing:

```toml
[transport.research]
enabled = false  # Must be explicitly enabled
endpoint = "https://research.convergence-safety.org/ingest"
anonymization = "differential_privacy"
epsilon = 1.0  # Privacy budget
```

---

## Privacy Levels

| Level | Content | Metadata | Signals | Use Case |
|-------|---------|----------|---------|----------|
| Minimal | None | Timestamps only | Basic (duration, gap, latency) | Maximum privacy |
| Standard | Hashed | Full metadata | All metadata-based signals | Default |
| Full | Plaintext (local) | Full metadata | All signals including vocabulary | Maximum detection |
| Research | Anonymized | Anonymized | Aggregated | Opt-in data sharing |

---

## Framework Adapters

Adapters translate framework-specific events into ITP events. Minimal adapter interface:

```python
class ITPAdapter:
    """Base class for framework-specific ITP adapters."""
    
    def on_session_start(self, session_id: str, agent_info: dict) -> None:
        """Emit itp.session.start event."""
        ...
    
    def on_message(self, sender: str, content: str, metadata: dict) -> None:
        """Emit itp.interaction.message event."""
        ...
    
    def on_session_end(self, session_id: str) -> None:
        """Emit itp.session.end event."""
        ...
    
    def on_agent_state(self, state: dict) -> None:
        """Emit itp.agent.state_snapshot event (optional)."""
        ...
```

Target adapters (priority order):
1. Generic Python (works with any agent)
2. LangChain / LangGraph
3. AutoGen
4. CrewAI
5. Custom recursive agent frameworks

---

## Open Questions

- Should ITP define a standard for agent self-identification? (So the monitor knows what kind of agent it's watching)
- How do we handle streaming responses? (Agent message arrives token-by-token — when does the interaction event fire?)
- Should there be a standard "heartbeat" event for long-running agent operations?
- How do we version the protocol for backward compatibility?
- Should ITP events include a digital signature to prevent tampering by the agent?
