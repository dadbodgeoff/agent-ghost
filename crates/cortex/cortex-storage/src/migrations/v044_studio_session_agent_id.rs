//! Migration v044: persist durable agent identity on studio sessions.

use rusqlite::Connection;
use uuid::Uuid;

use cortex_core::models::error::CortexResult;

use crate::to_storage_err;

const STUDIO_AGENT_NAMESPACE: Uuid = Uuid::from_u128(0x6ba7b814_9dad_11d1_80b4_00c04fd430c8);

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "ALTER TABLE studio_chat_sessions
             ADD COLUMN agent_id TEXT;

         CREATE INDEX IF NOT EXISTS idx_studio_sessions_agent_id
             ON studio_chat_sessions(agent_id);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    let session_ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM studio_chat_sessions WHERE agent_id IS NULL OR agent_id = ''")
            .map_err(|e| to_storage_err(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| to_storage_err(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| to_storage_err(e.to_string()))?;
        rows
    };

    for session_id in session_ids {
        let agent_id = Uuid::new_v5(
            &STUDIO_AGENT_NAMESPACE,
            format!("studio-session:{session_id}").as_bytes(),
        );
        conn.execute(
            "UPDATE studio_chat_sessions SET agent_id = ?2 WHERE id = ?1",
            rusqlite::params![session_id, agent_id.to_string()],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    }

    Ok(())
}
