use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};
use serde_json::Value;

const HUMAN_REVIEW_REQUIRED: &str = "HumanReviewRequired";

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS goal_proposals_v2 (
            id                     TEXT PRIMARY KEY,
            lineage_id             TEXT NOT NULL,
            subject_type           TEXT NOT NULL,
            subject_key            TEXT NOT NULL,
            reviewed_revision      TEXT NOT NULL,
            proposer_type          TEXT NOT NULL,
            proposer_id            TEXT,
            agent_id               TEXT NOT NULL,
            session_id             TEXT NOT NULL,
            operation              TEXT NOT NULL,
            target_type            TEXT NOT NULL,
            content                TEXT NOT NULL,
            cited_memory_ids       TEXT NOT NULL DEFAULT '[]',
            validation_disposition TEXT NOT NULL,
            validation_flags       TEXT NOT NULL DEFAULT '[]',
            validation_scores      TEXT NOT NULL DEFAULT '{}',
            denial_reason          TEXT,
            supersedes_proposal_id TEXT,
            operation_id           TEXT,
            request_id             TEXT,
            created_at             TEXT NOT NULL,
            event_hash             BLOB NOT NULL,
            previous_hash          BLOB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_goal_proposals_v2_lineage
            ON goal_proposals_v2(lineage_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_goal_proposals_v2_subject
            ON goal_proposals_v2(subject_type, subject_key, created_at);

        CREATE TABLE IF NOT EXISTS goal_proposal_transitions (
            id                 TEXT PRIMARY KEY,
            proposal_id        TEXT NOT NULL,
            lineage_id         TEXT NOT NULL,
            from_state         TEXT,
            to_state           TEXT NOT NULL,
            actor_type         TEXT NOT NULL,
            actor_id           TEXT,
            reason_code        TEXT,
            rationale          TEXT,
            expected_state     TEXT,
            expected_revision  TEXT,
            operation_id       TEXT,
            request_id         TEXT,
            idempotency_key    TEXT,
            created_at         TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_goal_proposal_transitions_proposal
            ON goal_proposal_transitions(proposal_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_goal_proposal_transitions_lineage
            ON goal_proposal_transitions(lineage_id, created_at);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_goal_proposal_single_terminal
            ON goal_proposal_transitions(proposal_id)
            WHERE to_state IN (
                'auto_applied',
                'auto_rejected',
                'approved',
                'rejected',
                'superseded',
                'timed_out'
            );

        CREATE TABLE IF NOT EXISTS goal_lineage_heads (
            subject_type      TEXT NOT NULL,
            subject_key       TEXT NOT NULL,
            lineage_id        TEXT NOT NULL,
            head_proposal_id  TEXT NOT NULL,
            head_state        TEXT NOT NULL,
            current_revision  TEXT NOT NULL,
            updated_at        TEXT NOT NULL,
            PRIMARY KEY (subject_type, subject_key)
        );

        CREATE TRIGGER IF NOT EXISTS prevent_goal_proposals_v2_update
        BEFORE UPDATE ON goal_proposals_v2
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: goal_proposals_v2 is append-only. Updates forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_goal_proposals_v2_delete
        BEFORE DELETE ON goal_proposals_v2
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: goal_proposals_v2 is append-only. Deletes forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_goal_proposal_transitions_update
        BEFORE UPDATE ON goal_proposal_transitions
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: goal_proposal_transitions is append-only. Updates forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_goal_proposal_transitions_delete
        BEFORE DELETE ON goal_proposal_transitions
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: goal_proposal_transitions is append-only. Deletes forbidden.');
        END;
        ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    backfill_legacy_goal_proposals(conn)
}

#[derive(Debug)]
struct LegacyProposalRow {
    id: String,
    agent_id: String,
    session_id: String,
    proposer_type: String,
    operation: String,
    target_type: String,
    content: String,
    cited_memory_ids: String,
    decision: Option<String>,
    resolved_at: Option<String>,
    resolver: Option<String>,
    flags: Option<String>,
    dimension_scores: Option<String>,
    denial_reason: Option<String>,
    event_hash: Vec<u8>,
    previous_hash: Vec<u8>,
    created_at: String,
}

#[derive(Debug)]
struct DerivedIdentity {
    lineage_id: String,
    subject_type: String,
    subject_key: String,
    reviewed_revision: String,
    supersedes_proposal_id: Option<String>,
}

fn backfill_legacy_goal_proposals(conn: &Connection) -> CortexResult<()> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, proposer_type, operation, target_type,
                    content, cited_memory_ids, decision, resolved_at, resolver,
                    flags, dimension_scores, denial_reason, event_hash,
                    previous_hash, created_at
             FROM goal_proposals
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(LegacyProposalRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                session_id: row.get(2)?,
                proposer_type: row.get(3)?,
                operation: row.get(4)?,
                target_type: row.get(5)?,
                content: row.get(6)?,
                cited_memory_ids: row.get(7)?,
                decision: row.get(8)?,
                resolved_at: row.get(9)?,
                resolver: row.get(10)?,
                flags: row.get(11)?,
                dimension_scores: row.get(12)?,
                denial_reason: row.get(13)?,
                event_hash: row.get(14)?,
                previous_hash: row.get(15)?,
                created_at: row.get(16)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    for row in rows {
        let already_backfilled: Option<String> = conn
            .query_row(
                "SELECT id FROM goal_proposals_v2 WHERE id = ?1",
                [&row.id],
                |db_row| db_row.get(0),
            )
            .ok();
        if already_backfilled.is_some() {
            continue;
        }

        let content_json = serde_json::from_str::<Value>(&row.content).unwrap_or(Value::Null);
        let identity = derive_identity(
            &row.operation,
            &row.target_type,
            &content_json,
            &row.id,
            &row.session_id,
        );
        let decision = row.decision.as_deref().unwrap_or(HUMAN_REVIEW_REQUIRED);
        let (validation_disposition, state) = map_decision(decision);
        let transition_at = row.resolved_at.as_deref().unwrap_or(&row.created_at);
        let actor_type = match state {
            "approved" | "rejected" => {
                if row.resolver.is_some() {
                    "human"
                } else {
                    "legacy_backfill"
                }
            }
            _ => "system",
        };

        conn.execute(
            "INSERT INTO goal_proposals_v2 (
                id, lineage_id, subject_type, subject_key, reviewed_revision,
                proposer_type, proposer_id, agent_id, session_id, operation,
                target_type, content, cited_memory_ids, validation_disposition,
                validation_flags, validation_scores, denial_reason,
                supersedes_proposal_id, operation_id, request_id, created_at,
                event_hash, previous_hash
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, NULL, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13,
                ?14, ?15, ?16,
                ?17, NULL, NULL, ?18,
                ?19, ?20
            )",
            params![
                &row.id,
                &identity.lineage_id,
                &identity.subject_type,
                &identity.subject_key,
                &identity.reviewed_revision,
                &row.proposer_type,
                &row.agent_id,
                &row.session_id,
                &row.operation,
                &row.target_type,
                &row.content,
                &row.cited_memory_ids,
                validation_disposition,
                row.flags.as_deref().unwrap_or("[]"),
                row.dimension_scores.as_deref().unwrap_or("{}"),
                row.denial_reason.as_deref(),
                identity.supersedes_proposal_id.as_deref(),
                &row.created_at,
                &row.event_hash,
                &row.previous_hash,
            ],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "INSERT INTO goal_proposal_transitions (
                id, proposal_id, lineage_id, from_state, to_state, actor_type,
                actor_id, reason_code, rationale, expected_state,
                expected_revision, operation_id, request_id, idempotency_key,
                created_at
            ) VALUES (
                ?1, ?2, ?3, NULL, ?4, ?5,
                ?6, 'legacy_backfill', NULL, NULL,
                NULL, NULL, NULL, NULL,
                ?7
            )",
            params![
                format!("legacy-transition-{}", row.id),
                &row.id,
                &identity.lineage_id,
                state,
                actor_type,
                row.resolver.as_deref(),
                transition_at,
            ],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

        conn.execute(
            "INSERT INTO goal_lineage_heads (
                subject_type, subject_key, lineage_id, head_proposal_id,
                head_state, current_revision, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(subject_type, subject_key) DO UPDATE SET
                lineage_id = excluded.lineage_id,
                head_proposal_id = excluded.head_proposal_id,
                head_state = excluded.head_state,
                current_revision = excluded.current_revision,
                updated_at = excluded.updated_at",
            params![
                &identity.subject_type,
                &identity.subject_key,
                &identity.lineage_id,
                &row.id,
                state,
                &identity.reviewed_revision,
                transition_at,
            ],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    }

    Ok(())
}

fn map_decision(decision: &str) -> (&'static str, &'static str) {
    match decision {
        "AutoApproved" | "ApprovedWithFlags" => ("auto_apply", "auto_applied"),
        "AutoRejected" => ("auto_reject", "auto_rejected"),
        "approved" => ("human_review_required", "approved"),
        "rejected" => ("human_review_required", "rejected"),
        "TimedOut" => ("human_review_required", "timed_out"),
        "Superseded" => ("human_review_required", "superseded"),
        _ => ("human_review_required", "pending_review"),
    }
}

fn derive_identity(
    operation: &str,
    target_type: &str,
    content: &Value,
    proposal_id: &str,
    session_id: &str,
) -> DerivedIdentity {
    let subject_type = target_type.to_string();
    let explicit_subject_key = content
        .get("subject_key")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let reviewed_revision = content
        .get("reviewed_revision")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "legacy:unreviewed".to_string());

    let subject_key = explicit_subject_key.unwrap_or_else(|| {
        if operation == "GoalChange" {
            if let Some(goal_text) = content.get("goal_text").and_then(Value::as_str) {
                return format!("legacy-goal-text:{}", normalize_goal_text(goal_text));
            }
        }

        format!("legacy:{}:{}:{}", target_type, session_id, proposal_id)
    });

    let lineage_id = content
        .get("lineage_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| stable_lineage_id(&subject_type, &subject_key));

    DerivedIdentity {
        lineage_id,
        subject_type,
        subject_key,
        reviewed_revision,
        supersedes_proposal_id: content
            .get("supersedes_proposal_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    }
}

fn normalize_goal_text(goal_text: &str) -> String {
    goal_text
        .split_whitespace()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

fn stable_lineage_id(subject_type: &str, subject_key: &str) -> String {
    format!(
        "ln_{}",
        blake3::hash(format!("{subject_type}:{subject_key}").as_bytes()).to_hex()
    )
}
