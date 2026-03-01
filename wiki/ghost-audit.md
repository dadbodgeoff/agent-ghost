# ghost-audit

> Queryable audit log engine — paginated queries, aggregation summaries, and multi-format export over the append-only audit tables managed by cortex-storage.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 6 (Data Services) |
| Type | Library |
| Location | `crates/ghost-audit/` |
| Workspace deps | `cortex-core`, `cortex-storage` |
| External deps | `rusqlite`, `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, `tracing` |
| Modules | `query_engine` (paginated queries), `aggregation` (summary stats), `export` (JSON/CSV/JSONL) |
| Public API | `AuditQueryEngine`, `AuditAggregation`, `AuditExporter`, `AuditFilter`, `AuditEntry` |
| Export formats | JSON, CSV, JSONL |
| Test coverage | Insert/query, pagination, full-text search, aggregation, export format validation |
| Downstream consumers | `ghost-gateway` (API routes), `cortex-napi` (dashboard) |

---

## Why This Crate Exists

Every security-critical event in GHOST is logged: policy denials, boundary violations, sandbox escapes, kill gate activations, convergence escalations. These events are written to append-only SQLite tables by `cortex-storage`. But raw database rows aren't useful without a query layer.

`ghost-audit` provides three capabilities over the raw audit data:

1. **Paginated queries** with multi-dimensional filtering (time range, agent, event type, severity, tool, full-text search). The dashboard needs to show "all violations for agent X in the last 24 hours, page 2 of 5."

2. **Aggregation summaries** for trend analysis: violations per day, top violation types, policy denials by tool, boundary violations by pattern. These power the dashboard's overview panels.

3. **Multi-format export** for external analysis: JSON (human-readable), CSV (spreadsheet import), JSONL (log pipeline ingestion).

---

## Module Breakdown

### `query_engine.rs` — Paginated Audit Queries

The query engine is the primary interface for reading audit data. It builds parameterized SQL queries from a filter struct, executes them against the SQLite database, and returns paginated results.

#### The Audit Entry

```rust
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub agent_id: String,
    pub event_type: String,      // "violation", "policy_denial", "boundary_violation", etc.
    pub severity: String,        // "low", "medium", "high", "critical"
    pub tool_name: Option<String>,
    pub details: String,
    pub session_id: Option<String>,
}
```

**Why strings instead of enums for event_type and severity?** Extensibility. New event types and severity levels can be added without modifying the audit crate. The audit log is a recording system — it should accept whatever the upstream emitters produce, not constrain them.

#### The Filter

```rust
pub struct AuditFilter {
    pub time_start: Option<String>,
    pub time_end: Option<String>,
    pub agent_id: Option<String>,
    pub event_type: Option<String>,
    pub severity: Option<String>,
    pub tool_name: Option<String>,
    pub search: Option<String>,    // Full-text search in details
    pub page: u32,                 // Default: 1
    pub page_size: u32,            // Default: 50, max: 1000
}
```

**Design decisions:**

1. **All fields optional.** An empty filter returns all entries. Each field narrows the result set. This composable design lets the dashboard build filters incrementally as the user clicks facets.

2. **Full-text search via LIKE.** The `search` field maps to `details LIKE '%search%'`. This is simple but effective for the expected data volumes (thousands to tens of thousands of entries per agent). If performance becomes an issue, SQLite's FTS5 extension could be added without changing the API.

3. **Page size capped at 1000.** `page_size.max(1).min(1000)` prevents both zero-size pages and unbounded result sets. A malicious or buggy client can't request page_size=MAX_INT and OOM the process.

#### Dynamic SQL Construction

The query engine builds SQL dynamically from the filter:

```rust
// Each filter field adds a condition and a parameter
if let Some(ref agent) = filter.agent_id {
    conditions.push(format!("agent_id = ?{}", param_values.len() + 1));
    param_values.push(Box::new(agent.clone()));
}
```

**Why dynamic SQL instead of a fixed query with NULLable parameters?** Performance. SQLite's query planner optimizes better when conditions are present or absent rather than using `WHERE (agent_id = ?1 OR ?1 IS NULL)` patterns. The dynamic approach also produces cleaner EXPLAIN output for debugging.

**Parameterized queries prevent SQL injection.** All user-provided values go through `?N` placeholders — never string interpolation. The `Box<dyn ToSql>` pattern allows heterogeneous parameter types in a single vector.

#### Table Schema and Indexes

```sql
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info',
    tool_name TEXT,
    details TEXT NOT NULL DEFAULT '',
    session_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_agent ON audit_log(agent_id);
CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_log(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_severity ON audit_log(severity);
```

Four indexes cover the most common query patterns. The `timestamp` index supports time-range queries and the default `ORDER BY timestamp DESC`. The `agent_id` index supports per-agent filtering (the most common dashboard view).

---

### `aggregation.rs` — Summary Statistics

The aggregation engine computes dashboard-ready summaries from the audit data.

#### Five Aggregation Dimensions

| Aggregation | SQL Pattern | Use Case |
|-------------|-------------|----------|
| `violations_per_day` | `GROUP BY DATE(timestamp)` | Trend chart: "are violations increasing?" |
| `top_violation_types` | `GROUP BY severity` | Bar chart: "what severity dominates?" |
| `policy_denials_by_tool` | `GROUP BY tool_name` | Table: "which tools get denied most?" |
| `boundary_violations_by_pattern` | `GROUP BY details` | Table: "what patterns trigger violations?" |
| `total_entries` | `COUNT(*)` | Summary stat: total audit volume |

**Why compute aggregations in SQL instead of Rust?** SQLite is optimized for this. `GROUP BY` with `COUNT(*)` on indexed columns is orders of magnitude faster than fetching all rows and aggregating in application code. The audit table could have millions of rows — aggregation must be efficient.

**Optional agent_id filter:** All aggregations accept an optional `agent_id` parameter. When provided, aggregations are scoped to a single agent. When omitted, they cover the entire platform. This supports both the per-agent detail view and the platform overview dashboard.

---

### `export.rs` — Multi-Format Export

The exporter writes audit entries to any `Write` implementor in three formats.

#### Three Formats

| Format | Use Case | Implementation |
|--------|----------|----------------|
| JSON | Human-readable, API responses | `serde_json::to_string_pretty` |
| CSV | Spreadsheet import, Excel analysis | Manual with `csv_escape()` |
| JSONL | Log pipeline ingestion (Splunk, ELK) | One `serde_json::to_string` per line |

**Why manual CSV instead of the `csv` crate?** Dependency minimization. The CSV format is simple (8 columns, standard escaping). The `csv_escape()` function handles the three cases that require quoting: commas, double quotes, and newlines. Adding a full CSV library for this would be overkill.

**JSONL for streaming.** JSONL (one JSON object per line) is the standard format for log pipeline ingestion. Each line is independently parseable, so a consumer can process entries as they arrive without buffering the entire file.

---

## Security Properties

### Append-Only Guarantee

The audit table is managed by `cortex-storage`, which enforces append-only semantics via database triggers. `ghost-audit` only reads — it never deletes or updates entries. This means the audit trail is tamper-evident: if an entry exists, it was written at the recorded time and has not been modified.

### Parameterized Queries

All SQL queries use parameterized placeholders (`?N`). No string interpolation of user input. This eliminates SQL injection as an attack vector.

### Page Size Limits

The maximum page size is capped at 1000 entries. This prevents denial-of-service via unbounded queries that could exhaust memory.

---

## Downstream Consumer Map

```
ghost-audit (Layer 6)
├── ghost-gateway (Layer 8)
│   └── /api/audit/* routes serve paginated queries
│   └── /api/audit/export serves CSV/JSON/JSONL downloads
│   └── /api/audit/summary serves aggregation results
└── cortex-napi (Layer 2)
    └── TypeScript bindings for SvelteKit dashboard
    └── Renders violation trends, policy denial tables
```

---

## Test Strategy

### Integration Tests (`tests/audit_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `insert_and_query_with_filter` | Agent-scoped filter returns correct entries |
| `pagination_page1_and_page2` | Page boundaries work correctly with 10 entries |
| `full_text_search` | `search` field matches substring in details |
| `aggregation_returns_correct_counts` | Total entries count matches inserted data |
| `export_json_valid` | JSON export produces valid, parseable JSON array |
| `export_csv_valid_with_headers` | CSV has header row + correct data row count |
| `export_jsonl_valid` | Each JSONL line is independently valid JSON |
| `query_no_results_returns_empty_list` | Empty database → empty items, total=0 |
| `query_page_beyond_data_returns_empty` | Page 100 of 1 entry → empty items, total=1 |

---

## File Map

```
crates/ghost-audit/
├── Cargo.toml                          # Deps: cortex-core, cortex-storage, rusqlite
├── src/
│   ├── lib.rs                          # Re-exports, public API surface
│   ├── query_engine.rs                 # AuditQueryEngine, AuditFilter, pagination
│   ├── aggregation.rs                  # AuditAggregation, 5 summary dimensions
│   └── export.rs                       # AuditExporter, JSON/CSV/JSONL
└── tests/
    └── audit_tests.rs                  # Query, pagination, search, aggregation, export tests
```

---

## Common Questions

### Why SQLite instead of a dedicated time-series database?

GHOST runs as a single process on a developer's machine. SQLite is embedded, zero-config, and handles the expected audit volume (thousands of entries per day) with ease. A time-series database (InfluxDB, TimescaleDB) would add operational complexity for no benefit at this scale.

### How do I query audit data from the dashboard?

The gateway exposes `/api/audit/query` (POST with `AuditFilter` body), `/api/audit/summary` (GET with optional `agent_id`), and `/api/audit/export` (GET with format parameter). The SvelteKit dashboard calls these via `cortex-napi` TypeScript bindings.

### Can audit entries be deleted?

No. The append-only guarantee is enforced at the `cortex-storage` level via database triggers. Any `DELETE` or `UPDATE` on the audit table will fail. This is a deliberate security property — audit trails must be immutable for forensic integrity.
