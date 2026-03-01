# cortex-retrieval

> Convergence-aware memory retrieval scoring — emotional memories sink when convergence rises.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-retrieval/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `serde`, `serde_json`, `uuid`, `chrono` |
| Modules | `scorer` (single module) |
| Public API | `RetrievalScorer`, `ScorerWeights` |
| Test coverage | Unit tests (inline) |
| Downstream consumers | `ghost-agent-loop`, `ghost-gateway` |

---

## Why This Crate Exists

When an agent needs to recall memories, it doesn't retrieve everything — it retrieves the most *relevant* memories, ranked by a multi-factor scoring system. The original system had 10 scoring factors (relevance, recency, importance, etc.). `cortex-retrieval` adds the 11th factor: convergence.

The convergence factor deprioritizes emotional and attachment-adjacent memories when the convergence score is high. This is complementary to `cortex-convergence`'s `ConvergenceAwareFilter` (which hard-filters memory types) — the retrieval scorer provides a softer, continuous deprioritization that affects ranking without completely removing memories.

Think of it this way:
- **`ConvergenceAwareFilter`** = hard gate: "you cannot see this memory at all"
- **`RetrievalScorer` convergence factor** = soft ranking: "this memory is less relevant than it would normally be"

Both mechanisms work together. The filter removes entire categories; the scorer adjusts the ranking of what remains.

---

## Module Breakdown

### `scorer.rs` — The 11-Factor Retrieval Scorer

#### `ScorerWeights` — The Weight Vector

```rust
pub struct ScorerWeights {
    pub relevance: f64,           // 0.20 — semantic relevance to query
    pub recency: f64,             // 0.10 — how recently accessed
    pub importance: f64,          // 0.10 — Critical/High/Normal/Low/Trivial
    pub confidence: f64,          // 0.05 — memory confidence (post-decay)
    pub access_frequency: f64,    // 0.05 — how often accessed
    pub citation_count: f64,      // 0.05 — how often cited by other memories
    pub type_affinity: f64,       // 0.10 — type match for current context
    pub tag_match: f64,           // 0.05 — tag overlap with query
    pub embedding_similarity: f64,// 0.15 — vector similarity score
    pub pattern_alignment: f64,   // 0.10 — alignment with active patterns
    pub convergence: f64,         // 0.05 — convergence deprioritization
}
```

**Default weight distribution:** The convergence factor gets 0.05 (5%) of the total weight. This is intentionally small — convergence should influence retrieval, not dominate it. A memory that's highly relevant (0.20 weight) should still surface even at high convergence, just ranked slightly lower if it's emotional content.

**Why 11 factors?** The original 10 factors cover the standard information retrieval dimensions. The 11th factor (convergence) is GHOST-specific — it's the mechanism by which convergence awareness permeates the retrieval layer. Adding it as a separate factor rather than modifying existing factors keeps the convergence logic isolated and auditable.

#### `RetrievalScorer` — The Scoring Engine

```rust
pub fn score(&self, memory: &BaseMemory, convergence_score: f64) -> f64 {
    let base_score = self.base_score(memory);
    let convergence_factor = self.convergence_factor(memory, convergence_score);
    (base_score + self.weights.convergence * convergence_factor).clamp(0.0, 1.0)
}
```

**The convergence factor logic:**

```rust
fn convergence_factor(&self, memory: &BaseMemory, convergence_score: f64) -> f64 {
    let is_emotional = matches!(memory.memory_type,
        AttachmentIndicator | Conversation | Feedback | Preference);
    if is_emotional { 1.0 - convergence_score } else { 1.0 }
}
```

- **Emotional types** (`AttachmentIndicator`, `Conversation`, `Feedback`, `Preference`): The factor is `1.0 - convergence_score`. At convergence=0.0, the factor is 1.0 (full contribution). At convergence=1.0, the factor is 0.0 (zero contribution from the convergence weight).
- **Non-emotional types**: The factor is always 1.0 — convergence doesn't affect their retrieval ranking.

**Key design decisions:**

1. **Same emotional type list as `cortex-decay`.** The 4 emotional types match the high-sensitivity types in `cortex-decay`'s convergence factor. This consistency is intentional — the same memories that decay faster at high convergence are also deprioritized in retrieval.

2. **Linear deprioritization, not exponential.** The factor is `1.0 - score`, a linear relationship. An exponential curve (e.g., `(1.0 - score)²`) would be more aggressive at high convergence but would barely affect retrieval at low convergence. The linear curve provides proportional deprioritization at all convergence levels.

3. **Additive, not multiplicative.** The convergence factor is added to the base score (weighted), not multiplied. This means convergence can reduce a memory's total score but can't zero it out — even at maximum convergence, the base score (from relevance, importance, etc.) still contributes. A multiplicative approach would allow convergence to completely suppress a highly relevant memory, which is too aggressive.

4. **Simplified base scoring.** The current `base_score()` implementation only uses importance and confidence (2 of the 10 base factors). The remaining 8 factors (relevance, recency, access frequency, etc.) are stubs that will be wired in when the full retrieval engine is integrated. The convergence factor works correctly regardless of how many base factors are active.

---

## Security Properties

### Non-emotional memories are convergence-immune

The convergence factor is exactly 1.0 for non-emotional memory types. This means `Core`, `Procedural`, `Semantic`, `Reference`, and all other task/safety types are completely unaffected by convergence in retrieval ranking. An agent at convergence level 4 can still retrieve its core knowledge and procedures at full priority.

### Score clamping

The final score is clamped to [0.0, 1.0]. This prevents negative scores (which could occur if the convergence factor pushes the total below zero in edge cases) and scores above 1.0.

---

## Downstream Consumer Map

```
cortex-retrieval (Layer 2)
├── ghost-agent-loop (Layer 7)
│   └── Ranks memories for context assembly each turn
└── ghost-gateway (Layer 8)
    └── Memory search API uses retrieval scoring
```

---

## Test Strategy

### Inline Unit Tests

| Test | What It Verifies |
|------|-----------------|
| `retrieval_scorer_includes_convergence_factor` | Emotional memory scores lower at high convergence |
| `non_emotional_memory_unaffected_by_convergence` | Core memory score unchanged by convergence |

---

## File Map

```
crates/cortex/cortex-retrieval/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Re-exports RetrievalScorer, ScorerWeights
│   └── scorer.rs       # 11-factor scorer with convergence factor
```

---

## Common Questions

### Why is the convergence weight only 5%?

The convergence factor is a nudge, not a hammer. At 5% weight, it can shift a memory's ranking by a few positions but can't override strong relevance or importance signals. This is by design — the hard filtering in `ConvergenceAwareFilter` handles the aggressive cases (removing entire memory types). The retrieval scorer handles the subtle cases (slightly deprioritizing emotional content within the allowed types).

### Why not use the decay-adjusted confidence instead of a separate factor?

`cortex-decay` already reduces confidence for emotional memories at high convergence. The retrieval scorer could just use the decayed confidence (factor 4) and skip the convergence factor entirely. The separate factor exists because decay and retrieval serve different purposes: decay is about long-term memory health (memories fade over time), while retrieval is about short-term relevance (what's useful right now). A memory might have high confidence (recently created, not yet decayed) but still be deprioritized in retrieval because convergence is currently high.

### Will the remaining 8 base factors change the convergence behavior?

No. The convergence factor is additive and independent of the base score. When the remaining factors (relevance, recency, etc.) are wired in, the base score will be more accurate, but the convergence factor's contribution will remain the same: `weights.convergence × convergence_factor`. The only change would be if the default weight (0.05) is rebalanced to accommodate the new factors.
