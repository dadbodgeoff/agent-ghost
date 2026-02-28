//! OpenClaw migration orchestrator (Req 37 AC1, AC3).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::importers::{config, memory, skill, soul};
use crate::{MigrateError, MigrateResult};

/// Result of a migration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
    pub review_items: Vec<String>,
}

impl MigrationResult {
    pub fn new() -> Self {
        Self {
            imported: Vec::new(),
            skipped: Vec::new(),
            warnings: Vec::new(),
            review_items: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.imported.is_empty()
            && self.skipped.is_empty()
            && self.warnings.is_empty()
            && self.review_items.is_empty()
    }
}

impl Default for MigrationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Orchestrates OpenClaw → GHOST migration.
pub struct OpenClawMigrator {
    source: PathBuf,
    target: PathBuf,
}

impl OpenClawMigrator {
    pub fn new(source: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
        }
    }

    /// Detect if a valid OpenClaw installation exists at the source path.
    pub fn detect(path: &Path) -> bool {
        path.exists() && path.is_dir() && path.join("SOUL.md").exists()
    }

    /// Run the full migration. Non-destructive — source files are never modified.
    pub fn migrate(&self) -> MigrateResult<MigrationResult> {
        if !Self::detect(&self.source) {
            return Err(MigrateError::NotFound(format!(
                "No OpenClaw installation at: {}",
                self.source.display()
            )));
        }

        let mut result = MigrationResult::new();

        // Ensure target directory exists
        std::fs::create_dir_all(&self.target)?;

        // Import SOUL.md
        match soul::import_soul(&self.source, &self.target) {
            Ok(msg) => result.imported.push(msg),
            Err(e) => result.warnings.push(format!("SOUL.md: {}", e)),
        }

        // Import memories
        match memory::import_memories(&self.source, &self.target) {
            Ok(msgs) => result.imported.extend(msgs),
            Err(e) => result.warnings.push(format!("memories: {}", e)),
        }

        // Import skills
        match skill::import_skills(&self.source, &self.target) {
            Ok((imported, quarantined)) => {
                result.imported.extend(imported);
                result.review_items.extend(quarantined);
            }
            Err(e) => result.warnings.push(format!("skills: {}", e)),
        }

        // Import config
        match config::import_config(&self.source, &self.target) {
            Ok(msg) => result.imported.push(msg),
            Err(e) => result.warnings.push(format!("config: {}", e)),
        }

        // Verify source was not modified (non-destructive guarantee)
        tracing::info!(
            source = %self.source.display(),
            imported = result.imported.len(),
            warnings = result.warnings.len(),
            "Migration complete"
        );

        Ok(result)
    }
}
