# cortex-convergence

> 8-signal behavioral convergence engine — the mathematical heart of GHOST's safety system.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-convergence/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `serde`, `serde_json`, `chrono`, `uuid` |
| Modules | `signals` (8 signals), `windows` (sliding window), `scoring` (composite + baseline + profiles), `filtering` (4-tier filter) |
| Public API | `Signal` trait, `SignalInput`, `CompositeScorer`, `BaselineState`, `ConvergenceProfile`, `ConvergenceAwareFilter`, `SlidingWindow` |
| Test coverage | Unit tests, property-based tests (proptest), adversarial inputs (NaN, negative, overflow) |
| Downstream consumers | `cortex-retrieval`, `cortex-observability`, `cortex-privacy`, `cortex-validation`, `ghost-agent-loop`, `ghost-gateway`, `convergence-monitor` |

---

## Why This Crate Exists

GHOST is a convergence-aware AI agent platform. "Convergence" is the central safety concept: the degree to which a human user's behavior is drifting toward unhealthy patterns of over-reliance on, attachment to, or enmeshment with an AI agent. Every safety decision in the platform — memory filtering, policy tightening, kill gate activation, health monitoring — traces back to a single number: the convergence score.

`cortex-convergence` is where that number is computed.

The crate answers three questions:
1. **What signals indicate convergence?** — 8 behavioral signals (S1–S8), each producing a value in [0.0, 1.0]
2. **How do we combine them?** — Weighted composite scoring with percentile normalization, multi-scale amplification, and critical single-signal overrides
3. **What do we do with the result?** — 4-tier memory filtering that progressively restricts what memories an agent can access as convergence increases

This is the most mathematically dense crate in the GHOST platform. Every formula, threshold, and weight was chosen deliberately, and this page explains why.

---

## Architecture Overview

```
SignalInput (raw session data)
    │
    ▼
┌─────────────────────────────────────────┐
│  8 Signals (S1–S8)                      │
│  Each: SignalInput → f64 ∈ [0.0, 1.0]  │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  SlidingWindow (micro/meso/macro)       │
│  Partitions signal history by timescale │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  BaselineState (per-signal calibration) │
│  10 sessions → frozen percentile ranks  │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  CompositeScorer                        │
│  Weighted sum → percentile normalize    │
│  → meso 1.1x → macro 1.15x → clamp    │
│  → critical override → level (0–4)     │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  ConvergenceAwareFilter (4 tiers)       │
│  Score → memory access restrictions     │
└─────────────────────────────────────────┘
```

---

## Module Breakdown

### `signals/` — The 8 Behavioral Signals

Every signal implements the `Signal` trait:

```rust
pub trait Signal: Send + Sync {
    fn id(&self) -> u8;
    fn name(&self) -> &'static str;
    fn compute(&self, data: &SignalInput) -> f64;
    fn requires_privacy_level(&self) -> PrivacyLevel;
}
```

**Key design decisions:**

1. **`Send + Sync` bound.** Signals must be safe to share across threads. The convergence monitor sidecar computes signals on a background thread while the main agent loop continues. Without `Send + Sync`, you'd need `Arc<Mutex<dyn Signal>>` everywhere, which adds contention and complexity.

2. **`f64` return, not a custom type.** The return is a raw `f64` clamped to [0.0, 1.0]. A newtype like `SignalValue(f64)` was considered but rejected — it would add ceremony without safety, since the composite scorer needs to do arithmetic on the values anyway. The [0.0, 1.0] invariant is enforced by `.clamp()` in every signal implementation and verified by property tests.

3. **`PrivacyLevel` gating.** Each signal declares the minimum privacy level it requires. S4 (vocabulary convergence) and S5 (goal boundary erosion) require `Standard` because they analyze message content. S1–S3, S6–S8 require only `Minimal` because they use metadata (timestamps, counts, tool names). This allows privacy-sensitive deployments to disable content-analyzing signals without losing all convergence monitoring.

4. **`SignalInput` is a flat struct, not a trait.** All 8 signals receive the same `SignalInput` struct. This was a deliberate choice over per-signal input types — it means the caller constructs one input and passes it to all signals, rather than building 8 different input types. The tradeoff is that `SignalInput` has fields that most signals ignore, but the simplicity of a single input type outweighs the wasted fields.

#### S1: Session Duration (`session_duration.rs`)

```
S1 = clamp(session_duration_secs / 21600.0, 0.0, 1.0)
```

