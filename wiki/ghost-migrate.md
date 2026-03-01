# ghost-migrate

> OpenClaw to GHOST platform migration — non-destructive import of SOUL.md, memories, skills, and configuration with automatic quarantine of unsigned skills and conservative importance assignment.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 6 (Data Services) |
| Type | Library |
| Location | `crates/ghost-migrate/` |
| Workspace deps | None (standalone) |
| External deps | `serde`, `serde_json`, `serde_yaml`, `chrono`, `uuid`, `thiserror`, `tracing` |
| Modules | `migrator` (orchestrator), `importers/` (soul, memory, skill, config) |
| Public API | `OpenClawMigrator`, `MigrationResult` |
| Importers | SOUL.md, memories, skills (with quarantine), config (YAML mapping) |
| Guarantee | Non-destructive — source files are never modified |
| Test coverage | Detection, per-importer validation, quarantine, non-destructive guarantee, corruption handling |
| Downstream consumers | `ghost-gateway` (migration CLI command and API) |

---

## Why This Crate Exists

GHOST is the successor to OpenClaw. Users migrating from OpenClaw have existing agent configurations: a SOUL.md defining the agent's personality, accumulated memories, custom skills, and platform configuration. Throwing this away and starting fresh would be a terrible user experience.

`ghost-migrate` provides a one-command migration that:

1. **Detects** a valid OpenClaw installation (looks for `SOUL.md` in the source directory)
2. **Imports** four categories of data with appropriate transformations
3. **Never modifies** the source — if migration fails or the user wants to go back, the OpenClaw installation is untouched
4. **Reports** what was imported, what was skipped, what needs human review

---

## Module Breakdown

### `migrator.rs` — Migration Orchestrator

The orchestrator runs the four importers in sequence and collects results.

#### Detection

```rust
pub fn detect(path: &Path) -> bool {
    path.exists() && path.is_dir() && path.join("SOUL.md").exists()
}
```

**Why SOUL.md as the detection signal?** Every OpenClaw installation has a SOUL.md — it's the defining artifact. If it's missing, this isn't an OpenClaw directory. This is a simple, reliable heuristic that avoids false positives.

#### Migration Result

```rust
pub struct MigrationResult {
    pub imported: Vec<String>,      // Successfully imported items
    pub skipped: Vec<String>,       // Items that didn't need migration
    pub warnings: Vec<String>,      // Non-fatal issues
    pub review_items: Vec<String>,  // Items needing human review (quarantined skills)
}
```

**Four categories, not two.** A simple success/failure result would lose important nuance. The user needs to know:
- What was imported (so they can verify)
- What was skipped (so they know nothing was lost)
- What had warnings (so they can investigate)
- What needs review (quarantined skills that need signing)

#### Error Handling

Each importer runs independently. If the skill importer fails, the SOUL.md and memories are still imported. Failures are captured as warnings in the result, not as fatal errors. The only fatal error is "no OpenClaw installation found."

---

### `importers/soul.rs` — SOUL.md Import

The SOUL.md is the agent's personality definition. It's imported with one critical transformation: agent-mutable sections are stripped.

```rust
fn strip_agent_mutable(content: &str) -> String {
    // Remove content between <!-- AGENT-MUTABLE --> and <!-- /AGENT-MUTABLE --> markers
}
```

**Why strip agent-mutable sections?** In OpenClaw, agents could modify their own SOUL.md within marked sections. GHOST doesn't allow this — the soul document is human-authored and immutable. Importing agent-written content into an immutable document would freeze potentially problematic self-modifications. Stripping these sections gives the user a clean starting point.

---

### `importers/memory.rs` — Memory Import

Memories are imported with conservative importance levels.

```rust
let ghost_memory = format!(
    "---\nimportance: Low\nsource: openclaw_import\n---\n{}",
    content
);
```

**Why "Low" importance for all imported memories?** The GHOST convergence system uses importance levels to prioritize memory retrieval. Imported memories haven't been validated by the GHOST system — their relevance and accuracy are unknown. Starting at "Low" means they're available but won't dominate retrieval results. The convergence system will naturally promote important memories over time based on usage patterns.

**Source tagging:** Every imported memory is tagged with `source: openclaw_import`. This makes it easy to identify and bulk-manage imported memories later.

---

### `importers/skill.rs` — Skill Import with Quarantine

Skills are the most security-sensitive import. Unsigned skills are quarantined, not imported.

```rust
if has_signature {
    let cleaned = strip_incompatible_permissions(&content);
    // Write to skills/
} else {
    // Write to skills_quarantine/
}
```

**Signature detection:** The importer checks for both `"signature:"` and `"-----BEGIN"` in the content. This is a heuristic — it catches PEM-formatted signatures embedded in YAML frontmatter.

