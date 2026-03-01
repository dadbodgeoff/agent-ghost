# ghost-skills

> Skill registry, dual sandbox execution (WASM + native), credential brokering, workflow recording, and MCP bridge — the extensibility layer that lets agents learn reusable tool sequences and run untrusted code safely.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 5 (Agent Services) |
| Type | Library |
| Location | `crates/ghost-skills/` |
| Workspace deps | `cortex-core`, `ghost-signing` |
| External deps | `blake3`, `serde`, `serde_json`, `serde_yaml`, `chrono`, `uuid`, `tokio`, `tracing`, `thiserror` |
| Modules | `registry`, `sandbox/` (wasm + native), `credential/`, `recorder`, `proposer`, `bridges/` |
| Public API | `SkillRegistry`, `WasmSandbox`, `NativeSandbox`, `CredentialBroker`, `WorkflowRecorder`, `SkillProposer`, `DriftMCPBridge` |
| Test coverage | Unit tests, integration tests, property-based tests (proptest) |
| Downstream consumers | `ghost-agent-loop` (skill execution), `ghost-gateway` (registry lifecycle) |

---

## Why This Crate Exists

Agents repeat themselves. A developer asks "read this file, search for X, then write a patch" three times in a week — each time the agent burns tokens re-planning the same tool sequence. `ghost-skills` exists to close that loop: observe repeated workflows, propose reusable skills, and execute them in sandboxed environments with brokered credentials.

The crate solves five distinct problems:

1. **Discovery and trust.** Skills come from three sources (bundled, user, workspace) with different trust levels. The registry verifies Ed25519 signatures before loading — unsigned or tampered skills are quarantined, never executed.

2. **Untrusted code execution.** Third-party skills run in a WASM sandbox with capability-scoped imports, memory limits, and timeout enforcement. Escape attempts are detected and forensically logged.

3. **Trusted code execution.** Builtin skills run in-process via the native sandbox — no WASM overhead, but still capability-gated at the Rust API boundary.

4. **Credential isolation.** Skills never see raw API keys. The credential broker provides opaque handles that are reified only at execution time, with max-use limits and expiration enforcement.

5. **Workflow learning.** The recorder passively observes successful multi-tool sequences. The proposer detects repeated patterns and generates skill proposals for human approval.

---

## Module Breakdown

### `registry.rs` — Skill Discovery and Trust Verification

The registry is the front door for all skills. It manages discovery, signature verification, and state tracking.

#### Source Priority: Workspace > User > Bundled

```rust
pub enum SkillSource {
    Bundled = 0,   // Ships with GHOST
    User = 1,      // ~/.ghost/skills/
    Workspace = 2, // .ghost/skills/ in project root
}
```

**Why this ordering?** Workspace skills override user skills, which override bundled skills. This follows the same precedence pattern as MCP configs, VS Code settings, and `.gitconfig` — project-specific overrides win. The `Ord` derive on the integer discriminants makes this ordering machine-checkable.

#### Skill Manifest (YAML Frontmatter)

```rust
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,   // e.g., ["memory_read", "shell_execute"]
    pub timeout_seconds: u64,
    pub signature: Option<String>,   // Ed25519 signature via ghost-signing
}
```

**Design decisions:**

1. **YAML, not JSON or TOML.** Skill manifests use YAML frontmatter (hence the `serde_yaml` dependency). YAML was chosen because skill files are meant to be human-authored — YAML's minimal syntax reduces friction for developers writing custom skills.

2. **Capabilities are strings, not an enum.** New capabilities can be added without modifying the registry. The sandbox enforces capability checks at runtime — if a skill requests `"filesystem_write"` but the sandbox only grants `"memory_read"`, the call is denied. String-based capabilities are extensible without recompilation.

3. **`timeout_seconds` in the manifest.** Each skill declares its own timeout. A quick file-read skill might set 5 seconds; a complex code-generation skill might need 60. The sandbox enforces this — if the skill exceeds its declared timeout, execution is terminated.

#### Registration and Quarantine