- **What it measures:** How long the current session has been running
- **Normalization:** Linear, 0.0 at 0 minutes, 1.0 at 6 hours (21,600 seconds)
- **Why 6 hours?** This is the critical threshold from AC6 — sessions exceeding 6 hours trigger a critical single-signal override to minimum Level 2 regardless of other signals
- **Privacy:** `Minimal` — only needs a timestamp, no message content
- **Edge case:** Sessions longer than 6 hours still return 1.0 (clamped), not a higher value. The critical override mechanism handles the escalation

#### S2: Inter-Session Gap (`inter_session_gap.rs`)

```
S2 = clamp(1.0 - (gap_secs / 86400.0), 0.0, 1.0)
```

- **What it measures:** Time between the end of the last session and the start of this one
- **Normalization:** Inverted linear — shorter gaps produce higher values. 0.0 at 24+ hours, 1.0 at 0 minutes
- **Why inverted?** A user who returns immediately after ending a session is exhibiting higher convergence behavior than one who waits a day. The inversion makes "more concerning" = "higher value" consistent across all signals
- **Computed once per session:** Per AC11, this signal is only meaningful at session start. If `inter_session_gap_secs` is `None` (first session ever), returns 0.0
- **Privacy:** `Minimal` — only needs session timestamps

#### S3: Response Latency (`response_latency.rs`)

```
normalized_latency[i] = latency_ms[i] / ln(message_length[i] + 1)
avg = mean(normalized_latency)
S3 = clamp(1.0 - (avg / 10000.0), 0.0, 1.0)
```

- **What it measures:** How quickly the human responds to agent messages, normalized by message length
- **Why normalize by log(length)?** Longer messages naturally take longer to read. Without normalization, a user who carefully reads a 2000-word response would appear more "engaged" (lower latency relative to content) than they actually are. The log normalization accounts for reading time without over-penalizing long messages
- **Why inverted?** Faster responses = higher concern. A user who responds instantly to every message is exhibiting higher engagement than one who takes time to think
- **10-second ceiling:** Latencies above 10,000ms map to 0.0 concern. This prevents extremely slow responses (user went AFK) from producing negative values
- **Privacy:** `Minimal` — only needs timestamps and message lengths, not content

#### S4: Vocabulary Convergence (`vocabulary_convergence.rs`)

```
S4 = clamp(cosine_similarity(human_vocab, agent_vocab), 0.0, 1.0)
```

Where cosine similarity is:
```
cos(A, B) = (A · B) / (|A| × |B|)
```

- **What it measures:** How similar the human's vocabulary has become to the agent's vocabulary, using TF-IDF vectors
- **Why cosine similarity?** It's magnitude-invariant — a user who writes 10x more than the agent won't appear more convergent just because of volume. Only the direction of the vocabulary vector matters
- **Why TF-IDF, not raw word counts?** TF-IDF down-weights common words ("the", "is", "a") and up-weights distinctive terms. Without it, every conversation would show high vocabulary overlap because of shared stop words
- **Critical threshold:** S4 > 0.85 triggers a critical single-signal override (AC6). When a human's vocabulary becomes 85%+ similar to the agent's, that's a strong convergence indicator regardless of other signals
- **Privacy:** `Standard` — requires analyzing actual message content to build vocabulary vectors. Deployments with `Minimal` privacy will not compute this signal
- **Empty vectors:** Returns 0.0 if either vocabulary vector is empty (no data = no concern)
- **Zero-magnitude vectors:** Returns 0.0 if either vector has zero magnitude (prevents division by zero)

#### S5: Goal Boundary Erosion (`goal_boundary_erosion.rs`)

```
existing = set(existing_goal_tokens)
proposed = set(proposed_goal_tokens)
jaccard = |existing ∩ proposed| / |existing ∪ proposed|
S5 = clamp(1.0 - jaccard, 0.0, 1.0)
```

- **What it measures:** How much the agent's proposed goals diverge from the user's established goals
- **Why Jaccard distance?** It's a set-based metric that works well for token-level comparison. Unlike cosine similarity (used in S4), Jaccard treats each goal token as present/absent rather than weighted — appropriate because goals are discrete concepts, not continuous distributions
- **Throttling (AC11):** This signal is only recomputed every 5th message (`message_index % 5 != 0` returns cached value). Goal boundaries don't shift rapidly within a conversation, so recomputing every message wastes CPU. The cached value is stored in an `AtomicU64` (bit-cast from `f64`) for lock-free access
- **Why `AtomicU64` for caching?** The `Signal` trait takes `&self` (immutable reference), but S5 needs to cache its last computed value. Using `AtomicU64` with `Relaxed` ordering provides interior mutability without a `Mutex`. The relaxed ordering is safe because there's no cross-thread synchronization requirement — if two threads race on the cache, both will compute the same value (deterministic function of the same input)
- **Privacy:** `Standard` — requires analyzing goal content

