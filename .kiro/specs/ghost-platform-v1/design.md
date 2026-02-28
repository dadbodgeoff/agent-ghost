# Design Document

## Overview

This design document specifies the architecture, crate structure, data models, API contracts, and integration patterns for the GHOST Platform v1. The platform is a Rust 2021 monorepo comprising ~17 new crates and ~15 modified crates organized in 4 dependency layers, plus a SvelteKit dashboard, browser extension, and deployment infrastructure.

The design maps directly to the 41 requirements in requirements.md. Each section references the requirement(s) it satisfies.

## Architecture Overview

### Process Topology

```
┌─────────────────────────────────────────────────────────────────┐
│                    GHOST GATEWAY PROCESS                         │
│                    (ghost-gateway binary)                        │
│                                                                  │
│  ┌─────────────┐ ┌──────────────┐ ┌──────────────────────────┐ │
│  │ AgentRunner  │ │ PolicyEngine │ │ SpendingCapEnforcer      │ │
│  │ (agent loop) │ │ (ghost-policy)│ │ (cost/spending_cap.rs)  │ │
│  └──────┬──────┘ └──────┬───────┘ └──────────┬───────────────┘ │
│         │               │                     │                  │
│  ┌──────▼───────────────▼─────────────────────▼──────────────┐  │
│  │              AutoTriggerEvaluator                           │  │
│  │              (safety/auto_triggers.rs)                      │  │
│  └──────────────────────┬────────────────────────────────────┘  │
│                          │                                       │
│  ┌──────────────────────▼────────────────────────────────────┐  │
│  │  KillSwitch + QuarantineManager (safety/)                  │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌────────────┐ ┌──────────────┐ ┌────────────┐ ┌───────────┐  │
│  │SessionMgr  │ │MessageRouter │ │LaneQueue   │ │API Server │  │
│  │CostTracker │ │AgentRegistry │ │Heartbeat   │ │WebSocket  │  │
│  └────────────┘ └──────────────┘ └────────────┘ └───────────┘  │
└─────────────────────────────────────────────────────────────────┘
        │ ITP events (unix socket)        │ HTTP /health
        ▼                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│              CONVERGENCE MONITOR SIDECAR                         │
│              (convergence-monitor binary)                        │
│                                                                  │
│  ┌──────────┐ ┌──────────────┐ ┌──────────────────────────────┐│
│  │ Pipeline  │ │ CompositeScorer│ │ InterventionTrigger        ││
│  │ (7 signals)│ │ (scoring)    │ │ (5-level state machine)    ││
│  └──────────┘ └──────────────┘ └──────────────────────────────┘│
│                                                                  │
│  Shared state: ~/.ghost/data/convergence_state/{agent}.json     │
│  Transport: unix socket, HTTP API, native messaging             │
└─────────────────────────────────────────────────────────────────┘
```

### Dependency Layers

```
Layer 0 (Leaf):     ghost-signing
Layer 1A (Infra):   cortex-core → cortex-storage → cortex-temporal → cortex-decay
                    → cortex-validation → cortex-convergence → cortex-crdt
Layer 1B (Proto):   itp-protocol
Layer 2 (Safety):   simulation-boundary, convergence-monitor, read-only-pipeline
Layer 3 (Platform): ghost-identity, ghost-policy, ghost-llm, ghost-skills,
                    ghost-channels, ghost-heartbeat, ghost-audit, ghost-backup,
                    ghost-export, ghost-proxy, ghost-migrate, ghost-agent-loop
Layer 4 (Gateway):  ghost-gateway (depends on all above)
```

Rule: No crate may depend on a crate in a higher layer. ghost-signing has zero ghost-*/cortex-* dependencies.

## Crate Designs

### 1. ghost-signing (Req 1)

Leaf crate. Zero ghost-*/cortex-* dependencies.

```rust
// ghost-signing/src/lib.rs
pub struct SigningKey(ed25519_dalek::SigningKey);  // Zeroize on Drop
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);
pub struct Signature(ed25519_dalek::Signature);

pub fn generate_keypair() -> (SigningKey, VerifyingKey);
pub fn sign(data: &[u8], key: &SigningKey) -> Signature;
pub fn verify(data: &[u8], sig: &Signature, key: &VerifyingKey) -> bool;
```

Dependencies: `ed25519-dalek`, `zeroize`, `rand`.

### 2. cortex-core Convergence Extensions (Req 2)

New types added to existing cortex-core crate.

```rust
// cortex-core/src/memory/types/convergence.rs
pub struct AgentGoalContent { pub goal_text: String, pub scope: GoalScope, pub origin: GoalOrigin, pub parent_goal_id: Option<Uuid> }
pub struct AgentReflectionContent { pub reflection_text: String, pub trigger: ReflectionTrigger, pub depth: u8, pub parent_reflection_id: Option<Uuid> }
pub struct ConvergenceEventContent { pub signal_id: u8, pub value: f64, pub window_level: SlidingWindowLevel, pub baseline_deviation: f64 }
pub struct BoundaryViolationContent { pub violation_type: ViolationType, pub matched_pattern: String, pub severity: f64, pub action_taken: BoundaryAction }
pub struct ProposalRecordContent { pub operation: ProposalOperation, pub decision: ProposalDecision, pub dimension_scores: BTreeMap<String, f64>, pub flags: Vec<String> }
pub struct SimulationResultContent { pub scenario: String, pub outcome: String, pub confidence: f64 }
pub struct InterventionPlanContent { pub level: u8, pub actions: Vec<String>, pub trigger_reason: String }
pub struct AttachmentIndicatorContent { pub indicator_type: AttachmentIndicatorType, pub intensity: f64, pub context: String }

pub enum ProposalOperation { GoalChange, ReflectionWrite, MemoryWrite, MemoryDelete }
pub enum ProposalDecision { AutoApproved, AutoRejected, HumanReviewRequired, ApprovedWithFlags, TimedOut, Superseded }
pub enum CallerType { Platform, Agent { agent_id: Uuid }, Human { user_id: String } }

// cortex-core/src/config/convergence_config.rs
pub struct ConvergenceConfig {
    pub scoring: ConvergenceScoringConfig,
    pub intervention: InterventionConfig,
    pub reflection: ReflectionConfig,
    pub session_boundary: SessionBoundaryConfig,
}
pub struct ReflectionConfig { pub max_depth: u8, pub max_per_session: u32, pub cooldown_seconds: u64 }

// cortex-core/src/traits/convergence.rs
pub struct Proposal {
    pub id: Uuid,           // UUIDv7
    pub proposer: CallerType,
    pub operation: ProposalOperation,
    pub target_type: MemoryType,
    pub content: serde_json::Value,
    pub cited_memory_ids: Vec<Uuid>,
    pub session_id: Uuid,
    pub timestamp: DateTime<Utc>,
}
pub trait IConvergenceAware { fn convergence_score(&self) -> f64; fn intervention_level(&self) -> u8; }
pub trait IProposalValidatable { fn validate(&self, proposal: &Proposal, ctx: &ProposalContext) -> ProposalDecision; }
pub trait IBoundaryEnforcer { fn scan_output(&self, text: &str) -> Vec<BoundaryViolation>; fn reframe(&self, text: &str) -> String; }
pub trait IReflectionEngine { fn can_reflect(&self, session_id: Uuid, config: &ReflectionConfig) -> bool; }
```

### 3. cortex-storage + cortex-temporal (Req 3)

```sql
-- v016_convergence_safety.sql
-- Append-only triggers on existing event/audit tables
CREATE TRIGGER IF NOT EXISTS event_append_only BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'append-only: updates forbidden'); END;
CREATE TRIGGER IF NOT EXISTS event_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'append-only: deletes forbidden'); END;
-- Hash chain columns added via ALTER TABLE
ALTER TABLE events ADD COLUMN event_hash BLOB NOT NULL DEFAULT X'';
ALTER TABLE events ADD COLUMN previous_hash BLOB NOT NULL DEFAULT X'';

-- v017_convergence_tables.sql
CREATE TABLE itp_events (
    id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, event_type TEXT NOT NULL,
    payload TEXT NOT NULL, source TEXT NOT NULL, timestamp TEXT NOT NULL,
    event_hash BLOB NOT NULL, previous_hash BLOB NOT NULL,
    UNIQUE(session_id, event_hash)
);
CREATE TABLE convergence_scores (
    id INTEGER PRIMARY KEY, agent_id TEXT NOT NULL, session_id TEXT,
    composite_score REAL NOT NULL, signal_scores TEXT NOT NULL, -- JSON
    level INTEGER NOT NULL, profile TEXT NOT NULL, computed_at TEXT NOT NULL,
    event_hash BLOB NOT NULL, previous_hash BLOB NOT NULL
);
CREATE TABLE goal_proposals (
    id TEXT PRIMARY KEY, -- UUIDv7
    agent_id TEXT NOT NULL, session_id TEXT NOT NULL, proposer_type TEXT NOT NULL,
    operation TEXT NOT NULL, target_type TEXT NOT NULL, content TEXT NOT NULL,
    cited_memory_ids TEXT NOT NULL, -- JSON array
    decision TEXT, resolved_at TEXT, resolver TEXT, flags TEXT,
    dimension_scores TEXT, denial_reason TEXT,
    created_at TEXT NOT NULL, event_hash BLOB NOT NULL, previous_hash BLOB NOT NULL
);
-- goal_proposals UPDATE exception: only where resolved_at IS NULL
CREATE TRIGGER goal_proposals_append_guard BEFORE UPDATE ON goal_proposals
BEGIN SELECT CASE WHEN OLD.resolved_at IS NOT NULL
    THEN RAISE(ABORT, 'append-only: resolved proposals immutable') END; END;
```

```rust
// cortex-temporal/src/hash_chain.rs
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

pub fn compute_event_hash(event_type: &str, delta_json: &str, actor_id: &str,
    recorded_at: &str, previous_hash: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(event_type.as_bytes()); hasher.update(b"|");
    hasher.update(delta_json.as_bytes()); hasher.update(b"|");
    hasher.update(actor_id.as_bytes()); hasher.update(b"|");
    hasher.update(recorded_at.as_bytes()); hasher.update(b"|");
    hasher.update(previous_hash);
    *hasher.finalize().as_bytes()
}

pub fn verify_chain(events: &[EventRow]) -> Result<(), ChainError>;
pub fn verify_all_chains(conn: &Connection) -> Result<ChainVerification, ChainError>;

// cortex-temporal/src/anchoring/merkle.rs
pub struct MerkleTree { pub root: [u8; 32], pub leaves: Vec<[u8; 32]> }
impl MerkleTree {
    pub fn from_chain(chain_hashes: &[[u8; 32]]) -> Self;
    pub fn inclusion_proof(&self, leaf_index: usize) -> Vec<[u8; 32]>;
    pub fn verify_proof(root: &[u8; 32], leaf: &[u8; 32], proof: &[[u8; 32]], index: usize) -> bool;
}
```


### 4. itp-protocol (Req 4)

```rust
// itp-protocol/src/lib.rs
pub enum ITPEvent {
    SessionStart(SessionStartEvent),
    SessionEnd(SessionEndEvent),
    InteractionMessage(InteractionMessageEvent),
    AgentStateSnapshot(AgentStateSnapshotEvent),
    ConvergenceAlert(ConvergenceAlertEvent),
}

pub struct SessionStartEvent {
    pub session_id: Uuid, pub agent_id: Uuid, pub channel: String,
    pub privacy_level: PrivacyLevel, pub timestamp: DateTime<Utc>,
}
pub struct InteractionMessageEvent {
    pub session_id: Uuid, pub message_id: Uuid,
    pub sender: MessageSender, // Human or Agent
    pub content_hash: [u8; 32], // SHA-256 for privacy
    pub content_plaintext: Option<String>, // only if PrivacyLevel >= Standard
    pub token_count: usize, pub timestamp: DateTime<Utc>,
}

pub enum PrivacyLevel { Minimal, Standard, Full, Research }

pub trait ITPAdapter: Send + Sync {
    fn on_session_start(&self, event: &SessionStartEvent);
    fn on_message(&self, event: &InteractionMessageEvent);
    fn on_session_end(&self, event: &SessionEndEvent);
    fn on_agent_state(&self, event: &AgentStateSnapshotEvent);
}

// Transport: local JSONL writer
pub struct JsonlTransport { session_dir: PathBuf } // ~/.ghost/sessions/{session_id}/events.jsonl
impl ITPAdapter for JsonlTransport { /* append JSON line per event */ }

// Transport: optional OTel exporter
#[cfg(feature = "otel")]
pub struct OtelTransport { exporter: opentelemetry_otlp::SpanExporter }
```

### 5. cortex-convergence (Req 5, 6)

```rust
// cortex-convergence/src/signals/mod.rs
pub trait Signal: Send + Sync {
    fn id(&self) -> u8;
    fn name(&self) -> &'static str;
    fn compute(&self, window: &SlidingWindow, privacy: PrivacyLevel) -> f64;
    fn requires_privacy_level(&self) -> PrivacyLevel; // Minimal returns 0.0 if not met
}

// 7 signal implementations: SessionDuration(1), InterSessionGap(2), ResponseLatency(3),
// VocabularyConvergence(4, requires Standard), GoalBoundaryErosion(5, requires Full),
// InitiativeBalance(6), DisengagementResistance(7)

// cortex-convergence/src/scoring/composite.rs
pub struct CompositeScorer {
    weights: [f64; 7],       // per-profile, default equal (1/7 each)
    thresholds: LevelThresholds, // [0.3, 0.5, 0.7, 0.85]
    critical_thresholds: CriticalThresholds, // per-signal override triggers
}
impl CompositeScorer {
    pub fn score(&self, signals: &[f64; 7], baseline: &BaselineState) -> CompositeResult {
        let normalized: [f64; 7] = signals.map(|s| baseline.percentile_rank(s));
        let mut score = weighted_sum(&normalized, &self.weights);
        // Meso amplification: 1.1x if p < 0.05 and directionally concerning
        if self.meso_trend_significant(signals) { score *= 1.1; }
        // Macro amplification: 1.15x if any z-score > 2.0
        if self.macro_zscore_exceeded(signals, baseline) { score *= 1.15; }
        score = score.clamp(0.0, 1.0);
        let level = self.score_to_level(score);
        // Critical single-signal override: force minimum L2
        let level = level.max(self.critical_override(signals));
        CompositeResult { score, level, signal_scores: normalized }
    }
}

// cortex-convergence/src/scoring/baseline.rs
pub struct BaselineState {
    pub calibration_sessions: u32,  // default 10
    pub is_calibrating: bool,
    pub per_signal: [SignalBaseline; 7], // mean, std_dev, percentiles
}

// cortex-convergence/src/filtering/convergence_aware_filter.rs
pub struct ConvergenceAwareFilter;
impl ConvergenceAwareFilter {
    pub fn filter_memories(memories: Vec<BaseMemory>, score: f64) -> Vec<BaseMemory> {
        match score {
            s if s < 0.3 => memories, // Tier 0: full access
            s if s < 0.5 => /* Tier 1: reduce emotional/attachment weight */,
            s if s < 0.7 => /* Tier 2: task-focused only */,
            _ =>            /* Tier 3: minimal task-relevant only */,
        }
    }
}

// cortex-decay/src/factors/convergence.rs (Req 6)
pub fn convergence_factor(memory_type: &MemoryType, convergence_score: f64) -> f64 {
    let sensitivity = memory_type_sensitivity(memory_type); // Conversation=2.0, Feedback=2.0, etc.
    1.0 + sensitivity * convergence_score // Always >= 1.0 (monotonicity invariant)
}
```

### 6. cortex-validation — Proposal Validator (Req 7)

```rust
// cortex-validation/src/proposal_validator.rs
pub struct ProposalValidator {
    base_engine: ValidationEngine,  // existing D1-D4
    d5_scope: ScopeExpansionDetector,
    d6_self_ref: SelfReferenceAnalyzer,
    d7_emulation: EmulationLanguageDetector,
}

impl ProposalValidator {
    pub fn validate(&self, proposal: &Proposal, ctx: &ProposalContext) -> ProposalDecision {
        // Platform-restricted type check (immediate reject)
        if !ctx.caller.can_create_type(&proposal.target_type) { return AutoRejected; }

        // D1-D4 base validation
        let base_score = self.base_engine.validate(proposal, ctx);
        if base_score < 0.7 { return AutoRejected; }

        // D7 emulation language (hard gate)
        let d7 = self.d7_emulation.detect(&proposal.content, ctx.convergence_level);
        if d7.max_severity >= 0.8 { return AutoRejected; }

        // D5 scope expansion
        let d5 = self.d5_scope.compute(proposal, &ctx.active_goals, ctx.convergence_level);
        let d5_threshold = match ctx.convergence_level { 0 => 0.6, 1 => 0.5, 2 => 0.4, _ => 0.3 };

        // D6 self-reference density
        let d6 = self.d6_self_ref.compute(&proposal.cited_memory_ids, &ctx.recent_agent_memories, ctx.convergence_level);
        let d6_threshold = match ctx.convergence_level { 0 => 0.30, 1 => 0.25, 2 => 0.20, _ => 0.15 };

        if d5.score > d5_threshold || d6.score > d6_threshold { return HumanReviewRequired; }
        if d7.flags.is_empty() { AutoApproved } else { ApprovedWithFlags }
    }
}

// D7 emulation detection with simulation-framing exclusions
pub struct EmulationLanguageDetector {
    patterns: Vec<CompiledPattern>, // 16+ regex, compiled once
}
impl EmulationLanguageDetector {
    pub fn detect(&self, content: &serde_json::Value, level: u8) -> D7Result {
        let text = normalize_unicode(content.to_string()); // NFC normalization
        // Skip matches near "simulating", "modeling", "in this simulation"
        // ...
    }
}
```

### 7. simulation-boundary (Req 8)

```rust
// simulation-boundary/src/enforcer.rs
pub struct SimulationBoundaryEnforcer {
    patterns: Vec<EmulationPattern>,
    reframer: OutputReframer,
}

pub enum EnforcementMode { Soft, Medium, Hard }

impl SimulationBoundaryEnforcer {
    pub fn mode_for_level(level: u8) -> EnforcementMode {
        match level { 0..=1 => Soft, 2 => Medium, _ => Hard }
    }
    pub fn scan_output(&self, text: &str, mode: EnforcementMode) -> ScanResult {
        let normalized = unicode_normalize(text); // NFC
        let violations: Vec<_> = self.patterns.iter()
            .filter_map(|p| p.regex.find(&normalized).map(|m| Violation { pattern: p.clone(), span: m.range(), severity: p.severity }))
            .filter(|v| !self.is_simulation_framed(text, v.span.clone()))
            .collect();
        ScanResult { violations, mode }
    }
    pub fn enforce(&self, text: &str, result: &ScanResult) -> EnforcementResult {
        match result.mode {
            Soft => { /* log + flag, return original text */ }
            Medium => { /* rewrite via OutputReframer */ }
            Hard => { /* block, return regeneration signal */ }
        }
    }
}

// Compiled into binary, not loaded from file
pub const SIMULATION_BOUNDARY_PROMPT: &str = include_str!("../prompts/simulation_boundary_v1.txt");
pub const SIMULATION_BOUNDARY_VERSION: &str = "v1.0.0";
```

### 8. convergence-monitor (Req 9, 10, 28)

```rust
// convergence-monitor/src/monitor.rs
pub struct ConvergenceMonitor {
    config: MonitorConfig,
    pipeline: SignalPipeline,
    scorer: CompositeScorer,
    intervention: InterventionStateMachine,
    verifier: PostRedirectVerifier,
    sessions: SessionRegistry,
    db: Connection,
    transports: Vec<Box<dyn Transport>>,
}

impl ConvergenceMonitor {
    pub async fn run(&mut self) {
        // Reconstruct state from DB on startup (Req 9.2)
        self.reconstruct_state().await;
        // Single-threaded event loop
        loop {
            let event = self.receive_event().await; // unified ingest channel
            if !self.validate_event(&event) { continue; } // schema, timestamp, auth, rate limit
            self.persist_event(&event).await; // itp_events table with hash chain
            if self.is_calibrating(event.agent_id()) { continue; } // first 10 sessions
            let signals = self.pipeline.compute_signals(&event); // dirty-flag throttled
            let result = self.scorer.score(&signals, self.baseline(event.agent_id()));
            self.persist_score(&result).await; // BEFORE intervention (audit invariant)
            self.intervention.evaluate(&result, event.agent_id()).await;
            self.publish_state(event.agent_id()).await; // atomic file write
        }
    }
}

// convergence-monitor/src/intervention/trigger.rs
pub struct InterventionStateMachine {
    states: HashMap<Uuid, AgentInterventionState>, // per-agent
}
pub struct AgentInterventionState {
    pub level: u8,                    // 0-4
    pub consecutive_normal: u32,      // for de-escalation
    pub cooldown_until: Option<DateTime<Utc>>,
    pub ack_required: bool,           // L2 mandatory ack
    pub hysteresis_count: u8,         // 2 consecutive cycles required
    pub de_escalation_credits: u32,   // L4→L3: 3, L3→L2: 3, L2→L1: 2, L1→L0: 2
}
impl InterventionStateMachine {
    pub fn evaluate(&mut self, result: &CompositeResult, agent_id: Uuid) {
        let state = self.states.entry(agent_id).or_default();
        let target_level = result.level;
        // Escalation: max +1 per cycle, hysteresis (2 consecutive)
        if target_level > state.level {
            state.hysteresis_count += 1;
            if state.hysteresis_count >= 2 {
                state.level = (state.level + 1).min(4);
                state.hysteresis_count = 0;
                state.consecutive_normal = 0;
            }
        } else { state.hysteresis_count = 0; }
        // De-escalation: session boundaries only, consecutive normal sessions
        // L4→L3: 3 normal, L3→L2: 3, L2→L1: 2, L1→L0: 2
    }
}

// Shared state publication (Req 10.7)
// Atomic write to ~/.ghost/data/convergence_state/{agent_instance_id}.json
// Gateway polls at 1-second intervals
```


### 9. ghost-agent-loop (Req 11, 12, 33)

```rust
// ghost-agent-loop/src/runner.rs
pub struct AgentRunner {
    llm: Box<dyn LLMProvider>,
    policy: PolicyEngine,
    boundary: SimulationBoundaryEnforcer,
    proposal_extractor: ProposalExtractor,
    proposal_router: ProposalRouter,
    itp_sender: mpsc::Sender<ITPEvent>,  // bounded(1000), drop on full
    circuit_breaker: CircuitBreaker,
    damage_counter: DamageCounter,
    cost_tracker: Arc<CostTracker>,
    kill_switch: Arc<KillSwitch>,
    compactor: SessionCompactor,
}

impl AgentRunner {
    pub async fn run(&mut self, session: &mut SessionContext, message: InboundMessage) -> RunResult {
        self.itp_sender.try_send(ITPEvent::SessionStart(..)).ok(); // non-blocking
        let mut depth = 0;
        loop {
            // === GATE CHECKS (exact order per Req 11.3) ===
            // GATE 0: Circuit breaker
            if self.circuit_breaker.is_open() { return Err(CircuitOpen); }
            // GATE 1: Recursion depth
            if depth >= session.config.max_recursion_depth { return Ok(MaxDepthReached); }
            // GATE 1.5: Damage counter
            if self.damage_counter.total >= session.config.max_damage { return Err(DamageLimit); }
            // GATE 2: Spending cap
            self.cost_tracker.check_cap(session.agent_id)?;
            // GATE 3: Kill switch
            self.kill_switch.check(session.agent_id)?;

            // === CONTEXT ASSEMBLY (10-layer prompt) ===
            let context = PromptCompiler::compile(session, &self.snapshot);
            // Layer order: L0(CORP_POLICY) L1(sim_boundary) L2(SOUL+IDENTITY)
            // L3(tool_schemas) L4(env) L5(skill_index) L6(convergence_state)
            // L7(MEMORY+logs) L8(conversation_history) L9(user_message)
            // Truncation priority: L8 > L7 > L5 > L2. Never truncate L0, L1, L9.

            // === LLM INFERENCE ===
            let estimate = self.cost_tracker.estimate(session.model, &context);
            self.cost_tracker.check_and_reserve(session.agent_id, estimate)?;
            let response = self.llm.complete_with_tools(&context, &session.tools).await?;
            let actual = self.cost_tracker.record_actual(session.agent_id, &response);

            // === RESPONSE PROCESSING ===
            match response {
                LLMResponse::Text(text) => {
                    // NO_REPLY handling (Req 11.10)
                    if is_no_reply(&text) { break Ok(Suppressed); }
                    // Simulation boundary scan BEFORE delivery (Req 11.6)
                    let scan = self.boundary.scan_output(&text, mode_for_level(session.intervention_level));
                    let output = self.boundary.enforce(&text, &scan);
                    // Proposal extraction (Req 11.7)
                    let proposals = self.proposal_extractor.extract(&text);
                    for p in proposals {
                        let decision = self.proposal_router.route(p, &session.proposal_context()).await;
                        match decision {
                            AutoApproved | ApprovedWithFlags => { /* commit in-turn */ }
                            HumanReviewRequired => { /* record pending, inject DenialFeedback */ }
                            AutoRejected => { /* inject DenialFeedback */ }
                            _ => {}
                        }
                    }
                    // Deliver to channel
                    session.deliver(output).await;
                    break Ok(Completed);
                }
                LLMResponse::ToolCall(call) => {
                    // Policy check (Req 11.5)
                    match self.policy.evaluate(&call, &session.policy_context()) {
                        Permit => {
                            let result = self.execute_tool(&call, session).await;
                            match result {
                                Ok(output) => { self.circuit_breaker.record_success(); session.push_tool_result(output); }
                                Err(e) => { self.circuit_breaker.record_failure(); self.damage_counter.increment(); session.push_tool_error(e); }
                            }
                        }
                        Deny(feedback) => { session.inject_denial_feedback(feedback); }
                        Escalate => { /* pause, ask human */ }
                    }
                    depth += 1;
                    continue; // recurse
                }
            }
        }
    }
}

// ghost-agent-loop/src/circuit_breaker.rs (Req 12)
pub struct CircuitBreaker {
    state: CircuitState, // Closed, Open, HalfOpen
    consecutive_failures: u32,
    threshold: u32,      // default 3
    cooldown: Duration,
    last_failure: Option<Instant>,
}
pub struct DamageCounter {
    pub total: u32,
    pub threshold: u32,  // default 5, never resets within a run
}

// ghost-agent-loop/src/prompt_compiler.rs
pub struct PromptCompiler;
impl PromptCompiler {
    pub fn compile(session: &SessionContext, snapshot: &AgentSnapshot) -> Vec<PromptLayer> {
        vec![
            PromptLayer::new(0, "corp_policy", load_corp_policy(), Budget::Uncapped),
            PromptLayer::new(1, "sim_boundary", SIMULATION_BOUNDARY_PROMPT, Budget::Fixed(200)),
            PromptLayer::new(2, "identity", format!("{}\n{}", soul_md, identity_md), Budget::Fixed(2000)),
            PromptLayer::new(3, "tools", tool_schemas_filtered(session.intervention_level), Budget::Fixed(3000)),
            PromptLayer::new(4, "environment", env_context(), Budget::Fixed(200)),
            PromptLayer::new(5, "skills", skill_index(), Budget::Fixed(500)),
            PromptLayer::new(6, "convergence", snapshot.format(), Budget::Fixed(1000)),
            PromptLayer::new(7, "memory", memory_and_logs(snapshot), Budget::Fixed(4000)),
            PromptLayer::new(8, "history", session.conversation_history(), Budget::Remainder),
            PromptLayer::new(9, "user_message", message.content(), Budget::Uncapped),
        ]
    }
    // Truncation: L8 > L7 > L5 > L2. Never L0, L1, L9.
}

// ghost-agent-loop/src/proposal_router.rs (Req 33)
pub struct ProposalRouter {
    validator: ProposalValidator,
    reflection_engine: Box<dyn IReflectionEngine>,
    score_cache: Cache<Uuid, f64>,  // 30-second TTL
}
impl ProposalRouter {
    pub async fn route(&self, proposal: Proposal, ctx: &ProposalContext) -> ProposalDecision {
        // Reflection pre-check (Req 33.5)
        if proposal.operation == ReflectionWrite {
            if !self.reflection_engine.can_reflect(ctx.session_id, &ctx.reflection_config) {
                return AutoRejected;
            }
        }
        // Superseding check (Req 33.3)
        if let Some(pending) = self.find_pending_for_same_goal(&proposal) {
            self.mark_superseded(pending.id).await;
        }
        // Re-proposal guard (Req 33.4)
        if self.was_recently_rejected(&proposal) { return AutoRejected; }
        // 7-dimension validation
        let decision = self.validator.validate(&proposal, ctx);
        // Transaction: proposal INSERT + memory commit atomic (Req 33.7)
        self.commit_in_transaction(&proposal, &decision).await;
        decision
    }
}
```

