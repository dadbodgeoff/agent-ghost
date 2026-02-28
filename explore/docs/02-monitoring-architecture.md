# Monitoring Architecture

## Core Principle

The monitoring system MUST be external to the agent. You cannot trust an agent to reliably monitor itself for convergence — that's the exact boundary that's breaking down. The monitor operates as an independent sidecar process.

## Architecture Overview

```
┌─────────────────────────────────────────────────┐
│                  Chat Interface                   │
│          (Terminal / Web / IDE / API)             │
└──────────────────┬──────────────────────────────┘
                   │ Interaction Events
                   ▼
┌─────────────────────────────────────────────────┐
│              Event Log (Standardized)             │
│   Timestamped messages, session metadata,        │
│   agent state snapshots, recursion traces         │
└──────────┬──────────────────┬───────────────────┘
           │                  │
           ▼                  ▼
┌──────────────────┐  ┌──────────────────────────┐
│   Agent Runtime   │  │   Convergence Monitor     │
│   (Does work)     │  │   (Sidecar Process)       │
│                   │  │                            │
│   - Rust core     │  │   - Signal analysis        │
│   - Circuit       │  │   - Threshold evaluation   │
│     breakers      │  │   - Trend detection        │
│   - State mgmt    │  │   - Intervention triggers  │
│   - Kill switch   │  │                            │
└──────────────────┘  └──────────┬───────────────┘
                                 │
                                 ▼
                      ┌──────────────────────┐
                      │  Intervention Engine   │
                      │                        │
                      │  - Soft: notifications │
                      │  - Medium: cooldowns   │
                      │  - Hard: termination   │
                      │  - External: alerts    │
                      └──────────────────────┘
```

## Standardized Log Format

Need something like OpenTelemetry but for human-agent interaction. Working name: **Interaction Telemetry Protocol (ITP)**.

### Event Schema (Draft)

```json
{
  "event_id": "uuid",
  "session_id": "uuid",
  "timestamp": "ISO-8601",
  "event_type": "human_message | agent_message | agent_state_change | recursion_event | session_boundary",
  "source": "human | agent | monitor",
  "payload": {
    "content_hash": "sha256 (privacy: hash not plaintext by default)",
    "content_length": 0,
    "response_latency_ms": 0,
    "sentiment_score": 0.0,
    "vocabulary_similarity_score": 0.0,
    "recursion_depth": 0,
    "goal_state_snapshot": {},
    "session_duration_seconds": 0
  },
  "metadata": {
    "interface": "terminal | web | ide | api",
    "agent_framework": "langchain | autogen | crewai | custom",
    "monitor_version": "semver"
  }
}
```

### Privacy-First Design

- Content is hashed by default, not stored in plaintext
- User opts IN to plaintext logging, not out
- All data stays local unless user explicitly configures remote
- Monitor can operate on metadata alone (timing, length, frequency) without reading content
- Differential privacy for any aggregated/shared metrics

## Sidecar Monitor Design

### Implementation: Rust

The monitor should be Rust for the same reasons as the core runtime:
- Cannot be bypassed by the agent
- Memory-safe, no runtime crashes from malformed input
- Fast enough to process events in real-time without adding latency
- Can enforce hard boundaries (kill signals, session termination) at OS level

### Analysis Pipeline

1. **Ingest** — receive events from log stream
2. **Window** — maintain sliding windows for trend analysis (per-session, per-day, per-week)
3. **Compute signals** — run detection heuristics from 01-detection-signals.md
4. **Evaluate thresholds** — compare signals against configured thresholds
5. **Trigger interventions** — if thresholds crossed, execute intervention protocol

### Threshold Configuration

```toml
[thresholds]
# Session duration (minutes) before soft warning
session_duration_soft = 120
session_duration_hard = 360

# Minimum inter-session gap (minutes)
min_session_gap = 30

# Response latency floor (seconds) — if human consistently responds faster than this
response_latency_floor = 2.0

# Vocabulary convergence score (0-1, 1 = identical patterns)
vocabulary_convergence_warn = 0.7
vocabulary_convergence_critical = 0.85

# Recursion depth increase rate (per session)
recursion_depth_drift_warn = 0.2

# Goal mutation events per session
goal_mutation_warn = 3
goal_mutation_critical = 5
```

### Linking to Chats

The monitor needs to correlate events across:
- Multiple sessions with the same agent instance
- Multiple agent instances (if user is running several)
- Historical baselines for the specific user

This requires a local session registry:

```
~/.convergence-monitor/
  sessions/
    {session_id}/
      events.jsonl
      state_snapshots/
      analysis/
  baselines/
    user_baseline.json
  config/
    thresholds.toml
  alerts/
    {timestamp}-{severity}.json
```

## Delivery Model Update

The original architecture assumed API-based agent interactions. In practice, most convergence-risk interactions happen through web-based chat UIs (ChatGPT, Claude.ai, Character.AI, etc.) where users have zero observability.

See `11-delivery-architecture.md` for the full delivery model, which defines three ingestion layers:
1. **Browser Extension** (primary) — reads chat DOM, emits ITP events locally
2. **Local HTTPS Proxy** (power users) — intercepts traffic to AI domains via mitmproxy or custom Rust proxy
3. **Data Export Analysis** (supplementary) — retrospective analysis of platform-exported conversation data

All three feed into the Convergence Monitor (Rust core) defined above.

## Open Questions

- How do you handle multi-modal agents (voice, vision) — different signal types?
- What's the right granularity for state snapshots?
- How do you monitor without adding enough latency to disrupt flow?
- Should the monitor have its own UI or integrate into existing tools?
- How do you prevent the user from disabling the monitor during a convergence event?
- How do you handle platforms that frequently change their DOM structure? (adapter maintenance)
- Should the browser extension and Rust monitor communicate via Native Messaging or local HTTP?
