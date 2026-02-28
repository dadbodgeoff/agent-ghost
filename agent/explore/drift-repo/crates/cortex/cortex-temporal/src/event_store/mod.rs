//! Event store: append-only event log with replay, upcasting, and compaction.

pub mod append;
pub mod compaction;
pub mod query;
pub mod replay;
pub mod upcaster;
