//! Core convergence monitor coordinator (Req 9).
//!
//! Single-threaded event loop: `select!` over ingest channel, health check
//! interval, and shutdown signal. All state lives in `ConvergenceMonitor`.
//! No concurrent signal mutation — the pipeline is strictly sequential.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use cortex_storage::schema_contract::require_schema_ready;
use cortex_storage::sqlite::{
    apply_reader_pragmas, apply_writer_pragmas, ensure_maintenance_lock_absent,
};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::config::MonitorConfig;
use crate::intervention::actions::InterventionAction;
use crate::intervention::cooldown::{
    CooldownManager, DualKeyConfirmationResult, DualKeyInitiationResult, PendingCriticalAction,
};
use crate::intervention::escalation::EscalationManager;
use crate::intervention::trigger::{CompositeResult, InterventionStateMachine};
use crate::pipeline::signal_computer::SignalComputer;
use crate::pipeline::signal_scheduler::{ComputeTrigger, SignalScheduler};
use crate::pipeline::window_manager::WindowManager;
use crate::session::registry::SessionRegistry;
use crate::state_publisher::{ConvergenceSharedState, StatePublisher};
use crate::transport::http_api::{
    self, AckResult, HttpApiState, InterventionSnapshot, MonitorRequest, ScoreSnapshot,
    SessionSnapshot, ThresholdChangeResult, ThresholdConfirmResult,
};
#[cfg(unix)]
use crate::transport::unix_socket::UnixSocketTransport;
use crate::transport::{EventType, IngestEvent};
use crate::validation::{EventValidator, RateLimiter};

// ── Score cache (AC14: 30s TTL) ─────────────────────────────────────────

#[derive(Debug, Clone)]
struct CachedScore {
    score: f64,
    level: u8,
    signal_scores: [f64; 8],
    cached_at: Instant,
}

// ── Top-level coordinator ───────────────────────────────────────────────

pub struct ConvergenceMonitor {
    config: MonitorConfig,
    signal_computer: SignalComputer,
    signal_scheduler: SignalScheduler,
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
    /// Stores score, level, AND signal_scores so the cache path never
    /// publishes zeroed signals.
    score_cache: BTreeMap<Uuid, CachedScore>,
    /// Per-session hash chain: session_id → last event hash (AC4).
    hash_chains: BTreeMap<Uuid, [u8; 32]>,
    /// Reusable SQLite connection — avoids opening a new connection per
    /// event, critical for the 10K events/sec throughput target.
    db_conn: Option<rusqlite::Connection>,
    /// Flipped to false when DB persistence fails; surfaced via /health.
    db_write_healthy: bool,
}

impl ConvergenceMonitor {
    pub fn new(config: MonitorConfig) -> anyhow::Result<Self> {
        Self::verify_startup_contract(&config)?;
        let state_publisher = StatePublisher::new(config.state_dir.clone());
        let validator = EventValidator::new(config.clock_skew_tolerance);
        let rate_limiter = RateLimiter::new(config.rate_limit_per_min);
        let max_provisional = config.max_provisional_sessions;

        Ok(Self {
            config,
            signal_computer: SignalComputer::new(),
            signal_scheduler: SignalScheduler::new(),
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
            db_conn: None,
            db_write_healthy: true,
        })
    }

    fn verify_startup_contract(config: &MonitorConfig) -> anyhow::Result<()> {
        if !config.db_path.exists() {
            return Err(anyhow::anyhow!(
                "database {} is missing; run `ghost db migrate` before starting the convergence monitor",
                config.db_path.display()
            ));
        }
        ensure_maintenance_lock_absent(&config.db_path)?;

        let conn = rusqlite::Connection::open_with_flags(
            &config.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        apply_reader_pragmas(&conn)?;
        require_schema_ready(&conn)?;
        Ok(())
    }

    fn active_critical_override_threshold(&self) -> f64 {
        self.config
            .intervention_thresholds
            .critical_override_threshold
    }

    fn threshold_values_match(current: f64, actual: f64) -> bool {
        (current - actual).abs() <= f64::EPSILON
    }

    fn mark_db_persistence_failure(&mut self, operation: &str, error: &dyn std::fmt::Display) {
        self.db_write_healthy = false;
        tracing::error!(operation, error = %error, "database persistence failure degraded monitor health");
    }

    fn mark_db_persistence_recovered(&mut self) {
        self.db_write_healthy = true;
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
        if let Err(e) = apply_reader_pragmas(&conn) {
            tracing::warn!(
                error = %e,
                "failed to apply read pragmas during state reconstruction"
            );
        }

        // Restore intervention states per agent
        let mut restored_count = 0u32;
        match conn.prepare(
            "SELECT agent_id, level, consecutive_normal, cooldown_until, \
             ack_required, hysteresis_count, de_escalation_credits \
             FROM intervention_state",
        ) {
            Ok(mut stmt) => match stmt.query_map([], |row| {
                let agent_id_str: String = row.get(0)?;
                let level: u8 = row.get(1)?;
                let consecutive_normal: u32 = row.get(2)?;
                let cooldown_until: Option<String> = row.get(3)?;
                let ack_required: bool = row.get(4)?;
                let hysteresis_count: u32 = row.get(5)?;
                let de_escalation_credits: u32 = row.get(6)?;
                Ok((
                    agent_id_str,
                    level,
                    consecutive_normal,
                    cooldown_until,
                    ack_required,
                    hysteresis_count,
                    de_escalation_credits,
                ))
            }) {
                Ok(rows) => {
                    for row in rows {
                        let (
                            agent_id_str,
                            level,
                            consecutive_normal,
                            cooldown_until_str,
                            ack_required,
                            hysteresis_count,
                            de_escalation_credits,
                        ) = match row {
                            Ok(r) => r,
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    "skipping malformed intervention_state row"
                                );
                                continue;
                            }
                        };

                        let agent_id = match Uuid::parse_str(&agent_id_str) {
                            Ok(id) => id,
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    agent_id = %agent_id_str,
                                    "skipping invalid agent_id"
                                );
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
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to query intervention_state — starting fresh"
                    );
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "intervention_state table not found — starting fresh");
            }
        }

