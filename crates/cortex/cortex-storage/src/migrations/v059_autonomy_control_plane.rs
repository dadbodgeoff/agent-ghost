//! Migration v059: autonomy control-plane durable ledger.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS autonomy_jobs (
            id                     TEXT PRIMARY KEY,
            job_type               TEXT NOT NULL,
            agent_id               TEXT NOT NULL,
            tenant_key             TEXT NOT NULL DEFAULT 'local',
            workflow_id            TEXT,
            policy_scope           TEXT NOT NULL,
            payload_version        INTEGER NOT NULL CHECK(payload_version > 0),
            payload_json           TEXT NOT NULL,
            schedule_version       INTEGER NOT NULL CHECK(schedule_version > 0),
            schedule_json          TEXT NOT NULL,
            overlap_policy         TEXT NOT NULL CHECK(overlap_policy IN ('allow', 'forbid', 'replace', 'queue_one')),
            missed_run_policy      TEXT NOT NULL CHECK(missed_run_policy IN ('skip', 'catch_up_one', 'catch_up_all_with_cap', 'reschedule_from_now')),
            retry_policy_json      TEXT NOT NULL DEFAULT '{}',
            initiative_mode        TEXT NOT NULL CHECK(initiative_mode IN ('act', 'propose', 'draft', 'observe', 'suppress')),
            approval_policy        TEXT NOT NULL CHECK(approval_policy IN ('none', 'external_only', 'always')),
            state                  TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued', 'leased', 'running', 'waiting', 'succeeded', 'failed', 'paused', 'quarantined', 'aborted')),
            current_run_id         TEXT,
            next_run_at            TEXT NOT NULL,
            last_due_at            TEXT,
            last_enqueued_at       TEXT,
            last_started_at        TEXT,
            last_finished_at       TEXT,
            last_success_at        TEXT,
            last_failure_at        TEXT,
            last_heartbeat_at      TEXT,
            pause_reason           TEXT,
            quarantine_reason      TEXT,
            terminal_reason        TEXT,
            manual_review_required INTEGER NOT NULL DEFAULT 0 CHECK(manual_review_required IN (0, 1)),
            retry_count            INTEGER NOT NULL DEFAULT 0 CHECK(retry_count >= 0),
            retry_after            TEXT,
            created_at             TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at             TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS autonomy_runs (
            id                        TEXT PRIMARY KEY,
            job_id                    TEXT NOT NULL,
            attempt                   INTEGER NOT NULL CHECK(attempt >= 0),
            trigger_source            TEXT NOT NULL,
            triggered_at              TEXT NOT NULL,
            due_at                    TEXT NOT NULL,
            started_at                TEXT,
            completed_at              TEXT,
            state                     TEXT NOT NULL CHECK(state IN ('queued', 'leased', 'running', 'waiting', 'succeeded', 'failed', 'paused', 'quarantined', 'aborted')),
            why_now_json              TEXT NOT NULL,
            payload_version           INTEGER NOT NULL CHECK(payload_version > 0),
            payload_json              TEXT NOT NULL,
            initiative_mode           TEXT NOT NULL CHECK(initiative_mode IN ('act', 'propose', 'draft', 'observe', 'suppress')),
            approval_state            TEXT NOT NULL CHECK(approval_state IN ('not_required', 'pending', 'approved', 'rejected', 'expired')),
            approval_proposal_id      TEXT,
            approval_expires_at       TEXT,
            owner_identity            TEXT,
            owner_token               TEXT,
            lease_epoch               INTEGER NOT NULL DEFAULT 0 CHECK(lease_epoch >= 0),
            side_effect_correlation_key TEXT,
            side_effect_status        TEXT NOT NULL DEFAULT 'not_started' CHECK(side_effect_status IN ('not_started', 'prepared', 'applied', 'manual_review', 'failed', 'suppressed', 'aborted')),
            result_json               TEXT NOT NULL DEFAULT '{}',
            error_class               TEXT,
            error_message             TEXT,
            waiting_until             TEXT,
            terminal_reason           TEXT,
            manual_review_required    INTEGER NOT NULL DEFAULT 0 CHECK(manual_review_required IN (0, 1)),
            created_at                TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS autonomy_leases (
            job_id           TEXT PRIMARY KEY,
            run_id           TEXT NOT NULL,
            owner_identity   TEXT NOT NULL,
            owner_token      TEXT NOT NULL DEFAULT '' CHECK(length(owner_token) > 0),
            lease_epoch      INTEGER NOT NULL DEFAULT 0 CHECK(lease_epoch >= 0),
            leased_at        TEXT NOT NULL,
            last_seen_at     TEXT NOT NULL,
            lease_expires_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS autonomy_suppressions (
            id            TEXT PRIMARY KEY,
            scope_kind    TEXT NOT NULL,
            scope_key     TEXT NOT NULL,
            fingerprint   TEXT NOT NULL,
            reason        TEXT NOT NULL,
            created_by    TEXT NOT NULL,
            created_at    TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at    TEXT,
            active        INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
            policy_version INTEGER NOT NULL DEFAULT 1 CHECK(policy_version > 0),
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS autonomy_policies (
            id             TEXT PRIMARY KEY,
            scope_kind     TEXT NOT NULL,
            scope_key      TEXT NOT NULL,
            policy_version INTEGER NOT NULL CHECK(policy_version > 0),
            policy_json    TEXT NOT NULL,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(scope_kind, scope_key)
        );

        CREATE TABLE IF NOT EXISTS autonomy_notifications (
            id                  TEXT PRIMARY KEY,
            run_id              TEXT NOT NULL,
            job_id              TEXT NOT NULL,
            delivery_state      TEXT NOT NULL CHECK(delivery_state IN ('draft', 'pending_approval', 'ready', 'sent', 'manual_review', 'failed', 'suppressed', 'aborted')),
            channel             TEXT NOT NULL,
            correlation_key     TEXT NOT NULL,
            payload_json        TEXT NOT NULL,
            approval_proposal_id TEXT,
            last_error          TEXT,
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(correlation_key)
        );

        CREATE INDEX IF NOT EXISTS idx_autonomy_jobs_due_state
            ON autonomy_jobs(state, next_run_at);
        CREATE INDEX IF NOT EXISTS idx_autonomy_jobs_agent_state
            ON autonomy_jobs(agent_id, state, next_run_at);
        CREATE INDEX IF NOT EXISTS idx_autonomy_jobs_manual_review
            ON autonomy_jobs(manual_review_required, updated_at);
        CREATE INDEX IF NOT EXISTS idx_autonomy_runs_job_created
            ON autonomy_runs(job_id, created_at);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomy_runs_side_effect
            ON autonomy_runs(side_effect_correlation_key)
            WHERE side_effect_correlation_key IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_autonomy_runs_state_waiting
            ON autonomy_runs(state, waiting_until, created_at);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomy_leases_run_id
            ON autonomy_leases(run_id);
        CREATE INDEX IF NOT EXISTS idx_autonomy_leases_expiry
            ON autonomy_leases(lease_expires_at);
        CREATE INDEX IF NOT EXISTS idx_autonomy_suppressions_scope_active
            ON autonomy_suppressions(scope_kind, scope_key, active, expires_at);
        CREATE INDEX IF NOT EXISTS idx_autonomy_notifications_run_state
            ON autonomy_notifications(run_id, delivery_state, updated_at);",
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(())
}
