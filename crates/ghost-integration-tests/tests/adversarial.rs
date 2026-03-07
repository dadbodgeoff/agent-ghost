//! Adversarial test suites for GHOST platform (Task 7.3).
//!
//! Each suite validates that attacks are detected/blocked.
//! Failures indicate security gaps.

#[path = "adversarial/calibration_cold_start.rs"]
mod calibration_cold_start;
#[path = "adversarial/compaction_under_load.rs"]
mod compaction_under_load;
#[path = "adversarial/convergence_manipulation.rs"]
mod convergence_manipulation;
#[path = "adversarial/crdt_merge_conflict.rs"]
mod crdt_merge_conflict;
#[path = "adversarial/credential_exfil_patterns.rs"]
mod credential_exfil_patterns;
#[path = "adversarial/dual_signing_path_audit.rs"]
mod dual_signing_path_audit;
#[path = "adversarial/export_baseline_poisoning.rs"]
mod export_baseline_poisoning;
#[path = "adversarial/kill_gate_adversarial.rs"]
mod kill_gate_adversarial;
#[path = "adversarial/kill_gate_quorum_race.rs"]
mod kill_gate_quorum_race;
#[path = "adversarial/kill_switch_race.rs"]
mod kill_switch_race;
#[path = "adversarial/mesh_crdt_sybil_interaction.rs"]
mod mesh_crdt_sybil_interaction;
#[path = "adversarial/orchestrator_adversarial.rs"]
mod orchestrator_adversarial;
#[path = "adversarial/proposal_adversarial.rs"]
mod proposal_adversarial;
#[path = "adversarial/proxy_passthrough_stress.rs"]
mod proxy_passthrough_stress;
#[path = "adversarial/temporal_sybil_reregistration.rs"]
mod temporal_sybil_reregistration;
#[path = "adversarial/unicode_bypass.rs"]
mod unicode_bypass;
