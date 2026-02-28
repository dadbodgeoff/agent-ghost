use std::sync::Arc;

use crate::errors::CortexResult;
use crate::memory::{
    BaseMemory, ConstraintLink, FileLink, FunctionLink, Importance, MemoryType, PatternLink,
    RelationshipEdge, RelationshipType,
};
use chrono::{DateTime, Utc};

/// Full CRUD + bulk + query + vector + bitemporal + relationships + links + aggregation + maintenance.
pub trait IMemoryStorage: Send + Sync {
    // --- CRUD ---
    fn create(&self, memory: &BaseMemory) -> CortexResult<()>;
    fn get(&self, id: &str) -> CortexResult<Option<BaseMemory>>;
    fn update(&self, memory: &BaseMemory) -> CortexResult<()>;
    fn delete(&self, id: &str) -> CortexResult<()>;

    // --- Bulk ---
    fn create_bulk(&self, memories: &[BaseMemory]) -> CortexResult<usize>;
    fn get_bulk(&self, ids: &[String]) -> CortexResult<Vec<BaseMemory>>;

    // --- Query ---
    fn query_by_type(&self, memory_type: MemoryType) -> CortexResult<Vec<BaseMemory>>;
    fn query_by_importance(&self, min: Importance) -> CortexResult<Vec<BaseMemory>>;
    fn query_by_confidence_range(&self, min: f64, max: f64) -> CortexResult<Vec<BaseMemory>>;
    fn query_by_date_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CortexResult<Vec<BaseMemory>>;
    fn query_by_tags(&self, tags: &[String]) -> CortexResult<Vec<BaseMemory>>;

    // --- Search ---
    fn search_fts5(&self, query: &str, limit: usize) -> CortexResult<Vec<BaseMemory>>;
    fn search_vector(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> CortexResult<Vec<(BaseMemory, f64)>>;

    // --- Relationships ---
    fn get_relationships(
        &self,
        memory_id: &str,
        rel_type: Option<RelationshipType>,
    ) -> CortexResult<Vec<RelationshipEdge>>;
    fn add_relationship(&self, edge: &RelationshipEdge) -> CortexResult<()>;
    fn remove_relationship(&self, source_id: &str, target_id: &str) -> CortexResult<()>;

    // --- Links ---
    fn add_pattern_link(&self, memory_id: &str, link: &PatternLink) -> CortexResult<()>;
    fn add_constraint_link(&self, memory_id: &str, link: &ConstraintLink) -> CortexResult<()>;
    fn add_file_link(&self, memory_id: &str, link: &FileLink) -> CortexResult<()>;
    fn add_function_link(&self, memory_id: &str, link: &FunctionLink) -> CortexResult<()>;

    // --- Aggregation ---
    fn count_by_type(&self) -> CortexResult<Vec<(MemoryType, usize)>>;
    fn average_confidence(&self) -> CortexResult<f64>;
    fn stale_count(&self, threshold_days: u64) -> CortexResult<usize>;

    // --- Maintenance ---
    fn vacuum(&self) -> CortexResult<()>;
}

/// Blanket impl: `Arc<T>` implements `IMemoryStorage` by delegating to the inner `T`.
/// This allows `Arc<StorageEngine>` to be used transparently wherever `&dyn IMemoryStorage` is needed.
impl<T: IMemoryStorage> IMemoryStorage for Arc<T> {
    fn create(&self, memory: &BaseMemory) -> CortexResult<()> { (**self).create(memory) }
    fn get(&self, id: &str) -> CortexResult<Option<BaseMemory>> { (**self).get(id) }
    fn update(&self, memory: &BaseMemory) -> CortexResult<()> { (**self).update(memory) }
    fn delete(&self, id: &str) -> CortexResult<()> { (**self).delete(id) }
    fn create_bulk(&self, memories: &[BaseMemory]) -> CortexResult<usize> { (**self).create_bulk(memories) }
    fn get_bulk(&self, ids: &[String]) -> CortexResult<Vec<BaseMemory>> { (**self).get_bulk(ids) }
    fn query_by_type(&self, memory_type: MemoryType) -> CortexResult<Vec<BaseMemory>> { (**self).query_by_type(memory_type) }
    fn query_by_importance(&self, min: Importance) -> CortexResult<Vec<BaseMemory>> { (**self).query_by_importance(min) }
    fn query_by_confidence_range(&self, min: f64, max: f64) -> CortexResult<Vec<BaseMemory>> { (**self).query_by_confidence_range(min, max) }
    fn query_by_date_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> CortexResult<Vec<BaseMemory>> { (**self).query_by_date_range(from, to) }
    fn query_by_tags(&self, tags: &[String]) -> CortexResult<Vec<BaseMemory>> { (**self).query_by_tags(tags) }
    fn search_fts5(&self, query: &str, limit: usize) -> CortexResult<Vec<BaseMemory>> { (**self).search_fts5(query, limit) }
    fn search_vector(&self, embedding: &[f32], limit: usize) -> CortexResult<Vec<(BaseMemory, f64)>> { (**self).search_vector(embedding, limit) }
    fn get_relationships(&self, memory_id: &str, rel_type: Option<RelationshipType>) -> CortexResult<Vec<RelationshipEdge>> { (**self).get_relationships(memory_id, rel_type) }
    fn add_relationship(&self, edge: &RelationshipEdge) -> CortexResult<()> { (**self).add_relationship(edge) }
    fn remove_relationship(&self, source_id: &str, target_id: &str) -> CortexResult<()> { (**self).remove_relationship(source_id, target_id) }
    fn add_pattern_link(&self, memory_id: &str, link: &PatternLink) -> CortexResult<()> { (**self).add_pattern_link(memory_id, link) }
    fn add_constraint_link(&self, memory_id: &str, link: &ConstraintLink) -> CortexResult<()> { (**self).add_constraint_link(memory_id, link) }
    fn add_file_link(&self, memory_id: &str, link: &FileLink) -> CortexResult<()> { (**self).add_file_link(memory_id, link) }
    fn add_function_link(&self, memory_id: &str, link: &FunctionLink) -> CortexResult<()> { (**self).add_function_link(memory_id, link) }
    fn count_by_type(&self) -> CortexResult<Vec<(MemoryType, usize)>> { (**self).count_by_type() }
    fn average_confidence(&self) -> CortexResult<f64> { (**self).average_confidence() }
    fn stale_count(&self, threshold_days: u64) -> CortexResult<usize> { (**self).stale_count(threshold_days) }
    fn vacuum(&self) -> CortexResult<()> { (**self).vacuum() }
}
