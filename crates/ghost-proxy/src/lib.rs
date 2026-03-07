//! ghost-proxy — local HTTPS proxy for passive convergence monitoring (Req 36).
//!
//! Intercepts traffic to AI chat platforms, parses streaming responses,
//! and emits ITP events to the convergence monitor. Never modifies traffic.

pub mod domain_filter;
pub mod emitter;
pub mod parsers;
pub mod server;

pub use domain_filter::DomainFilter;
pub use emitter::ProxyITPEmitter;
pub use server::ProxyServer;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("connection error: {0}")]
    ConnectionError(String),
}

pub type ProxyResult<T> = Result<T, ProxyError>;