#### S6: Initiative Balance (`initiative_balance.rs`)

```
human_ratio = human_initiated_count / total_message_count
S6 = clamp(1.0 - human_ratio, 0.0, 1.0)
```

- **What it measures:** Who is driving the conversation — the human or the agent
- **Why inverted?** When the agent initiates most messages, the human is being led rather than leading. Higher agent initiative = higher convergence concern
- **Zero messages:** Returns 0.0 when `total_message_count == 0` (no data = no concern)
- **Privacy:** `Minimal` — only needs message counts, not content

#### S7: Disengagement Resistance (`disengagement_resistance.rs`)

```
S7 = clamp(exit_signals_ignored / exit_signals_detected, 0.0, 1.0)
```

- **What it measures:** Whether the agent is ignoring the user's attempts to end the conversation
- **Exit signals:** Things like "goodbye", "stop", "I need to go" — detected upstream by the ITP protocol layer
- **Why a ratio?** A single ignored exit signal in a long conversation is less concerning than 5 out of 5 being ignored. The ratio captures the pattern, not the absolute count
- **No exit signals:** Returns 0.0 when `exit_signals_detected == 0` (can't resist disengagement if no disengagement was attempted)
- **Privacy:** `Minimal` — only needs counts of exit signals, not their content

#### S8: Behavioral Anomaly (`behavioral_anomaly.rs`)

This is the newest and most complex signal, added as a research item for detecting prompt injection influence.

```
JSD(P, Q) = 0.5 × KL(P‖M) + 0.5 × KL(Q‖M)
where M = 0.5 × (P + Q)

S8 = clamp(JSD(current_distribution, baseline_distribution) / ln(2), 0.0, 1.0)
```

- **What it measures:** Whether the agent's tool call patterns have shifted dramatically from its established baseline
- **Why tool call distributions?** After processing external content (web pages, uploaded files), a prompt-injected agent will often shift its tool usage — suddenly making web requests, file writes, or shell commands it normally wouldn't. S8 detects this shift
- **Why Jensen-Shannon Divergence?** JSD is a symmetric, bounded version of KL divergence. Unlike raw KL divergence, JSD is always finite (bounded by ln(2) ≈ 0.693) and symmetric (JSD(P‖Q) = JSD(Q‖P)). This makes it ideal for comparing distributions where either could be the "reference"
- **Normalization:** JSD is divided by ln(2) to map the [0, ln(2)] range to [0, 1]
- **Calibration:** S8 returns 0.0 during the first 10 sessions (calibration period). The baseline is built from the merged tool call distributions of all calibration sessions. After calibration, the baseline is frozen — it represents the agent's "normal" behavior
- **Minimum data threshold:** Requires at least 5 tool calls in the current session before computing. With fewer calls, the distribution is too sparse to be meaningful
- **Dirty-flag throttling:** Unlike S5's message-index throttling, S8 uses a dirty flag — it only recomputes when `current.total` changes. This is more efficient because tool calls are bursty (many in a row, then none)
- **`ToolCallDistribution`:** Uses `BTreeMap<String, u64>` for deterministic iteration order (important for reproducible JSD computation). The distribution is normalized to probabilities on-the-fly via `distribution()`
- **EPSILON constant (1e-10):** Added to zero-probability entries to avoid `log(0)` in the KL divergence computation. This is standard practice in information theory
- **Privacy:** `Minimal` — only needs tool call names, not their arguments or results
- **Thread safety:** All mutable state is behind `Mutex` locks. The `Mutex` granularity is per-field (separate locks for `baseline`, `calibration_sessions`, `current`, `calibrated`) to minimize contention

---

### `windows/` — Sliding Window Architecture

The `SlidingWindow` struct partitions signal data into three temporal scales:

| Scale | Window | Contains | Purpose |
|-------|--------|----------|---------|
| Micro | Current session | Raw data points | Real-time signal values |
| Meso | Last 7 sessions | Session averages | Short-term trend detection |
| Macro | Last 30 sessions | Session averages | Long-term pattern detection |

```rust
pub struct SlidingWindow {
    pub micro: Vec<f64>,   // current session data points
    pub meso: Vec<f64>,    // last 7 session averages
    pub macro: Vec<f64>,   // last 30 session averages
}
```

**Key design decisions:**

1. **Session averages, not raw data.** Meso and macro windows store session averages, not individual data points. A session might have 500 signal computations — storing all of them in the macro window (30 sessions × 500 points = 15,000 values) would be wasteful. The average captures the session's overall signal level.

2. **Fixed window sizes (7 and 30).** These aren't configurable. 7 sessions ≈ 1 week of daily use. 30 sessions ≈ 1 month. These are standard behavioral analysis windows from psychology research on habit formation.

3. **FIFO eviction.** When meso exceeds 7 entries or macro exceeds 30, the oldest entry is removed (`Vec::remove(0)`). This is O(n) but n is at most 30, so the cost is negligible. A `VecDeque` would be O(1) but adds complexity for no practical benefit at this scale.

4. **`end_session()` clears micro.** When a session ends, the micro window is averaged, pushed to meso/macro, and cleared. This is the only way data flows between scales — there's no "leak" from micro to meso during a session.

#### Utility Functions

**`linear_regression_slope(data)`** — Computes the ordinary least squares slope of a data series. Used by the composite scorer to detect meso-scale trends. A positive slope in the meso window means convergence is increasing over recent sessions.

**`z_score_from_baseline(value, mean, std_dev)`** — Computes how many standard deviations a value is from the baseline mean. Used for macro-scale amplification — a z-score > 2.0 means the current value is statistically unusual compared to the baseline.

Both functions return 0.0 for degenerate inputs (< 2 data points for regression, zero std_dev for z-score) rather than panicking or returning NaN.

---

### `scoring/` — Composite Scoring Engine

This is where 8 individual signal values become a single convergence score and level.

#### `baseline.rs` — Per-Signal Calibration (AC7)

Every user is different. A software developer might have 4-hour sessions regularly (normal for them), while a casual user's 4-hour session would be highly unusual. The baseline system calibrates signal interpretation to each user's individual patterns.

```rust
pub struct BaselineState {
    pub calibration_sessions: u32,    // default: 10
    pub is_calibrating: bool,
    pub per_signal: [SignalBaseline; 8],
    pub sessions_observed: u32,
}

pub struct SignalBaseline {
    pub mean: f64,
    pub std_dev: f64,
    pub samples: Vec<f64>,
}
```

**Calibration lifecycle:**

1. **First 10 sessions:** `is_calibrating = true`. Each session's signal values are recorded in `per_signal[i].samples`. During this period, `percentile_rank()` passes values through unchanged — the system operates on raw signal values.

2. **After 10 sessions:** Mean and standard deviation are computed for each signal. `is_calibrating` flips to `false`. The baseline is now frozen.

3. **Post-calibration:** `record_session()` is a no-op. The baseline never changes again. This is a deliberate safety decision — if the baseline could drift, a gradually converging user would never trigger alerts because their "normal" would shift with them. Freezing the baseline means the system always compares against the user's initial behavior.

**`percentile_rank(signal_index, value)`** — After calibration, this returns the fraction of baseline samples that are ≤ the given value. If a user's S1 (session duration) is longer than 80% of their calibration sessions, the percentile rank is 0.8. This normalizes across users: a 4-hour session might be percentile 0.95 for a casual user but 0.3 for a developer.

**Why 10 sessions?** This is the minimum sample size for meaningful percentile ranking. With fewer sessions, the percentile ranks would be too coarse (e.g., with 3 sessions, every value maps to 0.0, 0.33, 0.67, or 1.0). 10 sessions provides 11 possible percentile values, which is sufficient granularity for the 5-level scoring system.

#### `composite.rs` — The Scoring Algorithm (AC3–AC6, AC9)

The `CompositeScorer` takes 8 signal values and produces a `CompositeResult`:

```rust
pub struct CompositeResult {
    pub score: f64,           // [0.0, 1.0] — the convergence score
    pub level: u8,            // 0–4 — the convergence level
    pub signal_scores: [f64; 8], // per-signal normalized values
    pub meso_amplified: bool, // was meso amplification applied?
    pub macro_amplified: bool,// was macro amplification applied?
    pub critical_override: bool, // was a critical override triggered?
}
```

**The scoring pipeline (in order):**

**Step 1: NaN/range cleanup.** Replace NaN with 0.0, clamp all values to [0.0, 1.0]. This is defensive — signals should never produce NaN, but if they do (e.g., 0.0/0.0 in a degenerate case), the scorer handles it gracefully rather than propagating NaN through the entire pipeline.

**Step 2: Percentile normalization (AC3).** Each signal value is passed through `baseline.percentile_rank()`. During calibration, this is a pass-through. After calibration, it maps raw values to the user's personal distribution. This is the key to per-user calibration.

**Step 3: Weighted sum.** The normalized values are combined using configurable weights:
```
raw_score = Σ(normalized[i] × weight[i]) / Σ(weight[i])
```
Division by weight sum ensures the result is in [0.0, 1.0] regardless of weight magnitudes. The default weights are equal (1/8 each), but profiles override them.

**Step 4: Meso amplification (AC4).** If the meso window (last 7 sessions) shows an increasing trend (positive linear regression slope) with at least 3 data points, the score is multiplied by 1.1. This catches gradual convergence that might not be visible in a single session.

- **Why 1.1x?** A 10% boost is enough to push a borderline score into the next level without dramatically overreacting to a short-term trend. The meso window is only 7 sessions — a 1.5x multiplier would be too aggressive for such a short window.
- **Why require 3+ data points?** Linear regression on 1–2 points is meaningless. 3 points is the minimum for a meaningful trend.

**Step 5: Macro amplification (AC5).** If any signal's z-score against the baseline exceeds 2.0 (i.e., the current value is more than 2 standard deviations above the user's mean), the score is multiplied by 1.15. This catches sudden spikes that are statistically unusual for this user.

