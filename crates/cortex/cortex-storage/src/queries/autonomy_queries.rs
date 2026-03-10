use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

pub const AUTONOMY_STATES: [&str; 9] = [
    "queued",
    "leased",
    "running",
    "waiting",
    "succeeded",
    "failed",
    "paused",
    "quarantined",
    "aborted",
];

pub fn valid_run_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("queued", "leased")
            | ("queued", "paused")
            | ("queued", "quarantined")
            | ("queued", "aborted")
            | ("leased", "running")
            | ("leased", "waiting")
            | ("leased", "succeeded")
            | ("leased", "failed")
            | ("leased", "paused")
            | ("leased", "quarantined")
            | ("leased", "aborted")
            | ("running", "waiting")
            | ("running", "succeeded")
            | ("running", "failed")
            | ("running", "paused")
            | ("running", "quarantined")
            | ("running", "aborted")
            | ("waiting", "queued")
            | ("waiting", "paused")
            | ("waiting", "quarantined")
            | ("waiting", "aborted")
            | ("failed", "waiting")
            | ("failed", "aborted")
            | ("failed", "paused")
            | ("failed", "quarantined")
            | ("paused", "queued")
            | ("quarantined", "aborted")
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyJobRow {
    pub id: String,
    pub job_type: String,
    pub agent_id: String,
    pub tenant_key: String,
    pub workflow_id: Option<String>,
    pub policy_scope: String,
    pub payload_version: i64,
    pub payload_json: String,
    pub schedule_version: i64,
    pub schedule_json: String,
    pub overlap_policy: String,
    pub missed_run_policy: String,
    pub retry_policy_json: String,
    pub initiative_mode: String,
    pub approval_policy: String,
    pub state: String,
    pub current_run_id: Option<String>,
    pub next_run_at: String,
    pub last_due_at: Option<String>,
    pub last_enqueued_at: Option<String>,
    pub last_started_at: Option<String>,
    pub last_finished_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub pause_reason: Option<String>,
    pub quarantine_reason: Option<String>,
    pub terminal_reason: Option<String>,
    pub manual_review_required: bool,
    pub retry_count: i64,
    pub retry_after: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewAutonomyJob<'a> {
    pub id: &'a str,
    pub job_type: &'a str,
    pub agent_id: &'a str,
    pub tenant_key: &'a str,
    pub workflow_id: Option<&'a str>,
    pub policy_scope: &'a str,
    pub payload_version: i64,
    pub payload_json: &'a str,
    pub schedule_version: i64,
    pub schedule_json: &'a str,
    pub overlap_policy: &'a str,
    pub missed_run_policy: &'a str,
    pub retry_policy_json: &'a str,
    pub initiative_mode: &'a str,
    pub approval_policy: &'a str,
    pub state: &'a str,
    pub next_run_at: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyRunRow {
    pub id: String,
    pub job_id: String,
    pub attempt: i64,
    pub trigger_source: String,
    pub triggered_at: String,
    pub due_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub state: String,
    pub why_now_json: String,
    pub payload_version: i64,
    pub payload_json: String,
    pub initiative_mode: String,
    pub approval_state: String,
    pub approval_proposal_id: Option<String>,
    pub approval_expires_at: Option<String>,
    pub owner_identity: Option<String>,
    pub owner_token: Option<String>,
    pub lease_epoch: i64,
    pub side_effect_correlation_key: Option<String>,
    pub side_effect_status: String,
    pub result_json: String,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
    pub waiting_until: Option<String>,
    pub terminal_reason: Option<String>,
    pub manual_review_required: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewAutonomyRun<'a> {
    pub id: &'a str,
    pub job_id: &'a str,
    pub attempt: i64,
    pub trigger_source: &'a str,
    pub triggered_at: &'a str,
    pub due_at: &'a str,
    pub state: &'a str,
    pub why_now_json: &'a str,
    pub payload_version: i64,
    pub payload_json: &'a str,
    pub initiative_mode: &'a str,
    pub approval_state: &'a str,
    pub approval_proposal_id: Option<&'a str>,
    pub approval_expires_at: Option<&'a str>,
    pub owner_identity: Option<&'a str>,
    pub owner_token: Option<&'a str>,
    pub lease_epoch: i64,
    pub side_effect_correlation_key: Option<&'a str>,
    pub side_effect_status: &'a str,
    pub result_json: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyLeaseRow {
    pub job_id: String,
    pub run_id: String,
    pub owner_identity: String,
    pub owner_token: String,
    pub lease_epoch: i64,
    pub leased_at: String,
    pub last_seen_at: String,
    pub lease_expires_at: String,
}

#[derive(Debug, Clone)]
pub struct AcquiredAutonomyLease {
    pub owner_identity: String,
    pub owner_token: String,
    pub lease_epoch: i64,
}

#[derive(Debug, Clone)]
pub struct AutonomyRunFinish<'a> {
    pub next_state: &'a str,
    pub side_effect_status: &'a str,
    pub result_json: &'a str,
    pub error_class: Option<&'a str>,
    pub error_message: Option<&'a str>,
    pub terminal_reason: Option<&'a str>,
    pub manual_review_required: bool,
    pub completed_at: &'a str,
    pub updated_at: &'a str,
}

pub fn mark_run_running(
    conn: &Connection,
    job_id: &str,
    run_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    started_at: &str,
) -> CortexResult<bool> {
    let current = get_run(conn, run_id)?;
    let Some(current) = current else {
        return Ok(false);
    };
    if !valid_run_transition(&current.state, "running") {
        return Err(to_storage_err(format!(
            "invalid autonomy run transition {} -> running",
            current.state
        )));
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| to_storage_err(error.to_string()))?;

    let run_updated = conn
        .execute(
            "UPDATE autonomy_runs
             SET state = 'running',
                 started_at = ?5,
                 updated_at = ?5
             WHERE id = ?1
               AND job_id = ?2
               AND owner_token = ?3
               AND lease_epoch = ?4",
            params![run_id, job_id, owner_token, lease_epoch, started_at],
        )
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if run_updated == 0 {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    }

    conn.execute(
        "UPDATE autonomy_jobs
         SET state = 'running',
             last_started_at = ?2,
             updated_at = ?2
         WHERE id = ?1",
        params![job_id, started_at],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute_batch("COMMIT")
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct AutonomyJobReschedule<'a> {
    pub run_state: &'a str,
    pub job_state: &'a str,
    pub next_run_at: &'a str,
    pub waiting_until: Option<&'a str>,
    pub side_effect_status: &'a str,
    pub result_json: &'a str,
    pub error_class: Option<&'a str>,
    pub error_message: Option<&'a str>,
    pub updated_at: &'a str,
}

#[derive(Debug, Clone)]
pub struct AutonomyRunDeferral<'a> {
    pub run_state: &'a str,
    pub job_state: &'a str,
    pub next_run_at: &'a str,
    pub waiting_until: Option<&'a str>,
    pub side_effect_status: &'a str,
    pub result_json: &'a str,
    pub error_class: Option<&'a str>,
    pub error_message: Option<&'a str>,
    pub approval_state: Option<&'a str>,
    pub approval_expires_at: Option<&'a str>,
    pub updated_at: &'a str,
}

#[derive(Debug, Clone)]
pub struct AutonomyRunCompletionFollowup<'a> {
    pub finish: AutonomyRunFinish<'a>,
    pub next_job_state: &'a str,
    pub next_run_at: &'a str,
    pub retry_after: Option<&'a str>,
    pub last_heartbeat_at: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomySuppressionRow {
    pub id: String,
    pub scope_kind: String,
    pub scope_key: String,
    pub fingerprint: String,
    pub reason: String,
    pub created_by: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub active: bool,
    pub policy_version: i64,
    pub metadata_json: String,
}

#[derive(Debug, Clone)]
pub struct NewAutonomySuppression<'a> {
    pub id: &'a str,
    pub scope_kind: &'a str,
    pub scope_key: &'a str,
    pub fingerprint: &'a str,
    pub reason: &'a str,
    pub created_by: &'a str,
    pub created_at: &'a str,
    pub expires_at: Option<&'a str>,
    pub active: bool,
    pub policy_version: i64,
    pub metadata_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyPolicyRow {
    pub id: String,
    pub scope_kind: String,
    pub scope_key: String,
    pub policy_version: i64,
    pub policy_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct UpsertAutonomyPolicy<'a> {
    pub id: &'a str,
    pub scope_kind: &'a str,
    pub scope_key: &'a str,
    pub policy_version: i64,
    pub policy_json: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyNotificationRow {
    pub id: String,
    pub run_id: String,
    pub job_id: String,
    pub delivery_state: String,
    pub channel: String,
    pub correlation_key: String,
    pub payload_json: String,
    pub approval_proposal_id: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewAutonomyNotification<'a> {
    pub id: &'a str,
    pub run_id: &'a str,
    pub job_id: &'a str,
    pub delivery_state: &'a str,
    pub channel: &'a str,
    pub correlation_key: &'a str,
    pub payload_json: &'a str,
    pub approval_proposal_id: Option<&'a str>,
    pub last_error: Option<&'a str>,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

fn map_job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomyJobRow> {
    Ok(AutonomyJobRow {
        id: row.get(0)?,
        job_type: row.get(1)?,
        agent_id: row.get(2)?,
        tenant_key: row.get(3)?,
        workflow_id: row.get(4)?,
        policy_scope: row.get(5)?,
        payload_version: row.get(6)?,
        payload_json: row.get(7)?,
        schedule_version: row.get(8)?,
        schedule_json: row.get(9)?,
        overlap_policy: row.get(10)?,
        missed_run_policy: row.get(11)?,
        retry_policy_json: row.get(12)?,
        initiative_mode: row.get(13)?,
        approval_policy: row.get(14)?,
        state: row.get(15)?,
        current_run_id: row.get(16)?,
        next_run_at: row.get(17)?,
        last_due_at: row.get(18)?,
        last_enqueued_at: row.get(19)?,
        last_started_at: row.get(20)?,
        last_finished_at: row.get(21)?,
        last_success_at: row.get(22)?,
        last_failure_at: row.get(23)?,
        last_heartbeat_at: row.get(24)?,
        pause_reason: row.get(25)?,
        quarantine_reason: row.get(26)?,
        terminal_reason: row.get(27)?,
        manual_review_required: row.get::<_, i64>(28)? != 0,
        retry_count: row.get(29)?,
        retry_after: row.get(30)?,
        created_at: row.get(31)?,
        updated_at: row.get(32)?,
    })
}

fn map_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomyRunRow> {
    Ok(AutonomyRunRow {
        id: row.get(0)?,
        job_id: row.get(1)?,
        attempt: row.get(2)?,
        trigger_source: row.get(3)?,
        triggered_at: row.get(4)?,
        due_at: row.get(5)?,
        started_at: row.get(6)?,
        completed_at: row.get(7)?,
        state: row.get(8)?,
        why_now_json: row.get(9)?,
        payload_version: row.get(10)?,
        payload_json: row.get(11)?,
        initiative_mode: row.get(12)?,
        approval_state: row.get(13)?,
        approval_proposal_id: row.get(14)?,
        approval_expires_at: row.get(15)?,
        owner_identity: row.get(16)?,
        owner_token: row.get(17)?,
        lease_epoch: row.get(18)?,
        side_effect_correlation_key: row.get(19)?,
        side_effect_status: row.get(20)?,
        result_json: row.get(21)?,
        error_class: row.get(22)?,
        error_message: row.get(23)?,
        waiting_until: row.get(24)?,
        terminal_reason: row.get(25)?,
        manual_review_required: row.get::<_, i64>(26)? != 0,
        created_at: row.get(27)?,
        updated_at: row.get(28)?,
    })
}

fn map_lease_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomyLeaseRow> {
    Ok(AutonomyLeaseRow {
        job_id: row.get(0)?,
        run_id: row.get(1)?,
        owner_identity: row.get(2)?,
        owner_token: row.get(3)?,
        lease_epoch: row.get(4)?,
        leased_at: row.get(5)?,
        last_seen_at: row.get(6)?,
        lease_expires_at: row.get(7)?,
    })
}

fn map_suppression_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomySuppressionRow> {
    Ok(AutonomySuppressionRow {
        id: row.get(0)?,
        scope_kind: row.get(1)?,
        scope_key: row.get(2)?,
        fingerprint: row.get(3)?,
        reason: row.get(4)?,
        created_by: row.get(5)?,
        created_at: row.get(6)?,
        expires_at: row.get(7)?,
        active: row.get::<_, i64>(8)? != 0,
        policy_version: row.get(9)?,
        metadata_json: row.get(10)?,
    })
}

fn map_policy_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomyPolicyRow> {
    Ok(AutonomyPolicyRow {
        id: row.get(0)?,
        scope_kind: row.get(1)?,
        scope_key: row.get(2)?,
        policy_version: row.get(3)?,
        policy_json: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_notification_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomyNotificationRow> {
    Ok(AutonomyNotificationRow {
        id: row.get(0)?,
        run_id: row.get(1)?,
        job_id: row.get(2)?,
        delivery_state: row.get(3)?,
        channel: row.get(4)?,
        correlation_key: row.get(5)?,
        payload_json: row.get(6)?,
        approval_proposal_id: row.get(7)?,
        last_error: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub fn insert_job(conn: &Connection, job: &NewAutonomyJob<'_>) -> CortexResult<()> {
    if job.payload_version <= 0 {
        return Err(to_storage_err(
            "autonomy job payload_version must be greater than 0".into(),
        ));
    }
    if job.schedule_version <= 0 {
        return Err(to_storage_err(
            "autonomy job schedule_version must be greater than 0".into(),
        ));
    }
    conn.execute(
        "INSERT INTO autonomy_jobs (
            id, job_type, agent_id, tenant_key, workflow_id, policy_scope,
            payload_version, payload_json, schedule_version, schedule_json,
            overlap_policy, missed_run_policy, retry_policy_json, initiative_mode,
            approval_policy, state, next_run_at, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19
         )",
        params![
            job.id,
            job.job_type,
            job.agent_id,
            job.tenant_key,
            job.workflow_id,
            job.policy_scope,
            job.payload_version,
            job.payload_json,
            job.schedule_version,
            job.schedule_json,
            job.overlap_policy,
            job.missed_run_policy,
            job.retry_policy_json,
            job.initiative_mode,
            job.approval_policy,
            job.state,
            job.next_run_at,
            job.created_at,
            job.updated_at,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn get_job(conn: &Connection, id: &str) -> CortexResult<Option<AutonomyJobRow>> {
    conn.query_row(
        "SELECT id, job_type, agent_id, tenant_key, workflow_id, policy_scope,
                payload_version, payload_json, schedule_version, schedule_json,
                overlap_policy, missed_run_policy, retry_policy_json, initiative_mode,
                approval_policy, state, current_run_id, next_run_at, last_due_at,
                last_enqueued_at, last_started_at, last_finished_at, last_success_at,
                last_failure_at, last_heartbeat_at, pause_reason, quarantine_reason,
                terminal_reason, manual_review_required, retry_count, retry_after,
                created_at, updated_at
         FROM autonomy_jobs
         WHERE id = ?1",
        params![id],
        map_job_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn list_jobs(conn: &Connection, limit: usize) -> CortexResult<Vec<AutonomyJobRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_type, agent_id, tenant_key, workflow_id, policy_scope,
                    payload_version, payload_json, schedule_version, schedule_json,
                    overlap_policy, missed_run_policy, retry_policy_json, initiative_mode,
                    approval_policy, state, current_run_id, next_run_at, last_due_at,
                    last_enqueued_at, last_started_at, last_finished_at, last_success_at,
                    last_failure_at, last_heartbeat_at, pause_reason, quarantine_reason,
                    terminal_reason, manual_review_required, retry_count, retry_after,
                    created_at, updated_at
             FROM autonomy_jobs
             ORDER BY next_run_at ASC, created_at ASC
             LIMIT ?1",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    let rows = stmt
        .query_map(params![limit as i64], map_job_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows)
}

pub fn select_due_jobs(
    conn: &Connection,
    now: &str,
    limit: usize,
) -> CortexResult<Vec<AutonomyJobRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_type, agent_id, tenant_key, workflow_id, policy_scope,
                    payload_version, payload_json, schedule_version, schedule_json,
                    overlap_policy, missed_run_policy, retry_policy_json, initiative_mode,
                    approval_policy, state, current_run_id, next_run_at, last_due_at,
                    last_enqueued_at, last_started_at, last_finished_at, last_success_at,
                    last_failure_at, last_heartbeat_at, pause_reason, quarantine_reason,
                    terminal_reason, manual_review_required, retry_count, retry_after,
                    created_at, updated_at
             FROM autonomy_jobs
             WHERE state IN ('queued', 'waiting', 'failed')
               AND manual_review_required = 0
               AND COALESCE(retry_after, next_run_at) <= ?1
             ORDER BY COALESCE(retry_after, next_run_at) ASC, created_at ASC
             LIMIT ?2",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    let rows = stmt
        .query_map(params![now, limit as i64], map_job_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows)
}

pub fn insert_run(conn: &Connection, run: &NewAutonomyRun<'_>) -> CortexResult<()> {
    if run.payload_version <= 0 {
        return Err(to_storage_err(
            "autonomy run payload_version must be greater than 0".into(),
        ));
    }
    conn.execute(
        "INSERT INTO autonomy_runs (
            id, job_id, attempt, trigger_source, triggered_at, due_at, state,
            why_now_json, payload_version, payload_json, initiative_mode,
            approval_state, approval_proposal_id, approval_expires_at,
            owner_identity, owner_token, lease_epoch, side_effect_correlation_key,
            side_effect_status, result_json, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11,
            ?12, ?13, ?14,
            ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22
         )",
        params![
            run.id,
            run.job_id,
            run.attempt,
            run.trigger_source,
            run.triggered_at,
            run.due_at,
            run.state,
            run.why_now_json,
            run.payload_version,
            run.payload_json,
            run.initiative_mode,
            run.approval_state,
            run.approval_proposal_id,
            run.approval_expires_at,
            run.owner_identity,
            run.owner_token,
            run.lease_epoch,
            run.side_effect_correlation_key,
            run.side_effect_status,
            run.result_json,
            run.created_at,
            run.updated_at,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn get_run(conn: &Connection, id: &str) -> CortexResult<Option<AutonomyRunRow>> {
    conn.query_row(
        "SELECT id, job_id, attempt, trigger_source, triggered_at, due_at, started_at,
                completed_at, state, why_now_json, payload_version, payload_json,
                initiative_mode, approval_state, approval_proposal_id, approval_expires_at,
                owner_identity, owner_token, lease_epoch, side_effect_correlation_key,
                side_effect_status, result_json, error_class, error_message, waiting_until,
                terminal_reason, manual_review_required, created_at, updated_at
         FROM autonomy_runs
         WHERE id = ?1",
        params![id],
        map_run_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn approve_run(
    conn: &Connection,
    run_id: &str,
    approval_proposal_id: &str,
    approval_expires_at: &str,
    updated_at: &str,
) -> CortexResult<bool> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| to_storage_err(error.to_string()))?;

    let run = get_run(conn, run_id).inspect_err(|_| {
        let _ = conn.execute_batch("ROLLBACK");
    })?;
    let Some(run) = run else {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    };

    if run.approval_state != "pending" && run.approval_state != "expired" {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    }

    let run_updated = conn
        .execute(
            "UPDATE autonomy_runs
             SET approval_state = 'approved',
                 approval_proposal_id = ?2,
                 approval_expires_at = ?3,
                 updated_at = ?4
             WHERE id = ?1",
            params![
                run_id,
                approval_proposal_id,
                approval_expires_at,
                updated_at
            ],
        )
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if run_updated == 0 {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    }

    conn.execute(
        "UPDATE autonomy_jobs
         SET state = 'queued',
             next_run_at = ?2,
             retry_after = NULL,
             manual_review_required = 0,
             updated_at = ?2
         WHERE id = ?1",
        params![run.job_id, updated_at],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute_batch("COMMIT")
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(true)
}

pub fn latest_run_for_job(conn: &Connection, job_id: &str) -> CortexResult<Option<AutonomyRunRow>> {
    conn.query_row(
        "SELECT id, job_id, attempt, trigger_source, triggered_at, due_at, started_at,
                completed_at, state, why_now_json, payload_version, payload_json,
                initiative_mode, approval_state, approval_proposal_id, approval_expires_at,
                owner_identity, owner_token, lease_epoch, side_effect_correlation_key,
                side_effect_status, result_json, error_class, error_message, waiting_until,
                terminal_reason, manual_review_required, created_at, updated_at
         FROM autonomy_runs
         WHERE job_id = ?1
         ORDER BY attempt DESC, created_at DESC
         LIMIT 1",
        params![job_id],
        map_run_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn list_runs_for_job(
    conn: &Connection,
    job_id: &str,
    limit: usize,
) -> CortexResult<Vec<AutonomyRunRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_id, attempt, trigger_source, triggered_at, due_at, started_at,
                    completed_at, state, why_now_json, payload_version, payload_json,
                    initiative_mode, approval_state, approval_proposal_id, approval_expires_at,
                    owner_identity, owner_token, lease_epoch, side_effect_correlation_key,
                    side_effect_status, result_json, error_class, error_message, waiting_until,
                    terminal_reason, manual_review_required, created_at, updated_at
             FROM autonomy_runs
             WHERE job_id = ?1
             ORDER BY attempt DESC, created_at DESC
             LIMIT ?2",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    let rows = stmt
        .query_map(params![job_id, limit as i64], map_run_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows)
}

pub fn get_run_by_side_effect_key(
    conn: &Connection,
    side_effect_correlation_key: &str,
) -> CortexResult<Option<AutonomyRunRow>> {
    conn.query_row(
        "SELECT id, job_id, attempt, trigger_source, triggered_at, due_at, started_at,
                completed_at, state, why_now_json, payload_version, payload_json,
                initiative_mode, approval_state, approval_proposal_id, approval_expires_at,
                owner_identity, owner_token, lease_epoch, side_effect_correlation_key,
                side_effect_status, result_json, error_class, error_message, waiting_until,
                terminal_reason, manual_review_required, created_at, updated_at
         FROM autonomy_runs
         WHERE side_effect_correlation_key = ?1",
        params![side_effect_correlation_key],
        map_run_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn rebind_run_owner(
    conn: &Connection,
    run_id: &str,
    job_id: &str,
    owner_identity: &str,
    owner_token: &str,
    lease_epoch: i64,
    updated_at: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE autonomy_runs
             SET state = 'leased',
                 owner_identity = ?3,
                 owner_token = ?4,
                 lease_epoch = ?5,
                 updated_at = ?6
             WHERE id = ?1
               AND job_id = ?2
               AND state IN ('leased', 'running', 'waiting', 'failed', 'paused')",
            params![
                run_id,
                job_id,
                owner_identity,
                owner_token,
                lease_epoch,
                updated_at,
            ],
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(updated > 0)
}

pub fn acquire_lease(
    conn: &Connection,
    job_id: &str,
    run_id: &str,
    owner_identity: &str,
    owner_token: &str,
    leased_at: &str,
    lease_expires_at: &str,
) -> CortexResult<Option<AcquiredAutonomyLease>> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| to_storage_err(error.to_string()))?;

    let existing = conn
        .query_row(
            "SELECT job_id, run_id, owner_identity, owner_token, lease_epoch, leased_at, last_seen_at, lease_expires_at
             FROM autonomy_leases
             WHERE job_id = ?1",
            params![job_id],
            map_lease_row,
        )
        .optional()
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if let Some(lease) = existing.as_ref() {
        if lease.lease_expires_at.as_str() > leased_at {
            let _ = conn.execute_batch("ROLLBACK");
            return Ok(None);
        }
    }

    let next_epoch = existing
        .as_ref()
        .map(|lease| lease.lease_epoch + 1)
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO autonomy_leases (
            job_id, run_id, owner_identity, owner_token, lease_epoch, leased_at, last_seen_at, lease_expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)
         ON CONFLICT(job_id) DO UPDATE SET
            run_id = excluded.run_id,
            owner_identity = excluded.owner_identity,
            owner_token = excluded.owner_token,
            lease_epoch = excluded.lease_epoch,
            leased_at = excluded.leased_at,
            last_seen_at = excluded.last_seen_at,
            lease_expires_at = excluded.lease_expires_at",
        params![
            job_id,
            run_id,
            owner_identity,
            owner_token,
            next_epoch,
            leased_at,
            lease_expires_at
        ],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    let updated = conn
        .execute(
            "UPDATE autonomy_jobs
             SET state = 'leased',
                 current_run_id = ?2,
                 last_due_at = COALESCE(last_due_at, next_run_at),
                 last_enqueued_at = ?3,
                 updated_at = ?3
             WHERE id = ?1
               AND state IN ('queued', 'waiting', 'failed', 'leased')",
            params![job_id, run_id, leased_at],
        )
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if updated == 0 {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(None);
    }

    conn.execute_batch("COMMIT")
        .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(Some(AcquiredAutonomyLease {
        owner_identity: owner_identity.to_string(),
        owner_token: owner_token.to_string(),
        lease_epoch: next_epoch,
    }))
}

pub fn get_lease(conn: &Connection, job_id: &str) -> CortexResult<Option<AutonomyLeaseRow>> {
    conn.query_row(
        "SELECT job_id, run_id, owner_identity, owner_token, lease_epoch, leased_at, last_seen_at, lease_expires_at
         FROM autonomy_leases
         WHERE job_id = ?1",
        params![job_id],
        map_lease_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn renew_lease(
    conn: &Connection,
    job_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    last_seen_at: &str,
    lease_expires_at: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE autonomy_leases
             SET last_seen_at = ?4,
                 lease_expires_at = ?5
             WHERE job_id = ?1
               AND owner_token = ?2
               AND lease_epoch = ?3",
            params![
                job_id,
                owner_token,
                lease_epoch,
                last_seen_at,
                lease_expires_at
            ],
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(updated > 0)
}

pub fn finish_run(
    conn: &Connection,
    job_id: &str,
    run_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    finish: &AutonomyRunFinish<'_>,
) -> CortexResult<bool> {
    let current = get_run(conn, run_id)?;
    let Some(current) = current else {
        return Ok(false);
    };
    if !valid_run_transition(&current.state, finish.next_state) {
        return Err(to_storage_err(format!(
            "invalid autonomy run transition {} -> {}",
            current.state, finish.next_state
        )));
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| to_storage_err(error.to_string()))?;

    let run_updated = conn
        .execute(
            "UPDATE autonomy_runs
             SET completed_at = ?5,
                 state = ?6,
                 side_effect_status = ?7,
                 result_json = ?8,
                 error_class = ?9,
                 error_message = ?10,
                 terminal_reason = ?11,
                 manual_review_required = ?12,
                 updated_at = ?13
             WHERE id = ?1
               AND job_id = ?2
               AND owner_token = ?3
               AND lease_epoch = ?4",
            params![
                run_id,
                job_id,
                owner_token,
                lease_epoch,
                finish.completed_at,
                finish.next_state,
                finish.side_effect_status,
                finish.result_json,
                finish.error_class,
                finish.error_message,
                finish.terminal_reason,
                if finish.manual_review_required { 1 } else { 0 },
                finish.updated_at,
            ],
        )
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if run_updated == 0 {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    }

    conn.execute(
        "DELETE FROM autonomy_leases
         WHERE job_id = ?1
           AND owner_token = ?2
           AND lease_epoch = ?3",
        params![job_id, owner_token, lease_epoch],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    let retry_count_delta = if finish.next_state == "failed" { 1 } else { 0 };
    conn.execute(
        "UPDATE autonomy_jobs
         SET state = ?2,
             current_run_id = CASE WHEN ?2 IN ('succeeded', 'failed', 'aborted', 'paused', 'quarantined') THEN NULL ELSE current_run_id END,
             last_started_at = COALESCE(last_started_at, ?3),
             last_finished_at = ?3,
             last_success_at = CASE WHEN ?2 = 'succeeded' THEN ?3 ELSE last_success_at END,
             last_failure_at = CASE WHEN ?2 = 'failed' THEN ?3 ELSE last_failure_at END,
             terminal_reason = ?4,
             manual_review_required = ?5,
             retry_count = retry_count + ?6,
             updated_at = ?7
         WHERE id = ?1",
        params![
            job_id,
            finish.next_state,
            finish.completed_at,
            finish.terminal_reason,
            if finish.manual_review_required { 1 } else { 0 },
            retry_count_delta,
            finish.updated_at,
        ],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute_batch("COMMIT")
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(true)
}

pub fn complete_run_and_requeue(
    conn: &Connection,
    job_id: &str,
    run_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    followup: &AutonomyRunCompletionFollowup<'_>,
) -> CortexResult<bool> {
    let current = get_run(conn, run_id)?;
    let Some(current) = current else {
        return Ok(false);
    };
    if !valid_run_transition(&current.state, followup.finish.next_state) {
        return Err(to_storage_err(format!(
            "invalid autonomy run transition {} -> {}",
            current.state, followup.finish.next_state
        )));
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| to_storage_err(error.to_string()))?;

    let run_updated = conn
        .execute(
            "UPDATE autonomy_runs
             SET completed_at = ?5,
                 state = ?6,
                 side_effect_status = ?7,
                 result_json = ?8,
                 error_class = ?9,
                 error_message = ?10,
                 terminal_reason = ?11,
                 manual_review_required = ?12,
                 updated_at = ?13
             WHERE id = ?1
               AND job_id = ?2
               AND owner_token = ?3
               AND lease_epoch = ?4",
            params![
                run_id,
                job_id,
                owner_token,
                lease_epoch,
                followup.finish.completed_at,
                followup.finish.next_state,
                followup.finish.side_effect_status,
                followup.finish.result_json,
                followup.finish.error_class,
                followup.finish.error_message,
                followup.finish.terminal_reason,
                if followup.finish.manual_review_required {
                    1
                } else {
                    0
                },
                followup.finish.updated_at,
            ],
        )
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if run_updated == 0 {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    }

    conn.execute(
        "DELETE FROM autonomy_leases
         WHERE job_id = ?1
           AND owner_token = ?2
           AND lease_epoch = ?3",
        params![job_id, owner_token, lease_epoch],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute(
        "UPDATE autonomy_jobs
         SET state = ?2,
             current_run_id = NULL,
             next_run_at = ?3,
             retry_after = ?4,
             last_finished_at = ?5,
             last_success_at = CASE WHEN ?6 = 'succeeded' THEN ?5 ELSE last_success_at END,
             last_failure_at = CASE WHEN ?6 = 'failed' THEN ?5 ELSE last_failure_at END,
             last_heartbeat_at = COALESCE(?7, last_heartbeat_at),
             terminal_reason = ?8,
             manual_review_required = ?9,
             updated_at = ?10
         WHERE id = ?1",
        params![
            job_id,
            followup.next_job_state,
            followup.next_run_at,
            followup.retry_after,
            followup.finish.completed_at,
            followup.finish.next_state,
            followup.last_heartbeat_at,
            followup.finish.terminal_reason,
            if followup.finish.manual_review_required {
                1
            } else {
                0
            },
            followup.finish.updated_at,
        ],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute_batch("COMMIT")
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(true)
}

pub fn reschedule_job(
    conn: &Connection,
    job_id: &str,
    run_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    reschedule: &AutonomyJobReschedule<'_>,
) -> CortexResult<bool> {
    let current = get_run(conn, run_id)?;
    let Some(current) = current else {
        return Ok(false);
    };
    if !valid_run_transition(&current.state, reschedule.run_state) {
        return Err(to_storage_err(format!(
            "invalid autonomy run transition {} -> {}",
            current.state, reschedule.run_state
        )));
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| to_storage_err(error.to_string()))?;

    let run_updated = conn
        .execute(
            "UPDATE autonomy_runs
             SET state = ?5,
                 waiting_until = ?6,
                 side_effect_status = ?7,
                 result_json = ?8,
                 error_class = ?9,
                 error_message = ?10,
                 updated_at = ?11
             WHERE id = ?1
               AND job_id = ?2
               AND owner_token = ?3
               AND lease_epoch = ?4",
            params![
                run_id,
                job_id,
                owner_token,
                lease_epoch,
                reschedule.run_state,
                reschedule.waiting_until,
                reschedule.side_effect_status,
                reschedule.result_json,
                reschedule.error_class,
                reschedule.error_message,
                reschedule.updated_at,
            ],
        )
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if run_updated == 0 {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    }

    conn.execute(
        "DELETE FROM autonomy_leases
         WHERE job_id = ?1
           AND owner_token = ?2
           AND lease_epoch = ?3",
        params![job_id, owner_token, lease_epoch],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute(
        "UPDATE autonomy_jobs
         SET state = ?2,
             current_run_id = NULL,
             next_run_at = ?3,
             retry_after = ?4,
             last_failure_at = CASE WHEN ?2 = 'failed' THEN ?5 ELSE last_failure_at END,
             retry_count = retry_count + 1,
             updated_at = ?5
         WHERE id = ?1",
        params![
            job_id,
            reschedule.job_state,
            reschedule.next_run_at,
            reschedule.waiting_until,
            reschedule.updated_at,
        ],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute_batch("COMMIT")
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(true)
}

pub fn defer_run(
    conn: &Connection,
    job_id: &str,
    run_id: &str,
    owner_token: &str,
    lease_epoch: i64,
    deferral: &AutonomyRunDeferral<'_>,
) -> CortexResult<bool> {
    let current = get_run(conn, run_id)?;
    let Some(current) = current else {
        return Ok(false);
    };
    if !valid_run_transition(&current.state, deferral.run_state) {
        return Err(to_storage_err(format!(
            "invalid autonomy run transition {} -> {}",
            current.state, deferral.run_state
        )));
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| to_storage_err(error.to_string()))?;

    let run_updated = conn
        .execute(
            "UPDATE autonomy_runs
             SET state = ?5,
                 waiting_until = ?6,
                 side_effect_status = ?7,
                 result_json = ?8,
                 error_class = ?9,
                 error_message = ?10,
                 approval_state = COALESCE(?11, approval_state),
                 approval_expires_at = COALESCE(?12, approval_expires_at),
                 updated_at = ?13
             WHERE id = ?1
               AND job_id = ?2
               AND owner_token = ?3
               AND lease_epoch = ?4",
            params![
                run_id,
                job_id,
                owner_token,
                lease_epoch,
                deferral.run_state,
                deferral.waiting_until,
                deferral.side_effect_status,
                deferral.result_json,
                deferral.error_class,
                deferral.error_message,
                deferral.approval_state,
                deferral.approval_expires_at,
                deferral.updated_at,
            ],
        )
        .map_err(|error| {
            let _ = conn.execute_batch("ROLLBACK");
            to_storage_err(error.to_string())
        })?;

    if run_updated == 0 {
        let _ = conn.execute_batch("ROLLBACK");
        return Ok(false);
    }

    conn.execute(
        "DELETE FROM autonomy_leases
         WHERE job_id = ?1
           AND owner_token = ?2
           AND lease_epoch = ?3",
        params![job_id, owner_token, lease_epoch],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute(
        "UPDATE autonomy_jobs
         SET state = ?2,
             current_run_id = NULL,
             next_run_at = ?3,
             retry_after = ?4,
             updated_at = ?5
         WHERE id = ?1",
        params![
            job_id,
            deferral.job_state,
            deferral.next_run_at,
            deferral.waiting_until,
            deferral.updated_at,
        ],
    )
    .map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        to_storage_err(error.to_string())
    })?;

    conn.execute_batch("COMMIT")
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(true)
}

pub fn insert_suppression(
    conn: &Connection,
    suppression: &NewAutonomySuppression<'_>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO autonomy_suppressions (
            id, scope_kind, scope_key, fingerprint, reason, created_by, created_at,
            expires_at, active, policy_version, metadata_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            suppression.id,
            suppression.scope_kind,
            suppression.scope_key,
            suppression.fingerprint,
            suppression.reason,
            suppression.created_by,
            suppression.created_at,
            suppression.expires_at,
            if suppression.active { 1 } else { 0 },
            suppression.policy_version,
            suppression.metadata_json,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn list_active_suppressions(
    conn: &Connection,
    scope_kind: &str,
    scope_key: &str,
    now: &str,
) -> CortexResult<Vec<AutonomySuppressionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scope_kind, scope_key, fingerprint, reason, created_by, created_at,
                    expires_at, active, policy_version, metadata_json
             FROM autonomy_suppressions
             WHERE scope_kind = ?1
               AND scope_key = ?2
               AND active = 1
               AND (expires_at IS NULL OR expires_at > ?3)
             ORDER BY created_at DESC",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    let rows = stmt
        .query_map(params![scope_kind, scope_key, now], map_suppression_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows)
}

pub fn upsert_policy(conn: &Connection, policy: &UpsertAutonomyPolicy<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO autonomy_policies (
            id, scope_kind, scope_key, policy_version, policy_json, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(scope_kind, scope_key) DO UPDATE SET
            id = excluded.id,
            policy_version = excluded.policy_version,
            policy_json = excluded.policy_json,
            updated_at = excluded.updated_at",
        params![
            policy.id,
            policy.scope_kind,
            policy.scope_key,
            policy.policy_version,
            policy.policy_json,
            policy.created_at,
            policy.updated_at,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn get_policy(
    conn: &Connection,
    scope_kind: &str,
    scope_key: &str,
) -> CortexResult<Option<AutonomyPolicyRow>> {
    conn.query_row(
        "SELECT id, scope_kind, scope_key, policy_version, policy_json, created_at, updated_at
         FROM autonomy_policies
         WHERE scope_kind = ?1 AND scope_key = ?2",
        params![scope_kind, scope_key],
        map_policy_row,
    )
    .optional()
    .map_err(|error| to_storage_err(error.to_string()))
}

pub fn insert_notification(
    conn: &Connection,
    notification: &NewAutonomyNotification<'_>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO autonomy_notifications (
            id, run_id, job_id, delivery_state, channel, correlation_key, payload_json,
            approval_proposal_id, last_error, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            notification.id,
            notification.run_id,
            notification.job_id,
            notification.delivery_state,
            notification.channel,
            notification.correlation_key,
            notification.payload_json,
            notification.approval_proposal_id,
            notification.last_error,
            notification.created_at,
            notification.updated_at,
        ],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(())
}

pub fn list_notifications_for_run(
    conn: &Connection,
    run_id: &str,
) -> CortexResult<Vec<AutonomyNotificationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, run_id, job_id, delivery_state, channel, correlation_key, payload_json,
                    approval_proposal_id, last_error, created_at, updated_at
             FROM autonomy_notifications
             WHERE run_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    let rows = stmt
        .query_map(params![run_id], map_notification_row)
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows)
}
