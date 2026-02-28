//! Heartbeat engine + cron engine (Req 34).
//!
//! HeartbeatEngine: configurable interval, convergence-aware frequency,
//! dedicated session, synthetic message.
//!
//! CronEngine: standard cron syntax, timezone-aware, per-job cost tracking.

pub mod cron;
pub mod heartbeat;
