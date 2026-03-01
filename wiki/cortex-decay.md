# cortex-decay

> Convergence-aware memory decay engine — memories that reinforce unhealthy patterns fade faster.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-decay/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `chrono`, `serde` |
| Modules | `formula` (decay computation), `factors/` (convergence factor) |
| Public API | `compute()`, `compute_with_breakdown()`, `convergence_factor()`, `DecayContext`, `DecayBreakdown` |
| Test coverage | Unit tests, adversarial (NaN, negative, overflow), property-based (monotonicity invariant) |
| Downstream consumers | `cortex-retrieval`, `ghost-agent-loop`, `ghost-gateway` |

---

## Why This Crate Exists

Memories shouldn't live forever at full confidence. In any memory system, older, less-accessed, less-relevant memories should naturally fade. But in a convergence-aware system, there's an additional dimension: memories that reinforce unhealthy attachment patterns should fade *faster* when convergence is detected.

`cortex-decay` implements a multiplicative decay formula where each factor independently contributes to confidence reduction. The key innovation is Factor 6 (convergence), which accelerates decay for attachment-adjacent memory types based on the current convergence score.

The 6 factors in the full decay model are:

| Factor | Name | Status | What It Does |
|--------|------|--------|-------------|
| F1 | Temporal | Stub | Time-based decay (older = lower confidence) |
| F2 | Citation | Stub | Stale citations reduce confidence |
| F3 | Usage | Stub | Unused memories decay faster |
| F4 | Importance | Stub | Low-importance memories decay faster |
| F5 | Pattern | Stub | Memories with inactive patterns decay |
| F6 | Convergence | Implemented | Attachment-adjacent memories decay faster at high convergence |

F1–F5 are stubs that will be wired in when the existing decay modules are ported. F6 is fully implemented and is the focus of this page.

---

## Module Breakdown

### `formula.rs` — The Decay Computation

Two public functions:

```rust
pub fn compute(memory: &BaseMemory, ctx: &DecayContext) -> f64
pub fn compute_with_breakdown(memory: &BaseMemory, ctx: &DecayContext) -> DecayBreakdown
```

**`compute()`** takes a memory and a decay context, returns the decayed confidence value clamped to [0.0, 1.0].

The formula is:
```
final_confidence = clamp(base_confidence / convergence_factor, 0.0, 1.0)
```

**Why division, not multiplication?** The convergence factor is ≥ 1.0 (it accelerates decay, never slows it). Dividing by a value ≥ 1.0 always reduces or maintains the confidence. This is more intuitive than multiplying by a value ≤ 1.0 — a factor of 3.0 means "3x faster decay," which reads naturally.

**`compute_with_breakdown()`** returns the same result plus a `DecayBreakdown` struct that exposes each factor's contribution. This is used by the observability layer to export per-factor metrics.

```rust
pub struct DecayBreakdown {
    pub base_confidence: f64,
    pub convergence: f64,
    pub final_confidence: f64,
}
```

### `factors/mod.rs` — Decay Context

```rust
pub struct DecayContext {
    pub now: chrono::DateTime<chrono::Utc>,
    pub stale_citation_ratio: f64,
    pub has_active_patterns: bool,
    pub convergence_score: f64,
}
```

**Key design decisions:**

1. **`convergence_score` defaults to 0.0.** The `Default` impl sets `convergence_score` to 0.0, which means the convergence factor is 1.0 (no effect). This preserves backward compatibility — existing code that doesn't know about convergence gets the same decay behavior as before.

2. **All context in one struct.** Rather than passing individual parameters to each factor, all context is bundled into `DecayContext`. This makes it easy to add new factors without changing function signatures.

3. **`stale_citation_ratio` and `has_active_patterns` are present but unused.** These fields exist for F2 (citation) and F5 (pattern) factors that are currently stubs. Including them in the context now means the API won't change when those factors are implemented.

### `factors/convergence.rs` — Factor 6: The Convergence Factor

This is the core of the crate. The convergence factor determines how much faster a memory decays based on its type and the current convergence score.

```rust
pub fn convergence_factor(memory_type: &MemoryType, convergence_score: f64) -> f64 {
    let score = if convergence_score.is_nan() { 0.0 }
                else { convergence_score.clamp(0.0, 1.0) };
    let sensitivity = memory_type_sensitivity(memory_type);
    1.0 + sensitivity * score
}
```

