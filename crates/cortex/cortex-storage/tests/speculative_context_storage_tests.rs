use cortex_storage::queries::context_attempt_job_queries::{
    get_job, insert_job, mark_job_failed, mark_job_running, mark_job_succeeded, select_due_jobs,
    NewContextAttemptJob,
};
use cortex_storage::queries::context_attempt_promotion_queries::{
    insert_promotion, latest_for_attempt, NewContextAttemptPromotion,
};
use cortex_storage::queries::context_attempt_queries::{
    expire_due_attempts, get_attempt, insert_attempt, list_recent_for_turn,
    list_retrievable_for_session, update_attempt_status, NewContextAttempt,
};
use cortex_storage::queries::context_attempt_validation_queries::{
    insert_validation, list_for_attempt, NewContextAttemptValidation,
};
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn
}

#[test]
fn speculative_attempt_retrieval_is_session_scoped_and_expiry_aware() {
    let conn = setup();

    insert_attempt(
        &conn,
        &NewContextAttempt {
            id: "attempt-1",
            agent_id: "agent-a",
            session_id: "session-a",
            turn_id: "turn-1",
            attempt_kind: "summary",
            content: "summary one",
            redacted_content: None,
            status: "retrievable",
            severity: 0.1,
            confidence: 0.7,
            retrieval_weight: 0.4,
            source_refs: "[\"msg-1\"]",
            source_hash: None,
            fast_gate_version: 1,
            contradicted_by_memory_id: None,
            promotion_candidate: false,
            expires_at: "2026-03-10T12:00:00Z",
        },
    )
    .unwrap();
    insert_attempt(
        &conn,
        &NewContextAttempt {
            id: "attempt-2",
            agent_id: "agent-a",
            session_id: "session-b",
            turn_id: "turn-1",
            attempt_kind: "summary",
            content: "summary two",
            redacted_content: None,
            status: "retrievable",
            severity: 0.1,
            confidence: 0.7,
            retrieval_weight: 0.4,
            source_refs: "[\"msg-2\"]",
            source_hash: None,
            fast_gate_version: 1,
            contradicted_by_memory_id: None,
            promotion_candidate: false,
            expires_at: "2026-03-10T12:00:00Z",
        },
    )
    .unwrap();
    insert_attempt(
        &conn,
        &NewContextAttempt {
            id: "attempt-3",
            agent_id: "agent-a",
            session_id: "session-a",
            turn_id: "turn-2",
            attempt_kind: "summary",
            content: "expired summary",
            redacted_content: None,
            status: "retrievable",
            severity: 0.1,
            confidence: 0.7,
            retrieval_weight: 0.4,
            source_refs: "[\"msg-3\"]",
            source_hash: None,
            fast_gate_version: 1,
            contradicted_by_memory_id: None,
            promotion_candidate: false,
            expires_at: "2026-03-10T09:00:00Z",
        },
    )
    .unwrap();

    let rows =
        list_retrievable_for_session(&conn, "session-a", "2026-03-10T10:00:00Z", 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "attempt-1");
}

#[test]
fn speculative_attempt_status_updates_and_expiration_work() {
    let conn = setup();

    insert_attempt(
        &conn,
        &NewContextAttempt {
            id: "attempt-status",
            agent_id: "agent-a",
            session_id: "session-a",
            turn_id: "turn-1",
            attempt_kind: "summary",
            content: "summary one",
            redacted_content: Some("redacted"),
            status: "pending",
            severity: 0.1,
            confidence: 0.5,
            retrieval_weight: 0.4,
            source_refs: "[\"msg-1\"]",
            source_hash: None,
            fast_gate_version: 1,
            contradicted_by_memory_id: None,
            promotion_candidate: false,
            expires_at: "2026-03-10T09:00:00Z",
        },
    )
    .unwrap();

    assert!(update_attempt_status(&conn, "attempt-status", "blocked", Some("memory-1")).unwrap());
    let row = get_attempt(&conn, "attempt-status").unwrap().unwrap();
    assert_eq!(row.status, "blocked");
    assert_eq!(row.contradicted_by_memory_id.as_deref(), Some("memory-1"));

    assert!(update_attempt_status(&conn, "attempt-status", "pending", None).unwrap());
    let expired = expire_due_attempts(&conn, "2026-03-10T10:00:00Z", 10).unwrap();
    assert_eq!(expired, 1);
    let row = get_attempt(&conn, "attempt-status").unwrap().unwrap();
    assert_eq!(row.status, "expired");
}

