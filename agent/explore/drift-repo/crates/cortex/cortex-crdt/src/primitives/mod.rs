//! CRDT primitive data structures.
//!
//! Five conflict-free replicated data types, each with mathematically proven
//! convergence properties (commutativity, associativity, idempotency).

pub mod gcounter;
pub mod lww_register;
pub mod max_register;
pub mod mv_register;
pub mod or_set;

pub use gcounter::GCounter;
pub use lww_register::LWWRegister;
pub use max_register::MaxRegister;
pub use mv_register::MVRegister;
pub use or_set::{ORSet, UniqueTag};
