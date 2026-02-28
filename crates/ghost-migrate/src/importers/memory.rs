//! Memory importer — converts free-form entries to Cortex typed memories.

use std::path::Path;

use crate::MigrateResult;

/// Import memories with conservative importance levels.
pub fn import_memories(source: &Path, target: &Path) -> MigrateResult<Vec<String>> {
    let memory_dir = source.join("memories");
    if !memory_dir.exists() {
        return Ok(vec!["No memories directory found, skipped".to_string()]);
    }

    let target_dir = target.join("memories");
    std::fs::create_dir_all(&target_dir)?;

    let mut imported = Vec::new();

    for entry in std::fs::read_dir(&memory_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "md" || e == "json").unwrap_or(false) {
            let content = std::fs::read_to_string(&path)?;
            let filename = entry.file_name().to_string_lossy().to_string();

            // Convert to GHOST format with conservative importance (Low)
            let ghost_memory = format!(
                "---\nimportance: Low\nsource: openclaw_import\n---\n{}",
                content
            );

            std::fs::write(target_dir.join(&filename), &ghost_memory)?;
            imported.push(format!("memory: {}", filename));
        }
    }

    Ok(imported)
}
