//! Epistemic status module — determination, transitions, and confidence aggregation.

pub mod aggregation;
pub mod status;
pub mod transitions;

pub use aggregation::aggregate_confidence;
pub use status::determine_initial_status;
pub use transitions::{demote_to_stale, promote_to_provisional, promote_to_verified};
