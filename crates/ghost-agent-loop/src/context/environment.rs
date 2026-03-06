//! L4 Environment context builder.
//!
//! Gathers workspace metadata for the agent's environmental awareness:
//! - Working directory and project type
//! - Git repository state (branch, remotes)
//! - OS/platform info
//! - Available runtimes
//!
//! Output is kept compact (target ≤200 tokens) for L4's fixed budget.

use std::path::Path;

/// Build the L4 environment context string from the current workspace.
///
/// This is called once at agent startup (not per-turn) since the environment
/// is largely static within a session.
pub fn build_environment_context(workspace_root: Option<&Path>) -> String {
    let mut sections: Vec<String> = Vec::new();

    // Platform
    sections.push(format!(
        "Platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
    ));

    // Date (minute precision for KV cache stability — no seconds)
    let now = chrono::Local::now();
    sections.push(format!("Date: {}", now.format("%Y-%m-%d %H:%M")));

    // Working directory
    let cwd = workspace_root
        .map(|p| p.to_path_buf())
        .or_else(|| std::env::current_dir().ok());

    if let Some(ref dir) = cwd {
        sections.push(format!("Workspace: {}", dir.display()));

        // Project type detection
        let project_types = detect_project_types(dir);
        if !project_types.is_empty() {
            sections.push(format!("Project: {}", project_types.join(", ")));
        }

        // Git info
        if let Some(git_info) = detect_git_info(dir) {
            sections.push(git_info);
        }

        // Key config files present
        let configs = detect_config_files(dir);
        if !configs.is_empty() {
            sections.push(format!("Config: {}", configs.join(", ")));
        }
    }

    // GHOST-specific
    let ghost_home = dirs::home_dir()
        .map(|h| h.join(".ghost"))
        .filter(|p| p.exists());
    if let Some(ref gh) = ghost_home {
        let mut ghost_info = vec!["~/.ghost present".to_string()];
        if gh.join("config/SOUL.md").exists() {
            ghost_info.push("SOUL.md loaded".into());
        }
        if gh.join("data/ghost.db").exists() {
            ghost_info.push("DB active".into());
        }
        sections.push(format!("GHOST: {}", ghost_info.join(", ")));
    }

    sections.join("\n")
}

/// Detect project types from marker files in the workspace root.
fn detect_project_types(dir: &Path) -> Vec<&'static str> {
    let mut types = Vec::new();

    if dir.join("Cargo.toml").exists() {
        types.push("Rust");
    }
    if dir.join("package.json").exists() {
        types.push("Node.js");
    }
    if dir.join("pyproject.toml").exists() || dir.join("setup.py").exists() {
        types.push("Python");
    }
    if dir.join("go.mod").exists() {
        types.push("Go");
    }
    if dir.join("pom.xml").exists() || dir.join("build.gradle").exists() {
        types.push("Java");
    }
    if dir.join("Dockerfile").exists() || dir.join("docker-compose.yml").exists() {
        types.push("Docker");
    }
    if dir.join("src-tauri").exists() {
        types.push("Tauri");
    }

    types
}

/// Detect git repository info (branch, remote).
fn detect_git_info(dir: &Path) -> Option<String> {
    let git_dir = dir.join(".git");
    if !git_dir.exists() {
        return None;
    }

    let mut parts = vec!["git repo".to_string()];

    // Read current branch from HEAD
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) {
        let head = head.trim();
        if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
            parts.push(format!("branch={branch}"));
        }
    }

    // Read origin remote
    if let Ok(config) = std::fs::read_to_string(git_dir.join("config")) {
        for line in config.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("url = ") {
                if let Some(url) = trimmed.strip_prefix("url = ") {
                    // Shorten to just org/repo
                    let short = shorten_git_url(url);
                    parts.push(format!("origin={short}"));
                    break;
                }
            }
        }
    }

    Some(format!("Git: {}", parts.join(", ")))
}

/// Shorten a git URL to org/repo format.
fn shorten_git_url(url: &str) -> String {
    // Handle SSH: git@github.com:org/repo.git
    if let Some(path) = url.strip_prefix("git@github.com:") {
        return path.trim_end_matches(".git").to_string();
    }
    // Handle HTTPS: https://github.com/org/repo.git
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        return rest.trim_end_matches(".git").to_string();
    }
    // Fallback: just use the URL as-is but truncate
    if url.len() > 50 {
        format!("{}...", &url[..47])
    } else {
        url.to_string()
    }
}

/// Detect notable config files in the workspace.
fn detect_config_files(dir: &Path) -> Vec<&'static str> {
    let mut configs = Vec::new();

    if dir.join("ghost.yml").exists() {
        configs.push("ghost.yml");
    }
    if dir.join(".env").exists() {
        configs.push(".env");
    }
    if dir.join("CLAUDE.md").exists() {
        configs.push("CLAUDE.md");
    }

    configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_environment_context_no_workspace() {
        let ctx = build_environment_context(None);
        assert!(ctx.contains("Platform:"));
        assert!(ctx.contains("Date:"));
    }

    #[test]
    fn test_shorten_git_url_ssh() {
        assert_eq!(
            shorten_git_url("git@github.com:org/repo.git"),
            "org/repo"
        );
    }

    #[test]
    fn test_shorten_git_url_https() {
        assert_eq!(
            shorten_git_url("https://github.com/org/repo.git"),
            "org/repo"
        );
    }

    #[test]
    fn test_detect_project_types_empty() {
        let types = detect_project_types(Path::new("/nonexistent"));
        assert!(types.is_empty());
    }
}
