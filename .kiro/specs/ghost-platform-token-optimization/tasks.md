# Tasks — GHOST Platform Token Optimization & Autonomy Efficiency

> Generated from `INFRASTRUCTURE_AUDIT.md` (15 sections, 32 audit items).
> Continues the GHOST Platform numbering (Phases 16–22, Tasks 16.1–22.4).
> No source code in this file. Each task describes WHAT to build, WHAT context is needed,
> HOW to verify it works (production-grade, not happy-path), and WHERE it maps to the audit.
> Tasks are ordered by dependency — later phases depend on earlier phases compiling and passing.
> All conventions from v1 tasks.md apply: thiserror errors, tracing, BTreeMap for signed payloads,
> zeroize on key material, bounded async channels, proptest for invariants, workspace dep style.
>
> Critical path: Phase 16 (free refactor) → Phase 17 (observation masking) → Phase 18 (compressor)
> → Phase 19 (cron scheduler) → Phase 20 (context compaction) → Phase 21 (skill evolution)
> → Phase 22 (mesh efficiency + hardening).
>
> Projected savings: 500K tokens/session → 50-80K tokens/session (6-10x reduction).

---

## Phase 16: KV Cache Optimization (Week 1)

> Deliverable: Stable prefix preservation for L0-L6 (~6,900 tokens). Tool filtering moved
> from L3 content removal to L6 constraint instruction. L4 timestamp sanitization.
> Spotlighting instruction templated once, not re-injected per turn. Stable prefix hash
> validation ensures cache hits across turns. 90% cost reduction on the stable prefix portion.

---

### Task 16.1 — ghost-agent-loop: Stable Prefix Hash Validator
- **Audit**: Item 3.1 (Stable Prefix Preservation) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/stable_prefix.rs` (NEW), `src/context/mod.rs` (modify)
- **Context needed**: Existing `PromptCompiler` in `src/context/prompt_compiler.rs` assembles 10 layers from `PromptInput`. L0-L5 are stable within a session (~6,900 tokens). Currently, content is passed fresh each turn via `PromptInput` — no guarantee L0-L5 is identical across turns. KV cache providers (Anthropic, OpenAI) cache based on prefix token identity — any mutation invalidates from that point forward.
- **What to build**:
  - `StablePrefixCache` struct:
    - `prefix_hash: Option<[u8; 32]>` — blake3 hash of concatenated L0-L5 content
    - `cached_layers: Option<[String; 6]>` — memoized L0-L5 content from first turn
    - `validate(input: &PromptInput) -> PrefixValidation` — computes hash of current L0-L5, compares to cached
  - `PrefixValidation` enum: `CacheHit`, `CacheMiss { layer: u8, reason: String }`, `FirstTurn`
  - On `FirstTurn`: store hash + content, return `FirstTurn`
  - On subsequent turns: compare hash. If match → `CacheHit`. If mismatch → log which layer changed and why → `CacheMiss`
  - `StablePrefixCache::reset()` — clear cache (for session boundary)
  - Tracing: on `CacheMiss`, emit `tracing::warn!` with the layer index and a content diff summary (first 50 chars that differ)
- **Conventions**: blake3 for hashing (already in workspace deps). Cache is per-session, not global. Thread-safe via `Arc<Mutex<>>` if shared across async tasks.
- **Testing**:
  - Unit: First turn → `FirstTurn`, hash stored
  - Unit: Second turn with identical input → `CacheHit`
  - Unit: Second turn with L3 changed → `CacheMiss { layer: 3 }`
  - Unit: Second turn with L0 changed → `CacheMiss { layer: 0 }`
  - Unit: `reset()` clears cache, next call returns `FirstTurn`
  - Unit: Hash is deterministic — same input always produces same hash
  - Proptest: For 500 random PromptInput pairs where L0-L5 are identical, always `CacheHit`
  - Proptest: For 500 random PromptInput pairs where one layer differs, always `CacheMiss` with correct layer
  - Adversarial: Empty L0-L5 content → valid hash, no panic
  - Adversarial: Very large L0 content (100KB) → hash completes in <10ms

---

### Task 16.2 — ghost-agent-loop: Move Tool Filtering from L3 to L6 Constraint
- **Audit**: Item 3.1 (L3 tool filtering invalidates cache) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/prompt_compiler.rs` (modify)
- **Context needed**: Existing `PromptCompiler::filter_tool_schemas()` removes tool lines from L3 based on `intervention_level`. This changes L3 content per convergence level → KV cache miss. The blueprint alternative: keep L3 constant (all tool schemas always present), add a constraint instruction to L6 (convergence state) that tells the LLM which tools are permitted at the current level. LLM providers support logit biasing / instruction-following to restrict tool use without removing schemas.
- **What to build**:
  - Remove the content-filtering behavior from `filter_tool_schemas()` — L3 always contains ALL tool schemas regardless of convergence level
  - Keep `filter_tool_schemas()` as a public function but change its purpose: instead of returning filtered schema text, return a constraint instruction string
  - New function `tool_constraint_instruction(intervention_level: u8) -> String`:
    - L0: `""` (empty — all tools permitted)
    - L1: `""` (all tools permitted)
    - L2: `"TOOL RESTRICTION: Do not use proactive or heartbeat tools at current convergence level."`
    - L3: `"TOOL RESTRICTION: Only task-focused tools permitted. Do not use proactive, heartbeat, personal, or emotional tools."`
    - L4: `"TOOL RESTRICTION: Minimal tools only. Only read, search, shell, and filesystem tools are permitted."`
  - Inject this constraint into L6 content (convergence state) — append after existing convergence state text
  - Deprecate the old `filter_tool_schemas()` with `#[deprecated]` attribute
- **Conventions**: L3 content is now immutable within a session → stable prefix preserved. The constraint instruction in L6 is expected to change (L6 is already mutable — convergence state changes every evaluation). This is a net-zero token change: tokens removed from L3 filtering are replaced by a shorter instruction in L6.
- **Testing**:
  - Unit: `tool_constraint_instruction(0)` returns empty string
  - Unit: `tool_constraint_instruction(2)` contains "proactive" and "heartbeat"
  - Unit: `tool_constraint_instruction(3)` contains "task-focused"
  - Unit: `tool_constraint_instruction(4)` contains "Minimal tools only"
  - Unit: L3 content is identical regardless of intervention_level (compile with level 0 and level 3, compare L3)
  - Unit: L6 content includes tool constraint at level >= 2
  - Unit: StablePrefixCache reports `CacheHit` when only convergence level changes (L3 unchanged)
  - Integration: Compile prompt at L0, then at L3 — L0-L5 hash is identical (cache preserved)
  - Adversarial: intervention_level > 4 → same as L4 (clamp)

---

### Task 16.3 — ghost-agent-loop: L4 Timestamp Sanitization
- **Audit**: Item 3.4 (No Dynamic Timestamps in Early Layers) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/prompt_compiler.rs` (modify)
- **Context needed**: L4 (environment context) content is caller-provided via `PromptInput.environment`. If the caller includes timestamps with seconds/milliseconds, every turn produces different L4 content → cache miss. The blueprint says: date is fine, hour is acceptable (cache TTL is 5-10 min), but seconds/milliseconds must be stripped.
- **What to build**:
  - `sanitize_environment_timestamps(content: &str) -> String`:
    - Regex to detect common timestamp patterns with seconds precision:
      - ISO 8601: `\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}` → truncate to `\d{4}-\d{2}-\d{2}T\d{2}:\d{2}`
      - Unix epoch with ms: `\d{10,13}` (10-13 digit numbers) → remove
      - Time with seconds: `\d{2}:\d{2}:\d{2}` → truncate to `\d{2}:\d{2}`
    - Returns sanitized content with seconds/ms stripped
  - Call `sanitize_environment_timestamps()` on `input.environment` inside `compile()` before assembling L4
  - Add `tracing::debug!` when timestamps are sanitized (so developers know it's happening)
- **Conventions**: Regex compiled once via `once_cell::sync::Lazy` (already in workspace deps). Only affects L4 — other layers are not sanitized. Date and hour granularity preserved for context relevance.
- **Testing**:
  - Unit: `"2026-02-28T14:30:45Z"` → `"2026-02-28T14:30"`
  - Unit: `"2026-02-28T14:30:45.123Z"` → `"2026-02-28T14:30"`
  - Unit: `"Current time: 14:30:45"` → `"Current time: 14:30"`
  - Unit: `"Date: 2026-02-28"` → unchanged (no seconds)
  - Unit: Content without timestamps → unchanged
  - Unit: Multiple timestamps in one string → all sanitized
  - Unit: StablePrefixCache reports `CacheHit` when only seconds differ between turns
  - Proptest: For 500 random strings without timestamp patterns, output equals input
  - Adversarial: String that looks like a timestamp but isn't (e.g., version "1.2.3") → not mangled

---

### Task 16.4 — ghost-agent-loop: Spotlighting Instruction Template Fix
- **Audit**: Item 3.3 (Append-only context — L1 mutation) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/spotlighting.rs` (modify), `src/context/prompt_compiler.rs` (modify)
- **Context needed**: Currently `PromptCompiler::compile()` injects the spotlighting system instruction INTO L1 every turn via `format!("{}\n\n{}", instruction, l1.content)`. This mutates L1 content, which is an early layer — any change invalidates KV cache from L1 forward. The instruction text is static (same every turn), but the injection happens after initial L1 content is set, making the hash validation see different content on first vs subsequent turns if the caller provides L1 differently.
- **What to build**:
  - Move spotlighting instruction from runtime injection to a template that's part of the L1 input itself
  - New method `Spotlighter::l1_template(&self, base_simulation_prompt: &str) -> String`:
    - If spotlighting enabled: prepend instruction to base prompt (same as current behavior, but done BEFORE `PromptInput` is constructed)
    - If disabled: return base prompt unchanged
  - Remove the post-assembly L1 mutation block from `PromptCompiler::compile()` (the `if let Some(instruction) = self.spotlighter.system_instruction()` block)
  - The caller (gateway bootstrap / agent loop) calls `spotlighter.l1_template()` when constructing `PromptInput.simulation_prompt` — this happens once at session start, not per turn
- **Conventions**: L1 content is now set once and never mutated during a session. The spotlighting instruction is baked into the simulation prompt at session initialization time.
- **Testing**:
  - Unit: `l1_template()` with spotlighting enabled prepends instruction
  - Unit: `l1_template()` with spotlighting disabled returns base prompt unchanged
  - Unit: `compile()` no longer modifies L1 content (L1 output equals L1 input)
  - Unit: StablePrefixCache reports `CacheHit` across turns (L1 stable)
  - Unit: Spotlighting still applied to L7/L8 content (datamarking behavior unchanged)
  - Integration: Full compile with spotlighting → L1 contains instruction, L7/L8 are datamarked, L0/L9 untouched



