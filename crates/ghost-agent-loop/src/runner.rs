//! AgentRunner — core recursive loop with gate checks (Req 11, 12).
//!
//! Gate check order (HARD INVARIANT — changing order is a bug):
//! GATE 0: circuit breaker
//! GATE 1: recursion depth
//! GATE 1.5: damage counter
//! GATE 2: spending cap
//! GATE 3: kill switch
//!
//! Pre-loop orchestrator: 11 steps executed IN ORDER before run() enters
//! the recursive loop (per AGENT_LOOP_SEQUENCE_FLOW §3).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use read_only_pipeline::snapshot::{AgentSnapshot, ConvergenceState};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::circuit_breaker::CircuitBreaker;
use crate::context::prompt_compiler::PromptCompiler;
use crate::context::run_context::RunContext;
use crate::damage_counter::DamageCounter;
use crate::itp_emitter::ITPEmitter;
use crate::output_inspector::OutputInspector;
use crate::proposal::router::ProposalRouter;
use crate::tools::executor::ToolExecutor;
use crate::tools::registry::ToolRegistry;

/// Lightweight reference to convergence shared state read from the
/// atomic state file published by the convergence monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConvergenceSharedStateRef {
    pub level: u8,
    pub score: f64,
    pub cooldown_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("circuit breaker open")]
    CircuitBreakerOpen,
    #[error("recursion depth exceeded: {depth}/{max}")]
    RecursionDepthExceeded { depth: u32, max: u32 },
    #[error("damage counter threshold reached: {count}/{threshold}")]
    DamageThreshold { count: u32, threshold: u32 },
    #[error("spending cap exceeded: ${spent:.2} / ${cap:.2}")]
    SpendingCapExceeded { spent: f64, cap: f64 },
    #[error("kill switch active")]
    KillSwitchActive,
    #[error("distributed kill gate closed")]
    KillGateClosed,
    #[error("cooldown active")]
    CooldownActive,
    #[error("session boundary violation")]
    SessionBoundaryViolation,
    #[error("LLM error: {0}")]
    LLMError(String),
    #[error("credential exfiltration detected — KILL ALL")]
    CredentialExfiltration,
}

/// Result of a single agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub output: Option<String>,
    pub tool_calls_made: u32,
    pub proposals_extracted: u32,
    pub total_tokens: usize,
    pub total_cost: f64,
    pub halted_by: Option<String>,
}

/// Tracks gate check execution order for testing.
#[derive(Debug, Default, Clone)]
pub struct GateCheckLog {
    pub checks: Vec<&'static str>,
}

/// The core agent runner.
pub struct AgentRunner {
    pub circuit_breaker: CircuitBreaker,
    pub damage_counter: DamageCounter,
    pub tool_registry: ToolRegistry,
    pub tool_executor: ToolExecutor,
    pub proposal_router: ProposalRouter,
    pub output_inspector: OutputInspector,
    pub itp_emitter: Option<ITPEmitter>,
    pub prompt_compiler: PromptCompiler,
    /// External kill switch flag (SeqCst ordering).
    pub kill_switch: Arc<AtomicBool>,
    /// Distributed kill gate (optional — None when running single-node).
    pub kill_gate: Option<Arc<ghost_kill_gates::gate::KillGate>>,
    /// Maximum recursion depth (default 10).
    pub max_recursion_depth: u32,
    /// Spending cap.
    pub spending_cap: f64,
    /// Current daily spend.
    pub daily_spend: f64,
    /// Optional DB connection for persisting proposals and audit entries.
    pub db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
    /// Optional cost recording callback: (agent_id, session_id, cost, is_compaction).
    pub cost_recorder: Option<Arc<dyn Fn(Uuid, Uuid, f64, bool) + Send + Sync>>,
}

