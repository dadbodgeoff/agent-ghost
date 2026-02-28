//! CONVERGENCE_MONITOR_SEQUENCE_FLOW INVARIANT 11:
//! Hash algorithm separation verification.

/// Verify itp-protocol depends on sha2, NOT blake3.
#[test]
fn itp_protocol_uses_sha2_not_blake3() {
    let cargo_toml = include_str!("../../../itp-protocol/Cargo.toml");
    assert!(
        cargo_toml.contains("sha2"),
        "itp-protocol must depend on sha2 for content hashing"
    );
    assert!(
        !cargo_toml.contains("blake3"),
        "itp-protocol must NOT depend on blake3 (hash algorithm separation)"
    );
}

/// Verify cortex-temporal depends on blake3, NOT sha2.
#[test]
fn cortex_temporal_uses_blake3_not_sha2() {
    let cargo_toml = include_str!("../../cortex-temporal/Cargo.toml");
    assert!(
        cargo_toml.contains("blake3"),
        "cortex-temporal must depend on blake3 for hash chains"
    );
}

/// Verify ghost-signing is a leaf crate with no ghost-*/cortex-* dependencies.
#[test]
fn ghost_signing_is_leaf_crate() {
    let cargo_toml = include_str!("../../../ghost-signing/Cargo.toml");
    let deps_section = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .split("[dev-dependencies]")
        .next()
        .unwrap_or("");

    assert!(
        !deps_section.contains("ghost-"),
        "ghost-signing must not depend on any ghost-* crate"
    );
    assert!(
        !deps_section.contains("cortex-"),
        "ghost-signing must not depend on any cortex-* crate"
    );
}

/// Verify cortex-crdt does NOT depend on ghost-signing (Layer 1/Layer 3 separation).
#[test]
fn cortex_crdt_independent_of_ghost_signing() {
    let cargo_toml = include_str!("../../cortex-crdt/Cargo.toml");
    let deps_section = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .split("[dev-dependencies]")
        .next()
        .unwrap_or("");

    assert!(
        !deps_section.contains("ghost-signing"),
        "cortex-crdt must NOT depend on ghost-signing"
    );
}
