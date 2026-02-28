//! # cortex-session
//!
//! Per-session memory tracking with deduplication.
//! Filters already-sent memories for 30–50% token savings.
//! Concurrent access via `DashMap`.
//!
//! ## Modules
//!
//! - `manager` — `SessionManager` with `DashMap` for concurrent access
//! - `context` — `SessionContext` with loaded sets and token tracking
//! - `deduplication` — Filter already-sent memories
//! - `analytics` — Per-session retrieval and intent analytics
//! - `efficiency` — Token efficiency metrics
//! - `cleanup` — Session lifecycle and stale session removal

pub mod analytics;
pub mod cleanup;
pub mod context;
pub mod deduplication;
pub mod efficiency;
pub mod manager;

pub use analytics::SessionAnalytics;
pub use cleanup::{cleanup_old_sessions, cleanup_stale_sessions};
pub use context::SessionContext;
pub use deduplication::{filter_duplicates, DeduplicationResult};
pub use efficiency::TokenEfficiency;
pub use manager::SessionManager;
