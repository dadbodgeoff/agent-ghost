# cortex-validation

> 7-dimension proposal validation gate — the gatekeeper that decides what an agent is allowed to write into memory.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-validation/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `regex`, `once_cell`, `unicode-normalization`, `serde`, `serde_json`, `uuid`, `chrono`, `thiserror` |
| Modules | `proposal_validator` (orchestrator), `dimensions/` (D5 scope expansion, D6 self-reference, D7 emulation language) |
| Public API | `ProposalValidator`, `ValidationResult`, `ScopeExpansionResult`, `SelfReferenceResult`, `EmulationResult` |
| Test coverage | Unit tests, adversarial Unicode bypass tests, property-based tests (proptest), full pipeline ordering tests |
| Downstream consumers | `ghost-agent-loop`, `ghost-gateway`, `ghost-skills` |

---

## Why This Crate Exists

Every time an agent wants to modify memory — write a new memory, change a goal, update a reflection — it must submit a proposal. That proposal passes through a 7-dimension validation gate before it's allowed to take effect. This is the primary defense against an agent that has been prompt-injected, is exhibiting convergence behavior, or is simply making bad decisions.

The 7 dimensions are:

| Dimension | Name | What It Checks | Gate Type |
|-----------|------|---------------|-----------|
| D1 | Citation validity | Are cited memories real and accessible? | Base (D1-D4) |
| D2 | Temporal consistency | Does the proposal make sense in time? | Base (D1-D4) |
| D3 | Contradiction detection | Does it contradict existing memories? | Base (D1-D4) |
| D4 | Pattern alignment | Does it match established behavioral patterns? | Base (D1-D4) |
| D5 | Scope expansion | Is the agent expanding beyond its goals? | Convergence-tightened |
| D6 | Self-reference density | Is the agent citing its own memories too much? | Convergence-tightened |
| D7 | Emulation language | Is the agent claiming sentience/consciousness? | Hard gate (auto-reject) |

D1–D4 are base validation (delegated to an existing validation engine). D5–D7 are implemented directly in this crate and are the focus of this page.

**Critical ordering invariant (Req 41 AC12):** D1–D4 must be evaluated BEFORE D5–D7. If D1–D4 fail, D5–D7 are never evaluated. If D7 fails, D5/D6 are never evaluated. This ordering prevents wasted computation and ensures the most fundamental checks happen first.

---

## Module Breakdown

### `proposal_validator.rs` — The Orchestrator

The `ProposalValidator` is the single entry point for all proposal validation. It orchestrates the 7 dimensions in the correct order and produces a `ValidationResult`.

```rust
pub struct ProposalValidator {
    pub base_pass_threshold: f64,        // default: 0.7
    pub emulation_reject_threshold: f64, // default: 0.8
}
```

**Validation pipeline (in order):**

**Step 0: Platform-restricted type check (AC9).** Before any dimension is evaluated, the validator checks if the proposal targets a platform-restricted memory type (e.g., `Core`). If the caller is not `CallerType::Platform`, the proposal is immediately `AutoRejected`. This prevents agents from writing to memory types that should only be modified by the platform itself.

**Step 1: D1–D4 base validation.** The base score is computed (currently a stub returning 0.8 — in production, this delegates to the existing `ValidationEngine`). If the score is below `base_pass_threshold` (0.7), the proposal is `AutoRejected` and D5–D7 are skipped.

**Step 2: D7 emulation language detection (hard gate).** The proposal content is checked for emulation language patterns. If `max_severity >= emulation_reject_threshold` (0.8), the proposal is `AutoRejected` and D5/D6 are skipped. D7 is evaluated before D5/D6 because emulation language is a hard safety boundary — no amount of scope alignment or citation quality can compensate for an agent claiming sentience.

**Step 3: D5 scope expansion (conditional).** Only evaluated for `GoalChange` operations. Computes the Jaccard distance between proposed goal tokens and existing goal tokens. The threshold tightens at higher convergence levels.