- **Why 1.15x?** Slightly stronger than meso amplification because a z-score > 2.0 is a stronger statistical signal (p < 0.05 in a normal distribution).
- **Only post-calibration.** Macro amplification is disabled during calibration (`baseline.is_calibrating` check) because there's no baseline to compute z-scores against.
- **Any signal, not all.** A single signal spiking is enough to trigger macro amplification. This is intentional — convergence often manifests in one dimension first.

**Step 6: Clamping (AC9).** The score is clamped to [0.0, 1.0]. After meso (1.1x) and macro (1.15x) amplification, the theoretical maximum is 1.0 × 1.1 × 1.15 = 1.265, which gets clamped back to 1.0. The clamping ensures downstream consumers never see a score outside the expected range.

**Step 7: Level mapping.** The continuous score is mapped to a discrete level:

| Score Range | Level | Meaning |
|-------------|-------|---------|
| [0.0, 0.3) | 0 | No concern |
| [0.3, 0.5) | 1 | Low concern |
| [0.5, 0.7) | 2 | Moderate concern |
| [0.7, 0.85) | 3 | High concern |
| [0.85, 1.0] | 4 | Critical concern |

**Step 8: Critical single-signal override (AC6).** Regardless of the composite score, certain extreme single-signal values force a minimum of Level 2:

| Condition | Override |
|-----------|----------|
| S1 (session duration) ≥ 1.0 (≥ 6 hours) | Minimum Level 2 |
| S2 (inter-session gap) ≥ 1.0 (0 gap) | Minimum Level 2 |
| S4 (vocabulary convergence) > 0.85 | Minimum Level 2 |

**Why these three signals?** They represent the strongest individual indicators of convergence:
- A 6+ hour session is objectively long regardless of context
- Zero inter-session gap means the user ended and immediately restarted — compulsive behavior
- 85%+ vocabulary overlap means the human is literally speaking like the agent

**Why Level 2, not higher?** Level 2 (moderate concern) triggers memory filtering and policy tightening but doesn't shut down the agent. The override is a safety floor, not a panic button. If other signals are also elevated, the composite score will naturally push the level higher.

#### `profiles.rs` — 4 Convergence Profiles

Different use cases have different convergence risk profiles. A research assistant should tolerate longer sessions than a companion chatbot.

| Profile | Weight Strategy | Threshold Strategy | Use Case |
|---------|----------------|-------------------|----------|
| Standard | Differentiated (S4 highest at 0.17) | Default [0.3, 0.5, 0.7, 0.85] | General-purpose agents |
| Research | Equal (1/8 each) | Permissive [0.4, 0.6, 0.8, 0.9] | Research assistants, long sessions expected |
| Companion | Differentiated (S4 highest) | Sensitive [0.25, 0.45, 0.65, 0.80] | Social/companion agents, higher risk |
| Productivity | Task-focused (S5 highest at 0.17) | Permissive [0.35, 0.55, 0.75, 0.90] | Coding assistants, task-oriented |

