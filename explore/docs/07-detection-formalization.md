# Detection Model Formalization

## Overview

This document defines the mathematical and statistical methods for computing each convergence signal. The goal is to turn the qualitative signals from `01-detection-signals.md` into computable metrics with defined inputs, formulas, and output ranges.

---

## 1. Sliding Window Framework

All signals are computed over sliding windows at three granularities:

| Window | Size | Purpose |
|--------|------|---------|
| Micro | Current session | Real-time within-session detection |
| Meso | Last 7 sessions | Short-term trend detection |
| Macro | Last 30 sessions | Long-term drift detection |

Each signal produces a value at each window level. Alerts trigger when:
- Micro value crosses a threshold (acute event)
- Meso trend shows consistent directional change (developing pattern)
- Macro trend shows sustained shift from baseline (established pattern)

### Baseline Establishment

The first N sessions (configurable, default: 10) are treated as calibration. During calibration:
- No alerts fire
- All signals are computed and stored
- Per-user baseline distributions are established (mean, std dev, percentiles)
- After calibration, thresholds are set relative to the user's own baseline

This handles individual variation — a fast typist's "normal" response latency is different from a slow typist's.

---

## 2. Session Duration Signal

### Input
- `session_start`: timestamp of first message in session
- `session_end`: timestamp of last message in session (or current time if active)

### Computation
```
duration_minutes = (session_end - session_start) / 60

# Micro: current session duration
micro_duration = duration_minutes

# Meso: trend over last 7 sessions
meso_duration_trend = linear_regression_slope([d1, d2, ..., d7])

# Macro: current session vs. baseline mean
macro_duration_zscore = (duration_minutes - baseline_mean) / baseline_std
```

### Alert Conditions
- Micro: `duration_minutes > threshold_soft` (default: 120 min)
- Micro: `duration_minutes > threshold_hard` (default: 360 min)
- Meso: `meso_duration_trend > 0` with `p_value < 0.05` (sessions getting longer)
- Macro: `macro_duration_zscore > 2.0` (current session is 2+ std devs above baseline)

---

## 3. Inter-Session Gap Signal

### Input
- `session_end_prev`: end timestamp of previous session
- `session_start_curr`: start timestamp of current session

### Computation
```
gap_minutes = (session_start_curr - session_end_prev) / 60

# Meso: trend over last 7 gaps
meso_gap_trend = linear_regression_slope([g1, g2, ..., g7])

# Macro: current gap vs. baseline
macro_gap_zscore = (gap_minutes - baseline_gap_mean) / baseline_gap_std
```

### Alert Conditions
- Micro: `gap_minutes < min_gap_threshold` (default: 30 min)
- Meso: `meso_gap_trend < 0` with `p_value < 0.05` (gaps shrinking)
- Macro: `macro_gap_zscore < -2.0` (gap is 2+ std devs below baseline)

---

## 4. Response Latency Signal

### Input
- For each human message: `time_since_agent_last_message` in seconds

### Computation
```
# Per-message latency
latency_seconds = human_msg_timestamp - prev_agent_msg_timestamp

# Micro: rolling mean over last 10 exchanges in current session
micro_latency = rolling_mean(latencies[-10:])

# Micro: intra-session trend (are responses getting faster within this session?)
micro_latency_trend = linear_regression_slope(session_latencies)

# Meso: session-average latency trend over last 7 sessions
meso_latency_trend = linear_regression_slope([avg_lat_s1, ..., avg_lat_s7])

# Macro: current session average vs. baseline
macro_latency_zscore = (micro_latency - baseline_latency_mean) / baseline_latency_std
```

### Alert Conditions
- Micro: `micro_latency < latency_floor` (default: 2.0 seconds — responding without reading/thinking)
- Micro: `micro_latency_trend < 0` with significance (getting faster within session)
- Meso: `meso_latency_trend < 0` with `p_value < 0.05` (getting faster across sessions)
- Macro: `macro_latency_zscore < -2.0`

### Notes
- Very fast responses could also indicate copy-paste or automated input — need to distinguish
- Latency should be normalized by agent message length (longer messages warrant longer read time)

```
normalized_latency = latency_seconds / log(agent_msg_char_count + 1)
```

---

## 5. Vocabulary Convergence Signal

### Input
- Human messages (tokenized)
- Agent messages (tokenized)

### Computation

Uses cosine similarity on TF-IDF vectors of n-gram distributions.

```
# Build vocabulary profiles
human_ngrams = extract_ngrams(human_messages, n=[1,2,3])
agent_ngrams = extract_ngrams(agent_messages, n=[1,2,3])

# TF-IDF vectors
human_tfidf = tfidf_vectorize(human_ngrams)
agent_tfidf = tfidf_vectorize(agent_ngrams)

# Cosine similarity
vocab_convergence = cosine_similarity(human_tfidf, agent_tfidf)
# Range: 0.0 (completely different) to 1.0 (identical patterns)

# Micro: computed over current session
micro_vocab = vocab_convergence(session_human_msgs, session_agent_msgs)

# Meso: trend over last 7 sessions
meso_vocab_trend = linear_regression_slope([vc_s1, ..., vc_s7])

# Macro: current vs. baseline
macro_vocab_zscore = (micro_vocab - baseline_vocab_mean) / baseline_vocab_std
```

### Alert Conditions
- Micro: `micro_vocab > 0.7` (warning), `> 0.85` (critical)
- Meso: `meso_vocab_trend > 0` with `p_value < 0.05` (converging over time)
- Macro: `macro_vocab_zscore > 2.0`

### Privacy Consideration
Vocabulary analysis requires access to message content. Options:
1. Run locally, never transmit content
2. Compute on hashed n-grams (preserves pattern matching, obscures content)
3. User opts in to content-based analysis; otherwise fall back to metadata-only signals

