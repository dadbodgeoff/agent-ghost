//! Embedding storage queries (v032 memory_embeddings table).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

/// Store an embedding for a memory.
///
/// Embeddings are stored as f32 little-endian byte blobs.
pub fn upsert_embedding(
    conn: &Connection,
    memory_id: &str,
    embedding: &[f32],
    provider: &str,
) -> CortexResult<()> {
    let bytes = embedding_to_bytes(embedding);
    let dims = embedding.len() as i64;

    conn.execute(
        "INSERT OR REPLACE INTO memory_embeddings (memory_id, embedding, dimensions, provider)
         VALUES (?1, ?2, ?3, ?4)",
        params![memory_id, bytes, dims, provider],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Retrieve an embedding for a memory.
pub fn get_embedding(conn: &Connection, memory_id: &str) -> CortexResult<Option<Vec<f32>>> {
    let result = conn.query_row(
        "SELECT embedding FROM memory_embeddings WHERE memory_id = ?1",
        params![memory_id],
        |row| {
            let bytes: Vec<u8> = row.get(0)?;
            Ok(bytes_to_embedding(&bytes))
        },
    );

    match result {
        Ok(embedding) => Ok(Some(embedding)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(to_storage_err(e.to_string())),
    }
}

/// Retrieve embeddings for multiple memory_ids.
pub fn get_embeddings_batch(
    conn: &Connection,
    memory_ids: &[&str],
) -> CortexResult<Vec<(String, Vec<f32>)>> {
    if memory_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build a parameterized IN clause.
    let placeholders: Vec<String> = (1..=memory_ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT memory_id, embedding FROM memory_embeddings WHERE memory_id IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| to_storage_err(e.to_string()))?;

    let params: Vec<&dyn rusqlite::types::ToSql> = memory_ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt
        .query_map(params.as_slice(), |row| {
            let id: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            Ok((id, bytes_to_embedding(&bytes)))
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// Check if the memory_embeddings table exists.
pub fn embeddings_available(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memory_embeddings'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

/// Count the number of stored embeddings.
pub fn embedding_count(conn: &Connection) -> CortexResult<i64> {
    conn.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |row| {
        row.get(0)
    })
    .map_err(|e| to_storage_err(e.to_string()))
}

/// Convert f32 slice to little-endian bytes.
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Convert little-endian bytes back to f32 vec.
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bytes() {
        let original = vec![1.0f32, -2.5, 3.14, 0.0, f32::EPSILON];
        let bytes = embedding_to_bytes(&original);
        let restored = bytes_to_embedding(&bytes);
        assert_eq!(original, restored);
    }

    #[test]
    fn empty_embedding_roundtrip() {
        let original: Vec<f32> = vec![];
        let bytes = embedding_to_bytes(&original);
        let restored = bytes_to_embedding(&bytes);
        assert_eq!(original, restored);
    }
}
