//! # cortex-validation
//!
//! 7-dimension proposal validation gate.
//! D1-D4: base validation (citation, temporal, contradiction, pattern alignment)
//! D5: scope expansion
//! D6: self-reference density
//! D7: emulation language detection

pub mod dimensions;
pub mod proposal_validator;
