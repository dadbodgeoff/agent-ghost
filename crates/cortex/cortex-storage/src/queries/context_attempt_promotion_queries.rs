//! Query helpers for speculative context promotion link records.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct NewContextAttemptPromotion<'a> {
    pub id: &'a str,
    pub attempt_id: &'a str,
    pub promoted_memory_id: &'a str,
    pub promotion_type: &'a str,
}

#[derive(Debug, Clone)]
pub struct ContextAttemptPromotionRow {
    pub id: String,
    pub attempt_id: String,
    pub promoted_memory_id: String,
    pub promotion_type: String,
    pub created_at: String,
}

pub fn insert_promotion(
    conn: &Connection,
    record: &NewContextAttemptPromotion<'_>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO context_attempt_promotion (
            id, attempt_id, promoted_memory_id, promotion_type
        ) VALUES (?1, ?2, ?3, ?4)",
        params![
            record.id,
            record.attempt_id,
            record.promoted_memory_id,
            record.promotion_type,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn latest_for_attempt(
    conn: &Connection,
    attempt_id: &str,
) -> CortexResult<Option<ContextAttemptPromotionRow>> {
    conn.query_row(
        "SELECT id, attempt_id, promoted_memory_id, promotion_type, created_at
         FROM context_attempt_promotion
         WHERE attempt_id = ?1
         ORDER BY created_at DESC
         LIMIT 1",
        params![attempt_id],
        |row| {
            Ok(ContextAttemptPromotionRow {
                id: row.get(0)?,
                attempt_id: row.get(1)?,
                promoted_memory_id: row.get(2)?,
                promotion_type: row.get(3)?,
                created_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}
