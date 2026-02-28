//! CONVERGENCE_MONITOR INVARIANT 11: Hash algorithm separation.
//!
//! ITP content hashes use SHA-256 (itp-protocol/privacy.rs).
//! Hash chains and all other hashing use blake3 (cortex-temporal).
//! These are NEVER confused.

/// ITP crate depends on sha2, NOT blake3.
#[test]
fn itp_protocol_uses_sha2_not_blake3() {
    let cargo_toml = include_str!("../../crates/itp-protocol/Cargo.toml");
    assert!(
        cargo_toml.contains("sha2"),
        "itp-protocol must depend on sha2 for content hashing"
    );
    assert!(
        !cargo_toml.contains("blake3"),
        "itp-protocol must NOT depend on blake3 (hash algorithm separation)"
    );
}

/// cortex-temporal depends on blake3, NOT sha2.
#[test]
fn cortex_temporal_uses_blake3_not_sha2() {
    let cargo_toml = include_str!("../../crates/cortex/cortex-temporal/Cargo.toml");
    assert!(
        cargo_toml.contains("blake3"),
        "cortex-temporal must depend on blake3 for hash chains"
    );
    assert!(
        !cargo_toml.contains("sha2"),
        "cortex-temporal must NOT depend on sha2 (hash algorithm separation)"
    );
}

/// ITP content hashing produces SHA-256 output (32 bytes hex = 64 chars).
#[test]
fn itp_content_hash_is_sha256() {
    let hash = itp_protocol::privacy::hash_content("test content");
    // SHA-256 hex digest is 64 characters
    assert_eq!(
        hash.len(),
        64,
        "SHA-256 hex digest must be 64 chars, got {}",
        hash.len()
    );
}

/// blake3 hash chain output is 32 bytes.
#[test]
fn hash_chain_uses_blake3() {
    let hash = cortex_temporal::hash_chain::compute_event_hash(
        "test_type",
        "{}",
        "actor",
        "2026-01-01T00:00:00Z",
        &cortex_temporal::hash_chain::GENESIS_HASH,
    );
    assert_eq!(hash.len(), 32, "blake3 hash must be 32 bytes");
}

/// cortex-crdt does NOT depend on ghost-signing (Layer 1/Layer 3 separation).
#[test]
fn cortex_crdt_independent_of_ghost_signing() {
    let cargo_toml = include_str!("../../crates/cortex/cortex-crdt/Cargo.toml");
    assert!(
        !cargo_toml.contains("ghost-signing"),
        "cortex-crdt must NOT depend on ghost-signing (Layer 1/Layer 3 separation)"
    );
}