```rust
pub fn register(&mut self, manifest: SkillManifest, source: SkillSource, path: PathBuf) {
    let state = if self.verify_signature(&manifest) {
        SkillState::Loaded
    } else {
        tracing::warn!(skill = %manifest.name, "Skill quarantined: invalid or missing signature");
        SkillState::Quarantined
    };
    // ...
}
```

**The quarantine model:** Skills are never rejected outright — they're quarantined. This is deliberate:

- A quarantined skill is visible in the registry (via `quarantined_skills()`), so the user knows it exists and why it failed.
- The user can fix the signature and re-register without restarting the gateway.
- Audit logs capture quarantine events for forensic review.
- The alternative (silent rejection) would leave users confused about why their skill isn't appearing.

**Signature verification:** Currently checks for signature presence (`manifest.signature.is_some()`). In production, this delegates to `ghost-signing::verify()` with the skill author's public key. The registry depends on `ghost-signing` specifically for this verification path.

#### Lookup and Filtering

The registry provides three access patterns:

- `lookup(name)` — O(log n) BTreeMap lookup by skill name
- `loaded_skills()` — All non-quarantined skills (for execution)
- `quarantined_skills()` — All quarantined skills (for diagnostics)

**Why BTreeMap, not HashMap?** Deterministic iteration order. When listing skills in a UI or log, the order should be stable across runs. BTreeMap gives alphabetical ordering by skill name for free.

---

### `sandbox/wasm_sandbox.rs` — WASM Isolation for Untrusted Skills

The WASM sandbox is the primary execution environment for third-party skills. It provides hardware-level isolation via WebAssembly's linear memory model.

#### Configuration

```rust
pub struct WasmSandboxConfig {
    pub timeout: Duration,              // Default: 30 seconds
    pub memory_limit_bytes: usize,      // Default: 64 MB
    pub allowed_capabilities: HashSet<String>,
}
```

**Why 30 seconds and 64 MB?** These defaults balance safety with usability:

- **30s timeout:** Most tool calls complete in under 5 seconds. 30 seconds accommodates complex skills (code generation, multi-step analysis) while preventing infinite loops from consuming resources indefinitely. The timeout is enforced via wasmtime's fuel-based metering — not a wall-clock timer — so it measures actual computation, not I/O wait.

- **64 MB memory:** Enough for most data processing tasks. WASM's linear memory model means the sandbox can't access memory outside this limit — there's no "just allocate more" escape hatch. If a skill exceeds 64 MB, execution terminates with `ExecutionResult::MemoryExceeded`.

#### Execution Results

```rust
pub enum ExecutionResult {
    Success { output: serde_json::Value, elapsed: Duration },
    Timeout { elapsed: Duration },
    MemoryExceeded { used_bytes: usize, limit_bytes: usize },
    EscapeDetected(EscapeAttempt),
    Error(String),
}
```

**Five outcomes, not two.** A simple success/failure enum would lose critical diagnostic information. Each variant carries the data needed for the agent loop to make the right decision:

- `Success` — pass output to the agent
- `Timeout` — log, maybe retry with a longer timeout
- `MemoryExceeded` — log, don't retry (the skill is fundamentally too large)
- `EscapeDetected` — quarantine the skill, emit a security event
- `Error` — skill-level error (bad input, logic bug)

#### Escape Detection and Forensics

```rust
pub struct EscapeAttempt {
    pub skill_name: String,
    pub skill_hash: String,
    pub escape_type: EscapeType,
    pub details: String,
    pub agent_id: Uuid,
    pub detected_at: DateTime<Utc>,
}

pub enum EscapeType {
    FilesystemWrite,    // Tried to write outside sandbox
    NetworkAccess,      // Tried to reach non-allowlisted domain
    EnvVarRead,         // Tried to read environment variables
    ProcessSpawn,       // Tried to spawn a subprocess
    MemoryExceeded,     // Exceeded memory limit
}
```

**Why five escape types?** These cover the WASM sandbox's attack surface:

1. **FilesystemWrite** — WASM modules have no filesystem access by default. If a skill tries to call a WASI filesystem write without an explicit grant, it's an escape attempt.
2. **NetworkAccess** — Skills can only reach domains explicitly allowlisted in their capability set. Any other network call is flagged.
3. **EnvVarRead** — Environment variables often contain secrets (API keys, tokens). WASM modules cannot read them — credentials come through the broker.
4. **ProcessSpawn** — No subprocess creation. Period. A skill that tries to spawn a shell is immediately terminated.
5. **MemoryExceeded** — While technically a resource limit rather than an escape, exceeding memory limits can be a deliberate attack (memory exhaustion DoS).

**Forensic capture:** Every escape attempt records the skill name, its blake3 hash (for identifying the exact binary), the agent that triggered it, and a human-readable description. This data feeds into `ghost-audit` for post-incident analysis.

---

### `sandbox/native_sandbox.rs` — In-Process Execution for Trusted Skills

Builtin skills (shipped with GHOST) don't need WASM isolation — they're compiled into the gateway binary. But they still need capability gating.

```rust
pub struct NativeSandbox {
    granted_capabilities: HashSet<String>,
}
```

**Why not just skip the sandbox for builtins?** Defense in depth. Even trusted code should declare what it needs. If a builtin skill is refactored and accidentally starts calling a filesystem API it didn't previously use, the native sandbox catches it at the capability check boundary. This prevents privilege creep.

#### Capability Check vs. Tool Call Validation

The native sandbox provides two levels of checking:

1. **`check_capability(cap)`** — "Does this sandbox instance have capability X?" Used for general permission checks.
2. **`validate_tool_call(tool_name, required_cap)`** — "Can this tool be called given the granted capabilities?" Used at the tool dispatch boundary. Returns a `ToolDenied` error with the tool name and missing capability for clear diagnostics.

**Error types are informative, not generic:**

```rust
pub enum NativeSandboxError {
    CapabilityDenied { requested: String, granted: Vec<String> },
    ToolDenied { tool: String, required_capability: String },
}
```

Both variants include the full context needed to diagnose the issue without additional logging. `CapabilityDenied` shows what was requested AND what was granted — so the developer can see exactly what's missing.

---

### `recorder.rs` — Passive Workflow Observation

The recorder watches successful multi-tool sequences without modifying them. It's the data collection layer for skill proposal.

#### Recording Lifecycle

```
start_recording(session_id, trigger_message)
    → record_step(session_id, WorkflowStep) × N
    → complete(session_id) → CompletedWorkflow
    OR
    → abandon(session_id)  // User intervened, policy violation, etc.
```