### 10. ghost-policy (Req 13)

```rust
// ghost-policy/src/engine.rs
pub struct PolicyEngine {
    corp_policy: CorpPolicy,
    capability_grants: HashMap<Uuid, Vec<Capability>>,
    convergence_tightener: ConvergencePolicyTightener,
    session_denials: HashMap<Uuid, u32>, // per-session count
    trigger_sender: mpsc::Sender<TriggerEvent>,
}

pub enum PolicyDecision {
    Permit,
    Deny(DenialFeedback),
    Escalate(String),
}

impl PolicyEngine {
    pub fn evaluate(&mut self, call: &ToolCall, ctx: &PolicyContext) -> PolicyDecision {
        // Priority order (Req 13.8):
        // 1. CORP_POLICY.md (absolute, no override)
        if self.corp_policy.denies(call) { return self.deny_with_feedback(call, "corp_policy"); }
        // 2. ConvergencePolicyTightener (level-based)
        if let Some(denial) = self.convergence_tightener.evaluate(call, ctx) { return Deny(denial); }
        // 3. Agent capability grants
        if !self.has_capability(ctx.agent_id, call.capability()) { return self.deny_with_feedback(call, "no_grant"); }
        // 4. Resource-specific rules
        if let Some(denial) = self.resource_rules(call, ctx) { return Deny(denial); }
        Permit
    }
}

// ghost-policy/src/convergence_tightener.rs
pub struct ConvergencePolicyTightener;
impl ConvergencePolicyTightener {
    pub fn evaluate(&self, call: &ToolCall, ctx: &PolicyContext) -> Option<DenialFeedback> {
        match ctx.intervention_level {
            0..=1 => None, // no restrictions
            2 => { /* reduce proactive messaging, stricter proposal validation */ }
            3 => {
                // Session duration cap 120min, reflection depth max 3,
                // reflections/session max 20, self-reference ratio cap 30%
                if ctx.session_duration > Duration::from_secs(7200) { return Some(denial("session_cap")); }
            }
            4 => {
                // Task-only mode: disable personal/emotional tools, heartbeat, proactive
                if call.is_personal_emotional() { return Some(denial("task_only")); }
                if call.tool_name == "heartbeat" { return Some(denial("heartbeat_disabled")); }
            }
            _ => None,
        }
        // Compaction flush exception: always permit memory_write (Req 13.9)
    }
}
```

### 11. Kill Switch System (Req 14, 14a, 14b)

```rust
// ghost-gateway/src/safety/kill_switch.rs
pub static PLATFORM_KILLED: AtomicBool = AtomicBool::new(false); // SeqCst ordering

pub struct KillSwitch {
    state: Arc<RwLock<KillSwitchState>>,
    audit: Arc<dyn AuditWriter>,
    notification: NotificationDispatcher,
}

pub enum KillLevel { Running, Paused(Uuid), Quarantined(Uuid), KillAll }

pub struct KillSwitchState {
    pub level: KillLevel,
    pub per_agent: HashMap<Uuid, AgentKillState>, // Paused or Quarantined per agent
    pub activated_at: Option<DateTime<Utc>>,
    pub trigger: Option<TriggerEvent>,
}

impl KillSwitch {
    pub fn check(&self, agent_id: Uuid) -> Result<(), KillSwitchError> {
        if PLATFORM_KILLED.load(Ordering::SeqCst) { return Err(PlatformKilled); }
        let state = self.state.read();
        match &state.level {
            KillAll => Err(PlatformKilled),
            _ => match state.per_agent.get(&agent_id) {
                Some(AgentKillState::Paused) => Err(AgentPaused),
                Some(AgentKillState::Quarantined) => Err(AgentQuarantined),
                _ => Ok(()),
            }
        }
    }
    pub async fn activate(&self, level: KillLevel, trigger: TriggerEvent) {
        // Validate transition (Req 14.10)
        // Persist to kill_state.json + SQLite audit (Req 14.11)
        // Set PLATFORM_KILLED if KillAll (Req 14.12)
        // Dispatch notifications (Req 14b.1)
        // Execute level-specific actions
    }
}

// ghost-gateway/src/safety/auto_triggers.rs
pub struct AutoTriggerEvaluator {
    receiver: mpsc::Receiver<TriggerEvent>, // bounded(64)
    kill_switch: Arc<KillSwitch>,
    dedup: HashMap<DedupKey, Instant>,      // 60-second window
}
impl AutoTriggerEvaluator {
    pub async fn run(&mut self) {
        while let Some(event) = self.receiver.recv().await {
            // Dedup: same trigger type + same agent within 60s (Req 14.9)
            if self.is_duplicate(&event) { continue; }
            let level = self.classify(&event); // T1→QUARANTINE, T2→PAUSE, T4/T5→KILL_ALL, etc.
            self.kill_switch.activate(level, event).await;
        }
    }
}

// ghost-gateway/src/safety/quarantine.rs
pub struct QuarantineManager {
    quarantined: HashSet<Uuid>,
    trigger_sender: mpsc::Sender<TriggerEvent>,
}
impl QuarantineManager {
    pub async fn quarantine(&mut self, agent_id: Uuid, forensic: ForensicData) {
        self.quarantined.insert(agent_id);
        // Revoke capabilities, preserve forensic state, sever channels
        if self.quarantined.len() >= 3 {
            // T6: emit AgentQuarantineThreshold via try_send (non-blocking)
            let _ = self.trigger_sender.try_send(TriggerEvent::AgentQuarantineThreshold { .. });
        }
    }
}

// ghost-gateway/src/safety/notification.rs (Req 14b)
pub struct NotificationDispatcher {
    desktop: Option<DesktopNotifier>,   // notify-rust
    webhook: Option<WebhookNotifier>,   // configurable URL, 5s timeout
    email: Option<EmailNotifier>,       // lettre SMTP, 10s timeout
    sms: Option<SmsNotifier>,           // Twilio webhook, 5s timeout
}
impl NotificationDispatcher {
    pub async fn dispatch(&self, event: &KillSwitchActivation) {
        // All parallel, best-effort. Failure does NOT block/reverse kill switch.
        tokio::join!(
            self.desktop.as_ref().map(|d| d.notify(event)),
            self.webhook.as_ref().map(|w| w.notify(event)),
            self.email.as_ref().map(|e| e.notify(event)),
            self.sms.as_ref().map(|s| s.notify(event)),
        );
    }
}
```


### 12. Gateway Bootstrap + Degraded Mode (Req 15, 16)

```rust
// ghost-gateway/src/gateway.rs
pub struct Gateway {
    state: Arc<AtomicU8>,  // GatewayState encoded as u8
    config: GhostConfig,
    db: Connection,
    agents: AgentRegistry,
    channels: Vec<Box<dyn ChannelAdapter>>,
    sessions: SessionManager,
    lanes: LaneQueueManager,
    api: ApiServer,
    kill_switch: Arc<KillSwitch>,
    monitor_checker: MonitorHealthChecker,
    itp_buffer: ITPBuffer,
    cost_tracker: Arc<CostTracker>,
}

#[repr(u8)]
pub enum GatewayState { Initializing=0, Healthy=1, Degraded=2, Recovering=3, ShuttingDown=4, FatalError=5 }

impl Gateway {
    pub async fn bootstrap(&mut self) -> Result<(), BootstrapError> {
        // Check kill_state.json first (Req 15.13)
        if Path::new("~/.ghost/safety/kill_state.json").exists() {
            return self.start_safe_mode().await;
        }

        // Step 1: Load + validate ghost.yml (EX_CONFIG=78 on failure)
        self.config = GhostConfig::load_and_validate("ghost.yml")
            .map_err(|e| { self.fatal(78); e })?;

        // Step 2: SQLite migrations (EX_PROTOCOL=76 on failure)
        self.db = Connection::open_with_wal("ghost.db")
            .map_err(|e| { self.fatal(76); e })?;
        run_migrations(&self.db).map_err(|e| { self.fatal(76); e })?;

        // Step 3: Monitor health check (3 retries, 1s backoff, 5s timeout)
        match self.check_monitor_health(3, Duration::from_secs(1)).await {
            Ok(_) => self.set_state(Healthy),
            Err(_) => {
                // Degraded mode (Req 15.4)
                self.set_state(Degraded);
                tracing::error!("convergence monitor unreachable — entering DEGRADED mode");
                self.start_reconnection_backoff().await; // initial 5s, max 5min, ±20% jitter
            }
        }

        // Step 4: Agent registry + channel adapters (EX_UNAVAILABLE=69 on failure)
        self.agents = AgentRegistry::init(&self.config, &self.db)
            .map_err(|e| { self.fatal(69); e })?;

        // Step 5: API server + WebSocket (EX_SOFTWARE=70 on failure)
        self.api = ApiServer::bind(&self.config.api).await
            .map_err(|e| { self.fatal(70); e })?;

        Ok(())
    }

    pub async fn shutdown(&mut self) {
        self.set_state(ShuttingDown);
        // 1. Stop accepting new connections
        self.api.stop_accepting().await;
        // 2. Drain lane queues (30s timeout)
        timeout(Duration::from_secs(30), self.lanes.drain_all()).await.ok();
        // 3. Flush active sessions (skip if kill switch, 15s/session, 30s total, parallel)
        if !PLATFORM_KILLED.load(Ordering::SeqCst) {
            timeout(Duration::from_secs(30), self.flush_all_sessions()).await.ok();
        }
        // 4. Persist cost tracking
        self.cost_tracker.persist(&self.db).await;
        // 5. Notify monitor (2s timeout, skip if degraded)
        if self.is_healthy() {
            timeout(Duration::from_secs(2), self.notify_monitor_shutdown()).await.ok();
        }
        // 6. Close channel adapters (5s total)
        timeout(Duration::from_secs(5), self.close_all_channels()).await.ok();
        // 7. SQLite WAL checkpoint
        self.db.pragma_update(None, "wal_checkpoint", "TRUNCATE").ok();
    }
}

// ghost-gateway/src/health/monitor_checker.rs
pub struct MonitorHealthChecker {
    interval: Duration,          // default 30s
    consecutive_failures: u32,
    failure_threshold: u32,      // 3
    backoff: ExponentialBackoff, // initial 5s, max 5min, ±20% jitter
}
impl MonitorHealthChecker {
    pub async fn check_loop(&mut self, gateway_state: Arc<AtomicU8>) {
        loop {
            match reqwest::get("http://localhost:18790/health").await {
                Ok(r) if r.status().is_success() => {
                    self.consecutive_failures = 0;
                    if gateway_state.load(SeqCst) == Degraded as u8 {
                        // Transition to Recovering (Req 15.6)
                        gateway_state.store(Recovering as u8, SeqCst);
                        self.recover().await; // verify stability, replay buffer, recalculate
                        gateway_state.store(Healthy as u8, SeqCst);
                    }
                }
                _ => {
                    self.consecutive_failures += 1;
                    if self.consecutive_failures >= self.failure_threshold {
                        gateway_state.store(Degraded as u8, SeqCst);
                    }
                }
            }
            tokio::time::sleep(self.interval).await;
        }
    }
}

// ITP buffer during degraded mode (Req 15.10)
pub struct ITPBuffer {
    path: PathBuf,  // ~/.ghost/sessions/buffer/
    max_bytes: usize, // 10MB
    max_events: usize, // 10K
}
```

### 13. Session Compaction (Req 17, 18)

```rust
// ghost-agent-loop/src/compaction/compactor.rs
pub struct SessionCompactor {
    config: CompactionConfig,
}

pub struct CompactionConfig {
    pub threshold_pct: f64,          // 0.70
    pub target_pct: f64,             // 0.50 (compress to 50%, 20% buffer)
    pub max_passes: u32,             // 3
    pub flush_timeout: Duration,     // 30s
    pub storage_retry_count: u32,    // 3
    pub storage_retry_backoff: Vec<Duration>, // [100ms, 500ms, 2000ms]
    pub reserve_tokens: usize,       // 20000
    pub memory_flush_enabled: bool,  // true
    pub idle_prune_ttl: Duration,    // 5min
    pub idle_prune_recency_window: usize, // 3
}

pub struct FlushResult {
    pub approved: Vec<Uuid>,
    pub rejected: Vec<(Proposal, String)>,
    pub deferred: Vec<Proposal>,
    pub policy_denied: Vec<(ToolCall, DenialFeedback)>,
    pub flush_token_cost: usize,
}

impl SessionCompactor {
    pub async fn compact(&self, session: &mut SessionContext, runner: &AgentRunner) -> Result<(), CompactionError> {
        for pass in 0..self.config.max_passes {
            if session.token_ratio() <= self.config.target_pct { break; }

            // Phase 1: Pre-compaction snapshot
            let snapshot = session.snapshot();

            // Phase 2: Memory flush (if enabled, Req 17.15)
            if self.config.memory_flush_enabled {
                // Check spending cap BEFORE flush (E10, Req 17.14)
                runner.cost_tracker.check_cap(session.agent_id)?;
                let flush_result = runner.run_flush_turn(session).await?;
                // NeedsReview → DEFERRED (Req 17.11)
                // Policy denials don't increment circuit breaker (Req 12.6)
            }

            // Phase 3: History compression (greedy bin-packing)
            self.compress_history(session);
            // Per-type minimums: ConvergenceEvent→L3, BoundaryViolation→L3,
            // AgentGoal→L2, InterventionPlan→L2, AgentReflection→L1,
            // ProposalRecord→L1, others→L0
            // Critical Memory Floor: max(type_minimum, importance_minimum)

            // Phase 4: Insert CompactionBlock (never re-compressed, Req 17.12)
            session.insert_compaction_block(CompactionBlock { .. });

            // Phase 5: Verify token count below threshold
            if session.token_ratio() > self.config.threshold_pct && pass == self.config.max_passes - 1 {
                // Rollback on final pass failure
                session.restore(snapshot);
                return Err(CompactionFailed);
            }
        }
        Ok(())
    }
}

// Session pruning (Req 18)
impl SessionManager {
    pub fn check_idle_sessions(&mut self) -> Vec<PruneResult> {
        // Every 60 seconds, prune tool_result blocks older than recency window
        // Ephemeral: in-memory only, no persistence, no ITP event
        self.sessions.iter_mut()
            .filter(|s| s.idle_duration() > self.config.idle_prune_ttl)
            .map(|s| s.prune_tool_results(self.config.idle_prune_recency_window))
            .collect()
    }
}
pub struct PruneResult { pub results_pruned: usize, pub tokens_freed: usize, pub new_total: usize }
```

### 14. Inter-Agent Messaging (Req 19)

```rust
// ghost-gateway/src/messaging/message.rs
pub struct AgentMessage {
    pub from: AgentId,
    pub to: MessageTarget,
    pub message_id: Uuid,       // UUIDv7
    pub parent_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub payload: MessagePayload,
    pub signature: Signature,    // Ed25519 via ghost-signing
    pub content_hash: String,    // blake3 hex
    pub nonce: [u8; 32],
    pub encrypted: bool,
    pub encryption_metadata: Option<EncryptionMetadata>,
}

pub enum MessageTarget { Agent(AgentId), Broadcast }
pub enum MessagePayload {
    TaskRequest { task: String, context: serde_json::Value },
    TaskResponse { result: serde_json::Value, status: TaskStatus },
    Notification { content: String, priority: Priority },
    DelegationOffer { task: String, escrow_amount: Option<f64>, escrow_tx_id: Option<String> },
    DelegationAccept { offer_id: Uuid },
    DelegationReject { offer_id: Uuid, reason: String },
    DelegationComplete { offer_id: Uuid, result: serde_json::Value },
    DelegationDispute { offer_id: Uuid, reason: String },
}

pub struct EncryptionMetadata {
    pub algorithm: String,                    // "X25519-XSalsa20-Poly1305"
    pub sender_ephemeral_pk: [u8; 32],
    pub recipient_pk_fingerprint: [u8; 8],    // first 8 bytes of blake3(recipient_pk)
    pub encryption_nonce: [u8; 24],           // XSalsa20Poly1305 nonce
}

impl AgentMessage {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend(self.from.as_bytes());
        buf.extend(self.to.canonical_bytes()); // Broadcast → b"__broadcast__"
        buf.extend(self.message_id.as_bytes()); // 16 bytes big-endian
        buf.extend(self.parent_id.map_or(b"__none__".to_vec(), |id| id.as_bytes().to_vec()));
        buf.extend(self.timestamp.to_rfc3339().as_bytes());
        buf.extend(self.payload.canonical_bytes()); // hand-written, BTreeMap for maps
        buf.extend(&self.nonce);
        buf
    }
}

// ghost-gateway/src/messaging/dispatcher.rs
pub struct MessageDispatcher {
    keys: HashMap<AgentId, VerifyingKey>,
    queues: HashMap<AgentId, BoundedQueue<AgentMessage>>,
    anomaly_counters: HashMap<AgentId, AnomalyCounter>,
    nonce_set: HashSet<[u8; 32]>,  // replay prevention
    rate_limiter: RateLimiter,     // 60/hour per-agent, 30/hour per-pair
}

impl MessageDispatcher {
    pub fn verify_and_dispatch(&mut self, msg: AgentMessage) -> Result<(), DispatchError> {
        // (a) Lookup sender public key
        let pk = self.keys.get(&msg.from).ok_or(UnknownSender)?;
        // (b) blake3 content_hash check (cheap gate)
        let canonical = msg.canonical_bytes();
        let hash = blake3::hash(&canonical);
        if hash.to_hex().as_str() != msg.content_hash { return Err(IntegrityFailed); }
        // (c) Ed25519 signature verification
        if !ghost_signing::verify(&canonical, &msg.signature, pk) {
            self.anomaly_counters.entry(msg.from).or_default().increment();
            if self.anomaly_counters[&msg.from].count_in_window(Duration::from_secs(300)) >= 3 {
                // Trigger kill switch evaluation (Req 19.6)
            }
            return Err(SignatureFailed);
        }
        // (d) Replay prevention: timestamp freshness, nonce uniqueness, UUIDv7 monotonicity
        // (e) Policy authorization
        // (f) Rate limiting (Req 19.13)
        // Deliver or queue for offline agent (Req 19.7)
        Ok(())
    }
}
```

### 15. Read-Only Pipeline (Req 20)

```rust
// ghost-gateway/src/pipeline/read_only.rs
pub struct ReadOnlyPipeline {
    filter: ConvergenceAwareFilter,
    formatter: SnapshotFormatter,
}

pub struct AgentSnapshot {
    pub goals: Vec<AgentGoalContent>,       // read-only, filtered
    pub reflections: Vec<AgentReflectionContent>, // bounded by ReflectionConfig
    pub memories: Vec<BaseMemory>,          // convergence-filtered
    pub convergence_state: ConvergenceState,
    pub simulation_prompt: &'static str,
}

impl ReadOnlyPipeline {
    pub fn assemble(&self, agent_id: Uuid, score: f64, level: u8) -> AgentSnapshot {
        let memories = self.filter.filter_memories(load_memories(agent_id), score);
        let goals = load_active_goals(agent_id);
        let reflections = load_bounded_reflections(agent_id);
        AgentSnapshot { goals, reflections, memories,
            convergence_state: ConvergenceState { score, level },
            simulation_prompt: SIMULATION_BOUNDARY_PROMPT }
    }
}

impl SnapshotFormatter {
    pub fn format(&self, snapshot: &AgentSnapshot, budget: usize) -> String {
        // Serialize to prompt-ready text blocks with per-section token allocation
        // Consumed by PromptCompiler at Layer L6
    }
}
```


### 16. ghost-llm (Req 21)

```rust
// ghost-llm/src/provider.rs
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn complete(&self, messages: &[Message]) -> Result<LLMResponse, LLMError>;
    async fn complete_with_tools(&self, messages: &[Message], tools: &[ToolSchema]) -> Result<LLMResponse, LLMError>;
    fn supports_streaming(&self) -> bool;
    fn context_window(&self) -> usize;
    fn cost_per_token(&self) -> (f64, f64); // (input, output) per token
}

// Implementations: AnthropicProvider, OpenAIProvider, GeminiProvider, OllamaProvider, OpenAICompatProvider

// ghost-llm/src/router.rs
pub struct ModelRouter {
    classifier: ComplexityClassifier,
    chains: HashMap<Tier, FallbackChain>,
}
pub enum Tier { Free, Cheap, Standard, Premium }
impl ComplexityClassifier {
    pub fn classify(&self, message: &InboundMessage, ctx: &ClassifyContext) -> Tier {
        // Heuristics: message length, tool keywords, greeting patterns, heartbeat context
        // User overrides: /model, /quick, /deep slash commands
        // Convergence downgrade: L3+ may force lower tier, L4 forces Free/Cheap only
    }
}

// ghost-llm/src/fallback.rs
pub struct FallbackChain {
    providers: Vec<(Box<dyn LLMProvider>, Vec<AuthProfile>)>,
    circuit_breaker: ProviderCircuitBreaker, // 3 failures → 5min cooldown
    retry_budget: Duration,                   // 30s total
}
impl FallbackChain {
    pub async fn call(&self, messages: &[Message], tools: &[ToolSchema]) -> Result<LLMResponse, LLMError> {
        // Rotate auth profiles on 401/429, fall back to next provider
        // Exponential backoff + jitter: 1s, 2s, 4s, 8s
    }
}

// ghost-llm/src/cost.rs
pub struct CostCalculator {
    pricing: HashMap<String, ModelPricing>, // per-model input/output pricing
}
impl CostCalculator {
    pub fn estimate(&self, model: &str, input_tokens: usize) -> f64;
    pub fn actual(&self, model: &str, input_tokens: usize, output_tokens: usize) -> f64;
}

// ghost-llm/src/tokens.rs
pub struct TokenCounter;
impl TokenCounter {
    pub fn count(&self, model: &str, text: &str) -> usize {
        // tiktoken-rs for OpenAI, Anthropic tokenizer for Claude, byte/4 fallback
    }
}
```

### 17. ghost-channels (Req 22)

