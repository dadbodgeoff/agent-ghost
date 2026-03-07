//! Shared signal handling helpers for streaming CLI commands (T-X.4).
//!
//! Provides consistent patterns for Ctrl+C handling across all streaming
//! and interactive commands.
//!
//! ## WebSocket streaming commands
//!
//! `ghost logs` and `ghost audit tail` use the async pattern:
//! ```ignore
//! tokio::select! {
//!     _ = tokio::signal::ctrl_c() => {
//!         let _ = write.send(Message::Close(None)).await;
//!         eprintln!("\nDisconnected.");
//!         return Ok(());
//!     }
//!     msg = read.next() => { /* handle */ }
//! }
//! ```
//!
//! ## Interactive REPL commands
//!
//! `ghost chat` wraps blocking stdin reads with `spawn_blocking` inside
//! a `tokio::select!` so that Ctrl+C is handled by the async runtime.
//!
//! ## Convention
//!
//! All streaming commands MUST:
//! 1. Clean up resources on Ctrl+C (send WS Close frame, flush buffers)
//! 2. Print a goodbye/disconnect message to stderr (not stdout)
//! 3. Return `Ok(())` on clean shutdown (not `Err`)

use std::io::{self, BufRead};

/// Read a line from stdin in a blocking-safe manner compatible with tokio::select!.
///
/// Returns `None` on EOF (Ctrl+D) or I/O error, `Some(line)` otherwise.
pub async fn read_line_async() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        let stdin = io::stdin();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => None, // EOF (Ctrl+D)
            Ok(_) => Some(line),
            Err(_) => None,
        }
    })
    .await
    .ok()
    .flatten()
}
