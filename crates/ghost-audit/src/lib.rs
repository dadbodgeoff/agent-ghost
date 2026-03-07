//! ghost-audit — queryable audit log engine (Req 30 AC1, AC2).
//!
//! Provides paginated queries, aggregation summaries, and multi-format export
//! over the append-only audit tables managed by cortex-storage.

pub mod aggregation;
pub mod export;
pub mod query_engine;

pub use aggregation::{AggregationResult, AuditAggregation};
pub use export::{AuditExporter, ExportFormat};
pub use query_engine::{AuditEntry, AuditFilter, AuditQueryEngine};