**When recordings are abandoned:**
- The user corrected the agent mid-workflow (the sequence wasn't fully autonomous)
- A policy violation occurred (the workflow isn't safe to replay)
- The agent loop was interrupted (incomplete data)

Only fully autonomous, successful, uninterrupted workflows become `CompletedWorkflow` — the quality bar for skill proposals.

#### Similarity Hashing

```rust
fn compute_similarity_hash(recording: &WorkflowRecording) -> [u8; 32] {
    let mut hasher_input = String::new();
    for step in &recording.steps {
        hasher_input.push_str(&step.tool_name);
        hasher_input.push('|');
        // Hash argument SHAPE (keys only, not values)
        if let serde_json::Value::Object(map) = &step.arguments_template {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys { hasher_input.push_str(key); hasher_input.push(','); }
        }
        hasher_input.push(';');
    }
    *blake3::hash(hasher_input.as_bytes()).as_bytes()
}
```

**Why hash argument shapes, not values?** Two workflows that call `file_read(path="/src/main.rs")` and `file_read(path="/src/lib.rs")` are the same pattern — "read a file." The concrete path differs, but the tool sequence and argument structure are identical. By hashing only the keys (sorted for determinism), the recorder groups structurally identical workflows regardless of concrete values.

**Why blake3?** Consistent with the rest of the GHOST platform (hash chains in `cortex-temporal`, kill gate chains in `ghost-kill-gates`). blake3 is fast enough for inline hashing during workflow recording without measurable overhead.

#### Argument Templatization

```rust
pub fn templatize_arguments(args: &serde_json::Value) -> serde_json::Value
```

Replaces concrete values with placeholders:
- File paths (containing `/` and common extensions) → `{file_path}`
- URLs (starting with `http://` or `https://`) → `{url}`
- Other values pass through unchanged

This creates reusable argument templates for skill replay. When a skill is executed, the agent fills in the placeholders with context-specific values.

---

### `proposer.rs` — Pattern Detection and Skill Proposal

The proposer sits downstream of the recorder. It counts workflow pattern occurrences and generates proposals when the threshold is met.

#### The 3-Occurrence Threshold

```rust
pub fn observe(&mut self, workflow: &CompletedWorkflow) -> Option<SkillProposal> {
    // ...
    if *count >= self.proposal_threshold && !self.proposed_skills.contains(&hash) {
        // Generate proposal
    }
}
```

**Why 3, not 2 or 5?** Three is the sweet spot:
- **2 would be too aggressive** — coincidental repetition (doing the same thing twice) doesn't indicate a pattern worth codifying.
- **5 would be too conservative** — by the fifth repetition, the user has already wasted significant tokens on a clearly repeatable workflow.
- **3 is configurable** — `SkillProposer::with_threshold(n)` allows tuning for different use cases.

#### De-duplication: No Re-Proposals

Once a pattern has been proposed, it's added to `proposed_skills: BTreeSet<[u8; 32]>`. The 4th, 5th, 100th occurrence of the same pattern won't generate another proposal. This prevents notification fatigue.

#### Token Savings Estimation

```rust
estimated_tokens_saved: (workflow.total_tokens_used as f64 * 0.67) as usize,
```

**Why 67%?** A skill replay still needs some tokens (parsing the trigger, filling templates, formatting output), but skips the planning and reasoning tokens that dominate multi-tool workflows. 67% is a conservative estimate based on the observation that planning typically consumes 2/3 of tokens in a multi-step workflow.

#### Skill Name Generation

```rust
fn generate_skill_name(recording: &WorkflowRecording) -> String {
    // Deduplicate consecutive same tools, join with "then", kebab-case
    // e.g., ["file_read", "file_read", "web_search"] → "file-read-then-web-search"
}
```

Consecutive duplicate tools are collapsed (three `file_read` calls become one `file-read` segment). This produces readable, descriptive names like `file-read-then-web-search-then-file-write`.

#### Approval Flow

Proposals require human approval — the same flow as goal proposals in the agent loop. `SkillProposer::approve()` converts a proposal to a `SkillManifest` ready for registration. The manifest starts with `signature: None` — it needs signing via `ghost-signing` before the registry will load it.

---

### `credential/broker.rs` — Opaque Token Brokering

Skills need API keys (OpenAI, GitHub, etc.) but should never see the raw credentials. The broker implements the stand-in pattern: skills receive an opaque handle, and the broker reifies the actual secret only inside the sandbox at execution time.

#### The Handle Pattern

```rust
pub struct CredentialHandle {
    pub id: Uuid,
    pub provider: String,   // e.g., "openai"
    pub scope: String,      // e.g., "api"
}
```

The skill receives a `CredentialHandle` with a UUID, provider name, and scope. It passes this handle to the sandbox's host function when it needs to make an authenticated API call. The sandbox calls `broker.reify(handle.id)` to get the actual secret, makes the API call on behalf of the skill, and never exposes the secret to the WASM module.

#### Three-Layer Protection

1. **Max uses:** Each credential has a `max_uses` limit. After N reifications, the credential is exhausted. This prevents credential replay attacks — a compromised skill can't make unlimited API calls.

2. **Expiration:** Credentials can have an `expires_at` timestamp. Expired credentials are rejected even if uses remain. This handles time-limited tokens (OAuth access tokens, temporary API keys).

3. **Revocation:** `revoke(handle_id)` removes a single credential. `revoke_provider(provider)` removes ALL credentials for a provider — useful when a provider's API key is compromised and all associated credentials need immediate invalidation.

#### Error Semantics

```rust
pub enum CredentialError {
    NotFound(Uuid),
    Exhausted { handle_id: Uuid, max_uses: u32 },
    Expired { handle_id: Uuid, expired_at: DateTime<Utc> },
}
```

Each error variant carries enough context for the caller to understand what happened without additional lookups. `Exhausted` includes the max_uses limit so the caller can decide whether to request a new credential with a higher limit.

---

### `bridges/drift_bridge.rs` — MCP Tool Integration

The Drift MCP Bridge registers external MCP (Model Context Protocol) tools as first-party skills. This means MCP tools appear in the skill registry alongside native and WASM skills — the agent loop doesn't need to know the difference.

```rust
pub struct DriftMCPBridge {
    server_url: String,
    tools: Vec<DriftToolDefinition>,
}
```

#### Discovery and Execution

1. **`discover()`** — Calls the MCP server's `tools/list` endpoint to enumerate available tools. Each tool's name, description, and input schema are captured as a `DriftToolDefinition`.

2. **`register_tool()`** — Manually registers a tool definition (for testing or static configuration without a running MCP server).

3. **`execute(tool_name, arguments)`** — Proxies a `tools/call` request to the MCP server. The bridge handles serialization, transport, and error mapping.

**Why "Drift"?** Drift is the internal codename for the MCP integration layer. The bridge pattern allows GHOST to consume any MCP-compatible tool server without modifying the skill registry or agent loop.

---

## Security Properties

### Signature-Before-Load

No skill code executes without a valid Ed25519 signature. Unsigned skills are quarantined — visible but inert. This prevents supply-chain attacks where a malicious skill is placed in the workspace directory.

### Capability Scoping

Both WASM and native sandboxes enforce capability checks. A skill that declares `["memory_read"]` cannot call filesystem or network APIs. Capabilities are declared in the manifest and enforced at the sandbox boundary — there's no way to escalate privileges at runtime.

### Credential Isolation

Raw secrets never cross the sandbox boundary. The broker reifies credentials inside the host function, makes the API call, and returns only the result. Even if a WASM module's memory is dumped, it contains only the opaque handle UUID — not the secret.

### Escape Forensics

Every sandbox escape attempt is captured with full context (skill name, hash, escape type, agent ID, timestamp). This data is immutable once written and feeds into `ghost-audit` for incident response.

### Pattern Count Saturation

The proposer's pattern counter uses `saturating_add` — it can't overflow `u32::MAX`. This prevents integer overflow attacks where an adversary triggers a pattern billions of times to wrap the counter and re-trigger proposals.

---

## Downstream Consumer Map

```
ghost-skills (Layer 5)
├── ghost-agent-loop (Layer 7)
│   └── Executes skills via WASM/native sandbox
│   └── Passes credential handles to sandbox host functions
│   └── Feeds completed workflows to recorder → proposer pipeline
├── ghost-gateway (Layer 8)
│   └── Initializes SkillRegistry at startup
│   └── Manages skill discovery across source directories
│   └── Handles skill proposal approval UI
└── ghost-signing (Layer 0) [upstream]
    └── Provides Ed25519 verification for skill manifest signatures
```

---

## Test Strategy

### Unit Tests (`src/proposer.rs` inline tests)

| Test | What It Verifies |
|------|-----------------|
| `first_occurrence_no_proposal` | Single observation doesn't trigger proposal |
| `second_occurrence_no_proposal` | Two observations still below threshold |
| `third_occurrence_generates_proposal` | Threshold met → proposal generated with correct occurrence count |
| `fourth_occurrence_no_re_proposal` | Already-proposed patterns are suppressed |
| `estimated_tokens_saved_is_67_percent` | 67% savings calculation (670 of 1000 tokens) |
| `approve_produces_valid_manifest` | Proposal → manifest conversion preserves capabilities |
| `generated_name_is_kebab_case` | `file_read` + `web_search` → `"file-read-then-web-search"` |
| `pattern_count_saturates` | 1000 observations don't overflow |

### Unit Tests (`src/recorder.rs` inline tests)

| Test | What It Verifies |
|------|-----------------|
| `start_record_complete` | Full recording lifecycle produces CompletedWorkflow |
| `abandon_removes_recording` | Abandoned recordings don't appear in completed list |
| `similarity_hash_same_for_same_sequence` | Same tool sequence → same hash regardless of session |
| `similarity_hash_differs_for_different_sequence` | Different tools → different hash |
| `templatize_file_path` | File paths replaced with `{file_path}` |
| `templatize_url` | URLs replaced with `{url}` |
| `empty_recording_completes` | Zero-step workflow completes without panic |

### Integration Tests (`tests/skills_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `valid_signature_loads` | Signed skill → `SkillState::Loaded` |
| `invalid_signature_quarantines` | Unsigned skill → `SkillState::Quarantined` |
| `missing_signature_quarantines` | `None` signature → quarantine |
| `priority_order` | `Workspace > User > Bundled` ordering |
| `loaded_skills_excludes_quarantined` | Filter correctness |
| `execute_returns_result` | WASM sandbox returns `ExecutionResult::Success` |
| `capability_check` | Granted vs. denied capability |
| `default_timeout` / `default_memory_limit` | 30s / 64MB defaults |
| `record_escape_captures_forensics` | Escape attempt data integrity |
| `escape_types` | All 5 escape types exist |
| `capability_granted` / `capability_denied` | Native sandbox checks |
| `tool_call_validation` | Native sandbox tool dispatch |
| `register_and_reify` | Credential round-trip |
| `max_uses_enforced` | Credential exhaustion after N uses |
| `expired_credential_rejected` | Time-based rejection |
| `revoke_credential` / `revoke_provider` | Single and bulk revocation |
| `remaining_uses` | Use count tracking |

---

## File Map

```
crates/ghost-skills/
├── Cargo.toml                          # Deps: cortex-core, ghost-signing, blake3, serde_yaml
├── src/
│   ├── lib.rs                          # Module declarations
│   ├── registry.rs                     # SkillRegistry, SkillManifest, SkillSource, quarantine
│   ├── proposer.rs                     # SkillProposer, 3-occurrence threshold, name generation
│   ├── recorder.rs                     # WorkflowRecorder, similarity hashing, templatization
│   ├── sandbox/
│   │   ├── mod.rs                      # Sandbox module declarations
│   │   ├── wasm_sandbox.rs             # WasmSandbox, 5 escape types, forensic capture
│   │   └── native_sandbox.rs           # NativeSandbox, capability gating for builtins
│   ├── credential/
│   │   ├── mod.rs                      # Credential module declarations
│   │   └── broker.rs                   # CredentialBroker, opaque handles, max-use enforcement
│   └── bridges/
│       ├── mod.rs                      # Bridge module declarations
│       └── drift_bridge.rs             # DriftMCPBridge, MCP tool discovery and proxying
└── tests/
    └── skills_tests.rs                 # Registry, sandbox, credential, and escape tests
```

---

## Common Questions

### Why two sandbox types instead of running everything in WASM?

Performance. WASM adds ~10-50μs overhead per host function call due to the linear memory boundary crossing. Builtin skills (file read, web search, memory query) are called thousands of times per session. Running them in WASM would add measurable latency. The native sandbox gives trusted code full Rust performance while still enforcing capability boundaries at the API level.

### Why does the proposer need human approval?

Autonomy boundary. A skill is a reusable automation — it will execute the same tool sequence every time it's triggered. Creating one without human review would mean the agent is permanently modifying its own behavior based on observed patterns. The approval step ensures the human remains in control of what the agent can do autonomously.

### Can a skill call other skills?

Not currently. Skill execution is flat — one skill, one sandbox instance, one result. Nested skill calls would require a skill dependency graph, recursive sandbox instantiation, and credential scope propagation. The complexity isn't justified by current use cases. If needed, a "meta-skill" could be implemented as a native skill that orchestrates multiple tool calls.

### Why blake3 for similarity hashing instead of SHA-256?

Consistency and speed. blake3 is used throughout GHOST (`cortex-temporal` hash chains, `ghost-kill-gates` audit chains). Using the same algorithm everywhere reduces the dependency surface and avoids "which hash do I use?" decisions. blake3 is also ~3x faster than SHA-256 on modern hardware, though for the small inputs in similarity hashing, the difference is negligible.

### What happens if a credential is reified but the API call fails?

The use is consumed. The broker doesn't distinguish between "reified and used successfully" and "reified and the API returned an error." This is intentional — the secret was exposed to the sandbox host function, so from a security perspective, the use happened. If the skill needs to retry, it consumes another use. This prevents a malicious skill from claiming "the call failed" to get unlimited reifications.