---

## Phase 17: Observation Masking (Week 2)

> Deliverable: Old tool outputs in L8 (conversation history) replaced with compact references.
> Full outputs cached to disk for on-demand retrieval. 50% reduction on L8 token usage.
> Tool output cache with content-addressable storage. Age-based masking with configurable
> recency window.

---

### Task 17.1 — ghost-agent-loop: Tool Output Cache
- **Audit**: Item 2.2 (Observation Masking) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/tool_output_cache.rs` (NEW), `src/context/mod.rs` (modify)
- **Context needed**: Existing `PromptCompiler` assembles L8 from `PromptInput.conversation_history` as a raw string. Tool results appear inline in conversation history as `tool_result` messages. The blueprint says: cache full tool outputs to disk, replace old ones with compact references. The `.ghost/` directory is the standard location for GHOST runtime data.
- **What to build**:
  - `ToolOutputCache` struct:
    - `cache_dir: PathBuf` — defaults to `.ghost/cache/tool_outputs/`
    - `store(tool_call_id: &str, tool_name: &str, output: &str) -> Result<CacheRef, std::io::Error>`:
      - Compute blake3 hash of output → use as filename: `{hash}.txt`
      - Write output to `{cache_dir}/{hash}.txt` (atomic write: temp + rename)
      - Return `CacheRef { hash, tool_name, tool_call_id, token_count, byte_count }`
    - `load(hash: &str) -> Result<String, std::io::Error>` — read cached output
    - `reference_string(cache_ref: &CacheRef) -> String`:
      - Returns `"[tool_result: {tool_name} → {token_count} tokens, ref:{hash_prefix}]"`
      - `hash_prefix` is first 8 chars of hash (enough for uniqueness)
    - `cleanup_older_than(duration: Duration) -> Result<u32, std::io::Error>` — remove stale cache files
  - `CacheRef` struct: `hash: String`, `tool_name: String`, `tool_call_id: String`, `token_count: usize`, `byte_count: usize`
- **Conventions**: Content-addressable storage (same output → same hash → deduplicated). Atomic writes prevent corruption. Cache is per-workspace, not per-session (tool outputs may be reused across sessions). blake3 for hashing (fast, already in workspace).
- **Testing**:
  - Unit: `store()` creates file at expected path
  - Unit: `store()` then `load()` returns identical content
  - Unit: `store()` same content twice → same hash, no duplicate file
  - Unit: `reference_string()` format matches expected pattern
  - Unit: `cleanup_older_than(0)` removes all files
  - Unit: `cleanup_older_than(1 hour)` keeps recent files
  - Proptest: For 500 random tool outputs, store then load round-trips correctly
  - Adversarial: Very large output (1MB) → stores and loads correctly
  - Adversarial: Output with null bytes → stores correctly
  - Adversarial: Concurrent store of same content → no corruption (atomic write)
  - Adversarial: Cache dir doesn't exist → created automatically

---

### Task 17.2 — ghost-agent-loop: Observation Masker
- **Audit**: Item 2.2 (Observation Masking) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/observation_masker.rs` (NEW), `src/context/mod.rs` (modify)
- **Context needed**: Existing conversation history in L8 contains interleaved user messages, assistant messages, and tool_result messages. Tool results from recent turns should remain inline (the LLM needs them for context). Tool results from older turns should be replaced with compact references. The `ToolOutputCache` from Task 17.1 provides the caching mechanism.
- **What to build**:
  - `ObservationMasker` struct:
    - `cache: ToolOutputCache`
    - `recency_window: usize` — number of recent turns to keep inline (default 3)
    - `min_token_threshold: usize` — only mask outputs larger than this (default 200 tokens)
  - `ObservationMaskerConfig`:
    - `enabled: bool` (default true)
    - `recency_window: usize` (default 3)
    - `min_token_threshold: usize` (default 200)
    - `cache_dir: PathBuf` (default `.ghost/cache/tool_outputs/`)
  - `mask_history(history: &str) -> Result<String, std::io::Error>`:
    - Parse conversation history to identify tool_result blocks
    - For each tool_result older than `recency_window` turns AND larger than `min_token_threshold`:
      1. Store full output in `ToolOutputCache`
      2. Replace inline content with `reference_string()`
    - Return modified history with old tool outputs replaced
  - `unmask_reference(reference: &str) -> Result<String, std::io::Error>`:
    - Parse reference string to extract hash
    - Load full output from cache
    - Used when the LLM explicitly requests a cached tool output
- **Conventions**: Masking is applied BEFORE spotlighting (mask first, then datamark what remains). The recency window counts assistant turns, not individual messages. Tool results from the current turn are NEVER masked.
- **Testing**:
  - Unit: History with 5 tool results, recency_window=3 → last 3 inline, first 2 masked
  - Unit: History with 1 tool result → nothing masked (within recency window)
  - Unit: Tool result below min_token_threshold → not masked regardless of age
  - Unit: `unmask_reference()` recovers original content
  - Unit: Masked history is shorter than original (token count reduced)
  - Unit: Non-tool-result messages are never modified
  - Unit: Config disabled → history returned unchanged
  - Proptest: For 500 random histories with N tool results, masked count = max(0, N - recency_window)
  - Adversarial: Malformed tool_result block → skipped, not panic
  - Adversarial: Cache miss on unmask (file deleted) → graceful error

---

### Task 17.3 — ghost-agent-loop: Integrate ObservationMasker into PromptCompiler
- **Audit**: Item 2.2 (PromptCompiler integration) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/prompt_compiler.rs` (modify)
- **Context needed**: Existing `PromptCompiler::compile()` takes `PromptInput` and assembles layers. L8 (`conversation_history`) is currently passed through as-is. The `ObservationMasker` from Task 17.2 should be applied to L8 content before assembly.
- **What to build**:
  - Add `observation_masker: Option<ObservationMasker>` field to `PromptCompiler`
  - New constructor: `PromptCompiler::with_observation_masking(context_window, spotlighting_config, masker_config)`
  - In `compile()`, before assembling L8:
    - If `observation_masker` is `Some`, call `masker.mask_history(&input.conversation_history)`
    - Use masked history as L8 content
    - Log token savings: `tracing::info!(original_tokens, masked_tokens, saved, "Observation masking applied")`
  - Ordering in `compile()`: observation masking → spotlighting → budget allocation → truncation
  - Backward compatibility: existing `PromptCompiler::new()` and `with_spotlighting()` constructors unchanged (no masking)
- **Conventions**: Observation masking is opt-in via constructor. When combined with spotlighting, masking happens first (reduce content), then spotlighting marks what remains. This is the correct order because masking reduces token count, and spotlighting doubles it — masking first minimizes the doubling impact.
- **Testing**:
  - Unit: `PromptCompiler::new()` → no masking applied (backward compat)
  - Unit: `with_observation_masking()` → masking applied to L8
  - Unit: Masking + spotlighting → masked content is datamarked
  - Unit: Token count of L8 is reduced after masking
  - Unit: L0-L7, L9 are unaffected by observation masking
  - Integration: Full compile with masking → L8 contains references for old tool outputs, inline for recent
  - Adversarial: Masker returns error → fall back to unmasked history (non-fatal)

---

## Phase 18: Compressor-Predictor Pipeline (Weeks 3–4)

> Deliverable: ContentQuarantine repurposed as general-purpose compressor for ALL tool outputs.
> Local model support (Ollama/Qwen-2.5-7B) for zero-cost compression. L7 memory compression
> via post-filter summarization. 87-98% reduction on raw tool output tokens.

---

### Task 18.1 — ghost-llm: Local Model Tier for QuarantinedLLM
- **Audit**: Item 2.1 (ContentQuarantine as Compressor) | **Layer**: 3
- **Crate**: `crates/ghost-llm/` (MODIFY existing)
- **Files**: `src/quarantine.rs` (modify), `src/router.rs` (modify)
- **Context needed**: Existing `QuarantineModelTier` enum has `Free` and `Cheap` variants. Existing `OllamaProvider` in `src/provider.rs` supports local models with zero token cost. The blueprint calls for a dedicated local 3-7B model (e.g., Qwen-2.5-7B) for compression — this avoids spending API tokens on compression calls. Existing `ModelRouter` has 4 tiers: Free, Cheap, Standard, Premium.
- **What to build**:
  - Add `Local` variant to `QuarantineModelTier` enum
  - `QuarantineModelTier::Local` maps to `OllamaProvider` with configurable model name
  - Update `QuarantineModelTier::to_complexity_tier()`: `Local` → `ComplexityTier::Free` (zero cost)
  - Add `ComplexityTier::Local` variant to `ComplexityTier` enum (new tier below Free)
  - Update `ModelRouter`:
    - `providers` array: expand from `[Option<Arc<dyn LLMProvider>>; 4]` to `[Option<Arc<dyn LLMProvider>>; 5]` (add Local slot)
    - `get_provider()` fallback chain: Local → Free → Cheap → Standard → Premium
  - `QuarantineConfig` additions:
    - `local_model: Option<String>` — model name for local provider (e.g., `"qwen2.5:7b"`)
    - `local_endpoint: Option<String>` — Ollama endpoint (default `"http://localhost:11434"`)
  - When `model_tier` is `Local` and local model is unavailable, fall back to `Cheap` tier with `tracing::warn!`
- **Conventions**: Local model is optional — if not configured, quarantine uses Cheap tier as before. Local model availability is checked at startup (health check to Ollama endpoint). Zero-cost tracking: `CostCalculator` returns 0.0 for Local tier.
- **Testing**:
  - Unit: `QuarantineModelTier::Local` exists and maps to `ComplexityTier::Local`
  - Unit: `ComplexityTier::Local` is below `Free` in ordering
  - Unit: `ModelRouter` with Local provider set → `get_provider(Local)` returns it
  - Unit: `ModelRouter` without Local provider → falls back to Free
  - Unit: `CostCalculator::estimate()` with Local tier pricing → 0.0 total
  - Unit: `QuarantineConfig` with `local_model: Some("qwen2.5:7b")` parses correctly
  - Unit: `QuarantineConfig` with `local_model: None` → falls back to Cheap
  - Adversarial: Local endpoint unreachable → graceful fallback to Cheap with warning

---

### Task 18.2 — ghost-llm: Enable ContentQuarantine as Default Compressor
- **Audit**: Item 2.1 (Flip quarantine from defensive to compressor) | **Layer**: 3
- **Crate**: `crates/ghost-llm/` (MODIFY existing)
- **Files**: `src/quarantine.rs` (modify)
- **Context needed**: Existing `ContentQuarantine` has `enabled: false` by default and only triggers for configured `content_types` (e.g., `"web_fetch"`, `"email_read"`). The blueprint says: flip to enabled by default for ALL tool outputs, not just security-sensitive ones. The compressor extracts structured information, reducing 50K tokens of raw output to 500-2000 tokens of extraction.
- **What to build**:
  - Change `QuarantineConfig::default()`: `enabled: true` (was `false`)
  - Add `compress_all_tool_outputs: bool` field to `QuarantineConfig` (default `true`)
  - When `compress_all_tool_outputs` is true: `should_quarantine()` returns true for ALL tool output content types, not just configured ones
  - When false: existing behavior (only configured content_types)
  - Add `compression_mode: CompressionMode` enum to `QuarantineConfig`:
    - `SecurityOnly` — original behavior (defensive quarantine for untrusted content)
    - `CompressAll` — compress all tool outputs for token efficiency
    - `CompressLarge { threshold_tokens: usize }` — only compress outputs above threshold (default 500 tokens)
  - Default mode: `CompressLarge { threshold_tokens: 500 }`
  - Update `ContentQuarantine::should_quarantine()` to check `compression_mode`
  - Add `bits_per_token` tracking:
    - `CompressionStats` struct: `original_tokens: usize`, `compressed_tokens: usize`, `compression_ratio: f64`, `bits_per_token: f64`
    - `quarantine_content()` returns `(String, CompressionStats)` instead of just `String`
    - `bits_per_token = (compressed_tokens as f64 * log2(vocab_size)) / original_tokens as f64` (approximate with vocab_size = 100_000)
  - Tracing: emit `tracing::info!` with compression stats after each compression
- **Conventions**: Backward compatibility: `SecurityOnly` mode preserves original behavior. `CompressLarge` is the recommended default — small tool outputs (< 500 tokens) pass through uncompressed (compression overhead not worth it). The extraction prompt is tool-type-aware: file reads get "extract key definitions and structure", web fetches get "extract relevant facts and data", API responses get "extract status and key fields".
- **Testing**:
  - Unit: Default config has `enabled: true` and `compression_mode: CompressLarge { threshold_tokens: 500 }`
  - Unit: `CompressLarge` with 600-token output → compressed
  - Unit: `CompressLarge` with 400-token output → passed through
  - Unit: `CompressAll` mode → all outputs compressed regardless of size
  - Unit: `SecurityOnly` mode → only configured content_types compressed
  - Unit: `CompressionStats` has correct `compression_ratio` (compressed/original)
  - Unit: `bits_per_token` is positive and finite
  - Unit: Extraction prompt varies by tool type (file_read vs web_fetch vs api_call)
  - Integration: Large tool output (5000 tokens) → compressed to < 2000 tokens
  - Adversarial: Compression produces output LARGER than input → return original (skip compression)
  - Adversarial: Empty tool output → passed through, no compression attempted

---

### Task 18.3 — ghost-agent-loop: L7 Memory Compressor
- **Audit**: Item 2.3 (L7 Memory Compression) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/memory_compressor.rs` (NEW), `src/context/mod.rs` (modify)
- **Context needed**: Existing L7 gets `Budget::Fixed(4000)` tokens for MEMORY.md + daily logs. Existing `ConvergenceAwareFilter` in `cortex-convergence` filters memories by TYPE (removes emotional/attachment at higher convergence scores). But filtered memories are still raw text — no compression. The blueprint calls for a post-filter compression step: filtered memories → compressor → condensed summary (500-1000 tokens instead of 4000).
- **What to build**:
  - `MemoryCompressor` struct:
    - `compressor: Arc<ContentQuarantine>` — reuses the quarantine/compressor from ghost-llm
    - `target_tokens: usize` — target compressed size (default 1000)
    - `enabled: bool` (default false — opt-in, requires local model or API budget)
  - `MemoryCompressorConfig`:
    - `enabled: bool` (default false)
    - `target_tokens: usize` (default 1000)
    - `compression_prompt: String` — default: `"Summarize these memory entries into a concise context block. Preserve: active goals, recent decisions, key facts, unresolved items. Remove: redundant entries, old completed tasks, verbose descriptions."`
  - `compress_memories(filtered_memories: &str) -> Result<String, LLMError>`:
    - If disabled or input is already below `target_tokens` → return input unchanged
    - Send to `ContentQuarantine::quarantine_content()` with compression prompt
    - Return compressed summary
    - On error → fall back to raw filtered memories with `tracing::warn!`
  - Integration point: called AFTER `ConvergenceAwareFilter::filter()` and BEFORE L7 assembly in `PromptCompiler`
