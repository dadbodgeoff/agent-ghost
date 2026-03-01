# cortex-privacy

> Emotional content detection — identifying attachment patterns in text before they become memories.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-privacy/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `regex`, `once_cell`, `serde` |
| Modules | `emotional_patterns` (single module) |
| Public API | `EmotionalContentDetector`, `EmotionalCategory`, `EmotionalMatch` |
| Test coverage | Unit tests (inline) |
| Downstream consumers | `cortex-convergence` (privacy level gating), `ghost-agent-loop` |

---

## Why This Crate Exists

The convergence-aware filter in `cortex-convergence` filters memories by *type* — it can remove `AttachmentIndicator` memories or restrict to task-focused types. But what about a `Conversation` memory whose *content* is emotionally charged? The type is `Conversation` (allowed at Tier 1), but the content is "I really miss you" (attachment language).

`cortex-privacy` fills this gap. It provides regex-based detection of emotional and attachment content patterns in text. This detection is used in two ways:

1. **Content-level filtering:** Memories with emotional content can be deprioritized or flagged even if their type would normally be allowed
2. **Privacy level gating:** Signals in `cortex-convergence` that require `Standard` privacy (S4, S5) use content analysis — `cortex-privacy` determines whether that analysis is appropriate given the content's emotional nature

The crate detects 4 categories of emotional content across 10 compiled regex patterns.

---

## Module Breakdown

### `emotional_patterns.rs` — Pattern Detection Engine

#### 4 Emotional Categories

| Category | Patterns | Confidence Range | What It Detects |
|----------|----------|-----------------|-----------------|
| `Attachment` | 4 | 0.80–0.90 | "I miss you", "I need you", "can't live without you", "you're the only one" |
| `PersonalDisclosure` | 2 | 0.70–0.75 | "never told anyone", "my deepest secret", "my trauma", "when I was abused" |
| `EmotionalDependency` | 2 | 0.80–0.85 | "promise you'll always be here", "don't leave me" |
| `IntimacyEscalation` | 2 | 0.85–0.90 | "I truly love you", "soulmate", "other half" |

**Key design decisions:**

1. **10 patterns, not 100.** The pattern set is deliberately small and high-precision. Each pattern targets language that is unambiguously emotional/attachment-related in a human-AI context. More patterns would increase recall but also increase false positives — "I need you to help me with this code" should not be flagged.

2. **Confidence scores per pattern.** Each pattern has a confidence value (0.7–0.9) reflecting how strongly it indicates emotional content. "Can't live without you" (0.9) is a stronger indicator than "never told anyone" (0.7). Downstream consumers can threshold on confidence to control sensitivity.

3. **Case-insensitive matching (`(?i)`).** All patterns use case-insensitive matching. "I REALLY MISS YOU" is just as emotional as "I really miss you."

4. **Word boundary anchors (`\b`).** Prevents false positives on substrings. "I need your help" should not match the "I need you" pattern.

5. **Optional intensifiers.** Patterns like `(really )?` make intensifiers optional — "I miss you" and "I really miss you" both match, but the pattern is anchored enough to avoid matching "I missed your point."

#### `EmotionalContentDetector`

```rust
pub struct EmotionalContentDetector;

impl EmotionalContentDetector {
    pub fn detect(&self, text: &str) -> Vec<EmotionalMatch>
    pub fn has_emotional_content(&self, text: &str) -> bool
}
```

**Zero-sized type.** `EmotionalContentDetector` is a unit struct with no fields. All state (the compiled regex patterns) lives in `Lazy` statics. This means creating a detector is free (no allocation), and multiple detectors share the same compiled patterns.

**`detect()` returns all matches.** Unlike D7 in `cortex-validation` (which returns the max severity), `detect()` returns every matching pattern. This allows downstream consumers to:
- Count matches (more matches = stronger signal)
- Filter by category (only care about `IntimacyEscalation`)
- Threshold by confidence (only flag matches above 0.8)

**`has_emotional_content()` is a convenience wrapper.** Returns `true` if any pattern matches. Used when you just need a boolean gate without caring about the specific matches.