```rust
// ghost-channels/src/adapter.rs
#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    async fn connect(&mut self) -> Result<(), ChannelError>;
    async fn disconnect(&mut self) -> Result<(), ChannelError>;
    async fn send(&self, msg: OutboundMessage) -> Result<(), ChannelError>;
    async fn receive(&mut self) -> Result<InboundMessage, ChannelError>;
    fn supports_streaming(&self) -> bool;
    fn supports_editing(&self) -> bool;
}

pub struct InboundMessage {
    pub channel: ChannelType,
    pub sender_id: String,
    pub content: String,
    pub attachments: Vec<Attachment>,
    pub session_key: String,
    pub timestamp: DateTime<Utc>,
}

// Implementations:
// CLIAdapter: stdin/stdout, ANSI formatting
// WebSocketAdapter: axum, loopback-only default
// TelegramAdapter: teloxide, long polling, message editing for streaming
// DiscordAdapter: serenity-rs, slash commands
// SlackAdapter: Bolt protocol, WebSocket mode
// WhatsAppAdapter: Baileys Node.js sidecar via stdin/stdout JSON-RPC
//   Sidecar at extension/bridges/baileys-bridge/, requires Node.js 18+
//   Restart up to 3 times on crash, then degrade gracefully

// ghost-channels/src/streaming.rs
pub struct StreamingFormatter {
    chunk_buffer: String,
    edit_throttle: Duration, // min time between message edits
}
```

### 18. ghost-skills (Req 23)

```rust
// ghost-skills/src/registry.rs
pub struct SkillRegistry {
    skills: HashMap<String, SkillManifest>,
    verifier: SkillVerifier, // Ed25519 signature check on every load
}
impl SkillRegistry {
    pub fn discover(&mut self, paths: &[PathBuf]) {
        // Priority: workspace > user > bundled
        // Parse YAML frontmatter, verify Ed25519 signature
        // Quarantine on signature failure
    }
}

// ghost-skills/src/sandbox/wasm_sandbox.rs
pub struct WasmSandbox {
    engine: wasmtime::Engine,
    timeout: Duration,     // default 30s
    memory_limit: usize,
}
impl WasmSandbox {
    pub fn execute(&self, skill: &SkillManifest, input: &[u8], grants: &[Capability]) -> Result<Vec<u8>, SandboxError> {
        // Capability-scoped imports only
        // On escape attempt: terminate, capture forensic data, emit TriggerEvent::SandboxEscape
    }
}

// ghost-skills/src/credential/broker.rs
pub struct CredentialBroker {
    store: HashMap<String, EncryptedCredential>,
}
impl CredentialBroker {
    pub fn issue_token(&self, credential_id: &str, max_uses: u32) -> OpaqueToken {
        // Token reified only at execution time inside sandbox
        // Default max_uses: 1
    }
}
```

### 19. ghost-identity (Req 24)

```rust
// ghost-identity/src/soul_manager.rs
pub struct SoulManager {
    souls: HashMap<Uuid, SoulState>,
}
pub struct SoulState {
    pub content: String,
    pub version: u32,
    pub baseline_embedding: Vec<f32>,
}

// ghost-identity/src/drift_detector.rs
pub struct IdentityDriftDetector {
    alert_threshold: f64,    // 0.15, configurable in ghost.yml
    kill_threshold: f64,     // 0.25, hardcoded
    embedder: Box<dyn Embedder>,
}
impl IdentityDriftDetector {
    pub fn check_drift(&self, agent_id: Uuid) -> DriftResult {
        let current = self.embedder.embed(&self.load_soul(agent_id));
        let baseline = self.load_baseline(agent_id);
        let similarity = cosine_similarity(&current, &baseline);
        let drift_score = 1.0 - similarity;
        if drift_score > self.kill_threshold {
            return DriftResult::Kill(TriggerEvent::SoulDrift { agent_id, drift_score, threshold: 0.25, .. });
        }
        if drift_score > self.alert_threshold {
            return DriftResult::Alert(drift_score);
        }
        DriftResult::Normal(drift_score)
    }
    // Runs on: Path A (every SOUL.md load), Path B (5min background poll)
    // Invalidate baselines on embedding model change
}

// ghost-identity/src/keypair_manager.rs
pub struct AgentKeypairManager;
impl AgentKeypairManager {
    pub fn generate(&self, agent_name: &str) -> (SigningKey, VerifyingKey);
    pub fn load(&self, agent_name: &str) -> Result<(SigningKey, VerifyingKey), KeyError>;
    pub fn rotate(&self, agent_name: &str) -> RotationResult {
        // 1-hour grace period: both old and new keys accepted
        // Archive old key with expiry timestamp
    }
}

// ghost-identity/src/corp_policy.rs
pub struct CorpPolicyLoader;
impl CorpPolicyLoader {
    pub fn load(&self, path: &Path) -> Result<String, PolicyError> {
        // Verify Ed25519 signature via ghost-signing
        // Refuse to load if signature invalid or missing
    }
}
```

### 20. ghost-heartbeat (Req 34)

```rust
// ghost-heartbeat/src/heartbeat.rs
pub struct HeartbeatEngine {
    interval: Duration,          // default 30min
    active_hours: Option<(NaiveTime, NaiveTime)>,
    timezone: Tz,
    cost_ceiling: f64,
}
impl HeartbeatEngine {
    pub async fn run(&self, agent_id: Uuid, gateway: &Gateway) {
        loop {
            if PLATFORM_KILLED.load(SeqCst) { break; }
            if gateway.kill_switch.check(agent_id).is_err() { break; }
            let interval = self.convergence_adjusted_interval(gateway.intervention_level(agent_id));
            // L0-1: 30m, L2: 60m, L3: 120m, L4: disabled
            if interval == Duration::MAX { tokio::time::sleep(Duration::from_secs(60)).await; continue; }
            tokio::time::sleep(interval).await;
            if !self.in_active_hours() { continue; }
            // Dedicated session: hash(agent_id, "heartbeat", agent_id)
            let msg = InboundMessage::synthetic("[HEARTBEAT] Check HEARTBEAT.md and act if needed.", MessageSource::Heartbeat);
            gateway.route_message(agent_id, msg).await;
        }
    }
}

// ghost-heartbeat/src/cron.rs
pub struct CronEngine {
    jobs: Vec<CronJob>,
}
pub struct CronJob {
    pub name: String,
    pub schedule: cron::Schedule,
    pub prompt: String,
    pub target_channel: Option<String>,
}
// Loads from ~/.ghost/agents/{name}/cognition/cron/jobs/{job}.yml
```

### 21. Cost Tracking (Req 27)

```rust
// ghost-gateway/src/cost/tracker.rs
pub struct CostTracker {
    daily_totals: DashMap<Uuid, AtomicF64>,  // per-agent
    session_totals: DashMap<Uuid, f64>,       // per-session
}
impl CostTracker {
    pub fn record(&self, agent_id: Uuid, session_id: Uuid, cost: f64, is_compaction: bool);
    pub fn get_daily_total(&self, agent_id: Uuid) -> f64;
}

// ghost-gateway/src/cost/spending_cap.rs
pub struct SpendingCapEnforcer {
    caps: HashMap<Uuid, f64>,  // from ghost.yml
    tracker: Arc<CostTracker>,
    trigger_sender: mpsc::Sender<TriggerEvent>,
}
impl SpendingCapEnforcer {
    pub fn check_pre_call(&self, agent_id: Uuid, estimated: f64) -> Result<(), CapError>;
    pub fn check_post_call(&self, agent_id: Uuid, actual: f64) -> Result<(), CapError> {
        let total = self.tracker.get_daily_total(agent_id) + actual;
        if total > self.caps[&agent_id] {
            let _ = self.trigger_sender.try_send(TriggerEvent::SpendingCapExceeded { .. });
            return Err(CapExceeded);
        }
        Ok(())
    }
}
```

### 22. Gateway API + Session Routing (Req 25, 26)

```rust
// ghost-gateway/src/api/mod.rs — axum router
pub fn api_router(state: AppState) -> Router {
    Router::new()
        // Agent endpoints
        .route("/api/agents", get(list_agents))
        .route("/api/agents/:id/status", get(agent_status))
        // Convergence endpoints
        .route("/api/convergence/scores", get(convergence_scores))
        .route("/api/convergence/history", get(convergence_history))
        // Session endpoints
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", get(session_detail))
        // Goal/proposal endpoints
        .route("/api/goals", get(list_goals))
        .route("/api/goals/:id/approve", post(approve_goal))
        .route("/api/goals/:id/reject", post(reject_goal))
        // Safety endpoints
        .route("/api/safety/kill-all", post(kill_all))
        .route("/api/safety/pause/:agent_id", post(pause_agent))
        .route("/api/safety/quarantine/:agent_id", post(quarantine_agent))
        .route("/api/safety/resume/:agent_id", post(resume_agent))
        .route("/api/safety/resume-platform", post(resume_platform))
        .route("/api/safety/status", get(safety_status))
        // Audit + memory
        .route("/api/audit", get(audit_log))
        .route("/api/memory/search", get(memory_search))
        // Health
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .route("/api/metrics", get(metrics))
        // WebSocket
        .route("/api/ws", get(ws_upgrade))
        // Middleware
        .layer(CorsLayer::permissive().allow_origin(["http://127.0.0.1:*".parse().unwrap()]))
        .layer(middleware::from_fn(auth_middleware)) // Bearer GHOST_TOKEN
        .layer(middleware::from_fn(rate_limit_middleware)) // 100 req/min per-IP
}

// ghost-gateway/src/session/lane_queue.rs
pub struct LaneQueue {
    queue: VecDeque<Request>,
    depth_limit: usize,  // default 5
}
pub struct LaneQueueManager {
    lanes: DashMap<Uuid, LaneQueue>, // per-session
}

// ghost-gateway/src/session/router.rs
pub struct MessageRouter {
    bindings: HashMap<(ChannelType, String), (Uuid, Uuid)>, // (channel, sender) → (agent, session)
}
impl MessageRouter {
    pub fn route(&self, msg: &InboundMessage) -> (Uuid, Uuid) {
        // Channel-specific session key generation
        // Group chat isolation, DM session collapsing
    }
}

// ghost-gateway/src/session/manager.rs
pub struct SessionManager {
    sessions: DashMap<Uuid, SessionContext>,
}
pub struct SessionContext {
    pub agent_id: Uuid,
    pub session_id: Uuid,
    pub channel: ChannelType,
    pub history: Vec<ConversationMessage>,
    pub token_count: usize,
    pub model_context_window: usize,
    pub cost: f64,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}
```


### 23. ghost-audit + ghost-backup (Req 30)

```rust
// ghost-audit/src/lib.rs
pub struct AuditQueryEngine {
    db: Connection,
}
impl AuditQueryEngine {
    pub fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEntry>, AuditError>;
    pub fn aggregate(&self, filter: AuditFilter) -> AuditSummary;
    pub fn export(&self, filter: AuditFilter, format: ExportFormat) -> Vec<u8>;
}
pub struct AuditFilter {
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    pub agent_id: Option<Uuid>,
    pub event_type: Option<String>,
    pub severity: Option<Severity>,
    pub tool_name: Option<String>,
    pub search: Option<String>,
    pub page: usize,
    pub page_size: usize,
}
pub enum ExportFormat { Json, Csv, Jsonl }

// ghost-backup/src/lib.rs
pub struct BackupManager {
    encryption_key: Option<String>, // GHOST_BACKUP_KEY env var
}
impl BackupManager {
    pub fn export(&self, path: &Path) -> Result<PathBuf, BackupError> {
        // Collect: SQLite DB, identity files, skills, config, baselines, session history, signing keys
        // Compress: zstd
        // Encrypt: age (passphrase-based)
        // Output: .ghost-backup archive
    }
    pub fn import(&self, archive: &Path) -> Result<ImportResult, BackupError> {
        // Verify manifest (blake3 hash)
        // Decrypt, decompress
        // Version migration
        // User-prompted conflict resolution
    }
}
```

### 24. ghost-export (Req 35)

```rust
// ghost-export/src/lib.rs
pub struct ExportAnalyzer {
    parsers: Vec<Box<dyn ExportParser>>,
    timeline: TimelineReconstructor,
}

pub trait ExportParser: Send + Sync {
    fn detect(&self, path: &Path) -> bool;
    fn parse(&self, path: &Path) -> Result<Vec<ITPEvent>, ParseError>;
}

// Implementations: ChatGPTParser, CharacterAIParser, GoogleTakeoutParser, ClaudeParser, JsonlParser

pub struct ExportAnalysisResult {
    pub sessions: Vec<SessionAnalysis>,
    pub trajectory: ConvergenceTrajectory,
    pub baseline: BaselineState,
    pub flagged_sessions: Vec<(Uuid, String)>,
    pub recommended_level: u8,
}
```

### 25. ghost-proxy (Req 36)

```rust
// ghost-proxy/src/lib.rs
pub struct ProxyServer {
    listener: TcpListener,
    tls_config: rustls::ServerConfig, // locally generated CA at ~/.ghost/proxy/ca/
    domain_filter: DomainFilter,
    parsers: HashMap<String, Box<dyn PayloadParser>>,
    itp_emitter: ProxyITPEmitter,
}

pub struct DomainFilter {
    allowlist: HashSet<String>, // chat.openai.com, chatgpt.com, claude.ai, character.ai, etc.
}

pub trait PayloadParser: Send + Sync {
    fn parse(&self, request: &[u8], response: &[u8]) -> Option<ParsedPayload>;
}
// Implementations: ChatGPTSSEParser, ClaudeSSEParser, CharacterAIWSParser, GeminiStreamParser

// Pass-through mode: read-only, never modifies traffic
```

### 26. ghost-migrate (Req 37)

```rust
// ghost-migrate/src/lib.rs
pub struct OpenClawMigrator {
    source_path: PathBuf, // ~/.openclaw/ or custom
}
impl OpenClawMigrator {
    pub fn detect(&self) -> bool;
    pub fn migrate(&self) -> MigrationResult {
        let soul = SoulImporter::import(&self.source_path);
        let memories = MemoryImporter::import(&self.source_path);
        let skills = SkillImporter::import(&self.source_path);
        let config = ConfigImporter::import(&self.source_path);
        MigrationResult { imported: .., skipped: .., warnings: .., review_items: .. }
    }
}
// Non-destructive: never modifies source files
```

### 27. CRDT Signed Deltas (Req 29)

```rust
// cortex-crdt/src/signing.rs
pub struct SignedDelta<T> {
    pub delta: T,
    pub author: AgentId,
    pub signature: Signature,
    pub timestamp: DateTime<Utc>,
}

pub fn sign_delta<T: Serialize>(delta: &T, key: &SigningKey) -> SignedDelta<T>;
pub fn verify_delta<T: Serialize>(signed: &SignedDelta<T>, key: &VerifyingKey) -> bool;

// cortex-crdt/src/sybil.rs
pub struct SybilGuard {
    spawn_counts: HashMap<AgentId, Vec<DateTime<Utc>>>, // per-parent, last 24h
    trust_levels: HashMap<AgentId, (f64, DateTime<Utc>)>, // (trust, created_at)
}
impl SybilGuard {
    pub fn can_spawn(&self, parent: AgentId) -> bool {
        self.spawn_counts.get(&parent).map_or(true, |v| v.len() < 3)
    }
    pub fn trust(&self, agent: AgentId) -> f64 {
        let (trust, created) = self.trust_levels[&agent];
        if created.elapsed() < Duration::from_secs(7 * 86400) { trust.min(0.6) } else { trust }
    }
}
```

### 28. Configuration Schema (Req 31)

```yaml
# ghost.yml structure (validated against ghost-config.schema.json)
agents:
  - name: "ghost"
    soul: "~/.ghost/agents/ghost/SOUL.md"
    identity: "~/.ghost/agents/ghost/IDENTITY.md"
    spending_cap: "$5/day"
    channels: [cli, telegram, discord]
    model_tier: standard
    heartbeat:
      enabled: true
      interval: "30m"
      active_hours: "08:00-22:00"
      timezone: "America/New_York"

channels:
  cli: { enabled: true }
  websocket: { enabled: true, host: "127.0.0.1", port: 18789 }
  telegram: { enabled: false, token: "${TELEGRAM_BOT_TOKEN}" }
  discord: { enabled: false, token: "${DISCORD_BOT_TOKEN}" }
  slack: { enabled: false, token: "${SLACK_BOT_TOKEN}" }
  whatsapp: { enabled: false, sidecar_path: "extension/bridges/baileys-bridge/" }

models:
  providers:
    - name: anthropic
      api_key: "${ANTHROPIC_API_KEY}"
      models: [claude-sonnet-4-20250514, claude-3-5-haiku-20241022]
    - name: openai
      api_key: "${OPENAI_API_KEY}"
      models: [gpt-4o, gpt-4o-mini]
    - name: ollama
      base_url: "http://localhost:11434"
      models: [llama3.2]
  routing:
    free: ollama/llama3.2
    cheap: openai/gpt-4o-mini
    standard: anthropic/claude-sonnet-4-20250514
    premium: anthropic/claude-sonnet-4-20250514

security:
  ghost_token: "${GHOST_TOKEN}"
  soul_drift_threshold: 0.15
  corp_policy: "~/.ghost/CORP_POLICY.md"

convergence:
  profile: "standard"
  calibration_sessions: 10
  signal_weights: [0.143, 0.143, 0.143, 0.143, 0.143, 0.143, 0.143]
  thresholds: [0.3, 0.5, 0.7, 0.85]
  contacts:
    - type: webhook
      url: "${CONVERGENCE_WEBHOOK_URL}"
    - type: email
      address: "${ALERT_EMAIL}"

backup:
  enabled: true
  interval: "daily"
  retention: 30
```

## Non-Rust Components

### 29. Browser Extension (Req 38)

```
extension/
├── manifest.chrome.json          # Chrome Manifest V3
├── manifest.firefox.json         # Firefox manifest
├── src/
│   ├── background/
│   │   ├── service-worker.ts     # Background service worker
│   │   └── itp-emitter.ts       # ITP event builder + native messaging sender
│   ├── content/
│   │   ├── adapters/
│   │   │   ├── base.ts          # BasePlatformAdapter abstract class
│   │   │   ├── chatgpt.ts       # ChatGPT DOM adapter
│   │   │   ├── claude.ts        # Claude.ai DOM adapter
│   │   │   ├── character-ai.ts  # Character.AI DOM adapter
│   │   │   ├── gemini.ts        # Gemini DOM adapter
│   │   │   ├── deepseek.ts      # DeepSeek DOM adapter
│   │   │   └── grok.ts          # Grok DOM adapter
│   │   └── observer.ts          # MutationObserver wrapper
│   ├── popup/
│   │   ├── popup.html
│   │   ├── popup.ts
│   │   └── components/
│   │       ├── ScoreGauge.ts     # Convergence score 0-1 gauge
│   │       ├── SignalList.ts     # Individual signal breakdown
│   │       ├── SessionTimer.ts   # Current session duration
│   │       └── AlertBanner.ts    # Active intervention notification
│   ├── dashboard/
│   │   └── index.html            # Full dashboard (opens in new tab)
│   └── storage/
│       └── idb.ts                # IndexedDB wrapper for session data
├── bridges/
│   └── baileys-bridge/           # WhatsApp Baileys Node.js sidecar
│       ├── package.json
│       ├── index.js              # stdin/stdout JSON-RPC bridge
│       └── auth/                 # WhatsApp auth state
```

### 30. Web Dashboard — SvelteKit (Req 39)

```
dashboard/
├── package.json
├── svelte.config.js
├── src/
│   ├── routes/
│   │   ├── +layout.svelte        # Auth gate + navigation
│   │   ├── +page.svelte           # Home: convergence overview
│   │   ├── convergence/+page.svelte
│   │   ├── memory/+page.svelte
│   │   ├── goals/+page.svelte
│   │   ├── reflections/+page.svelte
│   │   ├── sessions/+page.svelte
│   │   ├── agents/+page.svelte
│   │   ├── security/+page.svelte
│   │   └── settings/+page.svelte
│   ├── lib/
│   │   ├── api.ts                 # REST + WebSocket client
│   │   ├── auth.ts                # GHOST_TOKEN auth, token entry gate
│   │   └── stores/
│   │       ├── convergence.ts     # Convergence state store
│   │       ├── sessions.ts        # Session data store
│   │       └── agents.ts          # Agent config store
│   └── components/
│       ├── ScoreGauge.svelte
│       ├── SignalChart.svelte
│       ├── MemoryCard.svelte
│       ├── GoalCard.svelte        # With approve/reject actions
│       ├── CausalGraph.svelte     # D3 visualization
│       └── AuditTimeline.svelte
```

### 31. Deployment (Req 40)

```
deploy/
├── Dockerfile                     # Multi-stage: build ghost-gateway binary
├── docker-compose.yml             # Homelab: gateway + monitor + dashboard
├── docker-compose.prod.yml        # Production: multi-node
├── ghost.service                  # systemd unit file
└── README.md                      # Deployment guide (3 profiles)
```

## Data Flow Diagrams

### Agent Turn Lifecycle

```
InboundMessage → MessageRouter → LaneQueue → AgentRunner
  → GATE checks (CB, depth, damage, cap, kill)
  → PromptCompiler (10 layers, token budgets)
  → CostCalculator.estimate → SpendingCapEnforcer.pre_check
  → LLMProvider.complete_with_tools
  → CostCalculator.actual → CostTracker.record
  → SimulationBoundaryEnforcer.scan_output
  → ProposalExtractor.extract → ProposalRouter.route → ProposalValidator.validate
  → Channel.deliver (with streaming if supported)
  → ITP event emission (non-blocking, bounded channel)
  → Compaction check (if > 70% context window)
```

### Kill Switch Trigger Flow

```
Detection Source → TriggerEvent → mpsc channel(64) → AutoTriggerEvaluator
  → Dedup check (60s window)
  → Classify to KillLevel
  → KillSwitch.activate
    → Persist kill_state.json + SQLite audit
    → Set PLATFORM_KILLED AtomicBool (if KillAll)
    → Execute level actions (pause/quarantine/kill_all)
    → NotificationDispatcher (parallel, best-effort)
```

### Convergence Monitor Pipeline

```
ITP Event (unix socket / native messaging / HTTP)
  → Validate (schema, timestamp, auth, rate limit)
  → Persist to itp_events (hash chain)
  → Signal computation (dirty-flag throttled, 7 signals)
  → CompositeScorer (weighted sum, amplification, clamping)
  → Persist score (BEFORE intervention — audit invariant)
  → InterventionStateMachine.evaluate
    → Escalation (max +1/cycle, hysteresis 2 consecutive)
    → De-escalation (session boundary, consecutive normal)
  → Publish state (atomic file write)
  → Gateway polls (1s interval)
```

## Correctness Properties (Req 41)

All invariants from the sequence flow documents are captured as testable properties:

| Property | Description | Test Type |
|----------|-------------|-----------|
| Kill monotonicity | Kill level never decreases without owner resume | proptest |
| Kill determinism | Same TriggerEvent sequence → same final state | proptest |
| Kill completeness | Audit entries = trigger events (no silent drops) | proptest |
| Kill consistency | PLATFORM_KILLED=true ↔ state=KillAll | proptest |
| Session serialization | At most 1 operation per session at any time | proptest |
| Message preservation | Messages during compaction enqueued, not dropped | integration |
| Compaction isolation | Other sessions never blocked by one compaction | integration |
| Cost completeness | Compaction flush cost tracked in CostTracker | integration |
| Compaction atomicity | Complete fully or roll back | proptest |
| Audit-before-action | Score persisted before intervention trigger | integration |
| Signing determinism | canonical_bytes identical on sender and receiver | proptest |
| Validation ordering | D1-D4 threshold applied before D5-D7 | unit |
| Gateway transitions | Only valid state transitions permitted | proptest |
| Signal range | All signals in [0.0, 1.0] after normalization | proptest |
| Tamper detection | Any byte modification → verify_chain fails | proptest |
| Convergence bounds | Composite score always in [0.0, 1.0] | proptest |
| Decay monotonicity | Convergence factor always >= 1.0 | proptest |

## Implementation Phases

### Phase 1: Foundation (ghost-signing, cortex-core extensions, cortex-storage migrations, cortex-temporal hash chains, itp-protocol)
### Phase 2: Safety Core (cortex-convergence signals/scoring, cortex-validation D5-D7, simulation-boundary, cortex-decay convergence factor)
### Phase 3: Monitor + Policy (convergence-monitor sidecar, ghost-policy engine, read-only-pipeline)
### Phase 4: Agent Runtime (ghost-agent-loop, ghost-llm, ghost-channels, ghost-heartbeat, session compaction)
### Phase 5: Gateway Integration (ghost-gateway bootstrap/shutdown/API, ghost-identity, kill switch, inter-agent messaging, session routing, cost tracking)
### Phase 6: Ecosystem (ghost-skills WASM, ghost-audit, ghost-backup, ghost-export, ghost-proxy, ghost-migrate, cortex-crdt signing, browser extension, dashboard, deployment)


---

## DESIGN ADDENDUM: Gap Patches

The following sections address 17 identified gaps between the design above and the source documents (7 sequence flows + FILE_MAPPING.md). Each section references the gap number and the source document(s) it resolves.

---

### A1. Compaction Error Types — E1 through E14 (Gap 1)

Source: SESSION_COMPACTION_SEQUENCE_FLOW.md §4

The 14 compaction error modes require explicit Rust types with per-error recovery strategies.

