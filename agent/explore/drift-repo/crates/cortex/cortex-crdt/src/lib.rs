//! # cortex-crdt
//!
//! CRDT (Conflict-free Replicated Data Type) primitives for multi-agent memory
//! convergence. Provides mathematically proven data structures that guarantee
//! eventual consistency across concurrent agent modifications.
//!
//! ## CRDT Primitives
//!
//! - [`VectorClock`] — Causal ordering primitive (happens-before, concurrent detection)
//! - [`GCounter`] — Grow-only counter (per-agent counts, merge = per-agent max)
//! - [`LWWRegister`] — Last-writer-wins register (timestamp + agent_id tie-breaking)
//! - [`MVRegister`] — Multi-value register (preserves concurrent values for manual resolution)
//! - [`ORSet`] — Observed-remove set (add-wins semantics)
//! - [`MaxRegister`] — Max-wins register (value only increases)
//!
//! ## Higher-Level Structures
//!
//! - [`MemoryCRDT`] — Per-field CRDT wrapper for `BaseMemory`
//! - [`FieldDelta`] — Per-field change descriptors for delta sync
//! - [`MergeEngine`] — Stateless merge orchestrator with causal ordering validation
//! - [`CausalGraphCRDT`] — DAG CRDT with cycle prevention for the causal graph
//!
//! ## Mathematical Guarantees
//!
//! All CRDT merge operations satisfy:
//! 1. **Commutativity**: `merge(A, B) == merge(B, A)`
//! 2. **Associativity**: `merge(A, merge(B, C)) == merge(merge(A, B), C)`
//! 3. **Idempotency**: `merge(A, A) == A`

pub mod clock;
pub mod graph;
pub mod memory;
pub mod primitives;
pub mod signing;

// Re-export public API
pub use clock::VectorClock;
pub use graph::CausalGraphCRDT;
pub use memory::{FieldDelta, MemoryCRDT, MemoryDelta, MergeEngine};
pub use primitives::{GCounter, LWWRegister, MVRegister, MaxRegister, ORSet, UniqueTag};
pub use signing::{KeyRegistry, SignedDelta, SignedDeltaVerifier};
