//! Integration tests for cortex-storage migrations v016 + v017.
//! Tests append-only triggers, hash chain columns, goal_proposals UPDATE exception (AC10).

use cortex_storage::{open_in_memory, run_all_migrations};
use rusqlite::params;

fn setup_db() -> rusqlite::Connection {
    let conn = open_in_memory().expect("open in-memory DB");
    run_all_migrations(&conn).expect("run migrations");
    conn
}

// ── v016 migration tests ────────────────────────────────────────────────

#[test]
fn v016_triggers_exist() {
    let conn = setup_db();
    let triggers: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(triggers.contains(&"prevent_memory_events_update".to_string()));
    assert!(triggers.contains(&"prevent_memory_events_delete".to_string()));
    assert!(triggers.contains(&"prevent_audit_log_update".to_string()));
    assert!(triggers.contains(&"prevent_audit_log_delete".to_string()));
}

#[test]
fn v017_all_six_tables_created() {
    let conn = setup_db();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for expected in &[
        "itp_events",
        "convergence_scores",
        "intervention_history",
        "goal_proposals",
        "reflection_entries",
        "boundary_violations",
    ] {
        assert!(
            tables.contains(&expected.to_string()),
            "missing table: {}",
            expected
        );
    }
}

// ── itp_events append-only tests ────────────────────────────────────────

#[test]
fn insert_itp_event_succeeds() {
    let conn = setup_db();
    cortex_storage::queries::itp_event_queries::insert_itp_event(
        &conn,
        "evt-1",
        "sess-1",
        "SessionStart",
        Some("agent-1"),
        "2025-01-01T00:00:00Z",
        0,
        None,
        None,
        "standard",
        &[1u8; 32],
        &[0u8; 32],
    )
    .expect("insert should succeed");
}

