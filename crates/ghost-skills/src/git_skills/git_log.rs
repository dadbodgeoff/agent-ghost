//! `git_log` — show commit history.
//!
//! Read-only skill. Always available (convergence max = Level 4).
//!
//! ## Input
//!
//! | Field       | Type   | Required | Default       |
//! |-------------|--------|----------|---------------|
//! | `repo_path` | string | no       | cwd discovery |
//! | `max_count` | u64    | no       | 20            |
//! | `skip`      | u64    | no       | 0             |
//!
//! ## Output
//!
//! ```json
//! {
//!   "commits": [{
//!     "id": "abc123...",
//!     "short_id": "abc1234",
//!     "author_name": "Name",
//!     "author_email": "email",
//!     "message": "full message",
//!     "summary": "first line",
//!     "time": "2026-01-01T00:00:00Z",
//!     "parent_count": 1
//!   }],
//!   "total_returned": 10
//! }
//! ```

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

use super::open_repo;

/// Skill that shows commit history from a git repository.
pub struct GitLogSkill;

impl Skill for GitLogSkill {
    fn name(&self) -> &str {
        "git_log"
    }

    fn description(&self) -> &str {
        "Show commit history from a git repository"
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

        let max_count = input
            .get("max_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let skip = input.get("skip").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        // Get HEAD reference. If the repo has no commits, return empty list.
        let head = match repo.head() {
            Ok(reference) => reference,
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                return Ok(serde_json::json!({
                    "commits": [],
                    "total_returned": 0,
                }));
            }
            Err(e) => {
                return Err(SkillError::Internal(format!("failed to read HEAD: {e}")));
            }
        };

        let head_oid = head
            .target()
            .ok_or_else(|| SkillError::Internal("HEAD reference has no target OID".into()))?;

        // Set up the revision walker.
        let mut revwalk = repo
            .revwalk()
            .map_err(|e| SkillError::Internal(format!("failed to create revwalk: {e}")))?;
        revwalk
            .push(head_oid)
            .map_err(|e| SkillError::Internal(format!("failed to push HEAD to revwalk: {e}")))?;
        revwalk
            .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
            .map_err(|e| SkillError::Internal(format!("failed to set sorting: {e}")))?;

        // Walk commits with skip and limit.
        let mut commits = Vec::new();
        for (i, oid_result) in revwalk.enumerate() {
            if i < skip {
                continue;
            }
            if commits.len() >= max_count {
                break;
            }

            let oid =
                oid_result.map_err(|e| SkillError::Internal(format!("revwalk error: {e}")))?;

            let commit = repo
                .find_commit(oid)
                .map_err(|e| SkillError::Internal(format!("failed to find commit {oid}: {e}")))?;

            let author = commit.author();
            let time = commit.time();
            let secs = time.seconds();

            // Format time as ISO-8601.
            let datetime = chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| format!("{secs}"));

            let id_str = oid.to_string();
            let short_id = &id_str[..std::cmp::min(7, id_str.len())];

            commits.push(serde_json::json!({
                "id": id_str,
                "short_id": short_id,
                "author_name": author.name().unwrap_or("<invalid utf-8>"),
                "author_email": author.email().unwrap_or("<invalid utf-8>"),
                "message": commit.message().unwrap_or("<invalid utf-8>"),
                "summary": commit.summary().unwrap_or("<invalid utf-8>"),
                "time": datetime,
                "parent_count": commit.parent_count(),
            }));
        }

        let total = commits.len();
        Ok(serde_json::json!({
            "commits": commits,
            "total_returned": total,
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

    struct TestRepo {
        path: std::path::PathBuf,
    }

    impl TestRepo {
        fn new() -> (Self, git2::Repository) {
            let path = std::env::temp_dir().join(format!("ghost-git-log-{}", Uuid::now_v7()));
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

    /// Helper: create a commit with a file in the test repo.
    fn commit_file(
        repo: &git2::Repository,
        dir: &std::path::Path,
        name: &str,
        content: &str,
        msg: &str,
    ) {
        std::fs::write(dir.join(name), content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(name)).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();

        let parents: Vec<git2::Commit<'_>> = match repo.head() {
            Ok(head) => vec![head.peel_to_commit().unwrap()],
            Err(_) => vec![],
        };
        let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
            .unwrap();
    }

    #[test]
    fn empty_repo_returns_empty_log() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, _repo) = TestRepo::new();

        let result = GitLogSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["total_returned"], 0);
        assert!(val["commits"].as_array().unwrap().is_empty());
    }

    #[test]
    fn returns_commits_in_reverse_chronological_order() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new();

        commit_file(&repo, &test_repo.path, "a.txt", "a", "first commit");
        commit_file(&repo, &test_repo.path, "b.txt", "b", "second commit");
        commit_file(&repo, &test_repo.path, "c.txt", "c", "third commit");

        let result = GitLogSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let commits = val["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0]["summary"], "third commit");
        assert_eq!(commits[1]["summary"], "second commit");
        assert_eq!(commits[2]["summary"], "first commit");
    }

    #[test]
    fn respects_max_count() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new();

        for i in 0..5 {
            commit_file(
                &repo,
                &test_repo.path,
                &format!("file{i}.txt"),
                &format!("content{i}"),
                &format!("commit {i}"),
            );
        }

        let result = GitLogSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "max_count": 2,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["total_returned"], 2);
    }

    #[test]
    fn respects_skip() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new();

        commit_file(&repo, &test_repo.path, "a.txt", "a", "first");
        commit_file(&repo, &test_repo.path, "b.txt", "b", "second");
        commit_file(&repo, &test_repo.path, "c.txt", "c", "third");

        let result = GitLogSkill.execute(
            &ctx,
            &serde_json::json!({
                "repo_path": test_repo.path_str(),
                "skip": 1,
                "max_count": 1,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let commits = val["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["summary"], "second");
    }

    #[test]
    fn commit_has_expected_fields() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (test_repo, repo) = TestRepo::new();

        commit_file(&repo, &test_repo.path, "file.txt", "data", "test message");

        let result = GitLogSkill.execute(
            &ctx,
            &serde_json::json!({ "repo_path": test_repo.path_str() }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let commit = &val["commits"][0];
        assert!(commit["id"].as_str().unwrap().len() >= 7);
        assert!(commit["short_id"].as_str().unwrap().len() == 7);
        assert_eq!(commit["author_name"], "Test User");
        assert_eq!(commit["author_email"], "test@example.com");
        assert_eq!(commit["summary"], "test message");
        assert!(commit["time"].as_str().is_some());
        assert_eq!(commit["parent_count"], 0); // Initial commit.
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(GitLogSkill.name(), "git_log");
        assert!(GitLogSkill.removable());
    }
}