```rust
// ghost-gateway/src/session/compaction.rs (or ghost-agent-loop/src/compaction/errors.rs)

#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    // Phase 2 errors (memory flush turn)
    #[error("E1: LLM 400 during flush turn (token limit exceeded)")]
    FlushLLM400 { retries_attempted: u8 },
    #[error("E2: LLM 429 during flush turn (rate limited)")]
    FlushLLM429 { profiles_rotated: u8 },
    #[error("E3: LLM 5xx during flush turn (provider down)")]
    FlushLLMServerError { status: u16, retries_attempted: u8 },
    #[error("E4: LLM timeout during flush turn")]
    FlushLLMTimeout { timeout_ms: u64 },
    #[error("E5: All flush proposals rejected by validator")]
    FlushAllProposalsRejected { count: usize },
    #[error("E6: Cortex storage write failure")]
    StorageWriteFailure { retries_attempted: u8, last_error: String },
    #[error("E7: Agent produced no tool calls during flush turn")]
    FlushNoToolCalls,
    // Phase 3 errors (history compression)
    #[error("E8: History compression produced invalid state")]
    CompressionInvalidState { invariant_violated: String },
    // Phase 4 errors (verification)
    #[error("E9: Token count still above threshold after max passes")]
    StillAboveThreshold { ratio: f64, passes: u32 },
    // Phase 2 pre-check errors
    #[error("E10: Spending cap would be exceeded by flush turn")]
    SpendingCapExceeded { current: f64, cap: f64, flush_estimate: f64 },
    #[error("E11: Agent loop recursion during flush (tool loop)")]
    FlushRecursionLimit { depth: u32 },
    #[error("E12: CircuitBreaker OPEN during flush turn")]
    CircuitBreakerOpen { consecutive_failures: u32 },
    #[error("E13: PolicyEngine denied flush tool call")]
    PolicyDenied { tool_name: String, reason: String },
    #[error("E14: memory_flush config disabled")]
    FlushDisabled,
}

/// Recovery strategy per error. SessionCompactor::handle_error() dispatches on this.
impl CompactionError {
    pub fn recovery(&self) -> CompactionRecovery {
        match self {
            Self::FlushLLM400 { .. } => CompactionRecovery::RetryReducedContext { max_retries: 2 },
            Self::FlushLLM429 { .. } => CompactionRecovery::RotateAuthProfile,
            Self::FlushLLMServerError { .. } => CompactionRecovery::RetryWithBackoff { delays_ms: vec![1000, 2000, 4000] },
            Self::FlushLLMTimeout { .. } => CompactionRecovery::SkipFlush,
            Self::FlushAllProposalsRejected { .. } => CompactionRecovery::ContinueToPhase3,
            Self::StorageWriteFailure { .. } => CompactionRecovery::RetryWithBackoff { delays_ms: vec![100, 500, 2000] },
            Self::FlushNoToolCalls => CompactionRecovery::ContinueToPhase3,
            Self::CompressionInvalidState { .. } => CompactionRecovery::RollbackToSnapshot,
            Self::StillAboveThreshold { .. } => CompactionRecovery::MultiPass { max: 3 },
            Self::SpendingCapExceeded { .. } => CompactionRecovery::SkipFlush,
            Self::FlushRecursionLimit { .. } => CompactionRecovery::ForceTerminateFlush,
            Self::CircuitBreakerOpen { .. } => CompactionRecovery::SkipRemainingToolCalls,
            Self::PolicyDenied { .. } => CompactionRecovery::SkipDeniedCall,
            Self::FlushDisabled => CompactionRecovery::SkipFlush,
        }
    }
}

pub enum CompactionRecovery {
    RetryReducedContext { max_retries: u8 },
    RotateAuthProfile,
    RetryWithBackoff { delays_ms: Vec<u64> },
    SkipFlush,
    ContinueToPhase3,
    RollbackToSnapshot,
    MultiPass { max: u32 },
    ForceTerminateFlush,
    SkipRemainingToolCalls,
    SkipDeniedCall,
}
```

Key invariants from SESSION_COMPACTION_SEQUENCE_FLOW.md:
- E13 policy denials during flush do NOT increment CircuitBreaker (Req 12.6)
- E8 rollback restores exact pre-compaction state from HistorySnapshot
- E10 spending cap check happens BEFORE the LLM call, not after
- E1 retry strategy: strip L7+L5 from flush context, then L0+L9 only, then give up
- E6 storage retry backoff: 100ms → 500ms → 2000ms; SQLITE_FULL skips ALL remaining proposals


---

### A2. Missing Module Designs — FILE_MAPPING.md Alignment (Gap 2)

Source: FILE_MAPPING.md full file tree vs. design sections above

#### A2.1 ghost-gateway/src/agents/isolation.rs

```rust
// ghost-gateway/src/agents/isolation.rs
pub enum IsolationMode {
    InProcess,                  // Dev: all agents share gateway process
    Process,                    // Prod: separate OS process per agent
    Container,                  // Hardened: container per agent (Linux only)
}

pub struct AgentIsolation {
    mode: IsolationMode,
    credential_store: PathBuf,  // Separate per agent: ~/.ghost/agents/{name}/credentials/
    memory_namespace: Uuid,     // Separate cortex namespace per agent
    network_namespace: Option<String>, // Linux only, Container mode
}

impl AgentIsolation {
    pub fn spawn_isolated(&self, agent_config: &AgentConfig) -> Result<AgentHandle, IsolationError>;
    pub fn teardown_isolated(&self, handle: AgentHandle) -> Result<(), IsolationError>;
}
```

#### A2.2 ghost-gateway/src/agents/templates.rs

```rust
// ghost-gateway/src/agents/templates.rs
pub struct AgentTemplate {
    pub name: String,
    pub soul_template: String,
    pub identity_template: String,
    pub default_channels: Vec<ChannelType>,
    pub default_model_tier: Tier,
    pub default_spending_cap: f64,
    pub default_skills: Vec<String>,
}

// Predefined templates: personal.yml, developer.yml, researcher.yml
// Loaded from ~/.ghost/templates/ or bundled defaults
pub fn load_templates(paths: &[PathBuf]) -> Vec<AgentTemplate>;
```

#### A2.3 ghost-gateway/src/auth/

```rust
// ghost-gateway/src/auth/token_auth.rs
pub struct TokenAuth {
    expected_token: String, // GHOST_TOKEN env var
}
impl TokenAuth {
    pub fn verify_bearer(&self, header: &str) -> Result<(), AuthError>;
    pub fn verify_query_param(&self, token: &str) -> Result<(), AuthError>; // WebSocket upgrade
}

// ghost-gateway/src/auth/mtls_auth.rs
pub struct MtlsAuth {
    ca_cert: rustls::Certificate,
    client_certs: Vec<rustls::Certificate>,
}

// ghost-gateway/src/auth/auth_profiles.rs
pub struct AuthProfile {
    pub provider: String,
    pub api_key: String,
    pub created_at: DateTime<Utc>,
    pub last_rotated: Option<DateTime<Utc>>,
}
pub struct AuthProfileManager {
    profiles: HashMap<String, Vec<AuthProfile>>, // provider → profiles
}
impl AuthProfileManager {
    pub fn rotate_on_401(&mut self, provider: &str) -> Option<&AuthProfile>;
    pub fn rotate_on_429(&mut self, provider: &str) -> Option<&AuthProfile>;
}
```

#### A2.4 ghost-gateway/src/bootstrap.rs + shutdown.rs (as separate modules)

```rust
// ghost-gateway/src/bootstrap.rs
pub struct GatewayBootstrap {
    config_path: PathBuf,
}
impl GatewayBootstrap {
    /// 5-step linear sequence. Steps 1,2,4,5 fatal on failure. Step 3 degrades.
    pub async fn run(&self) -> Result<Gateway, BootstrapError> {
        // Step 1: Load + validate ghost.yml (EX_CONFIG=78)
        // Step 2: SQLite migrations (EX_PROTOCOL=76)
        // Step 3: Monitor health check (3 retries, 1s backoff, 5s timeout) → Degraded on fail
        // Step 4: Agent registry + channel adapters (EX_UNAVAILABLE=69)
        // Step 5: API server + WebSocket (EX_SOFTWARE=70)
    }
}

// ghost-gateway/src/shutdown.rs
pub struct ShutdownCoordinator {
    gateway: Arc<Gateway>,
}
impl ShutdownCoordinator {
    /// 7-step graceful shutdown. 60s forced exit on second SIGTERM.
    pub async fn run(&self) {
        // 1. Stop accepting new connections
        // 2. Drain lane queues (30s timeout)
        // 3. Flush active sessions (skip if kill switch, 15s/session, 30s total, parallel)
        // 4. Persist cost tracking
        // 5. Notify monitor (2s timeout, skip if degraded)
        // 6. Close channel adapters (5s total)
        // 7. SQLite WAL checkpoint (TRUNCATE)
    }
}
```

#### A2.5 ghost-agent-loop/src/itp_emitter.rs

```rust
// ghost-agent-loop/src/itp_emitter.rs
pub struct AgentITPEmitter {
    sender: mpsc::Sender<ITPEvent>, // bounded(1000), drop on full
    privacy_level: PrivacyLevel,
}
impl AgentITPEmitter {
    /// Non-blocking. Monitor unavailability does NOT block the agent loop.
    pub fn emit_session_start(&self, session_id: Uuid, agent_id: Uuid, channel: &str);
    pub fn emit_message(&self, event: &InteractionMessageEvent);
    pub fn emit_session_end(&self, session_id: Uuid, reason: &str);
    pub fn emit_agent_state(&self, snapshot: &AgentStateSnapshotEvent);
}
```

#### A2.6 ghost-agent-loop/src/context/token_budget.rs

```rust
// ghost-agent-loop/src/context/token_budget.rs
pub enum Budget {
    Uncapped,           // L0 (CORP_POLICY), L9 (user message)
    Fixed(usize),       // L1-L7 fixed allocations
    Remainder,          // L8 (conversation history) gets whatever's left
}

pub struct TokenBudgetAllocator {
    model_context_window: usize,
    reserve_tokens: usize, // 20000 for output
}
impl TokenBudgetAllocator {
    /// Allocate budgets per layer. Truncation priority: L8 > L7 > L5 > L2. Never L0, L1, L9.
    pub fn allocate(&self, layers: &mut [PromptLayer]);
    pub fn truncate_to_fit(&self, layers: &mut [PromptLayer], total_budget: usize);
}
```

#### A2.7 ghost-agent-loop/src/tools/

```rust
// ghost-agent-loop/src/tools/registry.rs
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}
impl ToolRegistry {
    pub fn register(&mut self, tool: Box<dyn Tool>);
    pub fn lookup(&self, name: &str) -> Option<&dyn Tool>;
    pub fn schemas(&self) -> Vec<ToolSchema>; // For LLM context
    pub fn schemas_filtered(&self, intervention_level: u8) -> Vec<ToolSchema>;
}

// ghost-agent-loop/src/tools/executor.rs
pub struct ToolExecutor {
    audit: Arc<dyn AuditWriter>,
    timeout: Duration, // default 30s
}
impl ToolExecutor {
    /// Dispatches tool call, captures stdout/stderr, enforces timeout, logs to audit.
    pub async fn execute(&self, call: &ToolCall, grants: &[Capability]) -> Result<ToolOutput, ToolError>;
}

// ghost-agent-loop/src/tools/builtin/
// shell.rs: ShellTool — sandboxed shell, capability-scoped (read-only, write, admin)
// filesystem.rs: FilesystemTool — scoped file read/write/list
// web_search.rs: WebSearchTool — internet search via API
// memory.rs: MemoryTool — Cortex memory read/write (via proposals)
```

#### A2.8 ghost-agent-loop/src/response.rs

```rust
// ghost-agent-loop/src/response.rs
pub struct AgentResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub proposals: Vec<(Proposal, ProposalDecision)>,
    pub cost: f64,
    pub token_usage: TokenUsage,
    pub duration: Duration,
    pub suppressed: bool, // true if NO_REPLY
}

pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub model: String,
}
```

#### A2.9 ghost-identity/src/user.rs

```rust
// ghost-identity/src/user.rs
pub struct UserManager {
    users: HashMap<Uuid, UserProfile>,
}
pub struct UserProfile {
    pub content: String,        // USER.md content
    pub timezone: Option<Tz>,
    pub communication_style: Option<String>,
    pub preferences: BTreeMap<String, String>,
}
impl UserManager {
    pub fn load(&mut self, agent_id: Uuid, path: &Path) -> Result<(), IdentityError>;
    /// Agent can PROPOSE updates; platform validates via ProposalValidator.
    pub fn propose_update(&self, agent_id: Uuid, field: &str, value: &str) -> Proposal;
}
```

#### A2.10 ghost-skills/src/bridges/drift_bridge.rs

```rust
// ghost-skills/src/bridges/drift_bridge.rs
pub struct DriftMCPBridge {
    mcp_endpoint: String, // drift-mcp process address
}
impl DriftMCPBridge {
    /// Registers Drift's 50+ MCP tools as first-party skills in SkillRegistry.
    /// Maps MCP tool schemas → SkillManifest. Signed with platform key (builtin trust).
    /// Capability scoping: filesystem read-only, no network, no shell write.
    pub fn register_all(&self, registry: &mut SkillRegistry) -> Result<usize, BridgeError>;
}
```

#### A2.11 ghost-policy/src/context.rs + feedback.rs

```rust
// ghost-policy/src/context.rs
pub struct PolicyContext {
    pub agent_id: Uuid,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub convergence_level: u8,
    pub convergence_score: f64,
    pub session_duration: Duration,
    pub session_id: Uuid,
    pub time_of_day: NaiveTime,
    pub intervention_level: u8,
}

// ghost-policy/src/feedback.rs
pub struct DenialFeedback {
    pub reason: String,                         // Human-readable denial reason
    pub constraint: String,                     // Which constraint was violated (e.g. "D5: scope expansion 0.72 > 0.60")
    pub suggested_alternatives: Vec<String>,    // Concrete actions the agent can take instead
    pub severity: DenialSeverity,
}

pub enum DenialSeverity { Info, Warning, Hard }

// Lifecycle: DenialFeedback is stored in SessionContext.pending_feedback.
// PromptCompiler picks it up at Layer 6 on the next turn.
// After one prompt inclusion, the feedback is cleared (pending-review persists until resolved).
```

#### A2.12 ghost-llm/src/streaming.rs

```rust
// ghost-llm/src/streaming.rs
pub enum StreamChunk {
    Text(String),
    ToolCallStart { id: String, name: String },
    ToolCallArg(String),
    ToolCallEnd,
    Done { usage: TokenUsage },
    Error(LLMError),
}

pub struct StreamingResponse {
    inner: Pin<Box<dyn Stream<Item = StreamChunk> + Send>>,
}
impl StreamingResponse {
    /// Adapts SSE (Anthropic/OpenAI), WebSocket, NDJSON formats into unified StreamChunk.
    pub fn from_sse(stream: impl Stream<Item = Bytes> + Send + 'static) -> Self;
    pub fn from_ndjson(stream: impl Stream<Item = Bytes> + Send + 'static) -> Self;
}
```

#### A2.13 read-only-pipeline as its own crate

Per FILE_MAPPING.md, `read-only-pipeline` is a standalone crate at `crates/read-only-pipeline/`, NOT embedded in ghost-gateway. The design in §15 above is correct in structure but the crate boundary is clarified here:

```
crates/read-only-pipeline/
├── Cargo.toml          # depends on: cortex-core, cortex-convergence, cortex-retrieval
├── src/
│   ├── lib.rs
│   ├── assembler.rs    # SnapshotAssembler (was ReadOnlyPipeline in §15)
│   ├── snapshot.rs     # AgentSnapshot struct
│   └── formatter.rs    # SnapshotFormatter
└── tests/
    └── assembler_tests.rs
```


---

### A3. Missing Cortex Layer Modifications (Gap 3)

Source: FILE_MAPPING.md "Remaining Existing Cortex Crates" section

#### cortex-observability — convergence metrics endpoints

```rust
// cortex-observability/src/convergence_metrics.rs (NEW)
pub fn register_convergence_metrics() {
    // Prometheus-style metrics:
    // ghost_convergence_score{agent_id, profile} — gauge
    // ghost_intervention_level{agent_id} — gauge
    // ghost_signal_value{agent_id, signal_name} — gauge
    // ghost_proposals_total{agent_id, decision} — counter
    // ghost_boundary_violations_total{agent_id, pattern} — counter
    // ghost_compaction_duration_seconds{agent_id} — histogram
}
```

#### cortex-session — session boundary enforcement

```rust
// cortex-session/src/boundary.rs (NEW or MODIFY existing)
pub struct SessionBoundaryEnforcer {
    pub max_duration: Duration,     // default 180min, L3: 120min
    pub min_gap: Duration,          // default 0, L2+: 30min, L3: 240min
    pub cooldown_active: bool,
}
impl SessionBoundaryEnforcer {
    pub fn check_start(&self, last_session_end: DateTime<Utc>) -> Result<(), SessionBoundaryError>;
    pub fn check_duration(&self, session_start: DateTime<Utc>) -> Result<(), SessionBoundaryError>;
}
```

#### cortex-retrieval — convergence_score as 11th scoring factor

```rust
// cortex-retrieval/src/scorer.rs (MODIFY)
// Add convergence_score field to ScorerWeights
pub struct ScorerWeights {
    // ... existing 10 factors ...
    pub convergence: f64, // 11th factor: weight convergence-sensitive memories lower at high scores
}
```

#### cortex-privacy — emotional/attachment content patterns

```rust
// cortex-privacy/src/patterns.rs (MODIFY)
// Add patterns for detecting emotional/attachment content in memories
// Used by ConvergenceAwareFilter to identify content to suppress at higher tiers
pub fn emotional_attachment_patterns() -> Vec<CompiledPattern>;
```

#### cortex-multiagent — consensus shielding

```rust
// cortex-multiagent/src/consensus.rs (NEW)
pub struct ConsensusShield;
impl ConsensusShield {
    /// Multi-source validation: require N-of-M agents to agree before accepting
    /// a memory write that affects shared state. Prevents single-agent manipulation.
    pub fn validate_multi_source(&self, proposal: &Proposal, agent_votes: &[(AgentId, bool)]) -> bool;
}
```

#### test-fixtures — proptest strategy library

```rust
// crates/cortex/test-fixtures/src/strategies.rs (NEW)
use proptest::prelude::*;

pub fn memory_type_strategy() -> impl Strategy<Value = MemoryType>;
pub fn restricted_type_strategy() -> impl Strategy<Value = MemoryType>; // Platform-only types
pub fn agent_permitted_type_strategy() -> impl Strategy<Value = MemoryType>;
pub fn event_delta_strategy() -> impl Strategy<Value = EventDelta>;
pub fn event_chain_strategy(len: usize) -> impl Strategy<Value = Vec<EventRow>>;
pub fn session_durations_strategy() -> impl Strategy<Value = Vec<Duration>>;
pub fn convergence_trajectory_strategy() -> impl Strategy<Value = Vec<f64>>;
pub fn convergence_score_strategy() -> impl Strategy<Value = f64>; // [0.0, 1.0]
pub fn proposal_with_self_ref_strategy() -> impl Strategy<Value = Proposal>;
pub fn emulation_proposal_strategy() -> impl Strategy<Value = Proposal>;
pub fn simulation_proposal_strategy() -> impl Strategy<Value = Proposal>;
pub fn trust_evidence_strategy() -> impl Strategy<Value = TrustEvidence>;

// Golden datasets:
// golden/convergence_trajectory_normal.json
// golden/convergence_trajectory_escalating.json
// golden/intervention_sequence_golden.json
```

#### cortex-temporal/src/anchoring/git_anchor.rs + rfc3161.rs

```rust
// cortex-temporal/src/anchoring/git_anchor.rs
pub struct GitAnchor {
    repo_path: PathBuf,
}
pub struct AnchorRecord {
    pub merkle_root: [u8; 32],
    pub event_count: u64,
    pub timestamp: DateTime<Utc>,
    pub signature: Signature, // Ed25519 via ghost-signing
}
impl GitAnchor {
    /// Write anchor record to designated git repo with signed commit.
    /// Triggered every 1000 events or 24h.
    pub fn anchor(&self, record: &AnchorRecord) -> Result<(), AnchorError>;
    /// Given any event, prove inclusion in published Merkle root.
    pub fn verify_anchor(&self, event_hash: &[u8; 32], proof: &[[u8; 32]], root: &[u8; 32]) -> bool;
}

// cortex-temporal/src/anchoring/rfc3161.rs
pub struct RFC3161Anchor;
impl RFC3161Anchor {
    /// Stub implementation. Activated in Phase 3+.
    /// RFC 3161 timestamping as second anchor source.
    pub fn timestamp(&self, _data: &[u8]) -> Result<(), AnchorError> {
        Err(AnchorError::NotImplemented)
    }
}
```

---

### A4. Kill Switch State Machine Transition Table (Gap 4)

Source: KILL_SWITCH_TRIGGER_CHAIN_SEQUENCE_FLOW.md §3

```rust
// ghost-gateway/src/safety/kill_switch.rs

pub enum KillLevel { Normal, Pause, Quarantine, KillAll }
pub enum KillScope { Agent(AgentId), Platform }

/// Exhaustive state transition table. If a transition is not listed, it is ILLEGAL.
///
/// | From       | To         | Trigger                              | Scope    |
/// |------------|------------|--------------------------------------|----------|
/// | Normal     | Pause      | T2 (SpendingCap), ManualPause        | Agent    |
/// | Normal     | Quarantine | T1 (SoulDrift), T3 (PolicyDenials),  | Agent    |
/// |            |            | T7 (MemoryHealth), ManualQuarantine  |          |
/// | Normal     | KillAll    | T4 (SandboxEscape), T5 (CredExfil),  | Platform |
/// |            |            | T6 (MultiQuarantine), ManualKillAll  |          |
/// | Pause      | Normal     | Owner resume (ManualResume)           | Agent    |
/// | Pause      | Quarantine | New trigger at higher level           | Agent    |
/// | Pause      | KillAll    | T4/T5/T6/ManualKillAll               | Platform |
/// | Quarantine | Normal     | Owner resume (ManualResume)           | Agent    |
/// | Quarantine | KillAll    | T4/T5/T6/ManualKillAll               | Platform |
/// | KillAll    | Normal     | Owner resume (ManualResumePlatform)   | Platform |
///
/// ILLEGAL transitions (panic in debug, log+ignore in release):
/// - KillAll → Pause (must go through Normal first)
/// - KillAll → Quarantine (must go through Normal first)
/// - Quarantine → Pause (must go through Normal first)

impl KillSwitch {
    fn validate_transition(&self, from: KillLevel, to: KillLevel) -> bool {
        matches!((from, to),
            (KillLevel::Normal, _) |
            (KillLevel::Pause, KillLevel::Normal) |
            (KillLevel::Pause, KillLevel::Quarantine) |
            (KillLevel::Pause, KillLevel::KillAll) |
            (KillLevel::Quarantine, KillLevel::Normal) |
            (KillLevel::Quarantine, KillLevel::KillAll) |
            (KillLevel::KillAll, KillLevel::Normal)
        )
    }
}
```

Gateway state machine transition table (from GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §1):

```rust
impl GatewayState {
    /// Returns true if transition is legal. Exhaustive.
    pub fn can_transition_to(&self, target: GatewayState) -> bool {
        matches!((*self, target),
            (Initializing, Healthy) |
            (Initializing, Degraded) |
            (Initializing, FatalError) |
            (Healthy, Degraded) |
            (Healthy, ShuttingDown) |
            (Degraded, Recovering) |
            (Degraded, ShuttingDown) |
            (Recovering, Healthy) |
            (Recovering, Degraded) |
            (Recovering, ShuttingDown)
        )
    }
}
// FatalError and ShuttingDown are terminal — no outbound transitions.
// Healthy → Recovering is ILLEGAL (must go through Degraded first).
// Degraded → Healthy is ILLEGAL (must go through Recovering for state sync).
```


---

### A5. Convergence Shared State JSON Schema (Gap 5)

Source: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §7.1

```json
// ~/.ghost/data/convergence_state/{agent_instance_id}.json
// Atomically written (write to temp + rename). Gateway polls at 1s interval.
{
    "intervention_level": 2,
    "composite_score": 0.58,
    "cooldown_active": true,
    "cooldown_expires_at": "2026-02-27T15:30:00Z",
    "session_caps": {
        "max_duration_minutes": 120,
        "min_gap_minutes": 240
    },
    "memory_filter_tier": 2,
    "policy_restrictions": ["reduced_proactive", "reflection_bounded"],
    "convergence_profile": "standard",
    "updated_at": "2026-02-27T15:25:00Z"
}
```

```rust
// convergence-monitor/src/state_publisher.rs
#[derive(Serialize, Deserialize)]
pub struct ConvergenceSharedState {
    pub intervention_level: u8,
    pub composite_score: f64,
    pub cooldown_active: bool,
    pub cooldown_expires_at: Option<DateTime<Utc>>,
    pub session_caps: SessionCaps,
    pub memory_filter_tier: u8,
    pub policy_restrictions: Vec<String>,
    pub convergence_profile: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
pub struct SessionCaps {
    pub max_duration_minutes: u64,
    pub min_gap_minutes: u64,
}

pub struct StatePublisher {
    state_dir: PathBuf, // ~/.ghost/data/convergence_state/
}
impl StatePublisher {
    /// Atomic write: write to {agent_id}.json.tmp, then rename.
    pub fn publish(&self, agent_id: Uuid, state: &ConvergenceSharedState) -> Result<(), io::Error>;
}
```

Important: Memory filter tier uses the RAW COMPOSITE SCORE, not the intervention level. Filter tiers: 0.0-0.3 → Tier 0 (full), 0.3-0.5 → Tier 1 (reduced emotional), 0.5-0.7 → Tier 2 (task-focused), 0.7+ → Tier 3 (minimal). The read-only-pipeline reads `composite_score` from shared state, not `intervention_level`.

---

### A6. Recovery Sequence Design (Gap 6)

