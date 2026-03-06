//! `git_commit` — create a commit in a git repository.
//!
//! Write skill. Gated to convergence Level 2, confirmation required.
//!
//! ## Input
//!
//! | Field       | Type     | Required | Default       |
//! |-------------|----------|----------|---------------|
//! | `repo_path` | string   | no       | cwd discovery |
//! | `message`   | string   | yes      | —             |
//! | `paths`     | [string] | no       | —             |
//! | `all`       | bool     | no       | false         |
//!
//! If `paths` is provided, those files are staged before committing.
//! If `all` is true, all modified tracked files are staged.
//! If neither is provided, commits whatever is currently staged.

use std::path::Path;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::open_repo;

/// Skill that creates a commit in a git repository.
pub struct GitCommitSkill;

impl Skill for GitCommitSkill {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Create a commit in a git repository"
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

        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'message' (string)".into(),
                )
            })?;

        if message.trim().is_empty() {
            return Err(SkillError::InvalidInput(
                "commit message must not be empty".into(),
            ));
        }

        let stage_all = input
            .get("all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Stage files if requested.
        let mut index = repo.index().map_err(|e| {
            SkillError::Internal(format!("failed to open index: {e}"))
        })?;

        if let Some(paths) = input.get("paths").and_then(|v| v.as_array()) {
            for path_val in paths {
                if let Some(p) = path_val.as_str() {
                    index.add_path(Path::new(p)).map_err(|e| {
                        SkillError::InvalidInput(format!(
                            "failed to stage '{p}': {e}"
                        ))
                    })?;
                }
            }
            index.write().map_err(|e| {
                SkillError::Internal(format!("failed to write index: {e}"))
            })?;
        } else if stage_all {
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .map_err(|e| {
                    SkillError::Internal(format!("failed to stage all files: {e}"))
                })?;
            index.write().map_err(|e| {
                SkillError::Internal(format!("failed to write index: {e}"))
            })?;
        }

        // Write the index to a tree.
        let tree_oid = index.write_tree().map_err(|e| {
            SkillError::Internal(format!("failed to write tree: {e}"))
        })?;
        let tree = repo.find_tree(tree_oid).map_err(|e| {
            SkillError::Internal(format!("failed to find tree: {e}"))
        })?;

        // Resolve parent commits (empty for initial commit).
        let parents: Vec<git2::Commit<'_>> = match repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit().map_err(|e| {
                    SkillError::Internal(format!("failed to peel HEAD to commit: {e}"))
                })?;
                vec![commit]
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => vec![],
            Err(e) => {
                return Err(SkillError::Internal(format!(
                    "failed to read HEAD: {e}"
                )));
            }
        };
        let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

        // Get the committer signature from git config.
        let sig = repo.signature().map_err(|e| {
            SkillError::InvalidInput(format!(
                "git user identity not configured (set user.name and user.email): {e}"
            ))
        })?;

        // Create the commit.
        let commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .map_err(|e| {
                SkillError::Internal(format!("failed to create commit: {e}"))
            })?;

        let commit_id = commit_oid.to_string();
        let short_id = &commit_id[..std::cmp::min(7, commit_id.len())];
        let author = format!(
            "{} <{}>",
            sig.name().unwrap_or("?"),
            sig.email().unwrap_or("?")
        );

        tracing::info!(
            commit_id = %short_id,
            message = message,
            "Commit created"
        );

        Ok(serde_json::json!({
            "commit_id": commit_id,
            "short_id": short_id,
            "message": message,
            "author": author,
        }))
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("<no message>");
        Some(format!("Create commit: \"{message}\""))
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
                std::env::temp_dir().join(format!("ghost-git-commit-{}", Uuid::now_v7()));
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
    fn creates_initial_commit() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        std::fs::write(test_repo.path.join("file.txt"), "hello").unwrap();

        let result = GitCommitSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "message": "initial commit",
                "paths": ["file.txt"],
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["message"], "initial commit");
        assert!(val["commit_id"].as_str().unwrap().len() >= 7);
        assert_eq!(val["author"], "Test User <test@example.com>");
    }

    #[test]
    fn creates_subsequent_commit() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        // First commit.
        std::fs::write(test_repo.path.join("a.txt"), "a").unwrap();
        GitCommitSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "repo_path": test_repo.path_str(),
                    "message": "first",
                    "paths": ["a.txt"],
                }),
            )
            .unwrap();

        // Second commit.
        std::fs::write(test_repo.path.join("b.txt"), "b").unwrap();
        let result = GitCommitSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "message": "second",
                "paths": ["b.txt"],
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["message"], "second");
    }

    #[test]
    fn rejects_empty_message() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        let result = GitCommitSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "message": "  ",
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("empty"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_message() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        let result = GitCommitSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("message"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn preview_shows_message() {
        let preview = GitCommitSkill.preview(&serde_json::json!({
            "message": "fix: resolve auth bug",
        }));
        assert_eq!(
            preview,
            Some("Create commit: \"fix: resolve auth bug\"".into())
        );
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GitCommitSkill.name(), "git_commit");
        assert!(GitCommitSkill.removable());
    }
}
