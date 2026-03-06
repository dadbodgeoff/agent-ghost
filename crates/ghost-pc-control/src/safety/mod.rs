//! Safety infrastructure for PC control skills.
//!
//! Defense-in-depth layers:
//!
//! 1. Convergence Gate (via `ConvergenceGuard` — external)
//! 2. App Allowlist (via `ConvergenceGuard` — external)
//! 3. Screen Safe Zone (`InputValidator`)
//! 4. Action Budget (via `ConvergenceGuard` — external)
//! 5. Blocked Actions (`InputValidator`)
//! 6. Circuit Breaker (`PcControlCircuitBreaker`)
//! 7. Kill Switch (external — `ghost-gateway`)
//! 8. Audit Trail (`pc_control_actions` table)

pub mod circuit_breaker;
pub mod config;
pub mod input_validator;

pub use circuit_breaker::PcControlCircuitBreaker;
pub use config::PcControlConfig;
pub use input_validator::{InputValidator, ScreenRegion, ValidationResult};