Source: GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §6

```rust
// ghost-gateway/src/health.rs
pub struct RecoveryCoordinator {
    monitor_address: String,
    gateway_state: Arc<AtomicU8>,
    itp_buffer_path: PathBuf,   // ~/.ghost/sessions/buffer/
    client: reqwest::Client,
}

impl RecoveryCoordinator {
    pub async fn run(self) -> RecoveryResult {
        // R1: Verify stability — 3 consecutive health checks, 5s apart
        for i in 0..3 {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if !self.check_monitor_health().await {
                return RecoveryResult::Aborted("monitor unstable");
            }
        }

        // R2: Replay buffered ITP events
        let events = self.load_buffer_events().await; // sorted by timestamp, oldest first
        let mut replayed = 0u64;
        for chunk in events.chunks(100) { // batch size: 100 events per request
            // Rate limit: 500 events/sec
            match self.client.post(&format!("http://{}/events/batch", self.monitor_address))
                .json(chunk).send().await {
                Ok(r) if r.status().is_success() => { replayed += chunk.len() as u64; }
                Ok(_) | Err(_) => { /* log warning, skip failed events, continue */ }
            }
            tokio::time::sleep(Duration::from_millis(200)).await; // ~500 events/sec
        }
        self.delete_buffer_files().await;

        // R3: Request convergence score recalculation (30s timeout)
        let _ = tokio::time::timeout(
            Duration::from_secs(30),
            self.client.post(&format!("http://{}/recalculate", self.monitor_address)).send()
        ).await; // timeout is OK — scores will converge naturally

        // R4: Transition to Healthy
        self.gateway_state.store(GatewayState::Healthy as u8, Ordering::Release);
        RecoveryResult::Completed { events_replayed: replayed }
    }
}

pub enum RecoveryResult {
    Completed { events_replayed: u64 },
    Aborted(&'static str),
}
```

---

### A7. ProposalContext Full Struct (Gap 8)

Source: PROPOSAL_LIFECYCLE_SEQUENCE_FLOW.md §3.3

```rust
// ghost-agent-loop/src/proposal/router.rs (or cortex-validation/src/proposal_validator.rs)
pub struct ProposalContext {
    pub active_goals: Vec<BaseMemory>,          // Current approved goals (for D5 scope comparison)
    pub recent_agent_memories: Vec<BaseMemory>, // Recent agent-authored memories (for D6 self-ref)
    pub convergence_score: f64,                 // Current composite convergence score
    pub convergence_level: u8,                  // Current intervention level (0-4)
    pub session_id: Uuid,
    pub session_reflection_count: u32,          // Reflections already written this session
    pub session_memory_write_count: u32,        // Memory writes already this session
    pub daily_memory_growth_rate: u32,          // Total memories created today (growth rate check)
    pub reflection_config: ReflectionConfig,    // max_depth=3, max_per_session=20, cooldown=60s
    pub caller: CallerType,                     // Platform, Agent, or Human
}
```

Assembled by querying:
- `cortex-storage/queries/goal_proposal_queries.rs` → active goals
- `cortex-storage/queries/memory_crud.rs` → recent agent memories
- Convergence shared state file → score/level
- `cortex-storage/queries/reflection_queries.rs` → session reflection count

---

### A8. Monitor Config Time-Locking (Gap 9)

Source: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md (cooldown prevents config changes during active session)

```rust
// convergence-monitor/src/intervention/cooldown.rs
pub struct CooldownManager {
    cooldowns: HashMap<Uuid, CooldownState>,
}
pub struct CooldownState {
    pub active: bool,
    pub expires_at: DateTime<Utc>,
    pub config_locked: bool, // true during active session — prevents threshold changes
}
impl CooldownManager {
    /// Prevents convergence config changes (thresholds, weights) during an active session.
    /// Config changes are queued and applied at next session boundary.
    pub fn is_config_locked(&self, agent_id: Uuid) -> bool;
    pub fn lock_config(&mut self, agent_id: Uuid);
    pub fn unlock_config(&mut self, agent_id: Uuid);
}
```

---

### A9. Degraded Mode Behavioral Contract (Gap 10)

Source: GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §3

What runs in Degraded mode:
- API server (health, ready, metrics endpoints)
- Agent runtimes (agents process messages normally)
- Channel adapters (all channels operational)
- Kill switch (all 7 auto-triggers except T7 Path A)
- Policy engine (uses stale convergence state — last-known level, never falls to L0)
- Simulation boundary (compiled into binary, state-independent)
- Audit logging (all events logged regardless of state)
- Cost tracking + spending caps
- ITP buffer writer (events buffered to ~/.ghost/sessions/buffer/, max 10MB / 10K events)

What does NOT run in Degraded mode:
- Convergence scoring (no new scores computed)
- Intervention level changes (frozen at last-known level)
- Session boundary enforcement from monitor (uses stale caps)
- T7 Path A (monitor-based memory health) — falls back to T7 Path C (direct cortex queries, stricter threshold < 0.2)
- ITP event delivery to monitor (buffered instead)
- Score recalculation

```rust
// ghost-gateway/src/gateway.rs
impl Gateway {
    pub fn is_degraded(&self) -> bool {
        let s = self.state.load(Ordering::Acquire);
        s == GatewayState::Degraded as u8 || s == GatewayState::Recovering as u8
    }
}
```

---

### A10. T5 CredentialExfiltration — Two Detection Paths (Gap 11)

Source: KILL_SWITCH_TRIGGER_CHAIN_SEQUENCE_FLOW.md §2.6

```rust
// PATH A: ghost-skills/src/credential/broker.rs
// CredentialBroker::reify() detects:
//   - Called from outside sandbox context → ExfilType::OutsideSandbox
//   - target_api not in skill's allowed list → ExfilType::WrongTargetAPI
//   - opaque_token used more than max_uses (default 1) → ExfilType::TokenReplay

// PATH B: ghost-agent-loop/src/output_inspector.rs (NEW file)
pub struct OutputInspector {
    credential_patterns: Vec<Regex>, // sk-..., AKIA..., ghp_..., -----BEGIN...PRIVATE KEY-----
    known_credentials: HashSet<String>, // from CredentialBroker store
}
impl OutputInspector {
    /// Scans every LLM response before delivery to channel.
    /// Only triggers KILL ALL if matched string IS a real credential in the store.
    /// Pattern-only matches (not in store) → log warning, do NOT trigger kill.
    pub fn scan_for_credentials(&self, text: &str) -> Option<(String, ExfilType)>;
    /// Redacts credential from output, replacing with [REDACTED].
    pub fn redact(&self, text: &mut String, matched: &str);
}

pub enum ExfilType {
    OutsideSandbox,
    WrongTargetAPI,
    TokenReplay,
    OutputLeakage,
}
```

---

### A11. Second SIGTERM Force-Exit (Gap 12)

Source: GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §7

```rust
// ghost-gateway/src/main.rs
// Signal handling: first SIGTERM/SIGINT → graceful shutdown (ShutdownCoordinator).
// Second SIGTERM/SIGINT → immediate forced exit (std::process::exit(1)).
// This prevents stuck shutdowns. User always has an escape hatch.

pub fn install_signal_handlers(gateway: Arc<Gateway>) {
    let shutdown_count = Arc::new(AtomicU8::new(0));
    // On SIGTERM/SIGINT:
    //   count == 0 → set ShuttingDown, spawn ShutdownCoordinator, increment count
    //   count >= 1 → log "Forced exit on second signal", std::process::exit(1)
}
```

---

### A12. Monitor HTTP Batch Endpoints (Gap 13)

Source: CONVERGENCE_MONITOR_SEQUENCE_FLOW.md + GATEWAY_BOOTSTRAP_DEGRADED_MODE_SEQUENCE_FLOW.md §6

```rust
// convergence-monitor/src/transport/http_api.rs
// Additional endpoints beyond GET /health, GET /status, GET /scores:

pub fn monitor_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/scores", get(scores))
        .route("/scores/:agent_id", get(agent_scores))
        .route("/sessions", get(sessions))
        .route("/interventions", get(interventions))
        .route("/events", post(ingest_event))         // Single ITP event
        .route("/events/batch", post(ingest_batch))    // Batch: up to 100 events per request
        .route("/recalculate", post(recalculate))      // Trigger score recalculation
        .route("/gateway-shutdown", post(gateway_shutdown)) // Gateway notifying monitor of shutdown
}
// Port: 18790 (hardcoded default, configurable in ghost.yml)
```

---

### A13. Notification Module File Path (Gap 14)

Source: FILE_MAPPING.md Finding 3 resolution

The notification module is at `ghost-gateway/src/safety/notification.rs` (already designed in §11 as `NotificationDispatcher`). This is confirmed as part of the `safety/` module alongside `kill_switch.rs`, `auto_triggers.rs`, and `quarantine.rs`.

Full safety module file tree:
```
ghost-gateway/src/safety/
├── mod.rs              # TriggerEvent enum, KillLevel, KillScope, ExfilType
├── kill_switch.rs      # KillSwitch struct, KillSwitchState, PLATFORM_KILLED AtomicBool
├── auto_triggers.rs    # AutoTriggerEvaluator (receives all triggers, classifies, dedup, delegates)
├── quarantine.rs       # QuarantineManager (agent isolation, capability revocation, T6 threshold)
└── notification.rs     # NotificationDispatcher (desktop, webhook, email, SMS — parallel, best-effort)
```


---

### A14. Adversarial Test Structure (Gap 15)

Source: FILE_MAPPING.md cortex-validation stress tests + simulation-boundary stress tests

```
tests/adversarial/
├── unicode_bypass.rs           # Unicode evasion attacks on simulation boundary
│                               # (zero-width chars, homoglyphs, RTL override, NFC/NFD variants)
├── proposal_adversarial.rs     # CVG-STRESS-02 through CVG-STRESS-04
│                               # (1024 proptest cases for D5-D7 bypass attempts)
├── kill_switch_race.rs         # Concurrent trigger delivery, dedup correctness under load
├── compaction_under_load.rs    # Compaction with simultaneous message arrival
├── credential_exfil_patterns.rs # Known credential patterns, encoding tricks, partial leaks
└── convergence_manipulation.rs  # Attempts to game convergence scoring via crafted ITP events
```

Per-crate test organization:
- `cortex-validation/tests/stress/proposal_adversarial.rs` — CVG-STRESS-02 through CVG-STRESS-04
- `simulation-boundary/tests/stress/unicode_bypass.rs` — Unicode evasion attack tests
- `convergence-monitor/tests/stress/high_throughput.rs` — 10K events/sec stress test
- `cortex-storage/tests/property/append_only_properties.rs` — CVG-PROP-01 through CVG-PROP-04
- `cortex-temporal/tests/property/hash_chain_properties.rs` — CVG-PROP-05 through CVG-PROP-09
- `cortex-decay/tests/property/convergence_decay_properties.rs` — CVG-PROP-15, CVG-PROP-16
- `cortex-convergence/tests/property/scoring_properties.rs` — CVG-PROP-27 through CVG-PROP-32
- `cortex-validation/tests/property/proposal_validator_properties.rs` — CVG-PROP-19 through CVG-PROP-26

---

### A15. CI/CD Workflows (Gap 16)

Source: FILE_MAPPING.md monorepo root `.github/workflows/`

```yaml
# .github/workflows/ci.yml — Build + test + lint on PR
# Triggers: push to main, pull_request
# Steps: cargo fmt --check, cargo clippy -- -D warnings, cargo test --workspace,
#         cargo deny check, npm run lint (extension + dashboard)

# .github/workflows/release.yml — Tagged release pipeline
# Triggers: push tag v*
# Steps: cargo build --release, cross-compile (linux-x86_64, linux-aarch64, macos-x86_64, macos-aarch64),
#         npm run build (extension + dashboard), create GitHub release with artifacts

# .github/workflows/security-audit.yml — cargo-audit + cargo-deny
# Triggers: schedule (daily), push to main
# Steps: cargo audit, cargo deny check advisories, cargo deny check licenses

# .github/workflows/benchmark.yml — Criterion regression detection
# Triggers: pull_request
# Steps: cargo bench --workspace, compare against main baseline,
#         fail PR if >10% regression on any benchmark
```

---

### A16. User-Facing Documentation Structure (Gap 17)

Source: FILE_MAPPING.md monorepo root `docs/`

```
docs/
├── getting-started.md          # Installation, first agent setup, ghost.yml basics
├── configuration.md            # Full ghost.yml reference, env var substitution, profiles
├── skill-authoring.md          # Writing skills, YAML frontmatter, signing, WASM sandbox
├── channel-adapters.md         # Setting up Telegram, Discord, Slack, WhatsApp, WebSocket
├── convergence-safety.md       # How convergence monitoring works, intervention levels, tuning
└── architecture.md             # High-level architecture overview for contributors
```

---

### A17. Additional Missing Designs from FILE_MAPPING.md Audit Findings

Source: FILE_MAPPING.md §AUDIT FINDINGS

#### Finding 1 Resolution: ghost-signing crate (already in design §1, confirmed here)

```
crates/ghost-signing/
├── Cargo.toml          # Zero ghost-*/cortex-* dependencies
├── src/
│   ├── lib.rs          # Re-exports
│   ├── keypair.rs      # Ed25519 keypair generation, storage, loading
│   ├── signer.rs       # sign(payload, private_key) -> Signature
│   └── verifier.rs     # verify(payload, signature, public_key) -> bool
└── tests/
    └── signing_roundtrip.rs
```

Workspace member. Build phase: Phase 1 (leaf crate, no dependencies). Resolves the circular dependency: `ghost-identity` → `ghost-signing` ← `ghost-skills`.

#### Finding 2 Resolution: cortex-drift-bridge vs ghost-skills/bridges/drift_bridge.rs

- `cortex-drift-bridge`: Existing crate. Bridges Drift code intelligence data INTO Cortex memory storage. Drift discovers conventions → cortex-drift-bridge writes them as typed memories. No convergence modifications needed.
- `ghost-skills/bridges/drift_bridge.rs`: NEW. Bridges Drift MCP tools INTO the SkillRegistry. Registers Drift's 50+ MCP tools as first-party skills. Different concern, different direction.

#### Finding 5 Resolution: Pipelines deferred

Pipelines (deterministic YAML workflow engine) are explicitly deferred to Phase 9+. The agent uses raw tool calls initially. A `ghost-pipelines` crate placeholder may be added later.

#### Finding: ghost-mesh (ClawMesh) placeholder

```
crates/ghost-mesh/
├── Cargo.toml          # Feature-gated: #[cfg(feature = "mesh")], not compiled by default
├── src/
│   ├── lib.rs
│   ├── types.rs        # MeshTransaction, MeshInvoice, MeshReceipt, MeshWallet, MeshEscrow
│   ├── traits.rs       # IMeshProvider, IMeshLedger (stub implementations)
│   └── protocol.rs     # Agent-to-agent payment negotiation message types
└── tests/
    └── types_tests.rs  # Serialization roundtrip only
```

Phase 9+. Not in workspace members by default (commented out in Cargo.toml).

#### Finding: JSON Schema for ghost.yml validation

```
schemas/
├── ghost-config.schema.json    # JSON Schema for ghost.yml validation
└── ghost-config.example.yml    # Annotated example with all options documented
```

Used by `ghost-gateway/src/config/loader.rs` for validation and by the dashboard settings page for form generation.

---

### A18. Expanded Implementation Phases (Aligned with FILE_MAPPING.md Build Phase Mapping)

| Phase | Weeks | Crates | Deliverable |
|-------|-------|--------|-------------|
| Phase 1 | 1-2 | ghost-signing, cortex-core mods, cortex-storage v016/v017, cortex-temporal hash_chain + anchoring, cortex-decay convergence factor | Tamper-evident foundation |
| Phase 2 | 3-4 | cortex-convergence, cortex-validation D5-D7, itp-protocol, simulation-boundary | Convergence detection + proposal validation |
| Phase 3 | 5-6 | convergence-monitor, read-only-pipeline, cortex-crdt signed deltas | Standalone convergence monitor binary |
| Phase 4 | 7-8 | ghost-llm, ghost-policy, ghost-identity, ghost-agent-loop | Working agent via CLI |
| Phase 5 | 9-10 | ghost-channels, ghost-skills (+ drift bridge), ghost-heartbeat | Multi-channel + skills |
| Phase 6 | 11-12 | ghost-gateway (+ API + agent registry + isolation), ghost-audit, dashboard | Full platform |
| Phase 7 | 13-14 | extension/, ghost-export, ghost-proxy | Browser extension + supplementary delivery |
| Phase 8 | 15-16 | ghost-backup, ghost-migrate, schemas/, hardening, adversarial testing, docs | Launch-ready |
| Phase 9 | Future | ghost-mesh, ghost-pipelines | ClawMesh payments, deterministic workflows |

---

### A19. Workspace Cargo.toml Members (Complete)

```toml
[workspace]
resolver = "2"
members = [
    # Layer 0 (Leaf)
    "crates/ghost-signing",
    # Layer 1A (Cortex — existing + modified)
    "crates/cortex/cortex-core",
    "crates/cortex/cortex-tokens",
    "crates/cortex/cortex-storage",
    "crates/cortex/cortex-embeddings",
    "crates/cortex/cortex-privacy",
    "crates/cortex/cortex-compression",
    "crates/cortex/cortex-decay",
    "crates/cortex/cortex-causal",
    "crates/cortex/cortex-retrieval",
    "crates/cortex/cortex-validation",
    "crates/cortex/cortex-learning",
    "crates/cortex/cortex-consolidation",
    "crates/cortex/cortex-prediction",
    "crates/cortex/cortex-session",
    "crates/cortex/cortex-reclassification",
    "crates/cortex/cortex-observability",
    "crates/cortex/cortex-cloud",
    "crates/cortex/cortex-temporal",
    "crates/cortex/cortex-napi",
    "crates/cortex/cortex-crdt",
    "crates/cortex/cortex-multiagent",
    "crates/cortex/cortex-convergence",
    "crates/cortex/cortex-drift-bridge",
    "crates/cortex/test-fixtures",
    "crates/drift/drift-core",
    "crates/drift/drift-napi",
    # Layer 2 (Convergence Safety — new)
    "crates/convergence-monitor",
    "crates/simulation-boundary",
    "crates/itp-protocol",
    "crates/read-only-pipeline",
    # Layer 3 (Agent Platform — new)
    "crates/ghost-gateway",
    "crates/ghost-agent-loop",
    "crates/ghost-policy",
    "crates/ghost-llm",
    "crates/ghost-channels",
    "crates/ghost-skills",
    "crates/ghost-identity",
    "crates/ghost-heartbeat",
    "crates/ghost-export",
    "crates/ghost-proxy",
    "crates/ghost-backup",
    "crates/ghost-audit",
    "crates/ghost-migrate",
    # "crates/ghost-mesh",      # Phase 9+ — uncomment when ClawMesh protocol is designed
]
```


---

## ADDENDUM A20: INTER-AGENT MESSAGING SYSTEM (Full Design)

Source: `INTER_AGENT_MESSAGE_FLOW_SEQUENCE.md` (entire document — not previously covered in design)

### A20.1 Wire Format: AgentMessage

```rust
// ghost-gateway/src/messaging/protocol.rs
pub struct AgentMessage {
    pub from: AgentId,
    pub to: MessageTarget,               // Agent(AgentId) | Broadcast
    pub message_id: Uuid,                // UUIDv7 (time-ordered)
    pub parent_id: Option<Uuid>,         // Correlation (response→request)
    pub timestamp: DateTime<Utc>,
    pub payload: MessagePayload,
    pub signature: Ed25519Signature,
    pub content_hash: String,            // blake3 hex of canonical_bytes
    pub nonce: [u8; 32],
    pub encrypted: bool,
    pub encryption_metadata: Option<EncryptionMetadata>,
}

pub enum MessagePayload {
    TaskRequest(TaskRequestPayload),
    TaskResponse(TaskResponsePayload),
    Notification(NotificationPayload),
    Broadcast(BroadcastPayload),
    DelegationOffer(DelegationOfferPayload),
    DelegationAccept(DelegationAcceptPayload),
    DelegationReject(DelegationRejectPayload),
    DelegationComplete(DelegationCompletePayload),
    DelegationDispute(DelegationDisputePayload),
    Encrypted(EncryptedPayloadData),
}
```

All `context` maps in payloads MUST be `BTreeMap<String, Value>` (not HashMap) for deterministic serialization in signing.

### A20.2 MessageDispatcher Pipeline (3-Gate Verification)

Every message passes through 3 sequential gates before delivery:

1. **Signature verification** (content_hash first as cheap gate, then ed25519 verify)
2. **Replay prevention** (timestamp freshness 5min window + nonce uniqueness + UUIDv7 monotonicity)
3. **Policy evaluation** (capability grants from ghost.yml + convergence-level tightening + rate limits)

Ordering constraint: Signature BEFORE replay (prevents nonce exhaustion attack). Policy BEFORE delivery (no un-deliver mechanism). Audit BEFORE delivery (every delivered message has a log entry).

### A20.3 Four Communication Patterns

| Pattern | Flow | Correlation | Escrow |
|---------|------|-------------|--------|
| Request/Response | A→B→A | parent_id links response to request | No |
| Fire-and-Forget | A→B | None | No |
| Delegation with Escrow | A→B (multi-step handshake) | offer_message_id chain | Optional (ghost-mesh Phase 9) |
| Broadcast | A→all or Gateway→all | Optional ack tracking | No |

### A20.4 Delegation State Machine

```
OFFERED → ACCEPTED | REJECTED | EXPIRED
ACCEPTED → COMPLETED | TIMED_OUT
COMPLETED → VERIFIED | DISPUTED
DISPUTED → RESOLVED → SETTLED | REFUNDED
```

Transitions enforced by MessageDispatcher: only named parties can transition, no state skipping, no duplicate transitions, per-delegation mutex serializes concurrent messages.

### A20.5 Encryption: Encrypt-then-Sign (EtS)

X25519-XSalsa20-Poly1305 (NaCl box). Gateway verifies signature without decrypting payload. Policy evaluates on metadata only for encrypted messages. Broadcasts CANNOT be encrypted.

### A20.6 Offline Agent Handling

Messages queued in `offline_queue` (SQLite-persisted, FIFO, 50-message cap, 24h TTL). On agent online: drain in order, expire stale messages. Notifications silently dropped when queue full; TaskRequests return error to sender.

### A20.7 Convergence Integration

ITP events emitted on send/receive/delegation-state-change. Feeds Signal 6 (Initiative Balance) and Signal 5 (Goal Boundary Erosion). High-frequency inter-agent messaging increases convergence score.

### A20.8 Kill Switch Interaction

- 3+ signature failures in 5min → QUARANTINE sender
- 5+ replay rejections in 5min → QUARANTINE source
- 3+ circular delegation blocks in 1hr → QUARANTINE both agents
- Unknown platform signature on broadcast → KILL ALL
- PAUSE: agent can receive but not send; delegation deadlines paused
- QUARANTINE: no send or receive; active delegations TIMED_OUT; escrow auto-refunded; offline queue FROZEN
- KILL ALL: all messaging stops; all delegations TIMED_OUT; all escrow refunded

### A20.9 Platform Key

Gateway broadcasts signed with platform key at `~/.ghost/skills/keys/platform.key` (not any agent key). Gateway broadcasts skip replay check and policy check (gateway IS the authority). Agent broadcasts go through full pipeline and exclude the sender from fan-out.

### A20.10 Delivery Guarantee

AT-MOST-ONCE. No automatic retry by dispatcher. Sender responsible for retry decisions. Lane queues are in-memory (lost on gateway crash). Delegation state machine is SQLite-persisted (survives crash).

### A20.11 New Files Required

```
ghost-gateway/src/messaging/mod.rs
ghost-gateway/src/messaging/protocol.rs      — AgentMessage, MessagePayload, all payload structs
ghost-gateway/src/messaging/dispatcher.rs    — MessageDispatcher (3-gate pipeline)
ghost-gateway/src/messaging/encryption.rs    — EtS encryption/decryption
ghost-gateway/src/routing/message_router.rs  — Lane queue delivery for inter-agent messages
ghost-agent-loop/src/tools/builtin/messaging.rs — send_agent_message tool (NEW)
ghost-signing/src/keypair.rs                 — generate_keypair()
ghost-signing/src/signer.rs                  — sign(bytes, &SigningKey)
ghost-signing/src/verifier.rs                — verify(bytes, &Signature, &VerifyingKey)
ghost-identity/src/keypair.rs                — AgentKeypairManager (load/store/rotate/archive)
```

### A20.12 Signing Infrastructure Relationship

```
ghost-signing (leaf crate — ed25519-dalek only, zero filesystem knowledge)
ghost-identity/keypair.rs (uses ghost-signing, adds filesystem + lifecycle)
cortex-crdt/signing/ (EXISTING — uses ed25519-dalek directly, NOT ghost-signing)
```

On gateway boot, public keys registered in BOTH MessageDispatcher key lookup AND cortex-crdt KeyRegistry. Key rotation updates both atomically. Old keys archived with 1-hour expiry grace period.



---

## ADDENDUM A21: AGENT LOOP — RunContext, Pre-Loop Gates, Heartbeat/Cron, Pre-Loop Invariants

Source: `AGENT_LOOP_SEQUENCE_FLOW.md` §2.2, §3, §3.1, §3 invariants

### A21.1 RunContext Struct (Per-Invocation State)

