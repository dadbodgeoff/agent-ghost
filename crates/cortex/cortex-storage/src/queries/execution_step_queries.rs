use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionStepRow {
    pub execution_id: String,
    pub attempt: i64,
    pub step_seq: i64,
    pub step_kind: String,
    pub step_fingerprint: String,
    pub tool_name: Option<String>,
    pub reliability_class: String,
    pub status: String,
    pub request_json: Option<String>,
    pub result_json: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}

pub struct NewExecutionStep<'a> {
    pub execution_id: &'a str,
    pub attempt: i64,
    pub step_seq: i64,
    pub step_kind: &'a str,
    pub step_fingerprint: &'a str,
    pub tool_name: Option<&'a str>,
    pub reliability_class: &'a str,
    pub status: &'a str,
    pub request_json: Option<&'a str>,
    pub started_at: &'a str,
}

pub fn insert_started(conn: &Connection, step: &NewExecutionStep<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO execution_steps (
            execution_id, attempt, step_seq, step_kind, step_fingerprint, tool_name,
            reliability_class, status, request_json, started_at, ended_at, result_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL)",
        params![
            step.execution_id,
            step.attempt,
            step.step_seq,
            step.step_kind,
            step.step_fingerprint,
            step.tool_name,
            step.reliability_class,
            step.status,
            step.request_json,
            step.started_at,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn update_status_and_result(
    conn: &Connection,
    execution_id: &str,
    attempt: i64,
    step_seq: i64,
    status: &str,
    result_json: Option<&str>,
    ended_at: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE execution_steps
             SET status = ?4,
                 result_json = ?5,
                 ended_at = ?6
             WHERE execution_id = ?1
               AND attempt = ?2
               AND step_seq = ?3
               AND status = 'started'",
            params![
                execution_id,
                attempt,
                step_seq,
                status,
                result_json,
                ended_at
            ],
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(updated > 0)
}

pub fn list_for_execution(
    conn: &Connection,
    execution_id: &str,
) -> CortexResult<Vec<ExecutionStepRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT execution_id, attempt, step_seq, step_kind, step_fingerprint, tool_name,
                    reliability_class, status, request_json, result_json, started_at, ended_at
             FROM execution_steps
             WHERE execution_id = ?1
             ORDER BY attempt ASC, step_seq ASC",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;

    let rows = stmt
        .query_map(params![execution_id], map_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(rows)
}

pub fn has_non_replay_safe_step(conn: &Connection, execution_id: &str) -> CortexResult<bool> {
    let count = conn
        .query_row(
            "SELECT COUNT(1)
             FROM execution_steps
             WHERE execution_id = ?1 AND reliability_class != 'replay_safe'",
            params![execution_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(count > 0)
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExecutionStepRow> {
    Ok(ExecutionStepRow {
        execution_id: row.get(0)?,
        attempt: row.get(1)?,
        step_seq: row.get(2)?,
        step_kind: row.get(3)?,
        step_fingerprint: row.get(4)?,
        tool_name: row.get(5)?,
        reliability_class: row.get(6)?,
        status: row.get(7)?,
        request_json: row.get(8)?,
        result_json: row.get(9)?,
        started_at: row.get(10)?,
        ended_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_update_does_not_clobber_committed_step() {
        let conn = crate::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE execution_steps (
                execution_id TEXT NOT NULL,
                attempt INTEGER NOT NULL,
                step_seq INTEGER NOT NULL,
                step_kind TEXT NOT NULL,
                step_fingerprint TEXT NOT NULL,
                tool_name TEXT,
                reliability_class TEXT NOT NULL,
                status TEXT NOT NULL,
                request_json TEXT,
                result_json TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                PRIMARY KEY (execution_id, attempt, step_seq)
            );",
        )
        .unwrap();

        insert_started(
            &conn,
            &NewExecutionStep {
                execution_id: "exec-1",
                attempt: 1,
                step_seq: 1,
                step_kind: "tool_call",
                step_fingerprint: "fp-1",
                tool_name: Some("write_file"),
                reliability_class: "journaled_side_effecting",
                status: "started",
                request_json: Some("{}"),
                started_at: "now",
            },
        )
        .unwrap();
        assert!(update_status_and_result(
            &conn,
            "exec-1",
            1,
            1,
            "committed",
            Some("{\"ok\":true}"),
            "later"
        )
        .unwrap());

        insert_started(
            &conn,
            &NewExecutionStep {
                execution_id: "exec-1",
                attempt: 1,
                step_seq: 1,
                step_kind: "tool_call",
                step_fingerprint: "fp-1",
                tool_name: Some("write_file"),
                reliability_class: "journaled_side_effecting",
                status: "started",
                request_json: Some("{}"),
                started_at: "later",
            },
        )
        .unwrap();

        assert!(!update_status_and_result(
            &conn,
            "exec-1",
            1,
            1,
            "failed",
            Some("{\"error\":true}"),
            "latest"
        )
        .unwrap());

        let rows = list_for_execution(&conn, "exec-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "committed");
        assert_eq!(rows[0].result_json.as_deref(), Some("{\"ok\":true}"));
    }
}