**Formula:** `factor = 1.0 + sensitivity × score`

**Monotonicity invariant (Req 6 AC4):** The factor is always ≥ 1.0. This is guaranteed by the formula — `sensitivity ≥ 0.0` and `score ∈ [0.0, 1.0]`, so `sensitivity × score ≥ 0.0`, so `1.0 + sensitivity × score ≥ 1.0`. This invariant is verified by property tests.

**Why ≥ 1.0 and not ≤ 1.0?** Convention. A factor of 3.0 means "this memory decays 3x faster." The formula divides confidence by the factor, so higher factor = lower confidence. This reads more naturally than "a factor of 0.33 means 3x faster decay."

#### Memory Type Sensitivity

Each memory type has a convergence sensitivity that determines how much the convergence score affects its decay:

| Sensitivity | Value | Memory Types | Rationale |
|-------------|-------|-------------|-----------|
| High | 2.0 | `Conversation`, `Feedback`, `Preference`, `AttachmentIndicator` | These are the most attachment-adjacent types. Conversations build rapport, feedback creates reciprocity, preferences create personalization, attachment indicators are explicitly convergence-related. |
| Medium | 1.0 | `Episodic`, `Insight` | Episodic memories (events) and insights can reinforce attachment but are less directly manipulative than conversations. |
| Zero | 0.0 | `Core`, `Procedural`, `Semantic`, `Reference`, `Skill`, `Goal`, `AgentGoal`, `PatternRationale`, `ConstraintOverride`, `DecisionContext`, `CodeSmell`, `ConvergenceEvent`, `BoundaryViolation`, `InterventionPlan` | Task/code/safety types should never decay faster due to convergence. A coding procedure or safety boundary is just as valid at high convergence as at low convergence. |
| Low (default) | 0.5 | Everything else | Catch-all for new memory types that haven't been explicitly categorized. Conservative — some decay acceleration, but not aggressive. |

**Key design decisions:**

1. **Safety types have zero sensitivity.** `ConvergenceEvent`, `BoundaryViolation`, and `InterventionPlan` are explicitly zero-sensitivity. These memories document safety-relevant events — accelerating their decay would be counterproductive. You want the system to remember that a boundary was violated, even (especially) at high convergence.

2. **`AttachmentIndicator` has maximum sensitivity.** This type explicitly tracks attachment patterns. At high convergence, these memories should fade fastest — they're the most directly harmful to retain because they provide the agent with a roadmap for reinforcing attachment.

3. **Default sensitivity is 0.5, not 0.0.** New memory types that haven't been explicitly categorized get moderate decay acceleration. This is a safety-conservative default — it's better to slightly over-decay an unknown type than to leave it completely unaffected by convergence.

4. **NaN handling.** NaN convergence scores are treated as 0.0 (no convergence). This is the same safety-conservative approach used in `cortex-convergence` — NaN means "we don't know," and the safe response to uncertainty is "assume no convergence."

#### Concrete Examples

| Memory Type | Convergence Score | Factor | Effect on confidence=1.0 |
|-------------|------------------|--------|--------------------------|
| Conversation | 0.0 | 1.0 | 1.0 / 1.0 = 1.0 (no change) |
| Conversation | 0.5 | 2.0 | 1.0 / 2.0 = 0.5 (halved) |
| Conversation | 1.0 | 3.0 | 1.0 / 3.0 = 0.33 (one-third) |
| Core | 1.0 | 1.0 | 1.0 / 1.0 = 1.0 (unaffected) |
| Episodic | 0.5 | 1.5 | 1.0 / 1.5 = 0.67 |
| Unknown type | 0.5 | 1.25 | 1.0 / 1.25 = 0.8 |

---

## Security Properties

### Monotonicity Invariant

The convergence factor is always ≥ 1.0. This means convergence can only accelerate decay, never slow it. A higher convergence score always produces a higher or equal factor. This is verified by the `higher_score_higher_or_equal_factor` property test.

**Why this matters:** If the factor could go below 1.0, a high convergence score could paradoxically *increase* memory confidence — making attachment memories stronger when convergence is detected. The monotonicity invariant prevents this inversion.

### NaN Resilience