- **Conventions**: Memory compression is opt-in because it requires either a local model (free) or API calls (costs tokens to save tokens — only worth it for large memory sets). The compression prompt is configurable via ghost.yml. Compression is idempotent — compressing already-compressed text produces similar output.
- **Testing**:
  - Unit: Disabled config → input returned unchanged
  - Unit: Input below target_tokens → returned unchanged (no compression needed)
  - Unit: Input above target_tokens → compressed output is shorter
  - Unit: Compression preserves key information (goals, decisions) — verify via substring check
  - Unit: Compressor error → fallback to raw input
  - Unit: Config parses from ghost.yml correctly
  - Proptest: For 500 random memory strings above target, compressed output is always shorter
  - Adversarial: Empty memory string → returned unchanged
  - Adversarial: Memory string that's exactly target_tokens → returned unchanged

---

### Task 18.4 — ghost-agent-loop: Integrate Compressor Pipeline into PromptCompiler
- **Audit**: Items 2.1, 2.3 (Full pipeline integration) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/prompt_compiler.rs` (modify)
- **Context needed**: After Tasks 17.3 (observation masking) and 18.3 (memory compressor), the PromptCompiler needs to orchestrate the full pipeline: L7 memory compression → L8 observation masking → spotlighting → budget allocation → truncation.
- **What to build**:
  - Add `memory_compressor: Option<MemoryCompressor>` field to `PromptCompiler`
  - New constructor: `PromptCompiler::full(context_window, spotlighting_config, masker_config, compressor_config)` — all optimizations enabled
  - Update `compile()` pipeline order:
    1. L7: Apply `memory_compressor.compress_memories()` to `input.memory_logs`
    2. L8: Apply `observation_masker.mask_history()` to `input.conversation_history`
    3. All layers: Apply spotlighting to configured layers
    4. Budget allocation
    5. Truncation (should rarely trigger now — compression reduces content significantly)
  - Add `CompilationStats` struct returned alongside layers:
    - `l7_original_tokens: usize`, `l7_compressed_tokens: usize`
    - `l8_original_tokens: usize`, `l8_masked_tokens: usize`
    - `total_original_tokens: usize`, `total_optimized_tokens: usize`
    - `compression_ratio: f64`
    - `cache_hit: bool` (from StablePrefixCache)
  - `compile()` returns `(Vec<PromptLayer>, CompilationStats)` instead of just `Vec<PromptLayer>`
- **Conventions**: Each optimization is independently toggleable. The `full()` constructor enables everything. Stats are always computed (even when optimizations are disabled) for monitoring. Tracing emits stats at `info` level every turn.
- **Testing**:
  - Unit: `full()` constructor enables all optimizations
  - Unit: Pipeline order: compression before masking before spotlighting
  - Unit: `CompilationStats` has correct token counts
  - Unit: `compression_ratio` < 1.0 when optimizations active
  - Unit: Each optimization can be independently disabled
  - Integration: Full pipeline with all optimizations → significant token reduction
  - Integration: Full pipeline with all optimizations disabled → identical to `PromptCompiler::new()`
  - Adversarial: All optimizations fail → graceful fallback to unoptimized compile



---

## Phase 19: Cron-Based Signal Scheduling + Periodic Tasks (Weeks 4–5)

> Deliverable: 5-tier signal computation schedule replacing compute-on-every-event.
> Centralized periodic task scheduler for background maintenance (DNS re-resolution,
> token expiry, trust persistence, state file writes). ~80% reduction in signal compute
> overhead. ~65% reduction in redundant background work.

---

### Task 19.1 — convergence-monitor: Signal Frequency Tier Assignment
- **Audit**: Item 4.1 (Signal Computation Frequency Tiers) | **Layer**: Sidecar
- **Crate**: `crates/convergence-monitor/` (MODIFY existing)
- **Files**: `src/pipeline/signal_scheduler.rs` (NEW), `src/pipeline/mod.rs` (modify)
- **Context needed**: Existing `SignalComputer` in `src/pipeline/signal_computer.rs` has dirty-flag per signal per agent but treats all signals equally — every dirty signal is recomputed on every event. The blueprint defines 5 frequency tiers. Existing 8 signals: S1 (session_duration), S2 (inter_session_gap), S3 (response_latency), S4 (vocabulary_convergence), S5 (goal_boundary_erosion), S6 (initiative_balance), S7 (disengagement_resistance), S8 (behavioral_anomaly).
- **What to build**:
  - `SignalFrequencyTier` enum:
    - `EveryMessage` — S3 (response latency), S6 (initiative balance)
    - `Every5thMessage` — S5 (goal boundary erosion), S8 (behavioral anomaly)
    - `SessionBoundary` — S1, S2, S4, S7, full composite, baseline update, de-escalation
    - `Every5Minutes` — identity drift, DNS re-resolution, OAuth token expiry, AgentCard cache TTL
    - `Every15Minutes` — memory compaction eligibility, convergence state file write, ITP batch flush
  - `SignalScheduler` struct:
    - `tier_assignment: [SignalFrequencyTier; 8]` — maps signal index to tier
    - `message_counter: BTreeMap<Uuid, u64>` — per-agent message count
    - `last_computed: BTreeMap<(Uuid, usize), Instant>` — per-agent, per-signal last computation time
    - `stale_after: [Duration; 8]` — per-signal staleness threshold
  - `should_compute(agent_id: Uuid, signal_index: usize, trigger: &ComputeTrigger) -> bool`:
    - `ComputeTrigger` enum: `MessageReceived`, `SessionBoundary`, `Timer5Min`, `Timer15Min`
    - Returns true if the signal's tier matches the trigger AND the signal is dirty
  - `record_message(agent_id: Uuid)` — increment message counter, mark tier-appropriate signals dirty
  - Default `stale_after` values: EveryMessage → 0s, Every5thMessage → 30s, SessionBoundary → 5min, Every5Minutes → 5min, Every15Minutes → 15min
- **Conventions**: SignalScheduler wraps SignalComputer — it decides WHEN to compute, SignalComputer decides WHAT to compute. The scheduler is deterministic: same sequence of triggers → same computation schedule. Message counter resets at session boundary.
- **Testing**:
  - Unit: S3 (EveryMessage) → `should_compute` true on every `MessageReceived`
  - Unit: S5 (Every5thMessage) → `should_compute` true on 5th, 10th, 15th message
  - Unit: S5 → `should_compute` false on 1st, 2nd, 3rd, 4th message
  - Unit: S1 (SessionBoundary) → `should_compute` true only on `SessionBoundary` trigger
  - Unit: S1 → `should_compute` false on `MessageReceived`
  - Unit: `record_message()` increments counter correctly
  - Unit: Message counter resets at session boundary
  - Unit: Default tier assignments match blueprint (S3→EveryMessage, S5→Every5thMessage, etc.)
  - Proptest: For 500 random trigger sequences, EveryMessage signals compute on every message trigger
  - Proptest: For 500 random trigger sequences, Every5thMessage signals compute exactly on multiples of 5
  - Adversarial: Unknown signal index (>7) → ignored, no panic
  - Adversarial: Very high message count (u64::MAX - 1) → no overflow

---

### Task 19.2 — convergence-monitor: Integrate SignalScheduler into Monitor Event Loop
- **Audit**: Item 4.1 (Monitor integration) | **Layer**: Sidecar
- **Crate**: `crates/convergence-monitor/` (MODIFY existing)
- **Files**: `src/monitor.rs` (modify), `src/pipeline/signal_computer.rs` (modify)
- **Context needed**: Existing `ConvergenceMonitor::run()` uses a `tokio::select!` loop with event ingestion from transport channels. `handle_event()` processes each event and triggers signal computation. Currently, ALL dirty signals are recomputed on every event. The `SignalScheduler` from Task 19.1 should gate which signals are computed per event.
- **What to build**:
  - Add `signal_scheduler: SignalScheduler` field to `ConvergenceMonitor`
  - Add interval timers to the `select!` loop:
    - `tokio::time::interval(Duration::from_secs(300))` — 5-minute timer
    - `tokio::time::interval(Duration::from_secs(900))` — 15-minute timer
  - On `MessageReceived` event:
    - `signal_scheduler.record_message(agent_id)`
    - Only compute signals where `should_compute(agent_id, i, ComputeTrigger::MessageReceived)` is true
  - On 5-minute timer tick:
    - For all active agents, compute signals where `should_compute(agent_id, i, ComputeTrigger::Timer5Min)` is true
  - On 15-minute timer tick:
    - For all active agents, compute signals where `should_compute(agent_id, i, ComputeTrigger::Timer15Min)` is true
  - On session boundary event:
    - Compute ALL signals for the agent (full recomputation)
    - Reset message counter
  - Modify `handle_event()` to use scheduler-gated computation instead of computing all dirty signals
  - Add `tracing::debug!` logging: which signals were computed, which were skipped, trigger type
- **Conventions**: Timer ticks are independent of event ingestion — they fire even if no events are received. Kill switch stops all timers. The 5-minute and 15-minute timers are staggered (15-min timer offset by 2.5 minutes to avoid thundering herd).
- **Testing**:
  - Unit: MessageReceived → only EveryMessage signals computed
  - Unit: 5th MessageReceived → EveryMessage + Every5thMessage signals computed
  - Unit: Timer5Min → Every5Minutes signals computed
  - Unit: Timer15Min → Every15Minutes signals computed
  - Unit: SessionBoundary → all 8 signals computed
  - Unit: Kill switch active → no signals computed, timers stopped
  - Integration: Simulate 10 messages → S3/S6 computed 10 times, S5/S8 computed 2 times, S1/S2/S4/S7 computed 0 times
  - Adversarial: Timer fires with no active agents → no-op, no panic

---

### Task 19.3 — ghost-gateway: Centralized Periodic Task Scheduler
- **Audit**: Item 4.2 (Background Periodic Tasks) | **Layer**: 5
- **Crate**: `crates/ghost-gateway/` (MODIFY existing)
- **Files**: `src/periodic.rs` (NEW), `src/lib.rs` (modify), `src/bootstrap.rs` (modify)
- **Context needed**: Multiple components need periodic background work but have no centralized scheduler: Vault token renewal (ghost-secrets), convergence state file write (convergence-monitor), OAuth token expiry check (ghost-oauth), AgentCard cache TTL (ghost-mesh), egress DNS re-resolution (ghost-egress), trust score persistence (ghost-mesh), hash chain Merkle anchoring (cortex-temporal). The gateway bootstrap is the natural orchestration point.
- **What to build**:
  - `PeriodicTaskScheduler` struct:
    - `tasks: Vec<PeriodicTask>`
    - `kill_switch: Arc<AtomicBool>` — stops all tasks on KILL_ALL
    - `health: BTreeMap<String, TaskHealth>` — per-task health status
  - `PeriodicTask` struct:
    - `name: String`
    - `interval: Duration`
    - `task_fn: Box<dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>> + Send + Sync>`
    - `last_run: Option<Instant>`
    - `consecutive_failures: u32`
    - `max_failures: u32` (default 3 — disable task after 3 consecutive failures)
  - `TaskHealth` struct: `last_success: Option<Instant>`, `last_failure: Option<Instant>`, `consecutive_failures: u32`, `total_runs: u64`, `status: TaskStatus`
  - `TaskStatus` enum: `Healthy`, `Degraded`, `Disabled`
  - `PeriodicTaskScheduler::run(self) -> JoinHandle<()>`:
    - Spawns a tokio task that loops with `tokio::time::sleep(1s)` granularity
    - Each iteration: check each task's interval, run if due, update health
    - Respects kill switch
  - `register(task: PeriodicTask)` — add a task
  - `health_report() -> BTreeMap<String, TaskHealth>` — for health endpoint
  - Pre-registered tasks (wired in bootstrap):
    - "vault-token-renewal" — every 1 hour (if Vault provider configured)
    - "oauth-token-expiry-check" — every 5 minutes
    - "convergence-state-write" — every 15 minutes
    - "egress-dns-reresolution" — every 5 minutes
    - "trust-score-persistence" — every 1 hour
    - "cache-cleanup" — every 1 hour (tool output cache, AgentCard cache)
- **Conventions**: Each task runs in its own `tokio::spawn` (failure in one doesn't block others). Task functions are `async` and return `Result`. Failed tasks are retried on next interval. After `max_failures` consecutive failures, task is disabled with `tracing::error!`. Health report exposed via existing `/api/health` endpoint.
- **Testing**:
  - Unit: Task fires after interval elapses
  - Unit: Task does NOT fire before interval
  - Unit: Kill switch stops all tasks
  - Unit: Task failure increments `consecutive_failures`
  - Unit: Task success resets `consecutive_failures`
  - Unit: Task disabled after `max_failures` consecutive failures
  - Unit: `health_report()` returns correct status per task
  - Unit: Disabled task is not executed
  - Integration: Register 3 tasks with different intervals → each fires at correct frequency
  - Adversarial: Task panics → caught, marked as failure, other tasks unaffected
  - Adversarial: All tasks fail → all disabled, scheduler continues running (waiting for manual intervention)



---

## Phase 20: Context Compaction Strategy (Weeks 5–6)

> Deliverable: Progressive context compaction triggered at 60%/80%/95% thresholds.
> Within-session turn summarization with "keep last 3 turns verbatim" policy.
> Running objectives injection at end of context (high attention zone).
> Tiered heartbeat system with zero-token binary pings. 95% reduction in heartbeat
> token cost. Context window utilization stays in the optimal 40-60% range.

---

### Task 20.1 — ghost-agent-loop: Context Usage Tracker with Progressive Thresholds
- **Audit**: Item 5.1 (Context Window Usage Tracking) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/usage_tracker.rs` (NEW), `src/context/mod.rs` (modify)
- **Context needed**: Existing `TokenBudgetAllocator` distributes budget across 10 layers. Existing `SessionCompactor` triggers at 70% (`CompactionConfig.trigger_threshold`). But there's no cumulative tracking across turns — the system only reacts when the context window is full. The blueprint says: LLM performance drops sharply after 60-70% of context window. Need progressive compaction: gentle at 60%, aggressive at 80%, emergency at 95%.
- **What to build**:
  - `ContextUsageTracker` struct:
    - `context_window: usize` — total context window size in tokens
    - `history: Vec<TurnUsage>` — per-turn token usage history
    - `thresholds: CompactionThresholds`
  - `TurnUsage` struct: `turn_number: u32`, `total_tokens: usize`, `fill_percentage: f64`, `timestamp: DateTime<Utc>`
  - `CompactionThresholds`:
    - `gentle: f64` (default 0.60) — trigger observation masking + memory compression
    - `aggressive: f64` (default 0.80) — trigger turn summarization
    - `emergency: f64` (default 0.95) — trigger emergency truncation (existing behavior)
  - `record_turn(total_tokens: usize) -> CompactionAction`:
    - Computes `fill_percentage = total_tokens / context_window`
    - Returns `CompactionAction` enum:
      - `None` — below gentle threshold
      - `Gentle` — between gentle and aggressive (enable compression, increase masking)
      - `Aggressive` — between aggressive and emergency (summarize old turns)
      - `Emergency` — above emergency (truncate immediately)
  - `trend() -> UsageTrend` — analyzes last 5 turns: `Stable`, `Rising`, `Falling`
  - `projected_turns_remaining() -> Option<u32>` — linear extrapolation of when emergency threshold will be hit
