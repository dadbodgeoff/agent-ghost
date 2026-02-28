use crate::errors::CortexResult;
use crate::models::HealthReport;

/// System health reporting.
pub trait IHealthReporter: Send + Sync {
    /// Generate a comprehensive health report for all subsystems.
    fn report(&self) -> CortexResult<HealthReport>;
}
