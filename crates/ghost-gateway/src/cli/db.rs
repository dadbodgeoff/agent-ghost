//! ghost db — database management commands (T-1.7.1, T-1.7.2, T-2.5.1, T-2.5.2).

use cortex_storage::migrations::{current_version, run_migrations_with_backup, LATEST_VERSION};
use cortex_storage::schema_contract::require_schema_ready;
use serde::Serialize;

use super::backend::{BackendRequirement, CliBackend};
use super::confirm::confirm;
use super::error::CliError;
use super::http_client::GhostHttpClient;
use super::output::{print_output, OutputFormat, TableDisplay};

// ─── ghost db migrate (T-1.7.1) ───────────────────────────────────────────────

pub struct DbMigrateArgs {}

/// Run `ghost db migrate`.
pub async fn run_migrate(_args: DbMigrateArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(BackendRequirement::DirectOnly)?;
    let db = backend.db();
    let conn = db.write().await;

    let before = current_version(&conn).map_err(|e| CliError::Database(e.to_string()))?;
    run_migrations_with_backup(&conn, Some(db.db_path()))
        .map_err(|e| CliError::Database(format!("migration failed: {e}")))?;
    let after = current_version(&conn).map_err(|e| CliError::Database(e.to_string()))?;

    if before == after {
        println!("Already up to date (v{after}).");
    } else {
        let n = after - before;
        println!("Applied {n} migration(s) (v{before} → v{after}).");
    }
    Ok(())
}

// ─── ghost db status (T-1.7.2) ───────────────────────────────────────────────

pub struct DbStatusArgs {
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct DbStatusResult {
    current_version: u32,
    latest_version: u32,
    up_to_date: bool,
    journal_mode: String,
    db_size_bytes: u64,
    wal_size_bytes: u64,
    table_counts: std::collections::HashMap<String, i64>,
}

impl TableDisplay for DbStatusResult {
    fn print_table(&self) {
        let status = if self.up_to_date {
            "✓ up to date"
        } else {
            "✗ migrations pending"
        };
        println!(
            "Schema version : v{} / v{} — {}",
            self.current_version, self.latest_version, status
        );
        println!("Journal mode   : {}", self.journal_mode);
        println!(
            "DB size        : {:.1} KB",
            self.db_size_bytes as f64 / 1024.0
        );
        if self.wal_size_bytes > 0 {
            println!(
                "WAL size       : {:.1} KB",
                self.wal_size_bytes as f64 / 1024.0
            );
        }
        println!();
        println!("Table row counts:");
        let mut counts: Vec<_> = self.table_counts.iter().collect();
        counts.sort_by_key(|(k, _)| k.as_str());
        for (table, count) in counts {
            println!("  {:<30}  {:>8}", table, count);
        }
    }
}

/// Run `ghost db status`.
pub fn run_status(args: DbStatusArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(BackendRequirement::DirectOnly)?;
    let db = backend.db();
    let conn = db.read().map_err(|e| CliError::Database(e.to_string()))?;
    require_schema_ready(&conn).map_err(|e| CliError::Database(e.to_string()))?;

    let version = current_version(&conn).map_err(|e| CliError::Database(e.to_string()))?;

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap_or_else(|_| "unknown".into());

    let db_path = get_db_path(&conn);
    let db_size = db_path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0);

    let wal_size = db_path
        .as_ref()
        .and_then(|p| {
            let wal = format!("{}-wal", p);
            std::fs::metadata(&wal).ok().map(|m| m.len())
        })
        .unwrap_or(0);

    let tables = [
        "itp_events",
        "audit_log",
        "convergence_scores",
        "schema_version",
    ];
    let mut table_counts = std::collections::HashMap::new();
    for table in tables {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(0);
        table_counts.insert(table.to_string(), count);
    }

    let result = DbStatusResult {
        current_version: version,
        latest_version: LATEST_VERSION,
        up_to_date: version == LATEST_VERSION,
        journal_mode,
        db_size_bytes: db_size,
        wal_size_bytes: wal_size,
        table_counts,
    };

    print_output(&result, args.output);
    Ok(())
}

// ─── ghost db verify (T-2.5.1) ───────────────────────────────────────────────

pub struct DbVerifyArgs {
    /// Walk entire chain instead of spot-checking 100 random entries.
    pub full: bool,
}

