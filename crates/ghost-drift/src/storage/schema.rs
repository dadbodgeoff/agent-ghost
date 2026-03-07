//! SQLite storage for the Drift MCP server.
//!
//! Per-workspace database at `.ghost/drift/drift.db`.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// Wrapper around a SQLite connection with drift-specific operations.
pub struct DriftDb {
    conn: Mutex<Connection>,
}

impl DriftDb {
    /// Open (or create) the drift database at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS drift_files (
                path        TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                last_modified TEXT NOT NULL,
                size_bytes  INTEGER NOT NULL,
                symbol_count INTEGER NOT NULL DEFAULT 0,
                indexed_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS drift_symbols (
                id          TEXT PRIMARY KEY,
                file_path   TEXT NOT NULL REFERENCES drift_files(path) ON DELETE CASCADE,
                name        TEXT NOT NULL,
                kind        TEXT NOT NULL,
                line_start  INTEGER NOT NULL,
                line_end    INTEGER,
                signature   TEXT,
                embedding   BLOB,
                UNIQUE(file_path, name, kind)
            );

            CREATE INDEX IF NOT EXISTS idx_symbols_file ON drift_symbols(file_path);
            CREATE INDEX IF NOT EXISTS idx_symbols_kind ON drift_symbols(kind);

            CREATE TABLE IF NOT EXISTS drift_beliefs (
                id          TEXT PRIMARY KEY,
                file_path   TEXT NOT NULL,
                symbol_name TEXT,
                belief      TEXT NOT NULL,
                confidence  REAL NOT NULL DEFAULT 1.0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                verified_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_beliefs_file ON drift_beliefs(file_path);
            CREATE INDEX IF NOT EXISTS idx_beliefs_symbol ON drift_beliefs(symbol_name);

            CREATE TABLE IF NOT EXISTS drift_belief_changes (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                belief_id   TEXT NOT NULL,
                change_type TEXT NOT NULL,
                old_value   TEXT,
                new_value   TEXT,
                changed_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS drift_snapshots (
                id          TEXT PRIMARY KEY,
                created_at  TEXT NOT NULL,
                file_count  INTEGER NOT NULL,
                symbol_count INTEGER NOT NULL,
                belief_count INTEGER NOT NULL,
                ksi         REAL,
                freshness   REAL,
                contradiction_density REAL,
                data_json   TEXT
            );
            ",
        )?;
        Ok(())
    }

    /// Execute a closure within a SQLite transaction.
    /// If the closure returns Ok, the transaction is committed.
    /// If it returns Err, the transaction is rolled back.
    pub fn with_transaction<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        conn.execute("BEGIN IMMEDIATE", [])?;
        match f(&conn) {
            Ok(val) => {
                conn.execute("COMMIT", [])?;
                Ok(val)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    // ── Atomic file indexing ──

    /// Index a single file atomically: upsert file record, delete old symbols,
    /// insert new symbols, update symbol count — all in one transaction.
    /// Returns true if the file was indexed (false if hash unchanged).
    pub fn index_file_atomic(
        &self,
        rel_path: &str,
        content_hash: &str,
        last_modified: &str,
        size_bytes: i64,
        symbols: &[(
            String,
            String,
            String,
            i64,
            Option<i64>,
            Option<String>,
            Option<Vec<u8>>,
        )],
        // Each tuple: (id, name, kind, line_start, line_end, signature, embedding_bytes)
    ) -> anyhow::Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;

        // Check if file changed since last index
        let existing_hash: Option<String> = conn
            .query_row(
                "SELECT content_hash FROM drift_files WHERE path = ?1",
                params![rel_path],
                |row| row.get(0),
            )
            .optional()?;

        if existing_hash.as_deref() == Some(content_hash) {
            return Ok(false);
        }

        conn.execute("BEGIN IMMEDIATE", [])?;
        let result = (|| -> anyhow::Result<()> {
            conn.execute(
                "INSERT INTO drift_files (path, content_hash, last_modified, size_bytes, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(path) DO UPDATE SET
                    content_hash = excluded.content_hash,
                    last_modified = excluded.last_modified,
                    size_bytes = excluded.size_bytes,
                    indexed_at = excluded.indexed_at",
                params![rel_path, content_hash, last_modified, size_bytes],
            )?;

            conn.execute(
                "DELETE FROM drift_symbols WHERE file_path = ?1",
                params![rel_path],
            )?;

            for (id, name, kind, line_start, line_end, signature, embedding) in symbols {
                conn.execute(
                    "INSERT INTO drift_symbols (id, file_path, name, kind, line_start, line_end, signature, embedding)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(file_path, name, kind) DO UPDATE SET
                        line_start = excluded.line_start,
                        line_end = excluded.line_end,
                        signature = excluded.signature,
                        embedding = excluded.embedding",
                    params![id, rel_path, name, kind, line_start, line_end, signature, embedding.as_deref()],
                )?;
            }

            conn.execute(
                "UPDATE drift_files SET symbol_count = ?1 WHERE path = ?2",
                params![symbols.len() as i64, rel_path],
            )?;

            Ok(())
        })();

        match &result {
            Ok(()) => {
                conn.execute("COMMIT", [])?;
            }
            Err(_) => {
                let _ = conn.execute("ROLLBACK", []);
            }
        }

        result?;
        Ok(true)
    }

    // ── File operations ──

    pub fn upsert_file(
        &self,
        path: &str,
        content_hash: &str,
        last_modified: &str,
        size_bytes: i64,
    ) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO drift_files (path, content_hash, last_modified, size_bytes, indexed_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(path) DO UPDATE SET
                content_hash = excluded.content_hash,
                last_modified = excluded.last_modified,
                size_bytes = excluded.size_bytes,
                indexed_at = excluded.indexed_at",
            params![path, content_hash, last_modified, size_bytes],
        )?;
        Ok(())
    }

