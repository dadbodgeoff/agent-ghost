//! Skill registry, execution framework, and WASM sandbox (Req 23).
//!
//! This crate provides:
//! - `Skill` trait — the core execution interface for all skills
//! - `AutonomyLevel` — 4-position dial controlling agent latitude
//! - `ConvergenceGuard` — decorator wrapping skills with safety checks
//! - Safety skills (Phase 5) — platform-managed convergence safety
//! - Skill registry, credential broker, workflow recording
//! - WASM sandbox for user-installed skills

pub mod artifact;
pub mod autonomy;
pub mod bridges;
pub mod bundled_skills;
pub mod code_analysis;
pub mod convergence_guard;
pub mod credential;
pub mod delegation_skills;
pub mod git_skills;
pub mod proposer;
pub mod recorder;
pub mod registry;
pub mod safety_skills;
pub mod sandbox;
pub mod skill;
