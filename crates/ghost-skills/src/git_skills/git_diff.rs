//! `git_diff` — show diff for staged or unstaged changes.
//!
//! Read-only skill. Always available (convergence max = Level 4).
//!
//! ## Input
//!
//! | Field           | Type     | Required | Default       |
//! |-----------------|----------|----------|---------------|
//! | `repo_path`     | string   | no       | cwd discovery |
//! | `staged`        | bool     | no       | false         |
//! | `context_lines` | u32      | no       | 3             |
//! | `paths`         | [string] | no       | all files     |
//!
//! ## Output
//!
//! ```json
//! {
//!   "diff": "--- a/file.rs\n+++ b/file.rs\n...",
//!   "stats": { "files_changed": 2, "insertions": 10, "deletions": 5 },
//!   "file_diffs": [{ "old_path": "...", "new_path": "...", "status": "modified" }]
//! }
//! ```

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::open_repo;

/// Skill that shows diff for staged or unstaged changes.
pub struct GitDiffSkill;

impl Skill for GitDiffSkill {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show diff for staged or unstaged changes in a git repository"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let repo_path = input.get("repo_path").and_then(|v| v.as_str());
        let repo = open_repo(repo_path)?;

        let staged = input
            .get("staged")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let context_lines = input
            .get("context_lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;

        // Build diff options.
        let mut opts = git2::DiffOptions::new();
        opts.context_lines(context_lines);

        // Filter to specific paths if requested.
        if let Some(paths) = input.get("paths").and_then(|v| v.as_array()) {
            for path in paths {
                if let Some(p) = path.as_str() {
                    opts.pathspec(p);
                }
            }
        }

        // Compute the diff.
        let diff = if staged {
            // Staged changes: diff HEAD tree to index.
            let head_tree = repo
                .head()
                .and_then(|r| r.peel_to_tree())
                .ok();
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
        } else {
            // Unstaged changes: diff index to working directory.
            repo.diff_index_to_workdir(None, Some(&mut opts))
        }
        .map_err(|e| SkillError::Internal(format!("failed to compute diff: {e}")))?;

        // Render patch text.
        let mut patch_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let origin = line.origin();
            match origin {
                '+' | '-' | ' ' => patch_text.push(origin),
                _ => {}
            }
            if let Ok(content) = std::str::from_utf8(line.content()) {
                patch_text.push_str(content);
            }
            true
        })
        .map_err(|e| SkillError::Internal(format!("failed to render diff: {e}")))?;

        // Collect stats.
        let stats = diff
            .stats()
            .map_err(|e| SkillError::Internal(format!("failed to compute stats: {e}")))?;

        // Collect per-file diff metadata.
        let file_diffs: Vec<serde_json::Value> = (0..diff.deltas().len())
            .map(|i| {
                let delta = diff.get_delta(i).unwrap();
                serde_json::json!({
                    "old_path": delta.old_file().path().map(|p| p.to_string_lossy().into_owned()),
                    "new_path": delta.new_file().path().map(|p| p.to_string_lossy().into_owned()),
                    "status": format_delta_status(delta.status()),
                })
            })
            .collect();

        Ok(serde_json::json!({
            "diff": patch_text,
            "stats": {
                "files_changed": stats.files_changed(),
                "insertions": stats.insertions(),
                "deletions": stats.deletions(),
            },
            "file_diffs": file_diffs,
        }))
    }
}

/// Map a `git2::Delta` status to a human-readable string.
fn format_delta_status(status: git2::Delta) -> &'static str {
    match status {
        git2::Delta::Added => "added",
        git2::Delta::Deleted => "deleted",
        git2::Delta::Modified => "modified",
        git2::Delta::Renamed => "renamed",
        git2::Delta::Copied => "copied",
        git2::Delta::Typechange => "typechange",
        git2::Delta::Unmodified => "unmodified",
        git2::Delta::Ignored => "ignored",
        git2::Delta::Untracked => "untracked",
        git2::Delta::Unreadable => "unreadable",
        git2::Delta::Conflicted => "conflicted",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillContext;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    struct TestRepo {
        path: std::path::PathBuf,
    }

    impl TestRepo {
        fn new() -> (Self, git2::Repository) {
            let path =
                std::env::temp_dir().join(format!("ghost-git-diff-{}", Uuid::now_v7()));
            std::fs::create_dir_all(&path).unwrap();
            let repo = git2::Repository::init(&path).unwrap();

            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();

            (Self { path }, repo)
        }

        fn path_str(&self) -> &str {
            self.path.to_str().unwrap()
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn empty_repo_returns_empty_diff() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        let result = GitDiffSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["stats"]["files_changed"], 0);
        assert_eq!(val["stats"]["insertions"], 0);
        assert_eq!(val["stats"]["deletions"], 0);
    }

    #[test]
    fn detects_unstaged_modifications() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new();

        // Create and commit a file.
        let file_path = test_repo.path.join("file.txt");
        std::fs::write(&file_path, "line1\nline2\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("file.txt")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        // Modify the file (unstaged).
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let result = GitDiffSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "staged": false,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(val["stats"]["files_changed"].as_u64().unwrap() > 0);
        assert!(val["stats"]["insertions"].as_u64().unwrap() > 0);
        assert!(val["diff"].as_str().unwrap().contains("+line3"));
    }

    #[test]
    fn detects_staged_changes() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new();

        // Create and commit a file.
        let file_path = test_repo.path.join("file.txt");
        std::fs::write(&file_path, "original\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("file.txt")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        // Modify and stage the file.
        std::fs::write(&file_path, "modified\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("file.txt")).unwrap();
        index.write().unwrap();

        let result = GitDiffSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "staged": true,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert!(val["stats"]["files_changed"].as_u64().unwrap() > 0);
        assert!(val["diff"].as_str().unwrap().contains("+modified"));
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GitDiffSkill.name(), "git_diff");
        assert!(GitDiffSkill.removable());
        assert_eq!(GitDiffSkill.source(), SkillSource::Bundled);
    }
}