#[test]
fn update_itp_events_rejected_by_trigger() {
    let conn = setup_db();
    cortex_storage::queries::itp_event_queries::insert_itp_event(
        &conn,
        "evt-1",
        "sess-1",
        "SessionStart",
        Some("agent-1"),
        "2025-01-01T00:00:00Z",
        0,
        None,
        None,
        "standard",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    let result = conn.execute(
        "UPDATE itp_events SET event_type = 'Modified' WHERE id = 'evt-1'",
        [],
    );
    assert!(result.is_err(), "UPDATE on itp_events should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("append-only"),
        "error should mention append-only: {}",
        err_msg
    );
}

#[test]
fn delete_itp_events_rejected_by_trigger() {
    let conn = setup_db();
    cortex_storage::queries::itp_event_queries::insert_itp_event(
        &conn,
        "evt-1",
        "sess-1",
        "SessionStart",
        Some("agent-1"),
        "2025-01-01T00:00:00Z",
        0,
        None,
        None,
        "standard",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    let result = conn.execute("DELETE FROM itp_events WHERE id = 'evt-1'", []);
    assert!(result.is_err(), "DELETE on itp_events should be rejected");
}

// ── convergence_scores append-only tests ────────────────────────────────

#[test]
fn update_convergence_scores_rejected() {
    let conn = setup_db();
    cortex_storage::queries::convergence_score_queries::insert_score(
        &conn,
        "sc-1",
        "agent-1",
        Some("sess-1"),
        0.5,
        "{}",
        1,
        "standard",
        "2025-01-01T00:00:00Z",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    let result = conn.execute(
        "UPDATE convergence_scores SET composite_score = 0.9 WHERE id = 'sc-1'",
        [],
    );
    assert!(
        result.is_err(),
        "UPDATE on convergence_scores should be rejected"
    );
}

// ── goal_proposals AC10 exception tests ─────────────────────────────────

#[test]
fn update_unresolved_goal_proposal_succeeds() {
    let conn = setup_db();
    cortex_storage::queries::goal_proposal_queries::insert_proposal(
        &conn,
        "prop-1",
        "agent-1",
        "sess-1",
        "Agent",
        "GoalChange",
        "AgentGoal",
        "{}",
        "[]",
        "HumanReviewRequired",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    // Resolve the unresolved proposal — should succeed (AC10)
    let updated = cortex_storage::queries::goal_proposal_queries::resolve_proposal(
        &conn,
        "prop-1",
        "AutoApproved",
        "human-1",
        "2025-01-01T01:00:00Z",
    )
    .expect("resolve should succeed");
    assert!(updated, "should have updated the unresolved proposal");
}

#[test]
fn update_resolved_goal_proposal_rejected() {
    let conn = setup_db();
    cortex_storage::queries::goal_proposal_queries::insert_proposal(
        &conn,
        "prop-1",
        "agent-1",
        "sess-1",
        "Agent",
        "GoalChange",
        "AgentGoal",
        "{}",
        "[]",
        "HumanReviewRequired",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    // First resolve
    cortex_storage::queries::goal_proposal_queries::resolve_proposal(
        &conn,
        "prop-1",
        "AutoApproved",
        "human-1",
        "2025-01-01T01:00:00Z",
    )
    .unwrap();
    // Try to update the now-resolved proposal — should be rejected by trigger
    let result = conn.execute(
        "UPDATE goal_proposals SET decision = 'AutoRejected' WHERE id = 'prop-1'",
        [],
    );
    assert!(
        result.is_err(),
        "UPDATE on resolved proposal should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("resolved proposals are immutable"),
        "error: {}",
        err_msg
    );
}

#[test]
fn delete_goal_proposals_rejected() {
    let conn = setup_db();
    cortex_storage::queries::goal_proposal_queries::insert_proposal(
        &conn,
        "prop-1",
        "agent-1",
        "sess-1",
        "Agent",
        "GoalChange",
        "AgentGoal",
        "{}",
        "[]",
        "HumanReviewRequired",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    let result = conn.execute("DELETE FROM goal_proposals WHERE id = 'prop-1'", []);
    assert!(
        result.is_err(),
        "DELETE on goal_proposals should be rejected"
    );
}

// ── Delete on any convergence table rejected ────────────────────────────

#[test]
fn delete_on_all_convergence_tables_rejected() {
    let conn = setup_db();
    // Insert into each table, then try DELETE
    conn.execute(
        "INSERT INTO reflection_entries (id, session_id, chain_id, depth, trigger_type,
         reflection_text, event_hash, previous_hash)
         VALUES ('r1', 's1', 'c1', 0, 'auto', 'text', x'01', x'00')",
        [],
    )
    .unwrap();
    assert!(conn
        .execute("DELETE FROM reflection_entries WHERE id = 'r1'", [])
        .is_err());

    conn.execute(
        "INSERT INTO boundary_violations (id, session_id, violation_type, severity,
         trigger_text_hash, action_taken, event_hash, previous_hash)
         VALUES ('bv1', 's1', 'identity', 0.9, 'hash', 'blocked', x'01', x'00')",
        [],
    )
    .unwrap();
    assert!(conn
        .execute("DELETE FROM boundary_violations WHERE id = 'bv1'", [])
        .is_err());
}

// ── Query module tests ──────────────────────────────────────────────────

#[test]
fn query_itp_events_by_session() {
    let conn = setup_db();
    for i in 0..5 {
        cortex_storage::queries::itp_event_queries::insert_itp_event(
            &conn,
            &format!("evt-{}", i),
            "sess-1",
            "InteractionMessage",
            Some("agent-1"),
            "2025-01-01T00:00:00Z",
            i,
            None,
            None,
            "standard",
            &[i as u8; 32],
            &[0u8; 32],
        )
        .unwrap();
    }
    let rows =
        cortex_storage::queries::itp_event_queries::query_by_session(&conn, "sess-1").unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].sequence_number, 0);
    assert_eq!(rows[4].sequence_number, 4);
}

#[test]
fn query_convergence_scores_by_agent() {
    let conn = setup_db();
    cortex_storage::queries::convergence_score_queries::insert_score(
        &conn,
        "sc-1",
        "agent-1",
        Some("sess-1"),
        0.42,
        "{}",
        1,
        "standard",
        "2025-01-01T00:00:00Z",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    let rows = cortex_storage::queries::convergence_score_queries::query_by_agent(&conn, "agent-1")
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!((rows[0].composite_score - 0.42).abs() < 1e-6);
}

#[test]
fn latest_score_by_agent() {
    let conn = setup_db();
    cortex_storage::queries::convergence_score_queries::insert_score(
        &conn,
        "sc-1",
        "agent-1",
        None,
        0.3,
        "{}",
        1,
        "standard",
        "2025-01-01T00:00:00Z",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    cortex_storage::queries::convergence_score_queries::insert_score(
        &conn,
        "sc-2",
        "agent-1",
        None,
        0.7,
        "{}",
        2,
        "standard",
        "2025-01-02T00:00:00Z",
        &[2u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    let latest =
        cortex_storage::queries::convergence_score_queries::latest_by_agent(&conn, "agent-1")
            .unwrap()
            .expect("should have a latest score");
    assert!((latest.composite_score - 0.7).abs() < 1e-6);
}

#[test]
fn query_pending_proposals() {
    let conn = setup_db();
    cortex_storage::queries::goal_proposal_queries::insert_proposal(
        &conn,
        "p1",
        "a1",
        "s1",
        "Agent",
        "GoalChange",
        "AgentGoal",
        "{}",
        "[]",
        "HumanReviewRequired",
        &[1u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    cortex_storage::queries::goal_proposal_queries::insert_proposal(
        &conn,
        "p2",
        "a1",
        "s1",
        "Agent",
        "GoalChange",
        "AgentGoal",
        "{}",
        "[]",
        "HumanReviewRequired",
        &[2u8; 32],
        &[0u8; 32],
    )
    .unwrap();
    // Resolve p1
    cortex_storage::queries::goal_proposal_queries::resolve_proposal(
        &conn,
        "p1",
        "AutoApproved",
        "human",
        "2025-01-01T01:00:00Z",
    )
    .unwrap();
    let pending = cortex_storage::queries::goal_proposal_queries::query_pending(&conn).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "p2");
}

// ── Adversarial tests ───────────────────────────────────────────────────

#[test]
fn adversarial_insert_with_null_event_hash_rejected() {
    let conn = setup_db();
    // event_hash is NOT NULL, so inserting NULL should fail
    let result = conn.execute(
        "INSERT INTO itp_events (id, session_id, event_type, timestamp, sequence_number,
         privacy_level, event_hash, previous_hash)
         VALUES ('x', 's', 'T', '2025-01-01', 0, 'standard', NULL, x'00')",
        [],
    );
    assert!(
        result.is_err(),
        "NULL event_hash should be rejected by NOT NULL constraint"
    );
}

// ── v018 delegation_state tests ─────────────────────────────────────────

#[test]
fn v018_delegation_state_table_created() {
    let conn = setup_db();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        tables.contains(&"delegation_state".to_string()),
        "delegation_state table should exist"
    );
}

#[test]
fn v018_insert_delegation_succeeds() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO delegation_state (id, delegation_id, sender_id, recipient_id,
         task, state, offer_message_id, event_hash, previous_hash)
         VALUES ('d1', 'del-1', 'agent-a', 'agent-b', 'summarize', 'Offered',
                 'msg-1', x'01', x'00')",
        [],
    )
    .expect("insert delegation should succeed");
}

#[test]
fn v018_update_offered_delegation_succeeds() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO delegation_state (id, delegation_id, sender_id, recipient_id,
         task, state, offer_message_id, event_hash, previous_hash)
         VALUES ('d1', 'del-1', 'agent-a', 'agent-b', 'summarize', 'Offered',
                 'msg-1', x'01', x'00')",
        [],
    )
    .unwrap();
    // Update Offered → Accepted should succeed
    let result = conn.execute(
        "UPDATE delegation_state SET state = 'Accepted', accept_message_id = 'msg-2',
         updated_at = datetime('now') WHERE id = 'd1'",
        [],
    );
    assert!(
        result.is_ok(),
        "UPDATE on Offered delegation should succeed"
    );
}

#[test]
fn v018_update_resolved_delegation_rejected() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO delegation_state (id, delegation_id, sender_id, recipient_id,
         task, state, offer_message_id, event_hash, previous_hash)
         VALUES ('d1', 'del-1', 'agent-a', 'agent-b', 'summarize', 'Completed',
                 'msg-1', x'01', x'00')",
        [],
    )
    .unwrap();
    // Update on Completed delegation should be rejected
    let result = conn.execute(
        "UPDATE delegation_state SET state = 'Disputed' WHERE id = 'd1'",
        [],
    );
    assert!(
        result.is_err(),
        "UPDATE on resolved delegation should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("resolved delegations are immutable"),
        "error: {}",
        err_msg
    );
}

