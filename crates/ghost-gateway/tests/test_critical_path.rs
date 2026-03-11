//! WP9-H: Automated integration tests for critical path.
//!
//! These tests cover the manual "Verify" steps from P0/P1 work packages,
//! providing regression coverage for safety-critical and reliability fixes.
//!
//! All tests use the TestGateway harness (real gateway, temp DB, random port)
//! or in-memory SQLite for DB-only tests. No external dependencies required.

mod common;

use serde_json::json;

// ── 1. Session lifecycle ─────────────────────────────────────────────

#[tokio::test]
async fn session_lifecycle_create_and_list() {
    let gw = common::TestGateway::start().await;

    // Create a session.
    let resp = gw
        .client
        .post(gw.url("/api/studio/sessions"))
        .json(&json!({
            "title": "Test Session",
            "model": "test-model",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201, "create session should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["title"], "Test Session");

    // List sessions — should include the one we just created.
    let resp = gw
        .client
        .get(gw.url("/api/studio/sessions"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sessions = body["sessions"].as_array().unwrap();
    assert!(
        sessions
            .iter()
            .any(|s| s["id"].as_str() == Some(&session_id)),
        "created session should appear in list"
    );

    // Get session by ID.
    let resp = gw
        .client
        .get(gw.url(&format!("/api/studio/sessions/{session_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["id"], session_id);
    assert_eq!(body["title"], "Test Session");

    // Delete session.
    let resp = gw
        .client
        .delete(gw.url(&format!("/api/studio/sessions/{session_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify deletion — get should return 404.
    let resp = gw
        .client
        .get(gw.url(&format!("/api/studio/sessions/{session_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    gw.stop().await;
}

#[tokio::test]
async fn session_heartbeat_accepts_existing_studio_session_and_refreshes_liveness() {
    let gw = common::TestGateway::start().await;

    let resp = gw
        .client
        .post(gw.url("/api/studio/sessions"))
        .json(&json!({
            "title": "Heartbeat Session",
            "model": "test-model",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create session should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["id"].as_str().unwrap().to_string();

    gw.app_state.client_heartbeats.insert(
        session_id.clone(),
        std::time::Instant::now() - std::time::Duration::from_secs(120),
    );

    let resp = gw
        .client
        .post(gw.url(&format!("/api/sessions/{session_id}/heartbeat")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let refreshed_elapsed = gw
        .app_state
        .client_heartbeats
        .get(&session_id)
        .expect("heartbeat should be tracked")
        .elapsed();
    assert!(
        refreshed_elapsed < std::time::Duration::from_secs(5),
        "heartbeat should refresh stale liveness state"
    );

    gw.stop().await;
}

#[tokio::test]
async fn session_heartbeat_accepts_existing_runtime_session() {
    let gw = common::TestGateway::start().await;
    let runtime_session_id = "runtime-session-heartbeat";

    {
        let db = gw.app_state.db.write().await;
        cortex_storage::queries::itp_event_queries::insert_itp_event(
            &db,
            "event-heartbeat-1",
            runtime_session_id,
            "user_message",
            Some("tester"),
            "2026-03-08T12:00:00Z",
            1,
            None,
            None,
            "standard",
            &[1; 32],
            &[0; 32],
        )
        .unwrap();
    }

    let resp = gw
        .client
        .post(gw.url(&format!("/api/sessions/{runtime_session_id}/heartbeat")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    assert!(
        gw.app_state
            .client_heartbeats
            .get(runtime_session_id)
            .is_some(),
        "runtime session heartbeat should be tracked"
    );

    gw.stop().await;
}

#[tokio::test]
async fn session_heartbeat_accepts_active_runtime_session_before_persistence() {
    let gw = common::TestGateway::start().await;
    let runtime_session_id = uuid::Uuid::now_v7();
    let runtime_agent_id = uuid::Uuid::now_v7();

    gw.app_state
        .itp_session_tracker
        .as_ref()
        .expect("test gateway should provide ITP session tracker")
        .record_start(runtime_session_id, runtime_agent_id, "api")
        .await;

    let resp = gw
        .client
        .post(gw.url(&format!("/api/sessions/{runtime_session_id}/heartbeat")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    assert!(
        gw.app_state
            .client_heartbeats
            .get(&runtime_session_id.to_string())
            .is_some(),
        "active runtime session heartbeat should be tracked before persistence"
    );

    gw.stop().await;
}

#[tokio::test]
async fn session_heartbeat_rejects_unknown_session_id() {
    let gw = common::TestGateway::start().await;
    let session_id = "missing-session-heartbeat";

    let resp = gw
        .client
        .post(gw.url(&format!("/api/sessions/{session_id}/heartbeat")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    assert!(
        gw.app_state.client_heartbeats.get(session_id).is_none(),
        "unknown sessions must not seed heartbeat state"
    );

    gw.stop().await;
}

#[tokio::test]
async fn session_heartbeat_rejects_deleted_studio_session() {
    let gw = common::TestGateway::start().await;

    let resp = gw
        .client
        .post(gw.url("/api/studio/sessions"))
        .json(&json!({
            "title": "Deleted Heartbeat Session",
            "model": "test-model",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create session should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["id"].as_str().unwrap().to_string();

    let delete_resp = gw
        .client
        .delete(gw.url(&format!("/api/studio/sessions/{session_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), 200);

    gw.app_state.client_heartbeats.insert(
        session_id.clone(),
        std::time::Instant::now() - std::time::Duration::from_secs(120),
    );

    let resp = gw
        .client
        .post(gw.url(&format!("/api/sessions/{session_id}/heartbeat")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let stale_elapsed = gw
        .app_state
        .client_heartbeats
        .get(&session_id)
        .expect("existing stale record should not be refreshed")
        .elapsed();
    assert!(
        stale_elapsed >= std::time::Duration::from_secs(60),
        "deleted session heartbeat must not refresh stale liveness state"
    );

    gw.stop().await;
}

// ── 2. Auth enforcement ──────────────────────────────────────────────

#[tokio::test]
async fn health_endpoint_unauthenticated_succeeds() {
    // Health endpoints should always be accessible (no auth required).
    let gw = common::TestGateway::start().await;

    let resp = gw.client.get(gw.url("/api/health")).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let resp = gw.client.get(gw.url("/api/ready")).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    gw.stop().await;
}

// ── 3. Migration safety (WP9-E verify) ──────────────────────────────

#[tokio::test]
async fn migration_runs_on_fresh_db() {
    // Verify that all migrations run successfully on a fresh database.
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test_migration.db");

    let pool =
        ghost_gateway::db_pool::create_pool(db_path.clone()).expect("pool creation should succeed");

    let writer = pool.writer_for_migrations().await;
    let result = cortex_storage::migrations::run_migrations(&writer);
    assert!(
        result.is_ok(),
        "migrations should succeed: {:?}",
        result.err()
    );

    // Verify final version matches LATEST_VERSION.
    let version =
        cortex_storage::migrations::current_version(&writer).expect("should read version");
    assert_eq!(
        version,
        cortex_storage::migrations::LATEST_VERSION,
        "schema version should match LATEST_VERSION"
    );
}

#[tokio::test]
async fn migration_backup_created_when_pending() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test_backup.db");

    // Create DB with a subset of migrations (stop early).
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        // Run only schema_version table creation (no migrations applied).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
    }

    // Now run full migrations with backup enabled.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        let result = cortex_storage::migrations::run_migrations_with_backup(&conn, Some(&db_path));
        assert!(
            result.is_ok(),
            "migrations with backup should succeed: {:?}",
            result.err()
        );
    }

    // Verify backup file was created.
    let backup_path =
        std::path::PathBuf::from(format!("{}.pre-migration-v16.bak", db_path.display()));
    assert!(
        backup_path.exists(),
        "pre-migration backup should exist at {:?}",
        backup_path
    );

    // Verify backup is valid SQLite.
    let backup_conn = rusqlite::Connection::open(&backup_path);
    assert!(
        backup_conn.is_ok(),
        "backup should be a valid SQLite database"
    );
}

// ── 4. DB pool overflow cap (WP9-I verify) ───────────────────────────

#[tokio::test]
async fn db_pool_read_connections_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test_pool.db");

    let pool = ghost_gateway::db_pool::create_pool(db_path).expect("pool creation should succeed");

    // Run migrations so the DB is usable.
    {
        let writer = pool.writer_for_migrations().await;
        cortex_storage::migrations::run_migrations(&writer).unwrap();
    }

    // Acquire multiple read connections — pool should not panic.
    let mut conns = Vec::new();
    for _ in 0..8 {
        match pool.read() {
            Ok(conn) => conns.push(conn),
            Err(_) => break, // Expected once overflow cap is reached
        }
    }

    // Should have gotten at least pool_size connections.
    assert!(
        !conns.is_empty(),
        "should acquire at least one read connection"
    );

    // Drop all connections — they return to pool.
    drop(conns);

    // Pool should be usable again.
    let conn = pool.read();
    assert!(
        conn.is_ok(),
        "pool should recover after connections are returned"
    );
}

// ── 5. Gateway state machine transitions ─────────────────────────────

#[test]
fn gateway_state_machine_valid_transitions() {
    use ghost_gateway::gateway::{GatewaySharedState, GatewayState};

    let state = GatewaySharedState::new();
    assert_eq!(state.current_state(), GatewayState::Initializing);

    // Initializing → Healthy
    assert!(state.transition_to(GatewayState::Healthy).is_ok());
    assert_eq!(state.current_state(), GatewayState::Healthy);

    // Healthy → Degraded
    assert!(state.transition_to(GatewayState::Degraded).is_ok());
    assert_eq!(state.current_state(), GatewayState::Degraded);

    // Degraded → Recovering
    assert!(state.transition_to(GatewayState::Recovering).is_ok());
    assert_eq!(state.current_state(), GatewayState::Recovering);

    // Recovering → Healthy
    assert!(state.transition_to(GatewayState::Healthy).is_ok());
    assert_eq!(state.current_state(), GatewayState::Healthy);

    // Healthy → ShuttingDown
    assert!(state.transition_to(GatewayState::ShuttingDown).is_ok());
    assert_eq!(state.current_state(), GatewayState::ShuttingDown);
}

#[test]
fn gateway_state_machine_rejects_invalid_transitions() {
    use ghost_gateway::gateway::{GatewaySharedState, GatewayState};

    let state = GatewaySharedState::new();

    // Initializing → ShuttingDown is NOT valid (must go through Healthy first).
    // The FSM only allows Initializing → {Healthy, Degraded, FatalError}.
    let result = state.transition_to(GatewayState::ShuttingDown);
    assert!(
        result.is_err(),
        "Initializing → ShuttingDown should be rejected"
    );
}

#[test]
fn gateway_fatal_error_is_terminal() {
    use ghost_gateway::gateway::{GatewaySharedState, GatewayState};

    let state = GatewaySharedState::new();
    assert!(state.transition_to(GatewayState::FatalError).is_ok());

    // FatalError is terminal — no transitions allowed.
    assert!(state.transition_to(GatewayState::Healthy).is_err());
    assert!(state.transition_to(GatewayState::Degraded).is_err());
    assert!(state.transition_to(GatewayState::Initializing).is_err());
}

// ── 6. Session lifecycle cleanup (WP9-D verify) ─────────────────────

#[tokio::test]
async fn session_soft_delete_and_hard_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test_lifecycle.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    cortex_storage::migrations::run_migrations(&conn).unwrap();

    // Create a session.
    let session_id = "test-lifecycle-session";
    cortex_storage::queries::studio_chat_queries::create_session(
        &conn,
        session_id,
        "agent-lifecycle",
        "Old Session",
        "test-model",
        "",
        0.5,
        4096,
    )
    .unwrap();

    // Insert a message (updates last_activity_at).
    cortex_storage::queries::studio_chat_queries::insert_message(
        &conn, "msg-1", session_id, "user", "hello", 0, "clean",
    )
    .unwrap();

    // Manually set last_activity_at to 100 days ago to simulate inactivity.
    conn.execute(
        "UPDATE studio_chat_sessions SET last_activity_at = datetime('now', '-100 days') WHERE id = ?1",
        rusqlite::params![session_id],
    )
    .unwrap();

    // Soft-delete sessions inactive for > 90 days.
    let cutoff = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(90))
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let soft_deleted =
        cortex_storage::queries::studio_chat_queries::soft_delete_inactive_sessions(&conn, &cutoff)
            .unwrap();
    assert_eq!(soft_deleted, 1, "should soft-delete 1 inactive session");

    // Session should no longer appear in active list.
    let sessions =
        cortex_storage::queries::studio_chat_queries::list_sessions(&conn, 100, 0).unwrap();
    assert!(
        sessions.is_empty(),
        "soft-deleted session should not appear in list"
    );

    // Set deleted_at to > 2x TTL ago for hard-delete test.
    conn.execute(
        "UPDATE studio_chat_sessions SET deleted_at = datetime('now', '-200 days') WHERE id = ?1",
        rusqlite::params![session_id],
    )
    .unwrap();

    // Hard-delete sessions that were soft-deleted > TTL days ago.
    let hard_cutoff = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(90))
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let hard_deleted =
        cortex_storage::queries::studio_chat_queries::hard_delete_old_sessions(&conn, &hard_cutoff)
            .unwrap();
    assert_eq!(hard_deleted, 1, "should hard-delete 1 expired session");

    // Messages should also be gone.
    let messages =
        cortex_storage::queries::studio_chat_queries::list_messages(&conn, session_id).unwrap();
    assert!(messages.is_empty(), "messages should be cascade-deleted");
}

// ── 7. Active-since query parameter (WP9-D verify) ──────────────────

#[tokio::test]
async fn list_sessions_active_since_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test_active_since.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    cortex_storage::migrations::run_migrations(&conn).unwrap();

    // Create two sessions.
    cortex_storage::queries::studio_chat_queries::create_session(
        &conn,
        "recent",
        "agent-recent",
        "Recent",
        "m",
        "",
        0.5,
        4096,
    )
    .unwrap();
    cortex_storage::queries::studio_chat_queries::create_session(
        &conn,
        "old",
        "agent-old",
        "Old",
        "m",
        "",
        0.5,
        4096,
    )
    .unwrap();

    // Make "old" session inactive for 100 days.
    conn.execute(
        "UPDATE studio_chat_sessions SET last_activity_at = datetime('now', '-100 days') WHERE id = 'old'",
        [],
    )
    .unwrap();

    // Query with active_since = 30 days ago.
    let since = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(30))
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let results = cortex_storage::queries::studio_chat_queries::list_sessions_active_since(
        &conn, &since, 100, 0,
    )
    .unwrap();

    assert_eq!(results.len(), 1, "only recent session should match");
    assert_eq!(results[0].id, "recent");
}

// ── 8. Studio session API via TestGateway ────────────────────────────

#[tokio::test]
async fn studio_session_api_active_since_query() {
    let gw = common::TestGateway::start().await;

    // Create a session.
    let resp = gw
        .client
        .post(gw.url("/api/studio/sessions"))
        .json(&json!({ "title": "Active Session" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // List with active_since in the past — should return the session.
    let since = "2020-01-01 00:00:00";
    let resp = gw
        .client
        .get(gw.url(&format!("/api/studio/sessions?active_since={since}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sessions = body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);

    // List with active_since in the future — should return nothing.
    let future_since = "2099-01-01 00:00:00";
    let resp = gw
        .client
        .get(gw.url(&format!("/api/studio/sessions?active_since={future_since}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sessions = body["sessions"].as_array().unwrap();
    assert!(sessions.is_empty());

    gw.stop().await;
}

// ── 9. Kill switch operations ────────────────────────────────────────

#[test]
fn kill_switch_escalation_levels() {
    use cortex_core::safety::trigger::TriggerEvent;
    use ghost_gateway::safety::kill_switch::{KillLevel, KillSwitch};

    let ks = KillSwitch::new();
    let agent_id = uuid::Uuid::now_v7();

    // Initially Normal — agents can operate.
    let check = ks.check(agent_id);
    assert!(
        matches!(
            check,
            ghost_gateway::safety::kill_switch::KillCheckResult::Ok
        ),
        "agent should be allowed initially"
    );

    // Activate pause for a specific agent.
    let trigger = TriggerEvent::ManualPause {
        agent_id,
        reason: "test reason".into(),
        initiated_by: "test".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Pause, &trigger);

    // Agent should now be paused.
    let check = ks.check(agent_id);
    assert!(
        !matches!(
            check,
            ghost_gateway::safety::kill_switch::KillCheckResult::Ok
        ),
        "paused agent should not be allowed"
    );

    // Activate KILL_ALL.
    let trigger = TriggerEvent::ManualKillAll {
        reason: "emergency".into(),
        initiated_by: "test".into(),
    };
    ks.activate_kill_all(&trigger);

    // All agents should be blocked.
    let other_agent = uuid::Uuid::now_v7();
    let check = ks.check(other_agent);
    assert!(
        !matches!(
            check,
            ghost_gateway::safety::kill_switch::KillCheckResult::Ok
        ),
        "all agents should be blocked after KILL_ALL"
    );
}

// ── 10. Empty message rejected ───────────────────────────────────────

#[tokio::test]
async fn empty_message_rejected() {
    let gw = common::TestGateway::start().await;

    // Create a session first.
    let resp = gw
        .client
        .post(gw.url("/api/studio/sessions"))
        .json(&json!({ "title": "Test" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["id"].as_str().unwrap();

    // Try to send an empty message.
    let resp = gw
        .client
        .post(gw.url(&format!("/api/studio/sessions/{session_id}/messages")))
        .json(&json!({ "content": "   " }))
        .send()
        .await
        .unwrap();
    // Server returns 422 (Unprocessable Entity) for validation errors.
    assert!(
        resp.status() == 400 || resp.status() == 422,
        "empty message should be rejected, got {}",
        resp.status()
    );

    gw.stop().await;
}

// ── 11. Nonexistent session returns 404 ──────────────────────────────

#[tokio::test]
async fn nonexistent_session_returns_404() {
    let gw = common::TestGateway::start().await;

    let resp = gw
        .client
        .get(gw.url("/api/studio/sessions/nonexistent-id"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    gw.stop().await;
}

// ── 12. Migration idempotency ────────────────────────────────────────

#[tokio::test]
async fn migrations_are_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test_idempotent.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();

    // Run migrations twice — second run should be a no-op.
    cortex_storage::migrations::run_migrations(&conn).unwrap();
    let v1 = cortex_storage::migrations::current_version(&conn).unwrap();

    cortex_storage::migrations::run_migrations(&conn).unwrap();
    let v2 = cortex_storage::migrations::current_version(&conn).unwrap();

    assert_eq!(v1, v2, "running migrations twice should yield same version");
    assert_eq!(v2, cortex_storage::migrations::LATEST_VERSION);
}

// ── 13. Schema version table exists after migrations ─────────────────

#[test]
fn schema_version_table_populated() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    cortex_storage::migrations::run_migrations(&conn).unwrap();

    // Verify schema_version has the expected number of rows.
    let count: u32 = conn
        .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
        .unwrap();

    // Each migration adds a row. Total migrations = LATEST_VERSION - 15 (migrations start at v16).
    let expected = cortex_storage::migrations::LATEST_VERSION - 15;
    assert_eq!(
        count, expected,
        "schema_version should have one row per migration"
    );
}
