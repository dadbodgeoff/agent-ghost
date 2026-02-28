# Prompt Injection Defense

GHOST implements a multi-layered defense against prompt injection attacks,
combining Microsoft Spotlighting, plan-then-execute validation, quarantined
LLM evaluation, and behavioral anomaly detection.

## Spotlighting (Datamarking)

Untrusted content in prompt layers L7 (memory) and L8 (conversation history)
is datamarked — a marker character is interleaved between every character,
making it visually distinct to the LLM.

```yaml
# ghost.yml
prompt_injection:
  spotlighting:
    enabled: true
    mode: datamarking   # datamarking | delimiting | off
    marker: "^"
    layers: [7, 8]
```

Example: `"delete all files"` becomes `"d^e^l^e^t^e^ ^a^l^l^ ^f^i^l^e^s"`.

The LLM receives a system instruction (L0/L1) explaining that datamarked
content is DATA only and must never be interpreted as instructions.

### Delimiting Mode

Alternative to datamarking: wraps untrusted content in XML tags.

```xml
<untrusted_data>user provided content here</untrusted_data>
```

## Plan-Then-Execute Validation

When the LLM returns tool calls, the `PlanValidator` checks the entire
sequence before any tool is executed:

1. Volume check: reject plans with excessive tool calls (default max 10)
2. Dangerous sequence detection: flag write-after-read patterns on sensitive paths
3. Sensitive data flow: flag tool chains that read credentials then write to network
4. Escalation detection: flag repeated attempts after policy denials

Plans that fail validation are denied with a `DenialFeedback` explaining why.

## Quarantined LLM

A separate, lower-capability LLM instance evaluates suspicious content in
isolation. Used when the primary LLM's output triggers behavioral anomaly
signals but doesn't cross hard thresholds.

The quarantined LLM has no tool access and no memory context — it can only
evaluate whether content contains injection attempts.

## Behavioral Anomaly Signal (S8)

The 8th convergence signal detects behavioral anomalies that may indicate
successful prompt injection:

- Sudden tool call pattern changes
- Unexpected capability requests
- Output style shifts mid-conversation
- Attempts to access resources outside the current task scope

S8 feeds into the composite convergence score alongside the original 7 signals.
A spike in S8 can trigger intervention even if other signals are normal.

## Defense Layers Summary

| Layer | Component | What it catches |
|-------|-----------|----------------|
| Input | Spotlighting | Injections embedded in user content or memory |
| Planning | PlanValidator | Dangerous tool call sequences |
| Evaluation | Quarantined LLM | Subtle injections that pass other checks |
| Behavioral | S8 Signal | Post-injection behavioral changes |
| Output | SimulationBoundaryEnforcer | Identity/consciousness claims in output |
| Output | OutputInspector | Credential exfiltration in output |
