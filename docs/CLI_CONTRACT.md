# GHOST CLI — Output Stability Contract

> Defines the rules governing CLI JSON/YAML output evolution.
> Consumers of `ghost ... --output json` can rely on these guarantees.
>
> Ref: CLI_TASKS.md T-X.1, CLI_DESIGN.md E.10

---

## 1. Scope

This contract applies to all structured output produced by the `ghost` CLI
when invoked with `--output json`, `--output jsonl`, or `--output yaml`.

Table output (`--output table`, the default) is human-readable and may change
formatting at any time without notice.

---

## 2. Versioning

CLI output carries an implicit format version, currently **v1**.

- The `--format-version` global flag allows consumers to pin a specific
  version (default: `latest`, which resolves to the highest supported version).
- When the format version is pinned, the CLI guarantees that the output shape
  matches the contract for that version.

---

## 3. Non-Breaking Changes (Always Allowed)

The following changes may be shipped in any release without a format version
bump:

| Change | Example |
|---|---|
| Add a new field to a response object | `{"id": "...", "new_field": 42}` |
| Add a new variant to an enum string field | `status: "archived"` |
| Widen a numeric range | `score: 0.0–1.0` → `score: 0.0–2.0` |
| Add a new subcommand's output | New `ghost foo --output json` |

**Consumer obligation**: Parsers MUST ignore unknown fields. Parsers MUST NOT
fail on unexpected enum values. Use `serde(deny_unknown_fields)` only in tests,
never in production consumers.

---

## 4. Breaking Changes (Require Format Version Bump)

The following changes are breaking and require incrementing the format version:

| Change | Why it breaks |
|---|---|
| Remove or rename an existing field | Consumers reading the field will fail |
| Change a field's type | `"score": 0.85` → `"score": "0.85"` |
| Change the semantics of an existing field | `level: 0` meant "normal", now means "critical" |
| Change array item shape | `[{"id": "..."}]` → `["..."]` |
| Reorder fields relied upon positionally in NDJSON | Consumers using positional parsing break |

---

## 5. Format Version Lifecycle

| Version | Status | Notes |
|---|---|---|
| `v1` | **Current** | Initial stable format |

When a new format version is introduced:

1. The previous version remains supported for **6 months** from the date
   the new version ships.
2. During overlap, `--format-version v1` continues to produce the old shape.
3. After the 6-month window, the old version is removed and
   `--format-version v1` returns an error with an upgrade message.

---

## 6. NDJSON / JsonLines

`--output jsonl` emits one JSON object per line (newline-delimited JSON).

- Each line is independently parseable as valid JSON.
- The field set for each line matches the corresponding `--output json` shape
  as if the outer array were unwrapped.
- Streaming commands (`ghost logs --json`, `ghost audit tail`) emit NDJSON
  by default since they produce unbounded output.

---

## 7. Exit Codes

Exit codes are part of the contract and follow `sysexits.h`:

| Code | Meaning | Constant |
|---|---|---|
| 0 | Success | `EX_OK` |
| 1 | General error | — |
| 64 | Usage error (bad arguments) | `EX_USAGE` |
| 69 | Service unavailable (gateway not running) | `EX_UNAVAILABLE` |
| 70 | Internal error | `EX_SOFTWARE` |
| 76 | Database error | `EX_PROTOCOL` |
| 77 | Authentication required | `EX_NOPERM` |
| 78 | Configuration error | `EX_CONFIG` |

Exit codes will not be reassigned. New codes may be added.
