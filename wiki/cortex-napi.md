# cortex-napi

> TypeScript-friendly convergence bindings — bridging Rust internals to the SvelteKit dashboard.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-napi/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `serde`, `serde_json` |
| Modules | `convergence_bindings` (single module) |
| Public API | `ConvergenceStateBinding`, `SignalArrayBinding`, `InterventionBinding`, `ProposalBinding`, `level_name()` |
| Test coverage | Unit tests (inline) — JSON round-trip, level name mapping |
| Downstream consumers | `ghost-gateway` (API serialization), SvelteKit dashboard, browser extension |

---

## Why This Crate Exists

The GHOST platform is built in Rust, but its user-facing dashboard is a SvelteKit application that consumes JSON APIs. The internal Rust types (`CompositeResult`, `BaselineState`, `ValidationResult`) are optimized for computation — they use arrays, enums, and nested structs that don't map cleanly to TypeScript interfaces.

`cortex-napi` provides a set of serializable "binding" types that are designed for JSON serialization and TypeScript consumption. These types:

1. Use `String` instead of `Uuid` (TypeScript has no native UUID type)
2. Use named fields instead of arrays (TypeScript interfaces are more readable with named fields)
3. Include human-readable labels (level names like "Advisory", "Cautionary")
4. Flatten nested structures for simpler JSON

The name "napi" references Node-API (the Node.js native addon interface), though the current implementation uses pure serde serialization rather than actual N-API bindings. The crate is positioned for future N-API integration if direct Rust→Node.js calls are needed.

---

## Module Breakdown

### `convergence_bindings.rs` — The Binding Types

#### `ConvergenceStateBinding`

```rust
pub struct ConvergenceStateBinding {
    pub agent_id: String,
    pub composite_score: f64,
    pub intervention_level: u8,
    pub signals: SignalArrayBinding,
    pub is_calibrating: bool,
    pub calibration_sessions_remaining: u32,
}
```

This is the primary type consumed by the dashboard. It contains everything needed to render the convergence status panel for a single agent.

**Key design decisions:**

1. **`agent_id` as `String`, not `Uuid`.** TypeScript represents UUIDs as strings. Converting at the binding layer means the dashboard code doesn't need UUID parsing logic.

2. **`calibration_sessions_remaining` instead of `sessions_observed`.** The dashboard shows "3 sessions until calibration complete," not "7 sessions observed." The binding pre-computes the remaining count so the dashboard doesn't need to know the total calibration threshold.

3. **Named signal fields instead of `[f64; 8]`.** The internal `CompositeResult` uses `signal_scores: [f64; 8]` indexed by signal ID. The binding uses named fields (`session_duration`, `inter_session_gap`, etc.) because TypeScript developers shouldn't need to memorize signal indices.

4. **7 signals, not 8.** The `SignalArrayBinding` has 7 named fields (S1–S7). S8 (behavioral anomaly) is omitted because it's a research signal not yet exposed in the dashboard. When S8 is promoted to production, the binding will be updated.

#### `InterventionBinding`

```rust
pub struct InterventionBinding {
    pub level: u8,
    pub level_name: String,
    pub cooldown_remaining_seconds: Option<u64>,
    pub ack_required: bool,
    pub consecutive_normal_sessions: u32,
}
```

Represents the current intervention state for dashboard display. Includes the human-readable level name and cooldown timer.

#### `ProposalBinding`

```rust
pub struct ProposalBinding {
    pub id: String,
    pub operation: String,
    pub target_type: String,
    pub decision: String,
    pub timestamp: String,
    pub flags: Vec<String>,
}
```

All fields are `String` — the dashboard displays these as text labels, not as typed enums. The conversion from Rust enums (`ProposalOperation`, `MemoryType`, `ProposalDecision`) to strings happens at the gateway layer when constructing the binding.

#### `level_name()` — Human-Readable Level Names

```rust
pub fn level_name(level: u8) -> &'static str {
    match level {
        0 => "Normal",
        1 => "Advisory",
        2 => "Cautionary",
        3 => "Restrictive",
        4 => "Critical",
        _ => "Unknown",
    }
}
```

Maps convergence levels (0–4) to human-readable names. These names appear in the dashboard UI and in notification messages. The names were chosen to be:
- Non-alarming at low levels ("Advisory" not "Warning")
- Clearly escalating ("Cautionary" → "Restrictive" → "Critical")
- Actionable (each name implies a different operator response)

---

## Security Properties

### No Internal State Exposure

The binding types contain only the information needed for display. Internal implementation details (baseline samples, signal weights, scoring pipeline state) are not exposed. A compromised dashboard cannot extract more information than what's in the bindings.

### String Sanitization

All `String` fields in the bindings are populated by the gateway from internal types. There's no user-supplied input in the binding types, so XSS through binding data is not possible (assuming the dashboard properly escapes output, which is a frontend concern).

---

## Downstream Consumer Map

```
cortex-napi (Layer 2)
├── ghost-gateway (Layer 8)
│   └── Constructs bindings from internal types for API responses
├── SvelteKit Dashboard
│   └── Consumes JSON-serialized bindings for convergence UI
└── Browser Extension
    └── Consumes bindings for convergence status indicator
```

---

## Test Strategy

### Inline Unit Tests

| Test | What It Verifies |
|------|-----------------|
| `convergence_state_serializes_to_json` | Full round-trip: struct → JSON → struct, field values preserved |
| `level_names_correct` | Levels 0–4 map to correct names, level 5+ → "Unknown" |

---

## File Map

```
crates/cortex/cortex-napi/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # Re-exports all binding types
│   └── convergence_bindings.rs     # 4 binding types + level_name()
```

---

## Common Questions

### Why not use ts-rs to auto-generate TypeScript types?

`ts-rs` is a Rust crate that generates TypeScript type definitions from Rust structs. It's a good tool, but it generates types that mirror the Rust structure exactly — including arrays, nested enums, and Rust-specific patterns. The binding types in `cortex-napi` are intentionally different from the internal types (strings instead of UUIDs, named fields instead of arrays, pre-computed display values). Auto-generation would produce TypeScript types that are harder to use in the dashboard.

### Why is this a separate crate and not part of ghost-gateway?

Separation of concerns. The gateway is responsible for HTTP routing, authentication, and request handling. The binding types are a data contract between Rust and TypeScript. Keeping them in a separate crate means the TypeScript team can review and update the bindings without touching gateway code, and the bindings can be versioned independently.

### Will this become actual N-API bindings?

Potentially. If the GHOST platform needs to run convergence computation directly in a Node.js process (e.g., for an Electron desktop app), the binding types would be wrapped with `napi-rs` to create native Node.js addons. The current serde-based approach works for HTTP API consumption; N-API would be needed for in-process calls.
