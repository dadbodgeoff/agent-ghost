//! ghost-gateway — the single long-running GHOST platform process.
//!
//! Owns agent lifecycle, routing, sessions, API server, kill switch,
//! inter-agent messaging, cost tracking, and channel adapters.

pub mod agents;
pub mod api;
pub mod auth;
pub mod backup_scheduler;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod config_watcher;
pub mod convergence_watcher;
pub mod cost;
pub mod db_pool;
pub mod gateway;
pub mod health;
pub mod itp_buffer;
pub mod itp_router;
pub mod messaging;
pub mod periodic;
pub mod pid;
pub mod provider_runtime;
mod route_sets;
pub mod runtime;
pub mod runtime_safety;
pub mod runtime_status;
pub mod safety;
pub mod session;
pub mod shutdown;
pub mod skill_catalog;
pub mod skill_ingest;
pub mod state;

#[cfg(feature = "otel")]
pub mod otel;