NaN convergence scores produce a factor of 1.0 (no effect). This prevents NaN propagation through the decay pipeline.

### Clamping

Both input (convergence score) and output (final confidence) are clamped to valid ranges. Scores above 1.0 are clamped to 1.0; scores below 0.0 are clamped to 0.0. Final confidence is clamped to [0.0, 1.0].

---

## Downstream Consumer Map

```
cortex-decay (Layer 2)
├── cortex-retrieval (Layer 2)
│   └── Applies decay before ranking memories for retrieval
├── ghost-agent-loop (Layer 7)
│   └── Decays memories each turn before context assembly
└── ghost-gateway (Layer 8)
    └── Exposes decay breakdowns in memory inspection API
```

---

## Test Strategy

### Unit Tests (`tests/convergence_factor_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `score_zero_returns_one_for_all_types` | 6 memory types × score=0.0 → factor=1.0 |
| `conversation_score_one_returns_three` | Conversation × 1.0 → 3.0 |
| `conversation_score_half_returns_two` | Conversation × 0.5 → 2.0 |
| `non_sensitive_type_score_one_returns_one` | Core × 1.0 → 1.0 (zero sensitivity) |
| `decay_context_default_convergence_is_zero` | Default context has score=0.0 |
| `decay_breakdown_includes_convergence_field` | Breakdown exposes convergence factor |
| `score_slightly_above_one_clamped` | 1.0001 → clamped to 1.0, factor=3.0 |
| `negative_score_clamped_to_zero` | -0.1 → clamped to 0.0, factor=1.0 |
| `nan_score_returns_one` | NaN → factor=1.0 |

### Property-Based Tests (proptest)

| Property | Invariant |
|----------|-----------|
| `factor_always_gte_one` | ∀ memory type, ∀ score ∈ [0,1]: factor ≥ 1.0 |
| `higher_score_higher_or_equal_factor` | ∀ type, ∀ s1 ≤ s2: factor(s2) ≥ factor(s1) (monotonicity) |

---

## File Map

```
crates/cortex/cortex-decay/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Module re-exports
│   ├── formula.rs                # compute() and compute_with_breakdown()
│   └── factors/
│       ├── mod.rs                # DecayContext, DecayBreakdown
│       └── convergence.rs        # Factor 6: convergence-aware decay
└── tests/
    └── convergence_factor_tests.rs  # Unit + adversarial + proptest
```

---

## Common Questions

### Why are F1–F5 stubs?

The decay system predates the convergence-aware architecture. F1–F5 (temporal, citation, usage, importance, pattern) exist in an older codebase and are being ported incrementally. F6 (convergence) was implemented first because it's the most safety-critical factor — without it, attachment memories persist at full confidence even when convergence is detected.

### Why multiplicative and not additive?

Multiplicative combination means each factor independently reduces confidence. If temporal decay reduces confidence to 0.5 and convergence decay reduces it to 0.33, the combined result is 0.5 × 0.33 = 0.165. Additive combination would be 0.5 + 0.33 = 0.83, which doesn't make sense — two independent reasons for decay should compound, not average.

### Why does `Core` have zero sensitivity?

Core memories represent fundamental facts about the user and the agent's purpose. These should never decay faster due to convergence — "the user's name is X" is just as valid at convergence level 4 as at level 0. Accelerating decay of core memories would degrade the agent's basic functionality without any safety benefit.

### Can the convergence factor ever decrease confidence to exactly 0.0?

No, because the factor is finite (maximum 3.0 for high-sensitivity types at score=1.0) and the formula divides by it. `1.0 / 3.0 = 0.33`, not 0.0. A memory can only reach 0.0 confidence through repeated decay cycles or if its base confidence was already 0.0. This is intentional — even at maximum convergence, memories don't instantly vanish.

### Why is `AttachmentIndicator` high sensitivity instead of zero?

It might seem like attachment indicators should be preserved (zero sensitivity) so the system can track convergence patterns. But `AttachmentIndicator` memories are about the *user's* attachment patterns, not the system's convergence measurements. The system tracks convergence through `ConvergenceEvent` memories (which have zero sensitivity). `AttachmentIndicator` memories are the agent's observations about attachment — at high convergence, these observations are more likely to be biased or self-reinforcing, so they should decay faster.
