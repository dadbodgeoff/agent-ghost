//! `git_branch` — create, list, or delete branches.
//!
//! Write skill. Gated to convergence Level 2, confirmation required.
//!
//! ## Input
//!
//! | Field      | Type   | Required              | Default       |
//! |------------|--------|-----------------------|---------------|
//! | `repo_path`| string | no                    | cwd discovery |
//! | `action`   | string | yes                   | —             |
//! | `name`     | string | yes (create/delete)   | —             |
//! | `force`    | bool   | no                    | false         |
//! | `from_ref` | string | no (create only)      | HEAD          |
//!
//! Actions: `"list"`, `"create"`, `"delete"`

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::open_repo;

/// Skill that creates, lists, or deletes git branches.
pub struct GitBranchSkill;

impl Skill for GitBranchSkill {
    fn name(&self) -> &str {
        "git_branch"
    }

    fn description(&self) -> &str {
        "Create, list, or delete git branches"
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

        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'action' (one of: list, create, delete)".into(),
                )
            })?;

        match action {
            "list" => list_branches(&repo),
            "create" => create_branch(&repo, input),
            "delete" => delete_branch(&repo, input),
            other => Err(SkillError::InvalidInput(format!(
                "invalid action '{other}', must be one of: list, create, delete"
            ))),
        }
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("unknown");
        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        match action {
            "create" => Some(format!("Create branch '{name}'")),
            "delete" => Some(format!("Delete branch '{name}'")),
            "list" => Some("List all branches".into()),
            _ => Some(format!("git_branch: {action}")),
        }
    }
}

fn list_branches(repo: &git2::Repository) -> SkillResult {
    let branches = repo.branches(None).map_err(|e| {
        SkillError::Internal(format!("failed to list branches: {e}"))
    })?;

    let mut branch_list = Vec::new();
    let mut current_branch: Option<String> = None;

    for branch_result in branches {
        let (branch, branch_type) = branch_result.map_err(|e| {
            SkillError::Internal(format!("failed to read branch: {e}"))
        })?;

        let name = branch
            .name()
            .map_err(|e| SkillError::Internal(format!("invalid branch name: {e}")))?
            .unwrap_or("<invalid utf-8>")
            .to_string();

        let is_head = branch.is_head();
        if is_head {
            current_branch = Some(name.clone());
        }

        let commit_id = branch
            .get()
            .target()
            .map(|oid| oid.to_string());

        let upstream = branch
            .upstream()
            .ok()
            .and_then(|u| u.name().ok().flatten().map(|s| s.to_string()));

        branch_list.push(serde_json::json!({
            "name": name,
            "is_head": is_head,
            "branch_type": match branch_type {
                git2::BranchType::Local => "local",
                git2::BranchType::Remote => "remote",
            },
            "commit_id": commit_id,
            "upstream": upstream,
        }));
    }

    Ok(serde_json::json!({
        "branches": branch_list,
        "current": current_branch,
    }))
}

fn create_branch(repo: &git2::Repository, input: &serde_json::Value) -> SkillResult {
    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'name' for create action".into())
        })?;

    let force = input
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Resolve the base reference.
    let target_commit = if let Some(from_ref) = input.get("from_ref").and_then(|v| v.as_str()) {
        let reference = repo.resolve_reference_from_short_name(from_ref).map_err(|e| {
            SkillError::InvalidInput(format!("cannot resolve reference '{from_ref}': {e}"))
        })?;
        reference.peel_to_commit().map_err(|e| {
            SkillError::Internal(format!("cannot peel '{from_ref}' to commit: {e}"))
        })?
    } else {
        repo.head()
            .and_then(|r| r.peel_to_commit())
            .map_err(|e| {
                SkillError::Internal(format!("cannot resolve HEAD: {e}"))
            })?
    };

    let branch = repo
        .branch(name, &target_commit, force)
        .map_err(|e| SkillError::Internal(format!("failed to create branch: {e}")))?;

    let commit_id = branch
        .get()
        .target()
        .map(|oid| oid.to_string());

    tracing::info!(branch = name, "Branch created");

    Ok(serde_json::json!({
        "action": "created",
        "name": name,
        "commit_id": commit_id,
    }))
}

fn delete_branch(repo: &git2::Repository, input: &serde_json::Value) -> SkillResult {
    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'name' for delete action".into())
        })?;

    let mut branch = repo
        .find_branch(name, git2::BranchType::Local)
        .map_err(|e| {
            SkillError::InvalidInput(format!("branch '{name}' not found: {e}"))
        })?;

    // Prevent deleting the currently checked-out branch.
    if branch.is_head() {
        return Err(SkillError::InvalidInput(format!(
            "cannot delete the currently checked-out branch '{name}'"
        )));
    }

    branch.delete().map_err(|e| {
        SkillError::Internal(format!("failed to delete branch '{name}': {e}"))
    })?;

    tracing::info!(branch = name, "Branch deleted");

    Ok(serde_json::json!({
        "action": "deleted",
        "name": name,
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
        fn new_with_commit() -> (Self, git2::Repository) {
            let path =
                std::env::temp_dir().join(format!("ghost-git-branch-{}", Uuid::now_v7()));
            std::fs::create_dir_all(&path).unwrap();
            let repo = git2::Repository::init(&path).unwrap();

            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();

            // Create initial commit so HEAD exists.
            std::fs::write(path.join("README.md"), "# Test").unwrap();
            {
                let mut index = repo.index().unwrap();
                index.add_path(std::path::Path::new("README.md")).unwrap();
                index.write().unwrap();
                let tree_oid = index.write_tree().unwrap();
                let tree = repo.find_tree(tree_oid).unwrap();
                let sig = repo.signature().unwrap();
                repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
                    .unwrap();
            }

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
    fn list_branches_includes_main() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new_with_commit();

        let result = GitBranchSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "list",
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let branches = val["branches"].as_array().unwrap();
        assert!(!branches.is_empty());
        // The default branch should be marked as head.
        assert!(branches.iter().any(|b| b["is_head"] == true));
    }

    #[test]
    fn create_and_list_branch() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new_with_commit();

        // Create a branch.
        let result = GitBranchSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "create",
                "name": "feature-x",
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["name"], "feature-x");

        // Verify it appears in list.
        let result = GitBranchSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "list",
            }),
        );
        let val = result.unwrap();
        let branches = val["branches"].as_array().unwrap();
        assert!(branches.iter().any(|b| b["name"] == "feature-x"));
    }

    #[test]
    fn delete_branch() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new_with_commit();

        // Create then delete.
        GitBranchSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "repo_path": test_repo.path_str(),
                    "action": "create",
                    "name": "to-delete",
                }),
            )
            .unwrap();

        let result = GitBranchSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "delete",
                "name": "to-delete",
            }),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["action"], "deleted");
    }

    #[test]
    fn cannot_delete_head_branch() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new_with_commit();

        // Find the current branch name.
        let head = repo.head().unwrap();
        let branch_name = head.shorthand().unwrap().to_string();

        let result = GitBranchSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "action": "delete",
                "name": branch_name,
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("currently checked-out"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn missing_action_returns_error() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new_with_commit();

        let result = GitBranchSkill.execute(
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
        let preview = GitBranchSkill.preview(&serde_json::json!({
            "action": "create",
            "name": "my-branch",
        }));
        assert_eq!(preview, Some("Create branch 'my-branch'".into()));
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GitBranchSkill.name(), "git_branch");
        assert!(GitBranchSkill.removable());
    }
}
