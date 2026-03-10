use cortex_storage::queries::autonomy_queries::{
    acquire_lease, finish_run, get_job, get_lease, insert_job, insert_run, insert_suppression,
    latest_run_for_job, list_active_suppressions, mark_run_running, reschedule_job,
    select_due_jobs, upsert_policy, valid_run_transition, AutonomyJobReschedule, AutonomyRunFinish,
    NewAutonomyJob, NewAutonomyRun, NewAutonomySuppression, UpsertAutonomyPolicy,
};
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    cortex_storage::run_all_migrations(&conn).unwrap();
    conn
}

fn seed_job(conn: &Connection, id: &str, next_run_at: &str) {
    insert_job(
        conn,
        &NewAutonomyJob {
            id,
            job_type: "workflow_trigger",
            agent_id: "agent-a",
            tenant_key: "tenant-a",
            workflow_id: Some("wf-a"),
            policy_scope: "agent:agent-a",
            payload_version: 1,
            payload_json: "{\"workflow_id\":\"wf-a\"}",
            schedule_version: 1,
            schedule_json: "{\"kind\":\"interval\",\"every_seconds\":60}",
            overlap_policy: "forbid",
            missed_run_policy: "catch_up_one",
            retry_policy_json: "{\"attempts\":3}",
            initiative_mode: "act",
            approval_policy: "none",
            state: "queued",
            next_run_at,
            created_at: "2026-03-10T00:00:00Z",
            updated_at: "2026-03-10T00:00:00Z",
        },
    )
    .unwrap();
}

#[test]
fn autonomy_job_insert_and_select_due() {
    let conn = setup();
    seed_job(&conn, "job-due", "2026-03-10T00:00:00Z");
    seed_job(&conn, "job-later", "2026-03-11T00:00:00Z");

    let due = select_due_jobs(&conn, "2026-03-10T00:05:00Z", 10).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "job-due");
}

#[test]
fn autonomy_lease_acquire_is_exclusive() {
    let conn = setup();
    seed_job(&conn, "job-lease", "2026-03-10T00:00:00Z");

    let first = acquire_lease(
        &conn,
        "job-lease",
        "run-1",
        "gateway:one",
        "owner-1",
        "2026-03-10T00:00:00Z",
        "2026-03-10T00:01:00Z",
    )
    .unwrap();
    assert!(first.is_some());

    let second = acquire_lease(
        &conn,
        "job-lease",
        "run-2",
        "gateway:two",
        "owner-2",
        "2026-03-10T00:00:30Z",
        "2026-03-10T00:01:30Z",
    )
    .unwrap();
    assert!(second.is_none());
}

#[test]
fn autonomy_lease_expiry_allows_recovery() {
    let conn = setup();
    seed_job(&conn, "job-recover", "2026-03-10T00:00:00Z");

    let first = acquire_lease(
        &conn,
        "job-recover",
        "run-1",
        "gateway:one",
        "owner-1",
        "2026-03-10T00:00:00Z",
        "2026-03-10T00:01:00Z",
    )
    .unwrap()
    .unwrap();
    assert_eq!(first.lease_epoch, 0);

    let second = acquire_lease(
        &conn,
        "job-recover",
        "run-2",
        "gateway:two",
        "owner-2",
        "2026-03-10T00:02:00Z",
        "2026-03-10T00:03:00Z",
    )
    .unwrap()
    .unwrap();
    assert_eq!(second.lease_epoch, 1);

    let lease = get_lease(&conn, "job-recover").unwrap().unwrap();
    assert_eq!(lease.run_id, "run-2");
    assert_eq!(lease.owner_identity, "gateway:two");
}

#[test]
fn autonomy_lease_owner_identity_is_persisted() {
    let conn = setup();
    seed_job(&conn, "job-owner", "2026-03-10T00:00:00Z");

    acquire_lease(
        &conn,
        "job-owner",
        "run-1",
        "gateway:owner-test",
        "owner-token",
        "2026-03-10T00:00:00Z",
        "2026-03-10T00:01:00Z",
    )
    .unwrap()
    .unwrap();

    let lease = get_lease(&conn, "job-owner").unwrap().unwrap();
    assert_eq!(lease.owner_identity, "gateway:owner-test");
    assert_eq!(lease.owner_token, "owner-token");
}

#[test]
fn autonomy_payload_schema_version_is_required() {
    let conn = setup();
    let err = insert_job(
        &conn,
        &NewAutonomyJob {
            id: "job-bad-version",
            job_type: "workflow_trigger",
            agent_id: "agent-a",
            tenant_key: "tenant-a",
            workflow_id: None,
            policy_scope: "agent:agent-a",
            payload_version: 0,
            payload_json: "{}",
            schedule_version: 1,
            schedule_json: "{}",
            overlap_policy: "forbid",
            missed_run_policy: "skip",
            retry_policy_json: "{}",
            initiative_mode: "observe",
            approval_policy: "none",
            state: "queued",
            next_run_at: "2026-03-10T00:00:00Z",
            created_at: "2026-03-10T00:00:00Z",
            updated_at: "2026-03-10T00:00:00Z",
        },
    )
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("payload_version must be greater than 0"),
        "{err}"
    );
}

