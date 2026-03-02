//! CLI subcommand implementations (Task 6.6).

// Core infrastructure (Phase 0)
pub mod error;
pub mod output;
pub mod confirm;
pub mod auth;
pub mod http_client;
pub mod backend;

// Signal handling (T-X.4)
pub mod signal;

// Existing commands
pub mod chat;
pub mod status;
pub mod commands;

// Phase 0: completions
pub mod completions;

// Phase 1+ stubs
pub mod init;
pub mod doctor;
pub mod logs;
pub mod agent;
pub mod safety;
pub mod config_cmd;
pub mod db;
pub mod audit_cmd;
pub mod convergence;
pub mod session;
pub mod identity;
pub mod secret;
pub mod policy;
// Phase 4: mesh, skills, channels, heartbeat, cron
pub mod mesh;
pub mod skill;
pub mod channel;
pub mod heartbeat;
pub mod cron;