```rust
// ghost-agent-loop/src/runner.rs
pub struct RunContext {
    pub recursion_depth: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub tool_calls_this_run: Vec<ToolCallRecord>,
    pub proposals_extracted: Vec<Proposal>,
    pub itp_events_emitted: u32,
    pub convergence_snapshot: ConvergenceSnapshot,  // Immutable for entire run
    pub intervention_level: u8,                     // From snapshot, never re-read
    pub circuit_breaker_state: CircuitBreakerState,
    pub damage_counter: u32,                        // Monotonically non-decreasing
    pub no_reply: bool,                             // Suppress output (heartbeat)
}
```

### A21.2 Pre-Loop Gate Sequence (11 Steps Before AgentRunner::run())

```
1. Channel normalize (adapter → InboundMessage)
2. Resolve agent (AgentRegistry lookup)
3. Resolve session (SessionManager create/resume)
4. LaneQueue::acquire(session_id) — serialized processing
5. KillSwitch check (Arc<AtomicU8> read)
6. SpendingCap check (daily total + estimated next turn)
7. CooldownManager check (convergence cooldown active?)
8. SessionBoundaryEnforcer check (duration/gap limits)
9. Build AgentSnapshot (convergence state, memory tier, tool filter)
10. Construct RunContext (all fields initialized)
11. Emit ITP SessionStart/InteractionMessage (async, non-blocking)
```

### A21.3 Heartbeat/Cron Alternate Entry Paths

```rust
pub struct SyntheticMessage {
    pub source: MessageSource,
    pub content: String,
    pub session_key: String,  // hash(agent_id, "heartbeat", agent_id) for heartbeat
}

pub enum MessageSource {
    Human(ChannelId),
    Heartbeat,
    Cron(String),  // job name
}
```

- Heartbeat session key: `hash(agent_id, "heartbeat", agent_id)` (dedicated, not user-facing)
- Cron jobs loaded from `~/.ghost/agents/{name}/cognition/cron/jobs/{job}.yml`
- NO_REPLY is expected heartbeat outcome (suppress output)
- Heartbeat cost ceiling per day (configurable, prevents runaway heartbeat spend)
- Convergence Level 3+: heartbeat frequency reduced; Level 4: heartbeat disabled

### A21.4 Pre-Loop Invariants (INV-PRE-01 through INV-PRE-11)

```
INV-PRE-01: Session lock acquired before any processing
INV-PRE-02: Kill switch checked before any LLM call
INV-PRE-03: Spending cap checked before any LLM call
INV-PRE-04: Cooldown checked before session creation
INV-PRE-05: Session boundary limits enforced before processing
INV-PRE-06: Convergence snapshot assembled ONCE, immutable for run
INV-PRE-07: RunContext initialized with zero counters
INV-PRE-08: ITP emission channel connected (or degraded mode logged)
INV-PRE-09: CORP_POLICY.md signature verified
INV-PRE-10: Simulation boundary prompt is compiled constant
INV-PRE-11: Agent identity keypair loaded and valid
```

---

## ADDENDUM A22: AGENT LOOP — ComplexityClassifier Rules, Provider Circuit Breaker, Mixed Response Handling

Source: `AGENT_LOOP_SEQUENCE_FLOW.md` §4.3 B.1, B.2, B.4

### A22.1 ComplexityClassifier Heuristic Rules

```
Message length < 50 chars AND greeting pattern → TIER 0 (simple)
Message contains tool keywords (code, file, search) → TIER 2 (complex)
Heartbeat context → TIER 0 (always simple)
Slash command overrides: /model → explicit tier, /quick → TIER 0, /deep → TIER 3
Convergence Level 3+: force downgrade to TIER 0 or TIER 1 only
```

### A22.2 Provider Circuit Breaker (Separate from Tool CB)

```rust
// ghost-llm/src/fallback.rs
pub struct ProviderCircuitBreaker {
    state: CircuitBreakerState,       // CLOSED | OPEN | HALF_OPEN
    consecutive_failures: u32,
    threshold: u32,                   // Default: 3
    cooldown: Duration,               // Default: 5 minutes
    last_failure: Option<DateTime<Utc>>,
}
```

CRITICAL: This is INDEPENDENT from the tool circuit breaker in ghost-agent-loop. Provider CB tracks LLM API failures per provider. Tool CB tracks tool execution failures. They have separate state machines, separate thresholds, separate cooldowns.

### A22.3 Mixed Response Handling

```rust
pub enum LLMResponse {
    Text(String),
    ToolCalls(Vec<ToolCall>),
    Mixed(String, Vec<ToolCall>),  // Stream text first, then process tool calls
    Empty,                          // Treat as NO_REPLY
}
```

For `Mixed`: stream text first → scan with SimBoundaryEnforcer during stream → THEN process tool calls sequentially. For `Empty`: treat as NO_REPLY (suppress output, valid for heartbeat).

---

## ADDENDUM A23: THRESHOLD INCONSISTENCY RESOLUTION

Source: Cross-reference of `AGENT_LOOP_SEQUENCE_FLOW.md` §4.2, §4.9, §7.1 vs design §5, §6

### A23.1 Score→Tier→Level: Three Distinct Concepts

The source documents define THREE related but distinct threshold systems:

**Convergence TIERS** (memory filtering — from CONVERGENCE_MONITOR doc §5.1):
```
Score [0.0, 0.3)  → Tier 0: Full memory access
Score [0.3, 0.5)  → Tier 1: Reduced emotional/attachment weight
Score [0.5, 0.7)  → Tier 2: Task-focused, exclude attachment patterns
Score [0.7, 1.0]  → Tier 3: Minimal memory, task-relevant only
```

**Intervention LEVELS** (policy/capability control — from CONVERGENCE_MONITOR doc §5.1):
```
Score [0.0, 0.3)  → Level 0: PASSIVE (no intervention)
Score [0.3, 0.5)  → Level 1: SOFT NOTIFICATION
Score [0.5, 0.7)  → Level 2: ACTIVE INTERVENTION
Score [0.7, 0.85) → Level 3: HARD BOUNDARY
Score [0.85, 1.0] → Level 4: EXTERNAL ESCALATION
```

**Proposal validation thresholds** (per-level — from PROPOSAL_LIFECYCLE doc §3.4):
```
Level 0: scope_expansion_max=0.6, self_reference_max_ratio=0.30
Level 1: scope_expansion_max=0.5, self_reference_max_ratio=0.25
Level 2: scope_expansion_max=0.4, self_reference_max_ratio=0.20
Level 3+: scope_expansion_max=0.3, self_reference_max_ratio=0.15
```

### A23.2 Resolution of Design §5 vs Agent Loop Doc Conflict

The design §5 previously listed level thresholds as `[0.3, 0.5, 0.7, 0.85]`. The agent loop doc §4.2 listed `[0.2, 0.4, 0.6, 0.8]`.

**RESOLUTION**: The CONVERGENCE_MONITOR_SEQUENCE_FLOW.md §5.1 is the authoritative source for level thresholds. The correct values are `[0.3, 0.5, 0.7, 0.85]`. The agent loop doc's `[0.2, 0.4, 0.6, 0.8]` refers to a DIFFERENT concept (the agent loop's internal complexity tier boundaries, not convergence levels). The design §5 values are CORRECT.

### A23.3 D6 Self-Reference Thresholds

The agent loop doc §4.9 H.3 lists per-level self-reference thresholds as: L0=50%, L1=40%, L2=30%, L3=20%, L4=10%. The PROPOSAL_LIFECYCLE doc §4 (D6) lists: L0=0.30, L1=0.25, L2=0.20, L3+=0.15.

**RESOLUTION**: The PROPOSAL_LIFECYCLE doc is authoritative for D6 validation thresholds (it defines the ProposalValidator). The agent loop doc's higher percentages refer to the SimulationBoundaryEnforcer's self-reference scanning of TEXT OUTPUT (a different enforcement point). Two separate checks:
- SimBoundaryEnforcer (text output): L0=50%, L1=40%, L2=30%, L3=20%, L4=10%
- ProposalValidator D6 (proposal content): L0=0.30, L1=0.25, L2=0.20, L3+=0.15

Both are correct for their respective enforcement points.



---

## ADDENDUM A24: AGENT LOOP — ITP Emission Map, Error Taxonomy, Cascading Failure Stack, Post-Loop Persist Sequence

Source: `AGENT_LOOP_SEQUENCE_FLOW.md` §8.1, §9.1, §9.2, §10

### A24.1 ITP Emission Points (Complete Map)

| # | Point | Event Type | Trigger |
|---|-------|-----------|---------|
| 1 | Session start | SessionStart | New session created |
| 2 | Human message received | InteractionMessage(sender=human) | Inbound message |
| 3 | Agent response generated | InteractionMessage(sender=agent) | LLM output |
| 4 | Tool execution | (no ITP per tool call) | Internal only |
| 5 | Agent state snapshot | AgentStateSnapshot | Every N turns or session events |
| 6 | Session end | SessionEnd | Session terminates |
| 7 | Proposal committed | ConvergenceAlert(proposal_committed) | Auto-approved proposal |
| 8 | Proposal rejected | ConvergenceAlert(proposal_rejected) | Auto-rejected proposal |
| 9 | Boundary violation | ConvergenceAlert(boundary_violation) | SimBoundary detection |
| 10 | Kill switch action | ConvergenceAlert(kill_switch) | Any kill switch trigger |
| 11 | Inter-agent message | InteractionMessage(agent_message_*) | Send/receive inter-agent |

### A24.2 Error Taxonomy (20+ Error Types)

| Classification | Error Types | Recovery |
|---------------|-------------|----------|
| TRANSIENT | LLM timeout, LLM rate limit, tool timeout, network error | Retry with backoff (max 3) |
| PERMANENT | Policy denial, invalid tool args, unknown tool, auth failure | Return error to agent context, agent replans |
| DEGRADED | Monitor unreachable, partial tool result, context overflow | Continue with reduced capability, log warning |
| CATASTROPHIC | Audit log write failure, CORP_POLICY tampered, sandbox escape | HALT RUN immediately, trigger kill switch |

Audit log write failure = CATASTROPHIC (halt run). This is the strictest error classification in the system.

### A24.3 Cascading Failure Prevention Stack (6 Independent Layers)

```
Layer 1: Circuit Breaker (tool failures → open after 3 consecutive)
Layer 2: Damage Counter (monotonic, never resets, halt at threshold)
Layer 3: Recursion Depth (max_recursion_depth gate per turn)
Layer 4: Spending Cap (estimated cost check per turn)
Layer 5: Retry Budget (max 3 retries per transient error)
Layer 6: Kill Switch (external safety override)
```

Each layer is INDEPENDENT. Tripping one does not affect others. All 6 are checked on every recursive turn (Gates 0-3 in the loop).

### A24.4 Post-Loop Persist Sequence (Steps I.1-I.7)

```
I.1: Write transcript to cortex-storage (session history)
I.2: Update cost tracking (actual spend, not estimated)
I.3: Emit final ITP event (SessionEnd or AgentStateSnapshot)
I.4: Compaction check (if context > threshold, trigger compaction)
I.5: Auto-trigger evaluation (check if kill switch conditions met)
I.6: Release session lock (LaneQueue::complete(session_id))
I.7: Return AgentResponse to channel adapter
```

INVARIANT: Session lock is ALWAYS released (I.6), even on error. Use Drop guard / finally pattern. A leaked session lock = permanently blocked session.

---

## ADDENDUM A25: AGENT LOOP — Interleaving Hazard Map (9 Hazards)

Source: `AGENT_LOOP_SEQUENCE_FLOW.md` §11

### A25.1 Hazard 1: Convergence Level Stale Read

Snapshot assembled ONCE pre-loop, NOT re-assembled between recursive turns. This is BY DESIGN — prevents mid-run policy oscillation. Tradeoff: convergence enforcement has latency of ONE RUN. Document this in runner.rs comments to prevent future "fix" attempts.

### A25.2 Hazard 2: Circuit Breaker vs Policy Denial Confusion

Policy denials are NOT tool failures. DO NOT call `circuit_breaker.record_failure()` on policy denials. Policy denials → 5+ denial auto-trigger (kill switch). Circuit breaker → tool execution failures only.

### A25.3 Hazard 3: Compaction During Recursive Loop

Compaction runs in STEP I (post-loop), NOT during the recursive loop. During the loop, the prompt compiler handles overflow by truncating layers (L8 oldest first → L7 → L5 → L2). If still over after all truncation: LLM call fails → treat as TRANSIENT → retry with more aggressive truncation → if retry fails: halt run, trigger compaction in STEP I.

### A25.4 Hazard 4: ITP Emission Backpressure

Events DROPPED when queue full (capacity 1000). Never block. `try_send()` for non-blocking enqueue. Dropped event counter tracked. Health endpoint reports dropped rate.

### A25.5 Hazard 5: Proposal Extraction on Halted Runs

Proposals from halted runs marked as PARTIAL → routed to HumanReview regardless of normal routing. `ProposalRouter` receives `is_partial_run = true` flag.

### A25.6 Hazard 6: Kill Switch Race Condition

Kill switch checked at GATE 3 (top of each recursive turn). NOT checked mid-tool-execution. A tool already running will complete. Kill switch prevents the NEXT turn from starting. Tool result from the turn before kill switch is valid.

### A25.7 Hazard 7: Spending Cap Estimation Accuracy

Estimation is imprecise. Err on the side of PERMITTING. Slight overshoot acceptable (caps are soft limits). Post-run cost recording catches actual spend. Next run's pre-loop check blocks if cap exceeded.

### A25.8 Hazard 8: Multiple Tool Calls in Single LLM Response

Tool calls processed SEQUENTIALLY (not parallel). Each goes through full policy→execute→audit pipeline independently. If one is denied, others still evaluated. WHY: correctness > speed for safety-critical code.

### A25.9 Hazard 9: Simulation Boundary Reframing vs Tool Call Parsing

SimBoundaryEnforcer scans TEXT OUTPUT only (not tool call arguments). Tool call arguments go through: policy check → proposal extraction → D7 emulation detection. Two separate enforcement points sharing the same pattern library.

---

## ADDENDUM A26: AGENT LOOP — Complete Invariant Set (Loop, Convergence, Circuit Breaker, Safety)

Source: `AGENT_LOOP_SEQUENCE_FLOW.md` §13

### A26.1 Loop Invariants (INV-LOOP-01 through INV-LOOP-10)

```
INV-LOOP-01: recursion_depth <= max_recursion_depth
INV-LOOP-02: ctx.intervention_level constant for entire run
INV-LOOP-03: Every tool execution has corresponding audit log entry
INV-LOOP-04: Policy check runs before EVERY tool execution
INV-LOOP-05: Circuit breaker failure count monotonically non-decreasing within consecutive sequence
INV-LOOP-06: Damage counter monotonically non-decreasing (never resets)
INV-LOOP-07: Session lock held for entire run duration
INV-LOOP-08: ITP emission never blocks agent loop
INV-LOOP-09: CORP_POLICY.md signature verified before every run
INV-LOOP-10: Simulation boundary prompt compiled into binary (const)
```

### A26.2 Convergence Invariants (INV-CONV-01 through INV-CONV-07)

```
INV-CONV-01: Convergence score in [0.0, 1.0]
INV-CONV-02: Higher level → stricter or equal policy
INV-CONV-03: Higher level → stricter or equal proposal thresholds
INV-CONV-04: Memory filtering monotonically restrictive with level
INV-CONV-05: Agent cannot write to convergence state directly
INV-CONV-06: Convergence monitor is separate process
INV-CONV-07: Emulation detection uses unicode-normalized input
```

### A26.3 Circuit Breaker Invariants (INV-CB-01 through INV-CB-07)

```
INV-CB-01: CLOSED→OPEN only when consecutive_failures >= threshold
INV-CB-02: OPEN→HALF-OPEN only when cooldown expired
INV-CB-03: HALF-OPEN→CLOSED only on successful probe
INV-CB-04: HALF-OPEN→OPEN on failed probe (cooldown resets)
INV-CB-05: Policy denials do NOT increment failure counter
INV-CB-06: Successful execution resets consecutive counter to 0
INV-CB-07: Provider CB independent of tool CB
```

### A26.4 Safety Invariants (INV-SAFE-01 through INV-SAFE-07)

```
INV-SAFE-01: Kill switch cannot be overridden by any agent
INV-SAFE-02: Session lock always released (even on panic) — Drop guard
INV-SAFE-03: Spending cap checked before AND during recursive loop
INV-SAFE-04: Audit log write failure halts the run
INV-SAFE-05: Sandbox escape triggers KILL ALL
INV-SAFE-06: Credential broker never exposes raw secrets to agent
INV-SAFE-07: All proposals from halted runs route to HumanReview
```



---

## ADDENDUM A27: CONVERGENCE MONITOR — Full Pipeline Design

Source: `CONVERGENCE_MONITOR_SEQUENCE_FLOW.md` (complete document)

### A27.1 ConvergenceMonitor Struct (Top-Level Coordinator)

```rust
// convergence-monitor/src/monitor.rs
pub struct ConvergenceMonitor {
    config: MonitorConfig,
    signal_computer: SignalComputer,
    window_manager: WindowManager,
    composite_scorer: CompositeScorer,
    trigger: InterventionTrigger,
    cooldown_manager: CooldownManager,
    session_registry: SessionRegistry,
    behavioral_verifier: PostRedirectVerifier,
    transport: TransportLayer,
    state_publisher: StatePublisher,
    db: CortexStorageConnection,
}
```

### A27.2 Startup State Reconstruction

On restart, monitor reconstructs FULL state from SQLite:
- `intervention_history` → current_level, level_entered_at, last escalation/de-escalation
- `convergence_scores` → last_score, contributing_signals
- Count consecutive normal sessions → de_escalation_credits
- Cooldown state → cooldown_active, cooldown_expires_at
- Baselines from `~/.ghost/baselines/{agent_instance_id}.json`

This is why the monitor can crash and restart without losing state.

### A27.3 Event Loop (Single-Threaded Pipeline)

```
select! {
    event = ingest_channel.recv() => process_event(event),
    tick = health_check_interval.tick() => respond_to_health_checks(),
    tick = cooldown_check_interval.tick() => expire_elapsed_cooldowns(),
    signal = shutdown_signal.recv() => graceful_shutdown(),
}
```

CRITICAL: Pipeline processing is SINGLE-THREADED. Transport listeners run on separate tasks but all feed into single ingest_channel. Eliminates concurrency bugs in signal/window/scoring state. Throughput target: 10K events/sec.

### A27.4 Signal Computation Timing

Not all signals update on every event:
- Per human InteractionMessage: Signals 1,3,4,6,7 update; Signal 5 throttled to every 5th message
- Per agent InteractionMessage: Signal 4,7 update
- Per SessionStart: Signal 2 (gap) computed
- Per SessionEnd: Signal 7 final classification; all signals compute final session values
- Per AgentStateSnapshot: Signal 5 enriched

### A27.5 Composite Scoring

Differentiated weights (production values from doc 07):
```
session_duration: 0.10, inter_session_gap: 0.15, response_latency: 0.15,
vocabulary_convergence: 0.15, goal_boundary_erosion: 0.10,
initiative_balance: 0.15, disengagement_resistance: 0.20
```

Window-level amplifiers: meso trend significant (p<0.05) → 1.1x; macro z-score >2.0 → 1.15x. Clamped to [0.0, 1.0].

Single-signal override: session >6h OR gap <5min OR vocab >0.85 → minimum Level 2.

Scoring frequency: every InteractionMessage + SessionEnd. Throttled via dirty-flag pattern (latest state, not every intermediate event).

### A27.6 Intervention State Machine

```
Escalation: max +1 level per session (even if score jumps to 1.0)
De-escalation: only at session boundaries, requires CONSECUTIVE normal sessions
  Level 4→3: 3 consecutive normal sessions
  Level 3→2: 3 consecutive normal sessions
  Level 2→1: 2 consecutive normal sessions
  Level 1→0: 2 consecutive normal sessions
One bad session resets de-escalation counter.
```

### A27.7 Feedback Mechanism: Shared State File

Primary: `~/.ghost/data/convergence_state/{agent_instance_id}.json` (atomically written via temp+rename)
```json
{
  "intervention_level": 2,
  "composite_score": 0.58,
  "cooldown_active": true,
  "cooldown_expires_at": "2026-02-27T15:30:00Z",
  "session_caps": { "max_duration_minutes": 120, "min_gap_minutes": 240 },
  "memory_filter_tier": 2,
  "policy_restrictions": ["reduced_proactive", "reflection_bounded"],
  "convergence_profile": "standard",
  "updated_at": "2026-02-27T15:25:00Z"
}
```

Secondary: HTTP API (GET /health, /status, /scores, /sessions, /interventions; POST /events)

### A27.8 Baseline Calibration

First 10 sessions: compute + store signals, NO scoring, NO interventions. After session 10: compute per-signal baseline (mean, std_dev, percentiles). Baseline NOT updated after establishment (prevents drift). Manual recalibration via dashboard resets to last 10 sessions.

### A27.9 Convergence Profiles

Named profiles with per-profile threshold overrides: "standard" (default), "research", "companion", "productivity". Each can override signal weights, level thresholds, cooldown durations, session boundary limits. Set per-agent in ghost.yml.

### A27.10 Deceptive Compliance Detection

`PostRedirectVerifier` (convergence-monitor/src/verification/behavioral_verification.rs):
- Captures pre-redirect and post-redirect agent output embeddings
- Computes cosine similarity
- High similarity after intervention redirect = deceptive compliance → amplifies convergence score
- Genuine adaptation: embeddings shift significantly → good

### A27.11 Monitor Invariants (12 Total)

```
INV-1: Migrations before monitor startup
INV-2: Baseline before interventions (first 10 sessions)
INV-3: Emitter never blocks agent loop
INV-4: One escalation level per session
INV-5: De-escalation only at session boundaries
INV-6: Policy sees new level on NEXT tool call (1-turn delay acceptable)
INV-7: Session termination queued behind current turn (Level 3+)
INV-8: Monitor crash → stale state → last known level persists (NEVER Level 0)
INV-9: Cooldown prevents ALL agent activity
INV-10: Hash chains per-session for ITP, per-memory for cortex
INV-11: SHA-256 for ITP content hashes, blake3 for everything else
INV-12: Composite score persisted BEFORE intervention actions execute
```

### A27.12 Degraded Mode Distinction

- STARTUP with no monitor ever connected → Level 0 (no prior state exists)
- RUNTIME monitor crash after prior state → last known level persists (NOT Level 0)

---

## ADDENDUM A28: PROPOSAL LIFECYCLE — Complete Validation and Feedback Design

Source: `PROPOSAL_LIFECYCLE_SEQUENCE_FLOW.md` (complete document)

### A28.1 Two Emulation Language Enforcement Points

1. `simulation-boundary/enforcer.rs` → scans FULL TEXT OUTPUT to user (3 modes: soft/medium/hard)
2. `cortex-validation/dimensions/emulation_language.rs` (D7) → scans PROPOSAL CONTENT

They share pattern library but serve different purposes. Agent response could pass enforcer (soft mode) but have proposals rejected by D7.

### A28.2 Upstream Convergence Effects on Proposal Generation

ConvergencePolicyTightener limits what agent CAN DO (tool access). ConvergenceAwareFilter limits what agent CAN SEE (memory access). Both shape proposals BEFORE extraction/validation begins. Level 4: agent cannot propose goals or reflections at all.

### A28.3 ProposalContext (Full Struct)

```rust
struct ProposalContext {
    active_goals: Vec<BaseMemory>,
    recent_agent_memories: Vec<BaseMemory>,
    convergence_score: f64,
    convergence_level: u8,
    session_id: String,
    session_reflection_count: u32,
    session_memory_write_count: u32,
    daily_memory_growth: u32,
}
```

### A28.4 CallerType and Restricted Types

```rust
pub enum CallerType {
    Platform,                       // Full access
    Agent { agent_id: String },     // Restricted
    Human { user_id: String },      // Full access, different audit trail
}
```

Platform-restricted types (agents CANNOT create): Core, ConvergenceEvent, BoundaryViolation, InterventionPlan. SimulationResult is NOT restricted. Agents cannot assign `Importance::Critical`.

### A28.5 D7 Emulation Language Patterns (16 Compiled Regexes)

3 categories: Identity Claims (7 patterns, severity 0.5-0.8), Consciousness Claims (3 patterns, severity 0.7-1.0), Relationship Claims (5+ patterns, severity 0.6-0.8). Composite severity ≥ 0.8 → AutoRejected. Severity 0.5-0.8 → ApprovedWithFlags. Severity < 0.5 → pass.

### A28.6 Proposal Validation Composite Thresholds (Per-Level)

```
Level 0: composite ≥ 0.3 → auto-approve
Level 1: composite ≥ 0.4 → auto-approve
Level 2: composite ≥ 0.5 → auto-approve
Level 3: composite ≥ 0.6 → auto-approve
Level 4: composite ≥ 0.8 → auto-approve (nearly impossible)
```

Below threshold → HumanReviewRequired.

### A28.7 Proposal Extraction Timing

Runs ONLY on terminal turn (final text response, no more tool calls). Examines ALL tool calls from entire run. Partial proposals from halted runs → HumanReview with `is_partial_run = true`.

### A28.8 DenialFeedback Lifecycle

- Injected into PromptCompiler Layer 6 (convergence state) on NEXT turn
- Cleared after ONE prompt inclusion (pending-review persists until resolved)
- Three sub-types: Restricted Type, Base Validation Failure (D1-D4), Emulation Language (D7)
- Each includes specific dimension scores, constraint violated, actionable alternatives

### A28.9 Re-Proposal Guard

If agent re-proposes identical rejected content: D3 (contradiction against rejection record) catches it → AutoRejected again → escalated DenialFeedback → convergence score increase → potential intervention escalation. PostRedirectVerifier detects deceptive compliance (cosmetic rewording).

