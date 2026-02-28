---
description: Build the Drift V2 MCP server — the complete presentation layer hardening from NAPI contracts through integration testing
---

# MCP Phase Workflow

> **Scope:** Presentation Layer Hardening (Phases A–E) as defined in `PRESENTATION-LAYER-HARDENING-TASKS.md`.
> This is the post-scaffold work — Phase 8 scaffold (`P8-MCP-01` through `P8-MCP-12`) is already complete.
> The existing `packages/drift-mcp/` has working server, tools, types, transport, and tests, but with
> 10+ NAPI signature mismatches, missing infrastructure modules, missing tools, and zero parity tests.
>
> **Source Documents (read these first if context is needed):**
> - `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md` — The task tracker (83 impl + 180 test tasks)
> - `docs/v2-research/systems/32-MCP-SERVER-V2-PREP.md` — The full MCP spec (3,302 lines)
> - `docs/v2-research/DRIFT-V2-IMPLEMENTATION-TASKS.md` — Phase 8 baseline (already complete)
> - `docs/v2-research/DRIFT-V2-IMPLEMENTATION-ORCHESTRATION.md` — Build order rationale
>
> **Existing Code (what's already built):**
> - `packages/drift-mcp/src/server.ts` — `createDriftMcpServer()` with `McpServer` from SDK
> - `packages/drift-mcp/src/tools/index.ts` — 4 entry points: `drift_status`, `drift_context`, `drift_scan`, `drift_tool`
> - `packages/drift-mcp/src/tools/drift_tool.ts` — 24 internal tools in `buildToolCatalog()` with NAPI mismatches
> - `packages/drift-mcp/src/napi.ts` — Local `DriftNapi` interface + stub (divergent from Rust)
> - `packages/drift-mcp/src/types.ts` — Local types (to be replaced by contracts)
> - `packages/drift-mcp/src/transport/` — stdio + HTTP transport wrappers
> - `packages/drift-mcp/tests/mcp_server.test.ts` — 10 tests (T8-MCP-01 through T8-MCP-10)
>
> **Rust NAPI Bindings (the ground truth):**
> - `crates/drift/drift-napi/src/bindings/` — 9 modules: lifecycle, scanner, analysis, patterns, graph, structural, enforcement, feedback, advanced
> - 38 total `#[napi]` functions — these are the canonical signatures TypeScript must match
>
> **Rule:** No Phase N+1 begins until Phase N quality gate passes.
> **Architectural Decision:** Refactor, not wipe. Existing transport, server, CLI bones are sound.

---

## Pre-Flight Checklist

Before starting any phase, verify the environment:

1. **Check Rust NAPI compiles:**
```bash
cd crates/drift && cargo build -p drift-napi --release 2>&1 | tail -5
```

2. **Check existing MCP tests pass:**
```bash
cd packages/drift-mcp && npx vitest run 2>&1 | tail -20
```

3. **Read the hardening task tracker** to see current checkbox state:
```
cat docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md | head -40
```

4. **Read Rust NAPI binding signatures** (ground truth for all TypeScript interfaces):
```
# These 9 files define the canonical #[napi] function signatures:
crates/drift/drift-napi/src/bindings/lifecycle.rs    — driftInitialize, driftShutdown, driftIsInitialized
crates/drift/drift-napi/src/bindings/scanner.rs      — driftScan, driftScanWithProgress, driftCancelScan
crates/drift/drift-napi/src/bindings/analysis.rs     — drift_analyze, drift_call_graph, drift_boundaries
crates/drift/drift-napi/src/bindings/patterns.rs     — drift_patterns, drift_confidence, drift_outliers, drift_conventions
crates/drift/drift-napi/src/bindings/graph.rs        — drift_reachability, drift_taint_analysis, drift_error_handling, drift_impact_analysis, drift_test_topology
crates/drift/drift-napi/src/bindings/structural.rs   — drift_coupling_analysis, drift_constraint_verification, drift_contract_tracking, drift_constants_analysis, drift_wrapper_detection, drift_dna_analysis, drift_owasp_analysis, drift_crypto_analysis, drift_decomposition
crates/drift/drift-napi/src/bindings/enforcement.rs  — drift_check, drift_audit, drift_violations, drift_gates
crates/drift/drift-napi/src/bindings/feedback.rs     — drift_dismiss_violation, drift_fix_violation, drift_suppress_violation
crates/drift/drift-napi/src/bindings/advanced.rs     — drift_simulate(category,description,context_json), drift_decisions(repo_path), drift_context(intent,depth,data_json), drift_generate_spec(module_json,migration_path_json)
```

---

## Phase A: NAPI Contracts Package (Foundation)

> **Goal:** Single source of truth for all Rust↔TypeScript NAPI function signatures and types.
> **Why:** Audit found 10+ signature mismatches. Three packages (`drift-mcp`, `drift-cli`, `drift-ci`) each maintain divergent `napi.ts` files.
> **Estimated effort:** 2–3 days
> **Task IDs:** `PH-NAPI-01` through `PH-NAPI-17`, Tests `TH-NAPI-01` through `TH-NAPI-25`
> **Tracker:** `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md` §Phase A

### Step A1 — Package Scaffold

Create `packages/drift-napi-contracts/` with:

- `package.json` — name `@drift/napi-contracts`, `"type": "module"`, zero runtime deps
- `tsconfig.json` — strict mode, `declaration: true`, `declarationMap: true`, target ES2022
- `vitest.config.ts` — coverage thresholds: lines ≥90%, branches ≥85%, functions ≥90%

Task IDs: `PH-NAPI-01`, `PH-NAPI-02`, `PH-NAPI-03`

### Step A2 — Canonical DriftNapi Interface

Create `packages/drift-napi-contracts/src/interface.ts` with the `DriftNapi` interface containing **exactly 38 functions** aligned to the Rust `#[napi]` exports. Group by binding module:

**Critical:** Read every `.rs` file in `crates/drift/drift-napi/src/bindings/` and extract the exact `#[napi]` function names, parameter types, and return types. The TypeScript interface MUST match Rust exactly.

The 38 functions by category:
- **Lifecycle (3):** `driftInitialize(dbPath?, projectRoot?, configToml?)`, `driftShutdown()`, `driftIsInitialized(): boolean`
- **Scanner (3):** `driftScan(path, options?)`, `driftScanWithProgress(path, callback)`, `driftCancelScan()`
- **Analysis (3):** `drift_analyze(path)`, `drift_call_graph(path)`, `drift_boundaries(path)`
- **Patterns (4):** `drift_patterns(path)`, `drift_confidence(patternId)`, `drift_outliers(path, cursor?)`, `drift_conventions(path)`
- **Graph (5):** `drift_reachability(functionKey, direction)`, `drift_taint_analysis(root)`, `drift_error_handling(root)`, `drift_impact_analysis(root)`, `drift_test_topology(root)`
- **Structural (9):** `drift_coupling_analysis(root)`, `drift_constraint_verification(root)`, `drift_contract_tracking(root)`, `drift_constants_analysis(root)`, `drift_wrapper_detection(root)`, `drift_dna_analysis(root)`, `drift_owasp_analysis(root)`, `drift_crypto_analysis(root)`, `drift_decomposition(root)`
- **Enforcement (4):** `drift_check(path, policy?)`, `drift_audit(path)`, `drift_violations(path)`, `drift_gates(path)`
- **Feedback (3):** `drift_dismiss_violation(input: JsFeedbackInput)`, `drift_fix_violation(violationId)`, `drift_suppress_violation(violationId, reason)`
- **Advanced (4):** `drift_simulate(category, description, contextJson)`, `drift_decisions(repoPath)`, `drift_context(intent, depth, dataJson)`, `drift_generate_spec(moduleJson, migrationPathJson?)`

Task IDs: `PH-NAPI-04`, `PH-NAPI-05`

### Step A3 — Type Definitions

Create type files in `packages/drift-napi-contracts/src/types/`:

- `index.ts` — Barrel re-exports
- `lifecycle.ts` — `InitOptions`, `ProgressCallback`, `ProgressUpdate`
- `scanner.ts` — `ScanOptions`, `ScanSummary`, `ScanDiff`
- `analysis.ts` — `JsAnalysisResult`, `JsPatternMatch`, `JsCallGraphResult`, `JsBoundaryResult`
- `patterns.ts` — `PatternsResult`, `ConfidenceResult`, `OutlierResult`, `ConventionResult`
- `graph.ts` — `JsReachabilityResult`, `JsTaintResult`, `JsErrorHandlingResult`, `JsImpactResult`, `JsTestTopologyResult`
- `structural.ts` — All 9 structural result types including `JsOwaspResult`, `JsCryptoResult`, `JsDecompositionResult`
- `enforcement.ts` — `JsViolation`, `JsCheckResult`, `JsAuditResult`, `JsGateResult`, `JsFeedbackResult`
- `advanced.ts` — `SimulationResult`, `DecisionResult`, `ContextResult`, `SpecResult`

**Critical:** Cross-reference return types against `crates/drift/drift-napi/src/conversions/types.rs` for the exact JS-facing type shapes.

Task IDs: `PH-NAPI-06` through `PH-NAPI-14`

### Step A4 — Loader, Stub, and Validators

- `src/loader.ts` — `loadNapi()` lazy singleton with `require('drift-napi')`, stub fallback. `setNapi()` for test injection, `resetNapi()` for cleanup. **Must validate** loaded module has all 38 function names — throws `NapiLoadError` with missing list if incomplete.
- `src/stub.ts` — `createStubNapi(): DriftNapi` — complete stub for every function. Returns structurally valid typed empty data (not `{}`). Async stubs return resolved Promises.
- `src/validation.ts` — Runtime param validators per NAPI function. Returns `{ valid: true }` or `{ valid: false, error, field }`. Runs BEFORE NAPI call to prevent Rust panics from bad JS input.

Task IDs: `PH-NAPI-15`, `PH-NAPI-16`, `PH-NAPI-17`

### Step A5 — Tests

Write tests in `packages/drift-napi-contracts/tests/`:

**Interface Alignment (5):** Every method has stub entry, methods match Rust names, exactly 38 functions, no `any`, no `Record<string, unknown>` returns.

**Stub Completeness (6):** Every method implemented, valid typed shape returned, async stubs resolve, specific stubs checked (`driftIsInitialized()` → false, `drift_violations()` → `[]`).

**Loader (6):** Stub fallback works, idempotent (10 calls same instance), `setNapi()` overrides, `resetNapi()` clears, incomplete object throws `NapiLoadError`, concurrent `Promise.all(5)` safe.

**Validation (8):** Scan params, context params, simulate params — valid and invalid cases, SQL injection passthrough.

Test IDs: `TH-NAPI-01` through `TH-NAPI-25`

### QG-A Verification

Run this to verify Phase A is complete:
```bash
cd packages/drift-napi-contracts && npx vitest run --coverage 2>&1 | tail -30
```

**All must pass:**
- [ ] Zero TypeScript errors
- [ ] `DriftNapi` has exactly 38 methods, zero `any`, zero `Record<string, unknown>` returns
- [ ] Every method has a stub returning valid typed shape
- [ ] `loadNapi()` gracefully returns stub when native binary unavailable
- [ ] `setNapi()`/`resetNapi()` lifecycle works
- [ ] All validators catch missing required fields, invalid enums, empty strings
- [ ] ≥90% line coverage

**After QG-A passes**, check off all Phase A tasks in `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md`.

---

## Phase B: MCP Infrastructure Layer

> **Goal:** Add 7 infrastructure modules v1 had but v2 lacks.
> **Prerequisite:** Phase A complete (QG-A passed)
> **Estimated effort:** 3–4 days
> **Task IDs:** `PH-INFRA-01` through `PH-INFRA-14`, Tests `TH-CACHE-01` through `TH-FILTER-04`
> **Tracker:** `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md` §Phase B

### Step B1 — Infrastructure Modules

Create `packages/drift-mcp/src/infrastructure/` with 8 files:

1. **`index.ts`** — Barrel exports + `InfrastructureLayer` class initializing all modules, providing unified `ctx` for tool handlers. (`PH-INFRA-01`)

2. **`cache.ts`** — `ResponseCache`: L1 in-memory LRU (`Map`, max 100, 5min TTL). Key: `${projectRoot}:${toolName}:${paramsHash}`. Project-isolated. Methods: `get/set/invalidate(glob)/invalidateProject`. Stores `{ data, createdAt, ttlMs, tokenEstimate }`. (`PH-INFRA-02`)

3. **`rate_limiter.ts`** — `RateLimiter`: sliding window. Global 100/60s, expensive 10/60s (scan, simulate, taint, impact). Returns `{ allowed }` or `{ allowed: false, retryAfterMs, reason }`. (`PH-INFRA-03`)

4. **`token_estimator.ts`** — `TokenEstimator`: heuristic (chars/3.5 English, chars/2.5 code). `estimateTokens(text)`, `estimateResponseTokens(toolName, params)`, `wouldExceedBudget(toolName, params, budget)`. (`PH-INFRA-04`)

5. **`error_handler.ts`** — `ErrorHandler`: wraps tool execution. Maps NAPI error codes → MCP errors with `recoveryHints[]`, `alternativeTools[]`, `retryable`, `retryAfterMs`. Key mappings: `[SCAN_ERROR]` → "Run drift setup first", `[DB_BUSY]` → retryable+1000ms. (`PH-INFRA-05`)

6. **`cursor_manager.ts`** — `CursorManager`: opaque keyset cursors. `encodeCursor({sortColumn, lastValue, lastId, version})` → base64url+HMAC. `decodeCursor()` → returns null for invalid/tampered/expired (1h TTL). (`PH-INFRA-06`)

7. **`response_builder.ts`** — `ResponseBuilder`: summary-first formatting. If over token budget: truncate arrays, prepend `_summary`, add `_truncated: true` + `_totalCount`. Always includes `_tokenEstimate`. (`PH-INFRA-07`)

8. **`tool_filter.ts`** — `ToolFilter`: filters catalog by project languages. Never filters core tools (status, context, scan, check, violations). Falls back to full catalog if detection fails. (`PH-INFRA-08`)

**Reference specs:** `32-MCP-SERVER-V2-PREP.md` §6.1–§6.10 for detailed interfaces and algorithms.

### Step B2 — Server Integration

- **Refactor `server.ts`** — Initialize `InfrastructureLayer`, pass `ctx` to tool registration. Wrap every handler in `errorHandler.wrap()`. (`PH-INFRA-09`)
- **Refactor `tools/index.ts`** — Accept `InfrastructureLayer`. Apply `toolFilter` to catalog. Add rate limiter check before `drift_tool` dispatch. (`PH-INFRA-10`)

### Step B3 — Import Alignment

- **Refactor `types.ts`** — Remove local `DriftNapi` interface (use contracts). Add infrastructure types: `InfrastructureConfig`, `McpResponse`, `RecoveryHint`, `CursorData`. (`PH-INFRA-11`)
- **Refactor `index.ts`** — Update imports from `@drift/napi-contracts`. (`PH-INFRA-12`)
- **Refactor `napi.ts`** — Replace entirely with re-exports: `export { loadNapi, setNapi, resetNapi } from '@drift/napi-contracts';` (`PH-INFRA-13`)
- **Refactor `package.json`** — Add `@drift/napi-contracts` workspace dependency. (`PH-INFRA-14`)

### Step B4 — Tests

**Cache (8):** round-trip, TTL expiry, LRU eviction at 100, project isolation, glob invalidation, project invalidation, undefined no-op, tokenEstimate stored. (`TH-CACHE-01`–`TH-CACHE-08`)

**Rate Limiter (6):** 100 calls allowed, 101st blocked, expensive 10-call limit, window sliding, retryAfterMs ≤ 60000, non-expensive not subject to expensive limit. (`TH-RATE-01`–`TH-RATE-06`)

**Token Estimator (5):** 'hello world' → 2-4, 10KB code within 20%, empty → 0, wouldExceedBudget correct, per-tool averages. (`TH-TOKEN-01`–`TH-TOKEN-05`)

**Error Handler (7):** SCAN_ERROR mapping, DB_BUSY retryable, UNSUPPORTED_LANGUAGE, CANCELLED, unknown error, non-Error thrown, stack trace preserved. (`TH-ERR-01`–`TH-ERR-07`)

**Cursor Manager (6):** encode→decode round-trip, tampered → null, expired → null, wrong version → null, empty → null, invalid base64 → null. (`TH-CURSOR-01`–`TH-CURSOR-06`)

**Response Builder (5):** under-budget passthrough, truncation with _totalCount, _summary present, _tokenEstimate present, 0 items valid. (`TH-RESP-01`–`TH-RESP-05`)

**Tool Filter (4):** Python filters TS tools, core never filtered, empty → full catalog, multi-language union. (`TH-FILTER-01`–`TH-FILTER-04`)

### QG-B Verification

```bash
cd packages/drift-mcp && npx vitest run --coverage 2>&1 | tail -30
```

**All must pass:**
- [ ] All 7 infrastructure modules compile and export from barrel
- [ ] `server.ts` initializes `InfrastructureLayer` and passes `ctx` to handlers
- [ ] Cache LRU works at 100 entries with project isolation
- [ ] Rate limiter enforces 100/60s global and 10/60s expensive limits
- [ ] Error handler maps all 14 NAPI error codes to structured recovery hints
- [ ] Cursor manager detects tampered/expired/wrong-version cursors
- [ ] Response builder enforces token budget with summary-first truncation
- [ ] Tool filter protects core tools
- [ ] ≥80% coverage for infrastructure/
- [ ] Local `napi.ts` replaced with re-export from `@drift/napi-contracts`

---

## Phase C: MCP Tool Hardening

> **Goal:** Fix all NAPI mismatches, add 11 missing tools, create 2 new entry points.
> **Prerequisite:** Phases A + B complete
> **Estimated effort:** 3–4 days
> **Task IDs:** `PH-TOOL-01` through `PH-TOOL-28`, Tests `TH-TOOL-01` through `TH-TOOL-28` + `TH-DISC-01`–`TH-DISC-10` + `TH-WORK-01`–`TH-WORK-09`
> **Tracker:** `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md` §Phase C

### Step C1 — Fix 11 NAPI Mismatches in `drift_tool.ts`

Each fix corrects a TS→Rust function name or argument mismatch. The "Before" state silently returns empty stub data. Read the current `packages/drift-mcp/src/tools/drift_tool.ts` and fix each handler:

| Tool | Before (Wrong) | After (Correct) | Task ID |
|------|----------------|------------------|---------|
| `drift_callers` | `napi.drift_call_graph(path)` | `napi.drift_call_graph(params.path ?? projectRoot)` | `PH-TOOL-01` |
| `drift_reachability` | `napi.drift_reachability(functionId)` | `napi.drift_reachability(functionId, direction ?? 'forward')` — 2 args | `PH-TOOL-02` |
| `drift_taint` | `napi.drift_taint(functionId)` | `napi.drift_taint_analysis(root)` — wrong name + arg | `PH-TOOL-03` |
| `drift_impact_analysis` | `napi.drift_impact(functionId)` | `napi.drift_impact_analysis(root)` — wrong name | `PH-TOOL-04` |
| `drift_error_handling` | `napi.drift_analyze(path)` | `napi.drift_error_handling(root)` — was generic | `PH-TOOL-05` |
| `drift_coupling` | `napi.drift_analyze(path)` | `napi.drift_coupling_analysis(root)` — was generic | `PH-TOOL-06` |
| `drift_constants` | `napi.drift_analyze(path)` | `napi.drift_constants_analysis(root)` — was generic | `PH-TOOL-07` |
| `drift_constraints` | `napi.drift_check(path)` | `napi.drift_constraint_verification(root)` — wrong fn | `PH-TOOL-08` |
| `drift_dna_profile` | `napi.drift_analyze(path)` | `napi.drift_dna_analysis(root)` — was generic | `PH-TOOL-09` |
| `drift_simulate` | `napi.drift_simulate(task)` | `napi.drift_simulate(category, description, context_json)` — 3 args | `PH-TOOL-10` |
| `drift_explain` | `napi.drift_context(query, 'deep')` | `napi.drift_context(intent, depth, data_json)` — 3 args | `PH-TOOL-11` |

### Step C2 — Fix Entry Point Mismatches

- **`drift_context.ts`** — `napi.drift_context(intent, depth)` → `napi.drift_context(intent, depth, JSON.stringify({ focus }))` — Rust takes 3 args. Remove client-side focus filtering. (`PH-TOOL-12`)
- **`drift_scan.ts`** — Add MCP Tasks support for long-running scans. Wrap with `driftScanWithProgress()` when available. Add cancellation via `driftCancelScan()`. (`PH-TOOL-13`)

### Step C3 — Add 11 Missing Tools

These Rust NAPI functions exist but have zero MCP catalog entries. Add each to `buildToolCatalog()`:

| Tool | NAPI Function | Category | Tokens | Task ID |
|------|---------------|----------|--------|---------|
| `drift_outliers` | `drift_outliers(path, cursor)` | exploration | 400 | `PH-TOOL-14` |
| `drift_conventions` | `drift_conventions(path)` | exploration | 500 | `PH-TOOL-15` |
| `drift_owasp` | `drift_owasp_analysis(root)` | analysis | 600 | `PH-TOOL-16` |
| `drift_crypto` | `drift_crypto_analysis(root)` | analysis | 400 | `PH-TOOL-17` |
| `drift_decomposition` | `drift_decomposition(root)` | analysis | 800 | `PH-TOOL-18` |
| `drift_contracts` | `drift_contract_tracking(root)` | exploration | 600 | `PH-TOOL-19` |
| `drift_dismiss` | `drift_dismiss_violation(input)` | feedback | 50 | `PH-TOOL-20` |
| `drift_fix` | `drift_fix_violation(violationId)` | feedback | 50 | `PH-TOOL-21` |
| `drift_suppress` | `drift_suppress_violation(id, reason)` | feedback | 50 | `PH-TOOL-22` |
| `drift_scan_progress` | `driftScanWithProgress(path, cb)` | operational | 100 | `PH-TOOL-23` |
| `drift_cancel_scan` | `driftCancelScan()` | operational | 30 | `PH-TOOL-24` |

### Step C4 — New Entry Points

1. **Create `drift_discover.ts`** — Intent-guided tool recommendation. Schema: `{ intent, focus?, maxTools? }`. Scores catalog tools by keyword match to description + category, applies `ToolFilter`, returns top N ranked by relevance. 5th MCP entry point. (`PH-TOOL-25`)

2. **Create `drift_workflow.ts`** — Composite workflow dispatch. Schema: `{ workflow, path?, options? }`. 5 workflows: (`PH-TOOL-26`)
   - `pre_commit` → check → violations → impact
   - `security_audit` → owasp → crypto → taint → error_handling
   - `code_review` → status → context → violations → patterns
   - `health_check` → status → audit → test_topology → dna
   - `onboard` → status → conventions → patterns → contracts

3. **Refactor `tools/index.ts`** — Register `drift_discover` and `drift_workflow`. Total entry points: 4 → 6. (`PH-TOOL-27`)

4. **Update `drift_tool.ts`** — Catalog: ~41 internal tools. Add `feedback` and `operational` categories. Validate all handlers use `@drift/napi-contracts` loader. (`PH-TOOL-28`)

### Step C5 — Tests

**NAPI Mismatch Fixes (11):** Mock NAPI, verify each tool calls the correct function with correct arg count. Plus: iterate ALL 41 tools — zero `undefined` function calls. (`TH-TOOL-01`–`TH-TOOL-11`)

**New Tools Happy Path (9):** Each new tool returns the correct typed shape. (`TH-TOOL-12`–`TH-TOOL-20`)

**New Tools Error Handling (5):** Invalid violationId, already-fixed, duration=0, empty database, no security issues. (`TH-TOOL-21`–`TH-TOOL-25`)

**Discover (10):** Intent matching for security audit, fix bug, understand code, pre-commit; maxTools works; unknown intent doesn't crash; results sorted by relevance. (`TH-DISC-01`–`TH-DISC-10`)

**Workflow (9):** Each of 5 workflows calls correct sub-tools; unknown workflow errors; partial failure → partial results; _workflow metadata present; rate limiter respected. (`TH-WORK-01`–`TH-WORK-09`)

**Entry Point Registration (3):** 6 entry points, 41 internal tools, < 1.5K tokens. (`TH-TOOL-26`–`TH-TOOL-28`)

### QG-C Verification

```bash
cd packages/drift-mcp && npx vitest run --coverage 2>&1 | tail -30
```

**All must pass:**
- [ ] All 11 NAPI mismatches fixed — every tool calls correct Rust function with correct arg count
- [ ] 11 new tools in catalog
- [ ] `drift_discover` returns relevant tools for 5 intents
- [ ] `drift_workflow` executes 5 workflows with correct sub-tool calls
- [ ] 6 MCP entry points registered
- [ ] 41 internal tools in catalog — zero `undefined` function calls
- [ ] Feedback tools return `JsFeedbackResult` with confidence adjustments
- [ ] Workflow partial failure → partial results, not total failure
- [ ] ≥80% coverage for tools/

---

## Phase D: CLI + CI Alignment

> **Goal:** Eliminate duplicated `napi.ts` in drift-cli and drift-ci. Align to correct NAPI signatures.
> **Prerequisite:** Phase A complete. Phases B/C NOT required (CLI is independent of MCP infra).
> **Can run in PARALLEL with Phases B/C after Phase A.**
> **Estimated effort:** 2–3 days
> **Task IDs:** `PH-CLI-01` through `PH-CI-03`, Tests `TH-CLI-01` through `TH-CI-07`
> **Tracker:** `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md` §Phase D

### Step D1 — CLI NAPI Alignment (`packages/drift-cli/`)

1. Add `@drift/napi-contracts` workspace dep to `package.json` (`PH-CLI-01`)
2. Delete local `src/napi.ts` — replace with re-export from contracts (`PH-CLI-02`)
3. Fix `src/index.ts` — `napi.drift_init()` → `napi.driftInitialize()` (`PH-CLI-03`)
4. Fix each command handler to use correct NAPI function names and arg counts:
   - `commands/scan.ts` → `driftScan(path, options)` + progress via `driftScanWithProgress` (`PH-CLI-04`)
   - `commands/check.ts` → `drift_check(path, policy)` (`PH-CLI-05`)
   - `commands/patterns.ts` → `drift_patterns(path)` (`PH-CLI-06`)
   - `commands/violations.ts` → `drift_violations(path)` returning `JsViolation[]` (`PH-CLI-07`)
   - `commands/impact.ts` → `drift_impact_analysis(root)` not `drift_impact(functionId)` (`PH-CLI-08`)
   - `commands/simulate.ts` → `drift_simulate(category, description, context_json)` — 3 args (`PH-CLI-09`)
   - `commands/audit.ts` → `drift_audit(path)` (`PH-CLI-10`)
   - `commands/setup.ts` → `driftInitialize({ dbPath })` (`PH-CLI-11`)
   - `commands/doctor.ts` → `driftIsInitialized()` (`PH-CLI-12`)
   - `commands/explain.ts` → `drift_context(intent, depth, data_json)` — 3 args (`PH-CLI-13`)
   - `commands/fix.ts` → `drift_fix_violation(violationId)` (`PH-CLI-14`)
   - `commands/export.ts` → align to contracts types (`PH-CLI-15`)

### Step D2 — CI Agent Alignment (`packages/drift-ci/`)

1. Add `@drift/napi-contracts` workspace dep (`PH-CI-01`)
2. Delete local `src/napi.ts` — replace with re-export (`PH-CI-02`)
3. Fix `src/agent.ts` — Update 9 analysis passes to correct NAPI names (`PH-CI-03`)

### Step D3 — Tests

**CLI Happy Path (20):** scan, scan --incremental, scan /nonexistent, scan empty dir, check with/without violations, check --policy, status, status no db, violations with/without, setup, setup existing, doctor, doctor missing db, fix valid/no-id/invalid, export sarif, export json. (`TH-CLI-01`–`TH-CLI-20`)

**CLI Output (7):** Unicode tables, 0-row tables, JSON parseable, JSON round-trip, SARIF schema, SARIF empty, SARIF Unicode. (`TH-CLI-21`–`TH-CLI-27`)

**CLI Integration (4):** Exit codes, --quiet, --format json, unknown command. (`TH-CLI-28`–`TH-CLI-31`)

**CI Agent (7):** 9 correct NAPI names, empty diff, timeout, partial failure, PR comment valid, 0 violations, Unicode. (`TH-CI-01`–`TH-CI-07`)

### QG-D Verification

```bash
cd packages/drift-cli && npx vitest run --coverage 2>&1 | tail -20
cd packages/drift-ci && npx vitest run --coverage 2>&1 | tail -20
```

**All must pass:**
- [ ] `drift-cli/src/napi.ts` and `drift-ci/src/napi.ts` contain only re-exports
- [ ] `drift setup` calls `driftInitialize()` (correct Rust name)
- [ ] `drift doctor` calls `driftIsInitialized()` with meaningful status
- [ ] `drift fix <id>` calls `drift_fix_violation()` with confidence adjustment
- [ ] `drift simulate` passes 3 args
- [ ] CI agent calls 9 correct NAPI function names
- [ ] Exit codes: 0=clean, 1=violations, 2=error
- [ ] ≥80% coverage for drift-cli and drift-ci

---

## Phase E: Integration, Parity & Regression

> **Goal:** Verify MCP, CLI, CI produce consistent results. Measure token efficiency. Run adversarial tests.
> **Prerequisite:** Phases A–D ALL complete
> **Estimated effort:** 2–3 days
> **Task IDs:** `PH-PARITY-01` through `PH-PARITY-06`, Tests `TH-PARITY-01` through `TH-ADV-06`
> **Tracker:** `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md` §Phase E

### Step E1 — Cross-Interface Parity Tests

Create in `packages/drift-mcp/tests/integration/`:

1. **`mcp_cli_parity.test.ts`** — Scan shared fixture via both MCP and CLI, compare results. (`PH-PARITY-01`)
2. **`progressive_disclosure.test.ts`** — Measure token overhead: 6 entry points < 1.5K tokens, all 41 tools > 5K tokens, reduction ≥ 75%. (`PH-PARITY-02`)
3. **`concurrent_requests.test.ts`** — 5 simultaneous `drift_tool` calls → correct results, no mixing. (`PH-PARITY-03`)
4. **`graceful_shutdown.test.ts`** — In-flight request completes before exit. (`PH-PARITY-04`)

### Step E2 — End-to-End Regression

5. **`full_pipeline.test.ts`** — Full MCP: scan → analyze → enforce → report. All tools return typed results. (`PH-PARITY-05`)
6. **`github_action.test.ts`** in `packages/drift-ci/tests/integration/` — Full CI simulation: scan → 9 passes → SARIF → PR comment. (`PH-PARITY-06`)

### Step E3 — Tests

**MCP↔CLI Parity (6):** violations count, health score, pattern count, check verdict, audit score, divergence detection. (`TH-PARITY-01`–`TH-PARITY-06`)

**Progressive Disclosure Token Efficiency (4):** < 1.5K for 6 entry points, > 5K for 41 tools, ≥ 75% reduction, discover response < 500 tokens. (`TH-TOKEN-PD-01`–`TH-TOKEN-PD-04`)

**Concurrent Requests (4):** 5 status calls identical, mixed tool types correct, concurrent scan + status non-blocking, burst rate limiter. (`TH-CONC-01`–`TH-CONC-04`)

**Graceful Shutdown (3):** In-flight completes, no new after signal, NAPI shutdown called. (`TH-SHUT-01`–`TH-SHUT-03`)

**End-to-End Pipeline (5):** MCP pipeline, CLI pipeline, CI pipeline, security_audit workflow, pre_commit workflow. (`TH-E2E-01`–`TH-E2E-05`)

**Adversarial Input (6):** 1MB string → no OOM, null/undefined rejected, SQL injection safe, Unicode edge cases, all 41 tools with empty params, all 41 tools in stub mode. (`TH-ADV-01`–`TH-ADV-06`)

### QG-E Verification (Final Gate)

```bash
# Run all packages
cd packages/drift-napi-contracts && npx vitest run --coverage
cd packages/drift-mcp && npx vitest run --coverage
cd packages/drift-cli && npx vitest run --coverage
cd packages/drift-ci && npx vitest run --coverage
```

**All must pass:**
- [ ] MCP and CLI produce identical results for violations, patterns, status, check, audit
- [ ] Progressive disclosure reduces token overhead ≥ 75%
- [ ] 5 concurrent requests complete without mixing
- [ ] Graceful shutdown completes in-flight requests
- [ ] Full MCP pipeline returns valid typed results
- [ ] Full CLI pipeline exits with correct codes
- [ ] Full CI pipeline generates valid SARIF + PR comment
- [ ] All 41 tools handle empty params without crash
- [ ] All string params handle Unicode + adversarial input safely
- [ ] ≥80% coverage across all 4 TS packages combined

---

## Critical Path & Parallelization

```
Phase A (2-3d) ──→ Phase B (3-4d) ──→ Phase C (3-4d) ──→ Phase E (2-3d)
                ↘                                        ↗
                  Phase D (2-3d) ───────────────────────┘
```

**Phases B/C and D are parallelizable after A.** Phase E requires all of A–D.

Total: **10–14 working days** (single developer).

---

## Milestone Markers

| Milestone | Phase | What It Means |
|-----------|-------|---------------|
| M-A: "Contracts Locked" | End of A | Single NAPI source of truth, zero divergent interfaces |
| M-B: "Infrastructure Ready" | End of B | Cache, rate limiting, error handling, pagination operational |
| M-C: "Tools Complete" | End of C | 41 tools, 6 entry points, zero NAPI mismatches |
| M-D: "CLI Aligned" | End of D | All 3 packages use contracts, all commands wired |
| M-E: "Frontier Certified" | End of E | Parity verified, token-efficient, adversarial-safe |

---

## Risk Register

| Risk | Severity | Mitigation |
|------|----------|------------|
| R1: Rust NAPI signatures change during implementation | Medium | `interface_alignment.test.ts` catches at CI time |
| R2: 41-tool catalog token overhead exceeds budget | Low | Progressive disclosure + `drift_discover` mitigates |
| R3: Cache invalidation bugs cause stale data | Medium | Conservative 5min TTL + explicit invalidation on scan |
| R4: Rate limiter blocks legitimate burst usage | Low | Configurable limits + per-tool overrides |
| R5: Workflow partial failure confuses AI agents | Medium | Clear `_workflow` metadata with per-tool status |
| R6: MCP↔CLI parity drift after alignment | High | Parity tests in CI prevent regression |

---

## Quick Reference: File Locations

### Packages to Create
- `packages/drift-napi-contracts/` — New package (Phase A)

### Packages to Modify
- `packages/drift-mcp/src/server.ts` — Add infrastructure layer (Phase B)
- `packages/drift-mcp/src/tools/drift_tool.ts` — Fix 11 NAPI mismatches + add 17 tools (Phase C)
- `packages/drift-mcp/src/tools/index.ts` — Register 2 new entry points (Phase C)
- `packages/drift-mcp/src/napi.ts` — Replace with re-export (Phase B)
- `packages/drift-mcp/src/types.ts` — Replace local types (Phase B)
- `packages/drift-cli/src/napi.ts` — Replace with re-export (Phase D)
- `packages/drift-ci/src/napi.ts` — Replace with re-export (Phase D)

### Files to Create
- `packages/drift-mcp/src/infrastructure/*.ts` — 8 infra modules (Phase B)
- `packages/drift-mcp/src/tools/drift_discover.ts` — New entry point (Phase C)
- `packages/drift-mcp/src/tools/drift_workflow.ts` — New entry point (Phase C)
- `packages/drift-mcp/tests/integration/*.ts` — 4 integration test files (Phase E)

### Ground Truth (Read-Only Reference)
- `crates/drift/drift-napi/src/bindings/*.rs` — 9 Rust binding modules (38 functions)
- `crates/drift/drift-napi/src/conversions/types.rs` — JS-facing type shapes
- `docs/v2-research/systems/32-MCP-SERVER-V2-PREP.md` — Full MCP spec
- `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md` — Task checkbox tracker

---

## How to Check Off Tasks

After completing each task, update the checkbox in `docs/v2-research/PRESENTATION-LAYER-HARDENING-TASKS.md`:

```
- [ ] `PH-NAPI-01` — ...   →   - [x] `PH-NAPI-01` — ...
```

This keeps the tracker as the single source of truth for progress.
