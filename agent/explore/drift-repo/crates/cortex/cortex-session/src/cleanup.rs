//! Session lifecycle management.
//!
//! Handles inactivity timeout, max duration, max tokens,
//! and deletion of sessions older than 7 days.

use chrono::Duration;

use crate::manager::SessionManager;

/// Default inactivity timeout: 1 hour.
pub const DEFAULT_INACTIVITY_TIMEOUT: Duration = Duration::hours(1);

/// Default max session duration: 24 hours.
pub const DEFAULT_MAX_DURATION: Duration = Duration::hours(24);

/// Default max session age for cleanup: 7 days.
pub const DEFAULT_MAX_AGE: Duration = Duration::days(7);

/// Default max tokens per session.
pub const DEFAULT_MAX_TOKENS: usize = 500_000;

/// Clean up stale sessions from the manager.
///
/// Removes sessions that are:
/// - Inactive for longer than `inactivity_timeout`
/// - Older than `max_age`
/// - Over the token budget
///
/// Returns the number of sessions removed.
pub fn cleanup_stale_sessions(
    manager: &SessionManager,
    inactivity_timeout: Duration,
    max_age: Duration,
    max_tokens: usize,
) -> usize {
    let session_ids = manager.session_ids();
    let mut removed = 0;

    for id in session_ids {
        let should_remove = manager
            .get_session(&id)
            .map(|ctx| {
                ctx.idle_duration() > inactivity_timeout
                    || ctx.session_duration() > max_age
                    || ctx.tokens_sent > max_tokens
            })
            .unwrap_or(false);

        if should_remove {
            manager.remove_session(&id);
            removed += 1;
        }
    }

    removed
}

/// Clean up sessions older than 7 days (convenience wrapper).
pub fn cleanup_old_sessions(manager: &SessionManager) -> usize {
    cleanup_stale_sessions(
        manager,
        DEFAULT_INACTIVITY_TIMEOUT,
        DEFAULT_MAX_AGE,
        DEFAULT_MAX_TOKENS,
    )
}
