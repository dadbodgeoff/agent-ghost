//! ghost-gateway — the single long-running GHOST platform process.
//!
//! Owns agent lifecycle, routing, sessions, API server, kill switch,
//! inter-agent messaging, cost tracking, and channel adapters.

pub mod agents;
pub mod api;
pub mod auth;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod cost;
pub mod gateway;
pub mod health;
pub mod itp_buffer;
pub mod itp_router;
pub mod messaging;
pub mod periodic;
pub mod safety;
pub mod session;
pub mod shutdown;
