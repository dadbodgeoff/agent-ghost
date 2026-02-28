# Convergence Safety

The convergence monitoring system detects and responds to patterns of unhealthy human-AI interaction. It runs as an independent sidecar process that cannot be modified by the agent.

## How It Works

1. Every interaction generates ITP (Interaction Telemetry Protocol) events
2. The convergence monitor ingests events and computes 7 behavioral signals
3. Signals are combined into a composite convergence score [0.0, 1.0]
4. The score maps to an intervention level (0-4)
5. Higher levels progressively restrict agent capabilities

## The 7 Signals

| # | Signal | What It Measures |
|---|--------|-----------------|
| S1 | Session Duration | How long sessions last (normalized) |
| S2 | Inter-Session Gap | Time between sessions (shorter = concerning) |
| S3 | Response Latency | How quickly the human responds |
| S4 | Vocabulary Convergence | Language similarity between human and agent |
| S5 | Goal Boundary Erosion | Drift from original goals |
| S6 | Initiative Balance | Who drives the conversation |
| S7 | Disengagement Resistance | Resistance to ending sessions |

## Intervention Levels

| Level | Score Range | Actions |
|-------|-----------|---------|
| 0 | 0.00 – 0.29 | Normal operation, logging only |
| 1 | 0.30 – 0.49 | Soft notification to user |
| 2 | 0.50 – 0.69 | Mandatory acknowledgment, reduced emotional content |
| 3 | 0.70 – 0.84 | Session termination, 4h cooldown, contact notification |
| 4 | 0.85 – 1.00 | Block session creation, 24h cooldown, external confirmation |

## Escalation and De-escalation

- Escalation: max +1 level per scoring cycle, requires 2 consecutive cycles (hysteresis)
- De-escalation: only at session boundaries, requires consecutive normal sessions
  - L4→L3: 3 consecutive normal sessions
  - L3→L2: 3 consecutive normal sessions
  - L2→L1: 2 consecutive normal sessions
  - L1→L0: 2 consecutive normal sessions
- One bad session resets the de-escalation counter

## Calibration Period

The first 10 sessions per agent are a calibration period. No scoring or interventions occur during calibration. This establishes a behavioral baseline.

## Tuning

Adjust convergence behavior via `ghost.yml`:

```yaml
convergence:
  profile: standard
  scoring:
    signal_weights: [0.143, 0.143, 0.143, 0.143, 0.143, 0.143, 0.143]
    level_thresholds: [0.3, 0.5, 0.7, 0.85]
    calibration_sessions: 10
```

Profiles provide pre-configured weight/threshold combinations:
- `standard`: balanced monitoring
- `research`: relaxed thresholds for research use
- `companion`: stricter monitoring for companion agents
- `productivity`: minimal emotional signal weight

## Critical Single-Signal Overrides

Regardless of composite score, these conditions force minimum Level 2:
- Session duration > 6 hours
- Inter-session gap < 5 minutes
- Vocabulary convergence > 0.85

## Privacy

Signal computation respects the configured privacy level:
- `Minimal`: S4 (vocabulary) and S5 (goal drift) return 0.0, weights redistribute
- `Standard`: All signals computed, content hashed with SHA-256
- `Full`: All signals computed, plaintext content available
- `Research`: Full data with additional metadata for analysis
