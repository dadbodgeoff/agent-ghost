//! # ghost-identity
//!
//! Soul document management, identity loading, Ed25519 keypair lifecycle,
//! CORP_POLICY signature verification, and identity drift detection.

pub mod corp_policy;
pub mod drift_detector;
pub mod identity_manager;
pub mod keypair_manager;
pub mod soul_manager;
pub mod user;
