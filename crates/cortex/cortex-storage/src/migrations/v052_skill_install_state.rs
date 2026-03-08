//! Migration v052: canonical skill install-state table.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS skill_install_state (
            skill_name   TEXT PRIMARY KEY,
            state        TEXT NOT NULL CHECK (state IN ('installed', 'disabled')),
            updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
            updated_by   TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_skill_install_state_state
            ON skill_install_state(state);

        INSERT OR IGNORE INTO skill_install_state (skill_name, state, updated_at, updated_by)
        SELECT
            skill_name,
            'installed',
            COALESCE(installed_at, datetime('now')),
            installed_by
        FROM installed_skills;",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_forward_legacy_installed_skills() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::migrations::v027_installed_skills::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO installed_skills (
                id, skill_name, version, description, capabilities, source, state, installed_by
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "legacy-row",
                "note_take",
                "0.1.0",
                "legacy note skill",
                "[]",
                "bundled",
                "active",
                "tester",
            ],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let row: (String, String, Option<String>) = conn
            .query_row(
                "SELECT skill_name, state, updated_by
                 FROM skill_install_state
                 WHERE skill_name = 'note_take'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(row.0, "note_take");
        assert_eq!(row.1, "installed");
        assert_eq!(row.2.as_deref(), Some("tester"));
    }
}
