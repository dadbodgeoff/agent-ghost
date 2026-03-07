//! Cost tracker: per-agent daily totals, per-session totals (Req 27 AC1).
//!
//! WP4-A: Persist/restore cost state across restarts via `cost_snapshots` table.

use dashmap::DashMap;
use rusqlite::Connection;
use uuid::Uuid;

/// Pre-captured snapshot of cost state for persistence.
/// Holds plain Vecs — no DashMap locks held.
pub struct CostSnapshot {
    pub daily: Vec<(String, f64)>,
    pub sessions: Vec<(String, f64)>,
    pub compaction: Vec<(String, f64)>,
}

/// Cost tracker with per-agent and per-session tracking.
pub struct CostTracker {
    /// Per-agent daily totals.
    agent_daily: DashMap<Uuid, f64>,
    /// Per-session totals.
    session_totals: DashMap<Uuid, f64>,
    /// Compaction cost (tracked separately from user cost).
    compaction_cost: DashMap<Uuid, f64>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            agent_daily: DashMap::new(),
            session_totals: DashMap::new(),
            compaction_cost: DashMap::new(),
        }
    }

    /// Record a cost for an agent in a session.
    pub fn record(&self, agent_id: Uuid, session_id: Uuid, cost: f64, is_compaction: bool) {
        *self.agent_daily.entry(agent_id).or_insert(0.0) += cost;
        *self.session_totals.entry(session_id).or_insert(0.0) += cost;
        if is_compaction {
            *self.compaction_cost.entry(agent_id).or_insert(0.0) += cost;
        }
    }

    /// Get daily total for an agent.
    pub fn get_daily_total(&self, agent_id: Uuid) -> f64 {
        self.agent_daily.get(&agent_id).map(|v| *v).unwrap_or(0.0)
    }

    /// Get session total.
    pub fn get_session_total(&self, session_id: Uuid) -> f64 {
        self.session_totals
            .get(&session_id)
            .map(|v| *v)
            .unwrap_or(0.0)
    }

    /// Get compaction cost for an agent.
    pub fn get_compaction_cost(&self, agent_id: Uuid) -> f64 {
        self.compaction_cost
            .get(&agent_id)
            .map(|v| *v)
            .unwrap_or(0.0)
    }

    /// Reset daily totals (called at midnight).
    pub fn reset_daily(&self) {
        self.agent_daily.clear();
        self.compaction_cost.clear();
    }

    /// Snapshot all cost data into plain Vecs (releases DashMap shard locks immediately).
    /// Call this *before* acquiring the DB writer lock.
    pub fn snapshot(&self) -> CostSnapshot {
        CostSnapshot {
            daily: self.agent_daily.iter()
                .map(|e| (e.key().to_string(), *e.value()))
                .collect(),
            sessions: self.session_totals.iter()
                .map(|e| (e.key().to_string(), *e.value()))
                .collect(),
            compaction: self.compaction_cost.iter()
                .map(|e| (e.key().to_string(), *e.value()))
                .collect(),
        }
    }

    /// WP4-A: Persist a pre-captured snapshot to the `cost_snapshots` table.
    /// Uses UPSERT keyed on (scope, entity_id, date).
    ///
    /// Caller should: (1) call `snapshot()` (no DB lock needed),
    /// (2) acquire writer, (3) call `persist_snapshot()`.
    pub fn persist_snapshot(snapshot: &CostSnapshot, conn: &Connection) -> Result<(), String> {
        let total = snapshot.daily.len() + snapshot.sessions.len() + snapshot.compaction.len();
        if total == 0 {
            return Ok(());
        }

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        conn.execute_batch("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;

        let upsert_sql = "INSERT INTO cost_snapshots (scope, entity_id, amount, snapshot_date, updated_at)
                          VALUES (?1, ?2, ?3, ?4, ?5)
                          ON CONFLICT(scope, entity_id, snapshot_date)
                          DO UPDATE SET amount = ?3, updated_at = ?5";

        let mut stmt = conn.prepare(upsert_sql)
            .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;

        for (id, amount) in &snapshot.daily {
            stmt.execute(rusqlite::params!["agent_daily", id, amount, today, now])
                .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;
        }
        for (id, amount) in &snapshot.sessions {
            stmt.execute(rusqlite::params!["session", id, amount, today, now])
                .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;
        }
        for (id, amount) in &snapshot.compaction {
            stmt.execute(rusqlite::params!["compaction", id, amount, today, now])
                .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;
        }

        drop(stmt);
        conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
        tracing::debug!(entries = total, "cost tracker state persisted");
        Ok(())
    }

    /// Convenience: snapshot + persist in one call (for shutdown path where
    /// caller already holds the writer lock).
    pub fn persist(&self, conn: &Connection) -> Result<(), String> {
        let snap = self.snapshot();
        Self::persist_snapshot(&snap, conn)
    }

    /// WP4-A: Restore cost state from `cost_snapshots` for today's date.
    /// Called at startup to recover costs from a previous run on the same day.
    pub fn restore(&self, conn: &Connection) -> Result<(), String> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        let mut stmt = conn.prepare(
            "SELECT scope, entity_id, amount FROM cost_snapshots WHERE snapshot_date = ?1"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map(rusqlite::params![today], |row| {
            let scope: String = row.get(0)?;
            let entity_id: String = row.get(1)?;
            let amount: f64 = row.get(2)?;
            Ok((scope, entity_id, amount))
        }).map_err(|e| e.to_string())?;

        let mut restored = 0usize;
        for row_result in rows {
            let (scope, entity_id, amount) = row_result.map_err(|e| e.to_string())?;
            let uuid = Uuid::parse_str(&entity_id).map_err(|e| e.to_string())?;
            match scope.as_str() {
                "agent_daily" => { self.agent_daily.insert(uuid, amount); }
                "session" => { self.session_totals.insert(uuid, amount); }
                "compaction" => { self.compaction_cost.insert(uuid, amount); }
                _ => {}
            }
            restored += 1;
        }

        if restored > 0 {
            tracing::info!(entries = restored, "cost tracker state restored from DB");
        }
        Ok(())
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}
