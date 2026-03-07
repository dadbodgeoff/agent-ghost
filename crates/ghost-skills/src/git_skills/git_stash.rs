//! `git_stash` — save, apply, pop, list, or drop stash entries.
//!
//! Write skill. Gated to convergence Level 2, confirmation required.
//!
//! ## Input
//!
//! | Field               | Type   | Required                | Default       |
//! |---------------------|--------|-------------------------|---------------|
//! | `repo_path`         | string | no                      | cwd discovery |
//! | `action`            | string | yes                     | —             |
//! | `message`           | string | no (save only)          | —             |
//! | `index`             | u64    | no (apply/pop/drop)     | 0             |
//! | `include_untracked` | bool   | no (save only)          | false         |
//!
//! Actions: `"save"`, `"apply"`, `"pop"`, `"list"`, `"drop"`

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::open_repo;

/// Skill that manages git stash entries.
pub struct GitStashSkill;

impl Skill for GitStashSkill {
    fn name(&self) -> &str {
        "git_stash"
    }

    fn description(&self) -> &str {
        "Save, apply, pop, list, or drop git stash entries"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let repo_path = input.get("repo_path").and_then(|v| v.as_str());
        let mut repo = open_repo(repo_path)?;

        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'action' (one of: save, apply, pop, list, drop)".into(),
                )
            })?;

        match action {
            "save" => stash_save(&mut repo, input),
            "apply" => stash_apply(&mut repo, input),
            "pop" => stash_pop(&mut repo, input),
            "list" => stash_list(&mut repo),
            "drop" => stash_drop(&mut repo, input),
            other => Err(SkillError::InvalidInput(format!(
                "invalid action '{other}', must be one of: save, apply, pop, list, drop"
            ))),
        }
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let index = input.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
        match action {
            "save" => {
                let msg = input
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("WIP");
                Some(format!("Stash save: \"{msg}\""))
            }
            "apply" => Some(format!("Stash apply @{{{index}}}")),
            "pop" => Some(format!("Stash pop @{{{index}}}")),
            "list" => Some("Stash list".into()),
            "drop" => Some(format!("Stash drop @{{{index}}}")),
            _ => Some(format!("git_stash: {action}")),
        }
    }
}

fn stash_save(repo: &mut git2::Repository, input: &serde_json::Value) -> SkillResult {
    let message = input
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("WIP");

    let include_untracked = input
        .get("include_untracked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let sig = repo
        .signature()
        .map_err(|e| SkillError::InvalidInput(format!("git user identity not configured: {e}")))?;

    let mut flags = git2::StashFlags::DEFAULT;
    if include_untracked {
        flags |= git2::StashFlags::INCLUDE_UNTRACKED;
    }

    let stash_oid = repo
        .stash_save(&sig, message, Some(flags))
        .map_err(|e| SkillError::Internal(format!("failed to save stash: {e}")))?;

    tracing::info!(stash_id = %stash_oid, message = message, "Stash saved");

    Ok(serde_json::json!({
        "action": "saved",
        "stash_id": stash_oid.to_string(),
        "message": message,
    }))
}

fn stash_apply(repo: &mut git2::Repository, input: &serde_json::Value) -> SkillResult {
    let index = input.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    repo.stash_apply(index, None)
        .map_err(|e| SkillError::Internal(format!("failed to apply stash @{{{index}}}: {e}")))?;

    tracing::info!(index = index, "Stash applied");

    Ok(serde_json::json!({
        "action": "applied",
        "index": index,
    }))
}

fn stash_pop(repo: &mut git2::Repository, input: &serde_json::Value) -> SkillResult {
    let index = input.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    repo.stash_pop(index, None)
        .map_err(|e| SkillError::Internal(format!("failed to pop stash @{{{index}}}: {e}")))?;

    tracing::info!(index = index, "Stash popped");

    Ok(serde_json::json!({
        "action": "popped",
        "index": index,
    }))
}

fn stash_list(repo: &mut git2::Repository) -> SkillResult {
    let mut stashes = Vec::new();

    repo.stash_foreach(|index, message, _oid| {
        stashes.push(serde_json::json!({
            "index": index,
            "message": message,
        }));
        true // continue iterating
    })
    .map_err(|e| SkillError::Internal(format!("failed to list stashes: {e}")))?;

    Ok(serde_json::json!({
        "stashes": stashes,
        "total": stashes.len(),
    }))
}

fn stash_drop(repo: &mut git2::Repository, input: &serde_json::Value) -> SkillResult {
    let index = input.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    repo.stash_drop(index)
        .map_err(|e| SkillError::Internal(format!("failed to drop stash @{{{index}}}: {e}")))?;

    tracing::info!(index = index, "Stash dropped");

    Ok(serde_json::json!({
        "action": "dropped",
        "index": index,
    }))
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
        /// Create a test repo with one committed file and one modified file
        /// (so there's something to stash).
        fn new_with_changes() -> (Self, git2::Repository) {
            let path = std::env::temp_dir().join(format!("ghost-git-stash-{}", Uuid::now_v7()));
            std::fs::create_dir_all(&path).unwrap();
            let repo = git2::Repository::init(&path).unwrap();

            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();

            // Create initial commit.
            let file_path = path.join("file.txt");
            std::fs::write(&file_path, "original").unwrap();
            {
                let mut index = repo.index().unwrap();
                index.add_path(std::path::Path::new("file.txt")).unwrap();
                index.write().unwrap();
                let tree_oid = index.write_tree().unwrap();
                let tree = repo.find_tree(tree_oid).unwrap();
                let sig = repo.signature().unwrap();
                repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                    .unwrap();
            }

            // Modify the file (creates something to stash).
            std::fs::write(&file_path, "modified").unwrap();

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
    fn save_and_list_stash() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new_with_changes();

        // Save stash.
        let result = GitStashSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "save",
                "message": "test stash",
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["action"], "saved");

        // List stashes.
        let result = GitStashSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "list",
            }),
        );
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["total"], 1);
    }

    #[test]
    fn save_and_pop_stash() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new_with_changes();

        // Save stash.
        GitStashSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "repo_path": test_repo.path_str(),
                    "action": "save",
                }),
            )
            .unwrap();

        // Pop stash.
        let result = GitStashSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "pop",
                "index": 0,
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["action"], "popped");

        // Verify stash list is now empty.
        let result = GitStashSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "list",
            }),
        );
        assert_eq!(result.unwrap()["total"], 0);
    }

    #[test]
    fn missing_action_returns_error() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new_with_changes();

        let result = GitStashSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("action"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn preview_describes_action() {
        let preview = GitStashSkill.preview(&serde_json::json!({
            "action": "save",
            "message": "WIP feature",
        }));
        assert_eq!(preview, Some("Stash save: \"WIP feature\"".into()));

        let preview = GitStashSkill.preview(&serde_json::json!({
            "action": "pop",
            "index": 2,
        }));
        assert_eq!(preview, Some("Stash pop @{2}".into()));
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GitStashSkill.name(), "git_stash");
        assert!(GitStashSkill.removable());
    }
}