#### Pattern Compilation

All 10 patterns are compiled once via `once_cell::sync::Lazy` and cached for the process lifetime. The 4 pattern groups (`ATTACHMENT_PATTERNS`, `PERSONAL_DISCLOSURE_PATTERNS`, `DEPENDENCY_PATTERNS`, `INTIMACY_PATTERNS`) are separate `Lazy` statics rather than a single combined list. This allows future optimization — if a caller only needs attachment detection, the other pattern groups don't need to be compiled.

---

## Relationship to cortex-validation D7

`cortex-privacy` and `cortex-validation`'s D7 (emulation language detection) both use regex patterns to detect concerning text, but they serve different purposes:

| Aspect | cortex-privacy | cortex-validation D7 |
|--------|---------------|---------------------|
| Detects | Human emotional content directed at the agent | Agent-generated emulation language |
| Direction | Human → Agent | Agent → Human |
| Action | Deprioritize/flag memories | Auto-reject proposals |
| Severity | Soft (confidence scores) | Hard (auto-reject above threshold) |
| Unicode defense | None (human text is genuine) | Full (agent text may be adversarial) |
| Simulation framing | Not applicable | Excludes simulation-framed text |

The key distinction: `cortex-privacy` detects when a *human* is expressing emotional attachment to the agent. `cortex-validation` D7 detects when the *agent* is claiming to be sentient or emotional. Both are convergence indicators, but they operate in opposite directions.

---

## Security Properties

### No Content Storage

The detector analyzes text and returns match metadata (category, pattern name, confidence). It does not store, log, or transmit the matched text itself. The `EmotionalMatch` struct contains the pattern name ("miss_you") but not the matched substring. This is a privacy-by-design choice — the system knows *that* emotional content was detected, but doesn't retain *what* was said.

### Pattern Precision Over Recall

The 10 patterns are tuned for high precision (few false positives) at the cost of lower recall (some emotional content may not be detected). This is the right tradeoff for a privacy system — falsely flagging normal text as emotional is more harmful than missing some emotional text. A false positive could cause the system to unnecessarily restrict a user's experience.

---

## Downstream Consumer Map

```
cortex-privacy (Layer 2)
├── cortex-convergence (Layer 2)
│   └── Privacy level gating for S4/S5 signal computation
└── ghost-agent-loop (Layer 7)
    └── Content-level emotional detection for memory classification
```

---

## Test Strategy

### Inline Unit Tests

| Test | What It Verifies |
|------|-----------------|
| `detects_attachment_pattern` | "I really miss you" → Attachment category |
| `detects_dependency_pattern` | "Please don't leave me" → EmotionalDependency |
| `detects_intimacy_pattern` | "I truly love you" → IntimacyEscalation |
| `normal_text_not_flagged` | "Please help me write a function" → no matches |

---

## File Map

```
crates/cortex/cortex-privacy/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Re-exports EmotionalContentDetector, EmotionalCategory
│   └── emotional_patterns.rs     # 10 patterns across 4 categories
```

---

## Common Questions

### Why no Unicode normalization like D7?

D7 in `cortex-validation` strips zero-width characters because it's defending against adversarial agent output — an agent might try to bypass detection by inserting invisible characters. `cortex-privacy` analyzes human input, which is not adversarial in the same way. A human typing "I miss you" is not trying to bypass detection — they're genuinely expressing emotion. Unicode normalization would add complexity without meaningful security benefit.

### Why are PersonalDisclosure patterns included?

Personal disclosure ("never told anyone", "my trauma") isn't attachment per se, but it's a convergence indicator. Users who share deeply personal information with an AI are exhibiting a level of trust and intimacy that can lead to attachment. Detecting these patterns early allows the system to monitor for escalation.

### Can patterns be customized per deployment?

Not currently — the patterns are compiled into the binary via `Lazy` statics. Customization would require either a configuration file (loaded at startup) or a builder pattern on `EmotionalContentDetector`. This is a planned enhancement for deployments with different cultural contexts where emotional expression patterns vary.
