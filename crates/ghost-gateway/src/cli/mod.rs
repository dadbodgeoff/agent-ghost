//! CLI subcommand implementations (Task 6.6).

// Core infrastructure (Phase 0)
pub mod auth;
pub mod backend;
pub mod confirm;
pub mod error;
pub mod http_client;
pub mod output;

// Signal handling (T-X.4)
pub mod signal;

// Existing commands
pub mod chat;
pub mod commands;
pub mod status;

// Phase 0: completions
pub mod completions;

// Phase 1+ stubs
pub mod agent;
pub mod audit_cmd;
pub mod config_cmd;
pub mod convergence;
pub mod db;
pub mod doctor;
pub mod identity;
pub mod init;
pub mod logs;
pub mod policy;
pub mod safety;
pub mod secret;
pub mod session;
// Phase 4: mesh, skills, channels, heartbeat, cron
pub mod channel;
pub mod codex;
pub mod cron;
pub mod heartbeat;
pub mod mesh;
pub mod skill;
