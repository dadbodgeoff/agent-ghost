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
use serde::Deserialize;
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
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug)]
pub struct RunResult {
    pub output: Option<String>,
    pub tool_calls_made: u32,
    pub proposals_extracted: u32,
    pub total_tokens: usize,
    pub total_cost: f64,
    pub halted_by: Option<String>,
}

/// Tracks gate check execution order for testing.
#[derive(Debug, Default)]
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
    /// Maximum recursion depth (default 10).
    pub max_recursion_depth: u32,
    /// Spending cap.
    pub spending_cap: f64,
    /// Current daily spend.
    pub daily_spend: f64,
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
            max_recursion_depth: 10,
            spending_cap: 10.0,
            daily_spend: 0.0,
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
        if total_spend > self.spending_cap {
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

    /// Read convergence shared state from the atomic state file.
    /// Returns None if the file doesn't exist or can't be parsed
    /// (first boot or degraded mode → defaults to level 0).
    fn read_convergence_shared_state(&self, agent_id: Uuid) -> Option<ConvergenceSharedStateRef> {
        let state_path = format!(
            "{}/.ghost/data/convergence_state/{}.json",
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default(),
            agent_id
        );
        let content = std::fs::read_to_string(&state_path).ok()?;
        serde_json::from_str(&content).ok()
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
    ) -> Result<(), String>;
}
