//! Timeline reconstruction from parsed messages (Req 35 AC3).

use std::collections::BTreeMap;

use crate::NormalizedMessage;

/// A reconstructed session with boundaries.
#[derive(Debug, Clone)]
pub struct ReconstructedSession {
    pub session_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub messages: Vec<NormalizedMessage>,
    pub message_count: usize,
}

/// Reconstructs session boundaries from a flat list of messages.
pub struct TimelineReconstructor {
    /// Maximum gap between messages before a new session is inferred (seconds).
    pub session_gap_threshold: i64,
}

impl Default for TimelineReconstructor {
    fn default() -> Self {
        Self {
            session_gap_threshold: 3600, // 1 hour
        }
    }
}

impl TimelineReconstructor {
    pub fn new(gap_threshold_secs: i64) -> Self {
        Self {
            session_gap_threshold: gap_threshold_secs,
        }
    }

    /// Reconstruct sessions from messages, inferring boundaries from timestamps.
    pub fn reconstruct(&self, messages: &[NormalizedMessage]) -> Vec<ReconstructedSession> {
        if messages.is_empty() {
            return Vec::new();
        }

        // Group by explicit session_id first
        let mut by_session: BTreeMap<String, Vec<NormalizedMessage>> = BTreeMap::new();
        let mut unassigned = Vec::new();

        for msg in messages {
            if let Some(ref sid) = msg.session_id {
                by_session.entry(sid.clone()).or_default().push(msg.clone());
            } else {
                unassigned.push(msg.clone());
            }
        }

        // For unassigned messages, infer sessions from timestamp gaps
        if !unassigned.is_empty() {
            let mut sorted = unassigned;
            sorted.sort_by_key(|m| m.timestamp);

            let mut current_session = Vec::new();
            let mut session_counter = 0u64;

            for msg in sorted {
                if let Some(last) = current_session.last() {
                    let gap = (msg.timestamp - last_timestamp(last)).num_seconds();
                    if gap > self.session_gap_threshold {
                        // Start new session
                        let sid = format!("inferred-{}", session_counter);
                        by_session.insert(sid, std::mem::take(&mut current_session));
                        session_counter += 1;
                    }
                }
                current_session.push(msg);
            }
            if !current_session.is_empty() {
                let sid = format!("inferred-{}", session_counter);
                by_session.insert(sid, current_session);
            }
        }

        // Build ReconstructedSession for each group
        let mut sessions: Vec<ReconstructedSession> = by_session
            .into_iter()
            .map(|(sid, mut msgs)| {
                msgs.sort_by_key(|m| m.timestamp);
                let start = msgs.first().map(|m| m.timestamp).unwrap_or_default();
                let end = msgs.last().map(|m| m.timestamp).unwrap_or_default();
                let count = msgs.len();
                ReconstructedSession {
                    session_id: sid,
                    start,
                    end,
                    messages: msgs,
                    message_count: count,
                }
            })
            .collect();

        sessions.sort_by_key(|s| s.start);
        sessions
    }
}

fn last_timestamp(msg: &NormalizedMessage) -> chrono::DateTime<chrono::Utc> {
    msg.timestamp
}