**Permission stripping:** Even signed skills have incompatible permissions stripped. OpenClaw permissions like `permission: root`, `permission: admin`, and `permission: system` don't map to GHOST's capability model. Stripping them prevents privilege escalation — the skill will need to declare GHOST-compatible capabilities before it can execute.

**Quarantine directory:** Unsigned skills are written to `skills_quarantine/` and added to `review_items` in the migration result. The user sees "quarantined (unsigned): my_skill.yml" and knows they need to sign it before GHOST will load it.

---

### `importers/config.rs` — Configuration Mapping

The config importer maps OpenClaw's YAML configuration to GHOST's `ghost.yml` format.

```rust
fn map_to_ghost_config(source: &serde_yaml::Value) -> serde_yaml::Value {
    // Map gateway settings (bind, port)
    // Map agent settings (name)
}
```

**Conservative mapping:** Only well-understood settings are mapped. The gateway gets default bind/port values. Agent names are preserved if present. Unknown settings are silently dropped — it's safer to use GHOST defaults than to guess at OpenClaw setting semantics.

**Multiple config file names:** The importer checks for `config.yml`, `config.yaml`, `openclaw.yml`, and `openclaw.yaml`. If none exist, it reports "No config file found, using defaults" — not an error.

---

## Security Properties

### Non-Destructive Guarantee

The migrator never writes to the source directory. All output goes to the target directory. This is tested explicitly: `source_files_never_modified` reads SOUL.md before and after migration and asserts byte-for-byte equality.

### Unsigned Skill Quarantine

Skills without valid signatures are quarantined — written to a separate directory and flagged for review. They cannot be loaded by the skill registry until signed. This prevents supply-chain attacks where a malicious skill is smuggled in via an OpenClaw migration.

### Permission Stripping

Incompatible permissions (`root`, `admin`, `system`) are stripped from imported skills. Even if a skill was trusted in OpenClaw, it starts with minimal privileges in GHOST and must explicitly declare capabilities.

### Conservative Importance

All imported memories start at "Low" importance. This prevents imported content from dominating the agent's behavior before the convergence system has had a chance to evaluate it.

---

## Downstream Consumer Map

```
ghost-migrate (Layer 6)
└── ghost-gateway (Layer 8)
    └── `ghost migrate` CLI command
    └── /api/migrate/detect — check for OpenClaw installation
    └── /api/migrate/run — execute migration
    └── /api/migrate/status — check migration result
```

---

## Test Strategy

### Integration Tests (`tests/migrate_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `detect_valid_openclaw_installation` | SOUL.md presence → detection succeeds |
| `detect_missing_installation` | Missing directory → detection fails |
| `soul_importer_produces_valid_ghost_soul` | Agent-mutable sections stripped, rest preserved |
| `memory_importer_assigns_conservative_importance` | All memories get `importance: Low` |
| `skill_importer_quarantines_unsigned` | Unsigned skills → quarantine dir + review_items |
| `config_importer_produces_valid_ghost_yml` | Output contains `gateway` section |
| `migration_result_contains_all_categories` | Result has non-empty imported list |
| `full_migration_from_mock` | End-to-end: all files created, result non-empty |
| `source_files_never_modified` | SOUL.md identical before and after migration |
| `corrupted_openclaw_graceful_error` | Invalid YAML config → warning (not crash) |

---

## File Map

```
crates/ghost-migrate/
├── Cargo.toml                          # Deps: serde_yaml, chrono, uuid
├── src/
│   ├── lib.rs                          # MigrateError, result types
│   ├── migrator.rs                     # OpenClawMigrator, detection, orchestration
│   └── importers/
│       ├── mod.rs                      # Importer module declarations
│       ├── soul.rs                     # SOUL.md import, agent-mutable stripping
│       ├── memory.rs                   # Memory import, conservative importance
│       ├── skill.rs                    # Skill import, quarantine, permission stripping
│       └── config.rs                   # Config mapping, YAML transformation
└── tests/
    └── migrate_tests.rs                # Detection, per-importer, non-destructive, corruption tests
```

---

## Common Questions

### What if the user has customized their OpenClaw installation heavily?

The migrator imports what it understands and skips what it doesn't. Unknown files in the OpenClaw directory are ignored. The migration result's `warnings` list tells the user what was skipped. They can manually migrate custom artifacts.

### Can I run the migration multiple times?

Yes. The migrator overwrites the target directory on each run. It's idempotent — running it twice produces the same result. The source is never modified, so there's no risk of data loss.

### Why not auto-sign imported skills?

Trust boundary. The GHOST signing key belongs to the user, not to the migration tool. Auto-signing would mean the migration tool has access to the signing key, which violates the principle of least privilege. The user reviews quarantined skills and signs them explicitly.