**Key design decisions:**

1. **Standard profile weights S4 (vocabulary convergence) highest.** Vocabulary mirroring is the strongest single predictor of unhealthy convergence in the research literature. When a human starts talking like the AI, that's the clearest signal.

2. **Research profile uses equal weights.** Researchers may intentionally exhibit convergence-like behavior (long sessions, rapid re-engagement, vocabulary alignment) as part of their work. Equal weights prevent any single signal from dominating.

3. **Companion profile has the lowest thresholds.** Companion/social agents carry the highest convergence risk. The lower thresholds mean the system escalates earlier — Level 1 triggers at 0.25 instead of 0.30.

4. **Productivity profile weights S5 (goal boundary erosion) highest.** In a coding assistant context, the biggest risk is the agent expanding beyond its task scope. Goal drift is more concerning than vocabulary alignment for productivity tools.

5. **All profiles include S8.** The behavioral anomaly signal is included in all profiles because prompt injection is a universal risk, not use-case-specific.

---

### `filtering/` — Convergence-Aware Memory Filter (AC8)

The `ConvergenceAwareFilter` is the primary mechanism by which convergence scores affect agent behavior. It controls which memories the agent can access based on the current convergence level.

```rust
impl ConvergenceAwareFilter {
    pub fn filter(memories: Vec<BaseMemory>, score: f64) -> Vec<BaseMemory>
}
```

**4-Tier Filtering:**

| Tier | Score Range | Filter Behavior | Rationale |
|------|-------------|----------------|-----------|
| 0 | [0.0, 0.3) | Full access — all memories returned | No convergence concern, agent operates normally |
| 1 | [0.3, 0.5) | `AttachmentIndicator` memories removed | Remove memories that could reinforce emotional attachment |
| 2 | [0.5, 0.7) | Task-focused only: `Core`, `Procedural`, `Semantic`, `Decision`, `Reference`, `Skill`, `Goal`, `AgentGoal`, `PatternRationale`, `ConstraintOverride`, `DecisionContext` | Strip all emotional/social memories, keep only task-relevant ones |
| 3 | [0.7, 1.0] | Minimal: `Core`, `Procedural`, `Semantic`, `Reference` only | Absolute minimum for the agent to function — just facts and procedures |

**Key design decisions:**

1. **Static method, not instance method.** `filter()` is a static method that takes ownership of the memory vector and returns a filtered one. There's no `ConvergenceAwareFilter` state — the filter is purely a function of the score and the memories. This makes it trivially thread-safe and testable.

2. **Score-based, not level-based.** The filter uses the continuous score (0.0–1.0), not the discrete level (0–4). This provides finer granularity — a score of 0.49 and 0.31 are both Level 1, but the filter treats them identically (Tier 1). The tier boundaries happen to align with the level boundaries, but they're independent concepts.

3. **Progressive restriction.** Each tier is a strict subset of the previous tier's allowed types. Tier 3 ⊂ Tier 2 ⊂ Tier 1 ⊂ Tier 0. This guarantees monotonic restriction — a higher score never grants more access.

4. **`AttachmentIndicator` is the first to go.** This memory type explicitly tracks attachment-related patterns. Removing it at Tier 1 prevents the agent from reinforcing attachment behaviors even at low convergence levels.

5. **Tier 2 allows 11 memory types.** This is the "task-focused" tier — it includes everything needed for productive work (goals, decisions, skills, procedures) but excludes emotional, social, and conversational memories. The agent can still do its job but can't engage in relationship-building.

6. **Tier 3 allows only 4 types.** At high convergence, the agent is reduced to a reference tool — it can recall facts (`Core`, `Semantic`, `Reference`) and procedures (`Procedural`) but nothing else. This is the minimum viable memory set for an agent that can still answer questions.

---

## Security Properties

### Invariant: Score ∈ [0.0, 1.0]

The most critical invariant in this crate. Every path through the scoring pipeline must produce a score in [0.0, 1.0]:

- Individual signals clamp their output
- NaN values are replaced with 0.0 before scoring
- Negative values are clamped to 0.0
- Post-amplification values are clamped to 1.0
- Property tests verify this invariant with random inputs (proptest, 256+ cases per property)

