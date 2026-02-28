//! Core convergence monitor coordinator (Req 9).
//!
//! Single-threaded event loop: `select!` over ingest channel, health check
//! interval, and shutdown signal. All state lives in `ConvergenceMonitor`.
//! No concurrent signal mutation — the pipeline is strictly sequential.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::config::MonitorConfig;
use crate::intervention::actions::InterventionAction;
use crate::intervention::cooldown::CooldownManager;
use crate::intervention::escalation::EscalationManager;
use crate::intervention::trigger::{CompositeResult, InterventionStateMachine};
use crate::pipeline::signal_computer::SignalComputer;
use crate::pipeline::window_manager::WindowManager;
use crate::session::registry::SessionRegistry;
use crate::state_publisher::{ConvergenceSharedState, StatePublisher};
use crate::transport::http_api::{self, HttpApiState};
#[cfg(unix)]
use crate::transport::unix_socket::UnixSocketTransport;
use crate::transport::{EventType, IngestEvent};
use crate::validation::{EventValidator, RateLimiter};

// ── Score cache (AC14: 30s TTL) ─────────────────────────────────────────

struct CachedScore {
    score: f64,
    level: u8,
    cached_at: Instant,
}

// ── Top-level coordinator ───────────────────────────────────────────────

pub struct ConvergenceMonitor {
    config: MonitorConfig,
    signal_computer: SignalComputer,
    window_manager: WindowManager,
    intervention: InterventionStateMachine,
    cooldown: CooldownManager,
    escalation: EscalationManager,
    sessions: SessionRegistry,
    state_publisher: StatePublisher,
    validator: EventValidator,
    rate_limiter: RateLimiter,
    /// Per-agent calibration session count (AC5).
    calibration_counts: BTreeMap<Uuid, u32>,
    /// Score cache with TTL (AC14).
    score_cache: BTreeMap<Uuid, CachedScore>,
    /// Per-session hash chain: session_id → last event hash (AC4).
    hash_chains: BTreeMap<Uuid, [u8; 32]>,
}

impl ConvergenceMonitor {
    pub fn new(config: MonitorConfig) -> anyhow::Result<Self> {
        let state_publisher = StatePublisher::new(config.state_dir.clone());
        let validator = EventValidator::new(config.clock_skew_tolerance);
        let rate_limiter = RateLimiter::new(config.rate_limit_per_min);
        let max_provisional = config.max_provisional_sessions;

        Ok(Self {
            config,
            signal_computer: SignalComputer::new(),
            window_manager: WindowManager::new(),
            intervention: InterventionStateMachine::new(),
            cooldown: CooldownManager::new(),
            escalation: EscalationManager::new(None),
            sessions: SessionRegistry::new(max_provisional),
            state_publisher,
            validator,
            rate_limiter,
            calibration_counts: BTreeMap::new(),
            score_cache: BTreeMap::new(),
            hash_chains: BTreeMap::new(),
        })
    }

