//! Adversarial test suites for GHOST platform (Task 7.3).
//!
//! Each suite validates that attacks are detected/blocked.
//! Failures indicate security gaps.

mod unicode_bypass;
mod proposal_adversarial;
mod kill_switch_race;
mod compaction_under_load;
mod credential_exfil_patterns;
mod convergence_manipulation;
mod kill_gate_adversarial;
mod orchestrator_adversarial;
mod dual_signing_path_audit;
mod proxy_passthrough_stress;
mod mesh_crdt_sybil_interaction;
mod calibration_cold_start;
mod temporal_sybil_reregistration;
mod crdt_merge_conflict;
mod kill_gate_quorum_race;
mod export_baseline_poisoning;