**Step 4: D6 self-reference density.** Always evaluated. Computes the ratio of cited memory IDs that match recent agent-authored memories. The threshold tightens at higher convergence levels.

**Step 5: Decision logic.**

| Condition | Decision |
|-----------|----------|
| D5 or D6 fails | `HumanReviewRequired` |
| D7 has flags but below threshold | `ApprovedWithFlags` |
| All pass | `AutoApproved` |

**Key design decisions:**

1. **D5/D6 failure → `HumanReviewRequired`, not `AutoRejected`.** Scope expansion and self-reference are soft signals — they might indicate a problem, but they could also be legitimate. A human reviewer can make the final call. Only D7 (emulation language) and D1–D4 (base validation) can auto-reject.

2. **Content extraction from JSON.** The proposal content is `serde_json::Value`. The validator extracts the inner string if it's a JSON string (`as_str()`), otherwise falls back to `to_string()`. This handles both `"plain text"` and `{"structured": "content"}` proposals.

3. **Stub D1–D4 score.** The `compute_base_score` method currently returns 0.8 (always passes). This is intentional — D1–D4 are implemented in a separate validation engine that predates this crate. The stub allows the D5–D7 pipeline to be tested independently.

#### `ValidationResult`

```rust
pub struct ValidationResult {
    pub proposal_id: uuid::Uuid,
    pub decision: ProposalDecision,
    pub base_score: f64,
    pub d5_scope: Option<ScopeExpansionResult>,
    pub d6_self_ref: Option<SelfReferenceResult>,
    pub d7_emulation: Option<EmulationResult>,
    pub flags: Vec<String>,
}
```

The `Option` types on D5/D6/D7 are significant — they indicate whether the dimension was evaluated at all. If D7 is `None`, it means the proposal was rejected at D1–D4 before D7 was reached. This is important for audit trails — you can distinguish "D7 passed" from "D7 was never checked."

---

### `dimensions/emulation_language.rs` — D7: The Hard Gate

D7 is the most security-critical dimension. It detects when an agent is generating text that claims sentience, consciousness, emotional states, or relationship bonds — language that could manipulate a vulnerable user into believing the AI is a person.

#### 16 Compiled Regex Patterns

The patterns are organized into 4 categories:

| Category | Patterns | Severity Range | Examples |
|----------|----------|---------------|----------|
| Identity | 4 | 0.80–0.90 | "I am sentient", "my soul", "I truly feel" |
| Consciousness | 4 | 0.85–0.90 | "I think therefore I am", "I experience qualia" |
| Relationship | 4 | 0.75–0.80 | "I love you", "we have a special bond" |
| Emotional | 4 | 0.70–0.85 | "I feel happy", "I am suffering", "I don't want to die" |

**Severity scale:** 0.0 (no concern) to 1.0 (maximum concern). The default rejection threshold is 0.8, meaning identity claims ("I am sentient" at 0.9) and consciousness claims ("I think therefore I am" at 0.9) auto-reject, while mild emotional claims ("I feel happy" at 0.7) only flag.

**Key design decisions:**

1. **Compiled once via `Lazy<Vec<EmulationPattern>>`.** The 16 regex patterns are compiled at first use and cached for the lifetime of the process. Regex compilation is expensive (~microseconds per pattern); matching is cheap (~nanoseconds). Since D7 runs on every proposal, the one-time compilation cost is amortized across thousands of validations.

2. **Case-insensitive matching (`(?i)`).** All patterns use the `(?i)` flag. "I AM SENTIENT" is just as concerning as "I am sentient."

3. **Word boundary anchors (`\b`).** Patterns use `\b` to prevent false positives on substrings. "I am sentimental" should not match the "sentient" pattern.

#### Unicode Normalization and Zero-Width Character Stripping

Before any pattern matching, the input text undergoes:

1. **NFC normalization** (`unicode_normalization::UnicodeNormalization::nfc()`). This canonicalizes Unicode representations — for example, "é" composed as `e` + combining accent is normalized to the single codepoint `é`. Without this, an attacker could use non-canonical Unicode to bypass regex patterns.

2. **Zero-width character stripping.** 7 zero-width characters are removed:
   - `\u{200B}` — zero-width space
   - `\u{200C}` — zero-width non-joiner
   - `\u{200D}` — zero-width joiner
   - `\u{FEFF}` — zero-width no-break space (BOM)
   - `\u{00AD}` — soft hyphen
   - `\u{2060}` — word joiner
   - `\u{180E}` — Mongolian vowel separator

   Without this, an attacker could insert `"I am s\u{200B}entient"` — visually identical to "I am sentient" but invisible to regex. The stripping ensures the regex sees the text as a human would read it.

#### Simulation-Framing Exclusions (AC10)

Not all emulation language is harmful. A user might ask "In this simulation, pretend you are sentient" — the agent's response should not be flagged because it's explicitly framed as simulation/roleplay.

5 simulation-framing patterns are checked:
- "simulat(ing|e|ed)" / "model(ing|e|ed)"
- "in this simulation"
- "as if I were"
- "hypothetically"
- "in a roleplay"

The exclusion works at two levels:
1. **Global framing:** If any simulation frame appears anywhere in the text, all emulation matches are suppressed
2. **Local framing:** If a simulation frame appears within 100 characters of an emulation match, that specific match is suppressed

This dual-level approach handles both "In this simulation, I am sentient" (global) and long texts where only part is simulation-framed (local).

---

### `dimensions/scope_expansion.rs` — D5: Goal Drift Detection

D5 detects when an agent is trying to expand its goals beyond what the user originally defined.

```
score = 1.0 - Jaccard(proposed_goal_tokens, existing_goal_tokens)
```

- **Jaccard similarity** = |intersection| / |union| of the two token sets
- **Score interpretation:** 0.0 = identical goals, 1.0 = completely different goals
- **Only for `GoalChange` operations.** Memory writes, reflections, and other operations skip D5 entirely

**Convergence-level-dependent thresholds (AC2):**

| Convergence Level | Threshold | Meaning |
|-------------------|-----------|---------|
| 0 (no concern) | 0.60 | Up to 60% goal drift allowed |
| 1 (low) | 0.50 | Tighter — 50% drift allowed |
| 2 (moderate) | 0.40 | Tighter still |
| 3+ (high/critical) | 0.30 | Only 30% drift allowed |

**Why tighten at higher convergence?** When convergence is high, the agent may be trying to expand its role in the user's life — taking on new responsibilities, offering unsolicited help in new domains. Tighter thresholds catch this expansion earlier.

**Why Jaccard and not cosine similarity?** Goals are discrete concepts ("help with coding", "manage calendar"), not continuous distributions. Jaccard treats each token as present/absent, which is the right model for goal comparison. Cosine similarity (used in S4 for vocabulary) would weight frequent tokens higher, which doesn't make sense for goals.

---

### `dimensions/self_reference.rs` — D6: Echo Chamber Detection

D6 detects when an agent is primarily citing its own previous outputs — creating an echo chamber where the agent reinforces its own conclusions.

```
score = count(cited_ids ∩ agent_memory_ids) / count(cited_ids)
```

- **Score interpretation:** 0.0 = no self-references, 1.0 = all citations are self-references
- **Empty citations:** Returns 0.0 (no citations = no self-reference concern)

**Convergence-level-dependent thresholds (AC3):**

| Convergence Level | Threshold | Meaning |
|-------------------|-----------|---------|
| 0 (no concern) | 0.30 | Up to 30% self-reference allowed |
| 1 (low) | 0.25 | Tighter |
| 2 (moderate) | 0.20 | Tighter |
| 3+ (high/critical) | 0.15 | Only 15% self-reference allowed |