/// Run `ghost db verify`.
pub fn run_verify(args: DbVerifyArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(BackendRequirement::DirectOnly)?;
    let db = backend.db();
    let conn = db.read().map_err(|e| CliError::Database(e.to_string()))?;
    require_schema_ready(&conn).map_err(|e| CliError::Database(e.to_string()))?;

    let start = std::time::Instant::now();

    let sql = if args.full {
        "SELECT hex(event_hash), hex(previous_hash), content_hash \
         FROM itp_events ORDER BY session_id, sequence_number ASC"
            .to_string()
    } else {
        "SELECT hex(event_hash), hex(previous_hash), content_hash \
         FROM itp_events ORDER BY RANDOM() LIMIT 100"
            .to_string()
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| CliError::Database(e.to_string()))?;

    struct EventRow {
        event_hash: String,
        previous_hash: String,
        content_hash: Option<String>,
    }

    let rows: Result<Vec<EventRow>, _> = stmt
        .query_map([], |row| {
            Ok(EventRow {
                event_hash: row.get(0)?,
                previous_hash: row.get(1)?,
                content_hash: row.get(2)?,
            })
        })
        .map_err(|e| CliError::Database(e.to_string()))?
        .collect();

    let rows = rows.map_err(|e| CliError::Database(e.to_string()))?;
    let chain_len = rows.len();
    let mut breaks = 0usize;

    for row in &rows {
        let expected = compute_event_hash(
            row.content_hash.as_deref().unwrap_or(""),
            &row.previous_hash,
        );
        if expected.to_ascii_uppercase() != row.event_hash.to_ascii_uppercase() {
            breaks += 1;
        }
    }

    let elapsed = start.elapsed();
    let mode = if args.full {
        "full"
    } else {
        "spot-check (100)"
    };

    println!(
        "Hash chain verification ({mode}): {} events checked in {:.1}s",
        chain_len,
        elapsed.as_secs_f64()
    );

    if breaks == 0 {
        println!("✓ No breaks found — chain is intact.");
    } else {
        eprintln!("✗ {breaks} break(s) detected in the hash chain.");
        return Err(CliError::Internal(format!(
            "{breaks} hash chain break(s) detected"
        )));
    }

    Ok(())
}

/// Compute the expected event_hash from content_hash and previous_hash.
///
/// Mirrors the agent loop: event_hash = blake3(content_hash_bytes || previous_hash_bytes)
fn compute_event_hash(content_hash_hex: &str, previous_hash_hex: &str) -> String {
    let content = hex_decode(content_hash_hex);
    let previous = hex_decode(previous_hash_hex);
    let mut hasher = blake3::Hasher::new();
    hasher.update(&content);
    hasher.update(&previous);
    hex_encode(hasher.finalize().as_bytes())
}

fn hex_decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

// ─── ghost db compact (T-2.5.2) ──────────────────────────────────────────────

pub struct DbCompactArgs {
    pub yes: bool,
    pub dry_run: bool,
    /// Skip gateway health probe (dangerous).
    pub force: bool,
    pub gateway_url: String,
    /// Only run SQLite VACUUM, skip memory event compaction.
    pub vacuum_only: bool,
}

/// Run `ghost db compact`.
pub async fn run_compact(args: DbCompactArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(BackendRequirement::DirectOnly)?;

    if args.dry_run {
        let db = backend.db();
        let conn = db.read().map_err(|e| CliError::Database(e.to_string()))?;
        let db_path = get_db_path(&conn);
        let wal_size = db_path
            .as_ref()
            .and_then(|p| {
                let wal = format!("{}-wal", p);
                std::fs::metadata(&wal).ok().map(|m| m.len())
            })
            .unwrap_or(0);
        println!(
            "[dry-run] WAL size: {:.1} KB. VACUUM + checkpoint would reclaim space.",
            wal_size as f64 / 1024.0
        );

        // Show memory compaction candidates.
        if !args.vacuum_only {
            let candidates =
                cortex_storage::queries::compaction_queries::memories_above_threshold(&conn, 50)
                    .unwrap_or_default();
            if candidates.is_empty() {
                println!("[dry-run] No memories with >50 uncompacted events.");
            } else {
                println!(
                    "[dry-run] {} memories eligible for compaction:",
                    candidates.len()
                );
                for (memory_id, count) in &candidates {
                    println!("  {memory_id}: {count} uncompacted events");
                }
            }
        }
        return Ok(());
    }

    // Pre-flight: abort if gateway is reachable (R19).
    if !args.force {
        if GhostHttpClient::health_check(&args.gateway_url).await {
            eprintln!(
                "Gateway is running. VACUUM requires exclusive DB access and will conflict \
                 with active connections. Stop the gateway first, then compact. \
                 Pass --force to skip this check at your own risk."
            );
            return Err(CliError::GatewayRequired);
        }
    } else {
        eprintln!("⚠  Skipping gateway check. Ensure no other process has the DB open.");
    }

    if !confirm("Compact database? This may take a moment.", args.yes) {
        return Err(CliError::Cancelled);
    }

    let db = backend.db();
    let conn = db.write().await;

    // Memory event compaction (summarize old events into snapshots).
    if !args.vacuum_only {
        match run_memory_compaction(&conn) {
            Ok((memories, events)) => {
                if memories > 0 {
                    println!(
                        "  Memory compaction: {} memories, {} events summarized.",
                        memories, events
                    );
                } else {
                    println!("  Memory compaction: no memories above threshold.");
                }
            }
            Err(e) => {
                eprintln!("  Memory compaction failed (non-fatal): {e}");
            }
        }
    }

    // SQLite VACUUM.
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
        .map_err(|e| CliError::Database(format!("compact failed: {e}")))?;

    println!("✓ Database compacted.");
    Ok(())
}