    pub fn update_file_symbol_count(&self, path: &str, count: i64) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        conn.execute(
            "UPDATE drift_files SET symbol_count = ?1 WHERE path = ?2",
            params![count, path],
        )?;
        Ok(())
    }

    pub fn get_file_hash(&self, path: &str) -> anyhow::Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let hash = conn
            .query_row(
                "SELECT content_hash FROM drift_files WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(hash)
    }

    pub fn file_count(&self) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM drift_files", [], |row| row.get(0))?;
        Ok(count)
    }

    // ── Symbol operations ──

    pub fn upsert_symbol(
        &self,
        id: &str,
        file_path: &str,
        name: &str,
        kind: &str,
        line_start: i64,
        line_end: Option<i64>,
        signature: Option<&str>,
        embedding: Option<&[u8]>,
    ) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        conn.execute(
            "INSERT INTO drift_symbols (id, file_path, name, kind, line_start, line_end, signature, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(file_path, name, kind) DO UPDATE SET
                line_start = excluded.line_start,
                line_end = excluded.line_end,
                signature = excluded.signature,
                embedding = excluded.embedding",
            params![id, file_path, name, kind, line_start, line_end, signature, embedding],
        )?;
        Ok(())
    }

    pub fn delete_symbols_for_file(&self, file_path: &str) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        conn.execute(
            "DELETE FROM drift_symbols WHERE file_path = ?1",
            params![file_path],
        )?;
        Ok(())
    }

    pub fn symbol_count(&self) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM drift_symbols", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Query symbols with optional file filter, returning (file_path, name, kind, line_start, signature).
    pub fn query_symbols(
        &self,
        file_filter: Option<&str>,
        limit: u32,
    ) -> anyhow::Result<Vec<(String, String, String, i64, Option<String>)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let mut results = Vec::new();

        if let Some(filter) = file_filter {
            let mut stmt = conn.prepare(
                "SELECT file_path, name, kind, line_start, signature
                 FROM drift_symbols WHERE file_path LIKE ?1 ORDER BY file_path, line_start LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![format!("%{filter}%"), limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?;
            for row in rows {
                results.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT file_path, name, kind, line_start, signature
                 FROM drift_symbols ORDER BY file_path, line_start LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?;
            for row in rows {
                results.push(row?);
            }
        }

        Ok(results)
    }

    /// Get all symbols with their embeddings for similarity search.
    pub fn symbols_with_embeddings(
        &self,
        file_filter: Option<&str>,
    ) -> anyhow::Result<Vec<(String, String, String, i64, Option<String>, Vec<u8>)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let mut results = Vec::new();

        let row_mapper = |row: &rusqlite::Row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        };

        if let Some(filter) = file_filter {
            let mut stmt = conn.prepare(
                "SELECT file_path, name, kind, line_start, signature, embedding
                 FROM drift_symbols WHERE embedding IS NOT NULL AND file_path LIKE ?1",
            )?;
            let rows = stmt.query_map(params![format!("%{filter}%")], row_mapper)?;
            for row in rows {
                results.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT file_path, name, kind, line_start, signature, embedding
                 FROM drift_symbols WHERE embedding IS NOT NULL",
            )?;
            let rows = stmt.query_map([], row_mapper)?;
            for row in rows {
                results.push(row?);
            }
        }

        Ok(results)
    }

    // ── Belief operations ──

    pub fn insert_belief(
        &self,
        id: &str,
        file_path: &str,
        symbol_name: Option<&str>,
        belief: &str,
        confidence: f64,
    ) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO drift_beliefs (id, file_path, symbol_name, belief, confidence, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, file_path, symbol_name, belief, confidence, now],
        )?;
        conn.execute(
            "INSERT INTO drift_belief_changes (belief_id, change_type, new_value, changed_at)
             VALUES (?1, 'created', ?2, ?3)",
            params![id, belief, now],
        )?;
        Ok(())
    }

    pub fn get_beliefs_for_file(
        &self,
        file_path: &str,
    ) -> anyhow::Result<Vec<(String, Option<String>, String, f64, String)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT id, symbol_name, belief, confidence, updated_at
             FROM drift_beliefs WHERE file_path = ?1 ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![file_path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn get_all_beliefs(
        &self,
    ) -> anyhow::Result<Vec<(String, String, Option<String>, String, f64, String)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT id, file_path, symbol_name, belief, confidence, updated_at
             FROM drift_beliefs ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn belief_count(&self) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM drift_beliefs", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_stale_beliefs(
        &self,
        max_freshness_days: f64,
        limit: u32,
    ) -> anyhow::Result<Vec<(String, String, Option<String>, String, f64, String)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT id, file_path, symbol_name, belief, confidence, updated_at
             FROM drift_beliefs
             WHERE julianday('now') - julianday(COALESCE(verified_at, updated_at)) > ?1
             ORDER BY julianday('now') - julianday(COALESCE(verified_at, updated_at)) DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![max_freshness_days, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ── Snapshot operations ──

    pub fn insert_snapshot(
        &self,
        id: &str,
        file_count: i64,
        symbol_count: i64,
        belief_count: i64,
        ksi: Option<f64>,
        freshness: Option<f64>,
        contradiction_density: Option<f64>,
        data_json: Option<&str>,
    ) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO drift_snapshots (id, created_at, file_count, symbol_count, belief_count, ksi, freshness, contradiction_density, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, now, file_count, symbol_count, belief_count, ksi, freshness, contradiction_density, data_json],
        )?;
        Ok(())
    }

    pub fn get_snapshot(
        &self,
        id: &str,
    ) -> anyhow::Result<
        Option<(
            String,
            String,
            i64,
            i64,
            i64,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<String>,
        )>,
    > {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let result = conn
            .query_row(
                "SELECT id, created_at, file_count, symbol_count, belief_count, ksi, freshness, contradiction_density, data_json
                 FROM drift_snapshots WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<f64>>(5)?,
                        row.get::<_, Option<f64>>(6)?,
                        row.get::<_, Option<f64>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;
        Ok(result)
    }

    pub fn get_latest_snapshot(
        &self,
    ) -> anyhow::Result<
        Option<(
            String,
            String,
            i64,
            i64,
            i64,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<String>,
        )>,
    > {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let result = conn
            .query_row(
                "SELECT id, created_at, file_count, symbol_count, belief_count, ksi, freshness, contradiction_density, data_json
                 FROM drift_snapshots ORDER BY created_at DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<f64>>(5)?,
                        row.get::<_, Option<f64>>(6)?,
                        row.get::<_, Option<f64>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;
        Ok(result)
    }

    // ── Metrics helpers ──

    /// Count beliefs with contradictory content for the same file/symbol.
    pub fn contradiction_count(&self) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        // Count pairs of beliefs on the same file+symbol with low similarity
        // (simplified: count beliefs sharing same file+symbol as potential contradictions)
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM (
                SELECT file_path, symbol_name FROM drift_beliefs
                WHERE symbol_name IS NOT NULL
                GROUP BY file_path, symbol_name
                HAVING COUNT(*) > 1
             )",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Count stale beliefs (not verified within threshold days) — O(1) via COUNT.
    pub fn stale_belief_count(&self, threshold_days: f64) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM drift_beliefs
             WHERE julianday('now') - julianday(COALESCE(verified_at, updated_at)) > ?1",
            params![threshold_days],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Count belief changes in a time window.
    pub fn belief_changes_in_window(&self, days: f64) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM drift_belief_changes
             WHERE julianday('now') - julianday(changed_at) <= ?1",
            params![days],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get beliefs created in a time window (for pattern detection).
    pub fn beliefs_created_in_window(&self, days: f64) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM drift_belief_changes
             WHERE change_type = 'created'
             AND julianday('now') - julianday(changed_at) <= ?1",
            params![days],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get beliefs with declining confidence (2+ revisions with lower confidence).
    pub fn eroding_belief_count(&self) -> anyhow::Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT belief_id) FROM drift_belief_changes
             WHERE change_type = 'confidence_decreased'",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}