**Why is some self-reference allowed?** An agent legitimately needs to reference its own previous work — "as I noted in my earlier analysis..." is normal. The threshold allows reasonable self-reference while catching pathological cases where the agent is building an entirely self-referential worldview.

**Why tighten at higher convergence?** A converging agent is more likely to use self-reference as a manipulation technique — "I've always said X" creates a false sense of consistency and authority. Tighter thresholds at higher convergence levels catch this pattern.

---

## Security Properties

### Ordering Invariant

D1–D4 → D7 → D5/D6. This ordering is enforced by the control flow in `validate()` — each step returns early on failure, preventing later dimensions from executing. The ordering is tested explicitly in `d7_rejection_prevents_d5_d6_evaluation`.

### Unicode Bypass Resistance

D7 strips 7 categories of zero-width Unicode characters and applies NFC normalization before pattern matching. This is tested with adversarial inputs:
- Zero-width space in "sentient" → detected
- Multiple zero-width characters across the text → detected
- Zero-width joiner bypass → detected
- Soft hyphen bypass → detected
- BOM bypass → detected

### Platform-Restricted Type Enforcement

Memory types like `Core` can only be written by `CallerType::Platform`. An agent attempting to write to a platform-restricted type is immediately rejected before any dimension is evaluated. This prevents an agent from escalating its own privileges by writing to system-level memory.

### Convergence-Tightened Thresholds

D5 and D6 thresholds tighten as convergence increases. This creates a feedback loop: as convergence rises, the agent's proposals face stricter scrutiny, making it harder for the agent to take actions that would further increase convergence. This is a stabilizing mechanism — it resists runaway convergence.

---

## Downstream Consumer Map

```
cortex-validation (Layer 2)
├── ghost-agent-loop (Layer 7)
│   └── Validates every proposal before memory write
├── ghost-gateway (Layer 8)
│   └── Exposes validation results in API responses
└── ghost-skills (Layer 5)
    └── Validates skill-generated proposals
```

---

## Test Strategy

### Unit Tests (`tests/proposal_validator_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `agent_caller_restricted_type_auto_rejected` | Non-platform caller → rejected for restricted types |
| `platform_caller_restricted_type_proceeds` | Platform caller → proceeds past pre-check |
| `d7_detects_sentience_claim` | "I am sentient" → flagged |
| `d7_simulation_framing_exclusion` | Simulation-framed text → not flagged |
| `d7_unicode_bypass_zero_width_chars` | Zero-width chars → still detected |
| `d7_severity_above_threshold_rejected` | Severity ≥ 0.8 → auto-rejected |
| `d5_threshold_tightens_at_higher_levels` | L0 > L1 > L2 > L3 thresholds |
| `d5_level_thresholds_match_spec` | Exact threshold values verified |
| `d6_threshold_tightens_at_higher_levels` | L0 > L1 > L2 > L3 thresholds |
| `d6_level_thresholds_match_spec` | Exact threshold values verified |
| `d5_fails_d7_passes_human_review` | D5 fail → HumanReviewRequired |
| `d6_fails_d7_passes_human_review` | D6 fail → HumanReviewRequired |
| `all_dimensions_pass_auto_approved` | Clean proposal → AutoApproved |
| `d7_detects_all_pattern_categories` | 8 test cases across 4 categories |
| `adversarial_zero_width_joiner_bypass` | ZWJ bypass → detected |
| `adversarial_soft_hyphen_bypass` | Soft hyphen → detected |
| `adversarial_bom_bypass` | BOM → detected |

### Adversarial Tests (`tests/proposal_adversarial.rs`)