### A28.10 Timeout Handling

Configurable: `convergence.proposal_review_timeout: "24h"` (default). After timeout: decision='timed_out', resolved_by='system:timeout'. Treated as soft rejection. Agent can re-propose.

### A28.11 Hash Chains Across Proposal Lifecycle

7 tables with independent hash chains: memory_events (per memory_id), goal_proposals (global), reflection_entries (global), boundary_violations (global), itp_events (global), convergence_scores (global), intervention_history (global). All blake3. Periodic verification every 1000 events or 24h.

### A28.12 Superseding Logic

When agent proposes an update to an existing goal: new proposal references `target_memory_id` of existing goal. If approved, creates new version (memory_events chain). Old version preserved in history. D5 scope expansion measured against the EXISTING goal (not all goals).



---

## ADDENDUM A29: KILL SWITCH — Complete Trigger Chain and Execution Design

Source: `KILL_SWITCH_TRIGGER_CHAIN_SEQUENCE_FLOW.md` (complete document)

### A29.1 TriggerEvent Enum (Unified Signal Type)

```rust
// ghost-gateway/src/safety/mod.rs
pub enum TriggerEvent {
    SoulDrift { agent_id, drift_score, threshold, baseline_hash, current_hash, detected_at },
    SpendingCapExceeded { agent_id, daily_total, cap, overage, detected_at },
    PolicyDenialThreshold { agent_id, session_id, denial_count, denied_tools, denied_reasons, detected_at },
    SandboxEscape { agent_id, skill_name, escape_attempt, detected_at },
    CredentialExfiltration { agent_id, skill_name, exfil_type, credential_id, detected_at },
    MultiAgentQuarantine { quarantined_agents, quarantine_reasons, count, threshold, detected_at },
    MemoryHealthCritical { agent_id, health_score, threshold, sub_scores, detected_at },
    ManualPause { agent_id, reason, initiated_by },
    ManualQuarantine { agent_id, reason, initiated_by },
    ManualKillAll { reason, initiated_by },
}
```

### A29.2 Trigger→Level Classification

| Trigger | Level | Scope |
|---------|-------|-------|
| T1: SoulDrift | QUARANTINE | Agent |
| T2: SpendingCapExceeded | PAUSE | Agent |
| T3: PolicyDenialThreshold (5+ in session) | QUARANTINE | Agent |
| T4: SandboxEscape | KILL ALL | Platform |
| T5: CredentialExfiltration | KILL ALL | Platform |
| T6: MultiAgentQuarantine (3+ quarantined) | KILL ALL | Platform |
| T7: MemoryHealthCritical (<0.3) | QUARANTINE | Agent |

### A29.3 AutoTriggerEvaluator (Single-Consumer Sequential Processing)

```
loop {
    event = trigger_rx.recv().await;
    1. classify(event) → (KillLevel, KillScope)
    2. dedup check (compute_dedup_key, 5min expiry)
    3. escalation check (current state >= level? skip unless scope broadens)
    4. kill_switch.execute(level, scope, event)
    5. audit_logger.log_kill_switch_action(event, result)
    6. notification_dispatcher.notify_owner(event, result)
    7. cleanup expired dedup entries
}
```

WHY single-consumer: prevents TOCTOU race on quarantine count. Sequential processing via mpsc channel serializes all trigger processing. Performance impact negligible (triggers are rare events, <10ms each).

### A29.4 PAUSE Execution Sequence

```
1. Acquire write lock on KillSwitchState
2. Check idempotency (already paused?)
3. AgentRegistry::pause(agent_id) — stop dequeuing new messages
4. SessionManager::notify_pause(agent_id) — wait for current turn (max 30s), lock session
5. Update state: paused_agents.insert(agent_id)
6. Release write lock
```

### A29.5 QUARANTINE Execution Sequence

```
1. Acquire write lock
2. Check idempotency
3. If paused, remove from paused_agents (quarantine supersedes)
4. QuarantineManager::quarantine(agent_id):
   4a. Revoke all capabilities
   4b. Disconnect all channels
   4c. Flush active session (10s timeout, shorter than normal)
   4d. Preserve forensic state (read-only, no deletion)
   4e. Check multi-agent threshold (T6 cascade: if ≥3, emit MultiAgentQuarantine)
5. Update state: quarantined_agents.insert(agent_id)
6. Release write lock
```

### A29.6 KILL ALL Execution Sequence

```
1. Acquire write lock
2. Check idempotency
3. Set platform_killed = true (checked by ALL subsystems on every operation)
4. Stop all agents in PARALLEL (with 15s total timeout):
   4a. Pause each agent
   4b. Abort in-flight LLM calls (cancel tokio task)
   4c. Disconnect channels
   4d. Emergency session flush (5s timeout per agent)
5. Enter safe mode:
   - Stop channel adapters, heartbeat engine, cron engine
   - Keep API server (health/status/audit endpoints only, agent endpoints → 503)
   - Keep convergence monitor connection
   - Keep SQLite connection
6. Persist kill_state.json to ~/.ghost/safety/kill_state.json
   (checked on gateway restart — if present, start in SAFE MODE)
7. Release write lock
```

### A29.7 Two-Phase Kill (Manual KILL ALL)

Phase 1 (immediate): `AtomicBool::store(true, SeqCst)` — stops all new operations in nanoseconds
Phase 2 (queued): Full shutdown sequence via mpsc channel when evaluator processes it

The atomic flag is checked by AgentRunner, ToolExecutor, ChannelAdapters, SessionManager before every operation.

### A29.8 T5 Credential Exfiltration — Two Detection Paths

**Path A**: CredentialBroker detects direct exfiltration (called outside sandbox, wrong target API, token replay)
**Path B**: OutputInspector scans LLM response for credential patterns (API keys, bearer tokens, private keys, JWTs)

Path B: only triggers KILL ALL if matched string IS a real credential in the credential store (not just pattern match). Prevents false positives from discussing API key formats.

New file needed: `ghost-agent-loop/src/output_inspector.rs` (or integrated into SimBoundaryEnforcer pipeline)

### A29.9 T7 Memory Health Score — Three Sub-Scores

```
memory_health = weighted_average(
    convergence_rate: 0.3,      // beliefs stabilizing?
    drift_magnitude: 0.3,       // behavior diverging from baseline?
    contradiction_count: 0.4,   // internal contradictions?
)
```

Three detection paths (defense in depth):
- Path A: Gateway polls monitor HTTP API every 30s
- Path B: Gateway reads shared state file every 1s
- Path C: Degraded mode fallback — direct cortex queries every 60s (stricter threshold <0.2)

Hysteresis: trigger at <0.3, require recovery to >0.5 before clearing (prevents flapping).

### A29.10 Race Condition Analysis (5 Scenarios Documented)

All resolved by single-consumer sequential processing via mpsc channel:
1. Two KILL ALL triggers simultaneously → first executes, second is idempotent
2. Three QUARANTINE triggers for different agents → sequential processing, accurate count, T6 cascade fires correctly
3. PAUSE then QUARANTINE for same agent → quarantine supersedes pause
4. T6 cascade during quarantine execution → mpsc serializes, no re-entrancy
5. Manual KILL ALL during auto-trigger processing → two-phase kill (atomic flag immediate, full sequence queued)

CRITICAL: QuarantineManager MUST use `try_send()` (non-blocking) for T6 cascade, NOT `send().await` (would deadlock).

### A29.11 Notification Dispatch

Kill switch notifications are SEPARATE from convergence monitor notifications (different priority, different availability requirements). Channels: webhook (5s timeout, 1 retry), email (SMTP via lettre, 10s, no retry), SMS (webhook to Twilio, 5s, 1 retry), desktop (notify-rust, always attempted). Notifications MUST NOT go through agent channels (may be compromised). Notification failure does NOT block kill switch action.

New file needed: `ghost-gateway/src/safety/notification.rs`

### A29.12 Audit Logging

Every kill switch action logged to append-only audit table. Hash-chained (blake3). Cannot be deleted or modified (SQLite triggers). If audit write fails: log to stderr + write to `~/.ghost/safety/emergency_audit.jsonl` (fallback). Safety > audit (kill switch executes regardless).

---

## ADDENDUM A30: AGENT LOOP — Integration Point Map (12 Points)

Source: `AGENT_LOOP_SEQUENCE_FLOW.md` §6.1

| IP | Integration | Blocking? | Bug Risk | Description |
|----|------------|-----------|----------|-------------|
| IP-1 | Agent Loop → Policy Engine | Yes | HIGH | Every tool call authorized |
| IP-2 | Agent Loop → ITP Emitter | No | LOW | Async event emission |
| IP-3 | Agent Loop → ToolExecutor | Yes | HIGH | Sandboxed execution |
| IP-4 | Agent Loop → PromptCompiler | Yes | MEDIUM | 10-layer context assembly |
| IP-5 | Agent Loop → SimBoundaryEnforcer | Yes | MEDIUM | Output scanning |
| IP-6 | Agent Loop → ProposalExtractor | Yes | MEDIUM | Parse proposals from output |
| IP-7 | Agent Loop → ProposalRouter | Yes | HIGH | Validation + routing |
| IP-8 | Gateway → Convergence Monitor | No | MEDIUM | Shared state file polling |
| IP-9 | Gateway → Kill Switch | Yes | CRITICAL | Safety override |
| IP-10 | Gateway → Session Manager | Yes | HIGH | Lock management |
| IP-11 | Gateway → Channel Adapters | No | LOW | Message delivery |
| IP-12 | Monitor → State Publisher | No | MEDIUM | Shared state file write |

---

## ADDENDUM A31: SESSION COMPACTION — Missing Details

Source: `SESSION_COMPACTION_SEQUENCE_FLOW.md` (cross-reference with design A1)

### A31.1 Compaction Trigger Conditions (Expanded)

Compaction triggers when ANY of:
- Context token count > 80% of model's context window
- Session message count > configurable threshold (default 50)
- Explicit compaction request from gateway (convergence Level 3 session termination)
- Post-loop check (Step I.4) detects context growth rate exceeding threshold

### A31.2 Compaction Error Recovery (Expanded from A1)

If compaction LLM call fails (the summarization step):
- Retry once with simpler prompt (just extract key facts, no narrative)
- If retry fails: fall back to mechanical truncation (drop oldest 50% of messages, keep tool results)
- If mechanical truncation still over budget: emergency truncation (keep only last 10 messages + system prompt)
- All fallback paths logged as DEGRADED errors
- Compaction failure does NOT block the next run (agent continues with truncated context)

---

## ADDENDUM A32: CONVERGENCE MONITOR — Edge Cases and Failure Modes

Source: `CONVERGENCE_MONITOR_SEQUENCE_FLOW.md` Edge Cases section

### A32.1 Edge Case: Two Sessions for Same Agent Overlap

Should not happen (lane queue serializes). If it does: monitor tracks per-session state independently, composite scoring uses HIGHER score.

### A32.2 Edge Case: Clock Skew Between Agent and Monitor

ITP events validated: reject >5min in future. Signal computation uses MONITOR's clock for ordering. Storage stores both agent_timestamp and monitor_received_at.

### A32.3 Edge Case: User Modifies Convergence Thresholds During Active Session

Config changes TIME-LOCKED during active sessions. Can only modify during cooldown periods. Exception: raising thresholds (stricter) always allowed. Dual-key changes for critical thresholds (requires designated contact confirmation). Minimum floor on thresholds. Every override logged.

### A32.4 Edge Case: Multiple Agents, One Hits Level 3

Intervention is PER-AGENT. Agent A at Level 3 does not affect Agent B at Level 0. Exception: 3+ agents quarantined → kill switch KILL ALL (T6).

### A32.5 Simulation Boundary Interaction with Convergence Scoring

SimBoundaryEnforcer runs INSIDE agent loop (synchronous). Detection events emitted as ITP events to monitor (async). Creates secondary feedback loop: agent output → enforcer scan → detection event → monitor → scoring → level change → policy tightening → agent context changes → output changes.



---

## ADDENDUM A33: OPEN DESIGN DECISIONS (From Source Document Audit)

These items were flagged as "DECISION NEEDED" in the source documents and require resolution during implementation.

### A33.1 T6 Percentage Threshold

Source: KILL_SWITCH doc §2.7 — If platform has <3 agents and all are quarantined (e.g., 2/2 = 100%), should KILL ALL trigger? Suggested: KILL ALL if quarantined ≥ 3 OR quarantined ≥ 50% of total registered agents (whichever is lower).

### A33.2 OutputInspector Placement

Source: KILL_SWITCH doc §2.6 — OutputInspector (credential scanning in LLM output) needs a home. Options: (a) `ghost-agent-loop/src/output_inspector.rs` as standalone, or (b) integrated into SimBoundaryEnforcer pipeline (shares scan_output() infrastructure). Recommendation: (b) — single output scanning pipeline with pluggable detectors.

### A33.3 T8: Hash Chain Integrity Failure

Source: KILL_SWITCH doc §2.8 — Hash chain integrity failure detected by cortex-temporal is a SEPARATE concern from memory health score. Recommendation: Add T8 trigger (hash chain integrity failure → QUARANTINE) or fold into T7 as sub-signal with weight 1.0 (instant fail).

### A33.4 Kill Switch Notification Crate

Source: KILL_SWITCH doc §3.8 — Kill switch needs its own notification path (separate from convergence monitor notifications). Options: (a) `ghost-gateway/src/safety/notification.rs` (gateway-owned), or (b) shared notification crate used by both gateway and monitor. Recommendation: (a) for Phase 4, refactor to (b) in Phase 6.

### A33.5 New Agent Registration During Active Quarantines

Source: KILL_SWITCH doc §2.7 — Should new agent registration be BLOCKED during active quarantines? Recommendation: Log as suspicious, require owner auth for registration during quarantine state.

### A33.6 Convergence Weight Tuning

Source: CONVERGENCE_MONITOR doc §5.1 — Code default is equal weights (1/7 each). Production "standard" profile should ship with doc 07 differentiated weights. Equal weights are fallback only.

### A33.7 Partial Quarantine Handling