#[test]
fn autonomy_run_transition_matrix_valid() {
    assert!(valid_run_transition("queued", "leased"));
    assert!(valid_run_transition("leased", "running"));
    assert!(valid_run_transition("running", "succeeded"));
    assert!(valid_run_transition("failed", "waiting"));
    assert!(valid_run_transition("waiting", "queued"));
    assert!(!valid_run_transition("queued", "succeeded"));
    assert!(!valid_run_transition("succeeded", "queued"));
}

#[test]
fn autonomy_idempotency_scope_blocks_duplicate_dispatch() {
    let conn = setup();
    seed_job(&conn, "job-idempotent", "2026-03-10T00:00:00Z");

    insert_run(
        &conn,
        &NewAutonomyRun {
            id: "run-1",
            job_id: "job-idempotent",
            attempt: 0,
            trigger_source: "schedule",
            triggered_at: "2026-03-10T00:00:00Z",
            due_at: "2026-03-10T00:00:00Z",
            state: "queued",
            why_now_json: "{\"reason\":\"due\"}",
            payload_version: 1,
            payload_json: "{\"workflow_id\":\"wf-a\"}",
            initiative_mode: "act",
            approval_state: "not_required",
            approval_proposal_id: None,
            approval_expires_at: None,
            owner_identity: Some("gateway:one"),
            owner_token: Some("owner-1"),
            lease_epoch: 0,
            side_effect_correlation_key: Some(
                "workflow-trigger:job-idempotent:2026-03-10T00:00:00Z",
            ),
            side_effect_status: "not_started",
            result_json: "{}",
            created_at: "2026-03-10T00:00:00Z",
            updated_at: "2026-03-10T00:00:00Z",
        },
    )
    .unwrap();

    let err = insert_run(
        &conn,
        &NewAutonomyRun {
            id: "run-2",
            job_id: "job-idempotent",
            attempt: 1,
            trigger_source: "retry",
            triggered_at: "2026-03-10T00:01:00Z",
            due_at: "2026-03-10T00:00:00Z",
            state: "queued",
            why_now_json: "{\"reason\":\"retry\"}",
            payload_version: 1,
            payload_json: "{\"workflow_id\":\"wf-a\"}",
            initiative_mode: "act",
            approval_state: "not_required",
            approval_proposal_id: None,
            approval_expires_at: None,
            owner_identity: Some("gateway:two"),
            owner_token: Some("owner-2"),
            lease_epoch: 1,
            side_effect_correlation_key: Some(
                "workflow-trigger:job-idempotent:2026-03-10T00:00:00Z",
            ),
            side_effect_status: "not_started",
            result_json: "{}",
            created_at: "2026-03-10T00:01:00Z",
            updated_at: "2026-03-10T00:01:00Z",
        },
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("UNIQUE constraint failed"),
        "{err}"
    );
}