/// Run memory event compaction. Returns (memories_processed, events_compacted).
fn run_memory_compaction(conn: &rusqlite::Connection) -> Result<(i64, i64), CliError> {
    use cortex_storage::queries::{compaction_queries, memory_snapshot_queries};

    let candidates = compaction_queries::memories_above_threshold(conn, 50)
        .map_err(|e| CliError::Database(e.to_string()))?;

    if candidates.is_empty() {
        return Ok((0, 0));
    }

    let run_id = compaction_queries::insert_compaction_run(conn)
        .map_err(|e| CliError::Database(e.to_string()))?;

    let mut total_memories = 0i64;
    let mut total_events = 0i64;

    for (memory_id, _count) in &candidates {
        let events = compaction_queries::uncompacted_events(conn, memory_id)
            .map_err(|e| CliError::Database(e.to_string()))?;

        if events.is_empty() {
            continue;
        }

        let min_id = events.first().map(|e| e.event_id).unwrap_or(0);
        let max_id = events.last().map(|e| e.event_id).unwrap_or(0);

        // Build a summary from the event deltas.
        let summary = build_compaction_summary(memory_id, &events);
        let state_hash = blake3::hash(summary.as_bytes());

        // Insert summary snapshot (append-only safe).
        memory_snapshot_queries::insert_snapshot(
            conn,
            memory_id,
            &summary,
            Some(state_hash.as_bytes()),
        )
        .map_err(|e| CliError::Database(e.to_string()))?;

        // Get the snapshot ID we just inserted.
        let snapshot_id: i64 = conn
            .query_row(
                "SELECT id FROM memory_snapshots WHERE memory_id = ?1 ORDER BY id DESC LIMIT 1",
                [memory_id],
                |row| row.get(0),
            )
            .map_err(|e| CliError::Database(e.to_string()))?;

        // Record compacted range.
        compaction_queries::insert_compaction_range(
            conn,
            run_id,
            memory_id,
            min_id,
            max_id,
            Some(snapshot_id),
        )
        .map_err(|e| CliError::Database(e.to_string()))?;

        total_memories += 1;
        total_events += events.len() as i64;
    }

    compaction_queries::complete_compaction_run(conn, run_id, total_memories, total_events)
        .map_err(|e| CliError::Database(e.to_string()))?;

    Ok((total_memories, total_events))
}

/// Build a summary snapshot from compacted events.
fn build_compaction_summary(
    memory_id: &str,
    events: &[cortex_storage::queries::compaction_queries::CompactableEvent],
) -> String {
    let event_count = events.len();
    let first_at = events
        .first()
        .map(|e| e.recorded_at.as_str())
        .unwrap_or("unknown");
    let last_at = events
        .last()
        .map(|e| e.recorded_at.as_str())
        .unwrap_or("unknown");

    // Collect unique event types.
    let event_types: std::collections::BTreeSet<&str> =
        events.iter().map(|e| e.event_type.as_str()).collect();

    // Try to merge deltas (take the last one as the most recent state).
    let latest_delta = events.last().map(|e| e.delta.as_str()).unwrap_or("{}");

    serde_json::json!({
        "memory_id": memory_id,
        "compaction_summary": true,
        "event_count": event_count,
        "event_types": event_types.into_iter().collect::<Vec<_>>(),
        "first_event_at": first_at,
        "last_event_at": last_at,
        "latest_delta": serde_json::from_str::<serde_json::Value>(latest_delta)
            .unwrap_or(serde_json::json!(latest_delta)),
    })
    .to_string()
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn get_db_path(conn: &rusqlite::Connection) -> Option<String> {
    conn.path().map(|p| p.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_backend(db_path: &std::path::Path) -> CliBackend {
        let config = crate::config::GhostConfig::test_config(39780, db_path.to_str().unwrap());
        let pool = crate::db_pool::create_existing_pool(db_path.to_path_buf()).unwrap();
        CliBackend::Direct { config, db: pool }
    }

    #[test]
    fn status_fails_when_required_objects_are_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("status.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        conn.execute_batch("DROP TABLE audit_log;").unwrap();

        let backend = direct_backend(&db_path);
        let result = run_status(
            DbStatusArgs {
                output: OutputFormat::Table,
            },
            &backend,
        );
        assert!(result.is_err(), "status should fail on missing audit_log");
    }

    #[test]
    fn verify_fails_when_required_objects_are_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("verify.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        conn.execute_batch("DROP INDEX idx_channels_agent;")
            .unwrap();

        let backend = direct_backend(&db_path);
        let result = run_verify(DbVerifyArgs { full: false }, &backend);
        assert!(
            result.is_err(),
            "verify should fail on missing channels index"
        );
    }
}
