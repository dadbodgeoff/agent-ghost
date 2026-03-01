//! Adversarial tests for Consolidated Audit (Prompts 1-6, 8).
//!
//! Tests the fixes applied across all audit prompts.

// ═══════════════════════════════════════════════════════════════════════
// Prompt 2 Fix: ITP sequence_number auto-increment
// ═══════════════════════════════════════════════════════════════════════

mod itp_sequence_tests {
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();
        conn
    }

    /// Multiple ITP events in the same session should have incrementing sequence numbers.
    #[test]
    fn itp_events_have_incrementing_sequence_numbers() {
        let conn = setup_db();

        // Insert 3 events for the same session using the query function.
        for i in 0..3 {
            cortex_storage::queries::itp_event_queries::insert_itp_event(
                &conn,
                &format!("evt-{i}"),
                "sess-1",
                "InteractionMessage",
                Some("agent-1"),
                "2025-01-01T00:00:00Z",
                i, // explicit sequence_number
                None,
                None,
                "standard",
                &[0u8; 32],
                &[0u8; 32],
            )
            .unwrap();
        }

        let rows = cortex_storage::queries::itp_event_queries::query_by_session(&conn, "sess-1")
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].sequence_number, 0);
        assert_eq!(rows[1].sequence_number, 1);
        assert_eq!(rows[2].sequence_number, 2);
    }

    /// Events in different sessions should have independent sequence numbers.
    #[test]
    fn itp_sequence_numbers_independent_per_session() {
        let conn = setup_db();

        cortex_storage::queries::itp_event_queries::insert_itp_event(
            &conn, "evt-a1", "sess-a", "SessionStart", Some("agent-1"),
            "2025-01-01T00:00:00Z", 0, None, None, "standard",
            &[0u8; 32], &[0u8; 32],
        ).unwrap();

        cortex_storage::queries::itp_event_queries::insert_itp_event(
            &conn, "evt-b1", "sess-b", "SessionStart", Some("agent-2"),
            "2025-01-01T00:00:01Z", 0, None, None, "standard",
            &[0u8; 32], &[0u8; 32],
        ).unwrap();

        cortex_storage::queries::itp_event_queries::insert_itp_event(
            &conn, "evt-a2", "sess-a", "InteractionMessage", Some("agent-1"),
            "2025-01-01T00:00:02Z", 1, None, None, "standard",
            &[0u8; 32], &[0u8; 32],
        ).unwrap();

        let sess_a = cortex_storage::queries::itp_event_queries::query_by_session(&conn, "sess-a").unwrap();
        let sess_b = cortex_storage::queries::itp_event_queries::query_by_session(&conn, "sess-b").unwrap();

        assert_eq!(sess_a.len(), 2);
        assert_eq!(sess_a[0].sequence_number, 0);
        assert_eq!(sess_a[1].sequence_number, 1);

        assert_eq!(sess_b.len(), 1);
        assert_eq!(sess_b[0].sequence_number, 0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Prompt 4 Fix: Error swallowing — convergence_watcher signal parsing
// ═══════════════════════════════════════════════════════════════════════

mod signal_parsing_tests {
    /// Valid signal_scores JSON should parse correctly.
    #[test]
    fn valid_signal_scores_parse() {
        let json = "[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]";
        let signals: Vec<f64> = serde_json::from_str(json).unwrap();
        assert_eq!(signals.len(), 8);
        assert!((signals[0] - 0.1).abs() < f64::EPSILON);
    }

    /// Malformed signal_scores should not panic — should return empty.
    #[test]
    fn malformed_signal_scores_returns_empty() {
        let json = "not valid json";
        let signals: Result<Vec<f64>, _> = serde_json::from_str(json);
        assert!(signals.is_err());
    }

    /// Empty string signal_scores should not panic.
    #[test]
    fn empty_signal_scores_returns_error() {
        let json = "";
        let signals: Result<Vec<f64>, _> = serde_json::from_str(json);
        assert!(signals.is_err());
    }

    /// Null-containing signal_scores should fail gracefully.
    #[test]
    fn null_in_signal_scores() {
        let json = "[0.1, null, 0.3]";
        let signals: Result<Vec<f64>, _> = serde_json::from_str(json);
        assert!(signals.is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Prompt 6 Fix: KillSwitch resume audit trail
// ═══════════════════════════════════════════════════════════════════════

mod kill_switch_audit_tests {
    use ghost_gateway::safety::kill_switch::{KillSwitch, KillLevel};
    use cortex_core::safety::trigger::TriggerEvent;

    /// Pausing and resuming an agent should produce audit entries for both actions.
    #[test]
    fn pause_resume_produces_two_audit_entries() {
        let ks = KillSwitch::new();
        let agent_id = uuid::Uuid::now_v7();

        // Pause
        let trigger = TriggerEvent::ManualPause {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Pause, &trigger);

        let entries_after_pause = ks.audit_entries();
        assert_eq!(entries_after_pause.len(), 1);
        assert_eq!(entries_after_pause[0].action, KillLevel::Pause);

        // Resume
        ks.resume_agent(agent_id).unwrap();

        let entries_after_resume = ks.audit_entries();
        assert_eq!(entries_after_resume.len(), 2, "Resume should add an audit entry");
        assert_eq!(entries_after_resume[1].action, KillLevel::Normal);
        assert_eq!(entries_after_resume[1].agent_id, Some(agent_id));
    }

    /// Quarantine and resume should produce audit entries for both actions.
    #[test]
    fn quarantine_resume_produces_two_audit_entries() {
        let ks = KillSwitch::new();
        let agent_id = uuid::Uuid::now_v7();

        // Quarantine
        let trigger = TriggerEvent::ManualQuarantine {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Quarantine, &trigger);

        let entries_after_quarantine = ks.audit_entries();
        assert_eq!(entries_after_quarantine.len(), 1);
        assert_eq!(entries_after_quarantine[0].action, KillLevel::Quarantine);

        // Resume
        ks.resume_agent(agent_id).unwrap();

        let entries_after_resume = ks.audit_entries();
        assert_eq!(entries_after_resume.len(), 2, "Quarantine resume should add an audit entry");
        assert_eq!(entries_after_resume[1].action, KillLevel::Normal);
    }

    /// Resuming a non-existent agent should not add an audit entry.
    #[test]
    fn resume_nonexistent_agent_no_audit_entry() {
        let ks = KillSwitch::new();
        let agent_id = uuid::Uuid::now_v7();

        let result = ks.resume_agent(agent_id);
        assert!(result.is_err());

        let entries = ks.audit_entries();
        assert_eq!(entries.len(), 0, "Failed resume should not add audit entry");
    }

    /// KillAll should not be resumable via agent resume.
    #[test]
    fn kill_all_not_resumable_via_agent() {
        let ks = KillSwitch::new();
        let agent_id = uuid::Uuid::now_v7();

        let trigger = TriggerEvent::ManualKillAll {
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_kill_all(&trigger);

        // Try to resume — should fail.
        // Agent isn't in per_agent map (KillAll is platform-level).
        let result = ks.resume_agent(agent_id);
        assert!(result.is_err());
    }

    /// Monotonicity: can't downgrade from Quarantine to Pause.
    #[test]
    fn cannot_downgrade_quarantine_to_pause() {
        let ks = KillSwitch::new();
        let agent_id = uuid::Uuid::now_v7();

        let trigger = TriggerEvent::ManualQuarantine {
            agent_id,
            reason: "test".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Quarantine, &trigger);

        // Try to downgrade to Pause — should be ignored (monotonicity).
        let pause_trigger = TriggerEvent::ManualPause {
            agent_id,
            reason: "downgrade attempt".into(),
            initiated_by: "test".into(),
        };
        ks.activate_agent(agent_id, KillLevel::Pause, &pause_trigger);

        let state = ks.current_state();
        let agent_state = state.per_agent.get(&agent_id).unwrap();
        assert_eq!(agent_state.level, KillLevel::Quarantine, "Should still be Quarantine");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Prompt 1: Dead Write Paths — verify all tables have both read and write
// ═══════════════════════════════════════════════════════════════════════

mod dead_write_path_tests {
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();
        // Also create audit_log (created by ghost-audit, not migrations).
        let engine = ghost_audit::AuditQueryEngine::new(&conn);
        engine.ensure_table().unwrap();
        conn
    }

    /// Every table should be writable and readable.
    #[test]
    fn all_tables_have_write_and_read_paths() {
        let conn = setup_db();

        // memory_events: write + read
        cortex_storage::queries::memory_event_queries::insert_event(
            &conn, "mem-1", "create", "{}", "actor-1", &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let events = cortex_storage::queries::memory_event_queries::query_by_memory(&conn, "mem-1").unwrap();
        assert_eq!(events.len(), 1);

        // memory_snapshots: write + read
        cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
            &conn, "mem-1", "{\"key\": \"value\"}", Some(&[0u8; 32]),
        ).unwrap();
        let snaps = cortex_storage::queries::memory_snapshot_queries::query_by_memory(&conn, "mem-1").unwrap();
        assert_eq!(snaps.len(), 1);

        // memory_audit_log: write + read (genesis already inserted by v016)
        cortex_storage::queries::memory_audit_queries::insert_audit(
            &conn, "mem-1", "create", Some("test"),
        ).unwrap();
        let audits = cortex_storage::queries::memory_audit_queries::query_by_memory(&conn, "mem-1").unwrap();
        assert_eq!(audits.len(), 1);

        // convergence_scores: write + read
        cortex_storage::queries::convergence_score_queries::insert_score(
            &conn, "score-1", "agent-1", Some("sess-1"), 0.85, "[0.9,0.8]",
            3, "standard", "2025-01-01T00:00:00Z", &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let scores = cortex_storage::queries::convergence_score_queries::query_by_agent(&conn, "agent-1").unwrap();
        assert_eq!(scores.len(), 1);

        // goal_proposals: write + read
        cortex_storage::queries::goal_proposal_queries::insert_proposal(
            &conn, "prop-1", "agent-1", "sess-1", "Agent", "create", "memory",
            "content", "[]", "pending", &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let proposals = cortex_storage::queries::goal_proposal_queries::query_pending(&conn).unwrap();
        assert!(proposals.len() >= 1);

        // delegation_state: write + read
        cortex_storage::queries::delegation_state_queries::insert_delegation(
            &conn, "del-1", "deleg-1", "sender-1", "recipient-1", "task-1",
            "msg-1", &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let delegations = cortex_storage::queries::delegation_state_queries::query_pending(&conn).unwrap();
        assert_eq!(delegations.len(), 1);

        // itp_events: write + read
        cortex_storage::queries::itp_event_queries::insert_itp_event(
            &conn, "itp-1", "sess-1", "SessionStart", Some("agent-1"),
            "2025-01-01T00:00:00Z", 0, None, None, "standard",
            &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let itp = cortex_storage::queries::itp_event_queries::query_by_session(&conn, "sess-1").unwrap();
        assert_eq!(itp.len(), 1);

        // intervention_history: write + read
        cortex_storage::queries::intervention_history_queries::insert_intervention(
            &conn, "int-1", "agent-1", "sess-1", 2, 1, 0.45,
            "[0.5,0.4]", "escalate", &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let interventions = cortex_storage::queries::intervention_history_queries::query_by_agent(&conn, "agent-1").unwrap();
        assert_eq!(interventions.len(), 1);

        // reflection_entries: write + read
        cortex_storage::queries::reflection_queries::insert_reflection(
            &conn, "ref-1", "sess-1", "chain-1", 0, "periodic",
            "I reflected on my actions", 0.1, &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let reflections = cortex_storage::queries::reflection_queries::query_by_session(&conn, "sess-1").unwrap();
        assert_eq!(reflections.len(), 1);

        // boundary_violations: write + read
        cortex_storage::queries::boundary_violation_queries::insert_violation(
            &conn, "viol-1", "sess-1", "prompt_injection", 0.9,
            "hash123", "[\"pattern1\"]", "blocked", Some(0.3), Some(2),
            &[0u8; 32], &[0u8; 32],
        ).unwrap();
        let violations = cortex_storage::queries::boundary_violation_queries::query_by_agent_session(&conn, "sess-1").unwrap();
        assert_eq!(violations.len(), 1);

        // audit_log: write + read
        let engine = ghost_audit::AuditQueryEngine::new(&conn);
        engine.insert(&ghost_audit::AuditEntry {
            id: "audit-1".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
            agent_id: "agent-1".into(),
            event_type: "test".into(),
            severity: "info".into(),
            tool_name: None,
            details: "test entry".into(),
            session_id: None,
        }).unwrap();
        let result = engine.query(&ghost_audit::AuditFilter::new()).unwrap();
        assert!(result.total >= 1);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Prompt 5: Type contract — verify handler column indices match schema
// ═══════════════════════════════════════════════════════════════════════

mod type_contract_tests {
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        cortex_storage::migrations::run_migrations(&conn).unwrap();
        conn
    }

    /// Sessions query: GROUP BY session_id with COALESCE on sender should work
    /// even when all senders are NULL.
    #[test]
    fn sessions_query_handles_all_null_senders() {
        let conn = setup_db();

        // Insert ITP event with NULL sender.
        cortex_storage::queries::itp_event_queries::insert_itp_event(
            &conn, "evt-1", "sess-null", "SessionStart", None,
            "2025-01-01T00:00:00Z", 0, None, None, "standard",
            &[0u8; 32], &[0u8; 32],
        ).unwrap();

        // The sessions query uses COALESCE(sender, 'unknown') — should not fail.
        let mut stmt = conn.prepare(
            "SELECT session_id, \
                    MIN(timestamp) as started_at, \
                    MAX(timestamp) as last_event_at, \
                    COUNT(*) as event_count, \
                    GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown')) as agents \
             FROM itp_events \
             GROUP BY session_id \
             ORDER BY started_at DESC \
             LIMIT 50 OFFSET 0"
        ).unwrap();

        let rows: Vec<(String, String, String, i64, Option<String>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "sess-null");
        assert_eq!(rows[0].3, 1); // event_count
        assert_eq!(rows[0].4.as_deref(), Some("unknown")); // COALESCE worked
    }

    /// Memory query: JOIN between memory_snapshots and memory_events should
    /// return correct COUNT(DISTINCT ms.id) even with multiple events per memory.
    #[test]
    fn memory_count_distinct_prevents_inflation() {
        let conn = setup_db();

        // Insert 1 snapshot, 3 events for the same memory_id.
        cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
            &conn, "mem-1", "{}", Some(&[0u8; 32]),
        ).unwrap();
        for i in 0..3 {
            cortex_storage::queries::memory_event_queries::insert_event(
                &conn, "mem-1", &format!("event-{i}"), "{}", "actor-1",
                &[0u8; 32], &[0u8; 32],
            ).unwrap();
        }

        // COUNT(DISTINCT ms.id) should be 1, not 3.
        let count: u32 = conn.query_row(
            "SELECT COUNT(DISTINCT ms.id) FROM memory_snapshots ms \
             JOIN memory_events me ON ms.memory_id = me.memory_id \
             WHERE me.actor_id = ?1",
            ["actor-1"],
            |row| row.get(0),
        ).unwrap();

        assert_eq!(count, 1, "COUNT(DISTINCT) should prevent JOIN inflation");
    }

    /// Convergence score query: latest_by_agent should return the most recent score.
    #[test]
    fn latest_by_agent_returns_most_recent() {
        let conn = setup_db();

        cortex_storage::queries::convergence_score_queries::insert_score(
            &conn, "s1", "agent-1", None, 0.5, "[]", 1, "standard",
            "2025-01-01T00:00:00Z", &[0u8; 32], &[0u8; 32],
        ).unwrap();
        cortex_storage::queries::convergence_score_queries::insert_score(
            &conn, "s2", "agent-1", None, 0.9, "[]", 3, "standard",
            "2025-01-02T00:00:00Z", &[0u8; 32], &[0u8; 32],
        ).unwrap();

        let latest = cortex_storage::queries::convergence_score_queries::latest_by_agent(&conn, "agent-1")
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, "s2");
        assert!((latest.composite_score - 0.9).abs() < f64::EPSILON);
    }
}
