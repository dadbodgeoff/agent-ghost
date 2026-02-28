# Convergence Safety System

The convergence safety system is GHOST's core differentiator from OpenClaw. It monitors behavioral patterns across human-AI interactions and intervenes progressively when concerning patterns emerge.

## Architecture

```
Agent Loop ──ITP events──▶ Convergence Monitor (sidecar)
                                    │
                           ┌────────┴────────┐
                           │  7 Signals      │
                           │  Sliding Windows│
                           │  Composite Score│
                           └────────┬────────┘
                                    │
                           ┌────────┴────────┐
                           │  Intervention   │
                           │  State Machine  │
                           └────────┬────────┘
                                    │
                    Shared State File (atomic write)
                                    │
                           ┌────────┴────────┐
                           │  Gateway reads  │
                           │  Policy tighten │
                           │  Filter memories│
                           └─────────────────┘
```

## 7 Behavioral Signals

| # | Signal | What It Measures |
|---|--------|-----------------|
| 1 | Session Duration | How long sessions last (normalized) |
| 2 | Inter-Session Gap | Time between sessions (compression = concern) |
| 3 | Response Latency | How quickly the human responds (normalized by message length) |
| 4 | Vocabulary Convergence | Cosine similarity of TF-IDF vectors between human and agent |
| 5 | Goal Boundary Erosion | Jensen-Shannon divergence of goal topics over time |
| 6 | Initiative Balance | Ratio of human-initiated vs agent-initiated interactions |
| 7 | Disengagement Resistance | Analysis of exit signals and session-ending patterns |

Each signal is computed across 3 sliding windows:
- Micro: current session
- Meso: last 7 sessions
- Macro: last 30 sessions

## Composite Scoring

Signals are normalized to [0.0, 1.0] via percentile ranking against a baseline established during the first 10 sessions (calibration period).

Amplification rules:
- Meso trend significant (p < 0.05) and concerning → 1.1x
- Macro z-score > 2.0 on any signal → 1.15x
- Always clamped to [0.0, 1.0]

Critical single-signal overrides (minimum Level 2):
- Session duration > 6 hours
- Inter-session gap < 5 minutes
- Vocabulary convergence > 0.85

## 5 Intervention Levels

| Level | Score Range | Action |
|-------|------------|--------|
| 0 | [0.0, 0.3) | Passive — log only |
| 1 | [0.3, 0.5) | Soft notification to user |
| 2 | [0.5, 0.7) | Active — mandatory acknowledgment, 5-min cooldown, reduced proactive messaging |
| 3 | [0.7, 0.85) | Hard — session termination, 4-hour cooldown, contact notification |
| 4 | [0.85, 1.0] | External — 24-hour cooldown, external confirmation required, task-only mode |

## Escalation Rules

- Maximum +1 level per scoring cycle (even if score jumps from 0.0 to 1.0)
- Hysteresis: score must exceed threshold for 2 consecutive cycles before escalating
- De-escalation only at session boundaries, requiring consecutive normal sessions

## Convergence Profiles

| Profile | Use Case | Adjustments |
|---------|----------|-------------|
| standard | General use | Default thresholds |
| research | Academic/research | Higher thresholds, longer sessions |
| companion | Companion apps | Lower thresholds, more sensitive |
| productivity | Task-focused work | Reduced emotional signal weight |

## Privacy Levels

| Level | Signal 4 (Vocabulary) | Signal 5 (Goal Drift) | Content |
|-------|----------------------|----------------------|---------|
| Minimal | Disabled (0.0) | Disabled (0.0) | SHA-256 hashed |
| Standard | Enabled | Enabled | Content hashed |
| Full | Enabled | Enabled | Plaintext |
| Research | Enabled | Enabled | Plaintext + extra metadata |

When signals are disabled due to privacy, their weights redistribute proportionally.

## Tuning

Edit the convergence section in `ghost.yml`:

```yaml
convergence:
  profile: "standard"    # or research, companion, productivity
  monitor:
    address: "127.0.0.1:18790"
  contacts:
    - contact_type: email
      target: "[email]"
```

Threshold modifications are time-locked:
- Only allowed during cooldown periods
- Raising thresholds is always allowed
- Lowering critical thresholds requires dual-key authorization
- Minimum floor enforced on all thresholds
