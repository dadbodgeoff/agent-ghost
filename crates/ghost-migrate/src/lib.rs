//! ghost-migrate — OpenClaw to GHOST platform migration (Req 37).
//!
//! Non-destructive migration: detects OpenClaw installations, imports
//! SOUL.md, memories, skills, and config into GHOST format.

pub mod migrator;
pub mod importers;

pub use migrator::{OpenClawMigrator, MigrationResult};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("conversion error: {0}")]
    ConversionError(String),
}

pub type MigrateResult<T> = Result<T, MigrateError>;
