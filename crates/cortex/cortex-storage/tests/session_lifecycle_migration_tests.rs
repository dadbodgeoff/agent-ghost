use cortex_storage::migrations::{
    v037_studio_chat_tables, v043_session_lifecycle, v044_studio_session_agent_id,
};
use cortex_storage::queries::studio_chat_queries;
use cortex_storage::sqlite::apply_writer_pragmas;
use rusqlite::{params, Connection};

fn open_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    apply_writer_pragmas(&conn).unwrap();
    conn
}

#[test]
fn v043_session_lifecycle_backfills_existing_sessions_on_sqlite() {
    let conn = open_db();
    v037_studio_chat_tables::migrate(&conn).unwrap();

    conn.execute(
        "INSERT INTO studio_chat_sessions (
            id, title, model, system_prompt, temperature, max_tokens, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            "session-1",
            "Session 1",
            "qwen3.5:9b",
            "",
            0.5_f64,
            4096_i64,
            "2026-03-01 10:00:00",
            "2026-03-01 11:00:00",
        ],
    )
    .unwrap();

    v043_session_lifecycle::migrate(&conn).unwrap();

    let (last_activity_at, deleted_at): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT last_activity_at, deleted_at
             FROM studio_chat_sessions
             WHERE id = ?1",
            params!["session-1"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(last_activity_at.as_deref(), Some("2026-03-01 11:00:00"));
    assert_eq!(deleted_at, None);
}

#[test]
fn create_session_sets_last_activity_at_after_lifecycle_migration() {
    let conn = open_db();
    v037_studio_chat_tables::migrate(&conn).unwrap();
    v043_session_lifecycle::migrate(&conn).unwrap();
    v044_studio_session_agent_id::migrate(&conn).unwrap();

    studio_chat_queries::create_session(
        &conn,
        "session-2",
        "agent-1",
        "Session 2",
        "qwen3.5:9b",
        "",
        0.5,
        4096,
    )
    .unwrap();

    let (last_activity_at, deleted_at): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT last_activity_at, deleted_at
             FROM studio_chat_sessions
             WHERE id = ?1",
            params!["session-2"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert!(last_activity_at.is_some());
    assert_eq!(deleted_at, None);
}
