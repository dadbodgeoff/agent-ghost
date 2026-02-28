//! Adversarial test suites for GHOST platform (Task 7.3).
//!
//! Each suite validates that attacks are detected/blocked.
//! Failures indicate security gaps.

#[path = "adversarial/unicode_bypass.rs"]
mod unicode_bypass;
#[path = "adversarial/proposal_adversarial.rs"]
mod proposal_adversarial;
#[path = "adversarial/kill_switch_race.rs"]
mod kill_switch_race;
#[path = "adversarial/compaction_under_load.rs"]
mod compaction_under_load;
#[path = "adversarial/credential_exfil_patterns.rs"]
mod credential_exfil_patterns;
#[path = "adversarial/convergence_manipulation.rs"]
mod convergence_manipulation;
