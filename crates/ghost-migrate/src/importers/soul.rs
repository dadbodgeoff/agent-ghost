//! SOUL.md importer — maps OpenClaw SOUL.md to GHOST format.

use std::path::Path;

use crate::MigrateResult;

/// Import SOUL.md, stripping agent-mutable sections.
pub fn import_soul(source: &Path, target: &Path) -> MigrateResult<String> {
    let soul_path = source.join("SOUL.md");
    let content = std::fs::read_to_string(&soul_path)?;

    // Strip agent-mutable sections (between <!-- AGENT-MUTABLE --> markers)
    let cleaned = strip_agent_mutable(&content);

    let target_path = target.join("SOUL.md");
    std::fs::write(&target_path, &cleaned)?;

    Ok(format!("SOUL.md imported ({} bytes)", cleaned.len()))
}

fn strip_agent_mutable(content: &str) -> String {
    let mut result = String::new();
    let mut in_mutable = false;

    for line in content.lines() {
        if line.contains("<!-- AGENT-MUTABLE -->") {
            in_mutable = true;
            continue;
        }
        if line.contains("<!-- /AGENT-MUTABLE -->") {
            in_mutable = false;
            continue;
        }
        if !in_mutable {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}
