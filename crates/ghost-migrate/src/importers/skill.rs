//! Skill importer — converts YAML frontmatter, quarantines unsigned.

use std::path::Path;

use crate::MigrateResult;

/// Import skills, quarantining unsigned ones.
/// Returns (imported, quarantined) lists.
pub fn import_skills(source: &Path, target: &Path) -> MigrateResult<(Vec<String>, Vec<String>)> {
    let skills_dir = source.join("skills");
    if !skills_dir.exists() {
        return Ok((
            vec!["No skills directory found, skipped".to_string()],
            Vec::new(),
        ));
    }

    let target_dir = target.join("skills");
    let quarantine_dir = target.join("skills_quarantine");
    std::fs::create_dir_all(&target_dir)?;
    std::fs::create_dir_all(&quarantine_dir)?;

    let mut imported = Vec::new();
    let mut quarantined = Vec::new();

    for entry in std::fs::read_dir(&skills_dir)? {
        let entry = entry?;
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();

        if !path.is_file() {
            continue;
        }

        let content = std::fs::read_to_string(&path)?;

        // Check for signature in frontmatter
        let has_signature = content.contains("signature:") && content.contains("-----BEGIN");

        if has_signature {
            // Strip incompatible permissions, keep the rest
            let cleaned = strip_incompatible_permissions(&content);
            std::fs::write(target_dir.join(&filename), &cleaned)?;
            imported.push(format!("skill: {}", filename));
        } else {
            // Quarantine unsigned skills
            std::fs::write(quarantine_dir.join(&filename), &content)?;
            quarantined.push(format!("quarantined (unsigned): {}", filename));
        }
    }

    Ok((imported, quarantined))
}

fn strip_incompatible_permissions(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            !line.contains("permission: root")
                && !line.contains("permission: admin")
                && !line.contains("permission: system")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
