//! `git_status` — list file statuses in a git repository.
//!
//! Read-only skill. Always available (convergence max = Level 4).
//!
//! ## Input
//!
//! | Field       | Type   | Required | Default       |
//! |-------------|--------|----------|---------------|
//! | `repo_path` | string | no       | cwd discovery |
//!
//! ## Output
//!
//! ```json
//! {
//!   "entries": [{ "path": "src/main.rs", "status": "modified", "flags": ["wt_modified"] }],
//!   "is_clean": false,
//!   "head": "abc123..."
//! }
//! ```

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::{format_status, open_repo, status_flags};

/// Skill that lists file statuses in a git repository.
pub struct GitStatusSkill;

impl Skill for GitStatusSkill {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "List file statuses in a git repository (staged, modified, untracked)"
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

        // Collect file statuses.
        let statuses = repo.statuses(None).map_err(|e| {
            SkillError::Internal(format!("failed to read statuses: {e}"))
        })?;

        let entries: Vec<serde_json::Value> = statuses
            .iter()
            .filter(|entry| !entry.status().is_ignored())
            .map(|entry| {
                serde_json::json!({
                    "path": entry.path().unwrap_or("<invalid utf-8>"),
                    "status": format_status(entry.status()),
                    "flags": status_flags(entry.status()),
                })
            })
            .collect();

        let is_clean = entries.is_empty();

        // Read HEAD commit ID (if any).
        let head = repo
            .head()
            .ok()
            .and_then(|r| r.target())
            .map(|oid| oid.to_string());

        Ok(serde_json::json!({
            "entries": entries,
            "is_clean": is_clean,
            "head": head,
        }))
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

    /// RAII guard that creates a temp git repo and cleans up on drop.
    struct TestRepo {
        path: std::path::PathBuf,
    }

    impl TestRepo {
        fn new() -> (Self, git2::Repository) {
            let path =
                std::env::temp_dir().join(format!("ghost-git-test-{}", Uuid::now_v7()));
            std::fs::create_dir_all(&path).unwrap();
            let repo = git2::Repository::init(&path).unwrap();

            // Configure committer identity.
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
    fn clean_repo_returns_empty_entries() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        let result = GitStatusSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["is_clean"], true);
        assert!(val["entries"].as_array().unwrap().is_empty());
    }

    #[test]
    fn detects_new_untracked_file() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        // Create an untracked file.
        std::fs::write(test_repo.path.join("hello.txt"), "hello").unwrap();

        let result = GitStatusSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["is_clean"], false);

        let entries = val["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["path"], "hello.txt");
        assert_eq!(entries[0]["status"], "new");
    }

    #[test]
    fn detects_modified_file() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new();

        // Create and commit a file.
        let file_path = test_repo.path.join("file.txt");
        std::fs::write(&file_path, "initial").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("file.txt")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        // Modify the file.
        std::fs::write(&file_path, "modified").unwrap();

        let result = GitStatusSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["is_clean"], false);

        let entries = val["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["status"], "modified");
    }

    #[test]
    fn invalid_repo_path_returns_error() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = GitStatusSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": "/nonexistent/path" }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("cannot open git repository"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GitStatusSkill.name(), "git_status");
        assert!(GitStatusSkill.removable());
        assert_eq!(GitStatusSkill.source(), SkillSource::Bundled);
    }
}
