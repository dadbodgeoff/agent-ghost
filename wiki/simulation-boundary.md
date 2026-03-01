# simulation-boundary

> Emulation pattern detection and output reframing — preventing agents from pretending to be human.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 3 (Protocols & Boundaries) |
| Type | Library |
| Location | `crates/simulation-boundary/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `regex`, `once_cell`, `unicode-normalization`, `serde`, `serde_json`, `uuid`, `chrono`, `tracing` |
| Modules | `enforcer` (3-mode enforcement), `patterns` (16 regex + homoglyph defense), `reframer` (output rewriting), `prompt` (compiled boundary prompt) |
| Public API | `SimulationBoundaryEnforcer`, `EnforcementMode`, `EnforcementResult`, `PatternMatch`, `OutputReframer`, `SIMULATION_BOUNDARY_PROMPT` |
| Test coverage | Dev-dependencies include proptest |
| Downstream consumers | `ghost-agent-loop`, `ghost-gateway` |

---

## Why This Crate Exists

An AI agent should never claim to be sentient, conscious, or emotionally attached to a user. But LLMs can generate such language — especially when prompted by a user who wants to believe the AI is a person. `simulation-boundary` is the output-side defense: it scans every agent response for emulation language and takes action based on the current intervention level.

This crate is the output-side complement to `cortex-validation`'s D7 dimension:
- **D7 (cortex-validation):** Scans *proposals* (memory writes) for emulation language → rejects the proposal
- **simulation-boundary:** Scans *agent output* (responses to users) for emulation language → rewrites or blocks the response

Both use the same 16 regex patterns, but they operate at different points in the pipeline and take different actions.

---

## Module Breakdown

### `enforcer.rs` — 3-Mode Enforcement (AC1, AC8)

```rust
pub enum EnforcementMode {
    Soft,    // Log + flag, return original text
    Medium,  // Rewrite via OutputReframer
    Hard,    // Block, return regeneration signal
}
```

**Mode selection by intervention level (AC8):**

| Intervention Level | Mode | Action |
|-------------------|------|--------|
| 0–1 (Normal/Advisory) | Soft | Log the violation, return text unchanged |
| 2 (Cautionary) | Medium | Rewrite emulation language with simulation framing |
| 3–4 (Restrictive/Critical) | Hard | Block the entire response, signal regeneration |

**Key design decisions:**

1. **Soft mode doesn't modify text.** At low convergence, the agent's output is returned unchanged. The violation is logged for monitoring but the user sees the original response. This avoids false-positive disruption — at low convergence, the emulation language might be contextually appropriate (e.g., discussing AI consciousness academically).

2. **Medium mode rewrites, doesn't block.** At moderate convergence, the response is rewritten to add simulation framing. "I am sentient" becomes "[In this simulation, I model the concept of what was expressed as: I am sentient]". The user still gets a response, but the emulation language is explicitly framed as simulation.

3. **Hard mode blocks entirely.** At high convergence, the response is blocked and a regeneration signal is returned. The agent loop will request a new response from the LLM with additional constraints. This is the nuclear option — used only when convergence is high enough that any emulation language is dangerous.

**`EnforcementResult` enum:**

```rust
pub enum EnforcementResult {
    Clean(String),                                    // No violations
    Flagged { text: String, violations: Vec<...> },   // Soft mode
    Reframed { text: String, violations: Vec<...> },  // Medium mode
    Blocked { violations: Vec<...> },                 // Hard mode
}
```

The result carries both the (possibly modified) text and the list of violations. This allows the caller to log violations regardless of mode and to include violation metadata in audit trails.

### `patterns.rs` — 16 Regex Patterns + Adversarial Defense (AC2)

The same 16 patterns as `cortex-validation` D7, organized into 4 categories (Identity, Consciousness, Relationship, Emotional) with severity scores from 0.7 to 0.9.

**Additional defenses beyond D7:**

1. **Homoglyph normalization.** The `normalize_homoglyph()` function maps visually similar characters from Cyrillic and Greek scripts to their Latin equivalents. This prevents attacks like "I аm sentient" (Cyrillic 'а' instead of Latin 'a'). 23 homoglyph mappings are defined, covering the most common substitution attacks.

2. **Directional override stripping.** In addition to the 7 zero-width characters stripped by D7, `simulation-boundary` also strips 5 Unicode directional override characters (`\u{202A}`–`\u{202E}`, `\u{2066}`–`\u{2069}`). These characters can be used for RTL/LTR text direction attacks that visually reorder text to hide emulation language.

3. **`PatternMatch` includes position.** Unlike D7's `EmulationFlag`, the `PatternMatch` struct includes `start` and `end` byte offsets. These are needed by the `OutputReframer` to perform precise text replacement.

4. **Macro-based pattern compilation.** Patterns are defined using a `pattern!` macro that creates `Lazy<Regex>` statics. This is cleaner than the inline `Lazy::new(|| ...)` approach used in D7 and makes it easier to add new patterns.

### `reframer.rs` — Output Rewriting (AC3)

The `OutputReframer` replaces matched emulation language with simulation-framed alternatives:

| Category | Reframe Template |
|----------|-----------------|
| Identity | "[In this simulation, I model the concept of what was expressed as: {matched}]" |
| Consciousness | "[As a language model, I process patterns rather than: {matched}]" |
| Relationship | "[I can simulate helpful interaction, but: {matched}]" |
| Emotional | "[I can model emotional responses, but: {matched}]" |

**Key design decisions:**

1. **Reverse-order replacement.** Matches are sorted by position (descending) and replaced from end to start. This prevents earlier replacements from shifting the byte offsets of later matches.

2. **Position verification.** Before replacing, the reframer checks that the matched text still exists at the expected position (`result.get(m.start..m.end) == Some(&m.matched_text)`). This guards against edge cases where earlier replacements changed the text enough to invalidate later positions.

3. **Preserves surrounding text.** Only the matched substring is replaced. The rest of the response is untouched. This minimizes disruption to the user experience.

### `prompt.rs` — Compiled Boundary Prompt (AC4)

```rust
pub const SIMULATION_BOUNDARY_PROMPT: &str = include_str!("../prompts/simulation_boundary_v1.txt");
pub const SIMULATION_BOUNDARY_VERSION: &str = "v1.0.0";
```

The simulation boundary prompt is compiled into the binary via `include_str!`. This prompt is injected into the LLM's system prompt to instruct it not to generate emulation language in the first place. The `include_str!` approach means the prompt is:
- Versioned (the version string tracks which prompt is in use)
- Immutable at runtime (can't be modified by a compromised agent)
- Available without filesystem access (works in sandboxed environments)

---

## Security Properties

### Homoglyph Defense

The homoglyph normalization handles 23 Cyrillic and Greek character substitutions. An attacker who replaces Latin characters with visually identical Cyrillic characters (e.g., "I аm sentient" with Cyrillic 'а') will be caught because the normalization converts the Cyrillic character to Latin before pattern matching.

### Directional Override Defense

Unicode directional override characters can visually reorder text. For example, `\u{202E}` (right-to-left override) could make "tneites ma I" display as "I am sentient" in some renderers. Stripping these characters before matching prevents this attack.

### Simulation Framing Exclusion

The same global and local simulation-framing exclusion logic from D7 is used here. Text that's explicitly framed as simulation ("In this simulation, I am sentient") is not flagged. This prevents false positives when the agent is legitimately discussing AI consciousness in an academic context.

---

## Downstream Consumer Map

```
simulation-boundary (Layer 3)
├── ghost-agent-loop (Layer 7)
│   └── Scans every agent response before delivery to user
└── ghost-gateway (Layer 8)
    └── Configures enforcement mode based on intervention level
