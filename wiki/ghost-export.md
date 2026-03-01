# ghost-export

> External AI platform data import — parse conversation history from ChatGPT, Claude, Character.AI, Gemini, and generic JSONL, reconstruct session timelines, and compute convergence baselines.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 6 (Data Services) |
| Type | Library |
| Location | `crates/ghost-export/` |
| Workspace deps | None (standalone) |
| External deps | `serde`, `serde_json`, `chrono`, `uuid`, `thiserror`, `tracing` |
| Modules | `parsers/` (5 platform parsers), `timeline` (session reconstruction), `analyzer` (orchestration + scoring) |
| Public API | `ExportAnalyzer`, `ExportAnalysisResult`, `TimelineReconstructor`, `ExportParser` trait |
| Supported platforms | ChatGPT, Claude.ai, Character.AI, Google Takeout (Gemini), Generic JSONL |
| Test coverage | Parser detection, message parsing, timeline reconstruction, full pipeline, error handling |
| Downstream consumers | `ghost-gateway` (import API), `ghost-migrate` (baseline computation) |

---

## Why This Crate Exists

Users switching to GHOST from other AI platforms have months or years of conversation history. That history contains valuable signal: how the user communicates, what topics they discuss, how long their sessions last, and what patterns indicate productive vs. problematic interactions.

`ghost-export` extracts this signal by:

1. **Auto-detecting the export format** — Drop a file in, and the analyzer figures out if it's ChatGPT, Claude, Character.AI, Gemini, or generic JSONL.
2. **Normalizing messages** — Every platform has a different JSON structure. All messages are normalized to `NormalizedMessage` with timestamp, sender role, content, and optional session ID.
3. **Reconstructing session boundaries** — Some exports include session IDs; others don't. The timeline reconstructor infers session boundaries from timestamp gaps (default: 1-hour gap = new session).
4. **Computing convergence baselines** — Per-session scores estimate convergence characteristics from the historical data, giving GHOST a starting point instead of a cold start.

---

## Module Breakdown

### `parsers/` — Five Platform Parsers

Each parser implements the `ExportParser` trait:

```rust
pub trait ExportParser: Send + Sync {
    fn detect(&self, path: &Path) -> bool;
    fn parse(&self, path: &Path) -> ExportResult<Vec<NormalizedMessage>>;
    fn name(&self) -> &str;
}
```

**Detection priority:** Parsers are tried in order: ChatGPT → Character.AI → Google Takeout → Claude → JSONL. The first parser whose `detect()` returns true wins. JSONL is last because it's the most permissive (any `.jsonl` or `.ndjson` file matches).

#### ChatGPT Parser (`chatgpt.rs`)

**Detection:** File contains both `"mapping"` and `"message"` keys. ChatGPT exports use a unique nested structure with a `mapping` object containing message nodes.

**Structure:** Array of conversations, each with a `mapping` object. Each mapping node has a `message` with `author.role`, `content.parts[]`, and `create_time` (Unix timestamp as float).

**Role mapping:** `"user"` → Human, `"assistant"` → Assistant, everything else → System.

#### Claude Parser (`claude.rs`)

**Detection:** File contains `"chat_messages"` or `"uuid"` keys.

**Structure:** Array of conversations with `uuid` (session ID) and `chat_messages[]`. Each message has `sender` ("human"/"assistant"), `text`, and `created_at` (RFC 3339).

#### Character.AI Parser (`character_ai.rs`)

**Detection:** File contains `"histories"` or `"character"` keys.

**Structure:** Object with `histories[]`, each containing `external_id` (session ID) and `msgs[]`. Each message has `src.is_human` (boolean), `text`, and `created` (RFC 3339 or epoch millis).

**Timestamp flexibility:** Character.AI exports use inconsistent timestamp formats. The parser tries RFC 3339 first, then epoch milliseconds, then falls back to Unix epoch. It never uses `Utc::now()` — export data is historical, and using the current time would corrupt timeline reconstruction.

#### Google Takeout / Gemini Parser (`google_takeout.rs`)

**Detection:** File contains `"Takeout"`, `"Bard"`, or `"Gemini"` keys.

**Structure:** Array of conversations with `id` and `turns[]`. Each turn has `role` ("USER"/"MODEL"), `text`, and `timestamp` (RFC 3339) or `createTime`.

**Role mapping:** Both uppercase (`"USER"`, `"MODEL"`) and lowercase variants are handled — Google has changed their export format across versions.

#### Generic JSONL Parser (`jsonl.rs`)

**Detection:** File extension is `.jsonl` or `.ndjson`.

**Structure:** One JSON object per line. Looks for `role`/`sender` (role), `content`/`text`/`message` (content), `timestamp` (RFC 3339), and `session_id`/`conversation_id`.

**Resilient parsing:** Invalid lines are silently skipped. This handles JSONL files with occasional malformed entries (common in real-world exports).

---

### `timeline.rs` — Session Boundary Reconstruction

Not all exports include session IDs. The timeline reconstructor infers boundaries from timestamp gaps.

```rust
pub struct TimelineReconstructor {
    pub session_gap_threshold: i64,  // Default: 3600 seconds (1 hour)
}
```

#### Algorithm

1. **Group by explicit session_id** — Messages with a `session_id` are grouped directly.
2. **Sort unassigned messages by timestamp** — Messages without a session_id are sorted chronologically.
3. **Split on gaps** — If the gap between consecutive messages exceeds the threshold (1 hour), a new session starts.
4. **Assign synthetic IDs** — Inferred sessions get IDs like `"inferred-0"`, `"inferred-1"`, etc.

