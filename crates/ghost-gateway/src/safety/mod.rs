//! Safety subsystem: kill switch, auto-triggers, quarantine, notifications,
//! distributed kill gates.

pub mod auto_triggers;
pub mod kill_gate_bridge;
pub mod kill_switch;
pub mod notification;
pub mod quarantine;