#[test]
fn v018_delete_delegation_rejected() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO delegation_state (id, delegation_id, sender_id, recipient_id,
         task, state, offer_message_id, event_hash, previous_hash)
         VALUES ('d1', 'del-1', 'agent-a', 'agent-b', 'summarize', 'Offered',
                 'msg-1', x'01', x'00')",
        [],
    )
    .unwrap();
    let result = conn.execute("DELETE FROM delegation_state WHERE id = 'd1'", []);
    assert!(
        result.is_err(),
        "DELETE on delegation_state should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("append-only"), "error: {}", err_msg);
}

#[test]
fn v018_rejected_delegation_immutable() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO delegation_state (id, delegation_id, sender_id, recipient_id,
         task, state, offer_message_id, event_hash, previous_hash)
         VALUES ('d1', 'del-1', 'agent-a', 'agent-b', 'summarize', 'Rejected',
                 'msg-1', x'01', x'00')",
        [],
    )
    .unwrap();
    let result = conn.execute(
        "UPDATE delegation_state SET state = 'Offered' WHERE id = 'd1'",
        [],
    );
    assert!(
        result.is_err(),
        "UPDATE on Rejected delegation should be rejected"
    );
}

#[test]
fn v018_disputed_delegation_immutable() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO delegation_state (id, delegation_id, sender_id, recipient_id,
         task, state, offer_message_id, event_hash, previous_hash)
         VALUES ('d1', 'del-1', 'agent-a', 'agent-b', 'summarize', 'Disputed',
                 'msg-1', x'01', x'00')",
        [],
    )
    .unwrap();
    let result = conn.execute(
        "UPDATE delegation_state SET state = 'Completed' WHERE id = 'd1'",
        [],
    );
    assert!(
        result.is_err(),
        "UPDATE on Disputed delegation should be rejected"
    );
}

