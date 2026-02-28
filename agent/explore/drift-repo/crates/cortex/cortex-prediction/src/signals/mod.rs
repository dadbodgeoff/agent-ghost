//! Signal types for predictive memory preloading.
//!
//! Four signal categories feed into prediction strategies:
//! - File signals: active file, imports, symbols, directory
//! - Temporal signals: time of day, day of week, session duration
//! - Behavioral signals: recent queries, intents, frequent memories
//! - Git signals: branch name, modified files, commit messages

pub mod behavioral_signals;
pub mod file_signals;
pub mod git_signals;
pub mod temporal_signals;

pub use behavioral_signals::BehavioralSignals;
pub use file_signals::FileSignals;
pub use git_signals::GitSignals;
pub use temporal_signals::TemporalSignals;

use serde::{Deserialize, Serialize};

/// Aggregated signals from all sources, used by prediction strategies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregatedSignals {
    pub file: FileSignals,
    pub temporal: TemporalSignals,
    pub behavioral: BehavioralSignals,
    pub git: GitSignals,
}
