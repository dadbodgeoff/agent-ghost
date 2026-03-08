//! # cortex-storage
//!
//! SQLite storage layer with append-only triggers, hash chain columns,
//! and convergence tables for the Cortex memory system.

pub mod migrations;
pub mod queries;
pub mod schema_contract;
pub mod sqlite;

use cortex_core::models::error::{CortexError, CortexResult};
use rusqlite::Connection;

/// Convert a storage error string into a CortexError.
pub fn to_storage_err(msg: String) -> CortexError {
    CortexError::Storage(msg)
}

/// Open an in-memory database (for testing).
pub fn open_in_memory() -> CortexResult<Connection> {
    Connection::open_in_memory().map_err(|e| to_storage_err(e.to_string()))
}

/// Query the current schema version.
pub fn current_version(conn: &Connection) -> CortexResult<u32> {
    migrations::current_version(conn)
}

/// Run all migrations up to LATEST_VERSION on the given connection.
pub fn run_all_migrations(conn: &Connection) -> CortexResult<()> {
    migrations::run_migrations(conn)
}