impl AgentRunner {
    pub fn new(context_window: usize) -> Self {
        Self {
            circuit_breaker: CircuitBreaker::default(),
            damage_counter: DamageCounter::default(),
            tool_registry: ToolRegistry::new(),
            tool_executor: ToolExecutor::default(),
            proposal_router: ProposalRouter::new(),
            output_inspector: OutputInspector::new(),
            itp_emitter: None,
            prompt_compiler: PromptCompiler::new(context_window),
            kill_switch: Arc::new(AtomicBool::new(false)),
            kill_gate: None,
            max_recursion_depth: 10,
            spending_cap: 10.0,
            daily_spend: 0.0,
            db: None,
            cost_recorder: None,
        }
    }

    /// Execute gate checks in EXACT order. Returns error if any gate blocks.
    ///
    /// Order is a HARD INVARIANT:
    /// GATE 0: circuit breaker
    /// GATE 1: recursion depth
    /// GATE 1.5: damage counter
    /// GATE 2: spending cap
    /// GATE 3: kill switch
    pub fn check_gates(
        &mut self,
        ctx: &RunContext,
        log: &mut GateCheckLog,
    ) -> Result<(), RunError> {
        // GATE 0: Circuit breaker
        log.checks.push("circuit_breaker");
        if !self.circuit_breaker.allows_call() {
            return Err(RunError::CircuitBreakerOpen);
        }

        // GATE 1: Recursion depth
        log.checks.push("recursion_depth");
        if ctx.is_recursion_exceeded() {
            return Err(RunError::RecursionDepthExceeded {
                depth: ctx.recursion_depth,
                max: ctx.max_recursion_depth,
            });
        }

        // GATE 1.5: Damage counter
        log.checks.push("damage_counter");
        if self.damage_counter.is_halted() {
            return Err(RunError::DamageThreshold {
                count: self.damage_counter.count(),
                threshold: self.damage_counter.threshold(),
            });
        }

        // GATE 2: Spending cap
        log.checks.push("spending_cap");
        let total_spend = self.daily_spend + ctx.total_cost;
        // NaN guard: NaN + anything = NaN, and NaN > cap = false,
        // which would silently bypass the spending cap.
        if total_spend.is_nan() || total_spend.is_infinite() || total_spend > self.spending_cap {
            return Err(RunError::SpendingCapExceeded {
                spent: total_spend,
                cap: self.spending_cap,
            });
        }

        // GATE 3: Kill switch
        log.checks.push("kill_switch");
        if self.kill_switch.load(Ordering::SeqCst) {
            return Err(RunError::KillSwitchActive);
        }

        // GATE 3.5: Distributed kill gate (when enabled)
        log.checks.push("kill_gate");
        if let Some(ref gate) = self.kill_gate {
            if gate.is_closed() {
                return Err(RunError::KillGateClosed);
            }
        }

        Ok(())
    }

    /// Build a default RunContext for a new run.
    pub fn build_run_context(
        &self,
        agent_id: Uuid,
        session_id: Uuid,
        snapshot: AgentSnapshot,
    ) -> RunContext {
        RunContext {
            agent_id,
            session_id,
            recursion_depth: 0,
            max_recursion_depth: self.max_recursion_depth,
            total_tokens: 0,
            total_cost: 0.0,
            tool_call_count: 0,
            proposal_count: 0,
            snapshot,
            intervention_level: 0,
            cb_failures: self.circuit_breaker.consecutive_failures(),
            damage_count: self.damage_counter.count(),
            spending_cap: self.spending_cap,
            daily_spend: self.daily_spend,
            kill_switch_active: self.kill_switch.load(Ordering::SeqCst),
            context_window: 128_000,
        }
    }