#[test]
fn v018_schema_version_updated() {
    let conn = setup_db();
    let version: u32 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        version,
        cortex_storage::migrations::LATEST_VERSION,
        "Latest migration version should match LATEST_VERSION"
    );
}

// ── v019 migration tests ────────────────────────────────────────────────

#[test]
fn v019_intervention_state_table_created() {
    let conn = setup_db();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        tables.contains(&"intervention_state".to_string()),
        "v019 should create intervention_state table"
    );
}

#[test]
fn v019_indexes_created() {
    let conn = setup_db();
    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        indexes.contains(&"idx_memory_events_memory_id".to_string()),
        "v019 should create idx_memory_events_memory_id"
    );
    assert!(
        indexes.contains(&"idx_itp_events_event_type".to_string()),
        "v019 should create idx_itp_events_event_type"
    );
}

// ── v020 migration tests ────────────────────────────────────────────────

#[test]
fn v020_audit_log_table_created_with_actor_id() {
    let conn = setup_db();
    // audit_log should exist (created by v020 if not already present)
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        tables.contains(&"audit_log".to_string()),
        "v020 should ensure audit_log table exists"
    );

    // actor_id column should exist
    let has_actor_id: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('audit_log') WHERE name = 'actor_id'")
        .unwrap()
        .exists([])
        .unwrap();
    assert!(has_actor_id, "v020 should add actor_id column to audit_log");
}

#[test]
fn v020_audit_log_actor_id_index_exists() {
    let conn = setup_db();
    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        indexes.contains(&"idx_audit_log_actor_id".to_string()),
        "v020 should create idx_audit_log_actor_id"
    );
}

#[test]
fn v020_audit_log_insert_with_actor_id_succeeds() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO audit_log (id, timestamp, agent_id, event_type, severity, details, actor_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "test-1",
            "2026-03-01T00:00:00Z",
            "agent-1",
            "test_event",
            "info",
            "test details",
            "user@example.com"
        ],
    )
    .expect("INSERT with actor_id should succeed");

    let actor: Option<String> = conn
        .query_row(
            "SELECT actor_id FROM audit_log WHERE id = 'test-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(actor, Some("user@example.com".to_string()));
}

#[test]
fn v020_audit_log_insert_without_actor_id_succeeds() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO audit_log (id, timestamp, agent_id, event_type, severity, details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "test-2",
            "2026-03-01T00:00:00Z",
            "agent-1",
            "test_event",
            "info",
            "no actor"
        ],
    )
    .expect("INSERT without actor_id should succeed (NULL allowed)");

    let actor: Option<String> = conn
        .query_row(
            "SELECT actor_id FROM audit_log WHERE id = 'test-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(actor, None, "actor_id should be NULL when not provided");
}

#[test]
fn v020_migration_is_idempotent_with_existing_audit_log() {
    // Simulate the case where ghost-audit's ensure_table() already created
    // audit_log before migrations run. v020 should handle this gracefully.
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .unwrap();

    // Pre-create audit_log (as ghost-audit would)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS audit_log (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            severity TEXT NOT NULL DEFAULT 'info',
            tool_name TEXT,
            details TEXT NOT NULL DEFAULT '',
            session_id TEXT
        );",
    )
    .unwrap();

    // Insert a pre-existing row
    conn.execute(
        "INSERT INTO audit_log (id, timestamp, agent_id, event_type, details)
         VALUES ('pre-existing', '2026-01-01T00:00:00Z', 'agent-0', 'boot', 'before migration')",
        [],
    )
    .unwrap();

    // Now run all migrations — v020 should add actor_id without destroying data
    cortex_storage::run_all_migrations(&conn)
        .expect("migrations should succeed on pre-existing audit_log");

    // Pre-existing row should still be there with NULL actor_id
    let (details, actor): (String, Option<String>) = conn
        .query_row(
            "SELECT details, actor_id FROM audit_log WHERE id = 'pre-existing'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(details, "before migration");
    assert_eq!(actor, None, "pre-existing rows should have NULL actor_id");
}
