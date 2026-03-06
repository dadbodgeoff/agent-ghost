//! Phase 7: Git skills.
//!
//! Provides git operations via the `git2` crate (libgit2 bindings).
//! All read-only skills are always available (convergence max = Level 4).
//! Write skills are gated to convergence Level 2 and wrapped with
//! `ConvergenceGuard`.
//!
//! | Skill          | Risk     | Autonomy Default       | Convergence Max |
//! |----------------|----------|------------------------|-----------------|
//! | `git_status`   | Read     | Act Autonomously       | Level 4         |
//! | `git_diff`     | Read     | Act Autonomously       | Level 4         |
//! | `git_log`      | Read     | Act Autonomously       | Level 4         |
//! | `git_branch`   | Write    | Act with Confirmation  | Level 2         |
//! | `git_commit`   | Write    | Act with Confirmation  | Level 2         |
//! | `git_stash`    | Write    | Act with Confirmation  | Level 2         |

pub mod git_branch;
pub mod git_commit;
pub mod git_diff;
pub mod git_log;
pub mod git_stash;
pub mod git_status;

use crate::autonomy::AutonomyLevel;
use crate::convergence_guard::{ConvergenceGuard, GuardConfig};
use crate::skill::Skill;

/// Returns all Phase 7 git skills as boxed trait objects.
///
/// Read-only skills are wrapped with permissive `ConvergenceGuard` settings
/// (available up to Level 4, full autonomy). Write skills are wrapped with
/// stricter settings (max Level 2, confirmation required, 50-action budget).
pub fn all_git_skills() -> Vec<Box<dyn Skill>> {
    vec![
        // ── Read-only skills (always available) ─────────────────────
        Box::new(ConvergenceGuard::new(
            git_status::GitStatusSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            git_diff::GitDiffSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            git_log::GitLogSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        // ── Write skills (gated) ────────────────────────────────────
        Box::new(ConvergenceGuard::new(
            git_branch::GitBranchSkill,
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(50),
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            git_commit::GitCommitSkill,
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(50),
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            git_stash::GitStashSkill,
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(50),
                ..Default::default()
            },
        )),
    ]
}

/// Open a git repository from the given path.
///
/// If `repo_path` is provided, opens directly. Otherwise, discovers the
/// repository by walking up from the current working directory.
pub(crate) fn open_repo(repo_path: Option<&str>) -> Result<git2::Repository, crate::skill::SkillError> {
    let repo = match repo_path {
        Some(path) => git2::Repository::open(path),
        None => git2::Repository::discover("."),
    };
    repo.map_err(|e| {
        crate::skill::SkillError::InvalidInput(format!(
            "cannot open git repository: {e}"
        ))
    })
}

/// Format a `git2::Status` bitflags value into a human-readable string.
pub(crate) fn format_status(status: git2::Status) -> &'static str {
    if status.is_index_new() || status.is_wt_new() {
        "new"
    } else if status.is_index_modified() || status.is_wt_modified() {
        "modified"
    } else if status.is_index_deleted() || status.is_wt_deleted() {
        "deleted"
    } else if status.is_index_renamed() || status.is_wt_renamed() {
        "renamed"
    } else if status.is_index_typechange() || status.is_wt_typechange() {
        "typechange"
    } else if status.is_conflicted() {
        "conflicted"
    } else if status.is_ignored() {
        "ignored"
    } else {
        "unknown"
    }
}

/// Collect the set of human-readable status flag names.
pub(crate) fn status_flags(status: git2::Status) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if status.is_index_new() { flags.push("index_new"); }
    if status.is_index_modified() { flags.push("index_modified"); }
    if status.is_index_deleted() { flags.push("index_deleted"); }
    if status.is_index_renamed() { flags.push("index_renamed"); }
    if status.is_index_typechange() { flags.push("index_typechange"); }
    if status.is_wt_new() { flags.push("wt_new"); }
    if status.is_wt_modified() { flags.push("wt_modified"); }
    if status.is_wt_deleted() { flags.push("wt_deleted"); }
    if status.is_wt_renamed() { flags.push("wt_renamed"); }
    if status.is_wt_typechange() { flags.push("wt_typechange"); }
    if status.is_conflicted() { flags.push("conflicted"); }
    if status.is_ignored() { flags.push("ignored"); }
    flags
}