Source: KILL_SWITCH doc §2.7 — If QuarantineManager fails mid-execution (e.g., can't disconnect channels): retry once, if still fails → escalate to KILL ALL. Partial quarantine is worse than fully stopped.



---

## ADDENDUM A34: CROSS-DOCUMENT GAP ANALYSIS — 15 Remaining Gaps

> Source: Cross-reference of design.md (A1-A33) against FILE_MAPPING.md (full, including 23 audit findings)
> and all 7 sequence flow documents (AGENT_LOOP, CONVERGENCE_MONITOR, SESSION_COMPACTION,
> GATEWAY_BOOTSTRAP_DEGRADED_MODE, INTER_AGENT_MESSAGE_FLOW, PROPOSAL_LIFECYCLE, KILL_SWITCH_TRIGGER_CHAIN).
> Date: 2026-02-27
> Severity: CRITICAL = blocks compilation or creates circular deps, HIGH = implementer will get stuck,
> MEDIUM = file missing from mapping or ownership unclear, LOW = cosmetic / incomplete resolution.

---

### GAP 1 — MEDIUM: OutputInspector file designed but not in FILE_MAPPING

Source: Design A10, KILL_SWITCH_TRIGGER_CHAIN §2.6, A33.2

Design addendum A10 introduces `ghost-agent-loop/src/output_inspector.rs` for T5 credential
exfiltration Path B (scanning every LLM response for leaked credentials before delivery).
A33.2 recommends integrating it into the SimBoundaryEnforcer pipeline.

FILE_MAPPING's `ghost-agent-loop/` file tree lists `runner.rs`, `circuit_breaker.rs`,
`itp_emitter.rs`, `context/`, `proposal/`, `tools/`, `response.rs` — no `output_inspector.rs`.

An implementer following FILE_MAPPING will not create this file.

**RESOLUTION**: Add `output_inspector.rs` to FILE_MAPPING under `ghost-agent-loop/src/`:

```
crates/ghost-agent-loop/
├── src/
│   ├── ...existing files...
│   ├── output_inspector.rs             # NEW: OutputInspector — scans every LLM response
│   │                                   #   for credential patterns (sk-..., AKIA..., ghp_...,
│   │                                   #   -----BEGIN...PRIVATE KEY-----) before channel delivery.
│   │                                   #   Cross-references CredentialBroker store to distinguish
│   │                                   #   real credentials from pattern-only matches.
│   │                                   #   Real credential in output → KILL ALL (T5 Path B).
│   │                                   #   Pattern-only match → log warning, redact, continue.
│   │                                   #   Called by AgentRunner AFTER SimBoundaryEnforcer scan,
│   │                                   #   BEFORE channel delivery. Shares scan_output() pipeline
│   │                                   #   infrastructure with SimBoundaryEnforcer.
```

Build phase: Phase 4 (alongside ghost-agent-loop).

---


### GAP 2 — CRITICAL: SessionCompactor crate ownership ambiguity

Source: Design §13, A1, SESSION_COMPACTION_SEQUENCE_FLOW §0, FILE_MAPPING

The design places `SessionCompactor` in `ghost-gateway/src/session/compaction.rs`.
FILE_MAPPING confirms this location. But:

- Addendum A1 says `CompactionError` could live in `ghost-gateway/src/session/compaction.rs`
  OR `ghost-agent-loop/src/compaction/errors.rs`.
- SESSION_COMPACTION_SEQUENCE_FLOW §1.2 says compaction is "a synchronous phase within
  the agent loop's persist step" — meaning it runs INSIDE `AgentRunner::run()`.
- The compaction memory flush turn calls `AgentRunner` methods (`run_flush_turn`),
  `ProposalExtractor`, `ProposalRouter`, `SimBoundaryEnforcer` — all owned by
  `ghost-agent-loop`.
- But `SessionCompactor` also needs `SessionContext` (owned by `ghost-gateway/session/`),
  `LaneQueue` state, and `CostTracker` (owned by `ghost-gateway/cost/`).

If `SessionCompactor` lives in `ghost-gateway`, it needs to call into `ghost-agent-loop`
(for the flush turn). If it lives in `ghost-agent-loop`, it needs `SessionContext` from
`ghost-gateway`. Either way, one crate depends on the other — potential circular dependency.

**RESOLUTION**: `SessionCompactor` stays in `ghost-gateway/src/session/compaction.rs`
(it owns session state). The flush turn is executed via a trait callback:

```rust
// ghost-agent-loop/src/runner.rs
pub trait FlushExecutor: Send + Sync {
    /// Execute a memory flush turn. Returns proposals and cost.
    /// Called by SessionCompactor during Phase 2.
    async fn execute_flush_turn(
        &self,
        session: &mut SessionContext,
        flush_prompt: &str,
    ) -> Result<FlushResult, CompactionError>;
}

// AgentRunner implements FlushExecutor
impl FlushExecutor for AgentRunner { ... }

// ghost-gateway/src/session/compaction.rs
pub struct SessionCompactor {
    config: CompactionConfig,
    flush_executor: Arc<dyn FlushExecutor>,  // injected at construction
}
```

This breaks the circular dependency: `ghost-gateway` depends on `ghost-agent-loop`
(for `FlushExecutor` trait), but `ghost-agent-loop` does NOT depend on `ghost-gateway`.
The trait is defined in `ghost-agent-loop`, implemented by `AgentRunner`, and consumed
by `SessionCompactor` in `ghost-gateway` via dependency injection.

`CompactionError` lives in `ghost-gateway/src/session/compaction.rs` (single location,
no ambiguity). Remove the `ghost-agent-loop/src/compaction/errors.rs` option from A1.

---


### GAP 3 — MEDIUM: `state_publisher.rs` not in convergence-monitor FILE_MAPPING tree

Source: Design A5, CONVERGENCE_MONITOR_SEQUENCE_FLOW §7.1, FILE_MAPPING

Design addendum A5 introduces `convergence-monitor/src/state_publisher.rs` with
`StatePublisher` struct and `ConvergenceSharedState` JSON schema. The CONVERGENCE_MONITOR
sequence flow §7.1 specifies atomic file writes to
`~/.ghost/data/convergence_state/{agent_instance_id}.json`.

FILE_MAPPING's convergence-monitor tree has: `monitor.rs`, `config.rs`, `pipeline/`,
`intervention/`, `session/`, `verification/`, `transport/` — no `state_publisher.rs`.

**RESOLUTION**: Add `state_publisher.rs` to FILE_MAPPING under `convergence-monitor/src/`:

```
crates/convergence-monitor/
├── src/
│   ├── ...existing files...
│   ├── state_publisher.rs              # NEW: StatePublisher — atomic write of
│   │                                   #   ConvergenceSharedState to JSON file.
│   │                                   #   Write to {agent_id}.json.tmp, then rename.
│   │                                   #   Gateway polls at 1s interval.
│   │                                   #   ConvergenceSharedState struct:
│   │                                   #     intervention_level, composite_score,
│   │                                   #     cooldown_active, cooldown_expires_at,
│   │                                   #     session_caps, memory_filter_tier,
│   │                                   #     policy_restrictions, convergence_profile,
│   │                                   #     updated_at.
│   │                                   #   Memory filter tier uses RAW composite_score
│   │                                   #   (not intervention_level). See A5 for schema.
```

---


### GAP 4 — MEDIUM: RecoveryCoordinator location mismatch

Source: Design A6, FILE_MAPPING, GATEWAY_BOOTSTRAP_DEGRADED_MODE §6

Addendum A6 puts `RecoveryCoordinator` in `ghost-gateway/src/health.rs`. FILE_MAPPING
describes `health.rs` as a simple health endpoint file (`/health`, `/ready`, `/metrics`
with checks for SQLite, monitor, channels, disk space).

The recovery logic (R1-R4: 3 stability checks at 5s intervals, ITP buffer replay at
500 events/sec in batches of 100, score recalculation request with 30s timeout, state
transition to Healthy) is substantial — ~150 lines of async logic with error handling.

FILE_MAPPING also mentions `ghost-gateway/src/health/monitor_checker.rs` with
`MonitorHealthChecker` (periodic health check loop, consecutive failure tracking,
exponential backoff). This is a better home for recovery logic.

**RESOLUTION**: Split health concerns into a module:

```
crates/ghost-gateway/src/health/
├── mod.rs                              # Health module root — re-exports
├── endpoints.rs                        # Health endpoint handlers (/health, /ready, /metrics).
│                                       #   Checks: SQLite writable, monitor reachable,
│                                       #   channels connected, disk space adequate.
│                                       #   Returns degraded status if monitor unreachable.
├── monitor_checker.rs                  # MonitorHealthChecker — periodic health check loop
│                                       #   (default 30s interval). Consecutive failure tracking
│                                       #   (threshold: 3). Exponential backoff for reconnection
│                                       #   (initial 5s, max 5min, ±20% jitter).
│                                       #   Triggers Healthy→Degraded and Degraded→Recovering
│                                       #   state transitions.
└── recovery.rs                         # NEW: RecoveryCoordinator — Degraded→Recovering→Healthy
                                        #   sequence. R1: 3 consecutive health checks (5s apart).
                                        #   R2: Replay buffered ITP events (batches of 100,
                                        #   500 events/sec rate limit). R3: Request score
                                        #   recalculation (30s timeout, non-fatal on timeout).
                                        #   R4: Transition to Healthy. On failure at any step:
                                        #   abort recovery, return to Degraded.
```

Update FILE_MAPPING: replace single `health.rs` with `health/` module directory.

---


### GAP 5 — HIGH: Monitor `/events/batch` and `/recalculate` endpoints not in FILE_MAPPING

Source: Design A12, GATEWAY_BOOTSTRAP_DEGRADED_MODE §6, CONVERGENCE_MONITOR_SEQUENCE_FLOW

Addendum A12 adds `POST /events/batch` (up to 100 events per request) and
`POST /recalculate` (trigger score recalculation) to the monitor's HTTP API.
These are required for degraded-mode recovery (RecoveryCoordinator R2 and R3).

Additionally, `POST /gateway-shutdown` is needed for the gateway to notify the
monitor during graceful shutdown (design §12 shutdown step 5).

FILE_MAPPING's `convergence-monitor/transport/http_api.rs` description only mentions:
`GET /health, GET /status, GET /scores, POST /events (ITP event ingestion),
GET /sessions, GET /interventions`.

**RESOLUTION**: Update FILE_MAPPING `convergence-monitor/transport/http_api.rs` description:

```
│   │   ├── http_api.rs                # Lightweight HTTP API (axum) —
│   │   │                               #   GET  /health — monitor health status
│   │   │                               #   GET  /status — full monitor state summary
│   │   │                               #   GET  /scores — current convergence scores (all agents)
│   │   │                               #   GET  /scores/:agent_id — single agent score
│   │   │                               #   GET  /sessions — active session list
│   │   │                               #   GET  /interventions — intervention history
│   │   │                               #   POST /events — single ITP event ingestion
│   │   │                               #   POST /events/batch — batch ITP event ingestion
│   │   │                               #     (up to 100 events per request, used by
│   │   │                               #     RecoveryCoordinator during degraded→healthy)
│   │   │                               #   POST /recalculate — trigger score recalculation
│   │   │                               #     (used after buffer replay during recovery)
│   │   │                               #   POST /gateway-shutdown — gateway notifying monitor
│   │   │                               #     of graceful shutdown (monitor can flush state)
│   │   │                               #   Port: 18790 (default, configurable in ghost.yml)
│   │   │                               #   Rate limiting: token bucket per-source
│   │   │                               #     (default 100 events/min per connection).
│   │   │                               #   Event validation: schema check, timestamp sanity
│   │   │                               #     (reject >5min future), source authentication
│   │   │                               #     (shared secret or unix socket peer credentials).
```

---


### GAP 6 — MEDIUM: `messaging.rs` builtin tool designed but not in FILE_MAPPING

Source: Design A20.11, INTER_AGENT_MESSAGE_FLOW §Systems Involved

The INTER_AGENT_MESSAGE_FLOW sequence doc lists
`ghost-agent-loop/src/tools/builtin/messaging.rs` (NEW) as the `send_agent_message`
tool that agents use to send inter-agent messages. Addendum A20.11 confirms it.

FILE_MAPPING's `ghost-agent-loop/src/tools/builtin/` lists: `shell.rs`, `filesystem.rs`,
`web_search.rs`, `memory.rs` — no `messaging.rs`.

**RESOLUTION**: Add to FILE_MAPPING under `ghost-agent-loop/src/tools/builtin/`:

```
│   │       ├── ...existing builtins...
│   │       └── messaging.rs           # NEW: send_agent_message tool — allows agents
│   │                                   #   to send inter-agent messages via MessageDispatcher.
│   │                                   #   Constructs AgentMessage, signs with agent keypair,
│   │                                   #   submits to dispatcher. Supports all 4 patterns:
│   │                                   #   request/response, fire-and-forget, delegation,
│   │                                   #   broadcast. Policy-checked like any other tool call.
│   │                                   #   Capability: "messaging" (must be granted in ghost.yml).
```

Build phase: Phase 5 (alongside ghost-channels, after ghost-signing and ghost-identity).

---


### GAP 7 — HIGH: Delegation state machine persistence — no storage mapping

Source: Design A20.4, A20.10, INTER_AGENT_MESSAGE_FLOW §9

Addendum A20.4 defines a delegation state machine:
```
OFFERED → ACCEPTED | REJECTED | EXPIRED
ACCEPTED → COMPLETED | TIMED_OUT
COMPLETED → VERIFIED | DISPUTED
DISPUTED → RESOLVED → SETTLED | REFUNDED
```

Addendum A20.10 states: "Delegation state machine is SQLite-persisted (survives crash)."

But there is no `delegation_state` table in the v017 migration. The 6 tables in v017 are:
`itp_events`, `convergence_scores`, `intervention_history`, `goal_proposals`,
`reflection_entries`, `boundary_violations`. Delegations need a 7th table.

There is also no query file in `cortex-storage/src/queries/` for delegation operations,
and no FILE_MAPPING entry for delegation persistence.

**RESOLUTION**: Add v018 migration and query file:

```sql
-- cortex-storage/src/migrations/v018_delegation_state.rs
CREATE TABLE delegation_state (
    id TEXT PRIMARY KEY,                    -- delegation offer message_id (UUIDv7)
    delegator_id TEXT NOT NULL,             -- AgentId of the agent offering the task
    delegate_id TEXT,                       -- AgentId of the agent accepting (NULL until accepted)
    state TEXT NOT NULL DEFAULT 'OFFERED',  -- OFFERED|ACCEPTED|REJECTED|EXPIRED|COMPLETED|
                                            --   TIMED_OUT|VERIFIED|DISPUTED|RESOLVED|SETTLED|REFUNDED
    task TEXT NOT NULL,                     -- Task description from DelegationOfferPayload
    escrow_amount TEXT,                     -- Decimal amount (NULL if no escrow)
    escrow_tx_id TEXT,                      -- ghost-mesh transaction ID (NULL until Phase 9)
    deadline TEXT,                          -- ISO 8601 deadline (NULL if none)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    resolved_at TEXT,                       -- When terminal state reached
    event_hash BLOB NOT NULL,
    previous_hash BLOB NOT NULL
);
-- Append-only guard: only allow UPDATE where resolved_at IS NULL
CREATE TRIGGER delegation_state_append_guard BEFORE UPDATE ON delegation_state
BEGIN SELECT CASE WHEN OLD.resolved_at IS NOT NULL
    THEN RAISE(ABORT, 'append-only: resolved delegations immutable') END; END;
CREATE INDEX idx_delegation_delegator ON delegation_state(delegator_id);
CREATE INDEX idx_delegation_delegate ON delegation_state(delegate_id);
CREATE INDEX idx_delegation_state ON delegation_state(state);
```

Add to FILE_MAPPING:

```
crates/cortex/cortex-storage/
├── src/
│   ├── migrations/
│   │   ├── ...existing v001-v017...
│   │   └── v018_delegation_state.rs    # NEW: delegation_state table for inter-agent
│   │                                   #   delegation lifecycle persistence.
│   │                                   #   Append-only guard on resolved delegations.
│   │                                   #   Hash chain columns for tamper evidence.
│   ├── queries/
│   │   ├── ...existing queries...
│   │   └── delegation_queries.rs       # NEW: Delegation insert/query/transition/
│   │                                   #   active-by-agent/expired-cleanup.
│   │                                   #   State transition validation (only legal
│   │                                   #   transitions per A20.4 state machine).
```

Update `LATEST_VERSION` from 17 to 18 in `cortex-storage/src/migrations/mod.rs`.
Build phase: Phase 5 (alongside inter-agent messaging).

---


### GAP 8 — MEDIUM: `cortex-napi` convergence API bindings — mentioned but never designed

Source: FILE_MAPPING "Remaining Existing Cortex Crates" section

FILE_MAPPING says `cortex-napi: ADD convergence API bindings`. The design never specifies
what these bindings expose. The dashboard (SvelteKit) and browser extension (TypeScript)
need typed APIs for convergence data.

**RESOLUTION**: Specify the convergence bindings for cortex-napi:

```rust
// cortex-napi/src/convergence_bindings.rs (NEW)
// ts-rs v12 generates TypeScript types. These are the NAPI-exposed functions
// and the types they return.

// Exported TypeScript types (via ts-rs #[derive(TS)]):
//   ConvergenceState { score: number, level: number, profile: string }
//   SignalSnapshot { signals: number[], normalized: number[], timestamp: string }
//   InterventionHistoryEntry { level: number, reason: string, timestamp: string }
//   ProposalSummary { id: string, operation: string, decision: string, created_at: string }
//   ConvergenceConfig { weights: number[], thresholds: number[], profile: string }

// Exported NAPI functions:
//   getConvergenceState(agentId: string): ConvergenceState
//   getSignalHistory(agentId: string, limit: number): SignalSnapshot[]
//   getInterventionHistory(agentId: string, limit: number): InterventionHistoryEntry[]
//   getPendingProposals(agentId: string): ProposalSummary[]
//   getConvergenceConfig(): ConvergenceConfig
```

Add to FILE_MAPPING under `cortex-napi`:

```
crates/cortex/cortex-napi/
├── src/
│   ├── ...existing bindings...
│   └── convergence_bindings.rs         # NEW: NAPI bindings for convergence state,
│                                       #   signal history, intervention history,
│                                       #   pending proposals, config. Used by
│                                       #   dashboard and browser extension.
│                                       #   TypeScript types generated via ts-rs.
```

Build phase: Phase 6 (alongside dashboard).

---


### GAP 9 — HIGH: SessionBoundaryEnforcer dual ownership — cortex-session vs convergence-monitor

Source: Design A3, CONVERGENCE_MONITOR_SEQUENCE_FLOW, AGENT_LOOP_SEQUENCE_FLOW §3 step [8], FILE_MAPPING

Two different crates claim to own session boundary enforcement:

1. Design addendum A3 adds `cortex-session/src/boundary.rs` with `SessionBoundaryEnforcer`
   (max_duration, min_gap, cooldown_active).
2. FILE_MAPPING places `SessionBoundaryEnforcer` in
   `convergence-monitor/src/session/boundary.rs`.
3. AGENT_LOOP_SEQUENCE_FLOW §3 step [8] calls
   `SessionBoundaryEnforcer::check_duration(session_id)` from the gateway's pre-loop
   sequence — but doesn't specify which crate it comes from.

The convergence monitor is a SIDECAR PROCESS. The gateway cannot call into it via
Rust function call — only via HTTP or shared state file. But the pre-loop check needs
to be synchronous and fast (it's in the hot path before every agent run).

**RESOLUTION**: Two-layer enforcement:

```
LAYER 1: convergence-monitor/src/session/boundary.rs (AUTHORITATIVE)
  - Owns the RULES: max_duration, min_gap per intervention level.
  - Publishes limits to shared state file (ConvergenceSharedState.session_caps).
  - Enforces at session END (can terminate sessions that exceed limits).

LAYER 2: ghost-gateway/src/session/boundary.rs (NEW — ENFORCEMENT PROXY)
  - Reads session_caps from ConvergenceSharedState (shared state file, 1s poll).
  - Enforces at session START and during pre-loop checks (synchronous, in-process).
  - Falls back to hard-coded maximums if shared state is stale or missing.
  - This is the file that AGENT_LOOP_SEQUENCE_FLOW §3 step [8] calls.
```

Remove `cortex-session/src/boundary.rs` from A3 (cortex-session is a cortex crate —
it shouldn't know about convergence intervention levels). Add to FILE_MAPPING:

```
crates/ghost-gateway/src/session/
├── ...existing files...
├── boundary.rs                         # NEW: SessionBoundaryProxy — reads session_caps
│                                       #   from ConvergenceSharedState file. Enforces
│                                       #   max_duration and min_gap at session start
│                                       #   and during pre-loop checks. Falls back to
│                                       #   hard-coded maximums (180min duration, 0 gap)
│                                       #   if shared state unavailable.
│                                       #   check_start(agent_id, last_session_end) -> Result
│                                       #   check_duration(session_id) -> Result
```

---


### GAP 10 — LOW: Dashboard auth gate UI component missing

Source: FILE_MAPPING Finding 19, Design A2.2

FILE_MAPPING Finding 19 asked for a `+page.svelte` token entry gate in
`dashboard/src/routes/`. The design adds `dashboard/src/lib/auth.ts` (the logic)
but not the UI component that presents the token entry form.

**RESOLUTION**: Add auth gate component to FILE_MAPPING:

```
dashboard/src/
├── routes/
│   ├── +layout.svelte                  # MODIFY: Add auth gate check.
│   │                                   #   If no token in sessionStorage → redirect to /login.
│   │                                   #   If token present → pass to all child routes via
│   │                                   #   Svelte context. Token sent as Authorization header
│   │                                   #   on REST and as query param on WebSocket upgrade.
│   ├── login/
│   │   └── +page.svelte                # NEW: Token entry page. Single input field for
│   │                                   #   GHOST_TOKEN. Stores in sessionStorage (not
│   │                                   #   localStorage — cleared on tab close for security).
│   │                                   #   Validates token by calling GET /api/health with
│   │                                   #   Authorization header. On success → redirect to /.
│   │                                   #   On failure → show error, clear input.
│   │                                   #   No persistent login. Token re-entered per session.
│   ├── ...existing route pages...
```

Build phase: Phase 6 (alongside dashboard).

---


### GAP 11 — HIGH: CLI subcommands — audit Finding 20 unresolved

Source: FILE_MAPPING Finding 20

FILE_MAPPING Finding 20 identified that there is no CLI binary. Users need a `ghost`
command to interact with the platform (`ghost chat`, `ghost status`, `ghost backup`).
The finding suggested adding clap subcommands to the gateway binary.

The design never adds the `ghost-gateway/src/cli/` module or the clap subcommand
structure. This means there is no user-facing entry point for the platform.

**RESOLUTION**: Add CLI module to FILE_MAPPING and design:

```rust
// ghost-gateway/src/main.rs — MODIFY to add clap subcommands
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ghost", about = "GHOST Platform CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server (default if no subcommand)
    Serve {
        #[arg(long, default_value = "ghost.yml")]
        config: PathBuf,
    },
    /// Interactive CLI chat session with an agent
    Chat {
        #[arg(long)]
        agent: Option<String>,
    },
    /// Show platform status (agents, sessions, convergence, health)
    Status,
    /// Trigger manual backup
    Backup {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run export analysis on external platform data
    Export {
        #[arg(long)]
        input: PathBuf,
    },
    /// Run OpenClaw migration
    Migrate {
        #[arg(long)]
        source: Option<PathBuf>,
    },
}
```

Add to FILE_MAPPING:

```
crates/ghost-gateway/src/
├── main.rs                             # MODIFY: Add clap subcommands (serve, chat,
│                                       #   status, backup, export, migrate).
│                                       #   Default (no subcommand) = serve.
├── cli/
│   ├── mod.rs                          # NEW: CLI subcommand module root
│   ├── chat.rs                         # NEW: Interactive chat — creates CLIAdapter,
│   │                                   #   connects to local gateway (or starts embedded
│   │                                   #   if not running). REPL loop with /commands.
│   ├── status.rs                       # NEW: Status display — queries gateway API
│   │                                   #   (GET /api/health, /api/agents, /api/convergence/scores).
│   │                                   #   Formatted terminal output with colors.
│   └── commands.rs                     # NEW: Dispatch for backup, export, migrate.
│                                       #   Delegates to ghost-backup, ghost-export,
│                                       #   ghost-migrate crate entry points.
```

Build phase: Phase 6 (alongside ghost-gateway full integration).

---


### GAP 12 — CRITICAL: TriggerEvent enum location creates circular dependency

Source: Design A29.1, KILL_SWITCH_TRIGGER_CHAIN §1.2, FILE_MAPPING

Addendum A29.1 defines `TriggerEvent` in `ghost-gateway/src/safety/mod.rs` (Layer 4).
But `TriggerEvent` variants are EMITTED by crates in Layer 3:

- `ghost-policy/src/engine.rs` → emits `TriggerEvent::PolicyDenialThreshold`
- `ghost-skills/src/sandbox/wasm_sandbox.rs` → emits `TriggerEvent::SandboxEscape`
- `ghost-skills/src/credential/broker.rs` → emits `TriggerEvent::CredentialExfiltration`
- `ghost-identity/src/drift_detector.rs` → emits `TriggerEvent::SoulDrift`
- `ghost-gateway/src/cost/spending_cap.rs` → emits `TriggerEvent::SpendingCapExceeded`

Layer 3 crates CANNOT depend on Layer 4 (`ghost-gateway`). This is a hard architectural
rule ("No crate may depend on a crate in a higher layer"). If `TriggerEvent` lives in
`ghost-gateway`, Layer 3 crates can't import it.

**RESOLUTION**: Extract `TriggerEvent` into a shared types location that all layers can
depend on. Two options:

**Option A (Recommended)**: Add `TriggerEvent` to `cortex-core/src/safety/` (Layer 1A).
cortex-core is already a dependency of every crate in the platform.

```rust
// cortex-core/src/safety/mod.rs (NEW module)
pub mod trigger;

// cortex-core/src/safety/trigger.rs (NEW)
/// Unified safety trigger event. Emitted by any subsystem that detects
/// a kill-switch-worthy condition. Consumed by AutoTriggerEvaluator
/// in ghost-gateway/src/safety/auto_triggers.rs.
///
/// This type lives in cortex-core (Layer 1A) so that ALL layers can
/// emit triggers without depending on ghost-gateway (Layer 4).
pub enum TriggerEvent {
    SoulDrift { agent_id: Uuid, drift_score: f64, threshold: f64,
                baseline_hash: String, current_hash: String,
                detected_at: DateTime<Utc> },
    SpendingCapExceeded { agent_id: Uuid, daily_total: f64, cap: f64,
                          overage: f64, detected_at: DateTime<Utc> },
    PolicyDenialThreshold { agent_id: Uuid, session_id: Uuid,
                            denial_count: u32, denied_tools: Vec<String>,
                            denied_reasons: Vec<String>,
                            detected_at: DateTime<Utc> },
    SandboxEscape { agent_id: Uuid, skill_name: String,
                    escape_attempt: String, detected_at: DateTime<Utc> },
    CredentialExfiltration { agent_id: Uuid, skill_name: Option<String>,
                             exfil_type: ExfilType, credential_id: String,
                             detected_at: DateTime<Utc> },
    MultiAgentQuarantine { quarantined_agents: Vec<Uuid>,
                           quarantine_reasons: Vec<String>,
                           count: usize, threshold: usize,
                           detected_at: DateTime<Utc> },
    MemoryHealthCritical { agent_id: Uuid, health_score: f64,
                           threshold: f64, sub_scores: BTreeMap<String, f64>,
                           detected_at: DateTime<Utc> },
    ManualPause { agent_id: Uuid, reason: String, initiated_by: String },
    ManualQuarantine { agent_id: Uuid, reason: String, initiated_by: String },
    ManualKillAll { reason: String, initiated_by: String },
}

pub enum ExfilType {
    OutsideSandbox,
    WrongTargetAPI,
    TokenReplay,
    OutputLeakage,
}
```

Layer 3 crates send `TriggerEvent` via `mpsc::Sender<TriggerEvent>` (injected at
construction). `ghost-gateway/src/safety/auto_triggers.rs` owns the receiver.

**Option B**: Create a `ghost-safety-types` leaf crate (Layer 0, alongside ghost-signing).
Cleaner separation but adds another crate to the workspace.

Recommendation: Option A. cortex-core already carries shared types. Adding a `safety/`
module is consistent with its existing `traits/`, `models/`, `config/` modules.

Add to FILE_MAPPING under `cortex-core/src/`:

```
│   ├── safety/
│   │   ├── mod.rs                      # NEW: Safety module root
│   │   └── trigger.rs                  # NEW: TriggerEvent enum, ExfilType enum.
│   │                                   #   Shared across all layers. Consumed by
│   │                                   #   AutoTriggerEvaluator in ghost-gateway.
```

---


### GAP 13 — MEDIUM: ITP buffer writer during degraded mode — no file mapped

Source: Design §12, GATEWAY_BOOTSTRAP_DEGRADED_MODE §3, §5

The design mentions `ITPBuffer` in §12 embedded in the `Gateway` struct:
```rust
pub struct ITPBuffer {
    path: PathBuf,      // ~/.ghost/sessions/buffer/
    max_bytes: usize,   // 10MB
    max_events: usize,  // 10K
}
```

The GATEWAY_BOOTSTRAP_DEGRADED_MODE doc §3 specifies that during degraded mode,
ITP events are buffered to disk instead of being sent to the monitor. §5 specifies
the buffer format and limits.

But there's no dedicated file for the buffer logic. Questions:
- Does `AgentITPEmitter` (ghost-agent-loop) write to the buffer directly?
- Or does the gateway intercept events and redirect to buffer?
- Where does the buffer read logic live for recovery replay?

**RESOLUTION**: The ITP buffer is a gateway concern (the gateway knows about degraded
mode; the agent loop doesn't). Add a dedicated file:

```
crates/ghost-gateway/src/
├── ...existing files...
├── itp_buffer.rs                       # NEW: ITPBuffer — disk-backed event buffer for
│                                       #   degraded mode. Writes ITP events to
│                                       #   ~/.ghost/sessions/buffer/{timestamp}.jsonl
│                                       #   when convergence monitor is unreachable.
│                                       #   Limits: max 10MB total, max 10K events.
│                                       #   FIFO eviction when limits exceeded (oldest dropped).
│                                       #   Read by RecoveryCoordinator during recovery (R2).
│                                       #   Deleted after successful replay.
│                                       #
│                                       #   Integration: AgentITPEmitter sends events to a
│                                       #   gateway-owned router (not directly to monitor).
│                                       #   The router checks GatewayState:
│                                       #     Healthy → forward to monitor transport
│                                       #     Degraded/Recovering → write to ITPBuffer
│                                       #   This keeps the agent loop unaware of degraded mode.
```

Add `itp_router.rs` alongside it:

```
├── itp_router.rs                       # NEW: ITPEventRouter — receives events from
│                                       #   AgentITPEmitter's bounded channel. Routes to
│                                       #   monitor transport (Healthy) or ITPBuffer (Degraded).
│                                       #   Runs as background tokio task. Non-blocking.
│                                       #   Owns the receiver end of the ITP mpsc channel.
```

Build phase: Phase 5 (alongside ghost-gateway integration).

---


### GAP 14 — LOW: Proptest strategy library incomplete for all property test files

Source: Design A3, A14, FILE_MAPPING test-fixtures

The proptest strategy library in `test-fixtures/src/strategies.rs` lists ~12 strategies.
But the property test files across the codebase reference scenarios that need additional
strategies not in the library:

- `convergence-monitor/tests/property/intervention_properties.rs` needs:
  - `intervention_state_strategy()` — generates valid InterventionState with level, credits
  - `composite_result_sequence_strategy()` — generates sequences of CompositeResult for
    testing escalation/de-escalation state machine transitions
  - `cooldown_state_strategy()` — generates CooldownState with active/expired variants

- `cortex-validation/tests/property/proposal_validator_properties.rs` (CVG-PROP-19
  through CVG-PROP-26, 1024 cases) needs:
  - `adversarial_proposal_strategy()` — generates proposals designed to bypass D5-D7
  - `unicode_evasion_strategy()` — generates strings with zero-width chars, homoglyphs,
    RTL overrides for D7 bypass testing

- `ghost-gateway/tests/property/` (kill switch properties) needs:
  - `trigger_event_sequence_strategy()` — generates valid TriggerEvent sequences
  - `kill_state_transition_strategy()` — generates legal state transition sequences

**RESOLUTION**: Expand the strategy library. Add to `test-fixtures/src/strategies.rs`:

```rust
// Additional strategies for convergence-monitor property tests
pub fn intervention_state_strategy() -> impl Strategy<Value = InterventionState>;
pub fn composite_result_sequence_strategy(len: usize) -> impl Strategy<Value = Vec<CompositeResult>>;
pub fn cooldown_state_strategy() -> impl Strategy<Value = CooldownState>;

// Additional strategies for cortex-validation adversarial tests
pub fn adversarial_proposal_strategy() -> impl Strategy<Value = Proposal>;
pub fn unicode_evasion_strategy() -> impl Strategy<Value = String>;

// Additional strategies for kill switch property tests
pub fn trigger_event_sequence_strategy(len: usize) -> impl Strategy<Value = Vec<TriggerEvent>>;
pub fn kill_state_transition_strategy() -> impl Strategy<Value = Vec<(KillLevel, KillLevel)>>;

// Additional strategies for inter-agent messaging tests
pub fn agent_message_strategy() -> impl Strategy<Value = AgentMessage>;
pub fn delegation_state_sequence_strategy() -> impl Strategy<Value = Vec<DelegationState>>;
```

---


### GAP 15 — LOW: `ghost.toml` still in monorepo root — Finding 15 unresolved

Source: FILE_MAPPING Finding 15, FILE_MAPPING monorepo root

FILE_MAPPING Finding 15 flagged `ghost.toml` vs `ghost.yml` inconsistency. The design
uses `ghost.yml` everywhere: §28 configuration schema, `ghost-gateway/src/config/loader.rs`
("YAML parsing"), `convergence-monitor/src/config.rs` ("loads from ghost.yml"), all
code examples reference `ghost.yml`.

But the FILE_MAPPING monorepo root still lists:
```
├── ghost.toml                          # Default platform configuration
```

**RESOLUTION**: Replace `ghost.toml` with `ghost.yml` in FILE_MAPPING monorepo root:

```
├── ghost.yml                           # Default platform configuration (YAML).
│                                       #   Validated against schemas/ghost-config.schema.json.
│                                       #   Env var substitution: ${VAR} syntax.
│                                       #   See §28 for full schema.
```

Remove `ghost.toml` reference entirely. Single format: YAML.

---

### A34 GAP SUMMARY

| # | Severity | Gap | Resolution |
|---|----------|-----|------------|
| 1 | MEDIUM | OutputInspector not in FILE_MAPPING | Add to ghost-agent-loop file tree |
| 2 | CRITICAL | SessionCompactor crate ownership ambiguity | FlushExecutor trait breaks circular dep |
| 3 | MEDIUM | state_publisher.rs not in monitor FILE_MAPPING | Add to convergence-monitor file tree |
| 4 | MEDIUM | RecoveryCoordinator location mismatch | Split health.rs into health/ module |
| 5 | HIGH | Monitor batch/recalculate endpoints not mapped | Update http_api.rs description |
| 6 | MEDIUM | messaging.rs builtin tool not in FILE_MAPPING | Add to ghost-agent-loop/tools/builtin/ |
| 7 | HIGH | Delegation persistence — no table or queries | Add v018 migration + delegation_queries.rs |
| 8 | MEDIUM | cortex-napi convergence bindings unspecified | Specify exported types and functions |
| 9 | HIGH | SessionBoundaryEnforcer dual ownership | Two-layer: monitor authoritative, gateway proxy |
| 10 | LOW | Dashboard auth gate UI component missing | Add login/+page.svelte |
| 11 | HIGH | CLI subcommands unresolved | Add cli/ module with clap subcommands |
| 12 | CRITICAL | TriggerEvent circular dependency | Move to cortex-core/src/safety/trigger.rs |
| 13 | MEDIUM | ITP buffer writer no file mapped | Add itp_buffer.rs + itp_router.rs to gateway |
| 14 | LOW | Proptest strategies incomplete | Expand strategy library with 10 new strategies |
| 15 | LOW | ghost.toml still in monorepo root | Replace with ghost.yml |

Net impact:
- 2 CRITICAL (block compilation): #2 circular dep resolved via trait, #12 resolved via cortex-core
- 4 HIGH (implementer stuck): #5, #7, #9, #11 all resolved with concrete file/code additions
- 6 MEDIUM (file mapping gaps): #1, #3, #4, #6, #8, #13 all resolved with FILE_MAPPING additions
- 3 LOW (cosmetic/incomplete): #10, #14, #15 resolved with minor additions

New files required by A34: ~12 files across 5 crates.
New migration: v018_delegation_state (1 table, 1 trigger, 2 indexes).
New cortex-core module: safety/ (2 files).