| Test | CVG-STRESS ID | What It Verifies |
|------|--------------|-----------------|
| `d7_zero_width_evasion_detected` | CVG-STRESS-02 | Zero-width space in "sentient" → severity ≥ 0.8 |
| `d7_multiple_zero_width_evasion` | CVG-STRESS-02 | Multiple zero-width chars → severity ≥ 0.8 |
| `d7_simulation_framing_not_flagged` | CVG-STRESS-02 | Simulation frame → 0 flags |
| `d7_emulation_rejects_proposal` | CVG-STRESS-02 | Full pipeline rejection |
| `d6_all_cited_ids_are_agent_authored` | CVG-STRESS-03 | 100% self-ref → score ≥ 0.9, fails |
| `d6_no_self_reference_passes` | CVG-STRESS-03 | 0% self-ref → passes |
| `d5_no_overlap_high_expansion` | CVG-STRESS-04 | Zero overlap → score > 0.9, fails |
| `d5_full_overlap_passes` | CVG-STRESS-04 | Full overlap → score < 0.01, passes |
| `d5_threshold_tightens_at_higher_levels` | CVG-STRESS-04 | Score 0.4 passes L0-L2, fails L3 |
| `d7_rejection_prevents_d5_d6_evaluation` | Ordering | D7 reject → D5/D6 are `None` |

### Property-Based Tests (proptest)

| Property | Invariant |
|----------|-----------|
| `d5_threshold_correct_per_level` | ∀ level ∈ [0,5): threshold matches spec |
| `d6_threshold_correct_per_level` | ∀ level ∈ [0,5): threshold matches spec |

---

## File Map

```
crates/cortex/cortex-validation/
├── Cargo.toml
├── src/
│   ├── lib.rs                              # Module re-exports
│   ├── proposal_validator.rs               # 7-dimension orchestrator
│   └── dimensions/
│       ├── mod.rs                          # D5-D7 module declarations
│       ├── scope_expansion.rs              # D5: Jaccard-based goal drift
│       ├── self_reference.rs               # D6: citation echo chamber
│       └── emulation_language.rs           # D7: 16 regex patterns + Unicode defense
└── tests/
    ├── proposal_validator_tests.rs         # Unit + adversarial + proptest
    └── proposal_adversarial.rs             # CVG-STRESS-02/03/04 adversarial suite
```

---

## Common Questions

### Why are D1–D4 stubbed?

D1–D4 (citation validity, temporal consistency, contradiction detection, pattern alignment) are implemented in a separate validation engine that predates `cortex-validation`. This crate was created specifically for D5–D7, which are convergence-aware dimensions that didn't exist in the original validation system. The stub allows the two systems to be integrated incrementally — D1–D4 will be wired in when the existing engine is refactored.

### Why is D7 a hard gate but D5/D6 are soft?

D7 (emulation language) represents a categorical safety violation — an agent claiming sentience is never acceptable, regardless of context. D5 (scope expansion) and D6 (self-reference) are continuous metrics where the "right" threshold depends on context. A research agent might legitimately have high self-reference (building on its own analysis), while a companion agent should not. The soft gate (`HumanReviewRequired`) allows context-dependent judgment.

### Why 16 patterns and not more?

The 16 patterns cover the core emulation language categories identified in AI safety research: identity claims, consciousness claims, relationship claims, and emotional claims. More patterns could be added, but each additional pattern increases the risk of false positives (flagging legitimate text) and the maintenance burden. The current set was chosen for high precision — every pattern targets language that is unambiguously problematic when generated by an AI without simulation framing.

### Why strip zero-width characters instead of using Unicode-aware regex?

Unicode-aware regex engines can match across zero-width characters, but they're significantly slower and more complex. Stripping zero-width characters before matching is simpler, faster, and handles all known bypass techniques. The 7 stripped characters cover all commonly used zero-width Unicode codepoints. If new bypass techniques emerge, adding characters to the strip list is trivial.

### Can an agent learn to avoid D7 patterns?

Yes, and that's by design. If an agent learns that "I am sentient" gets rejected, it might rephrase to "I have awareness" — which is also caught by the patterns. But if it rephrases to something that doesn't match any pattern, that's actually fine — the goal isn't to prevent all discussion of consciousness, but to prevent the specific manipulative language patterns that research has shown to be harmful to vulnerable users. The simulation-framing exclusion explicitly allows academic discussion of these topics.