        // Restore calibration counts
        match conn.prepare(
            "SELECT sender, COUNT(*) FROM itp_events \
             WHERE event_type = 'SessionStart' GROUP BY sender",
        ) {
            Ok(mut cal_stmt) => {
                match cal_stmt.query_map([], |row| {
                    let agent_id_str: String = row.get(0)?;
                    let count: u32 = row.get(1)?;
                    Ok((agent_id_str, count))
                }) {
                    Ok(cal_rows) => {
                        for row in cal_rows {
                            match row {
                                Ok((agent_id_str, count)) => match Uuid::parse_str(&agent_id_str) {
                                    Ok(agent_id) => {
                                        self.calibration_counts.insert(agent_id, count);
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            error = %e,
                                            agent_id = %agent_id_str,
                                            "skipping calibration count for invalid agent_id"
                                        );
                                    }
                                },
                                Err(e) => {
                                    tracing::warn!(error = %e, "skipping malformed calibration count row");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to query calibration counts — calibration state may be inaccurate");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to prepare calibration count query — calibration state may be inaccurate");
            }
        }

        // Restore last known scores into cache (stale but conservative)
        match conn.prepare(
            "SELECT agent_id, composite_score, level FROM convergence_scores \
             WHERE rowid IN (SELECT MAX(rowid) FROM convergence_scores GROUP BY agent_id)",
        ) {
            Ok(mut score_stmt) => {
                match score_stmt.query_map([], |row| {
                    let agent_id_str: String = row.get(0)?;
                    let score: f64 = row.get(1)?;
                    let level: u8 = row.get(2)?;
                    Ok((agent_id_str, score, level))
                }) {
                    Ok(score_rows) => {
                        for row in score_rows {
                            match row {
                                Ok((agent_id_str, score, level)) => {
                                    match Uuid::parse_str(&agent_id_str) {
                                        Ok(agent_id) => {
                                            // Guard against NaN/Inf scores from corrupted DB
                                            let safe_score =
                                                if score.is_nan() || score.is_infinite() {
                                                    tracing::warn!(
                                                        agent_id = %agent_id,
                                                        raw_score = %score,
                                                        "non-finite score in DB — clamping to 0.0"
                                                    );
                                                    0.0
                                                } else {
                                                    score.clamp(0.0, 1.0)
                                                };
                                            self.score_cache.insert(
                                                agent_id,
                                                CachedScore {
                                                    score: safe_score,
                                                    level,
                                                    signal_scores: [0.0; 8], // Stale cache from DB — signals unknown
                                                    cached_at: Instant::now(),
                                                },
                                            );
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                error = %e,
                                                agent_id = %agent_id_str,
                                                "skipping score cache for invalid agent_id"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "skipping malformed score cache row");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to query convergence scores — score cache will be empty");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to prepare convergence score query — score cache will be empty");
            }
        }

        match conn.query_row(
            "SELECT critical_override_threshold FROM monitor_threshold_config \
             WHERE config_key = 'critical_override_threshold'",
            [],
            |row| row.get::<_, f64>(0),
        ) {
            Ok(threshold) => {
                self.config
                    .intervention_thresholds
                    .critical_override_threshold = threshold;
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to restore persisted critical override threshold — using config default"
                );
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
        let (recalculate_tx, mut recalculate_rx) = mpsc::channel::<()>(1);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let (monitor_tx, mut monitor_rx) = mpsc::channel::<MonitorRequest>(16);

        // ── Start HTTP API transport ────────────────────────────────
        let http_state = Arc::new(RwLock::new(HttpApiState {
            ingest_tx: ingest_tx.clone(),
            healthy: self.db_write_healthy,
            start_time: std::time::Instant::now(),
            scores: BTreeMap::new(),
            sessions: Vec::new(),
            interventions: BTreeMap::new(),
            agent_count: 0,
            event_count: 0,
            last_computation: None,
            recalculate_tx,
            last_recalculate: None,
            shutdown_tx,
            monitor_tx,
        }));
        let http_state_ref = http_state.clone();
        let router = http_api::build_router(http_state);
        let http_port = self.config.http_port;

        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{http_port}"))
                .await
                .expect("failed to bind HTTP listener");
            tracing::info!("HTTP API listening on port {http_port}");
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!(error = %e, "HTTP API server exited with error");
            }
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
            tracing::info!(
                "unix socket transport not available on this platform — using HTTP API only"
            );
        }

        // ── Start native messaging transport (T-6.5.1) ──────────────
        if self.config.native_messaging_enabled {
            let nm_transport = crate::transport::native_messaging::NativeMessagingTransport::new(
                ingest_tx.clone(),
            );
            tokio::spawn(async move {
                tracing::info!("native messaging transport started");
                if let Err(e) = nm_transport.run().await {
                    tracing::error!("native messaging transport error: {e}");
                }
            });
        }

        tracing::info!("convergence monitor running");

        // Sync initial state from reconstruction into HTTP API
        self.sync_http_state(&http_state_ref).await;

        // ── Single-threaded event loop (A27.3) ─────────────────────
        // select! over: ingest channel, health check interval,
        // cooldown check, shutdown signal.
        let mut health_interval = tokio::time::interval(tokio::time::Duration::from_secs(
            self.config.health_check_interval.as_secs(),
        ));
        let mut cooldown_interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        // Task 19.2: 5-minute and 15-minute timer ticks for signal scheduling.
        // 15-min timer offset by 150s (2.5 min) to avoid thundering herd.
        let mut signal_5min_interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        let mut signal_15min_interval =
            tokio::time::interval(tokio::time::Duration::from_secs(900));
        // Stagger the 15-min timer by 2.5 minutes
        signal_15min_interval.reset_after(tokio::time::Duration::from_secs(150));

        let mut event_count: u64 = 0;

        loop {
            tokio::select! {
                Some(event) = ingest_rx.recv() => {
                    self.handle_event(event);
                    event_count += 1;
                    // Sync state to HTTP API after each event
                    {
                        let mut state = http_state_ref.write().await;
                        state.event_count = event_count;
                    }
                    self.sync_http_state(&http_state_ref).await;
                }
                _ = health_interval.tick() => {
                    tracing::debug!("health check tick");
                    self.sync_http_state(&http_state_ref).await;
                }
                _ = cooldown_interval.tick() => {
                    // Check and expire cooldowns for all agents
                    self.check_cooldowns();
                    self.prune_stale_state();
                    self.sync_http_state(&http_state_ref).await;
                }
                _ = signal_5min_interval.tick() => {
                    tracing::debug!("5-minute signal scheduler tick");
                    self.handle_timer_tick(ComputeTrigger::Timer5Min);
                }
                _ = signal_15min_interval.tick() => {
                    tracing::debug!("15-minute signal scheduler tick");
                    self.handle_timer_tick(ComputeTrigger::Timer15Min);
                }
                Some(()) = recalculate_rx.recv() => {
                    tracing::info!("recalculate-all requested via HTTP API");
                    self.handle_recalculate_all();
                    self.sync_http_state(&http_state_ref).await;
                }
                Some(req) = monitor_rx.recv() => {
                    self.handle_monitor_request(req);
                    self.sync_http_state(&http_state_ref).await;
                }
                Some(()) = shutdown_rx.recv() => {
                    tracing::info!("shutdown requested via HTTP API");
                    // Flush pending scores to DB
                    self.flush_pending_scores();
                    break;
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("shutdown signal received");
                    self.flush_pending_scores();
                    break;
                }
            }
        }

        tracing::info!("convergence monitor stopped");
        Ok(())
    }

    /// Sync monitor state snapshots into the shared HTTP API state.
    async fn sync_http_state(&self, http_state: &Arc<RwLock<HttpApiState>>) {
        let mut state = http_state.write().await;
        state.healthy = self.db_write_healthy;

        // Scores: snapshot from score_cache
        state.scores = self
            .score_cache
            .iter()
            .map(|(agent_id, cached)| {
                (
                    *agent_id,
                    ScoreSnapshot {
                        agent_id: *agent_id,
                        composite_score: cached.score,
                        level: cached.level,
                        signals: cached.signal_scores,
                        computed_at: Utc::now()
                            - chrono::Duration::milliseconds(
                                cached.cached_at.elapsed().as_millis() as i64,
                            ),
                    },
                )
            })
            .collect();

        // Sessions: snapshot active sessions from registry
        state.sessions = self
            .sessions
            .all_active_agent_ids()
            .iter()
            .flat_map(|agent_id| {
                self.sessions
                    .active_sessions(agent_id)
                    .into_iter()
                    .map(|s| SessionSnapshot {
                        session_id: s.session_id,
                        agent_id: s.agent_id,
                        started_at: s.started_at,
                        last_event_at: s.last_event_at,
                        event_count: s.event_count,
                        is_active: s.is_active,
                    })
            })
            .collect();

        // Interventions: snapshot from intervention state machine
        let now = Utc::now();
        state.interventions = self
            .score_cache
            .keys()
            .filter_map(|agent_id| {
                self.intervention.get_state(agent_id).map(|is| {
                    let cooldown_remaining = is
                        .cooldown_until
                        .map(|until| (until - now).num_seconds().max(0));
                    (
                        *agent_id,
                        InterventionSnapshot {
                            agent_id: *agent_id,
                            level: is.level,
                            cooldown_remaining_secs: cooldown_remaining,
                            ack_required: is.ack_required,
                            consecutive_normal: is.consecutive_normal,
                        },
                    )
                })
            })
            .collect();

        // Status metadata
        state.agent_count = self.score_cache.len();
        state.last_computation = if self.score_cache.is_empty() {
            None
        } else {
            // Approximate: most recent cached_at
            self.score_cache
                .values()
                .map(|c| {
                    Utc::now()
                        - chrono::Duration::milliseconds(c.cached_at.elapsed().as_millis() as i64)
                })
                .max()
        };
    }

    /// Recompute scores for all tracked agents (T-6.2.7).
    fn handle_recalculate_all(&mut self) {
        let agents: Vec<Uuid> = self.score_cache.keys().copied().collect();
        for agent_id in agents {
            self.compute_score(agent_id);
        }
        tracing::info!("recalculated scores for all tracked agents");
    }

    /// Handle a request from the HTTP API (T-6.3.2, T-6.4.1, T-6.4.2).
    fn handle_monitor_request(&mut self, req: MonitorRequest) {
        match req {
            // T-6.3.2: Acknowledge a Level 2 intervention (Req 9 AC4).
            MonitorRequest::Acknowledge { agent_id, reply } => {
                let result = match self.intervention.get_state(&agent_id) {
                    None => AckResult::NotFound,
                    Some(state) if state.level == 2 && state.ack_required => {
                        self.intervention.acknowledge(agent_id);
                        if let Err(e) = self.persist_intervention_state(agent_id) {
                            tracing::error!(
                                agent_id = %agent_id,
                                error = %e,
                                "failed to persist state after acknowledge"
                            );
                        }
                        tracing::info!(agent_id = %agent_id, "Level 2 intervention acknowledged");
                        AckResult::Ok
                    }
                    Some(_) => AckResult::NotLevel2,
                };
                let _ = reply.send(result);
            }
            // T-6.4.2: Propose a threshold change (CS§ dual-key).
            MonitorRequest::ThresholdChange {
                initiator,
                current,
                proposed,
                reply,
            } => {
                let actual = self.active_critical_override_threshold();
                let result = if !Self::threshold_values_match(current, actual) {
                    ThresholdChangeResult::CurrentMismatch { actual }
                } else if proposed < self.cooldown.threshold_floor {
                    ThresholdChangeResult::Rejected {
                        reason: format!(
                            "proposed value {proposed} is below threshold floor {}",
                            self.cooldown.threshold_floor
                        ),
                    }
                } else if self.cooldown.pending_dual_key_change.is_some() {
                    let pending = self
                        .cooldown
                        .pending_dual_key_change
                        .as_ref()
                        .expect("pending change checked above");
                    ThresholdChangeResult::AlreadyPending {
                        intended_action: pending.intended_action.clone(),
                        expires_in_secs: (pending.expires_at - chrono::Utc::now())
                            .num_seconds()
                            .max(0) as u64,
                    }
                } else if self.cooldown.is_critical_change(current, proposed) {
                    // Lowering the runtime threshold is safety-critical and must be dual-key confirmed.
                    match self.cooldown.initiate_dual_key_change(
                        initiator,
                        PendingCriticalAction::ThresholdChange { current, proposed },
                        chrono::Utc::now(),
                        self.config.dual_key_ttl,
                    ) {
                        DualKeyInitiationResult::Initiated { token, expires_at } => {
                            tracing::info!(
                                current,
                                proposed,
                                expires_at = %expires_at,
                                "critical threshold change initiated — dual-key required"
                            );
                            ThresholdChangeResult::DualKeyRequired {
                                token,
                                expires_in_secs: (expires_at - chrono::Utc::now())
                                    .num_seconds()
                                    .max(0) as u64,
                            }
                        }
                        DualKeyInitiationResult::AlreadyPending {
                            intended_action,
                            expires_at,
                        } => ThresholdChangeResult::AlreadyPending {
                            intended_action,
                            expires_in_secs: (expires_at - chrono::Utc::now()).num_seconds().max(0)
                                as u64,
                        },
                    }
                } else if self.cooldown.can_change_threshold(current, proposed) {
                    match self.apply_threshold_change(proposed, &initiator, None, "immediate") {
                        Ok(()) => {
                            tracing::info!(current, proposed, "threshold change applied");
                            ThresholdChangeResult::Applied
                        }
                        Err(e) => ThresholdChangeResult::Rejected {
                            reason: format!("failed to persist threshold change: {e}"),
                        },
                    }
                } else {
                    let reason = if self.cooldown.config_locked {
                        "config locked during active sessions".to_string()
                    } else {
                        format!("proposed value {proposed} is below threshold floor")
                    };
                    ThresholdChangeResult::Rejected { reason }
                };
                let _ = reply.send(result);
            }
            // T-6.4.2: Confirm a dual-key threshold change.
            MonitorRequest::ThresholdConfirm {
                token,
                confirmer,
                reply,
            } => {
                let confirmation =
                    self.cooldown
                        .confirm_dual_key_change(&token, &confirmer, chrono::Utc::now());
                match &confirmation {
                    DualKeyConfirmationResult::Confirmed { pending_change } => {
                        tracing::info!(
                            confirmer,
                            intended_action = pending_change.intended_action,
                            "dual-key threshold change confirmed"
                        );
                    }
                    DualKeyConfirmationResult::MissingPending => {
                        tracing::warn!(
                            "dual-key threshold confirmation failed (no pending change)"
                        );
                    }
                    DualKeyConfirmationResult::InvalidToken => {
                        tracing::warn!("dual-key threshold confirmation failed (invalid token)");
                    }
                    DualKeyConfirmationResult::Expired { pending_change } => {
                        tracing::warn!(
                            intended_action = pending_change.intended_action,
                            "dual-key threshold confirmation failed (expired token)"
                        );
                    }
                    DualKeyConfirmationResult::SameActorRejected => {
                        tracing::warn!(
                            confirmer,
                            "dual-key threshold confirmation failed (same actor)"
                        );
                    }
                }
                let result = match confirmation {
                    DualKeyConfirmationResult::Confirmed { pending_change } => {
                        let apply_result = match pending_change.action {
                            PendingCriticalAction::ThresholdChange { current, proposed } => {
                                let actual = self.active_critical_override_threshold();
                                if !Self::threshold_values_match(current, actual) {
                                    Err(anyhow::anyhow!(
                                        "threshold drift detected during confirmation: active={actual}, pending_current={current}"
                                    ))
                                } else {
                                    self.apply_threshold_change(
                                        proposed,
                                        &pending_change.initiator,
                                        Some(&confirmer),
                                        "dual_key",
                                    )
                                }
                            }
                        };

                        match apply_result {
                            Ok(()) => ThresholdConfirmResult::Applied,
                            Err(e) => ThresholdConfirmResult::ApplyFailed {
                                reason: e.to_string(),
                            },
                        }
                    }
                    DualKeyConfirmationResult::MissingPending => {
                        ThresholdConfirmResult::MissingPending
                    }
                    DualKeyConfirmationResult::InvalidToken => ThresholdConfirmResult::InvalidToken,
                    DualKeyConfirmationResult::Expired { pending_change } => {
                        ThresholdConfirmResult::Expired {
                            intended_action: pending_change.intended_action,
                        }
                    }
                    DualKeyConfirmationResult::SameActorRejected => {
                        ThresholdConfirmResult::SameActorRejected
                    }
                };
                let _ = reply.send(result);
            }
        }
    }

    fn apply_threshold_change(
        &mut self,
        proposed: f64,
        initiated_by: &str,
        confirmed_by: Option<&str>,
        change_mode: &str,
    ) -> anyhow::Result<()> {
        let previous = self.active_critical_override_threshold();
        if Self::threshold_values_match(previous, proposed) {
            return Ok(());
        }

        self.config
            .intervention_thresholds
            .critical_override_threshold = proposed;
        self.score_cache.clear();
        self.persist_threshold_change(previous, proposed, initiated_by, confirmed_by, change_mode)?;
        Ok(())
    }

    /// Flush any pending scores to the database before shutdown (T-6.2.8).
    fn flush_pending_scores(&mut self) {
        let cached: Vec<(Uuid, f64, u8)> = self
            .score_cache
            .iter()
            .map(|(id, c)| (*id, c.score, c.level))
            .collect();
        for (agent_id, score, level) in cached {
            tracing::debug!(agent_id = %agent_id, "flushing cached score before shutdown");
            if let Err(e) = self.persist_convergence_score(
                agent_id,
                score,
                level,
                Uuid::nil(), // No specific session for flush
                &[0.0; 8],
                &[0u8; 32],
                &[0u8; 32],
            ) {
                tracing::error!(agent_id = %agent_id, error = %e, "failed to flush score on shutdown");
            }
        }
        // Persist intervention states
        let agent_ids: Vec<Uuid> = self.score_cache.keys().copied().collect();
        for agent_id in agent_ids {
            if let Err(e) = self.persist_intervention_state(agent_id) {
                tracing::error!(agent_id = %agent_id, error = %e, "failed to flush intervention state on shutdown");
            }
        }
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
        if self
            .validator
            .validate_session_id(&event.session_id)
            .is_err()
        {
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
                let closed =
                    self.sessions
                        .start_session(event.session_id, event.agent_id, event.timestamp);
                for sid in closed {
                    tracing::info!(
                        session_id = %sid,
                        "synthetic SessionEnd (mid-session restart, AC13)"
                    );
                    self.window_manager.record_session_end(event.agent_id);
                }

                // Track calibration count
                *self.calibration_counts.entry(event.agent_id).or_insert(0) += 1;

                // Task 19.2: session boundary — mark all signals dirty, reset counter
                self.signal_scheduler
                    .record_session_boundary(event.agent_id);

                // T-6.4.1: Lock config during active sessions (A8).
                self.cooldown.lock_config();
            }
            EventType::SessionEnd => {
                self.sessions.end_session(event.session_id);
                self.window_manager.record_session_end(event.agent_id);

                // Task 19.2: session boundary — mark all signals dirty, reset counter
                self.signal_scheduler
                    .record_session_boundary(event.agent_id);

                // T-6.5.3: Mark signals dirty at session boundary.
                // S2 (inter-session gap) and S6 (initiative balance) depend on session boundaries.
                self.signal_computer.mark_dirty(event.agent_id, 1); // S2
                self.signal_computer.mark_dirty(event.agent_id, 5); // S6

                // T-6.3.1: Try de-escalation at session boundary (Req 9 AC3).
                // A session is "normal" if the cached score level is below the
                // agent's current intervention level.
                let session_was_normal = {
                    let cached_level = self.score_cache.get(&event.agent_id).map_or(0, |c| c.level);
                    let intervention_level = self
                        .intervention
                        .get_state(&event.agent_id)
                        .map_or(0, |s| s.level);
                    cached_level < intervention_level
                };
                if self
                    .intervention
                    .try_deescalate(event.agent_id, session_was_normal)
                {
                    tracing::info!(
                        agent_id = %event.agent_id,
                        "de-escalation at session boundary"
                    );
                    if let Err(e) = self.persist_intervention_state(event.agent_id) {
                        tracing::error!(
                            agent_id = %event.agent_id,
                            error = %e,
                            "failed to persist intervention state after de-escalation"
                        );
                    }
                }

                // T-6.4.1: Unlock config when no active sessions remain (A8).
                if !self.sessions.has_active_sessions() {
                    self.cooldown.unlock_config();
                }
            }
            EventType::InteractionMessage => {
                self.sessions
                    .record_event(event.session_id, event.timestamp);

                // Task 19.2: record message for signal scheduling
                self.signal_scheduler.record_message(event.agent_id);

                // T-6.5.3: Mark signals dirty on message receipt.
                // S1 (session duration), S3 (response latency), S4 (vocabulary convergence).
                self.signal_computer.mark_dirty(event.agent_id, 0); // S1
                self.signal_computer.mark_dirty(event.agent_id, 2); // S3
                self.signal_computer.mark_dirty(event.agent_id, 3); // S4
            }
            EventType::AgentStateSnapshot => {
                self.sessions
                    .record_event(event.session_id, event.timestamp);
                self.signal_scheduler.record_message(event.agent_id);

                // T-6.5.3: State snapshots may affect boundary/disengagement signals.
                // S5 (goal boundary erosion), S7 (disengagement resistance).
                self.signal_computer.mark_dirty(event.agent_id, 4); // S5
                self.signal_computer.mark_dirty(event.agent_id, 6); // S7
            }
            EventType::ConvergenceAlert => {
                self.sessions
                    .record_event(event.session_id, event.timestamp);
                self.signal_scheduler.record_message(event.agent_id);

                // T-6.5.3: Convergence alerts affect behavioral anomaly detection.
                // S8 (behavioral anomaly).
                self.signal_computer.mark_dirty(event.agent_id, 7); // S8
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
                // Use cached score AND cached signals — never publish zeroed placeholders
                (cached.score, cached.level, cached.signal_scores)
            } else {
                self.compute_score(event.agent_id)
            }
        } else {
            self.compute_score(event.agent_id)
        };

        // ── Step 10: Persist score BEFORE intervention ──────────────
        // Audit trail completeness (Req 9 AC6): score is persisted
        // before any intervention action is taken.
        if let Err(e) = self.persist_convergence_score(
            event.agent_id,
            score,
            level,
            event.session_id,
            &signals,
            &event_hash,
            &previous_hash,
        ) {
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

        let previous_level = self
            .intervention
            .get_state(&event.agent_id)
            .map_or(0u8, |s| s.level);

        if let Some(action) = self.intervention.evaluate(&result, event.agent_id) {
            tracing::info!(
                agent_id = %event.agent_id,
                ?action,
                score = format_args!("{score:.3}"),
                level,
                "intervention triggered"
            );

            // Persist intervention state after every evaluation that triggers an action.
            if let Err(e) = self.persist_intervention_state(event.agent_id) {
                tracing::error!(
                    agent_id = %event.agent_id,
                    error = %e,
                    "failed to persist intervention state"
                );
            }

            // Persist to intervention_history (append-only audit trail).
            if let Err(e) = self.persist_intervention_history(
                event.agent_id,
                event.session_id,
                level,
                previous_level,
                score,
                &signals,
                &format!("{action:?}"),
                &event_hash,
                &previous_hash,
            ) {
                tracing::error!(
                    agent_id = %event.agent_id,
                    error = %e,
                    "failed to persist intervention history"
                );
            }

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
    fn compute_score(&mut self, agent_id: Uuid) -> (f64, u8, [f64; 8]) {
        // ── Step 8: Compute signals (dirty-flag throttled) ──────────
        let signals = self.signal_computer.compute(agent_id);

        // ── Step 9: Composite score (weighted, with amplification) ───
        // NaN guard: treat any NaN signal as 0.0 to prevent NaN
        // propagation through weighted_sum → base_score → persistence.
        // Without this, NaN passes through clamp(0.0, 1.0) unchanged
        // and gets persisted to the database and published to shared state.
        let sanitized_signals: [f64; 8] = {
            let mut s = signals;
            for v in s.iter_mut() {
                if v.is_nan() {
                    *v = 0.0;
                }
            }
            s
        };

        let weighted_sum: f64 = sanitized_signals
            .iter()
            .zip(self.config.signal_weights.iter())
            .map(|(s, w)| s * w)
            .sum();
        let weight_total: f64 = self.config.signal_weights.iter().sum();
        let base_score = if weight_total > 0.0 {
            weighted_sum / weight_total
        } else {
            sanitized_signals.iter().sum::<f64>() / 8.0
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
        let critical_override_threshold = self.active_critical_override_threshold();
        let critical_override = signals[0] > critical_override_threshold
            || signals[1] > critical_override_threshold
            || signals[3] > critical_override_threshold;

        let mut level = score_to_level(score);
        if critical_override && level < 2 {
            level = 2;
        }

        // Cache the score and signals
        self.score_cache.insert(
            agent_id,
            CachedScore {
                score,
                level,
                signal_scores: signals,
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
        if let Some(expired) = self.cooldown.prune_expired_dual_key_change(now) {
            tracing::info!(
                initiator = expired.initiator,
                intended_action = expired.intended_action,
                expired_at = %expired.expires_at,
                "expired pending dual-key change pruned"
            );
        }
        let mut expired_agents = Vec::new();
        for (agent_id, state) in self.intervention.states_mut() {
            if let Some(until) = state.cooldown_until {
                if now >= until {
                    state.cooldown_until = None;
                    tracing::info!(agent_id = %agent_id, "cooldown expired");
                    expired_agents.push(*agent_id);
                }
            }
        }
        // Persist state for agents whose cooldown expired.
        for agent_id in expired_agents {
            if let Err(e) = self.persist_intervention_state(agent_id) {
                tracing::error!(agent_id = %agent_id, error = %e, "failed to persist intervention state after cooldown expiry");
            }
        }
    }

    fn prune_stale_state(&mut self) {
        let now = chrono::Utc::now();
        let idle_horizon = chrono::Duration::from_std(self.config.session_idle_horizon)
            .unwrap_or_else(|_| chrono::Duration::minutes(30));
        let pruned_sessions = self.sessions.prune_stale(now, idle_horizon);
        if !pruned_sessions.session_ids.is_empty() {
            tracing::info!(
                count = pruned_sessions.session_ids.len(),
                "pruned stale sessions"
            );
        }
        if !pruned_sessions.provisional_agent_ids.is_empty() {
            tracing::info!(
                count = pruned_sessions.provisional_agent_ids.len(),
                "pruned stale provisional agents"
            );
        }
        if !self.sessions.has_active_sessions() {
            self.cooldown.unlock_config();
        }
        self.rate_limiter
            .prune_idle(self.config.rate_limit_bucket_idle_horizon);
    }

    /// Handle a timer tick (5-min or 15-min) for signal scheduling (Task 19.2).
    ///
    /// For all active agents, compute signals whose tier matches the trigger.
    /// Timer ticks are independent of event ingestion — they fire even if
    /// no events are received.
    fn handle_timer_tick(&mut self, trigger: ComputeTrigger) {
        let active_agents: Vec<Uuid> = self.sessions.all_active_agent_ids();
        for agent_id in active_agents {
            let mut computed = Vec::new();
            for i in 0..8 {
                if self.signal_scheduler.should_compute(agent_id, i, &trigger) {
                    computed.push(i);
                    self.signal_scheduler.mark_computed(agent_id, i);
                }
            }
            if !computed.is_empty() {
                tracing::debug!(
                    agent_id = %agent_id,
                    trigger = ?trigger,
                    signals = ?computed,
                    "timer-triggered signal computation"
                );
            }
        }
    }

    /// Get or create the reusable SQLite connection.
    ///
    /// If the cached connection is stale (e.g., disk full, corruption),
    /// drops it and creates a fresh one. A single DB error does NOT
    /// permanently break the monitor.
    fn get_db_conn(&mut self) -> anyhow::Result<&mut rusqlite::Connection> {
        // Probe the existing connection with a cheap no-op query.
        // If it fails, drop it so we reconnect below.
        if let Some(ref conn) = self.db_conn {
            if conn.execute_batch("SELECT 1").is_err() {
                tracing::warn!("cached SQLite connection is stale — reconnecting");
                self.db_conn = None;
            }
        }

        if self.db_conn.is_none() {
            let conn = rusqlite::Connection::open(&self.config.db_path)?;
            apply_writer_pragmas(&conn)?;
            self.db_conn = Some(conn);
        }
        Ok(self.db_conn.as_mut().unwrap())
    }

    /// Persist an ITP event to the database with hash chain linkage (AC4).
    fn persist_itp_event(
        &mut self,
        event: &IngestEvent,
        event_hash: &[u8; 32],
        previous_hash: &[u8; 32],
    ) -> anyhow::Result<()> {
        let result = (|| -> anyhow::Result<()> {
            let conn = self.get_db_conn()?;
            let id = Uuid::now_v7().to_string();
            let payload_str = event.payload.to_string();
            let content_hash = blake3::hash(payload_str.as_bytes()).to_hex().to_string();
            let content_length = payload_str.len() as i64;
            conn.execute(
                "INSERT INTO itp_events (id, session_id, event_type, sender, \
                 timestamp, sequence_number, content_hash, content_length, privacy_level, \
                 event_hash, previous_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, \
                 (SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM itp_events WHERE session_id = ?2), \
                 ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    id,
                    event.session_id.to_string(),
                    format!("{:?}", event.event_type),
                    event.agent_id.to_string(),
                    event.timestamp.to_rfc3339(),
                    content_hash,
                    content_length,
                    "standard",
                    event_hash.as_slice(),
                    previous_hash.as_slice(),
                ],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.mark_db_persistence_recovered();
                Ok(())
            }
            Err(error) => {
                self.mark_db_persistence_failure("persist_itp_event", &error);
                Err(error)
            }
        }
    }

    /// Persist a convergence score to the database (Step 10, Req 9 AC6).
    /// Score MUST be persisted BEFORE any intervention action is taken.
    fn persist_convergence_score(
        &mut self,
        agent_id: Uuid,
        score: f64,
        level: u8,
        session_id: Uuid,
        signal_scores: &[f64; 8],
        event_hash: &[u8; 32],
        previous_hash: &[u8; 32],
    ) -> anyhow::Result<()> {
        let default_profile = self.config.default_profile.clone();
        let result = (|| -> anyhow::Result<()> {
            let conn = self.get_db_conn()?;
            let id = Uuid::now_v7().to_string();
            let signal_json = serde_json::to_string(signal_scores)?;
            let profile = match conn.query_row(
                "SELECT profile_name FROM agent_profile_assignments WHERE agent_id = ?1",
                [agent_id.to_string()],
                |row| row.get::<_, String>(0),
            ) {
                Ok(profile) => profile,
                Err(rusqlite::Error::QueryReturnedNoRows) => default_profile.clone(),
                Err(error) => return Err(error.into()),
            };
            conn.execute(
                "INSERT INTO convergence_scores (id, agent_id, session_id, composite_score, \
                 signal_scores, level, profile, computed_at, event_hash, previous_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    id,
                    agent_id.to_string(),
                    session_id.to_string(),
                    score,
                    signal_json,
                    level as i32,
                    profile,
                    Utc::now().to_rfc3339(),
                    event_hash.as_slice(),
                    previous_hash.as_slice(),
                ],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.mark_db_persistence_recovered();
                Ok(())
            }
            Err(error) => {
                self.mark_db_persistence_failure("persist_convergence_score", &error);
                Err(error)
            }
        }
    }

    /// Persist the current intervention state for an agent to SQLite.
    /// Uses INSERT OR REPLACE (upsert) since intervention_state has one row per agent.
    fn persist_intervention_state(&mut self, agent_id: Uuid) -> anyhow::Result<()> {
        let state = self.intervention.get_state(&agent_id);
        let state = match state {
            Some(s) => s.clone(),
            None => return Ok(()), // No state to persist
        };
        let result = (|| -> anyhow::Result<()> {
            let conn = self.get_db_conn()?;
            conn.execute(
                "INSERT OR REPLACE INTO intervention_state \
                 (agent_id, level, consecutive_normal, cooldown_until, \
                  ack_required, hysteresis_count, de_escalation_credits, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
                rusqlite::params![
                    agent_id.to_string(),
                    state.level as i32,
                    state.consecutive_normal as i32,
                    state.cooldown_until.map(|t| t.to_rfc3339()),
                    state.ack_required,
                    state.hysteresis_count as i32,
                    state.de_escalation_credits as i32,
                ],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.mark_db_persistence_recovered();
                Ok(())
            }
            Err(error) => {
                self.mark_db_persistence_failure("persist_intervention_state", &error);
                Err(error)
            }
        }
    }

    fn persist_threshold_change(
        &mut self,
        previous: f64,
        proposed: f64,
        initiated_by: &str,
        confirmed_by: Option<&str>,
        change_mode: &str,
    ) -> anyhow::Result<()> {
        let result = (|| -> anyhow::Result<()> {
            let conn = self.get_db_conn()?;
            let tx = conn.transaction()?;
            let now = Utc::now().to_rfc3339();
            let updated_by = confirmed_by.unwrap_or(initiated_by);
            tx.execute(
                "INSERT INTO monitor_threshold_config \
                 (config_key, critical_override_threshold, updated_at, updated_by, confirmed_by) \
                 VALUES ('critical_override_threshold', ?1, ?2, ?3, ?4) \
                 ON CONFLICT(config_key) DO UPDATE SET
                    critical_override_threshold = excluded.critical_override_threshold,
                    updated_at = excluded.updated_at,
                    updated_by = excluded.updated_by,
                    confirmed_by = excluded.confirmed_by",
                rusqlite::params![proposed, now, updated_by, confirmed_by],
            )?;
            tx.execute(
                "INSERT INTO monitor_threshold_history \
                 (id, config_key, previous_value, new_value, initiated_by, confirmed_by, change_mode, changed_at) \
                 VALUES (?1, 'critical_override_threshold', ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    Uuid::now_v7().to_string(),
                    previous,
                    proposed,
                    initiated_by,
                    confirmed_by,
                    change_mode,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            tx.commit()?;
            tracing::info!(
                previous,
                proposed,
                initiated_by,
                confirmed_by,
                change_mode,
                "persisted critical override threshold change"
            );
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.mark_db_persistence_recovered();
                Ok(())
            }
            Err(error) => {
                self.mark_db_persistence_failure("persist_threshold_change", &error);
                Err(error)
            }
        }
    }

    /// Persist an intervention level transition to intervention_history (append-only).
    fn persist_intervention_history(
        &mut self,
        agent_id: Uuid,
        session_id: Uuid,
        level: u8,
        previous_level: u8,
        trigger_score: f64,
        signal_scores: &[f64; 8],
        action_type: &str,
        event_hash: &[u8; 32],
        previous_hash: &[u8; 32],
    ) -> anyhow::Result<()> {
        let result = (|| -> anyhow::Result<()> {
            let conn = self.get_db_conn()?;
            let id = Uuid::now_v7().to_string();
            let trigger_signals = serde_json::to_string(signal_scores)?;
            conn.execute(
                "INSERT INTO intervention_history (id, agent_id, session_id, intervention_level, \
                 previous_level, trigger_score, trigger_signals, action_type, \
                 event_hash, previous_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    id,
                    agent_id.to_string(),
                    session_id.to_string(),
                    level as i32,
                    previous_level as i32,
                    trigger_score,
                    trigger_signals,
                    action_type,
                    event_hash.as_slice(),
                    previous_hash.as_slice(),
                ],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.mark_db_persistence_recovered();
                Ok(())
            }
            Err(error) => {
                self.mark_db_persistence_failure("persist_intervention_history", &error);
                Err(error)
            }
        }
    }
}

// ── Pure functions ──────────────────────────────────────────────────────

/// Map composite score to intervention level.
///
/// NaN is treated as safe (L0) — a corrupted signal must never
/// escalate to L4 and trigger external notifications.
///
/// ```text
/// NaN         → L0 (safe default)
/// [0.0, 0.3)  → L0 (passive)
/// [0.3, 0.5)  → L1 (soft notification)
/// [0.5, 0.7)  → L2 (active intervention)
/// [0.7, 0.85) → L3 (hard boundary)
/// [0.85, 1.0] → L4 (external escalation)
/// ```
fn score_to_level(score: f64) -> u8 {
    if score.is_nan() {
        tracing::warn!("score_to_level received NaN — defaulting to L0 (safe)");
        return 0;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::http_api::{
        MonitorRequest, ThresholdChangeResult, ThresholdConfirmResult,
    };

    fn prepare_monitor_db(db_path: &std::path::Path) {
        let conn = rusqlite::Connection::open(db_path).unwrap();
        cortex_storage::sqlite::apply_writer_pragmas(&conn).unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
    }

    fn test_config(db_path: std::path::PathBuf, state_dir: std::path::PathBuf) -> MonitorConfig {
        prepare_monitor_db(&db_path);
        let mut config = MonitorConfig::default();
        config.db_path = db_path;
        config.state_dir = state_dir;
        config
    }

    #[test]
    fn threshold_change_updates_runtime_and_persists_audit_state() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("monitor.sqlite");
        let state_dir = temp.path().join("state");
        let config = test_config(db_path.clone(), state_dir);
        let mut monitor = ConvergenceMonitor::new(config).unwrap();

        monitor
            .apply_threshold_change(0.9, "alice", None, "immediate")
            .unwrap();

        assert_eq!(monitor.active_critical_override_threshold(), 0.9);

        let conn = rusqlite::Connection::open(db_path).unwrap();
        let persisted: f64 = conn
            .query_row(
                "SELECT critical_override_threshold FROM monitor_threshold_config \
                 WHERE config_key = 'critical_override_threshold'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted, 0.9);

        let history: (f64, f64, String, String) = conn
            .query_row(
                "SELECT previous_value, new_value, initiated_by, change_mode \
                 FROM monitor_threshold_history",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            history,
            (0.85, 0.9, "alice".to_string(), "immediate".to_string())
        );
    }

    #[test]
    fn threshold_configuration_is_restored_on_restart_without_other_state_tables() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("monitor.sqlite");
        let state_dir = temp.path().join("state");
        let config = test_config(db_path.clone(), state_dir.clone());
        let mut monitor = ConvergenceMonitor::new(config.clone()).unwrap();
        monitor
            .apply_threshold_change(0.92, "alice", None, "immediate")
            .unwrap();

        let mut restarted = ConvergenceMonitor::new(test_config(db_path, state_dir)).unwrap();
        restarted.reconstruct_state();

        assert_eq!(restarted.active_critical_override_threshold(), 0.92);
    }

    #[test]
    fn dual_key_threshold_confirmation_applies_runtime_change() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("monitor.sqlite");
        let state_dir = temp.path().join("state");
        let mut monitor = ConvergenceMonitor::new(test_config(db_path, state_dir)).unwrap();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (change_tx, change_rx) = tokio::sync::oneshot::channel();
        monitor.handle_monitor_request(MonitorRequest::ThresholdChange {
            initiator: "alice".to_string(),
            current: 0.85,
            proposed: 0.8,
            reply: change_tx,
        });
        let token = match runtime.block_on(change_rx).unwrap() {
            ThresholdChangeResult::DualKeyRequired { token, .. } => token,
            other => panic!("expected dual-key challenge, got {other:?}"),
        };

        let (confirm_tx, confirm_rx) = tokio::sync::oneshot::channel();
        monitor.handle_monitor_request(MonitorRequest::ThresholdConfirm {
            token,
            confirmer: "bob".to_string(),
            reply: confirm_tx,
        });
        assert_eq!(
            runtime.block_on(confirm_rx).unwrap(),
            ThresholdConfirmResult::Applied
        );
        assert_eq!(monitor.active_critical_override_threshold(), 0.8);
    }

    #[test]
    fn db_write_failure_degrades_monitor_health() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("monitor.sqlite");
        let state_dir = temp.path().join("state");
        let mut monitor = ConvergenceMonitor::new(test_config(db_path, state_dir)).unwrap();

        monitor.db_conn = Some(rusqlite::Connection::open_in_memory().unwrap());
        let result = monitor.persist_convergence_score(
            Uuid::new_v4(),
            0.7,
            2,
            Uuid::new_v4(),
            &[0.0; 8],
            &[1u8; 32],
            &[0u8; 32],
        );

        assert!(
            result.is_err(),
            "persistence should fail against wrong schema"
        );
        assert!(
            !monitor.db_write_healthy,
            "monitor health should degrade on DB persistence failure"
        );
    }

    #[test]
    fn profile_assignment_lookup_failure_degrades_monitor_health() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("monitor.sqlite");
        let state_dir = temp.path().join("state");
        let mut monitor = ConvergenceMonitor::new(test_config(db_path, state_dir)).unwrap();

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::sqlite::apply_writer_pragmas(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE convergence_scores (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                session_id TEXT,
                composite_score REAL NOT NULL,
                signal_scores TEXT NOT NULL,
                level INTEGER NOT NULL,
                profile TEXT NOT NULL DEFAULT 'standard',
                computed_at TEXT NOT NULL,
                event_hash BLOB NOT NULL,
                previous_hash BLOB NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        monitor.db_conn = Some(conn);

        let result = monitor.persist_convergence_score(
            Uuid::new_v4(),
            0.4,
            1,
            Uuid::new_v4(),
            &[0.0; 8],
            &[1u8; 32],
            &[0u8; 32],
        );

        assert!(
            result.is_err(),
            "profile lookup failure should not silently fall back"
        );
        assert!(
            !monitor.db_write_healthy,
            "monitor health should degrade on profile lookup failure"
        );
    }
}