#[test]
fn speculative_attempt_validation_rows_are_append_only() {
    let conn = setup();

    insert_attempt(
        &conn,
        &NewContextAttempt {
            id: "attempt-audit",
            agent_id: "agent-a",
            session_id: "session-a",
            turn_id: "turn-1",
            attempt_kind: "summary",
            content: "summary one",
            redacted_content: None,
            status: "retrievable",
            severity: 0.1,
            confidence: 0.5,
            retrieval_weight: 0.4,
            source_refs: "[\"msg-1\"]",
            source_hash: None,
            fast_gate_version: 1,
            contradicted_by_memory_id: None,
            promotion_candidate: false,
            expires_at: "2026-03-10T12:00:00Z",
        },
    )
    .unwrap();

    insert_validation(
        &conn,
        &NewContextAttemptValidation {
            id: "validation-1",
            attempt_id: "attempt-audit",
            gate_name: "fast_gate",
            decision: "passed",
            reason: Some("ok"),
            score: Some(0.1),
            details_json: Some("{\"severity\":0.1}"),
        },
    )
    .unwrap();
    insert_validation(
        &conn,
        &NewContextAttemptValidation {
            id: "validation-2",
            attempt_id: "attempt-audit",
            gate_name: "severity_check",
            decision: "passed",
            reason: None,
            score: Some(0.1),
            details_json: None,
        },
    )
    .unwrap();

    let rows = list_for_attempt(&conn, "attempt-audit").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "validation-1");
    assert_eq!(rows[1].id, "validation-2");
}

#[test]
fn speculative_attempt_job_queue_supports_due_selection_and_transitions() {
    let conn = setup();

    insert_attempt(
        &conn,
        &NewContextAttempt {
            id: "attempt-job",
            agent_id: "agent-a",
            session_id: "session-a",
            turn_id: "turn-1",
            attempt_kind: "summary",
            content: "summary one",
            redacted_content: None,
            status: "retrievable",
            severity: 0.1,
            confidence: 0.5,
            retrieval_weight: 0.4,
            source_refs: "[\"msg-1\"]",
            source_hash: None,
            fast_gate_version: 1,
            contradicted_by_memory_id: None,
            promotion_candidate: false,
            expires_at: "2026-03-10T12:00:00Z",
        },
    )
    .unwrap();

    insert_job(
        &conn,
        &NewContextAttemptJob {
            id: "job-1",
            attempt_id: "attempt-job",
            job_type: "deep_validate",
            status: "pending",
            retry_count: 0,
            last_error: None,
            run_after: "2026-03-10T10:00:00Z",
        },
    )
    .unwrap();
    insert_job(
        &conn,
        &NewContextAttemptJob {
            id: "job-2",
            attempt_id: "attempt-job",
            job_type: "expire",
            status: "pending",
            retry_count: 0,
            last_error: None,
            run_after: "2026-03-11T10:00:00Z",
        },
    )
    .unwrap();

    let due = select_due_jobs(&conn, "deep_validate", "2026-03-10T10:00:01Z", 10).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "job-1");

    assert!(mark_job_running(&conn, "job-1").unwrap());
    assert!(mark_job_failed(
        &conn,
        "job-1",
        "temporary failure",
        1,
        "failed",
        "2026-03-10T10:05:00Z"
    )
    .unwrap());
    let failed = get_job(&conn, "job-1").unwrap().unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.retry_count, 1);

    assert!(mark_job_succeeded(&conn, "job-1").unwrap());
    let succeeded = get_job(&conn, "job-1").unwrap().unwrap();
    assert_eq!(succeeded.status, "succeeded");
}

#[test]
fn speculative_attempt_duplicate_lookup_is_turn_scoped() {
    let conn = setup();

    for idx in 0..3 {
        let id = format!("attempt-{idx}");
        insert_attempt(
            &conn,
            &NewContextAttempt {
                id: &id,
                agent_id: "agent-a",
                session_id: "session-a",
                turn_id: "turn-1",
                attempt_kind: "summary",
                content: "summary one",
                redacted_content: None,
                status: "pending",
                severity: 0.1,
                confidence: 0.5,
                retrieval_weight: 0.4,
                source_refs: "[\"msg-1\"]",
                source_hash: None,
                fast_gate_version: 1,
                contradicted_by_memory_id: None,
                promotion_candidate: false,
                expires_at: "2026-03-10T12:00:00Z",
            },
        )
        .unwrap();
    }

    let rows = list_recent_for_turn(&conn, "session-a", "turn-1", "summary", 2).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.turn_id == "turn-1"));
}

#[test]
fn speculative_attempt_promotion_linkage_round_trips() {
    let conn = setup();

    insert_attempt(
        &conn,
        &NewContextAttempt {
            id: "attempt-promote",
            agent_id: "agent-a",
            session_id: "session-a",
            turn_id: "turn-1",
            attempt_kind: "fact_candidate",
            content: "durable fact",
            redacted_content: None,
            status: "promoted",
            severity: 0.1,
            confidence: 0.9,
            retrieval_weight: 0.4,
            source_refs: "[\"msg-1\"]",
            source_hash: None,
            fast_gate_version: 1,
            contradicted_by_memory_id: None,
            promotion_candidate: true,
            expires_at: "2026-03-10T12:00:00Z",
        },
    )
    .unwrap();

    insert_promotion(
        &conn,
        &NewContextAttemptPromotion {
            id: "promotion-1",
            attempt_id: "attempt-promote",
            promoted_memory_id: "memory-1",
            promotion_type: "semantic_fact",
        },
    )
    .unwrap();

    let row = latest_for_attempt(&conn, "attempt-promote")
        .unwrap()
        .unwrap();
    assert_eq!(row.promoted_memory_id, "memory-1");
    assert_eq!(row.promotion_type, "semantic_fact");
}
