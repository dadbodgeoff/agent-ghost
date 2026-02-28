//! Memory-level CRDT structures.
//!
//! Wraps every `BaseMemory` field in the appropriate CRDT type, providing
//! per-field merge semantics and delta computation for efficient sync.

pub mod field_delta;
pub mod memory_crdt;
pub mod merge_engine;

pub use field_delta::FieldDelta;
pub use memory_crdt::MemoryCRDT;
pub use merge_engine::{MemoryDelta, MergeEngine};
