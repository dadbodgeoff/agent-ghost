//! ghost-audit — queryable audit log engine (Req 30 AC1, AC2).
//!
//! Provides paginated queries, aggregation summaries, and multi-format export
//! over the append-only audit tables managed by cortex-storage.

pub mod query_engine;
pub mod aggregation;
pub mod export;

pub use query_engine::{AuditFilter, AuditQueryEngine, AuditEntry};
pub use aggregation::{AuditAggregation, AggregationResult};
pub use export::{ExportFormat, AuditExporter};