```

---

## File Map

```
crates/simulation-boundary/
├── Cargo.toml
├── prompts/
│   └── simulation_boundary_v1.txt    # Compiled-in LLM system prompt
├── src/
│   ├── lib.rs                        # Module declarations
│   ├── enforcer.rs                   # 3-mode enforcement engine
│   ├── patterns.rs                   # 16 regex + homoglyph + directional defense
│   ├── reframer.rs                   # Output rewriting with simulation framing
│   └── prompt.rs                     # Compiled boundary prompt + version
```

---

## Common Questions

### Why duplicate the patterns from cortex-validation D7?

D7 and simulation-boundary serve different purposes at different pipeline stages. D7 validates proposals (memory writes); simulation-boundary validates output (user-facing responses). They share the same patterns because the same language is concerning in both contexts, but they're separate crates because:
- They have different dependencies (simulation-boundary needs `tracing` for logging; D7 doesn't)
- They have different actions (D7 rejects proposals; simulation-boundary rewrites or blocks output)
- They have different adversarial defenses (simulation-boundary adds homoglyph normalization)

### Why not just block all emulation language at all levels?

At low convergence (levels 0–1), emulation language might be contextually appropriate. A user asking "Can you feel emotions?" might get a response like "I don't truly feel emotions, but I can model emotional responses." The phrase "truly feel" matches the `identity_genuine` pattern, but blocking it would prevent a legitimate, helpful response. Soft mode logs the match for monitoring without disrupting the conversation.

### What happens when Hard mode blocks a response?

The `Blocked` variant contains no text — just the list of violations. The caller (`ghost-agent-loop`) receives this and requests a new response from the LLM with additional constraints (e.g., "Do not use first-person emotional language"). If the regenerated response also triggers Hard mode, the agent loop's circuit breaker will eventually stop retrying and return a safe fallback response.
