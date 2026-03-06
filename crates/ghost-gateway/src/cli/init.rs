//! ghost init — first-run setup (CLI§3.1).
//!
//! Creates `~/.ghost/` directory tree and generates default `ghost.yml` if absent.
//! Idempotent — safe to re-run.

use std::fs;
use std::path::PathBuf;

use ghost_identity::soul_manager::SoulManager;

use crate::bootstrap::{ghost_home, shellexpand_tilde};
use crate::config::GhostConfig;

use super::error::CliError;

/// Subdirectories to create under `~/.ghost/`.
const SUBDIRS: &[&str] = &[
    "config",
    "data",
    "data/convergence_state",
    "logs",
    "secrets",
];

pub async fn run() -> Result<(), CliError> {
    let home = ghost_home();

    // Check if already initialized
    let config_path = home.join("config/ghost.yml");
    let already_initialized = home.exists() && config_path.exists();

    if already_initialized {
        eprintln!("GHOST is already initialized at {}", home.display());
        eprintln!("  Config: {}", config_path.display());
        eprintln!("  Data:   {}", home.join("data").display());
        return Ok(());
    }

    // Create directory tree
    for subdir in SUBDIRS {
        let dir = home.join(subdir);
        fs::create_dir_all(&dir).map_err(|e| {
            CliError::Internal(format!("failed to create {}: {e}", dir.display()))
        })?;
    }

    // Generate default ghost.yml if absent
    if !config_path.exists() {
        let default_config = GhostConfig::default();
        let yaml = serde_yaml::to_string(&default_config).map_err(|e| {
            CliError::Internal(format!("failed to serialize default config: {e}"))
        })?;
        fs::write(&config_path, yaml).map_err(|e| {
            CliError::Internal(format!("failed to write {}: {e}", config_path.display()))
        })?;
    }

    // Generate default SOUL.md if absent
    let soul_path = home.join("config").join("SOUL.md");
    if !soul_path.exists() {
        SoulManager::create_template(&soul_path).map_err(|e| {
            CliError::Internal(format!("failed to create SOUL.md: {e}"))
        })?;
    }

    // Ensure DB path parent directory exists
    let db_path = shellexpand_tilde(&GhostConfig::default().gateway.db_path);
    let db_parent = PathBuf::from(&db_path);
    if let Some(parent) = db_parent.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CliError::Internal(format!("failed to create db directory {}: {e}", parent.display()))
        })?;
    }

    eprintln!("GHOST initialized at {}", home.display());
    eprintln!();
    eprintln!("Next steps:");
    eprintln!("  1. Edit identity:  {}", soul_path.display());
    eprintln!("  2. Edit config:    {}", config_path.display());
    eprintln!("  3. Run migrations: ghost db migrate");
    eprintln!("  4. Start gateway:  ghost serve");

    Ok(())
}
