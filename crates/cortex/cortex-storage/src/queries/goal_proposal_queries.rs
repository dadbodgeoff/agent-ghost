//! Goal proposal queries.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

const PENDING_REVIEW: &str = "pending_review";

pub struct NewProposalRecord<'a> {
    pub id: &'a str,
    pub agent_id: &'a str,
    pub session_id: &'a str,
    pub proposer_type: &'a str,
    pub operation: &'a str,
    pub target_type: &'a str,
    pub content: &'a str,
    pub cited_memory_ids: &'a str,
    pub decision: &'a str,
    pub event_hash: &'a [u8],
    pub previous_hash: &'a [u8],
    pub created_at: Option<&'a str>,
    pub operation_id: Option<&'a str>,
    pub request_id: Option<&'a str>,
}

pub struct HumanDecisionPreconditions<'a> {
    pub expected_state: &'a str,
    pub expected_lineage_id: &'a str,
    pub expected_subject_key: &'a str,
    pub expected_reviewed_revision: &'a str,
    pub rationale: Option<&'a str>,
    pub actor_id: &'a str,
    pub operation_id: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub idempotency_key: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ProposalInsertOutcome {
    pub lineage_id: String,
    pub subject_type: String,
    pub subject_key: String,
    pub reviewed_revision: String,
    pub supersedes_proposal_id: Option<String>,
    pub canonical_content: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HumanDecisionError {
    NotFound,
    StaleState {
        expected: String,
        actual: String,
    },
    StaleLineage {
        expected: String,
        actual: String,
    },
    StaleSubject {
        expected: String,
        actual: String,
    },
    StaleReviewedRevision {
        expected: String,
        actual: String,
    },
    StaleHead {
        head_proposal_id: String,
        head_state: String,
    },
    Storage(String),
}

pub fn insert_proposal(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    session_id: &str,
    proposer_type: &str,
    operation: &str,
    target_type: &str,
    content: &str,
    cited_memory_ids: &str,
    decision: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    insert_proposal_record(
        conn,
        &NewProposalRecord {
            id,
            agent_id,
            session_id,
            proposer_type,
            operation,
            target_type,
            content,
            cited_memory_ids,
            decision,
            event_hash,
            previous_hash,
            created_at: None,
            operation_id: None,
            request_id: None,
        },
    )
}

pub fn insert_proposal_record(
    conn: &Connection,
    record: &NewProposalRecord<'_>,
) -> CortexResult<()> {
    insert_proposal_record_with_outcome(conn, record).map(|_| ())
}

pub fn insert_proposal_record_with_outcome(
    conn: &Connection,
    record: &NewProposalRecord<'_>,
) -> CortexResult<ProposalInsertOutcome> {
    begin_immediate(conn)?;
    match insert_proposal_record_in_transaction(conn, record) {
        Ok(outcome) => {
            commit(conn)?;
            Ok(outcome)
        }
        Err(error) => {
            let _ = rollback(conn);
            Err(error)
        }
    }
}

fn insert_proposal_record_in_transaction(
    conn: &Connection,
    record: &NewProposalRecord<'_>,
) -> CortexResult<ProposalInsertOutcome> {
    let created_at = record
        .created_at
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let content_json = serde_json::from_str::<Value>(record.content).unwrap_or(Value::Null);
    let identity = derive_identity(
        record.operation,
        record.target_type,
        &content_json,
        record.id,
        record.session_id,
    );
    let (validation_disposition, current_state) = map_decision(record.decision);
    let current_head = load_lineage_head(conn, &identity.subject_type, &identity.subject_key)?;

    let superseded_head_id = current_head.as_ref().and_then(|head| {
        if head.head_proposal_id != record.id && head.head_state == PENDING_REVIEW {
            Some(head.head_proposal_id.clone())
        } else {
            None
        }
    });
    let actual_supersedes_proposal_id = superseded_head_id
        .clone()
        .or_else(|| identity.supersedes_proposal_id.clone());
    let canonical_content = canonicalize_content(
        content_json,
        &identity,
        actual_supersedes_proposal_id.as_deref(),
    );
    let canonical_content_json =
        serde_json::to_string(&canonical_content).map_err(|e| to_storage_err(e.to_string()))?;

    insert_legacy_projection(conn, record, &canonical_content_json, &created_at)?;

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
            ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14,
            '[]', '{}', NULL,
            ?15, ?16, ?17, ?18,
            ?19, ?20
        )",
        params![
            record.id,
            &identity.lineage_id,
            &identity.subject_type,
            &identity.subject_key,
            &identity.reviewed_revision,
            record.proposer_type,
            proposer_id(record.proposer_type),
            record.agent_id,
            record.session_id,
            record.operation,
            record.target_type,
            &canonical_content_json,
            record.cited_memory_ids,
            validation_disposition,
            actual_supersedes_proposal_id.as_deref(),
            record.operation_id,
            record.request_id,
            &created_at,
            record.event_hash,
            record.previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    if let Some(old_id) = superseded_head_id.as_deref() {
        insert_transition(
            conn,
            old_id,
            current_head
                .as_ref()
                .map(|head| head.lineage_id.as_str())
                .unwrap_or(identity.lineage_id.as_str()),
            Some(PENDING_REVIEW),
            "superseded",
            "system",
            None,
            Some("superseded_by_new_proposal"),
            None,
            None,
            None,
            None,
            None,
            None,
            &created_at,
        )?;

        conn.execute(
            "UPDATE goal_proposals
             SET decision = 'Superseded', resolved_at = ?2, resolver = 'system'
             WHERE id = ?1 AND resolved_at IS NULL",
            params![old_id, &created_at],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    }

    insert_transition(
        conn,
        record.id,
        &identity.lineage_id,
        None,
        current_state,
        "system",
        None,
        Some("proposal_created"),
        None,
        None,
        None,
        record.operation_id,
        record.request_id,
        None,
        &created_at,
    )?;

    upsert_lineage_head(
        conn,
        &identity.subject_type,
        &identity.subject_key,
        &identity.lineage_id,
        record.id,
        current_state,
        &identity.reviewed_revision,
        &created_at,
    )?;

    Ok(ProposalInsertOutcome {
        lineage_id: identity.lineage_id,
        subject_type: identity.subject_type,
        subject_key: identity.subject_key,
        reviewed_revision: identity.reviewed_revision,
        supersedes_proposal_id: actual_supersedes_proposal_id,
        canonical_content,
    })
}

/// Resolve an unresolved proposal inside an existing transaction.
pub fn resolve_proposal_in_transaction(
    conn: &Connection,
    id: &str,
    decision: &str,
    resolver: &str,
    resolved_at: &str,
) -> CortexResult<bool> {
    let proposal = load_v2_proposal(conn, id)?;
    let Some(proposal) = proposal else {
        return Ok(false);
    };

    let current_state = current_state(conn, id)?.unwrap_or_else(|| PENDING_REVIEW.to_string());
    if current_state != PENDING_REVIEW {
        return Ok(false);
    }

    let (target_state, legacy_decision, reason_code) = match decision {
        "approved" => ("approved", "approved", "human_decision"),
        "rejected" => ("rejected", "rejected", "human_decision"),
        "AutoApproved" | "ApprovedWithFlags" => {
            ("auto_applied", decision, "legacy_auto_resolution")
        }
        "AutoRejected" => ("auto_rejected", "AutoRejected", "legacy_auto_resolution"),
        _ => {
            return Err(to_storage_err(format!(
                "unsupported resolve decision: {decision}"
            )))
        }
    };

    insert_transition(
        conn,
        id,
        &proposal.lineage_id,
        Some(PENDING_REVIEW),
        target_state,
        actor_type_for_transition(target_state, resolver),
        Some(resolver),
        Some(reason_code),
        None,
        Some(PENDING_REVIEW),
        Some(&proposal.reviewed_revision),
        None,
        None,
        None,
        resolved_at,
    )?;

    upsert_lineage_head(
        conn,
        &proposal.subject_type,
        &proposal.subject_key,
        &proposal.lineage_id,
        id,
        target_state,
        &proposal.reviewed_revision,
        resolved_at,
    )?;

    conn.execute(
        "UPDATE goal_proposals
         SET decision = ?2, resolver = ?3, resolved_at = ?4
         WHERE id = ?1 AND resolved_at IS NULL",
        params![id, legacy_decision, resolver, resolved_at],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(true)
}

/// Resolve an unresolved proposal. Only succeeds if the current v2 state is pending review.
pub fn resolve_proposal(
    conn: &Connection,
    id: &str,
    decision: &str,
    resolver: &str,
    resolved_at: &str,
) -> CortexResult<bool> {
    begin_immediate(conn)?;
    match resolve_proposal_in_transaction(conn, id, decision, resolver, resolved_at) {
        Ok(updated) => {
            commit(conn)?;
            Ok(updated)
        }
        Err(error) => {
            let _ = rollback(conn);
            Err(error)
        }
    }
}

pub fn time_out_proposal_in_transaction(
    conn: &Connection,
    id: &str,
    resolved_at: &str,
) -> CortexResult<bool> {
    let proposal = load_v2_proposal(conn, id)?;
    let Some(proposal) = proposal else {
        return Ok(false);
    };

    let current_state = current_state(conn, id)?.unwrap_or_else(|| PENDING_REVIEW.to_string());
    if current_state != PENDING_REVIEW {
        return Ok(false);
    }

    insert_transition(
        conn,
        id,
        &proposal.lineage_id,
        Some(PENDING_REVIEW),
        "timed_out",
        "system",
        None,
        Some("proposal_timeout"),
        None,
        Some(PENDING_REVIEW),
        Some(&proposal.reviewed_revision),
        None,
        None,
        None,
        resolved_at,
    )?;

    upsert_lineage_head(
        conn,
        &proposal.subject_type,
        &proposal.subject_key,
        &proposal.lineage_id,
        id,
        "timed_out",
        &proposal.reviewed_revision,
        resolved_at,
    )?;

    conn.execute(
        "UPDATE goal_proposals
         SET decision = 'TimedOut', resolver = 'system', resolved_at = ?2
         WHERE id = ?1 AND resolved_at IS NULL",
        params![id, resolved_at],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(true)
}

pub fn time_out_proposal(conn: &Connection, id: &str, resolved_at: &str) -> CortexResult<bool> {
    begin_immediate(conn)?;
    match time_out_proposal_in_transaction(conn, id, resolved_at) {
        Ok(updated) => {
            commit(conn)?;
            Ok(updated)
        }
        Err(error) => {
            let _ = rollback(conn);
            Err(error)
        }
    }
}

pub fn resolve_human_decision_in_transaction(
    conn: &Connection,
    id: &str,
    decision: &str,
    preconditions: &HumanDecisionPreconditions<'_>,
    resolved_at: &str,
) -> Result<(), HumanDecisionError> {
    let proposal = load_v2_proposal(conn, id).map_err(map_decision_storage_error)?;
    let Some(proposal) = proposal else {
        return Err(HumanDecisionError::NotFound);
    };

    let current_state = current_state(conn, id)
        .map_err(map_decision_storage_error)?
        .unwrap_or_else(|| PENDING_REVIEW.to_string());

    if current_state != preconditions.expected_state {
        return Err(HumanDecisionError::StaleState {
            expected: preconditions.expected_state.to_string(),
            actual: current_state,
        });
    }

    if proposal.lineage_id != preconditions.expected_lineage_id {
        return Err(HumanDecisionError::StaleLineage {
            expected: preconditions.expected_lineage_id.to_string(),
            actual: proposal.lineage_id,
        });
    }

    if proposal.subject_key != preconditions.expected_subject_key {
        return Err(HumanDecisionError::StaleSubject {
            expected: preconditions.expected_subject_key.to_string(),
            actual: proposal.subject_key,
        });
    }

    if proposal.reviewed_revision != preconditions.expected_reviewed_revision {
        return Err(HumanDecisionError::StaleReviewedRevision {
            expected: preconditions.expected_reviewed_revision.to_string(),
            actual: proposal.reviewed_revision,
        });
    }

    let head = load_lineage_head(conn, &proposal.subject_type, &proposal.subject_key)
        .map_err(map_decision_storage_error)?;
    let Some(head) = head else {
        return Err(HumanDecisionError::StaleHead {
            head_proposal_id: String::new(),
            head_state: String::new(),
        });
    };

    if head.head_proposal_id != id || head.head_state != preconditions.expected_state {
        return Err(HumanDecisionError::StaleHead {
            head_proposal_id: head.head_proposal_id,
            head_state: head.head_state,
        });
    }

    let target_state = match decision {
        "approved" => "approved",
        "rejected" => "rejected",
        other => {
            return Err(map_decision_storage_error(to_storage_err(format!(
                "unsupported human decision: {other}"
            ))))
        }
    };

    insert_transition(
        conn,
        id,
        &proposal.lineage_id,
        Some(preconditions.expected_state),
        target_state,
        "human",
        Some(preconditions.actor_id),
        Some("human_decision"),
        preconditions.rationale,
        Some(preconditions.expected_state),
        Some(preconditions.expected_reviewed_revision),
        preconditions.operation_id,
        preconditions.request_id,
        preconditions.idempotency_key,
        resolved_at,
    )
    .map_err(map_decision_storage_error)?;

    upsert_lineage_head(
        conn,
        &proposal.subject_type,
        &proposal.subject_key,
        &proposal.lineage_id,
        id,
        target_state,
        &proposal.reviewed_revision,
        resolved_at,
    )
    .map_err(map_decision_storage_error)?;

    conn.execute(
        "UPDATE goal_proposals
         SET decision = ?2, resolver = ?3, resolved_at = ?4
         WHERE id = ?1 AND resolved_at IS NULL",
        params![id, decision, preconditions.actor_id, resolved_at],
    )
    .map_err(|error| map_decision_storage_error(to_storage_err(error.to_string())))?;

    Ok(())
}

pub fn query_pending(conn: &Connection) -> CortexResult<Vec<ProposalRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, proposer_type, operation, target_type,
                    decision, resolved_at, created_at
             FROM goal_proposals WHERE resolved_at IS NULL
             ORDER BY created_at ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map([], map_proposal_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn query_by_agent(conn: &Connection, agent_id: &str) -> CortexResult<Vec<ProposalRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, proposer_type, operation, target_type,
                    decision, resolved_at, created_at
             FROM goal_proposals WHERE agent_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id], map_proposal_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct ProposalRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub proposer_type: String,
    pub operation: String,
    pub target_type: String,
    pub decision: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ProposalV2Row {
    pub id: String,
    pub lineage_id: String,
    pub subject_type: String,
    pub subject_key: String,
    pub reviewed_revision: String,
}

#[derive(Debug, Clone)]
struct LineageHeadRow {
    lineage_id: String,
    head_proposal_id: String,
    head_state: String,
}

#[derive(Debug, Clone)]
struct DerivedIdentity {
    lineage_id: String,
    subject_type: String,
    subject_key: String,
    reviewed_revision: String,
    supersedes_proposal_id: Option<String>,
}

fn begin_immediate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| to_storage_err(e.to_string()))
}

fn commit(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch("COMMIT")
        .map_err(|e| to_storage_err(e.to_string()))
}

fn rollback(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch("ROLLBACK")
        .map_err(|e| to_storage_err(e.to_string()))
}

fn map_proposal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProposalRow> {
    Ok(ProposalRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        session_id: row.get(2)?,
        proposer_type: row.get(3)?,
        operation: row.get(4)?,
        target_type: row.get(5)?,
        decision: row.get(6)?,
        resolved_at: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn insert_legacy_projection(
    conn: &Connection,
    record: &NewProposalRecord<'_>,
    content: &str,
    created_at: &str,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO goal_proposals (
            id, agent_id, session_id, proposer_type, operation, target_type,
            content, cited_memory_ids, decision, event_hash, previous_hash,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            record.id,
            record.agent_id,
            record.session_id,
            record.proposer_type,
            record.operation,
            record.target_type,
            content,
            record.cited_memory_ids,
            record.decision,
            record.event_hash,
            record.previous_hash,
            created_at,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

fn canonicalize_content(
    mut content: Value,
    identity: &DerivedIdentity,
    supersedes_proposal_id: Option<&str>,
) -> Value {
    if let Value::Object(ref mut map) = content {
        map.entry("subject_key".to_string())
            .or_insert_with(|| Value::String(identity.subject_key.clone()));
        map.entry("lineage_id".to_string())
            .or_insert_with(|| Value::String(identity.lineage_id.clone()));
        map.entry("reviewed_revision".to_string())
            .or_insert_with(|| Value::String(identity.reviewed_revision.clone()));
        if let Some(supersedes_proposal_id) = supersedes_proposal_id {
            map.insert(
                "supersedes_proposal_id".to_string(),
                Value::String(supersedes_proposal_id.to_string()),
            );
        }
    }

    content
}

fn insert_transition(
    conn: &Connection,
    proposal_id: &str,
    lineage_id: &str,
    from_state: Option<&str>,
    to_state: &str,
    actor_type: &str,
    actor_id: Option<&str>,
    reason_code: Option<&str>,
    rationale: Option<&str>,
    expected_state: Option<&str>,
    expected_revision: Option<&str>,
    operation_id: Option<&str>,
    request_id: Option<&str>,
    idempotency_key: Option<&str>,
    created_at: &str,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO goal_proposal_transitions (
            id, proposal_id, lineage_id, from_state, to_state, actor_type,
            actor_id, reason_code, rationale, expected_state,
            expected_revision, operation_id, request_id, idempotency_key,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            uuid::Uuid::now_v7().to_string(),
            proposal_id,
            lineage_id,
            from_state,
            to_state,
            actor_type,
            actor_id,
            reason_code,
            rationale,
            expected_state,
            expected_revision,
            operation_id,
            request_id,
            idempotency_key,
            created_at,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

fn upsert_lineage_head(
    conn: &Connection,
    subject_type: &str,
    subject_key: &str,
    lineage_id: &str,
    head_proposal_id: &str,
    head_state: &str,
    current_revision: &str,
    updated_at: &str,
) -> CortexResult<()> {
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
            subject_type,
            subject_key,
            lineage_id,
            head_proposal_id,
            head_state,
            current_revision,
            updated_at,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

fn load_lineage_head(
    conn: &Connection,
    subject_type: &str,
    subject_key: &str,
) -> CortexResult<Option<LineageHeadRow>> {
    conn.query_row(
        "SELECT lineage_id, head_proposal_id, head_state
         FROM goal_lineage_heads
         WHERE subject_type = ?1 AND subject_key = ?2",
        params![subject_type, subject_key],
        |row| {
            Ok(LineageHeadRow {
                lineage_id: row.get(0)?,
                head_proposal_id: row.get(1)?,
                head_state: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

fn load_v2_proposal(conn: &Connection, id: &str) -> CortexResult<Option<ProposalV2Row>> {
    conn.query_row(
        "SELECT id, lineage_id, subject_type, subject_key, reviewed_revision
         FROM goal_proposals_v2
         WHERE id = ?1",
        params![id],
        |row| {
            Ok(ProposalV2Row {
                id: row.get(0)?,
                lineage_id: row.get(1)?,
                subject_type: row.get(2)?,
                subject_key: row.get(3)?,
                reviewed_revision: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

fn current_state(conn: &Connection, proposal_id: &str) -> CortexResult<Option<String>> {
    conn.query_row(
        "SELECT to_state
         FROM goal_proposal_transitions
         WHERE proposal_id = ?1
         ORDER BY rowid DESC
         LIMIT 1",
        params![proposal_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

fn map_decision(decision: &str) -> (&'static str, &'static str) {
    match decision {
        "AutoApproved" | "ApprovedWithFlags" => ("auto_apply", "auto_applied"),
        "AutoRejected" => ("auto_reject", "auto_rejected"),
        "approved" => ("human_review_required", "approved"),
        "rejected" => ("human_review_required", "rejected"),
        "TimedOut" => ("human_review_required", "timed_out"),
        "Superseded" => ("human_review_required", "superseded"),
        _ => ("human_review_required", PENDING_REVIEW),
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
        .filter(|value| !value.is_empty())
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

fn proposer_id(proposer_type: &str) -> Option<String> {
    proposer_type
        .split("agent_id: ")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .map(|value| value.trim().to_string())
}

fn actor_type_for_transition(target_state: &str, resolver: &str) -> &'static str {
    if matches!(
        target_state,
        "auto_applied" | "auto_rejected" | "timed_out" | "superseded"
    ) || resolver.eq_ignore_ascii_case("system")
    {
        "system"
    } else {
        "human"
    }
}

fn map_decision_storage_error(
    error: cortex_core::models::error::CortexError,
) -> HumanDecisionError {
    HumanDecisionError::Storage(error.to_string())
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
