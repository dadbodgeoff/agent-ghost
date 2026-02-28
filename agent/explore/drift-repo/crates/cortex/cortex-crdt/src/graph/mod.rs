//! DAG CRDT for the causal graph.
//!
//! A novel CRDT for directed acyclic graphs with cycle prevention.
//! Edges use OR-Set semantics (add-wins), strengths use MaxRegister.

pub mod dag_crdt;

pub use dag_crdt::CausalGraphCRDT;
