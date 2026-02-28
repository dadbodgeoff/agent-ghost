//! Materialized views module — create, query, diff, and auto-refresh.

pub mod auto_refresh;
pub mod create;
pub mod query;

pub use auto_refresh::AutoRefreshScheduler;
pub use create::create_materialized_view;
pub use query::{diff_views, get_view, list_views};
