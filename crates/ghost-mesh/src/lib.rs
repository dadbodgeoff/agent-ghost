//! ClawMesh Agent-to-Agent Payment Protocol (Phase 9 — Deferred).
//!
//! This crate defines trait boundaries and type stubs for the ClawMesh
//! inter-agent payment protocol. No runtime implementation is provided.
//!
//! # Feature Gate
//!
//! This crate is feature-gated behind `mesh`. Enable it in dependents with:
//!
//! ```toml
//! ghost-mesh = { workspace = true, features = ["mesh"] }
//! ```

pub mod protocol;
pub mod traits;
pub mod types;

pub use protocol::{MeshMessage, MeshProtocol};
pub use traits::{IMeshLedger, IMeshProvider, MeshError};
pub use types::{
    MeshEscrow, MeshInvoice, MeshReceipt, MeshSettlement, MeshTransaction, MeshWallet,
    SettlementStatus,
};
