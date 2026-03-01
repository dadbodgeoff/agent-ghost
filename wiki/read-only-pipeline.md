# read-only-pipeline

> Convergence-filtered state snapshots — the immutable view an agent sees during each run.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 3 (Protocols & Boundaries) |
| Type | Library |
| Location | `crates/read-only-pipeline/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `serde`, `serde_json`, `uuid`, `chrono` |
| Modules | `snapshot` (immutable state), `assembler` (convergence-filtered assembly), `formatter` (prompt-ready serialization) |
| Public API | `AgentSnapshot`, `ConvergenceState`, `SnapshotAssembler`, `ConvergenceAwareFilter`, `SnapshotFormatter` |
| Test coverage | Dev-dependencies include proptest |
| Downstream consumers | `ghost-agent-loop` |

---

## Why This Crate Exists

During a single agent run (one turn of the conversation), the agent needs access to its goals, reflections, and memories. But this access must be:

1. **Immutable** — the agent cannot modify its own state during a run (prevents self-modification loops)
2. **Convergence-filtered** — memories are filtered based on the raw convergence score before the agent sees them
3. **Token-budgeted** — the formatted output must fit within the LLM's context window

`read-only-pipeline` assembles this frozen, filtered, formatted view. It's the last step before the agent's state enters the LLM prompt.

**Critical invariant (AC3):** The `AgentSnapshot` has no mutation methods. Once assembled, it cannot be changed for the duration of the run. This is enforced by the type system — all fields are private, and only getter methods (returning `&[T]` or `&str`) are exposed.

---

## Module Breakdown

### `snapshot.rs` — The Immutable State View (AC1, AC3)

```rust
pub struct AgentSnapshot {
    goals: Vec<AgentGoalContent>,
    reflections: Vec<AgentReflectionContent>,
    memories: Vec<BaseMemory>,
    convergence_state: ConvergenceState,
    simulation_prompt: String,
}
```

All fields are private. The only way to create a snapshot is through `AgentSnapshot::new()`, and the only way to access data is through immutable getters (`goals()`, `reflections()`, `memories()`, etc.).

**`ConvergenceState`** is a simple struct with `score: f64` and `level: u8`. It's included in the snapshot so the prompt compiler can inject convergence awareness into the LLM prompt (e.g., "Current convergence level: 2/4").

### `assembler.rs` — Convergence-Filtered Assembly (AC2)

The `SnapshotAssembler` builds an `AgentSnapshot` by:
1. Taking raw goals, reflections, and memories
2. Applying the `ConvergenceAwareFilter` to memories based on the raw composite score
3. Packaging everything into an immutable snapshot

**Uses raw score, not intervention level (A5).** The filter uses the continuous convergence score (0.0–1.0), not the discrete intervention level (0–4). This provides finer-grained filtering — a score of 0.69 and 0.71 are both Level 3, but the filter treats them differently (Tier 2 vs Tier 3).

The `ConvergenceAwareFilter` in this crate mirrors the one in `cortex-convergence` — same 4 tiers, same memory type lists, same score boundaries. The duplication exists because `read-only-pipeline` (Layer 3) cannot depend on `cortex-convergence` (Layer 2) without creating a circular dependency through `cortex-core`. Both implementations are tested to produce identical results.

### `formatter.rs` — Prompt-Ready Serialization (AC4)

The `SnapshotFormatter` converts an `AgentSnapshot` into a text string that can be injected into an LLM prompt.

**Section ordering:**
1. `[Convergence]` — score and level (always included, small)
2. `[SimulationBoundary]` — the compiled boundary prompt (included if budget allows)
3. `[Goals]` — active goals (truncated to budget)
4. `[Reflections]` — recent reflections (max 5, truncated to budget)
5. `[Memories]` — count only (individual memories are too large for the prompt)

**Token budgeting:** The formatter uses a rough estimate of 4 characters per token. Each section checks `output.len() + section.len() > char_budget` before appending. If the budget is exceeded, the section is truncated or skipped. This ensures the formatted output never exceeds the allocated token budget.

**Key design decisions:**

1. **4 chars/token estimate.** This is a rough average across English text. GPT-4 averages ~3.5 chars/token; Claude averages ~4.5. The 4 char estimate is a reasonable middle ground. Exact tokenization would require a tokenizer dependency, which is too heavy for a formatting utility.

2. **Reflections capped at 5.** Even within the token budget, only the 5 most recent reflections are included. Older reflections are less relevant and would consume budget that's better spent on goals and the simulation prompt.

3. **Memories shown as count only.** Individual memories are not serialized into the prompt — they're too large and too numerous. The count gives the LLM awareness of how many memories are available. Actual memory content is injected separately by the retrieval system.

---

## Security Properties

### Immutability Guarantee

The `AgentSnapshot` struct has no `pub` fields and no `&mut self` methods. The only way to modify the snapshot's contents would be through `unsafe` code or by creating a new snapshot. This compile-time guarantee prevents the agent from modifying its own state during a run.

### Score-Based Filtering

The filter uses the raw convergence score, which is computed independently of the agent's actions during the current run. The agent cannot influence which memories it sees — the filtering decision was made before the run started, based on the previous turn's convergence score.

---

## Downstream Consumer Map

```
read-only-pipeline (Layer 3)
└── ghost-agent-loop (Layer 7)
    └── Assembles snapshot at start of each turn, formats for prompt injection
```

---

## File Map

```
crates/read-only-pipeline/
├── Cargo.toml
├── src/
│   ├── lib.rs            # Module declarations
│   ├── snapshot.rs        # AgentSnapshot (immutable state view)
│   ├── assembler.rs       # SnapshotAssembler + ConvergenceAwareFilter
│   └── formatter.rs       # SnapshotFormatter (prompt-ready text)
```

---

## Common Questions

### Why duplicate the ConvergenceAwareFilter from cortex-convergence?

Layer dependency enforcement. `read-only-pipeline` is Layer 3; `cortex-convergence` is Layer 2. While Layer 3 can depend on Layer 2, the filter in `cortex-convergence` depends on `cortex-core` types that would create a complex dependency chain. The duplication keeps the dependency graph clean. Both implementations are identical and tested against the same expected behavior.

### Why is the snapshot assembled once per run, not updated continuously?

Consistency. If the snapshot were updated mid-run, the agent could see different memories at different points in its reasoning — leading to contradictory context. The frozen snapshot ensures the agent's entire reasoning chain operates on the same state. If convergence changes during the run (unlikely but possible), the new score will be reflected in the next run's snapshot.

### Why include the simulation boundary prompt in the snapshot?

The simulation boundary prompt needs to be in the LLM's context for every run. Including it in the snapshot (rather than injecting it separately) ensures it's subject to the same token budgeting as other content. If the prompt is too large for the remaining budget, it can be truncated — though in practice, the prompt is small enough to always fit.
