//! # ghost-identity
//!
//! Soul document management, identity loading, Ed25519 keypair lifecycle,
//! CORP_POLICY signature verification, and identity drift detection.

pub mod soul_manager;
pub mod identity_manager;
pub mod corp_policy;
pub mod keypair_manager;
pub mod drift_detector;
pub mod user;
