//! Safety types shared across all layers.
//!
//! Lives in cortex-core (Layer 1A) so that Layer 3 crates can emit
//! trigger events without depending on ghost-gateway (Layer 4).

pub mod trigger;