- **Conventions**: Tracker is per-session. Reset at session boundary. `CompactionAction` is advisory — the caller decides what to do. Trend analysis uses simple linear regression on last 5 data points.
- **Testing**:
  - Unit: 50% fill → `CompactionAction::None`
  - Unit: 65% fill → `CompactionAction::Gentle`
  - Unit: 85% fill → `CompactionAction::Aggressive`
  - Unit: 96% fill → `CompactionAction::Emergency`
  - Unit: Exactly at threshold → triggers (inclusive)
  - Unit: `trend()` with increasing usage → `Rising`
  - Unit: `trend()` with decreasing usage → `Falling`
  - Unit: `trend()` with stable usage → `Stable`
  - Unit: `projected_turns_remaining()` with linear growth → correct estimate
  - Unit: `projected_turns_remaining()` with stable usage → `None` (won't hit emergency)
  - Proptest: For 500 random fill percentages, correct CompactionAction returned
  - Adversarial: context_window = 0 → no panic (return Emergency)
  - Adversarial: total_tokens > context_window → Emergency

---

### Task 20.2 — ghost-gateway: Within-Session Turn Summarization
- **Audit**: Item 5.2 (Conversation Compaction) | **Layer**: 5
- **Crate**: `crates/ghost-gateway/` (MODIFY existing)
- **Files**: `src/session/compaction.rs` (modify)
- **Context needed**: Existing `SessionCompactor` has `compact()` which replaces entire history with a `CompactionBlock`. This is too aggressive — it loses all context. The blueprint calls for selective summarization: summarize turns 1-N into a structured state block, keep last 3 turns verbatim. The existing `CompactionBlock` struct is the right container for the summary.
- **What to build**:
  - `TurnSummarizer` struct:
    - `compressor: Arc<ContentQuarantine>` — reuses compressor for summarization
    - `keep_recent: usize` — number of recent turns to keep verbatim (default 3)
    - `summary_target_tokens: usize` — target size for summary block (default 2000)
  - `summarize_history(history: &[String], keep_recent: usize) -> Result<SummarizedHistory, LLMError>`:
    - Split history into `old_turns` (turns 0..N-keep_recent) and `recent_turns` (last keep_recent)
    - Send `old_turns` to compressor with prompt: `"Summarize this conversation history into a structured state block. Preserve: decisions made, files modified, errors encountered, current task state, unresolved questions. Format as bullet points."`
    - Return `SummarizedHistory { summary_block: CompactionBlock, recent_turns: Vec<String> }`
  - `SummarizedHistory` struct: `summary_block: CompactionBlock`, `recent_turns: Vec<String>`
  - Integration with `ContextUsageTracker`:
    - On `CompactionAction::Aggressive` → trigger `summarize_history()`
    - On `CompactionAction::Gentle` → only increase observation masking aggressiveness (reduce recency_window from 3 to 1)
  - Modify `SessionCompactor::compact()` to use `TurnSummarizer` instead of wholesale replacement
- **Conventions**: Summary block is a first-class message type (existing `CompactionBlock`). Summary blocks are NEVER re-summarized (existing invariant preserved). Recent turns include all message types (user, assistant, tool_result). The summary prompt is configurable via ghost.yml.
- **Testing**:
  - Unit: 10 turns, keep_recent=3 → summary of turns 0-6, turns 7-9 verbatim
  - Unit: 3 turns, keep_recent=3 → no summarization needed (all recent)
  - Unit: 1 turn, keep_recent=3 → no summarization needed
  - Unit: Summary block token count < summary_target_tokens
  - Unit: Recent turns are unmodified (exact match)
  - Unit: CompactionBlock from summarization has correct metadata
  - Unit: Existing CompactionBlocks in history are NOT re-summarized
  - Integration: Aggressive compaction → history replaced with summary + recent turns
  - Adversarial: Compressor fails → fall back to existing truncation behavior
  - Adversarial: All turns are CompactionBlocks → no summarization (nothing to summarize)

---

### Task 20.3 — ghost-agent-loop: Running Objectives Injection
- **Audit**: Item 5.2 (Running objectives at end of context) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/objectives.rs` (NEW), `src/context/prompt_compiler.rs` (modify), `src/context/mod.rs` (modify)
- **Context needed**: LLMs have highest attention at the beginning and end of context (primacy/recency effect). L9 (user message) is at the end — good. But there's no mechanism to recite current objectives near the end of context. The blueprint says: maintain a running objectives summary and inject it just before L9. This is the "todo.md trick" from Manus — keeps the agent focused on what it's supposed to be doing.
- **What to build**:
  - `ObjectivesTracker` struct:
    - `current_objectives: Vec<String>` — active objectives extracted from goal proposals
    - `recent_decisions: Vec<String>` — last 3 decisions made (from proposal outcomes)
    - `blockers: Vec<String>` — unresolved questions or errors
  - `ObjectivesTracker::compile_summary() -> String`:
    - Format: `"CURRENT STATE:\n- Objectives: {}\n- Recent decisions: {}\n- Blockers: {}"`
    - Target: < 200 tokens
  - `ObjectivesTracker::update_from_session(session_context: &SessionContext)`:
    - Extract active goals from session context
    - Extract recent proposal outcomes
    - Extract unresolved errors/questions from last 3 turns
  - Integration with `PromptCompiler`:
    - New optional field: `objectives_tracker: Option<ObjectivesTracker>`
    - In `compile()`, inject objectives summary between L8 and L9:
      - Append to end of L8 content: `"\n\n--- OBJECTIVES RECAP ---\n{summary}"`
    - This places objectives in the high-attention zone just before the user message
- **Conventions**: Objectives summary is always < 200 tokens (hard cap — truncate if needed). Updated once per turn (not per message). Objectives are extracted from existing data structures (no new storage). The recap section is clearly delimited so the LLM doesn't confuse it with conversation history.
- **Testing**:
  - Unit: `compile_summary()` with 3 objectives → formatted string < 200 tokens
  - Unit: `compile_summary()` with 0 objectives → minimal output ("No active objectives")
  - Unit: `compile_summary()` with very long objectives → truncated to 200 tokens
  - Unit: Objectives appear at end of L8 (before L9)
  - Unit: Objectives tracker disabled → L8 unchanged
  - Unit: `update_from_session()` extracts goals correctly
  - Adversarial: Session with no goals, no decisions, no blockers → minimal valid output
  - Adversarial: 100 objectives → truncated, most recent prioritized

---

### Task 20.4 — ghost-heartbeat: Tiered Heartbeat System
- **Audit**: Items 1.1, 1.2 (Tiered Heartbeat, Frequency Hysteresis) | **Layer**: 4
- **Crate**: `crates/ghost-heartbeat/` (MODIFY existing)
- **Files**: `src/heartbeat.rs` (modify), `src/tiers.rs` (NEW)
- **Context needed**: Existing `HeartbeatEngine` is single-tier — every heartbeat invokes the full LLM agent loop (~2000 tokens). The blueprint calls for 4 tiers where Tier 0/1 are zero-token. Current `interval_for_level()` SLOWS DOWN at higher convergence levels (30m→60m→120m→disabled) — the blueprint says it should SPEED UP (more monitoring when things are going wrong). Current granularity is minutes; blueprint uses seconds.
- **What to build**:
  - `HeartbeatTier` enum:
    - `Tier0` — Binary ping (16 bytes, zero tokens). UDP/unix-socket to convergence monitor. Just "I'm alive."
    - `Tier1` — Delta-encoded state (~20 bytes, zero tokens). Only changed fields since last beat.
    - `Tier2` — Full state snapshot (minimal tokens). Convergence score, active goals count, session duration.
    - `Tier3` — Full LLM invocation (existing behavior). Max 5% of beats.
  - `HeartbeatDelta` struct:
    - `agent_id: Uuid`
    - `seq: u64` — monotonic sequence number
    - `convergence_score: Option<f64>` — only if changed since last beat
    - `active_goals: Option<u32>` — only if changed
    - `session_duration_minutes: Option<u32>` — only if changed
    - `error_count: Option<u32>` — only if changed
  - `TierSelector` struct:
    - `select_tier(score_delta: f64, consecutive_stable: u32, convergence_level: u8) -> HeartbeatTier`:
      - `score_delta < 0.01` for 3+ consecutive beats → Tier0 (stable, just ping)
      - `score_delta < 0.05` → Tier1 (minor changes, delta only)
      - `score_delta >= 0.05` OR `convergence_level >= 2` → Tier2 (notable change, full snapshot)
      - `convergence_level >= 3` AND `score_delta >= 0.1` → Tier3 (escalation, invoke LLM)
    - Enforce: max 5% of beats are Tier3 (track ratio, downgrade to Tier2 if exceeded)
  - Replace `interval_for_level()` with `interval_for_state()`:
    - Stable (score_delta < 0.01 for 3 beats) → 120s
    - Active (score moving) → 30s
    - Escalated (level >= 2) → 15s
    - Critical (level >= 4) → 5s (Tier0 binary only — NOT disabled)
  - `HeartbeatEngine` modifications:
    - Add `last_score: Option<f64>` for delta computation
    - Add `consecutive_stable: u32` for hysteresis
    - Add `tier3_count: u64` and `total_count: u64` for 5% enforcement
    - `should_fire()` uses new `interval_for_state()` instead of `interval_for_level()`
    - `fire()` returns `HeartbeatTier` indicating what type of beat to execute
- **Conventions**: Tier0/Tier1 bypass the agent loop entirely — they go directly to the convergence monitor via the existing transport (unix socket or HTTP). Only Tier2/Tier3 enter the agent loop. The 5% Tier3 cap is a hard limit, not advisory. Hysteresis prevents rapid tier oscillation.
- **Testing**:
  - Unit: Stable state (delta < 0.01, 3 consecutive) → Tier0, 120s interval
  - Unit: Active state (delta 0.03) → Tier1, 30s interval
  - Unit: Escalated state (level 2) → Tier2, 15s interval
  - Unit: Critical state (level 4) → Tier0 at 5s (NOT disabled — this is the key fix)
  - Unit: Tier3 cap: after 5% of beats are Tier3, next escalation → Tier2 instead
  - Unit: `HeartbeatDelta` with no changes → all fields None (minimal payload)
  - Unit: `HeartbeatDelta` with score change → only `convergence_score` is Some
  - Unit: Hysteresis: 2 stable beats then 1 active → still Active (need 3 consecutive)
  - Unit: Hysteresis: 3 stable beats → transitions to Stable
  - Proptest: For 500 random (score_delta, consecutive_stable, level) tuples, tier is always valid
  - Proptest: For 500 random beat sequences, Tier3 ratio never exceeds 5%
  - Adversarial: score_delta = NaN → treated as 0.0 (stable)
  - Adversarial: score_delta = f64::MAX → Tier3 (capped by 5% rule)



---

## Phase 21: Skill Evolution + Information Efficiency (Weeks 7–9)

> Deliverable: Automatic skill creation from successful tool call sequences. Workflow
> recording and replay. Skill matching for incoming requests. Memory deduplication on write.
> 67% token savings per repeated task (compounding over time).

---

### Task 21.1 — ghost-skills: Workflow Recorder
- **Audit**: Item 8.1 (Skill Persistence from Successful Workflows) | **Layer**: 4
- **Crate**: `crates/ghost-skills/` (MODIFY existing)
- **Files**: `src/recorder.rs` (NEW), `src/lib.rs` (modify)
- **Context needed**: Existing `SkillRegistry` loads pre-authored skills from YAML manifests. Existing `ToolExecutor` in ghost-agent-loop executes tool calls. Existing `LLMToolCall` struct in ghost-llm has `id`, `name`, `arguments`. The blueprint says: after a successful multi-tool-call sequence, record it for potential skill creation. A "successful sequence" is defined as: all tool calls succeeded, no policy violations, user didn't intervene to correct, and the final outcome was accepted.
- **What to build**:
  - `WorkflowRecorder` struct:
    - `active_recordings: BTreeMap<Uuid, WorkflowRecording>` — per-session active recordings
    - `completed_recordings: Vec<CompletedWorkflow>` — successful recordings pending skill proposal
  - `WorkflowRecording` struct:
    - `session_id: Uuid`
    - `started_at: DateTime<Utc>`
    - `trigger_message: String` — the user message that initiated the workflow
    - `steps: Vec<WorkflowStep>`
    - `status: RecordingStatus` (Active, Completed, Abandoned)
  - `WorkflowStep` struct:
    - `tool_name: String`
    - `arguments_template: serde_json::Value` — arguments with concrete values replaced by placeholders
    - `output_summary: String` — compressed summary of tool output (not full output)
    - `duration_ms: u64`
    - `succeeded: bool`
  - `CompletedWorkflow` struct:
    - `recording: WorkflowRecording`
    - `total_tokens_used: usize`
    - `similarity_hash: [u8; 32]` — blake3 hash of tool sequence pattern (names + argument shapes)
  - `WorkflowRecorder::start_recording(session_id: Uuid, trigger: &str)` — begin recording
  - `WorkflowRecorder::record_step(session_id: Uuid, step: WorkflowStep)` — add step
  - `WorkflowRecorder::complete(session_id: Uuid) -> Option<CompletedWorkflow>` — finalize
  - `WorkflowRecorder::abandon(session_id: Uuid)` — discard (user intervened, policy violation, etc.)
  - Argument templating: replace concrete file paths with `{file_path}`, URLs with `{url}`, etc. using heuristic pattern matching
- **Conventions**: Recording is passive — it observes tool calls, doesn't modify them. Abandoned recordings are discarded (not stored). Completed recordings are held in memory until skill proposal (Task 21.2). The `similarity_hash` enables deduplication: if two workflows have the same tool sequence pattern, they're considered the same skill.
- **Testing**:
  - Unit: `start_recording()` creates active recording
  - Unit: `record_step()` adds step to active recording
  - Unit: `complete()` returns CompletedWorkflow with correct metadata
  - Unit: `abandon()` removes recording, returns None on complete
  - Unit: Argument templating: `/home/user/project/src/main.rs` → `{file_path}`
  - Unit: Argument templating: `https://api.example.com/v1/data` → `{url}`
  - Unit: `similarity_hash` is identical for same tool sequence with different concrete arguments
  - Unit: `similarity_hash` differs for different tool sequences
  - Proptest: For 500 random tool sequences, similarity_hash is deterministic
  - Adversarial: Recording with 0 steps → complete returns valid (empty) workflow
  - Adversarial: Recording with 100 steps → no memory issues
  - Adversarial: Concurrent recordings for different sessions → no cross-contamination

---

### Task 21.2 — ghost-skills: Skill Proposer
- **Audit**: Item 8.1 (Automatic skill creation) | **Layer**: 4
- **Crate**: `crates/ghost-skills/` (MODIFY existing)
- **Files**: `src/proposer.rs` (NEW), `src/lib.rs` (modify)
- **Context needed**: Existing `SkillManifest` has `name`, `version`, `description`, `capabilities`, `timeout_seconds`, `signature`. Existing proposal system in cortex-core (`Proposal`, `ProposalDecision`). `CompletedWorkflow` from Task 21.1 contains the recorded tool sequence. The blueprint says: after seeing the same workflow pattern 3+ times, propose creating a skill.
- **What to build**:
  - `SkillProposer` struct:
    - `pattern_counts: BTreeMap<[u8; 32], u32>` — similarity_hash → occurrence count
    - `proposal_threshold: u32` (default 3) — propose skill after N occurrences
    - `proposed_skills: BTreeSet<[u8; 32]>` — already-proposed patterns (don't re-propose)
  - `SkillProposer::observe(workflow: &CompletedWorkflow) -> Option<SkillProposal>`:
    - Increment pattern count for workflow's similarity_hash
    - If count >= threshold AND not already proposed → generate `SkillProposal`
    - Mark as proposed
  - `SkillProposal` struct:
    - `name: String` — auto-generated from tool sequence (e.g., "file-read-then-search-then-write")
    - `description: String` — auto-generated from trigger messages
    - `workflow: CompletedWorkflow`
    - `estimated_tokens_saved: usize` — `workflow.total_tokens_used * 0.67` (67% savings on replay)
    - `occurrences: u32`
  - `SkillProposer::approve(proposal: &SkillProposal) -> SkillManifest`:
    - Convert proposal to `SkillManifest` for registration in `SkillRegistry`
    - Serialize workflow steps as skill definition
  - `tokens_saved_by_skills: u64` — cumulative metric tracking total tokens saved by skill reuse
  - Integration: `SkillProposal` routes through existing proposal system (human approval required for new skills)
- **Conventions**: Skill proposals require human approval (same flow as goal proposals). Auto-generated names are kebab-case. The 67% savings estimate is conservative (actual savings depend on argument complexity). Proposed skills are not automatically registered — they go through the proposal pipeline.
- **Testing**:
  - Unit: First occurrence → no proposal (count = 1)
  - Unit: Second occurrence → no proposal (count = 2)
  - Unit: Third occurrence → proposal generated (count = 3, threshold met)
  - Unit: Fourth occurrence → no proposal (already proposed)
  - Unit: Different pattern → independent count
  - Unit: `approve()` produces valid SkillManifest
  - Unit: `estimated_tokens_saved` is 67% of workflow tokens
  - Unit: Auto-generated name is kebab-case
  - Proptest: For 500 random workflow sequences, proposal only at threshold
  - Adversarial: Pattern count overflow (u32::MAX) → saturates, no panic
  - Adversarial: Empty workflow → valid proposal (degenerate skill)

---

### Task 21.3 — ghost-agent-loop: Skill Matcher for Incoming Requests
- **Audit**: Item 8.1 (Skill reuse) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/tools/skill_matcher.rs` (NEW), `src/tools/mod.rs` (modify)
- **Context needed**: Existing `SkillRegistry` has `lookup()` by name. But there's no way to match an incoming user request to an existing skill by SIMILARITY. The blueprint says: when a new request comes in, check if a similar workflow has been recorded as a skill. If so, load the skill instead of re-discovering the tool chain.
- **What to build**:
  - `SkillMatcher` struct:
    - `registry: Arc<SkillRegistry>`
    - `similarity_threshold: f64` (default 0.7) — minimum similarity for a match
  - `SkillMatcher::find_match(user_message: &str) -> Option<SkillMatch>`:
    - Extract keywords from user message
    - Compare against skill descriptions and capabilities in registry
    - Use TF-IDF cosine similarity (same approach as S4 vocabulary_convergence signal)
    - Return best match above threshold
  - `SkillMatch` struct:
    - `skill: RegisteredSkill`
    - `similarity: f64`
    - `estimated_tokens_saved: usize`
  - `SkillMatcher::record_usage(skill_name: &str, tokens_saved: usize)`:
    - Track cumulative `tokens_saved_by_skills` metric
    - Emit `tracing::info!` with savings
  - Integration point: in the agent loop, before sending to LLM, check `SkillMatcher::find_match()`. If match found, inject skill context into prompt (L5 skill index) with higher priority.
- **Conventions**: Skill matching is advisory — the LLM still decides whether to use the skill. The matcher provides a hint, not a mandate. TF-IDF vectors are precomputed on skill registration and cached. Matching is fast (< 1ms for 100 skills).
- **Testing**:
  - Unit: Exact match on skill name → similarity 1.0
  - Unit: Similar request to skill description → similarity > threshold
  - Unit: Unrelated request → no match (below threshold)
  - Unit: `record_usage()` increments cumulative counter
  - Unit: Empty registry → no match
  - Unit: Multiple skills → best match returned
  - Proptest: For 500 random messages against 10 skills, similarity is always in [0.0, 1.0]
  - Adversarial: Very long user message (10KB) → matching completes in < 10ms
  - Adversarial: Skill with empty description → still matchable by name/capabilities

---

### Task 21.4 — cortex-storage: Memory Deduplication on Write
- **Audit**: Item 6.2 (Memory Deduplication) | **Layer**: 2
- **Crate**: `crates/cortex/cortex-storage/` (MODIFY existing)
- **Files**: `src/deduplication.rs` (NEW), `src/lib.rs` (modify)
- **Context needed**: Existing memory write path goes through the proposal system → cortex-validation → cortex-storage. No deduplication check on write — identical or near-identical memories can accumulate. The blueprint says: before writing to MEMORY.md, compute similarity against existing entries. If > 0.85 similarity, merge rather than append.
- **What to build**:
  - `MemoryDeduplicator` struct:
    - `similarity_threshold: f64` (default 0.85)
    - `max_comparisons: usize` (default 100) — only compare against most recent N entries
  - `MemoryDeduplicator::check_duplicate(new_entry: &str, existing_entries: &[String]) -> DeduplicationResult`:
    - Compute TF-IDF vectors for new entry and each existing entry
    - Cosine similarity between new entry and each existing
    - If any similarity > threshold → `Duplicate { index, similarity, merged }`
    - Otherwise → `Unique`
  - `DeduplicationResult` enum:
    - `Unique` — no duplicate found, proceed with write
    - `Duplicate { existing_index: usize, similarity: f64, merged_entry: String }` — merge with existing
  - Merge strategy:
    - Keep the longer entry as base
    - Append any unique information from the shorter entry
    - Update timestamp to most recent
    - Preserve higher importance score
  - Integration: called in the memory write path BEFORE cortex-validation (dedup first, then validate)
- **Conventions**: TF-IDF is computed on whitespace-tokenized words (simple, fast). No external embedding model required. The `max_comparisons` limit prevents O(N²) scaling on large memory stores. Deduplication is logged via tracing for auditability.
- **Testing**:
  - Unit: Identical entries → `Duplicate` with similarity 1.0
  - Unit: Completely different entries → `Unique`
  - Unit: Similar entries (same topic, different wording) → `Duplicate` if > 0.85
  - Unit: Merged entry contains information from both
  - Unit: Merged entry has most recent timestamp
  - Unit: Merged entry has higher importance
  - Unit: `max_comparisons` limits search to most recent N
  - Proptest: For 500 random entry pairs, similarity is in [0.0, 1.0]
  - Proptest: For 500 identical pairs, always `Duplicate`
  - Adversarial: Empty new entry → `Unique` (nothing to deduplicate)
  - Adversarial: Empty existing entries → `Unique`
  - Adversarial: Entry with only stop words → low similarity, likely `Unique`



---

## Phase 22: Mesh Efficiency + Hardening + Phase 15 Completion (Weeks 9–12)

> Deliverable: ghost-gateway ↔ ghost-mesh wiring (mesh_routes.rs, A2A endpoints).
> Delta-encoded task updates. AgentCard TTL caching. Capability bitfield for fast matching.
> Phase 15 completion: PWA support, post-v1 proptest strategies, cross-crate integration
> tests, 5 new docs, Criterion benchmarks. Information-theoretic exploration budget (stretch).

---

### Task 22.1 — ghost-gateway: Wire ghost-mesh into Gateway (A2A Endpoints)
- **Audit**: Items 10.1, 10.2, 10.3 (A2A Server, Gateway Routes, Discovery Registry) | **Layer**: 5
- **Crate**: `crates/ghost-gateway/` (MODIFY existing)
- **Files**: `Cargo.toml` (modify), `src/api/mesh_routes.rs` (NEW), `src/api/mod.rs` (modify), `src/bootstrap.rs` (modify), `src/config.rs` (modify)
- **Context needed**: Existing `A2ADispatcher` in `ghost-mesh/src/transport/a2a_server.rs` provides handler logic for `/.well-known/agent.json` and `POST /a2a` JSON-RPC dispatch. The `a2a_server.rs` module comment explicitly says: "The actual axum route registration happens in ghost-gateway (`src/api/mesh_routes.rs`)." But `mesh_routes.rs` doesn't exist and `ghost-mesh` is NOT in gateway's Cargo.toml dependencies. Existing gateway API modules follow the pattern: `pub mod {name};` in `api/mod.rs`, axum Router in the module.
- **What to build**:
  - Add `ghost-mesh = { workspace = true }` to ghost-gateway Cargo.toml `[dependencies]`
  - Add `pub mod mesh_routes;` to `src/api/mod.rs`
  - `mesh_routes.rs`:
    - `GET /.well-known/agent.json` → calls `A2ADispatcher::agent_card()`, returns JSON with `Content-Type: application/json`
    - `POST /a2a` → deserializes `MeshMessage` from body, calls `A2ADispatcher::dispatch()`, returns JSON-RPC response
    - `pub fn mesh_router(state: Arc<A2AServerState>) -> axum::Router`:
      - Creates `A2ADispatcher` from state
      - Registers both routes
    - Auth middleware: verify Ed25519 signature on incoming requests (extract from `X-Ghost-Signature` header, verify against known agent public keys)
  - `src/config.rs` additions:
    - `MeshConfig` struct: `enabled: bool` (default false), `known_agents: Vec<KnownAgent>`, `min_trust_for_delegation: f64` (default 0.3), `max_delegation_depth: u32` (default 3)
    - `KnownAgent` struct: `name: String`, `endpoint: String`, `public_key: String`
  - `src/bootstrap.rs` additions:
    - New bootstrap step: if `mesh.enabled`, construct `A2AServerState` from agent's signing key and config
    - Merge `mesh_router()` into the main axum Router
    - Initialize `AgentDiscovery` with known agents from config
  - ghost.yml additions:
    ```yaml
    mesh:
      enabled: false
      known_agents:
        - name: "helper"
          endpoint: "http://192.168.1.100:18789"
          public_key: "base64-encoded-ed25519-public-key"
      min_trust_for_delegation: 0.3
      max_delegation_depth: 3
    ```
- **Conventions**: Mesh is disabled by default (opt-in). When disabled, no mesh routes are registered. The `/.well-known/agent.json` path follows the A2A protocol standard. JSON-RPC 2.0 error responses follow the standard error code format (already implemented in `A2ADispatcher`).
- **Testing**:
  - Unit: `mesh_router()` creates valid axum Router
  - Unit: GET `/.well-known/agent.json` returns valid AgentCard JSON
  - Unit: POST `/a2a` with valid `tasks/send` → returns task JSON
  - Unit: POST `/a2a` with unknown method → JSON-RPC error response
  - Unit: POST `/a2a` without signature → 401 Unauthorized
  - Unit: POST `/a2a` with invalid signature → 401 Unauthorized
  - Unit: MeshConfig parses from ghost.yml correctly
  - Unit: Mesh disabled → no routes registered
  - Integration: Full request flow: discover agent card → submit task → get status
  - Adversarial: Malformed JSON body → 400 Bad Request
  - Adversarial: Very large request body (1MB) → rejected (size limit)

---

### Task 22.2 — ghost-mesh: Delta-Encoded Task Updates + AgentCard TTL Cache + Capability Bitfield
- **Audit**: Items 7.1, 7.3, 7.5 (AgentCard caching, delta encoding, capability bitfield) | **Layer**: 3
- **Crate**: `crates/ghost-mesh/` (MODIFY existing)
- **Files**: `src/types.rs` (modify), `src/discovery.rs` (modify), `src/transport/a2a_client.rs` (modify)
- **Context needed**: Existing `AgentCard` has `signed_at` and `signature` fields. Existing `AgentDiscoverable` trait defines `discover_agent()` and `get_known_agent()` but has no implementation. Existing `MeshTask` is always sent as full struct. No TTL on cached cards. No capability bitfield.
- **What to build**:
  - `AgentCardCache` struct:
    - `cards: BTreeMap<Uuid, CachedCard>`
    - `ttl: Duration` (default 1 hour)
  - `CachedCard` struct: `card: AgentCard`, `cached_at: Instant`, `last_signed_at: DateTime<Utc>`
  - `AgentCardCache::get(agent_id: &Uuid) -> Option<&AgentCard>`:
    - Return card if cached AND not expired (cached_at + ttl > now)
    - Return None if expired or not cached
  - `AgentCardCache::put(agent_id: Uuid, card: AgentCard)`:
    - Store with current timestamp
    - If card's `signed_at` matches existing cached card's `signed_at` → skip re-verification (signature-based invalidation)
  - `MeshTaskDelta` struct:
    - `task_id: Uuid`
    - `status: Option<TaskStatus>` — only if changed
    - `output: Option<serde_json::Value>` — only if changed
    - `updated_at: Option<DateTime<Utc>>` — only if changed
  - `MeshTask::compute_delta(&self, previous: &MeshTask) -> MeshTaskDelta`:
    - Compare fields, include only changed ones
  - `MeshTask::apply_delta(&mut self, delta: &MeshTaskDelta)`:
    - Merge delta into current state
  - `AgentCard` additions:
    - `capability_flags: u64` field
    - Bit mapping: bit 0 = code_execution, bit 1 = web_search, bit 2 = file_operations, bit 3 = api_calls, bit 4 = data_analysis, bit 5 = image_generation, bits 6-63 reserved
    - `capabilities_match(required: u64) -> bool`: `(self.capability_flags & required) == required`
    - `capabilities_from_strings(caps: &[String]) -> u64`: convert string capabilities to bitfield
  - Backward compatibility: `capabilities: Vec<String>` kept for human readability, `capability_flags` used for fast matching
- **Conventions**: TTL is configurable via ghost.yml mesh config. Delta encoding uses `Option` fields — `None` means "unchanged". Capability bitfield is computed from `capabilities` strings on card creation. Unknown capability strings map to bit 0 (no flag set — won't match specific requirements).
- **Testing**:
  - Unit: `AgentCardCache::get()` returns card within TTL
  - Unit: `AgentCardCache::get()` returns None after TTL expires
  - Unit: Same `signed_at` → skip re-verification
  - Unit: Different `signed_at` → re-verify signature
  - Unit: `compute_delta()` with no changes → all fields None
  - Unit: `compute_delta()` with status change → only status is Some
  - Unit: `apply_delta()` merges correctly
  - Unit: Round-trip: `apply_delta(compute_delta(a, b), a) == b`
  - Unit: `capabilities_match()` with exact match → true
  - Unit: `capabilities_match()` with subset → true
  - Unit: `capabilities_match()` with missing capability → false
  - Unit: `capabilities_from_strings(["code_execution", "web_search"])` → bits 0 and 1 set
  - Proptest: For 500 random MeshTask pairs, delta round-trip produces identical task
  - Proptest: For 500 random capability sets, bitfield round-trip preserves all capabilities
  - Adversarial: Delta with all fields Some → equivalent to full replacement
  - Adversarial: Unknown capability string → maps to 0 (no bits set)

---

### Task 22.3 — Phase 15 Completion: PWA, Test Fixtures, Integration Tests, Docs, Benchmarks
- **Audit**: Items 11.1-11.4 + Phase 15 tasks | **Layer**: Cross-cutting
- **Crates**: Multiple (see sub-items)
- **Context needed**: Phase 15 from post-v1 tasks.md (Tasks 15.1-15.6) was identified as incomplete. This task consolidates the remaining Phase 15 work.
- **What to build** (sub-items):

  **15.1 — PWA Support** (`dashboard/`):
  - `static/manifest.json`: name "GHOST Dashboard", icons, start_url "/", display "standalone"
  - `src/service-worker.ts`: cache shell (HTML/CSS/JS), network-first for API, offline fallback
  - `src/routes/+layout.svelte`: add `<link rel="manifest">`, install prompt, offline indicator
  - Gateway: `POST /api/push/subscribe`, `POST /api/push/unsubscribe` endpoints
  - VAPID key generation via ghost-secrets on first start

  **15.2 — Post-v1 Proptest Strategies** (`crates/cortex/test-fixtures/`):
  - Add dependencies: ghost-egress, ghost-oauth, ghost-mesh, ghost-agent-loop
  - New strategies: `egress_config_strategy()`, `domain_pattern_strategy()`, `oauth_ref_id_strategy()`, `token_set_strategy()`, `agent_card_strategy()`, `mesh_task_strategy()`, `interaction_outcome_strategy()`, `trust_matrix_strategy()`, `tool_call_plan_strategy()`, `spotlighting_config_strategy()`
  - Note: `signal_array_strategy()` already produces `[f64; 8]` ✅

  **15.3 — Cross-Crate Integration Tests** (`tests/integration/`):
  - `secrets_e2e.rs`: EnvProvider → AuthProfileManager → LLM call with credential
  - `egress_e2e.rs`: Allowlist → proxy → allowed call succeeds → blocked call denied → violation event
  - `oauth_e2e.rs`: Connect → callback → token stored → agent API call → disconnect → token deleted
  - `mesh_e2e.rs`: Discover agent → submit task → get status → cancel (requires Task 22.1 gateway wiring)

  **15.4 — Post-v1 Documentation** (`docs/`):
  - `secrets-management.md`: 3 providers, migration guide, security considerations
  - `network-egress.md`: Per-agent policy, domain allowlisting, platform backends, troubleshooting
  - `oauth-brokering.md`: Provider setup, connect/disconnect, agent tools, security model
  - `mesh-networking.md`: Discovery, delegation, trust scoring, A2A compatibility, safety
  - `prompt-injection-defense.md`: Spotlighting, plan-then-execute, quarantined LLM, behavioral anomaly
  - `architecture.md`: Update layer model for new crates

  **15.5 — Criterion Benchmarks** (various crates):
  - `crates/cortex/cortex-temporal/benches/temporal_bench.rs`: hash chain append, Merkle proof
  - `crates/cortex/cortex-decay/benches/decay_bench.rs`: decay formula computation
  - `crates/cortex/cortex-crdt/benches/crdt_bench.rs`: CRDT merge operations
  - Add `criterion = { workspace = true }` to each crate's `[dev-dependencies]`
  - Add `[[bench]]` sections to each crate's Cargo.toml
  - Root Cargo.toml: add `criterion = { version = "0.5", features = ["html_reports"] }` to workspace deps

- **Conventions**: Integration tests use `#[ignore]` for tests requiring external services. Docs follow existing style (see `docs/getting-started.md`). Benchmarks use Criterion's `black_box` for accurate measurement.
- **Testing**: Each sub-item has its own test criteria as defined in the post-v1 tasks.md (Tasks 15.1-15.6). Not repeated here to avoid duplication.

---

### Task 22.4 — Information-Theoretic Exploration Budget (Stretch Goal)
- **Audit**: Items 6.1, 6.3 (Bits-per-token, Exploration/Exploitation Ratio) | **Layer**: 4
- **Crate**: `crates/ghost-agent-loop/` (MODIFY existing)
- **Files**: `src/context/exploration_budget.rs` (NEW), `src/context/mod.rs` (modify)
- **Context needed**: Existing `CostCalculator` in ghost-llm tracks economic cost (dollars, token counts) but not information cost. The blueprint says: track `bits_per_token` for each tool call category, allocate 20% of token budget to exploration (gathering new information) and 80% to exploitation (acting on known information). This is a stretch goal — lower priority than Phases 16-21.
- **What to build**:
  - `ExplorationBudget` struct:
    - `exploration_ratio: f64` (default 0.20) — 20% of token budget for exploration
    - `exploitation_ratio: f64` (default 0.80) — 80% for exploitation
    - `per_category_density: BTreeMap<String, InformationDensity>` — per-tool-category tracking
  - `InformationDensity` struct:
    - `total_calls: u64`
    - `total_input_tokens: u64`
    - `total_output_tokens: u64`
    - `behavioral_change_score: f64` — how much agent behavior changed after this tool category's output
    - `bits_per_token: f64` — `behavioral_change_score / total_output_tokens` (approximate)
  - `ToolCallClassifier`:
    - `classify(tool_name: &str) -> ToolCallType` — `Exploration` or `Exploitation`
    - Heuristics: `file_read`, `web_search`, `api_call` → Exploration; `file_write`, `shell_execute`, `memory_write` → Exploitation
    - Configurable overrides via ghost.yml
  - `ExplorationBudget::should_allow(tool_type: ToolCallType, session_tokens_used: usize) -> bool`:
    - Track exploration vs exploitation token usage in current session
    - If exploration ratio exceeded → deny exploration calls (suggest exploitation instead)
    - If exploitation ratio exceeded → allow exploration (rebalance)
  - `ExplorationBudget::record(tool_name: &str, input_tokens: usize, output_tokens: usize, behavioral_change: f64)`:
    - Update per-category density metrics
  - Diminishing returns detection:
    - If last 5 exploration calls for a category have `bits_per_token` < threshold → suggest switching to exploitation
    - Emit `tracing::info!` with recommendation
- **Conventions**: This is advisory, not blocking — the agent can override the budget. The `behavioral_change_score` is approximated by measuring how much the agent's next response differs from what it would have been without the tool output (expensive to compute exactly — use heuristic: did the agent change its plan after seeing the output?). Stretch goal: implement fully only if Phases 16-21 are complete.
- **Testing**:
  - Unit: Default budget is 20/80 split
  - Unit: `classify("file_read")` → Exploration
  - Unit: `classify("file_write")` → Exploitation
  - Unit: `should_allow(Exploration)` when exploration budget exhausted → false
  - Unit: `should_allow(Exploitation)` when exploitation budget exhausted → true (allow rebalance)
  - Unit: `record()` updates per-category density
  - Unit: Diminishing returns: 5 low-density calls → recommendation emitted
  - Proptest: For 500 random tool sequences, exploration ratio stays within bounds
  - Adversarial: All calls are exploration → exploitation budget unused, exploration capped at 20%
  - Adversarial: Zero tokens used → no division by zero in bits_per_token



---

## Dependency Graph Summary

```
Phase 16 (KV Cache Optimization) — no dependencies, pure refactor
  └─ Task 16.1 StablePrefixCache (independent)
  └─ Task 16.2 Move tool filtering L3→L6 (independent)
  └─ Task 16.3 L4 timestamp sanitization (independent)
  └─ Task 16.4 Spotlighting template fix (independent)
  All 4 tasks are independent — can be parallelized.

Phase 17 (Observation Masking) — depends on Phase 16 (stable prefix must work first)
  └─ Task 17.1 ToolOutputCache (independent within phase)
  └─ Task 17.2 ObservationMasker (depends on 17.1)
  └─ Task 17.3 PromptCompiler integration (depends on 17.2)

Phase 18 (Compressor Pipeline) — depends on Phase 17 (masking before compression in pipeline)
  └─ Task 18.1 Local model tier (independent within phase)
  └─ Task 18.2 ContentQuarantine as compressor (depends on 18.1)
  └─ Task 18.3 L7 memory compressor (depends on 18.2)
  └─ Task 18.4 Full pipeline integration (depends on 17.3, 18.3)

Phase 19 (Cron Scheduler) — independent of Phases 17-18 (can parallelize)
  └─ Task 19.1 SignalScheduler (independent)
  └─ Task 19.2 Monitor integration (depends on 19.1)
  └─ Task 19.3 PeriodicTaskScheduler (independent of 19.1-19.2)

Phase 20 (Context Compaction) — depends on Phases 17-18 (uses compressor + masker)
  └─ Task 20.1 ContextUsageTracker (independent within phase)
  └─ Task 20.2 TurnSummarizer (depends on 18.2 for compressor)
  └─ Task 20.3 ObjectivesTracker (independent)
  └─ Task 20.4 Tiered heartbeat (independent of 20.1-20.3)

Phase 21 (Skill Evolution) — depends on Phase 18 (compressor for output summaries)
  └─ Task 21.1 WorkflowRecorder (independent within phase)
  └─ Task 21.2 SkillProposer (depends on 21.1)
  └─ Task 21.3 SkillMatcher (depends on 21.2)
  └─ Task 21.4 MemoryDeduplicator (independent of 21.1-21.3)

Phase 22 (Mesh + Hardening) — depends on all previous phases
  └─ Task 22.1 Gateway mesh wiring (independent within phase)
  └─ Task 22.2 Mesh efficiency (depends on 22.1 for testing)
  └─ Task 22.3 Phase 15 completion (depends on all new types from 16-21)
  └─ Task 22.4 Exploration budget (stretch, depends on 18.2 for density tracking)
```

## Parallelization Notes

The following can be worked on simultaneously:
- Phase 16 tasks (16.1, 16.2, 16.3, 16.4) are ALL independent of each other
- Phase 19 (Cron Scheduler) is independent of Phases 17-18 (Masking + Compression)
- Task 20.4 (Tiered Heartbeat) is independent of Tasks 20.1-20.3
- Task 21.4 (Memory Deduplication) is independent of Tasks 21.1-21.3
- Task 22.1 (Gateway Mesh Wiring) can start as soon as Phase 16 is done

Critical path: Phase 16 → Phase 17 → Phase 18 → Phase 20 → Phase 22

Estimated total: ~12 weeks with parallelization, ~18 weeks sequential.

---

## Token Savings Projection (from INFRASTRUCTURE_AUDIT.md)

| Phase | Technique | Current Tokens/Turn | After Optimization | Reduction |
|-------|-----------|--------------------|--------------------|-----------|
| 16 | KV cache stable prefix (L0-L6) | ~6,900 (recomputed) | ~690 (cached) | 90% |
| 17 | Observation masking (L8 old tool outputs) | ~8,000 (raw) | ~4,000 (references) | 50% |
| 18 | Compressor for tool results | ~50,000 (raw output) | ~2,000 (extraction) | 96% |
| 19 | Signal computation batching | 8 signals/event | 2-3 signals/event avg | 65% |
| 20 | Heartbeat frequency reduction | ~2,000/beat × 48/day | ~200/beat × 12/day | 95% |
| 21 | Skill reuse (compounding) | ~12,000 first time | ~4,000 subsequent | 67% |

For a typical 50-tool-call autonomous session:
- Current estimate: ~500K tokens
- After all optimizations: ~50-80K tokens
- Reduction factor: 6-10x

---

## Files Modified Summary

### New Files (17)
- `ghost-agent-loop/src/context/stable_prefix.rs`
- `ghost-agent-loop/src/context/tool_output_cache.rs`
- `ghost-agent-loop/src/context/observation_masker.rs`
- `ghost-agent-loop/src/context/memory_compressor.rs`
- `ghost-agent-loop/src/context/usage_tracker.rs`
- `ghost-agent-loop/src/context/objectives.rs`
- `ghost-agent-loop/src/context/exploration_budget.rs`
- `ghost-agent-loop/src/tools/skill_matcher.rs`
- `ghost-heartbeat/src/tiers.rs`
- `ghost-skills/src/recorder.rs`
- `ghost-skills/src/proposer.rs`
- `ghost-gateway/src/periodic.rs`
- `ghost-gateway/src/api/mesh_routes.rs`
- `convergence-monitor/src/pipeline/signal_scheduler.rs`
- `cortex-storage/src/deduplication.rs`
- `tests/integration/*.rs` (4 files)
- `docs/*.md` (5 files)

### Modified Files (18)
- `ghost-agent-loop/src/context/prompt_compiler.rs` (Phases 16, 17, 18, 20)
- `ghost-agent-loop/src/context/spotlighting.rs` (Phase 16)
- `ghost-agent-loop/src/context/mod.rs` (Phases 16-21)
- `ghost-agent-loop/src/tools/mod.rs` (Phase 21)
- `ghost-llm/src/quarantine.rs` (Phase 18)
- `ghost-llm/src/router.rs` (Phase 18)
- `ghost-heartbeat/src/heartbeat.rs` (Phase 20)
- `ghost-skills/src/lib.rs` (Phase 21)
- `ghost-gateway/src/session/compaction.rs` (Phase 20)
- `ghost-gateway/src/bootstrap.rs` (Phases 19, 22)
- `ghost-gateway/src/config.rs` (Phase 22)
- `ghost-gateway/src/api/mod.rs` (Phase 22)
- `ghost-gateway/src/lib.rs` (Phase 19)
- `ghost-gateway/Cargo.toml` (Phase 22)
- `convergence-monitor/src/monitor.rs` (Phase 19)
- `convergence-monitor/src/pipeline/mod.rs` (Phase 19)
- `convergence-monitor/src/pipeline/signal_computer.rs` (Phase 19)
- `cortex-storage/src/lib.rs` (Phase 21)
- `cortex/test-fixtures/src/strategies.rs` (Phase 22)
