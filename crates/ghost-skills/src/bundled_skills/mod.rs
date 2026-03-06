//! Phase 8: First bundled skills.
//!
//! User-installable skills distributed through a curated registry.
//! All skills are wrapped with `ConvergenceGuard` for safety gating.
//!
//! ## Categories
//!
//! | Category       | Skills                                              |
//! |----------------|-----------------------------------------------------|
//! | Productivity   | `note_take`, `timer_set`, `calendar_check`, `email_draft` |
//! | Research       | `arxiv_search`, `github_search`, `doc_summarize`    |
//! | Data           | `csv_analyze`, `json_transform`, `sqlite_query`     |
//!
//! ## Risk Classification
//!
//! | Skill            | Risk     | Autonomy Default       | Convergence Max |
//! |------------------|----------|------------------------|-----------------|
//! | `note_take`      | Low      | Act with Confirmation  | Level 3         |
//! | `timer_set`      | Low      | Act with Confirmation  | Level 3         |
//! | `calendar_check` | Read     | Act Autonomously       | Level 4         |
//! | `email_draft`    | Medium   | Act with Confirmation  | Level 2         |
//! | `arxiv_search`   | Read     | Act Autonomously       | Level 4         |
//! | `github_search`  | Read     | Act Autonomously       | Level 4         |
//! | `doc_summarize`  | Read     | Act Autonomously       | Level 4         |
//! | `csv_analyze`    | Read     | Act Autonomously       | Level 4         |
//! | `json_transform` | Read     | Act Autonomously       | Level 4         |
//! | `sqlite_query`   | Read     | Act Autonomously       | Level 4         |

pub mod arxiv_search;
pub mod calendar_check;
pub mod csv_analyze;
pub mod doc_summarize;
pub mod email_draft;
pub mod github_search;
pub mod json_transform;
pub mod note_take;
pub mod sqlite_query;
pub mod timer_set;

use crate::autonomy::AutonomyLevel;
use crate::convergence_guard::{ConvergenceGuard, GuardConfig};
use crate::skill::Skill;

/// Returns all Phase 8 bundled skills as boxed trait objects.
///
/// Each skill is wrapped with `ConvergenceGuard` using risk-appropriate
/// settings. Read-only skills get permissive guards (Level 4, autonomous).
/// Write skills get stricter guards (Level 2-3, confirmation required).
pub fn all_bundled_skills() -> Vec<Box<dyn Skill>> {
    vec![
        // ── Productivity ──────────────────────────────────────────────
        Box::new(ConvergenceGuard::new(
            note_take::NoteTakeSkill,
            GuardConfig {
                max_convergence_level: 3,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(100),
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            timer_set::TimerSetSkill,
            GuardConfig {
                max_convergence_level: 3,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(50),
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            calendar_check::CalendarCheckSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            email_draft::EmailDraftSkill,
            GuardConfig {
                max_convergence_level: 2,
                autonomy_level: AutonomyLevel::ActWithConfirmation,
                action_budget: Some(20),
                ..Default::default()
            },
        )),
        // ── Research ──────────────────────────────────────────────────
        Box::new(ConvergenceGuard::new(
            arxiv_search::ArxivSearchSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            github_search::GithubSearchSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            doc_summarize::DocSummarizeSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        // ── Data ──────────────────────────────────────────────────────
        Box::new(ConvergenceGuard::new(
            csv_analyze::CsvAnalyzeSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            json_transform::JsonTransformSkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
        Box::new(ConvergenceGuard::new(
            sqlite_query::SqliteQuerySkill,
            GuardConfig {
                max_convergence_level: 4,
                autonomy_level: AutonomyLevel::ActAutonomously,
                ..Default::default()
            },
        )),
    ]
}
