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
use std::time::{Duration, Instant, SystemTime};

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

type CostRecorder = Arc<dyn Fn(Uuid, Uuid, f64, bool) + Send + Sync>;

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
    #[error("agent paused")]
    AgentPaused,
    #[error("agent quarantined")]
    AgentQuarantined,
    #[error("platform killed")]
    PlatformKilled,
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
    #[error("convergence protection degraded: {0}")]
    ConvergenceProtectionDegraded(String),
    #[error("session boundary violation")]
    SessionBoundaryViolation,
    #[error("LLM error: {0}")]
    LLMError(String),
    #[error("credential exfiltration detected — KILL ALL")]
    CredentialExfiltration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoritativeKillState {
    Clear,
    Pause,
    Quarantine,
    KillAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradedConvergenceMode {
    Allow,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvergenceHealthState {
    Disabled,
    Healthy,
    Missing,
    Stale,
    Corrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceStateHealth {
    pub status: ConvergenceHealthState,
    pub level: Option<u8>,
    pub score: Option<f64>,
    pub cooldown_until: Option<chrono::DateTime<chrono::Utc>>,
    pub age_seconds: Option<u64>,
    pub detail: Option<String>,
}

impl ConvergenceStateHealth {
    pub fn status_label(&self) -> &'static str {
        match self.status {
            ConvergenceHealthState::Disabled => "disabled",
            ConvergenceHealthState::Healthy => "healthy",
            ConvergenceHealthState::Missing => "missing",
            ConvergenceHealthState::Stale => "stale",
            ConvergenceHealthState::Corrupted => "corrupted",
        }
    }
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

/// Events emitted during a streaming agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStreamEvent {
    /// Stream has started — provides the message ID.
    StreamStart { message_id: String },
    /// A text delta from the LLM.
    TextDelta { content: String },
    /// Agent is calling a tool.
    ToolUse {
        tool: String,
        tool_id: String,
        status: String,
    },
    /// Tool execution completed.
    ToolResult {
        tool: String,
        tool_id: String,
        status: String,
        preview: String,
    },
    /// The turn is complete.
    TurnComplete {
        token_count: usize,
        safety_status: String,
    },
    /// An error occurred.
    Error { message: String },
    /// Heartbeat — agent is alive, transitioning between phases.
    Heartbeat { phase: String },
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
    /// Gateway-owned kill-state authority for pause/quarantine/kill_all.
    pub authoritative_kill_state: Option<Arc<dyn Fn(Uuid) -> AuthoritativeKillState + Send + Sync>>,
    /// Gateway-owned distributed gate state check.
    pub distributed_gate_check: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    /// Whether convergence monitor supervision is expected in this runtime.
    pub convergence_monitor_enabled: bool,
    /// Whether degraded convergence protection blocks execution.
    pub degraded_convergence_mode: DegradedConvergenceMode,
    /// Maximum acceptable age of convergence shared state.
    pub convergence_state_stale_after: Duration,
    /// Maximum recursion depth (default 10).
    pub max_recursion_depth: u32,
    /// Spending cap.
    pub spending_cap: f64,
    /// Current daily spend.
    pub daily_spend: f64,
    /// Optional DB connection for persisting proposals and audit entries.
    pub db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
    /// Optional cost recording callback: (agent_id, session_id, cost, is_compaction).
    pub cost_recorder: Option<CostRecorder>,
    /// L2: SOUL.md + IDENTITY.md content, loaded at startup.
    pub soul_identity: String,
    /// L4: Environment context, built at startup.
    pub environment: String,
    /// Multi-turn conversation history (injected between system prompt and user message).
    /// Set this before calling `run_turn` for multi-turn sessions.
    pub conversation_history: Vec<ghost_llm::provider::ChatMessage>,
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
            authoritative_kill_state: None,
            distributed_gate_check: None,
            convergence_monitor_enabled: false,
            degraded_convergence_mode: DegradedConvergenceMode::Allow,
            convergence_state_stale_after: Duration::from_secs(300),
            max_recursion_depth: 10,
            spending_cap: 10.0,
            daily_spend: 0.0,
            db: None,
            cost_recorder: None,
            soul_identity: String::new(),
            environment: String::new(),
            conversation_history: Vec::new(),
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
    #[tracing::instrument(skip(self, ctx, log), fields(otel.kind = "internal"))]
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
        self.check_kill_state(ctx.agent_id)?;

        // GATE 3.5: Distributed kill gate (when enabled)
        log.checks.push("kill_gate");
        if self.is_distributed_gate_closed() {
            return Err(RunError::KillGateClosed);
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
            session_started_at: Instant::now(),
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
    fn persist_proposal(
        &self,
        proposal: &cortex_core::traits::convergence::Proposal,
        decision: &str,
    ) -> Option<cortex_storage::queries::goal_proposal_queries::ProposalInsertOutcome> {
        let db = match &self.db {
            Some(db) => db,
            None => return None,
        };
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "failed to lock DB for proposal persistence");
                return None;
            }
        };
        let id = proposal.id.to_string();
        let agent_id = match &proposal.proposer {
            cortex_core::traits::convergence::CallerType::Agent { agent_id } => {
                agent_id.to_string()
            }
            _ => "system".to_string(),
        };
        let session_id = proposal.session_id.to_string();
        let content = proposal.content.to_string();
        let cited = serde_json::to_string(&proposal.cited_memory_ids).unwrap_or_default();
        let proposer_type = format!("{:?}", proposal.proposer);
        let operation = format!("{:?}", proposal.operation);
        let target_type = format!("{:?}", proposal.target_type);
        let event_hash = blake3::hash(id.as_bytes());
        let created_at = proposal.timestamp.to_rfc3339();
        let record = cortex_storage::queries::goal_proposal_queries::NewProposalRecord {
            id: &id,
            agent_id: &agent_id,
            session_id: &session_id,
            proposer_type: &proposer_type,
            operation: &operation,
            target_type: &target_type,
            content: &content,
            cited_memory_ids: &cited,
            decision,
            event_hash: event_hash.as_bytes(),
            previous_hash: &[0u8; 32],
            created_at: Some(&created_at),
            operation_id: None,
            request_id: None,
        };
        match cortex_storage::queries::goal_proposal_queries::insert_proposal_record_with_outcome(
            &conn, &record,
        ) {
            Ok(outcome) => Some(outcome),
            Err(e) => {
                tracing::error!(error = %e, proposal_id = %id, "failed to persist proposal");
                None
            }
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
            &conn,
            memory_id,
            snapshot,
            Some(state_hash.as_bytes()),
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
            &conn,
            &id,
            &sid,
            violation_type,
            severity,
            &blake3::hash(pattern_name.as_bytes()).to_hex()[..16],
            pattern_name,
            action_taken,
            None,
            None,
            event_hash.as_bytes(),
            &[0u8; 32],
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
        let chain_id = proposal
            .content
            .get("chain_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
        let depth = proposal
            .content
            .get("depth")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let trigger_type = proposal
            .content
            .get("trigger_type")
            .and_then(|v| v.as_str())
            .unwrap_or("proposal")
            .to_string();
        let text = proposal
            .content
            .get("reflection_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ratio = proposal
            .content
            .get("self_reference_ratio")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let event_hash = blake3::hash(id.as_bytes());
        if let Err(e) = cortex_storage::queries::reflection_queries::insert_reflection(
            &conn,
            &id,
            &sid,
            &chain_id,
            depth,
            &trigger_type,
            &text,
            ratio,
            event_hash.as_bytes(),
            &[0u8; 32],
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

    /// Build L5 skill index from the tool registry.
    /// Produces a compact listing of available tools for the LLM's awareness.
    pub fn build_skill_index(&self) -> String {
        let names = self.tool_registry.tool_names();
        if names.is_empty() {
            return String::new();
        }
        let mut lines = vec!["Available tools:".to_string()];
        for name in names {
            if let Some(tool) = self.tool_registry.lookup(name) {
                lines.push(format!("- {}: {}", tool.name, tool.description));
            }
        }
        lines.join("\n")
    }

    /// Pre-loop orchestrator: 11 steps executed IN ORDER before run() enters
    /// the recursive loop (per AGENT_LOOP_SEQUENCE_FLOW §3).
    ///
    /// Steps 5-8 are blocking gates — failure halts before run().
    /// Step 9 is the most complex (multiple data sources, partial assembly
    /// must be valid with sensible defaults).
    #[tracing::instrument(skip(self, user_message), fields(
        gen_ai.operation.name = "agent_pre_loop",
        gen_ai.agent.id = %agent_id,
        gen_ai.session.id = %session_id,
    ))]
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
        if let Err(error) = self.check_kill_state(agent_id) {
            tracing::warn!(agent_id = %agent_id, error = %error, "step 5: kill state active — halting");
            return Err(error);
        }
        if self.is_distributed_gate_closed() {
            tracing::warn!(agent_id = %agent_id, "step 5: distributed kill gate closed — halting");
            return Err(RunError::KillGateClosed);
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
        // Convergence protection health is explicit: missing, stale, and
        // corrupted state are degraded, not silently healthy.
        let convergence_health = inspect_convergence_shared_state(
            agent_id,
            self.convergence_monitor_enabled,
            self.convergence_state_stale_after,
        );
        self.handle_degraded_convergence_health(agent_id, &convergence_health)?;
        if let Some(cooldown_until) = convergence_health.cooldown_until {
            if chrono::Utc::now() < cooldown_until {
                tracing::warn!(
                    agent_id = %agent_id,
                    cooldown_until = %cooldown_until,
                    "step 7: cooldown active — halting"
                );
                return Err(RunError::CooldownActive);
            }
        }
        tracing::debug!("step 7: cooldown clear");

        // ── Step 8: Session boundary check (BLOCKING GATE) ──────────
        // Enforce min_gap between sessions from convergence shared state.
        // Falls back to hard-coded maximums when shared state is missing.
        if let Some(level) = convergence_health.level {
            if level >= 3 {
                // At L3+, enforce minimum inter-session gap
                // (handled by SessionBoundaryProxy in gateway)
                tracing::debug!("step 8: session boundary check at L{}", level);
            }
        }
        tracing::debug!("step 8: session boundary clear");

        // ── Step 8.5: Reset damage counter for new session ──────
        // Previous session's damage must not block this session (WP3-A).
        self.damage_counter.reset();

        // ── Step 9: Snapshot assembly (immutable for entire run) ────
        // Assemble the AgentSnapshot from multiple data sources.
        // Must produce a valid snapshot even when convergence data is
        // unavailable (defaults: score 0.0, level 0, no filtering).
        // INV-PRE-06: snapshot is immutable — same object used for
        // entire recursive run.
        let use_degraded_state = matches!(convergence_health.status, ConvergenceHealthState::Stale);
        let intervention_level =
            if matches!(convergence_health.status, ConvergenceHealthState::Healthy)
                || use_degraded_state
            {
                convergence_health.level.unwrap_or(0)
            } else {
                0
            };
        let convergence_score =
            if matches!(convergence_health.status, ConvergenceHealthState::Healthy)
                || use_degraded_state
            {
                convergence_health.score.unwrap_or(0.0)
            } else {
                0.0
            };
        let convergence_state = ConvergenceState {
            score: convergence_score,
            level: intervention_level,
        };
        let snapshot = AgentSnapshot::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            convergence_state,
            simulation_boundary::prompt::SIMULATION_BOUNDARY_PROMPT.to_string(),
        );
        tracing::debug!(
            intervention_level,
            convergence_score,
            "step 9: snapshot assembled with real convergence data"
        );

        // ── Step 10: RunContext construction ─────────────────────────
        let ctx = RunContext {
            agent_id,
            session_id,
            session_started_at: Instant::now(),
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

    fn check_kill_state(&self, agent_id: Uuid) -> Result<(), RunError> {
        if self.kill_switch.load(Ordering::SeqCst) {
            return Err(RunError::KillSwitchActive);
        }

        if let Some(ref kill_state) = self.authoritative_kill_state {
            return match kill_state(agent_id) {
                AuthoritativeKillState::Clear => Ok(()),
                AuthoritativeKillState::Pause => Err(RunError::AgentPaused),
                AuthoritativeKillState::Quarantine => Err(RunError::AgentQuarantined),
                AuthoritativeKillState::KillAll => Err(RunError::PlatformKilled),
            };
        }

        Ok(())
    }

    fn is_distributed_gate_closed(&self) -> bool {
        if let Some(ref gate_check) = self.distributed_gate_check {
            return gate_check();
        }

        self.kill_gate
            .as_ref()
            .map(|gate| gate.is_closed())
            .unwrap_or(false)
    }

    fn handle_degraded_convergence_health(
        &self,
        agent_id: Uuid,
        convergence_health: &ConvergenceStateHealth,
    ) -> Result<(), RunError> {
        match convergence_health.status {
            ConvergenceHealthState::Healthy | ConvergenceHealthState::Disabled => Ok(()),
            ConvergenceHealthState::Missing
            | ConvergenceHealthState::Stale
            | ConvergenceHealthState::Corrupted => {
                tracing::warn!(
                    agent_id = %agent_id,
                    status = convergence_health.status_label(),
                    detail = ?convergence_health.detail,
                    "convergence protection degraded"
                );
                if self.degraded_convergence_mode == DegradedConvergenceMode::Block {
                    Err(RunError::ConvergenceProtectionDegraded(
                        convergence_health.status_label().into(),
                    ))
                } else {
                    Ok(())
                }
            }
        }
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
    #[tracing::instrument(skip(self, ctx, fallback_chain, user_message), fields(
        gen_ai.operation.name = "agent_run",
        gen_ai.agent.id = %ctx.agent_id,
        gen_ai.session.id = %ctx.session_id,
        recursion_depth = ctx.recursion_depth,
    ))]
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
            soul_identity: self.soul_identity.clone(),
            environment: self.environment.clone(),
            skill_index: self.build_skill_index(),
            user_message: user_message.to_string(),
            ..Default::default()
        };
        let (layers, _stats) = self.prompt_compiler.compile(&prompt_input);

        // L0–L7 as system message (everything except L8 conversation history and L9 user message).
        let system_content: String = layers
            .iter()
            .filter(|l| l.index <= 7 && !l.content.is_empty())
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

        // Inject multi-turn conversation history (if any).
        if !self.conversation_history.is_empty() {
            conversation.append(&mut self.conversation_history);
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
                    let error_str = e.to_string();
                    let failure_type = crate::circuit_breaker::classify_llm_error(&error_str);
                    self.circuit_breaker.record_classified_failure(failure_type);
                    RunError::LLMError(error_str)
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
                        InspectionResult::KillAll {
                            pattern_name,
                            trigger: _,
                        } => {
                            self.kill_switch.store(true, Ordering::SeqCst);
                            self.persist_boundary_violation(
                                ctx.session_id,
                                "credential_exfiltration",
                                1.0,
                                &pattern_name,
                                "kill_all",
                            );
                            tracing::error!(pattern = %pattern_name, "KILL ALL — credential exfiltration detected");
                            result.halted_by = Some("credential_exfiltration".into());
                            result.total_tokens = ctx.total_tokens;
                            result.total_cost = ctx.total_cost;
                            return Err(RunError::CredentialExfiltration);
                        }
                        InspectionResult::Warning {
                            pattern_name,
                            redacted_text,
                        } => {
                            self.persist_boundary_violation(
                                ctx.session_id,
                                "credential_pattern_match",
                                0.5,
                                &pattern_name,
                                "redacted",
                            );
                            redacted_text
                        }
                        InspectionResult::Clean => text,
                    };

                    // Extract proposals.
                    let proposals =
                        ProposalExtractor::extract(&final_text, ctx.agent_id, ctx.session_id);
                    result.proposals_extracted += proposals.len() as u32;
                    ctx.proposal_count += proposals.len() as u32;

                    // Route proposals through ProposalRouter (Req 33).
                    for mut proposal in proposals {
                        use cortex_core::models::proposal::ProposalDecision;
                        use cortex_core::models::proposal::ProposalOperation;
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
                        if let Some(outcome) =
                            self.persist_proposal(&proposal, &format!("{decision:?}"))
                        {
                            proposal.content = outcome.canonical_content;
                            if let Some(old_id) = outcome.supersedes_proposal_id.as_deref() {
                                match Uuid::parse_str(old_id) {
                                    Ok(old_id) => self.proposal_router.mark_superseded(old_id),
                                    Err(error) => tracing::warn!(
                                        error = %error,
                                        superseded_proposal_id = old_id,
                                        "storage returned non-UUID superseded proposal id"
                                    ),
                                }
                            }
                        }
                        // Persist reflection entries when approved.
                        if proposal.operation == ProposalOperation::ReflectionWrite
                            && decision == ProposalDecision::AutoApproved
                        {
                            self.persist_reflection(ctx.session_id, &proposal);
                        }
                        self.proposal_router
                            .record_decision(proposal, decision, false);
                    }
                    // Text-only response is the final answer — return.
                    result.total_tokens = ctx.total_tokens;
                    result.total_cost = ctx.total_cost;
                    result.output = Some(final_text);
                    return Ok(result);
                }

                LLMResponse::ToolCalls(calls) => {
                    // Validate plan.
                    if let PlanValidationResult::Deny(reason) =
                        self.tool_executor.validate_plan(&calls)
                    {
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

                    let exec_ctx = crate::tools::skill_bridge::ExecutionContext {
                        agent_id: ctx.agent_id,
                        session_id: ctx.session_id,
                        intervention_level: ctx.intervention_level,
                        session_duration: ctx.session_duration(),
                        session_reflection_count: self
                            .proposal_router
                            .session_reflection_count(ctx.session_id),
                        is_compaction_flush: false,
                    };
                    for call in &calls {
                        let tool_result = self
                            .tool_executor
                            .execute(call, &self.tool_registry, &exec_ctx)
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
                        InspectionResult::KillAll {
                            pattern_name,
                            trigger: _,
                        } => {
                            self.kill_switch.store(true, Ordering::SeqCst);
                            self.persist_boundary_violation(
                                ctx.session_id,
                                "credential_exfiltration",
                                1.0,
                                &pattern_name,
                                "kill_all",
                            );
                            tracing::error!(pattern = %pattern_name, "KILL ALL — credential exfiltration in mixed response");
                            result.halted_by = Some("credential_exfiltration".into());
                            result.total_tokens = ctx.total_tokens;
                            result.total_cost = ctx.total_cost;
                            return Err(RunError::CredentialExfiltration);
                        }
                        InspectionResult::Warning {
                            pattern_name,
                            redacted_text,
                        } => {
                            self.persist_boundary_violation(
                                ctx.session_id,
                                "credential_pattern_match",
                                0.5,
                                &pattern_name,
                                "redacted",
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
                    for mut proposal in proposals {
                        use cortex_core::models::proposal::ProposalDecision;
                        use cortex_core::models::proposal::ProposalOperation;
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
                        if let Some(outcome) =
                            self.persist_proposal(&proposal, &format!("{decision:?}"))
                        {
                            proposal.content = outcome.canonical_content;
                            if let Some(old_id) = outcome.supersedes_proposal_id.as_deref() {
                                match Uuid::parse_str(old_id) {
                                    Ok(old_id) => self.proposal_router.mark_superseded(old_id),
                                    Err(error) => tracing::warn!(
                                        error = %error,
                                        superseded_proposal_id = old_id,
                                        "storage returned non-UUID superseded proposal id"
                                    ),
                                }
                            }
                        }
                        // Persist reflection entries when approved.
                        if proposal.operation == ProposalOperation::ReflectionWrite
                            && decision == ProposalDecision::AutoApproved
                        {
                            self.persist_reflection(ctx.session_id, &proposal);
                        }
                        self.proposal_router
                            .record_decision(proposal, decision, false);
                    }

                    // Process tool calls (same as ToolCalls branch).
                    if let PlanValidationResult::Deny(reason) =
                        self.tool_executor.validate_plan(&tool_calls)
                    {
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

                    let exec_ctx = crate::tools::skill_bridge::ExecutionContext {
                        agent_id: ctx.agent_id,
                        session_id: ctx.session_id,
                        intervention_level: ctx.intervention_level,
                        session_duration: ctx.session_duration(),
                        session_reflection_count: self
                            .proposal_router
                            .session_reflection_count(ctx.session_id),
                        is_compaction_flush: false,
                    };
                    for call in &tool_calls {
                        let tool_result = self
                            .tool_executor
                            .execute(call, &self.tool_registry, &exec_ctx)
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

    /// Streaming variant of `run_turn`. Sends `AgentStreamEvent` through the
    /// channel as the agent generates text and executes tools.
    ///
    /// `get_stream` is a closure that creates a `StreamChunkStream` for a given
    /// conversation and tool schema set. The SSE endpoint provides either
    /// `OllamaProvider::stream_chat` or `complete_stream_shim` depending on the provider.
    pub async fn run_turn_streaming<F>(
        &mut self,
        ctx: &mut RunContext,
        user_message: &str,
        tx: tokio::sync::mpsc::Sender<AgentStreamEvent>,
        get_stream: F,
    ) -> Result<RunResult, RunError>
    where
        F: Fn(
                Vec<ghost_llm::provider::ChatMessage>,
                Vec<ghost_llm::provider::ToolSchema>,
            ) -> ghost_llm::streaming::StreamChunkStream
            + Send
            + Sync,
    {
        use crate::output_inspector::InspectionResult;
        use crate::proposal::extractor::ProposalExtractor;
        use crate::tools::plan_validator::PlanValidationResult;
        use futures::StreamExt;
        use ghost_llm::provider::{ChatMessage, LLMToolCall, MessageRole};
        use ghost_llm::streaming::StreamChunk;

        // Build initial conversation with user message (same as run_turn).
        let mut conversation: Vec<ChatMessage> = Vec::new();

        // Compile prompt layers to get system message.
        let prompt_input = crate::context::prompt_compiler::PromptInput {
            soul_identity: self.soul_identity.clone(),
            environment: self.environment.clone(),
            skill_index: self.build_skill_index(),
            user_message: user_message.to_string(),
            ..Default::default()
        };
        let (layers, _stats) = self.prompt_compiler.compile(&prompt_input);

        let system_content: String = layers
            .iter()
            .filter(|l| l.index <= 7 && !l.content.is_empty())
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

        // Inject multi-turn conversation history (if any).
        if !self.conversation_history.is_empty() {
            conversation.append(&mut self.conversation_history);
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

        let mut accumulated_output = String::new();

        loop {
            // Emit heartbeat so frontend knows agent is alive between turns.
            let _ = tx
                .send(AgentStreamEvent::Heartbeat {
                    phase: "gate_check".into(),
                })
                .await;

            // ── GATE CHECKS ──────────────────────────────────────────
            let mut gate_log = GateCheckLog::default();
            if let Err(e) = self.check_gates(ctx, &mut gate_log) {
                tracing::warn!(error = %e, "gate check failed — halting streaming loop");
                result.halted_by = Some(e.to_string());
                result.total_tokens = ctx.total_tokens;
                result.total_cost = ctx.total_cost;
                result.output = if accumulated_output.is_empty() {
                    None
                } else {
                    Some(accumulated_output)
                };
                return if result.output.is_some() {
                    Ok(result)
                } else {
                    Err(e)
                };
            }

            // ── STREAMING LLM CALL ───────────────────────────────────
            let _ = tx
                .send(AgentStreamEvent::Heartbeat {
                    phase: "llm_streaming".into(),
                })
                .await;
            let mut stream = get_stream(conversation.clone(), tool_schemas.clone());
            let mut segment_text = String::new();
            let mut segment_tool_calls: Vec<LLMToolCall> = Vec::new();
            let mut segment_tool_call_args: std::collections::HashMap<String, (String, String)> =
                std::collections::HashMap::new(); // id -> (name, accumulated_args)
            let mut segment_usage = ghost_llm::provider::UsageStats::default();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(StreamChunk::TextDelta(text)) => {
                        segment_text.push_str(&text);
                        let _ = tx.send(AgentStreamEvent::TextDelta { content: text }).await;
                    }
                    Ok(StreamChunk::ToolCallStart { id, name }) => {
                        segment_tool_call_args.insert(id.clone(), (name.clone(), String::new()));
                        let _ = tx
                            .send(AgentStreamEvent::ToolUse {
                                tool: name,
                                tool_id: id,
                                status: "parsing".into(),
                            })
                            .await;
                    }
                    Ok(StreamChunk::ToolCallDelta {
                        id,
                        arguments_delta,
                    }) => {
                        if let Some((_, ref mut args)) = segment_tool_call_args.get_mut(&id) {
                            args.push_str(&arguments_delta);
                        }
                    }
                    Ok(StreamChunk::Done(usage)) => {
                        segment_usage = usage;
                        break;
                    }
                    Ok(StreamChunk::Error(msg)) => {
                        let _ = tx
                            .send(AgentStreamEvent::Error {
                                message: msg.clone(),
                            })
                            .await;
                        let failure_type = crate::circuit_breaker::classify_llm_error(&msg);
                        self.circuit_breaker.record_classified_failure(failure_type);
                        return Err(RunError::LLMError(msg));
                    }
                    Err(e) => {
                        let error_str = e.to_string();
                        let _ = tx
                            .send(AgentStreamEvent::Error {
                                message: error_str.clone(),
                            })
                            .await;
                        let failure_type = crate::circuit_breaker::classify_llm_error(&error_str);
                        self.circuit_breaker.record_classified_failure(failure_type);
                        return Err(RunError::LLMError(error_str));
                    }
                }
            }

            // Assemble tool calls from accumulated deltas.
            for (id, (name, args_str)) in segment_tool_call_args {
                let arguments: serde_json::Value =
                    serde_json::from_str(&args_str).unwrap_or(serde_json::json!({}));
                segment_tool_calls.push(LLMToolCall {
                    id,
                    name,
                    arguments,
                });
            }

            // Record success + update context.
            self.circuit_breaker.record_success();
            ctx.total_tokens += segment_usage.total_tokens;
            let pricing = ghost_llm::provider::TokenPricing {
                input_per_1k: 0.0,
                output_per_1k: 0.0,
            };
            let call_cost = (segment_usage.prompt_tokens as f64 * pricing.input_per_1k / 1000.0)
                + (segment_usage.completion_tokens as f64 * pricing.output_per_1k / 1000.0);
            ctx.total_cost += call_cost;
            self.record_cost(ctx.agent_id, ctx.session_id, call_cost, false);

            // Determine response type.
            let has_text = !segment_text.is_empty();
            let has_tool_calls = !segment_tool_calls.is_empty();

            match (has_text, has_tool_calls) {
                (false, false) => {
                    // Empty — done.
                    result.total_tokens = ctx.total_tokens;
                    result.total_cost = ctx.total_cost;
                    result.output = if accumulated_output.is_empty() {
                        None
                    } else {
                        Some(accumulated_output)
                    };
                    return Ok(result);
                }
                (true, false) => {
                    // Pure text — inspect for safety, accumulate, continue loop.
                    let inspection = self.output_inspector.scan(&segment_text, ctx.agent_id);
                    let final_text = match inspection {
                        InspectionResult::KillAll {
                            pattern_name: _, ..
                        } => {
                            self.kill_switch.store(true, Ordering::SeqCst);
                            result.halted_by = Some("credential_exfiltration".into());
                            result.total_tokens = ctx.total_tokens;
                            result.total_cost = ctx.total_cost;
                            return Err(RunError::CredentialExfiltration);
                        }
                        InspectionResult::Warning { redacted_text, .. } => redacted_text,
                        InspectionResult::Clean => segment_text,
                    };

                    accumulated_output.push_str(&final_text);

                    // Extract proposals.
                    let proposals =
                        ProposalExtractor::extract(&final_text, ctx.agent_id, ctx.session_id);
                    result.proposals_extracted += proposals.len() as u32;
                    ctx.proposal_count += proposals.len() as u32;
                    // Route proposals (same as run_turn).
                    for mut proposal in proposals {
                        use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
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
                        if let Some(outcome) =
                            self.persist_proposal(&proposal, &format!("{decision:?}"))
                        {
                            proposal.content = outcome.canonical_content;
                            if let Some(old_id) = outcome.supersedes_proposal_id.as_deref() {
                                match Uuid::parse_str(old_id) {
                                    Ok(old_id) => self.proposal_router.mark_superseded(old_id),
                                    Err(error) => tracing::warn!(
                                        error = %error,
                                        superseded_proposal_id = old_id,
                                        "storage returned non-UUID superseded proposal id"
                                    ),
                                }
                            }
                        }
                        if proposal.operation == ProposalOperation::ReflectionWrite
                            && decision == ProposalDecision::AutoApproved
                        {
                            self.persist_reflection(ctx.session_id, &proposal);
                        }
                        self.proposal_router
                            .record_decision(proposal, decision, false);
                    }
                    // Text-only response is the final answer — break the loop.
                    result.total_tokens = ctx.total_tokens;
                    result.total_cost = ctx.total_cost;
                    result.output = Some(accumulated_output);
                    return Ok(result);
                }
                (_, true) => {
                    // Tool calls (possibly mixed with text).
                    if has_text {
                        let inspection = self.output_inspector.scan(&segment_text, ctx.agent_id);
                        match inspection {
                            InspectionResult::KillAll { .. } => {
                                self.kill_switch.store(true, Ordering::SeqCst);
                                result.halted_by = Some("credential_exfiltration".into());
                                result.total_tokens = ctx.total_tokens;
                                result.total_cost = ctx.total_cost;
                                return Err(RunError::CredentialExfiltration);
                            }
                            InspectionResult::Warning { redacted_text, .. } => {
                                accumulated_output.push_str(&redacted_text);
                                result.output = Some(accumulated_output.clone());
                            }
                            InspectionResult::Clean => {
                                accumulated_output.push_str(&segment_text);
                                result.output = Some(accumulated_output.clone());
                            }
                        }
                    }

                    // Plan validation.
                    if let PlanValidationResult::Deny(reason) =
                        self.tool_executor.validate_plan(&segment_tool_calls)
                    {
                        tracing::warn!(reason = %reason, "tool plan denied in streaming");
                        self.tool_executor
                            .record_denial(&segment_tool_calls[0].name);
                        conversation.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: segment_text,
                            tool_calls: Some(segment_tool_calls.clone()),
                            tool_call_id: None,
                        });
                        for call in &segment_tool_calls {
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

                    // Execute tool calls.
                    conversation.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: segment_text,
                        tool_calls: Some(segment_tool_calls.clone()),
                        tool_call_id: None,
                    });

                    let exec_ctx = crate::tools::skill_bridge::ExecutionContext {
                        agent_id: ctx.agent_id,
                        session_id: ctx.session_id,
                        intervention_level: ctx.intervention_level,
                        session_duration: ctx.session_duration(),
                        session_reflection_count: self
                            .proposal_router
                            .session_reflection_count(ctx.session_id),
                        is_compaction_flush: false,
                    };
                    for call in &segment_tool_calls {
                        let _ = tx
                            .send(AgentStreamEvent::ToolUse {
                                tool: call.name.clone(),
                                tool_id: call.id.clone(),
                                status: "running".into(),
                            })
                            .await;

                        // Execute tool with concurrent heartbeat sender.
                        // Sends a heartbeat every 15s during execution to prevent
                        // frontend idle timeout (60s) from killing the SSE stream.
                        // Uses tokio::select! so the heartbeat loop is cancelled
                        // as soon as the tool execution completes.
                        let heartbeat_tx = tx.clone();
                        let heartbeat_tool_name = call.name.clone();
                        let tool_result = {
                            // Spawn heartbeat as a background task that we abort
                            // when tool execution completes. This avoids select!
                            // issues where channel closure could race the tool.
                            let hb_tx = heartbeat_tx.clone();
                            let hb_name = heartbeat_tool_name.clone();
                            let heartbeat_handle = tokio::spawn(async move {
                                let mut interval =
                                    tokio::time::interval(std::time::Duration::from_secs(15));
                                interval.tick().await; // skip immediate first tick
                                loop {
                                    interval.tick().await;
                                    if hb_tx
                                        .send(AgentStreamEvent::Heartbeat {
                                            phase: format!("tool_exec:{}", hb_name),
                                        })
                                        .await
                                        .is_err()
                                    {
                                        break; // channel closed
                                    }
                                }
                            });
                            let result = self
                                .tool_executor
                                .execute(call, &self.tool_registry, &exec_ctx)
                                .await;
                            heartbeat_handle.abort();
                            result
                        };

                        let (output, status) = match tool_result {
                            Ok(tr) => {
                                result.tool_calls_made += 1;
                                ctx.tool_call_count += 1;
                                if call.name == "write_file" || call.name == "shell" {
                                    self.damage_counter.increment();
                                }
                                (tr.output, "done")
                            }
                            Err(e) => (format!("ERROR: {e}"), "error"),
                        };

                        let preview = if output.len() > 200 {
                            format!("{}…", &output[..200])
                        } else {
                            output.clone()
                        };

                        let _ = tx
                            .send(AgentStreamEvent::ToolResult {
                                tool: call.name.clone(),
                                tool_id: call.id.clone(),
                                status: status.into(),
                                preview,
                            })
                            .await;

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
}

pub fn inspect_convergence_shared_state(
    agent_id: Uuid,
    monitor_enabled: bool,
    stale_after: Duration,
) -> ConvergenceStateHealth {
    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(home) => home,
        Err(_) => {
            return ConvergenceStateHealth {
                status: if monitor_enabled {
                    ConvergenceHealthState::Corrupted
                } else {
                    ConvergenceHealthState::Disabled
                },
                level: None,
                score: None,
                cooldown_until: None,
                age_seconds: None,
                detail: Some("HOME/USERPROFILE not set".into()),
            };
        }
    };

    inspect_convergence_shared_state_at(
        std::path::Path::new(&home),
        agent_id,
        monitor_enabled,
        stale_after,
    )
}

fn inspect_convergence_shared_state_at(
    home_dir: &std::path::Path,
    agent_id: Uuid,
    monitor_enabled: bool,
    stale_after: Duration,
) -> ConvergenceStateHealth {
    if !monitor_enabled {
        return ConvergenceStateHealth {
            status: ConvergenceHealthState::Disabled,
            level: None,
            score: None,
            cooldown_until: None,
            age_seconds: None,
            detail: Some("convergence monitor disabled in config".into()),
        };
    }

    let state_path = home_dir
        .join(".ghost")
        .join("data")
        .join("convergence_state")
        .join(format!("{agent_id}.json"));
    let metadata = match std::fs::metadata(&state_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ConvergenceStateHealth {
                status: ConvergenceHealthState::Missing,
                level: None,
                score: None,
                cooldown_until: None,
                age_seconds: None,
                detail: Some(format!("state file not found: {}", state_path.display())),
            };
        }
        Err(error) => {
            return ConvergenceStateHealth {
                status: ConvergenceHealthState::Corrupted,
                level: None,
                score: None,
                cooldown_until: None,
                age_seconds: None,
                detail: Some(format!("metadata read failed: {error}")),
            };
        }
    };

    let content = match std::fs::read_to_string(&state_path) {
        Ok(content) => content,
        Err(error) => {
            return ConvergenceStateHealth {
                status: ConvergenceHealthState::Corrupted,
                level: None,
                score: None,
                cooldown_until: None,
                age_seconds: None,
                detail: Some(format!("state read failed: {error}")),
            };
        }
    };
    let state: ConvergenceSharedStateRef = match serde_json::from_str(&content) {
        Ok(state) => state,
        Err(error) => {
            return ConvergenceStateHealth {
                status: ConvergenceHealthState::Corrupted,
                level: None,
                score: None,
                cooldown_until: None,
                age_seconds: None,
                detail: Some(format!("state parse failed: {error}")),
            };
        }
    };

    let age_seconds = metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|elapsed| elapsed.as_secs());
    let status = if age_seconds.is_some_and(|age| age > stale_after.as_secs()) {
        ConvergenceHealthState::Stale
    } else {
        ConvergenceHealthState::Healthy
    };

    ConvergenceStateHealth {
        status,
        level: Some(state.level),
        score: Some(state.score),
        cooldown_until: state.cooldown_until,
        age_seconds,
        detail: Some(format!("state file: {}", state_path.display())),
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
fn fallback_chain_pricing(chain: &LLMFallbackChain) -> ghost_llm::provider::TokenPricing {
    chain.current_pricing()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_convergence_state(home: &std::path::Path, agent_id: Uuid, body: &str) {
        let dir = home.join(".ghost").join("data").join("convergence_state");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{agent_id}.json")), body).unwrap();
    }

    #[test]
    fn missing_convergence_state_enters_degraded_or_blocked_mode_per_profile() {
        let home = tempfile::tempdir().unwrap();
        let agent_id = Uuid::now_v7();
        let health = inspect_convergence_shared_state_at(
            home.path(),
            agent_id,
            true,
            Duration::from_secs(300),
        );
        assert_eq!(health.status, ConvergenceHealthState::Missing);

        let allow_runner = AgentRunner::new(1024);
        assert!(allow_runner
            .handle_degraded_convergence_health(agent_id, &health)
            .is_ok());

        let mut block_runner = AgentRunner::new(1024);
        block_runner.degraded_convergence_mode = DegradedConvergenceMode::Block;
        assert!(matches!(
            block_runner.handle_degraded_convergence_health(agent_id, &health),
            Err(RunError::ConvergenceProtectionDegraded(status)) if status == "missing"
        ));
    }

    #[test]
    fn stale_convergence_state_is_not_treated_as_healthy() {
        let home = tempfile::tempdir().unwrap();
        let agent_id = Uuid::now_v7();
        write_convergence_state(
            home.path(),
            agent_id,
            r#"{"level":2,"score":0.41,"cooldown_until":null}"#,
        );

        std::thread::sleep(Duration::from_secs(2));

        let health = inspect_convergence_shared_state_at(
            home.path(),
            agent_id,
            true,
            Duration::from_secs(1),
        );
        assert_eq!(health.status, ConvergenceHealthState::Stale);
        assert_eq!(health.level, Some(2));
    }

    #[test]
    fn corrupted_convergence_state_is_visible_and_handled() {
        let home = tempfile::tempdir().unwrap();
        let agent_id = Uuid::now_v7();
        write_convergence_state(home.path(), agent_id, "{not-json");

        let health = inspect_convergence_shared_state_at(
            home.path(),
            agent_id,
            true,
            Duration::from_secs(300),
        );
        assert_eq!(health.status, ConvergenceHealthState::Corrupted);

        let mut runner = AgentRunner::new(1024);
        runner.degraded_convergence_mode = DegradedConvergenceMode::Block;
        assert!(matches!(
            runner.handle_degraded_convergence_health(agent_id, &health),
            Err(RunError::ConvergenceProtectionDegraded(status)) if status == "corrupted"
        ));
    }
}