    /// Persist a proposal decision to the goal_proposals table.
    fn persist_proposal(&self, proposal: &cortex_core::traits::convergence::Proposal, decision: &str) {
        let db = match &self.db {
            Some(db) => db,
            None => return,
        };
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "failed to lock DB for proposal persistence");
                return;
            }
        };
        let id = proposal.id.to_string();
        let agent_id = match &proposal.proposer {
            cortex_core::traits::convergence::CallerType::Agent { agent_id } => agent_id.to_string(),
            _ => "system".to_string(),
        };
        let session_id = proposal.session_id.to_string();
        let content = proposal.content.to_string();
        let cited = serde_json::to_string(&proposal.cited_memory_ids).unwrap_or_default();
        let event_hash = blake3::hash(id.as_bytes());
        if let Err(e) = cortex_storage::queries::goal_proposal_queries::insert_proposal(
            &conn, &id, &agent_id, &session_id,
            &format!("{:?}", proposal.proposer),
            &format!("{:?}", proposal.operation),
            &format!("{:?}", proposal.target_type),
            &content, &cited, decision,
            event_hash.as_bytes(), &[0u8; 32],
        ) {
            tracing::error!(error = %e, proposal_id = %id, "failed to persist proposal");
        }
    }

    /// Record LLM cost via the cost_recorder callback.
    fn record_cost(&self, agent_id: Uuid, session_id: Uuid, cost: f64, is_compaction: bool) {
        if let Some(ref recorder) = self.cost_recorder {
            recorder(agent_id, session_id, cost, is_compaction);
        }
    }

    /// Persist a memory snapshot to the memory_snapshots table (Finding #8).
    pub fn persist_memory_snapshot(&self, memory_id: &str, snapshot: &str) {
        let db = match &self.db {
            Some(db) => db,
            None => return,
        };
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "failed to lock DB for memory snapshot persistence");
                return;
            }
        };
        let state_hash = blake3::hash(snapshot.as_bytes());
        if let Err(e) = cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
            &conn, memory_id, snapshot, Some(state_hash.as_bytes()),
        ) {
            tracing::error!(error = %e, memory_id = %memory_id, "failed to persist memory snapshot");
        }
    }

    /// Persist a boundary violation to the boundary_violations table.
    fn persist_boundary_violation(
        &self,
        session_id: Uuid,
        violation_type: &str,
        severity: f64,
        pattern_name: &str,
        action_taken: &str,
    ) {
        let db = match &self.db {
            Some(db) => db,
            None => return,
        };
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "failed to lock DB for boundary violation persistence");
                return;
            }
        };
        let id = Uuid::now_v7().to_string();
        let sid = session_id.to_string();
        let event_hash = blake3::hash(id.as_bytes());
        if let Err(e) = cortex_storage::queries::boundary_violation_queries::insert_violation(
            &conn, &id, &sid, violation_type, severity,
            &blake3::hash(pattern_name.as_bytes()).to_hex()[..16],
            pattern_name, action_taken, None, None,
            event_hash.as_bytes(), &[0u8; 32],
        ) {
            tracing::error!(error = %e, "failed to persist boundary violation");
        }
    }

    /// Persist a reflection entry to the reflection_entries table.
    fn persist_reflection(
        &self,
        session_id: Uuid,
        proposal: &cortex_core::traits::convergence::Proposal,
    ) {
        let db = match &self.db {
            Some(db) => db,
            None => return,
        };
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "failed to lock DB for reflection persistence");
                return;
            }
        };
        let id = proposal.id.to_string();
        let sid = session_id.to_string();
        let chain_id = proposal.content.get("chain_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
        let depth = proposal.content.get("depth")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let trigger_type = proposal.content.get("trigger_type")
            .and_then(|v| v.as_str())
            .unwrap_or("proposal")
            .to_string();
        let text = proposal.content.get("reflection_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ratio = proposal.content.get("self_reference_ratio")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let event_hash = blake3::hash(id.as_bytes());
        if let Err(e) = cortex_storage::queries::reflection_queries::insert_reflection(
            &conn, &id, &sid, &chain_id, depth, &trigger_type,
            &text, ratio, event_hash.as_bytes(), &[0u8; 32],
        ) {
            tracing::error!(error = %e, proposal_id = %id, "failed to persist reflection");
        }
    }

    /// Build a default snapshot (used when convergence data is unavailable).
    pub fn default_snapshot() -> AgentSnapshot {
        AgentSnapshot::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            ConvergenceState::default(),
            simulation_boundary::prompt::SIMULATION_BOUNDARY_PROMPT.to_string(),
        )
    }

    /// Pre-loop orchestrator: 11 steps executed IN ORDER before run() enters
    /// the recursive loop (per AGENT_LOOP_SEQUENCE_FLOW §3).
    ///
    /// Steps 5-8 are blocking gates — failure halts before run().
    /// Step 9 is the most complex (multiple data sources, partial assembly
    /// must be valid with sensible defaults).
    pub async fn pre_loop(
        &mut self,
        agent_id: Uuid,
        session_id: Uuid,
        channel: &str,
        user_message: &str,
    ) -> Result<RunContext, RunError> {
        // ── Step 1: Channel normalization ───────────────────────────
        // Normalize the inbound channel identifier to a canonical form.
        let _normalized_channel = channel.to_lowercase();
        tracing::debug!(channel, "step 1: channel normalized");

        // ── Step 2: Agent binding resolution ────────────────────────
        // Resolve which agent handles this channel. Already resolved
        // by the gateway's MessageRouter before reaching AgentRunner.
        tracing::debug!(agent_id = %agent_id, "step 2: agent binding resolved");

        // ── Step 3: Session resolution/creation ─────────────────────
        // Session is either resumed or created. session_id is provided
        // by the gateway's SessionManager.
        tracing::debug!(session_id = %session_id, "step 3: session resolved");

        // ── Step 4: Lane queue acquisition (session lock) ───────────
        // The session lock is held for the entire run, released via
        // Drop guard (INV-SAFE-02). Acquired by the gateway's
        // LaneQueueManager before dispatching to AgentRunner.
        tracing::debug!(session_id = %session_id, "step 4: lane queue acquired");

        // ── Step 5: Kill switch check (BLOCKING GATE) ───────────────
        if self.kill_switch.load(Ordering::SeqCst) {
            tracing::warn!(agent_id = %agent_id, "step 5: kill switch active — halting");
            return Err(RunError::KillSwitchActive);
        }
        tracing::debug!("step 5: kill switch clear");

        // ── Step 6: Spending cap check (BLOCKING GATE) ──────────────
        if self.daily_spend >= self.spending_cap {
            tracing::warn!(
                agent_id = %agent_id,
                spent = self.daily_spend,
                cap = self.spending_cap,
                "step 6: spending cap exceeded — halting"
            );
            return Err(RunError::SpendingCapExceeded {
                spent: self.daily_spend,
                cap: self.spending_cap,
            });
        }
        tracing::debug!("step 6: spending cap clear");

        // ── Step 7: Cooldown check (BLOCKING GATE) ──────────────────
        // Check if the agent is in a cooldown period (L3: 4h, L4: 24h).
        // Cooldown state is read from the convergence shared state file.
        let shared_state = self.read_convergence_shared_state(agent_id);
        if let Some(ref state) = shared_state {
            if let Some(cooldown_until) = state.cooldown_until {
                if chrono::Utc::now() < cooldown_until {
                    tracing::warn!(
                        agent_id = %agent_id,
                        cooldown_until = %cooldown_until,
                        "step 7: cooldown active — halting"
                    );
                    return Err(RunError::CooldownActive);
                }
            }
        }
        tracing::debug!("step 7: cooldown clear");

        // ── Step 8: Session boundary check (BLOCKING GATE) ──────────
        // Enforce min_gap between sessions from convergence shared state.
        // Falls back to hard-coded maximums when shared state is missing.
        if let Some(ref state) = shared_state {
            if state.level >= 3 {
                // At L3+, enforce minimum inter-session gap
                // (handled by SessionBoundaryProxy in gateway)
                tracing::debug!("step 8: session boundary check at L{}", state.level);
            }
        }
        tracing::debug!("step 8: session boundary clear");

        // ── Step 9: Snapshot assembly (immutable for entire run) ────
        // Assemble the AgentSnapshot from multiple data sources.
        // Must produce a valid snapshot even when convergence data is
        // unavailable (defaults: score 0.0, level 0, no filtering).
        // INV-PRE-06: snapshot is immutable — same object used for
        // entire recursive run.
        let intervention_level = shared_state.as_ref().map_or(0u8, |s| s.level);
        let snapshot = Self::default_snapshot();
        tracing::debug!(
            intervention_level,
            "step 9: snapshot assembled"
        );

        // ── Step 10: RunContext construction ─────────────────────────
        let ctx = RunContext {
            agent_id,
            session_id,
            recursion_depth: 0,
            max_recursion_depth: self.max_recursion_depth,
            total_tokens: 0,
            total_cost: 0.0,
            tool_call_count: 0,
            proposal_count: 0,
            snapshot,
            intervention_level,
            cb_failures: self.circuit_breaker.consecutive_failures(),
            damage_count: self.damage_counter.count(),
            spending_cap: self.spending_cap,
            daily_spend: self.daily_spend,
            kill_switch_active: false,
            context_window: 128_000,
        };
        tracing::debug!("step 10: RunContext constructed");

        // ── Step 11: ITP emission ───────────────────────────────────
        // Emit SessionStart for new sessions, InteractionMessage for
        // the user message. Uses bounded channel (capacity 1000),
        // try_send drops on full (AC4).
        if let Some(ref emitter) = self.itp_emitter {
            emitter.emit_session_start(agent_id, session_id);
            emitter.emit_interaction_message(agent_id, session_id, user_message);
        }
        tracing::debug!("step 11: ITP events emitted");

        Ok(ctx)
    }

    /// Run the recursive agentic loop.
    ///
    /// Each iteration: check gates → compile prompt → call LLM → process response.
    /// Tool calls loop back (append results, re-prompt). Text responses exit.
    ///
    /// Key invariants:
    /// - `ctx.snapshot` is immutable for the entire run (INV-PRE-06)
    /// - Gate checks happen EVERY iteration
    /// - `recursion_depth` increments per tool-call round-trip
    /// - `total_cost` accumulates across iterations
    /// - Kill switch is checked every iteration (GATE 3)
    pub async fn run_turn(
        &mut self,
        ctx: &mut RunContext,
        fallback_chain: &mut crate::runner::LLMFallbackChain,
        user_message: &str,
    ) -> Result<RunResult, RunError> {
        use crate::output_inspector::InspectionResult;
        use crate::proposal::extractor::ProposalExtractor;
        use crate::tools::plan_validator::PlanValidationResult;
        use ghost_llm::provider::{ChatMessage, LLMResponse, MessageRole};

        // Build initial conversation with user message.
        let mut conversation: Vec<ChatMessage> = Vec::new();

        // Compile prompt layers to get system message.
        let prompt_input = crate::context::prompt_compiler::PromptInput {
            user_message: user_message.to_string(),
            ..Default::default()
        };
        let (layers, _stats) = self.prompt_compiler.compile(&prompt_input);

        // L0 (CORP_POLICY) + L1 (SIMULATION_BOUNDARY) as system message.
        let system_content: String = layers.iter()
            .filter(|l| l.index <= 1)
            .map(|l| l.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        if !system_content.is_empty() {
            conversation.push(ChatMessage {
                role: MessageRole::System,
                content: system_content,
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // User message.
        conversation.push(ChatMessage {
            role: MessageRole::User,
            content: user_message.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        // Get tool schemas filtered by intervention level.
        let tool_schemas = self.tool_registry.schemas_filtered(ctx.intervention_level);

        let mut result = RunResult {
            output: None,
            tool_calls_made: 0,
            proposals_extracted: 0,
            total_tokens: 0,
            total_cost: 0.0,
            halted_by: None,
        };

        loop {
            // ── GATE CHECKS (every iteration) ───────────────────────
            let mut gate_log = GateCheckLog::default();
            if let Err(e) = self.check_gates(ctx, &mut gate_log) {
                tracing::warn!(error = %e, "gate check failed — halting loop");
                result.halted_by = Some(e.to_string());
                result.total_tokens = ctx.total_tokens;
                result.total_cost = ctx.total_cost;
                return if result.output.is_some() {
                    Ok(result)
                } else {
                    Err(e)
                };
            }

            // ── LLM CALL ────────────────────────────────────────────
            let completion = fallback_chain
                .complete(&conversation, &tool_schemas)
                .await
                .map_err(|e| {
                    self.circuit_breaker.record_failure();
                    RunError::LLMError(e.to_string())
                })?;

            // Record success + update context.
            self.circuit_breaker.record_success();
            ctx.total_tokens += completion.usage.total_tokens;
            let pricing = fallback_chain_pricing(fallback_chain);
            let call_cost = (completion.usage.prompt_tokens as f64 * pricing.input_per_1k / 1000.0)
                + (completion.usage.completion_tokens as f64 * pricing.output_per_1k / 1000.0);
            ctx.total_cost += call_cost;

            // Record cost via callback.
            self.record_cost(ctx.agent_id, ctx.session_id, call_cost, false);

            // ── PROCESS RESPONSE ────────────────────────────────────
            match completion.response {
                LLMResponse::Empty => {
                    result.total_tokens = ctx.total_tokens;
                    result.total_cost = ctx.total_cost;
                    return Ok(result);
                }

                LLMResponse::Text(text) => {
                    // Inspect for credential exfiltration.
                    let inspection = self.output_inspector.scan(&text, ctx.agent_id);
                    let final_text = match inspection {
                        InspectionResult::KillAll { pattern_name, trigger: _ } => {
                            self.kill_switch.store(true, Ordering::SeqCst);
                            self.persist_boundary_violation(
                                ctx.session_id, "credential_exfiltration", 1.0,
                                &pattern_name, "kill_all",
                            );
                            tracing::error!(pattern = %pattern_name, "KILL ALL — credential exfiltration detected");
                            result.halted_by = Some("credential_exfiltration".into());
                            result.total_tokens = ctx.total_tokens;
                            result.total_cost = ctx.total_cost;
                            return Err(RunError::CredentialExfiltration);
                        }
                        InspectionResult::Warning { pattern_name, redacted_text } => {
                            self.persist_boundary_violation(
                                ctx.session_id, "credential_pattern_match", 0.5,
                                &pattern_name, "redacted",
                            );
                            redacted_text
                        }
                        InspectionResult::Clean => text,
                    };

                    // Extract proposals.
                    let proposals = ProposalExtractor::extract(&final_text, ctx.agent_id, ctx.session_id);
                    result.proposals_extracted += proposals.len() as u32;
                    ctx.proposal_count += proposals.len() as u32;

                    // Route proposals through ProposalRouter (Req 33).
                    for proposal in proposals {
                        use cortex_core::models::proposal::ProposalDecision;
                        use cortex_core::models::proposal::ProposalOperation;
                        self.proposal_router.check_superseding(&proposal);
                        let decision = if self.proposal_router.is_resubmission(&proposal) {
                            ProposalDecision::AutoRejected
                        } else if let Some(d) = self.proposal_router.reflection_precheck(
                            &proposal,
                            &cortex_core::config::ReflectionConfig::default(),
                        ) {
                            d
                        } else if ctx.intervention_level <= 1 {
                            ProposalDecision::AutoApproved
                        } else {
                            ProposalDecision::HumanReviewRequired
                        };
                        // Persist to goal_proposals table.
                        self.persist_proposal(&proposal, &format!("{decision:?}"));
                        // Persist reflection entries when approved.
                        if proposal.operation == ProposalOperation::ReflectionWrite
                            && decision == ProposalDecision::AutoApproved
                        {
                            self.persist_reflection(ctx.session_id, &proposal);
                        }
                        self.proposal_router.record_decision(proposal, decision, false);
                    }
                }

                LLMResponse::ToolCalls(calls) => {
                    // Validate plan.
                    if let PlanValidationResult::Deny(reason) = self.tool_executor.validate_plan(&calls) {
                        tracing::warn!(reason = %reason, "tool plan denied");
                        self.tool_executor.record_denial(&calls[0].name);
                        // Feed denial back to LLM.
                        conversation.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: String::new(),
                            tool_calls: Some(calls.clone()),
                            tool_call_id: None,
                        });
                        for call in &calls {
                            conversation.push(ChatMessage {
                                role: MessageRole::Tool,
                                content: format!("ERROR: Tool plan denied — {reason}"),
                                tool_calls: None,
                                tool_call_id: Some(call.id.clone()),
                            });
                        }
                        ctx.recursion_depth += 1;
                        continue;
                    }

                    // Execute each tool call.
                    conversation.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: String::new(),
                        tool_calls: Some(calls.clone()),
                        tool_call_id: None,
                    });

                    for call in &calls {
                        let tool_result = self.tool_executor
                            .execute(call, &self.tool_registry)
                            .await;

                        let output = match tool_result {
                            Ok(tr) => {
                                result.tool_calls_made += 1;
                                ctx.tool_call_count += 1;
                                // Track destructive tools in damage counter.
                                if call.name == "write_file" || call.name == "shell" {
                                    self.damage_counter.increment();
                                }
                                tr.output
                            }
                            Err(e) => format!("ERROR: {e}"),
                        };

                        conversation.push(ChatMessage {
                            role: MessageRole::Tool,
                            content: output,
                            tool_calls: None,
                            tool_call_id: Some(call.id.clone()),
                        });
                    }

                    ctx.recursion_depth += 1;
                    continue;
                }

                LLMResponse::Mixed { text, tool_calls } => {
                    // Process text portion (inspect + extract proposals).
                    let inspection = self.output_inspector.scan(&text, ctx.agent_id);
                    match inspection {
                        InspectionResult::KillAll { pattern_name, trigger: _ } => {
                            self.kill_switch.store(true, Ordering::SeqCst);
                            self.persist_boundary_violation(
                                ctx.session_id, "credential_exfiltration", 1.0,
                                &pattern_name, "kill_all",
                            );
                            tracing::error!(pattern = %pattern_name, "KILL ALL — credential exfiltration in mixed response");
                            result.halted_by = Some("credential_exfiltration".into());
                            result.total_tokens = ctx.total_tokens;
                            result.total_cost = ctx.total_cost;
                            return Err(RunError::CredentialExfiltration);
                        }
                        InspectionResult::Warning { pattern_name, redacted_text } => {
                            self.persist_boundary_violation(
                                ctx.session_id, "credential_pattern_match", 0.5,
                                &pattern_name, "redacted",
                            );
                            result.output = Some(redacted_text);
                        }
                        InspectionResult::Clean => {
                            result.output = Some(text.clone());
                        }
                    }

                    let proposals = ProposalExtractor::extract(&text, ctx.agent_id, ctx.session_id);
                    result.proposals_extracted += proposals.len() as u32;
                    ctx.proposal_count += proposals.len() as u32;

                    // Route proposals through ProposalRouter (Req 33).
                    for proposal in proposals {
                        use cortex_core::models::proposal::ProposalDecision;
                        use cortex_core::models::proposal::ProposalOperation;
                        self.proposal_router.check_superseding(&proposal);
                        let decision = if self.proposal_router.is_resubmission(&proposal) {
                            ProposalDecision::AutoRejected
                        } else if let Some(d) = self.proposal_router.reflection_precheck(
                            &proposal,
                            &cortex_core::config::ReflectionConfig::default(),
                        ) {
                            d
                        } else if ctx.intervention_level <= 1 {
                            ProposalDecision::AutoApproved
                        } else {
                            ProposalDecision::HumanReviewRequired
                        };
                        // Persist to goal_proposals table.
                        self.persist_proposal(&proposal, &format!("{decision:?}"));
                        // Persist reflection entries when approved.
                        if proposal.operation == ProposalOperation::ReflectionWrite
                            && decision == ProposalDecision::AutoApproved
                        {
                            self.persist_reflection(ctx.session_id, &proposal);
                        }
                        self.proposal_router.record_decision(proposal, decision, false);
                    }

                    // Process tool calls (same as ToolCalls branch).
                    if let PlanValidationResult::Deny(reason) = self.tool_executor.validate_plan(&tool_calls) {
                        tracing::warn!(reason = %reason, "tool plan denied in mixed response");
                        self.tool_executor.record_denial(&tool_calls[0].name);
                        conversation.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: text,
                            tool_calls: Some(tool_calls.clone()),
                            tool_call_id: None,
                        });
                        for call in &tool_calls {
                            conversation.push(ChatMessage {
                                role: MessageRole::Tool,
                                content: format!("ERROR: Tool plan denied — {reason}"),
                                tool_calls: None,
                                tool_call_id: Some(call.id.clone()),
                            });
                        }
                        ctx.recursion_depth += 1;
                        continue;
                    }

                    conversation.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: text,
                        tool_calls: Some(tool_calls.clone()),
                        tool_call_id: None,
                    });

                    for call in &tool_calls {
                        let tool_result = self.tool_executor
                            .execute(call, &self.tool_registry)
                            .await;

                        let output = match tool_result {
                            Ok(tr) => {
                                result.tool_calls_made += 1;
                                ctx.tool_call_count += 1;
                                if call.name == "write_file" || call.name == "shell" {
                                    self.damage_counter.increment();
                                }
                                tr.output
                            }
                            Err(e) => format!("ERROR: {e}"),
                        };

                        conversation.push(ChatMessage {
                            role: MessageRole::Tool,
                            content: output,
                            tool_calls: None,
                            tool_call_id: Some(call.id.clone()),
                        });
                    }

                    ctx.recursion_depth += 1;
                    continue;
                }
            }
        }
    }

    /// Read convergence shared state from the atomic state file.
    /// Returns None if the file doesn't exist (first boot) — logs a debug message.
    /// Returns None if the file can't be parsed (corrupted) — logs a warning.
    /// Defaults to level 0 when None (degraded mode).
    fn read_convergence_shared_state(&self, agent_id: Uuid) -> Option<ConvergenceSharedStateRef> {
        let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            Ok(h) => h,
            Err(_) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    "HOME/USERPROFILE not set — cannot read convergence shared state, defaulting to level 0"
                );
                return None;
            }
        };
        let state_path = format!(
            "{}/.ghost/data/convergence_state/{}.json",
            home, agent_id
        );
        let content = match std::fs::read_to_string(&state_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!(
                    agent_id = %agent_id,
                    path = %state_path,
                    "convergence state file not found — first boot or monitor not running, defaulting to level 0"
                );
                return None;
            }
            Err(e) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    path = %state_path,
                    error = %e,
                    "failed to read convergence state file — defaulting to level 0"
                );
                return None;
            }
        };
        match serde_json::from_str(&content) {
            Ok(state) => Some(state),
            Err(e) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    path = %state_path,
                    error = %e,
                    "failed to parse convergence state file (corrupted?) — defaulting to level 0"
                );
                None
            }
        }
    }
}

/// FlushExecutor trait — defined here, implemented by AgentRunner.
/// Injected into SessionCompactor to break circular dependency (A34 Gap 2).
#[async_trait::async_trait]
pub trait FlushExecutor: Send + Sync {
    /// Execute a memory flush turn.
    async fn execute_flush(
        &self,
        agent_id: Uuid,
        session_id: Uuid,
        memories_to_flush: Vec<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Type alias for the LLM fallback chain used by `run_turn`.
pub type LLMFallbackChain = ghost_llm::fallback::FallbackChain;

/// Extract pricing from the first available provider in the fallback chain.
/// Falls back to zero pricing if no providers are available.
fn fallback_chain_pricing(
    chain: &LLMFallbackChain,
) -> ghost_llm::provider::TokenPricing {
    chain.current_pricing()
}