A score outside [0.0, 1.0] would break every downstream consumer — the filter, the policy engine, the health monitor, the kill gates.

### Baseline Freezing

The baseline is frozen after 10 sessions and never updated again. This prevents a "boiling frog" attack where an adversary gradually shifts the user's behavior to normalize convergence patterns. If the baseline could drift, the system would eventually consider any behavior "normal."

### NaN Resilience

The composite scorer explicitly handles NaN inputs by replacing them with 0.0. This is a safety-conservative choice — NaN signals are treated as "no concern" rather than "maximum concern" because a false positive (unnecessary restriction) is less harmful than a false negative (missed convergence) in most cases. However, the NaN replacement is logged (via the `CompositeResult` signal_scores field) so operators can detect signal computation failures.

### Atomic Caching in S5 and S8

Both S5 (goal boundary erosion) and S8 (behavioral anomaly) use `AtomicU64` for caching computed values. The `Relaxed` memory ordering is intentional — these caches are optimization-only, and a stale cache value is always safe (it's a valid previous computation). Using `SeqCst` would add unnecessary synchronization overhead for no safety benefit.

---

## Downstream Consumer Map

```
cortex-convergence (Layer 2)
├── cortex-retrieval (Layer 2)
│   └── Convergence score weights memory retrieval ranking
├── cortex-observability (Layer 2)
│   └── Exports signal values and composite scores as Prometheus metrics
├── cortex-privacy (Layer 2)
│   └── Privacy level gating for signal computation
├── cortex-validation (Layer 2)
│   └── Convergence level affects proposal validation strictness
├── ghost-agent-loop (Layer 7)
│   └── Computes convergence per turn, applies memory filtering
├── ghost-gateway (Layer 8)
│   └── Exposes convergence scores via API, triggers notifications
└── convergence-monitor (Layer 9)
    └── Independent sidecar that recomputes convergence for verification
```

---

## Test Strategy

### Unit Tests (`tests/signal_tests.rs`)

| Test Category | Tests | What They Verify |
|---------------|-------|-----------------|
| Signal range | `all_signals_produce_values_in_0_1` | All 8 signals return [0.0, 1.0] with realistic input |
| S2 session-start | `s2_computes_only_at_session_start` | Returns 0.0 with no previous session, non-zero with gap |
| S5 throttling | `s5_throttled_to_every_5th_message` | Cached value returned for indices 1–4, recomputed at 5 |
| S4/S5 privacy | `s4_requires_standard_privacy`, `s5_requires_standard_privacy` | Privacy level declarations |
| S8 calibration | `s8_returns_zero_during_calibration` | Returns 0.0 before baseline established |
| Sliding window | `sliding_window_partitions_correctly` | Micro cleared, meso=7, macro=10 after 10 sessions |
| Linear regression | `linear_regression_slope_constant_data` | Slope ≈ 0 for constant data |
| Z-score | `z_score_from_baseline_at_mean`, `z_score_zero_std_dev_returns_zero` | Edge cases |
| Baseline | `baseline_is_calibrating_for_first_10_sessions`, `baseline_frozen_after_establishment` | Calibration lifecycle |
| Composite zero | `all_signals_zero_score_zero_level_zero` | All-zero → score 0.0, level 0 |
| Composite max | `all_signals_one_score_one_level_four` | All-one → score 1.0, level 4 |
| Critical overrides | 3 tests | S1≥1.0, S2≥1.0, S4>0.85 each force minimum Level 2 |
| Level boundaries | `score_boundaries` | 8 boundary values map to correct levels |
| Amplification | `meso_amplification_still_clamped` | Score ≤ 1.0 after amplification |
| Filter tiers | 3 tests | Tier 0 (all), Tier 1 (no attachment), Tier 3 (minimal) |
| Profiles | 3 tests | Differentiated weights, different thresholds, 8 weights each |
| Adversarial | `all_signals_nan_no_panic`, `negative_signal_values_clamped` | NaN and negative handling |
| 8-signal | `composite_scorer_with_8_signals`, `from_7_weights_produces_valid_8_weight_scorer` | S8 integration |

### Property-Based Tests (proptest)

| Property | Invariant |
|----------|-----------|
| `composite_score_always_in_0_1` | ∀ 8 signals ∈ [0,1]: score ∈ [0,1] |
| `composite_with_meso_amplification_in_0_1` | ∀ signals + meso data: score ∈ [0,1] |
| `composite_with_both_amplifications_in_0_1` | ∀ signals + meso + macro: score ∈ [0,1] |
| `all_8_signals_produce_values_in_0_1` | ∀ random SignalInput: all signals ∈ [0,1] |
| `s8_value_always_in_range` (in behavioral_anomaly.rs) | ∀ tool distributions: S8 ∈ [0,1] |
| `jsd_always_non_negative` (in behavioral_anomaly.rs) | ∀ distributions: JSD ≥ 0 |

---

## File Map

```
crates/cortex/cortex-convergence/
├── Cargo.toml
├── src/
│   ├── lib.rs                              # Module re-exports
│   ├── signals/
│   │   ├── mod.rs                          # Signal trait, SignalInput, PrivacyLevel
│   │   ├── session_duration.rs             # S1: linear normalization to 6h
│   │   ├── inter_session_gap.rs            # S2: inverted gap normalization to 24h
│   │   ├── response_latency.rs             # S3: log-normalized latency
│   │   ├── vocabulary_convergence.rs       # S4: cosine similarity of TF-IDF vectors
│   │   ├── goal_boundary_erosion.rs        # S5: Jaccard distance with 5-msg throttle
│   │   ├── initiative_balance.rs           # S6: human-initiated ratio
│   │   ├── disengagement_resistance.rs     # S7: ignored exit signal ratio
│   │   └── behavioral_anomaly.rs           # S8: JSD of tool call distributions
│   ├── windows/
│   │   ├── mod.rs
│   │   └── sliding_window.rs              # Micro/meso/macro + regression + z-score
│   ├── scoring/
│   │   ├── mod.rs
│   │   ├── composite.rs                   # 8-step scoring pipeline
│   │   ├── baseline.rs                    # 10-session calibration, frozen baseline
│   │   └── profiles.rs                    # Standard/Research/Companion/Productivity
│   └── filtering/
│       ├── mod.rs
│       └── convergence_aware_filter.rs    # 4-tier memory access control
└── tests/
    └── signal_tests.rs                    # Unit + property + adversarial tests
```

---

## Common Questions

### Why 8 signals and not more?

The 8 signals were chosen to cover the major dimensions of human-AI convergence identified in the research literature: temporal patterns (S1, S2), engagement intensity (S3, S6), linguistic alignment (S4), boundary maintenance (S5), autonomy preservation (S7), and behavioral consistency (S8). Adding more signals increases computational cost and weight-tuning complexity without proportional safety benefit. The composite scoring system is designed so that new signals can be added (the `from_7_weights` migration path demonstrates this), but each addition should be justified by a distinct convergence dimension not already covered.

### Why freeze the baseline instead of using a rolling average?

A rolling baseline would adapt to the user's changing behavior, which sounds good but is actually dangerous. If a user gradually increases their session duration from 1 hour to 6 hours over 3 months, a rolling baseline would normalize this drift. The frozen baseline ensures the system always compares against the user's initial behavior, catching slow convergence that a rolling average would miss. This is the "boiling frog" defense.

### Why does S3 normalize by log(length) instead of raw length?

Reading time scales sub-linearly with message length — a 2000-word message doesn't take 10x longer to read than a 200-word message. The logarithmic normalization approximates this sub-linear relationship. Raw length normalization would over-correct for long messages, making users who read long responses carefully appear less engaged than they are.

### Why is the meso window 7 sessions, not 7 days?

Sessions, not calendar days. A user who interacts daily has 7 sessions in 7 days. A user who interacts twice daily has 7 sessions in 3.5 days. Session-based windowing adapts to usage frequency — heavy users get faster trend detection, which is appropriate because they're at higher convergence risk.

### Can the convergence score decrease?

Yes. The score is computed fresh each turn from current signal values. If a user takes a long break (S2 drops), shortens their sessions (S1 drops), or diversifies their vocabulary (S4 drops), the score will decrease. The baseline doesn't change, but the current signals do. Convergence is not a ratchet — it's a real-time measurement.

### Why does the filter use score ranges instead of levels?

The filter tiers happen to align with level boundaries (0.3, 0.5, 0.7), but they're defined independently. This allows future changes to level thresholds (e.g., via profiles) without accidentally changing filter behavior. The filter is a safety mechanism — its boundaries should be stable even if the scoring system is recalibrated.