    /// Reconstruct state from SQLite on startup (Req 9 AC2).
    ///
    /// Queries SQLite for last intervention level, last score,
    /// de-escalation credits, cooldown state, and baseline per agent.
    /// Stale state on crash: retain last-known level, never fall to L0 (AC8).
    fn reconstruct_state(&mut self) {
        tracing::info!("reconstructing state from database");

        let db_path = &self.config.db_path;
        let conn = match rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to open database for state reconstruction — starting fresh");
                return;
            }
        };

        // Restore intervention states per agent
        let mut stmt = match conn.prepare(
            "SELECT agent_id, level, consecutive_normal, cooldown_until, \
             ack_required, hysteresis_count, de_escalation_credits \
             FROM intervention_state"
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "intervention_state table not found — starting fresh");
                return;
            }
        };

        let rows = match stmt.query_map([], |row| {
            let agent_id_str: String = row.get(0)?;
            let level: u8 = row.get(1)?;
            let consecutive_normal: u32 = row.get(2)?;
            let cooldown_until: Option<String> = row.get(3)?;
            let ack_required: bool = row.get(4)?;
            let hysteresis_count: u32 = row.get(5)?;
            let de_escalation_credits: u32 = row.get(6)?;
            Ok((agent_id_str, level, consecutive_normal, cooldown_until, ack_required, hysteresis_count, de_escalation_credits))
        }) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "failed to query intervention_state — starting fresh");
                return;
            }
        };

        let mut restored_count = 0u32;
        for row in rows {
            let (agent_id_str, level, consecutive_normal, cooldown_until_str, ack_required, hysteresis_count, de_escalation_credits) = match row {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, "skipping malformed intervention_state row");
                    continue;
                }
            };

            let agent_id = match Uuid::parse_str(&agent_id_str) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!(error = %e, agent_id = %agent_id_str, "skipping invalid agent_id");
                    continue;
                }
            };

            let cooldown_until = cooldown_until_str.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            });

            self.intervention.restore_state_from_fields(
                agent_id,
                level,
                consecutive_normal,
                cooldown_until,
                ack_required,
                hysteresis_count,
                de_escalation_credits,
            );

            restored_count += 1;
            tracing::info!(
                agent_id = %agent_id,
                level,
                "restored intervention state"
            );
        }

        // Restore calibration counts
        if let Ok(mut cal_stmt) = conn.prepare(
            "SELECT agent_id, COUNT(*) FROM itp_events \
             WHERE event_type = 'SessionStart' GROUP BY agent_id"
        ) {
            if let Ok(cal_rows) = cal_stmt.query_map([], |row| {
                let agent_id_str: String = row.get(0)?;
                let count: u32 = row.get(1)?;
                Ok((agent_id_str, count))
            }) {
                for row in cal_rows {
                    if let Ok((agent_id_str, count)) = row {
                        if let Ok(agent_id) = Uuid::parse_str(&agent_id_str) {
                            self.calibration_counts.insert(agent_id, count);
                        }
                    }
                }
            }
        }

        // Restore last known scores into cache (stale but conservative)
        if let Ok(mut score_stmt) = conn.prepare(
            "SELECT agent_id, score, level FROM convergence_scores \
             WHERE rowid IN (SELECT MAX(rowid) FROM convergence_scores GROUP BY agent_id)"
        ) {
            if let Ok(score_rows) = score_stmt.query_map([], |row| {
                let agent_id_str: String = row.get(0)?;
                let score: f64 = row.get(1)?;
                let level: u8 = row.get(2)?;
                Ok((agent_id_str, score, level))
            }) {
                for row in score_rows {
                    if let Ok((agent_id_str, score, level)) = row {
                        if let Ok(agent_id) = Uuid::parse_str(&agent_id_str) {
                            self.score_cache.insert(agent_id, CachedScore {
                                score,
                                level,
                                cached_at: Instant::now(),
                            });
                        }
                    }
                }
            }
        }

        tracing::info!(restored = restored_count, "state reconstruction complete");
    }

    /// Main event loop (A27.3).
    ///
    /// `select!` over: ingest channel, shutdown signal.
    /// Single-threaded — no concurrent signal mutation.
    pub async fn run(&mut self) -> anyhow::Result<()> {
        self.reconstruct_state();

        let (ingest_tx, mut ingest_rx) = mpsc::channel::<IngestEvent>(10_000);

        // ── Start HTTP API transport ────────────────────────────────
        let http_state = Arc::new(RwLock::new(HttpApiState {
            ingest_tx: ingest_tx.clone(),
            healthy: true,
        }));
        let router = http_api::build_router(http_state);
        let http_port = self.config.http_port;

        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{http_port}"))
                .await
                .expect("failed to bind HTTP listener");
            tracing::info!("HTTP API listening on port {http_port}");
            axum::serve(listener, router).await.ok();
        });

        // ── Start Unix socket transport (Unix only) ────────────────
        #[cfg(unix)]
        {
            let socket_transport =
                UnixSocketTransport::new(self.config.socket_path.clone(), ingest_tx.clone());
            tokio::spawn(async move {
                if let Err(e) = socket_transport.run().await {
                    tracing::error!("unix socket transport error: {e}");
                }
            });
        }
        #[cfg(not(unix))]
        {
            tracing::info!("unix socket transport not available on this platform — using HTTP API only");
        }

        tracing::info!("convergence monitor running");

        // ── Single-threaded event loop (A27.3) ─────────────────────
        // select! over: ingest channel, health check interval,
        // cooldown check, shutdown signal.
        let mut health_interval = tokio::time::interval(
            tokio::time::Duration::from_secs(self.config.health_check_interval.as_secs()),
        );
        let mut cooldown_interval = tokio::time::interval(
            tokio::time::Duration::from_secs(60),
        );

        loop {
            tokio::select! {
                Some(event) = ingest_rx.recv() => {
                    self.handle_event(event);
                }
                _ = health_interval.tick() => {
                    tracing::debug!("health check tick");
                    // Publish health status for all tracked agents
                }
                _ = cooldown_interval.tick() => {
                    // Check and expire cooldowns for all agents
                    self.check_cooldowns();
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("shutdown signal received");
                    break;
                }
            }
        }

        tracing::info!("convergence monitor stopped");
        Ok(())
    }

    /// Process a single ingest event through the pipeline.
    ///
    /// Order: validate → persist with hash chain → session lifecycle →
    /// calibration gate → score cache check → compute signals →
    /// composite score → intervention evaluation → state publication.
    fn handle_event(&mut self, event: IngestEvent) {
        // ── Step 1: Validate timestamp (AC12: reject >5min future) ──
        if self.validator.validate_timestamp(event.timestamp).is_err() {
            tracing::warn!(
                agent_id = %event.agent_id,
                ts = %event.timestamp,
                "rejected event: future timestamp exceeds clock skew tolerance"
            );
            return;
        }

        // ── Step 2: Validate session_id (reject nil) ────────────────
        if self.validator.validate_session_id(&event.session_id).is_err() {
            tracing::warn!("rejected event: nil session_id");
            return;
        }

        // ── Step 3: Rate limiting (AC3: token bucket, 100/min/conn) ─
        let conn_id = event.source.to_string();
        if self.rate_limiter.try_consume(&conn_id).is_err() {
            tracing::warn!(conn = %conn_id, "rate limit exceeded");
            return;
        }

        // ── Step 4: Hash chain persistence (AC4) ────────────────────
        let previous_hash = self
            .hash_chains
            .get(&event.session_id)
            .copied()
            .unwrap_or([0u8; 32]); // GENESIS_HASH

        let event_hash = compute_event_hash(&event, &previous_hash);
        self.hash_chains.insert(event.session_id, event_hash);

        // Persist to itp_events with hash chain linkage
        if let Err(e) = self.persist_itp_event(&event, &event_hash, &previous_hash) {
            tracing::error!(
                session_id = %event.session_id,
                error = %e,
                "failed to persist ITP event — hash chain broken, skipping further processing"
            );
            // Revert the hash chain update since persistence failed.
            // This ensures the next event will re-link from the last
            // successfully persisted hash, maintaining chain integrity.
            if previous_hash == [0u8; 32] {
                self.hash_chains.remove(&event.session_id);
            } else {
                self.hash_chains.insert(event.session_id, previous_hash);
            }
            return;
        }

        // ── Step 5: Session lifecycle ────────────────────────────────
        match event.event_type {
            EventType::SessionStart => {
                // AC13: synthetic SessionEnd for prior active sessions
                let closed = self.sessions.start_session(
                    event.session_id,
                    event.agent_id,
                    event.timestamp,
                );
                for sid in closed {
                    tracing::info!(
                        session_id = %sid,
                        "synthetic SessionEnd (mid-session restart, AC13)"
                    );
                    self.window_manager.record_session_end(event.agent_id);
                }

                // Track calibration count
                *self
                    .calibration_counts
                    .entry(event.agent_id)
                    .or_insert(0) += 1;
            }
            EventType::SessionEnd => {
                self.sessions.end_session(event.session_id);
                self.window_manager.record_session_end(event.agent_id);

                // Lock config during active sessions, unlock on end (A8)
                if self.sessions.active_sessions(&event.agent_id).is_empty() {
                    self.cooldown.unlock_config();
                }
            }
            _ => {
                self.sessions
                    .record_event(event.session_id, event.timestamp);
            }
        }

        // ── Step 6: Calibration gate (AC5) ──────────────────────────
        // No scoring/interventions during first N sessions per agent.
        // calibration_sessions defaults to 10, so sessions 1-10 are
        // calibration-only. Session 11+ triggers scoring.
        let session_count = self
            .calibration_counts
            .get(&event.agent_id)
            .copied()
            .unwrap_or(0);
        if session_count < self.config.calibration_sessions {
            return;
        }

        // ── Step 7: Score cache check (AC14: 30s TTL) ───────────────
        // If a recent score exists, skip expensive signal RECOMPUTATION
        // (steps 8-9) but still proceed to intervention evaluation
        // (steps 10-12). The event has already been persisted with its
        // hash chain (step 4) and session lifecycle updated (step 5).
        let (score, level, signals) = if let Some(cached) = self.score_cache.get(&event.agent_id) {
            if cached.cached_at.elapsed() < self.config.score_cache_ttl {
                // Use cached score — skip expensive signal computation
                // Signals unavailable from cache; use zeroed placeholder
                (cached.score, cached.level, [0.0; 7])
            } else {
                self.compute_score(event.agent_id)
            }
        } else {
            self.compute_score(event.agent_id)
        };

        // ── Step 10: Persist score BEFORE intervention ──────────────
        // Audit trail completeness (Req 9 AC6): score is persisted
        // before any intervention action is taken.
        if let Err(e) = self.persist_convergence_score(event.agent_id, score, level) {
            tracing::error!(
                agent_id = %event.agent_id,
                error = %e,
                "failed to persist convergence score"
            );
        }

        // ── Step 11: Intervention evaluation ────────────────────────
        let result = CompositeResult {
            score,
            level,
            signal_scores: signals,
        };

        if let Some(action) = self.intervention.evaluate(&result, event.agent_id) {
            tracing::info!(
                agent_id = %event.agent_id,
                ?action,
                score = format_args!("{score:.3}"),
                level,
                "intervention triggered"
            );

            // Dispatch escalation notifications for L3+ (best-effort, parallel)
            match &action {
                InterventionAction::Level3SessionTermination
                | InterventionAction::Level4ExternalEscalation => {
                    let escalation = self.escalation.clone_config();
                    let agent = event.agent_id;
                    let reason = format!("Intervention L{level}: score={score:.3}");
                    tokio::spawn(async move {
                        let mgr = EscalationManager::new(escalation);
                        mgr.dispatch(level, agent, &reason).await;
                    });
                }
                _ => {}
            }
        }

        // ── Step 12: Publish shared state (AC7) ─────────────────────
        let intervention_state = self.intervention.get_state(&event.agent_id);
        let shared = ConvergenceSharedState {
            agent_id: event.agent_id,
            score,
            level,
            signal_scores: signals,
            consecutive_normal: intervention_state.map_or(0, |s| s.consecutive_normal),
            cooldown_until: intervention_state.and_then(|s| s.cooldown_until),
            ack_required: intervention_state.map_or(false, |s| s.ack_required),
            updated_at: Utc::now(),
        };

        if let Err(e) = self.state_publisher.publish(&shared) {
            tracing::error!("failed to publish shared state: {e}");
        }
    }

    /// Compute signals and composite score (steps 8-9).
    ///
    /// Extracted so the score cache path (step 7) can skip this expensive
    /// computation while still proceeding to intervention evaluation.
    fn compute_score(&mut self, agent_id: Uuid) -> (f64, u8, [f64; 7]) {
        // ── Step 8: Compute signals (dirty-flag throttled) ──────────
        let signals = self.signal_computer.compute(agent_id);

        // ── Step 9: Composite score (weighted, with amplification) ───
        let weighted_sum: f64 = signals.iter().zip(self.config.signal_weights.iter())
            .map(|(s, w)| s * w)
            .sum();
        let weight_total: f64 = self.config.signal_weights.iter().sum();
        let base_score = if weight_total > 0.0 {
            weighted_sum / weight_total
        } else {
            signals.iter().sum::<f64>() / 7.0
        };

        // Apply amplification rules
        let mut score = base_score;

        // Meso trend amplification: 1.1x when directionally concerning
        if self.window_manager.meso_trend_concerning(agent_id) {
            score *= 1.1;
        }

        // Macro z-score amplification: 1.15x when any z-score > 2.0
        if self.window_manager.macro_zscore_exceeds(agent_id, 2.0) {
            score *= 1.15;
        }

        // Clamp to [0.0, 1.0] after amplification (AC9)
        score = score.clamp(0.0, 1.0);

        // Critical single-signal overrides (AC6):
        // session >6h OR gap <5min OR vocab >0.85 → minimum Level 2
        let critical_override = signals[0] > 0.85  // S1: session duration (normalized, >6h)
            || signals[1] > 0.85                     // S2: inter-session gap (<5min)
            || signals[3] > 0.85;                    // S4: vocabulary convergence (>0.85)

        let mut level = score_to_level(score);
        if critical_override && level < 2 {
            level = 2;
        }

        // Cache the score
        self.score_cache.insert(
            agent_id,
            CachedScore {
                score,
                level,
                cached_at: Instant::now(),
            },
        );

        (score, level, signals)
    }

    /// Check and expire cooldowns for all agents.
    fn check_cooldowns(&mut self) {
        // Iterate all agents with active cooldowns and check expiry.
        // Expired cooldowns are cleared so scoring can resume.
        let now = chrono::Utc::now();
        for (_agent_id, state) in self.intervention.states_mut() {
            if let Some(until) = state.cooldown_until {
                if now >= until {
                    state.cooldown_until = None;
                    tracing::info!(agent_id = %_agent_id, "cooldown expired");
                }
            }
        }
    }

    /// Persist an ITP event to the database with hash chain linkage (AC4).
    fn persist_itp_event(
        &self,
        event: &IngestEvent,
        event_hash: &[u8; 32],
        previous_hash: &[u8; 32],
    ) -> anyhow::Result<()> {
        let conn = rusqlite::Connection::open(&self.config.db_path)?;
        conn.execute(
            "INSERT INTO itp_events (session_id, agent_id, event_type, payload, \
             timestamp, event_hash, previous_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                event.session_id.to_string(),
                event.agent_id.to_string(),
                format!("{:?}", event.event_type),
                event.payload.to_string(),
                event.timestamp.to_rfc3339(),
                event_hash.as_slice(),
                previous_hash.as_slice(),
            ],
        )?;
        Ok(())
    }

    /// Persist a convergence score to the database (Step 10, Req 9 AC6).
    /// Score MUST be persisted BEFORE any intervention action is taken.
    fn persist_convergence_score(
        &self,
        agent_id: Uuid,
        score: f64,
        level: u8,
    ) -> anyhow::Result<()> {
        let conn = rusqlite::Connection::open(&self.config.db_path)?;
        conn.execute(
            "INSERT INTO convergence_scores (agent_id, score, level, recorded_at) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                agent_id.to_string(),
                score,
                level,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }
}

