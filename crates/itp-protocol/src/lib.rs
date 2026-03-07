//! # itp-protocol
//!
//! Interaction Telemetry Protocol — event schema, privacy levels,
//! and transports (JSONL, optional OTel).
//!
//! Uses SHA-256 for content hashing (privacy). blake3 is for hash chains only.

pub mod adapter;
pub mod events;
pub mod privacy;
pub mod transport;
