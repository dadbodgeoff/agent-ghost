//! Durable workflow execution rows with typed ownership and state metadata.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowExecutionRow {
    pub id: String,
    pub workflow_id: Option<String>,
    pub workflow_name: Option<String>,
    pub journal_id: Option<String>,
    pub operation_id: Option<String>,
    pub owner_token: Option<String>,
    pub lease_epoch: Option<i64>,
    pub state_version: i64,
    pub status: String,
    pub current_step_index: Option<i64>,
    pub current_node_id: Option<String>,
    pub recovery_action: Option<String>,
    pub state: String,
    pub final_response_status: Option<i64>,
    pub final_response_body: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

pub struct NewWorkflowExecutionRow<'a> {
    pub id: &'a str,
    pub workflow_id: &'a str,
    pub workflow_name: &'a str,
    pub journal_id: &'a str,
    pub operation_id: &'a str,
    pub owner_token: &'a str,
    pub lease_epoch: i64,
    pub state_version: i64,
    pub status: &'a str,
    pub current_step_index: Option<i64>,
    pub current_node_id: Option<&'a str>,
    pub recovery_action: Option<&'a str>,
    pub state: &'a str,
    pub final_response_status: Option<i64>,
    pub final_response_body: Option<&'a str>,
    pub started_at: &'a str,
    pub completed_at: Option<&'a str>,
    pub updated_at: &'a str,
}

pub struct WorkflowExecutionOwnedUpdate<'a> {
    pub workflow_id: &'a str,
    pub workflow_name: &'a str,
    pub operation_id: &'a str,
    pub status: &'a str,
    pub current_step_index: Option<i64>,
    pub current_node_id: Option<&'a str>,
    pub recovery_action: Option<&'a str>,
    pub state_version: i64,
    pub state: &'a str,
    pub final_response_status: Option<i64>,
    pub final_response_body: Option<&'a str>,
    pub completed_at: Option<&'a str>,
    pub updated_at: &'a str,
}

pub fn insert(conn: &Connection, row: &NewWorkflowExecutionRow<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO workflow_executions (
            id,
            workflow_id,
            workflow_name,
            journal_id,
            operation_id,
            owner_token,
            lease_epoch,
            state_version,
            status,
            current_step_index,
            current_node_id,
            recovery_action,
            state,
            final_response_status,
            final_response_body,
            started_at,
            completed_at,
            updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            row.id,
            row.workflow_id,
            row.workflow_name,
            row.journal_id,
            row.operation_id,
            row.owner_token,
            row.lease_epoch,
            row.state_version,
            row.status,
            row.current_step_index,
            row.current_node_id,
            row.recovery_action,
            row.state,
            row.final_response_status,
            row.final_response_body,
            row.started_at,
            row.completed_at,
            row.updated_at,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: &str) -> CortexResult<Option<WorkflowExecutionRow>> {
    conn.query_row(
        "SELECT id, workflow_id, workflow_name, journal_id, operation_id, owner_token,
                lease_epoch, state_version, status, current_step_index, current_node_id,
                recovery_action, state, final_response_status, final_response_body,
                started_at, completed_at, updated_at
         FROM workflow_executions
         WHERE id = ?1",
        params![id],
        map_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn get_by_journal_id(
    conn: &Connection,
    journal_id: &str,
) -> CortexResult<Option<WorkflowExecutionRow>> {
    conn.query_row(
        "SELECT id, workflow_id, workflow_name, journal_id, operation_id, owner_token,
                lease_epoch, state_version, status, current_step_index, current_node_id,
                recovery_action, state, final_response_status, final_response_body,
                started_at, completed_at, updated_at
         FROM workflow_executions
         WHERE journal_id = ?1",
        params![journal_id],
        map_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn list_by_workflow_id(
    conn: &Connection,
    workflow_id: &str,
    limit: usize,
) -> CortexResult<Vec<WorkflowExecutionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, workflow_id, workflow_name, journal_id, operation_id, owner_token,
                    lease_epoch, state_version, status, current_step_index, current_node_id,
                    recovery_action, state, final_response_status, final_response_body,
                    started_at, completed_at, updated_at
             FROM workflow_executions
             WHERE workflow_id = ?1
             ORDER BY updated_at DESC
             LIMIT ?2",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;

    let rows = stmt
        .query_map(params![workflow_id, limit as i64], map_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows)
}

pub fn rebind_owner(
    conn: &Connection,
    id: &str,
    workflow_id: &str,
    workflow_name: &str,
    journal_id: &str,
    operation_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    updated_at: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE workflow_executions
             SET workflow_id = ?2,
                 workflow_name = ?3,
                 journal_id = ?4,
                 operation_id = ?5,
                 owner_token = ?6,
                 lease_epoch = ?7,
                 updated_at = ?8
             WHERE id = ?1",
            params![
                id,
                workflow_id,
                workflow_name,
                journal_id,
                operation_id,
                owner_token,
                lease_epoch,
                updated_at,
            ],
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(updated > 0)
}

pub fn update_owned_state(
    conn: &Connection,
    id: &str,
    journal_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    update: &WorkflowExecutionOwnedUpdate<'_>,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE workflow_executions
             SET workflow_id = ?5,
                 workflow_name = ?6,
                 operation_id = ?7,
                 status = ?8,
                 current_step_index = ?9,
                 current_node_id = ?10,
                 recovery_action = ?11,
                 state_version = ?12,
                 state = ?13,
                 final_response_status = ?14,
                 final_response_body = ?15,
                 completed_at = ?16,
                 updated_at = ?17
             WHERE id = ?1
               AND journal_id = ?2
               AND owner_token = ?3
               AND lease_epoch = ?4",
            params![
                id,
                journal_id,
                owner_token,
                lease_epoch,
                update.workflow_id,
                update.workflow_name,
                update.operation_id,
                update.status,
                update.current_step_index,
                update.current_node_id,
                update.recovery_action,
                update.state_version,
                update.state,
                update.final_response_status,
                update.final_response_body,
                update.completed_at,
                update.updated_at,
            ],
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(updated > 0)
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowExecutionRow> {
    Ok(WorkflowExecutionRow {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        workflow_name: row.get(2)?,
        journal_id: row.get(3)?,
        operation_id: row.get(4)?,
        owner_token: row.get(5)?,
        lease_epoch: row.get(6)?,
        state_version: row.get(7)?,
        status: row.get(8)?,
        current_step_index: row.get(9)?,
        current_node_id: row.get(10)?,
        recovery_action: row.get(11)?,
        state: row.get(12)?,
        final_response_status: row.get(13)?,
        final_response_body: row.get(14)?,
        started_at: row.get(15)?,
        completed_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}