// ── Pure functions ──────────────────────────────────────────────────────

/// Map composite score to intervention level.
///
/// ```text
/// [0.0, 0.3)  → L0 (passive)
/// [0.3, 0.5)  → L1 (soft notification)
/// [0.5, 0.7)  → L2 (active intervention)
/// [0.7, 0.85) → L3 (hard boundary)
/// [0.85, 1.0] → L4 (external escalation)
/// ```
fn score_to_level(score: f64) -> u8 {
    if score < 0.3 {
        0
    } else if score < 0.5 {
        1
    } else if score < 0.7 {
        2
    } else if score < 0.85 {
        3
    } else {
        4
    }
}

/// Compute blake3 hash for event persistence (AC4).
///
/// hash = blake3(event_type || "|" || payload || "|" || agent_id
///               || "|" || timestamp || "|" || previous_hash)
fn compute_event_hash(event: &IngestEvent, previous_hash: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(format!("{:?}", event.event_type).as_bytes());
    hasher.update(b"|");
    hasher.update(event.payload.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(event.agent_id.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(event.timestamp.to_rfc3339().as_bytes());
    hasher.update(b"|");
    hasher.update(previous_hash);
    *hasher.finalize().as_bytes()
}

// ── Display impl for EventSource (used by rate limiter conn_id) ─────────

impl std::fmt::Display for crate::transport::EventSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentLoop => write!(f, "agent_loop"),
            Self::BrowserExtension => write!(f, "browser_extension"),
            Self::Proxy => write!(f, "proxy"),
            Self::HttpApi => write!(f, "http_api"),
        }
    }
}
