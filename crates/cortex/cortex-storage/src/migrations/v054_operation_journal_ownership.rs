//! Migration v054: operation journal ownership CAS and fail-closed abort state.

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "ALTER TABLE operation_journal RENAME TO operation_journal_v053;

         CREATE TABLE operation_journal (
            id TEXT PRIMARY KEY,
            actor_key TEXT NOT NULL,
            method TEXT NOT NULL,
            route_template TEXT NOT NULL,
            operation_id TEXT NOT NULL,
            request_id TEXT,
            idempotency_key TEXT NOT NULL,
            request_fingerprint TEXT NOT NULL,
            request_body TEXT NOT NULL DEFAULT 'null',
            status TEXT NOT NULL CHECK(status IN ('in_progress', 'committed', 'aborted')),
            response_status_code INTEGER,
            response_body TEXT,
            response_content_type TEXT,
            created_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            committed_at TEXT,
            lease_expires_at TEXT,
            owner_token TEXT NOT NULL DEFAULT '' CHECK(length(owner_token) > 0),
            lease_epoch INTEGER NOT NULL DEFAULT 0 CHECK(lease_epoch >= 0)
        );

        INSERT INTO operation_journal (
            id,
            actor_key,
            method,
            route_template,
            operation_id,
            request_id,
            idempotency_key,
            request_fingerprint,
            request_body,
            status,
            response_status_code,
            response_body,
            response_content_type,
            created_at,
            last_seen_at,
            committed_at,
            lease_expires_at,
            owner_token,
            lease_epoch
        )
        SELECT
            id,
            actor_key,
            method,
            route_template,
            operation_id,
            request_id,
            idempotency_key,
            request_fingerprint,
            request_body,
            status,
            response_status_code,
            response_body,
            response_content_type,
            created_at,
            last_seen_at,
            committed_at,
            lease_expires_at,
            lower(hex(randomblob(16))),
            0
        FROM operation_journal_v053;

        DROP TABLE operation_journal_v053;

        CREATE UNIQUE INDEX idx_operation_journal_actor_key_idempotency
            ON operation_journal(actor_key, idempotency_key);
        CREATE UNIQUE INDEX idx_operation_journal_operation_id
            ON operation_journal(operation_id);
        CREATE INDEX idx_operation_journal_status_lease
            ON operation_journal(status, lease_expires_at);
        CREATE INDEX idx_operation_journal_fingerprint
            ON operation_journal(request_fingerprint);

        CREATE TRIGGER prevent_operation_journal_delete
        BEFORE DELETE ON operation_journal
        BEGIN
            SELECT RAISE(ABORT, 'operation_journal rows are immutable');
        END;

        CREATE TRIGGER operation_journal_commit_requires_current_request
        BEFORE UPDATE OF status ON operation_journal
        FOR EACH ROW
        WHEN NEW.status = 'committed'
             AND (
                OLD.status != 'in_progress'
                OR OLD.request_id IS NULL
                OR NEW.request_id IS NULL
                OR NEW.request_id != OLD.request_id
             )
        BEGIN
            SELECT RAISE(ABORT, 'operation_journal commit requires current request ownership');
        END;",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
