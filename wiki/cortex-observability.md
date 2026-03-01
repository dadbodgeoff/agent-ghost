# cortex-observability

> Prometheus-compatible convergence metrics — making the invisible visible.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 2 (Cortex Higher-Order) |
| Type | Library |
| Location | `crates/cortex/cortex-observability/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `serde`, `serde_json`, `tracing` |
| Modules | `convergence_metrics` (single module) |
| Public API | `ConvergenceMetrics`, `MetricsSnapshot`, `to_prometheus()` |
| Test coverage | Unit tests (inline) |
| Downstream consumers | `ghost-gateway`, `convergence-monitor` |

---

## Why This Crate Exists

You can't manage what you can't measure. GHOST computes convergence scores, intervention levels, and 8 signal values per agent per turn — but without an observability layer, this data is invisible to operators. `cortex-observability` bridges the gap between the convergence engine and external monitoring systems.

The crate provides a thread-safe metrics registry that:
1. Stores per-agent convergence scores (gauges)
2. Stores per-agent intervention levels (gauges)
3. Counts total interventions and boundary violations (counters)
4. Stores per-agent signal values (8-element arrays)
5. Exports everything in Prometheus text exposition format

This is intentionally minimal — it's a metrics bridge, not a full observability platform. The actual dashboarding, alerting, and long-term storage happen in external systems (Prometheus, Grafana, etc.).

---

## Module Breakdown

### `convergence_metrics.rs` — The Metrics Registry

```rust
pub struct ConvergenceMetrics {
    scores: RwLock<BTreeMap<String, f64>>,      // per-agent convergence score
    levels: RwLock<BTreeMap<String, u8>>,        // per-agent intervention level
    intervention_count: AtomicU64,               // total interventions
    violation_count: AtomicU64,                  // total violations
    signals: RwLock<BTreeMap<String, [f64; 8]>>, // per-agent signal values
}
```

**Key design decisions:**

1. **`RwLock` for maps, `AtomicU64` for counters.** The per-agent maps use `RwLock` because they need concurrent read access (Prometheus scrapes) with occasional writes (score updates). The counters use `AtomicU64` because they're write-heavy (incremented every turn) and don't need read-side locking. This hybrid approach minimizes contention — readers never block each other, and counter increments are lock-free.

2. **`BTreeMap`, not `HashMap`.** The maps use `BTreeMap` for deterministic iteration order. When Prometheus scrapes the metrics endpoint, the output should be stable — the same agents should appear in the same order. `HashMap` iteration order is random, which would cause unnecessary diff noise in monitoring tools.

3. **`Relaxed` ordering on atomics.** The counters use `Ordering::Relaxed` because exact ordering doesn't matter for metrics — if an intervention count is briefly stale by one increment, that's fine. `SeqCst` would add unnecessary synchronization overhead.

4. **Graceful lock failure.** All `write()` and `read()` calls use `if let Ok(...)` rather than `.unwrap()`. If a lock is poisoned (a thread panicked while holding it), the metrics operation silently fails rather than crashing the process. Metrics are observability, not safety — a missed metric update is acceptable; a process crash is not.

5. **Agent ID as `String`, not `Uuid`.** The maps are keyed by `String` rather than `Uuid` because Prometheus labels are strings. Converting `Uuid` → `String` at every update would be wasteful. The caller converts once when calling `set_score()`.

#### Prometheus Text Exposition

```rust
pub fn to_prometheus(&self) -> String
```

Generates Prometheus-compatible text format with proper `# HELP` and `# TYPE` annotations:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `ghost_convergence_score` | gauge | `agent_id` | Current convergence score per agent |
| `ghost_intervention_level` | gauge | `agent_id` | Current intervention level (0–4) per agent |
| `ghost_intervention_total` | counter | none | Total intervention activations across all agents |
| `ghost_violation_total` | counter | none | Total boundary violations across all agents |

**Why not use the `prometheus` crate?** The `prometheus` crate is a full-featured metrics library with global registries, histogram buckets, and label cardinality management. For GHOST's needs — a handful of gauges and counters — it's overkill. The hand-rolled text format is ~30 lines of code, has zero dependencies, and produces valid Prometheus exposition format. If the metrics surface grows significantly, migrating to the `prometheus` crate would be straightforward.

#### `MetricsSnapshot`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub scores: BTreeMap<String, f64>,
    pub levels: BTreeMap<String, u8>,
    pub intervention_count: u64,
    pub violation_count: u64,
    pub signals: BTreeMap<String, [f64; 8]>,
}
```

The snapshot is a serializable point-in-time copy of all metrics. It's used for:
- JSON API responses (the gateway exposes `/metrics` as JSON)
- Internal state inspection during testing
- Audit log entries that capture the metrics state at a specific moment

The snapshot clones all data, so it's safe to hold across await points without blocking metric updates.

---

## Security Properties

### No PII in Metrics

Agent IDs are UUIDs, not human-readable names. The metrics output contains no personally identifiable information — an operator can see "agent abc-123 has convergence score 0.7" but cannot determine which user is associated with that agent from the metrics alone.

### Lock Poisoning Resilience

If a thread panics while holding a `RwLock`, the lock becomes poisoned. All subsequent operations on that lock will fail. The metrics registry handles this gracefully — poisoned locks cause silent no-ops rather than panics. This prevents a bug in one part of the system from cascading into a metrics-related crash.

---

## Downstream Consumer Map

```
cortex-observability (Layer 2)
├── ghost-gateway (Layer 8)
│   └── Exposes /metrics endpoint (Prometheus + JSON)
└── convergence-monitor (Layer 9)
    └── Publishes metrics from independent convergence verification
```

---

## Test Strategy

### Inline Unit Tests

| Test | What It Verifies |
|------|-----------------|
| `metrics_registered_and_updated` | Set score, level, signals; increment counters; verify snapshot |
| `prometheus_format_valid` | Output contains all expected metric names and TYPE annotations |

---

## File Map

```
crates/cortex/cortex-observability/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # Re-exports ConvergenceMetrics
│   └── convergence_metrics.rs      # Registry, snapshot, Prometheus format
```

---

## Common Questions

### Why is this a separate crate and not part of cortex-convergence?

Separation of concerns. `cortex-convergence` computes convergence scores. `cortex-observability` exports them. Combining them would mean the convergence engine depends on serialization and text formatting libraries, and the metrics layer depends on signal computation. Keeping them separate means you can use the convergence engine without metrics (e.g., in tests) and the metrics layer without recomputing convergence (e.g., when reading cached scores).

### Why no histograms for signal computation latency?

The crate description mentions histograms, but the current implementation only has gauges and counters. Histograms are planned for tracking signal computation latency (how long each of the 8 signals takes to compute) and scoring pipeline latency. They'll be added when the convergence monitor sidecar is instrumented for performance profiling.

### Why `[f64; 8]` for signals instead of a named struct?

The 8-signal array is the canonical representation throughout the convergence pipeline. Using a named struct with fields like `session_duration`, `inter_session_gap`, etc. would be more readable but would require updating the struct every time a signal is added or removed. The array is indexed by signal ID (S1=0, S2=1, ..., S8=7), which is stable across signal changes.
