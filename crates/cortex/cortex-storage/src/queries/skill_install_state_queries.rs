//! Canonical install-state queries for compiled skills.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillInstallState {
    Installed,
    Disabled,
}

impl SkillInstallState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Disabled => "disabled",
        }
    }

    fn from_db(value: &str) -> CortexResult<Self> {
        match value {
            "installed" => Ok(Self::Installed),
            "disabled" => Ok(Self::Disabled),
            other => Err(to_storage_err(format!(
                "unknown skill_install_state value '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInstallStateRow {
    pub skill_name: String,
    pub state: SkillInstallState,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

pub fn list_skill_install_states(conn: &Connection) -> CortexResult<Vec<SkillInstallStateRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT skill_name, state, updated_at, updated_by
             FROM skill_install_state
             ORDER BY skill_name ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map([], map_state_row)
        .map_err(|e| to_storage_err(e.to_string()))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))
}

pub fn get_skill_install_state(
    conn: &Connection,
    skill_name: &str,
) -> CortexResult<Option<SkillInstallStateRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT skill_name, state, updated_at, updated_by
             FROM skill_install_state
             WHERE skill_name = ?1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![skill_name], map_state_row)
        .map_err(|e| to_storage_err(e.to_string()))?;

    match rows.next() {
        Some(row) => row.map(Some).map_err(|e| to_storage_err(e.to_string())),
        None => Ok(None),
    }
}

pub fn seed_skill_install_state(
    conn: &Connection,
    skill_name: &str,
    state: SkillInstallState,
) -> CortexResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO skill_install_state (skill_name, state)
         VALUES (?1, ?2)",
        params![skill_name, state.as_str()],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn upsert_skill_install_state(
    conn: &Connection,
    skill_name: &str,
    state: SkillInstallState,
    updated_by: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO skill_install_state (skill_name, state, updated_by)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(skill_name) DO UPDATE SET
             state = excluded.state,
             updated_at = datetime('now'),
             updated_by = excluded.updated_by",
        params![skill_name, state.as_str(), updated_by],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

fn map_state_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillInstallStateRow> {
    let state_raw: String = row.get(1)?;
    let state = SkillInstallState::from_db(&state_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        )
    })?;

    Ok(SkillInstallStateRow {
        skill_name: row.get(0)?,
        state,
        updated_at: row.get(2)?,
        updated_by: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::migrations::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn seed_only_inserts_when_missing() {
        let conn = test_db();

        seed_skill_install_state(&conn, "note_take", SkillInstallState::Installed).unwrap();
        seed_skill_install_state(&conn, "note_take", SkillInstallState::Disabled).unwrap();

        let row = get_skill_install_state(&conn, "note_take")
            .unwrap()
            .unwrap();
        assert_eq!(row.state, SkillInstallState::Installed);
    }

    #[test]
    fn upsert_replaces_existing_state_and_actor() {
        let conn = test_db();

        seed_skill_install_state(&conn, "note_take", SkillInstallState::Installed).unwrap();
        upsert_skill_install_state(
            &conn,
            "note_take",
            SkillInstallState::Disabled,
            Some("operator:test"),
        )
        .unwrap();

        let row = get_skill_install_state(&conn, "note_take")
            .unwrap()
            .unwrap();
        assert_eq!(row.state, SkillInstallState::Disabled);
        assert_eq!(row.updated_by.as_deref(), Some("operator:test"));
    }
}
