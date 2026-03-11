//! Cost tracker: per-agent daily totals, per-session totals (Req 27 AC1).
//!
//! WP4-A: Persist/restore cost state across restarts via `cost_snapshots` table.

use dashmap::DashMap;
use rusqlite::{params, Connection};
use std::sync::Mutex;
use uuid::Uuid;

fn current_utc_day() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

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
    /// The UTC day represented by the in-memory daily and compaction maps.
    active_day_utc: Mutex<String>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            agent_daily: DashMap::new(),
            session_totals: DashMap::new(),
            compaction_cost: DashMap::new(),
            active_day_utc: Mutex::new(current_utc_day()),
        }
    }

    fn ensure_current_day(&self) {
        let today = current_utc_day();
        let mut active_day = self
            .active_day_utc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *active_day == today {
            return;
        }

        self.agent_daily.clear();
        self.compaction_cost.clear();
        *active_day = today;
    }

    /// Record a cost for an agent in a session.
    pub fn record(&self, agent_id: Uuid, session_id: Uuid, cost: f64, is_compaction: bool) {
        self.ensure_current_day();
        *self.agent_daily.entry(agent_id).or_insert(0.0) += cost;
        *self.session_totals.entry(session_id).or_insert(0.0) += cost;
        if is_compaction {
            *self.compaction_cost.entry(agent_id).or_insert(0.0) += cost;
        }
    }

    /// Get daily total for an agent.
    pub fn get_daily_total(&self, agent_id: Uuid) -> f64 {
        self.ensure_current_day();
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
        self.ensure_current_day();
        self.compaction_cost
            .get(&agent_id)
            .map(|v| *v)
            .unwrap_or(0.0)
    }

    /// Reset daily totals (called at midnight).
    pub fn reset_daily(&self) {
        self.agent_daily.clear();
        self.compaction_cost.clear();
        let mut active_day = self
            .active_day_utc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active_day = current_utc_day();
    }

    /// Snapshot all cost data into plain Vecs (releases DashMap shard locks immediately).
    /// Call this *before* acquiring the DB writer lock.
    pub fn snapshot(&self) -> CostSnapshot {
        self.ensure_current_day();
        CostSnapshot {
            daily: self
                .agent_daily
                .iter()
                .map(|e| (e.key().to_string(), *e.value()))
                .collect(),
            sessions: self
                .session_totals
                .iter()
                .map(|e| (e.key().to_string(), *e.value()))
                .collect(),
            compaction: self
                .compaction_cost
                .iter()
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

        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;

        let upsert_sql =
            "INSERT INTO cost_snapshots (scope, entity_id, amount, snapshot_date, updated_at)
                          VALUES (?1, ?2, ?3, ?4, ?5)
                          ON CONFLICT(scope, entity_id, snapshot_date)
                          DO UPDATE SET amount = ?3, updated_at = ?5";

        let mut stmt = conn.prepare(upsert_sql).map_err(|e| {
            let _ = conn.execute_batch("ROLLBACK");
            e.to_string()
        })?;

        for (id, amount) in &snapshot.daily {
            stmt.execute(rusqlite::params!["agent_daily", id, amount, today, now])
                .map_err(|e| {
                    let _ = conn.execute_batch("ROLLBACK");
                    e.to_string()
                })?;
        }
        for (id, amount) in &snapshot.sessions {
            stmt.execute(rusqlite::params!["session", id, amount, today, now])
                .map_err(|e| {
                    let _ = conn.execute_batch("ROLLBACK");
                    e.to_string()
                })?;
        }
        for (id, amount) in &snapshot.compaction {
            stmt.execute(rusqlite::params!["compaction", id, amount, today, now])
                .map_err(|e| {
                    let _ = conn.execute_batch("ROLLBACK");
                    e.to_string()
                })?;
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

    /// Remove stale daily snapshots and superseded session snapshots.
    pub fn prune_stale_snapshots(conn: &Connection, retain_date: &str) -> Result<usize, String> {
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;

        let deleted_daily = conn
            .execute(
                "DELETE FROM cost_snapshots
                 WHERE scope IN ('agent_daily', 'compaction')
                   AND snapshot_date <> ?1",
                params![retain_date],
            )
            .map_err(|e| {
                let _ = conn.execute_batch("ROLLBACK");
                e.to_string()
            })?;

        let deleted_sessions = conn
            .execute(
                "DELETE FROM cost_snapshots
                 WHERE scope = 'session'
                   AND snapshot_date < (
                       SELECT MAX(latest.snapshot_date)
                       FROM cost_snapshots AS latest
                       WHERE latest.scope = 'session'
                         AND latest.entity_id = cost_snapshots.entity_id
                   )",
                [],
            )
            .map_err(|e| {
                let _ = conn.execute_batch("ROLLBACK");
                e.to_string()
            })?;

        conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
        Ok(deleted_daily + deleted_sessions)
    }

    /// WP4-A: Restore cost state from `cost_snapshots`.
    /// Daily and compaction totals restore only for the current UTC day.
    /// Session totals restore from the latest snapshot for each session.
    pub fn restore(&self, conn: &Connection) -> Result<(), String> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        let mut stmt = conn
            .prepare(
                "SELECT scope, entity_id, amount
                 FROM cost_snapshots
                 WHERE (scope IN ('agent_daily', 'compaction') AND snapshot_date = ?1)
                    OR (
                        scope = 'session'
                        AND snapshot_date = (
                            SELECT MAX(latest.snapshot_date)
                            FROM cost_snapshots AS latest
                            WHERE latest.scope = 'session'
                              AND latest.entity_id = cost_snapshots.entity_id
                        )
                    )",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(rusqlite::params![today], |row| {
                let scope: String = row.get(0)?;
                let entity_id: String = row.get(1)?;
                let amount: f64 = row.get(2)?;
                Ok((scope, entity_id, amount))
            })
            .map_err(|e| e.to_string())?;

        let mut restored = 0usize;
        for row_result in rows {
            let (scope, entity_id, amount) = row_result.map_err(|e| e.to_string())?;
            let uuid = Uuid::parse_str(&entity_id).map_err(|e| e.to_string())?;
            match scope.as_str() {
                "agent_daily" => {
                    self.agent_daily.insert(uuid, amount);
                }
                "session" => {
                    self.session_totals.insert(uuid, amount);
                }
                "compaction" => {
                    self.compaction_cost.insert(uuid, amount);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_rotates_daily_totals_when_tracker_day_is_stale() {
        let tracker = CostTracker::new();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        tracker.agent_daily.insert(agent_id, 9.5);
        tracker.compaction_cost.insert(agent_id, 1.25);
        *tracker
            .active_day_utc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = "2000-01-01".to_string();

        tracker.record(agent_id, session_id, 2.0, false);

        assert!((tracker.get_daily_total(agent_id) - 2.0).abs() < f64::EPSILON);
        assert!(tracker.get_compaction_cost(agent_id).abs() < f64::EPSILON);
        assert!((tracker.get_session_total(session_id) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn prune_stale_snapshots_keeps_only_requested_day() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(
            "CREATE TABLE cost_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scope TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                amount REAL NOT NULL DEFAULT 0.0,
                snapshot_date TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(scope, entity_id, snapshot_date)
            );",
        )
        .expect("create cost_snapshots");
        conn.execute_batch(
            "INSERT INTO cost_snapshots (scope, entity_id, amount, snapshot_date, updated_at)
             VALUES
                ('agent_daily', 'a', 1.0, '2026-03-09', '2026-03-09T00:00:00Z'),
                ('agent_daily', 'b', 2.0, '2026-03-10', '2026-03-10T00:00:00Z'),
                ('session', 'session-1', 3.0, '2026-03-09', '2026-03-09T00:00:00Z'),
                ('session', 'session-1', 4.0, '2026-03-10', '2026-03-10T00:00:00Z');",
        )
        .expect("insert snapshot rows");

        let deleted =
            CostTracker::prune_stale_snapshots(&conn, "2026-03-10").expect("prune snapshots");
        let remaining: Vec<(String, String, f64)> = conn
            .prepare(
                "SELECT scope, entity_id, amount
                 FROM cost_snapshots
                 ORDER BY scope, entity_id, snapshot_date",
            )
            .expect("prepare query")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("query rows")
            .collect::<Result<_, _>>()
            .expect("count rows");

        assert_eq!(deleted, 2);
        assert_eq!(
            remaining,
            vec![
                ("agent_daily".to_string(), "b".to_string(), 2.0),
                ("session".to_string(), "session-1".to_string(), 4.0),
            ]
        );
    }

    #[test]
    fn restore_loads_latest_session_snapshot_even_from_prior_day() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(
            "CREATE TABLE cost_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scope TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                amount REAL NOT NULL DEFAULT 0.0,
                snapshot_date TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(scope, entity_id, snapshot_date)
            );",
        )
        .expect("create cost_snapshots");

        let today = current_utc_day();
        let session_id = Uuid::now_v7();
        let agent_id = Uuid::now_v7();
        conn.execute_batch(&format!(
            "INSERT INTO cost_snapshots (scope, entity_id, amount, snapshot_date, updated_at)
             VALUES
                ('session', '{session_id}', 7.5, '2000-01-01', '2000-01-01T00:00:00Z'),
                ('agent_daily', '{agent_id}', 2.0, '{today}', '{today}T00:00:00Z');"
        ))
        .expect("insert restore snapshots");

        let tracker = CostTracker::new();
        tracker.restore(&conn).expect("restore tracker");

        assert!((tracker.get_session_total(session_id) - 7.5).abs() < f64::EPSILON);
        assert!((tracker.get_daily_total(agent_id) - 2.0).abs() < f64::EPSILON);
    }
}
