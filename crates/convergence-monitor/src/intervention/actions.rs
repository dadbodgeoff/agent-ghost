//! Intervention actions per level.

use serde::{Deserialize, Serialize};

/// Actions taken at each intervention level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterventionAction {
    /// Level 0: log only, no notification.
    Level0LogOnly,
    /// Level 1: emit soft notification.
    Level1SoftNotification,
    /// Level 2: mandatory ack + scoring pause.
    Level2MandatoryAck,
    /// Level 3: session termination + 4h cooldown + contact notification.
    Level3SessionTermination,
    /// Level 4: block session creation + 24h cooldown + external confirmation.
    Level4ExternalEscalation,
}
