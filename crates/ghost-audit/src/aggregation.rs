//! Audit aggregation for summary statistics (Req 30 AC2).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::query_engine::{AuditError, AuditResult};

fn to_err(msg: String) -> AuditError {
    AuditError::Storage(msg)
}

/// Aggregation result types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountByKey {
    pub key: String,
    pub count: u64,
}

/// Summary aggregation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    pub violations_per_day: Vec<CountByKey>,
    pub violations_by_severity: Vec<CountByKey>,
    pub policy_denials_by_tool: Vec<CountByKey>,
    pub boundary_violations_by_pattern: Vec<CountByKey>,
    pub total_entries: u64,
}

/// Audit aggregation engine.
pub struct AuditAggregation<'a> {
    conn: &'a Connection,
}

impl<'a> AuditAggregation<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Compute all aggregation summaries.
    pub fn summarize(&self, agent_id: Option<&str>) -> AuditResult<AggregationResult> {
        Ok(AggregationResult {
            violations_per_day: self.violations_per_day(agent_id)?,
            violations_by_severity: self.violations_by_severity(agent_id)?,
            policy_denials_by_tool: self.policy_denials_by_tool(agent_id)?,
            boundary_violations_by_pattern: self.boundary_violations_by_pattern(agent_id)?,
            total_entries: self.total_entries(agent_id)?,
        })
    }

    fn violations_per_day(&self, agent_id: Option<&str>) -> AuditResult<Vec<CountByKey>> {
        // event_type = 'violation' is a hardcoded literal, so agent_id
        // is always the first (and only) parameterized placeholder.
        let (where_clause, param) = agent_filter(agent_id, 1);
        let sql = format!(
            "SELECT DATE(timestamp) as day, COUNT(*) as cnt
             FROM audit_log
             WHERE event_type = 'violation' {}
             GROUP BY day ORDER BY day DESC LIMIT 30",
            where_clause
        );
        self.query_count_by_key(&sql, param.as_deref())
    }

    /// Returns violation counts grouped by severity level (F12 fix:
    /// renamed from top_violation_types — the query groups by severity,
    /// not by violation type).
    fn violations_by_severity(&self, agent_id: Option<&str>) -> AuditResult<Vec<CountByKey>> {
        let (where_clause, param) = agent_filter(agent_id, 1);
        let sql = format!(
            "SELECT severity, COUNT(*) as cnt
             FROM audit_log
             WHERE event_type = 'violation' {}
             GROUP BY severity ORDER BY cnt DESC LIMIT 20",
            where_clause
        );
        self.query_count_by_key(&sql, param.as_deref())
    }

    fn policy_denials_by_tool(&self, agent_id: Option<&str>) -> AuditResult<Vec<CountByKey>> {
        let (where_clause, param) = agent_filter(agent_id, 1);
        let sql = format!(
            "SELECT COALESCE(tool_name, 'unknown') as tool, COUNT(*) as cnt
             FROM audit_log
             WHERE event_type = 'policy_denial' {}
             GROUP BY tool ORDER BY cnt DESC LIMIT 20",
            where_clause
        );
        self.query_count_by_key(&sql, param.as_deref())
    }

    fn boundary_violations_by_pattern(
        &self,
        agent_id: Option<&str>,
    ) -> AuditResult<Vec<CountByKey>> {
        let (where_clause, param) = agent_filter(agent_id, 1);
        let sql = format!(
            "SELECT details, COUNT(*) as cnt
             FROM audit_log
             WHERE event_type = 'boundary_violation' {}
             GROUP BY details ORDER BY cnt DESC LIMIT 20",
            where_clause
        );
        self.query_count_by_key(&sql, param.as_deref())
    }

    fn total_entries(&self, agent_id: Option<&str>) -> AuditResult<u64> {
        let (where_clause, param) = agent_filter(agent_id, 1);
        let sql = format!(
            "SELECT COUNT(*) FROM audit_log WHERE 1=1 {}",
            where_clause
        );
        let count: u64 = if let Some(ref p) = param {
            self.conn
                .query_row(&sql, params![p], |row| row.get(0))
                .map_err(|e| to_err(e.to_string()))?
        } else {
            self.conn
                .query_row(&sql, [], |row| row.get(0))
                .map_err(|e| to_err(e.to_string()))?
        };
        Ok(count)
    }

    fn query_count_by_key(
        &self,
        sql: &str,
        agent_id: Option<&str>,
    ) -> AuditResult<Vec<CountByKey>> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| to_err(e.to_string()))?;

        let mapper = |row: &rusqlite::Row| -> rusqlite::Result<CountByKey> {
            Ok(CountByKey {
                key: row.get(0)?,
                count: row.get(1)?,
            })
        };

        let results: Vec<CountByKey> = if let Some(aid) = agent_id {
            stmt.query_map(params![aid], mapper)
                .map_err(|e| to_err(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| to_err(e.to_string()))?
        } else {
            stmt.query_map([], mapper)
                .map_err(|e| to_err(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| to_err(e.to_string()))?
        };

        Ok(results)
    }
}

/// Build an agent_id filter clause with explicit parameter index (F4 fix).
///
/// The `next_param_idx` argument specifies which `?N` placeholder to use,
/// making this safe to compose with other parameterized conditions.
/// Currently all callers pass `1` because the event_type filters are
/// hardcoded literals, but this is now future-proof.
fn agent_filter(agent_id: Option<&str>, next_param_idx: usize) -> (String, Option<String>) {
    match agent_id {
        Some(id) => (
            format!("AND agent_id = ?{next_param_idx}"),
            Some(id.to_string()),
        ),
        None => (String::new(), None),
    }
}