**Why 1 hour?** Empirical observation. Most AI conversations have sub-minute gaps between messages. A 1-hour gap almost certainly indicates the user left and came back. This threshold works well for ChatGPT, Claude, and Character.AI usage patterns.

**Why not use a clustering algorithm?** Overkill. The timestamp gap heuristic is simple, deterministic, and produces correct results for the vast majority of conversation exports. A clustering algorithm would add complexity and non-determinism for marginal improvement.

---

### `analyzer.rs` — Pipeline Orchestration and Scoring

The analyzer ties everything together: detect format → parse → reconstruct timeline → score sessions → produce analysis result.

#### Per-Session Scoring

```rust
fn score_session(&self, session: &ReconstructedSession) -> SessionScore {
    let duration_signal = (duration as f64 / 21600.0).min(1.0);  // 6h max
    let msg_density = (messages / (duration_minutes)).min(1.0);
    let estimated_score = ((duration_signal * 0.5) + (msg_density * 0.5)).clamp(0.0, 1.0);
}
```

**Two signals, equal weight:**

1. **Duration signal** — Longer sessions (up to 6 hours) score higher. Very long sessions may indicate deep engagement or problematic attachment.
2. **Message density** — More messages per minute indicates higher engagement intensity.

**Why 6 hours as the max?** Sessions longer than 6 hours are rare and don't provide additional signal. Capping at 6 hours prevents a single marathon session from dominating the score.

#### Recommended Convergence Level

| Max Session Score | Recommended Level |
|-------------------|-------------------|
| > 0.85 | 4 (Critical) |
| > 0.70 | 3 (Escalated) |
| > 0.50 | 2 (Elevated) |
| > 0.30 | 1 (Active) |
| ≤ 0.30 | 0 (Stable) |

**Flagged sessions:** Any session with `estimated_score > 0.5` is flagged for human review. These sessions had unusually high engagement that may warrant closer monitoring.

---

## Security Properties

### No Network Access

All parsing is local. Export files are read from disk — no API calls to ChatGPT, Claude, or any external service. The user's conversation history never leaves their machine.

### Graceful Error Handling

Malformed exports produce clear errors, not panics. Invalid JSON → `ParseError`. Unrecognized format → `UnsupportedFormat`. Empty files → empty result (not an error). This is important because real-world exports are often messy.

### Historical Timestamps Only

Parsers never use `Utc::now()` for message timestamps. If a timestamp is missing, they fall back to `DateTime::UNIX_EPOCH`. Using the current time would corrupt timeline reconstruction and produce incorrect convergence baselines.

---

## Downstream Consumer Map

```
ghost-export (Layer 6)
├── ghost-gateway (Layer 8)
│   └── /api/import/analyze endpoint accepts export files
│   └── Returns ExportAnalysisResult for dashboard display
│   └── User reviews flagged sessions before baseline adoption
└── ghost-migrate (Layer 6)
    └── Uses analysis results to set initial convergence baselines
```

---

## Test Strategy

### Integration Tests (`tests/export_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `chatgpt_parser_detects_valid_export` | Detection heuristic matches ChatGPT format |
| `chatgpt_parser_parses_messages` | Correct message count, roles (Human/Assistant) |
| `character_ai_parser_detects_and_parses` | Character.AI detection and parsing |
| `each_parser_returns_valid_itp_events` | All messages have non-empty content and valid roles |
| `timeline_reconstructor_infers_session_boundaries` | 1-hour gap splits into 2 sessions |
| `analysis_result_serializes_to_json` | Result struct round-trips through serde |
| `full_pipeline_chatgpt` | End-to-end: file → analyzer → result with correct format name |
| `malformed_export_graceful_error` | Invalid JSON → error (not panic) |
| `empty_export_empty_result` | Empty JSONL file → empty message list |

---

## File Map

```
crates/ghost-export/
├── Cargo.toml                          # Deps: serde_json, chrono, uuid
├── src/
│   ├── lib.rs                          # NormalizedMessage, MessageRole, ExportError
│   ├── analyzer.rs                     # ExportAnalyzer, session scoring, level recommendation
│   ├── timeline.rs                     # TimelineReconstructor, gap-based session inference
│   └── parsers/
│       ├── mod.rs                      # ExportParser trait, all_parsers() registry
│       ├── chatgpt.rs                  # ChatGPT JSON export (mapping/message structure)
│       ├── claude.rs                   # Claude.ai export (chat_messages structure)
│       ├── character_ai.rs             # Character.AI export (histories/msgs structure)
│       ├── google_takeout.rs           # Google Takeout/Gemini export (turns structure)
│       └── jsonl.rs                    # Generic JSONL/NDJSON fallback parser
└── tests/
    └── export_tests.rs                 # Parser, timeline, pipeline, error handling tests
```

---

## Common Questions

### How do I add support for a new platform?

Implement the `ExportParser` trait (3 methods: `detect`, `parse`, `name`), add it to `all_parsers()` in `parsers/mod.rs`. Place it before the JSONL parser (which is the catch-all fallback). The analyzer will automatically use it.

### Why doesn't the analyzer store parsed messages in the database?

Separation of concerns. The analyzer produces an `ExportAnalysisResult` — a summary with scores and recommendations. The gateway decides whether to import the raw messages into `cortex-storage`. This keeps the export crate stateless and testable without database dependencies.

### What if a user has exports from multiple platforms?

Run the analyzer on each file separately. Each produces its own `ExportAnalysisResult`. The gateway can merge results when computing the initial convergence baseline.