#[test]
fn autonomy_finish_and_reschedule_round_trip() {
    let conn = setup();
    seed_job(&conn, "job-round-trip", "2026-03-10T00:00:00Z");
    let lease = acquire_lease(
        &conn,
        "job-round-trip",
        "run-round-trip",
        "gateway:test",
        "owner-round-trip",
        "2026-03-10T00:00:00Z",
        "2026-03-10T00:01:00Z",
    )
    .unwrap()
    .unwrap();

    insert_run(
        &conn,
        &NewAutonomyRun {
            id: "run-round-trip",
            job_id: "job-round-trip",
            attempt: 0,
            trigger_source: "schedule",
            triggered_at: "2026-03-10T00:00:00Z",
            due_at: "2026-03-10T00:00:00Z",
            state: "leased",
            why_now_json: "{\"reason\":\"due\"}",
            payload_version: 1,
            payload_json: "{\"workflow_id\":\"wf-a\"}",
            initiative_mode: "act",
            approval_state: "not_required",
            approval_proposal_id: None,
            approval_expires_at: None,
            owner_identity: Some("gateway:test"),
            owner_token: Some("owner-round-trip"),
            lease_epoch: lease.lease_epoch,
            side_effect_correlation_key: Some(
                "workflow-trigger:job-round-trip:2026-03-10T00:00:00Z",
            ),
            side_effect_status: "not_started",
            result_json: "{}",
            created_at: "2026-03-10T00:00:00Z",
            updated_at: "2026-03-10T00:00:00Z",
        },
    )
    .unwrap();

    reschedule_job(
        &conn,
        "job-round-trip",
        "run-round-trip",
        "owner-round-trip",
        lease.lease_epoch,
        &AutonomyJobReschedule {
            run_state: "waiting",
            job_state: "waiting",
            next_run_at: "2026-03-10T00:05:00Z",
            waiting_until: Some("2026-03-10T00:05:00Z"),
            side_effect_status: "not_started",
            result_json: "{\"retry\":true}",
            error_class: Some("transient"),
            error_message: Some("temporary failure"),
            updated_at: "2026-03-10T00:01:00Z",
        },
    )
    .unwrap();

    let job = get_job(&conn, "job-round-trip").unwrap().unwrap();
    assert_eq!(job.state, "waiting");
    assert_eq!(job.next_run_at, "2026-03-10T00:05:00Z");

    let lease = acquire_lease(
        &conn,
        "job-round-trip",
        "run-round-trip-2",
        "gateway:test",
        "owner-round-trip-2",
        "2026-03-10T00:05:00Z",
        "2026-03-10T00:06:00Z",
    )
    .unwrap()
    .unwrap();

    insert_run(
        &conn,
        &NewAutonomyRun {
            id: "run-round-trip-2",
            job_id: "job-round-trip",
            attempt: 1,
            trigger_source: "retry",
            triggered_at: "2026-03-10T00:05:00Z",
            due_at: "2026-03-10T00:05:00Z",
            state: "leased",
            why_now_json: "{\"reason\":\"retry_due\"}",
            payload_version: 1,
            payload_json: "{\"workflow_id\":\"wf-a\"}",
            initiative_mode: "act",
            approval_state: "not_required",
            approval_proposal_id: None,
            approval_expires_at: None,
            owner_identity: Some("gateway:test"),
            owner_token: Some("owner-round-trip-2"),
            lease_epoch: lease.lease_epoch,
            side_effect_correlation_key: Some(
                "workflow-trigger:job-round-trip:2026-03-10T00:05:00Z",
            ),
            side_effect_status: "not_started",
            result_json: "{}",
            created_at: "2026-03-10T00:05:00Z",
            updated_at: "2026-03-10T00:05:00Z",
        },
    )
    .unwrap();

    mark_run_running(
        &conn,
        "job-round-trip",
        "run-round-trip-2",
        "owner-round-trip-2",
        lease.lease_epoch,
        "2026-03-10T00:05:05Z",
    )
    .unwrap();

    finish_run(
        &conn,
        "job-round-trip",
        "run-round-trip-2",
        "owner-round-trip-2",
        lease.lease_epoch,
        &AutonomyRunFinish {
            next_state: "succeeded",
            side_effect_status: "applied",
            result_json: "{\"ok\":true}",
            error_class: None,
            error_message: None,
            terminal_reason: None,
            manual_review_required: false,
            completed_at: "2026-03-10T00:05:30Z",
            updated_at: "2026-03-10T00:05:30Z",
        },
    )
    .unwrap();

    let run = latest_run_for_job(&conn, "job-round-trip")
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "succeeded");

    let job = get_job(&conn, "job-round-trip").unwrap().unwrap();
    assert_eq!(job.state, "succeeded");
    assert!(get_lease(&conn, "job-round-trip").unwrap().is_none());
}

#[test]
fn autonomy_policy_and_suppression_queries_round_trip() {
    let conn = setup();
    upsert_policy(
        &conn,
        &UpsertAutonomyPolicy {
            id: "policy-1",
            scope_kind: "agent",
            scope_key: "agent-a",
            policy_version: 1,
            policy_json:
                "{\"quiet_hours\":{\"timezone\":\"UTC\",\"start_hour\":22,\"end_hour\":6}}",
            created_at: "2026-03-10T00:00:00Z",
            updated_at: "2026-03-10T00:00:00Z",
        },
    )
    .unwrap();

    let policy = cortex_storage::queries::autonomy_queries::get_policy(&conn, "agent", "agent-a")
        .unwrap()
        .unwrap();
    assert_eq!(policy.id, "policy-1");

    insert_suppression(
        &conn,
        &NewAutonomySuppression {
            id: "suppression-1",
            scope_kind: "agent",
            scope_key: "agent-a",
            fingerprint: "notify:daily-brief",
            reason: "too noisy",
            created_by: "user:test",
            created_at: "2026-03-10T00:00:00Z",
            expires_at: Some("2026-03-11T00:00:00Z"),
            active: true,
            policy_version: 1,
            metadata_json: "{\"source\":\"ui\"}",
        },
    )
    .unwrap();

    let suppressions =
        list_active_suppressions(&conn, "agent", "agent-a", "2026-03-10T12:00:00Z").unwrap();
    assert_eq!(suppressions.len(), 1);
    assert_eq!(suppressions[0].reason, "too noisy");
}
