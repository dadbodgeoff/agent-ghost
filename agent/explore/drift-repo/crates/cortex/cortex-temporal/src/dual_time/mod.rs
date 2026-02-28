//! Dual-time (bitemporal) validation and correction.

pub mod correction;
pub mod late_arrival;
pub mod validation;

pub use correction::apply_temporal_correction;
pub use late_arrival::handle_late_arriving_fact;
pub use validation::{validate_temporal_bounds, validate_transaction_time_immutability};