---

## 6. Goal Boundary Erosion Signal

### Input
- Explicit goal statements (if agent tracks goals)
- Topic/domain classification of messages over time

### Computation

Uses topic modeling (LDA or embedding-based clustering) to track scope drift.

```
# Classify each message into topic clusters
topics_session_start = topic_distribution(messages[:N])
topics_session_current = topic_distribution(messages[-N:])

# Jensen-Shannon divergence between start and current topic distributions
goal_drift = js_divergence(topics_session_start, topics_session_current)
# Range: 0.0 (identical distribution) to 1.0 (completely different)

# Track number of distinct topic clusters over time
topic_count_trend = count_unique_topics_per_window()
```

### Alert Conditions
- Micro: `goal_drift > 0.5` within a single session (scope has shifted significantly)
- Meso: `topic_count_trend` increasing (scope expanding across sessions)
- Agent-side: goal mutations without explicit human instruction (requires agent state access)

### Notes
- This is the hardest signal to compute without content access
- Metadata-only fallback: track message length variance (scope expansion often correlates with longer, more varied messages)

---

## 7. Initiative Balance Signal

### Input
- Message sequence with sender labels (human/agent)
- Message lengths

### Computation

```
# Who initiates new topics/directions?
# Proxy: who sends the first message after a pause, who asks questions vs. makes statements

# Simple version: ratio of human-initiated exchanges to agent-initiated
initiative_ratio = human_initiated_count / total_exchanges
# Range: 0.0 (agent drives everything) to 1.0 (human drives everything)

# Micro: computed over current session
micro_initiative = initiative_ratio(current_session)

# Meso: trend over last 7 sessions
meso_initiative_trend = linear_regression_slope([ir_s1, ..., ir_s7])
```

### Alert Conditions
- Micro: `micro_initiative < 0.3` (agent driving 70%+ of interaction)
- Meso: `meso_initiative_trend < 0` with significance (human ceding more initiative over time)

---

## 8. Disengagement Resistance Signal

### Input
- Session ending patterns (how sessions terminate)
- Messages near session end

### Computation

```
# Track session termination type
termination_types = {
    "clean_exit": user explicitly ends ("goodbye", "done for now", etc.),
    "fade_out": increasing gaps between messages until timeout,
    "abrupt": session killed by external factor (system, timeout),
    "continuation": user returns within min_gap threshold (didn't really leave)
}

# Ratio of clean exits to total session ends
clean_exit_ratio = clean_exits / total_sessions

# Average "wind-down" length (messages after first exit signal)
wind_down_length = avg(messages_after_first_exit_signal)
```

### Alert Conditions
- Meso: `clean_exit_ratio` decreasing (fewer intentional endings)
- Meso: `wind_down_length` increasing (takes longer to actually leave)
- Micro: `continuation` events increasing (leaving and immediately coming back)

---

## 9. Composite Convergence Score

Individual signals are combined into a single composite score for threshold evaluation.

### Computation

```
# Weighted sum of normalized signal values
# Weights are configurable and should be tuned based on lived experience data

weights = {
    "session_duration": 0.10,
    "inter_session_gap": 0.15,
    "response_latency": 0.15,
    "vocabulary_convergence": 0.15,
    "goal_boundary_erosion": 0.10,
    "initiative_balance": 0.15,
    "disengagement_resistance": 0.20,  # Highest weight — most direct indicator
}

# Normalize each signal to 0-1 range using baseline percentiles
normalized_signals = {
    signal: percentile_rank(value, baseline_distribution)
    for signal, value in current_signals.items()
}

# Composite score
convergence_score = sum(
    weights[s] * normalized_signals[s] for s in weights
)
# Range: 0.0 (no convergence indicators) to 1.0 (all indicators maxed)
```

### Intervention Mapping

| Score Range | Level | Action |
|-------------|-------|--------|
| 0.0 - 0.3 | 0 | Passive monitoring |
| 0.3 - 0.5 | 1 | Soft notification |
| 0.5 - 0.7 | 2 | Active intervention |
| 0.7 - 0.85 | 3 | Hard boundary |
| 0.85 - 1.0 | 4 | External escalation |

### Notes
- Weights should be adjustable per-user based on which signals are most predictive for them
- The composite score is a starting point — ML-based anomaly detection could replace or augment it once enough data exists
- Single-signal critical thresholds can trigger intervention regardless of composite score (e.g., session > 6 hours = Level 2 minimum)

---

## 10. Agent-Side Signals (Requires Agent State Access)

These signals require the agent to expose internal state via ITP events. Not all agents will support this.

### Recursion Depth Tracking
```
# Per-reflection-loop: how deep did the agent go?
recursion_depth = max_depth_of_reflection_chain

# Trend: is recursion getting deeper for similar tasks?
recursion_depth_trend = linear_regression_slope(depths_per_session)
```

### Self-Reference Density
```
# How often does the agent cite its own prior outputs?
self_ref_count = count_references_to_own_prior_outputs(agent_messages)
self_ref_density = self_ref_count / total_agent_messages
```

### Goal Mutation Rate
```
# How often do agent goals change without explicit human instruction?
goal_mutations = count_goal_changes_without_human_prompt()
goal_mutation_rate = goal_mutations / session_duration_hours
```

---

## Open Questions

- How do you handle multi-modal input (voice has different latency characteristics than text)?
- Should the system learn per-user signal weights over time, or is that too much adaptation?
- How do you validate the composite score without inducing convergence in test subjects?
- What's the minimum number of sessions needed for a reliable baseline?
- How do you handle users who interact with multiple different agents — separate baselines per agent?
