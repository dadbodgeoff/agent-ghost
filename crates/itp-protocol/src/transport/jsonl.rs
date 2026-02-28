//! JSONL file transport (Req 4 AC3).
//!
//! Writes per-session JSONL to ~/.ghost/sessions/{session_id}/events.jsonl

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::adapter::ITPAdapter;
use crate::events::*;

/// JSONL transport — appends one JSON line per event to a session file.
pub struct JsonlTransport {
    base_dir: PathBuf,
}

impl JsonlTransport {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Default path: ~/.ghost/sessions/
    pub fn default_path() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Self::new(PathBuf::from(home).join(".ghost/sessions"))
    }

    fn session_file(&self, session_id: &uuid::Uuid) -> PathBuf {
        self.base_dir
            .join(session_id.to_string())
            .join("events.jsonl")
    }

    fn append_event(&self, session_id: &uuid::Uuid, event: &ITPEvent) -> std::io::Result<()> {
        let path = self.session_file(session_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        // Advisory lock for concurrent write safety
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        unsafe {
            libc::flock(fd, libc::LOCK_EX);
        }
        let json = serde_json::to_string(event).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        writeln!(file, "{}", json)?;
        unsafe {
            libc::flock(fd, libc::LOCK_UN);
        }
        Ok(())
    }
}

impl ITPAdapter for JsonlTransport {
    fn on_session_start(&self, event: &SessionStartEvent) {
        let itp = ITPEvent::SessionStart(event.clone());
        let _ = self.append_event(&event.session_id, &itp);
    }

    fn on_message(&self, event: &InteractionMessageEvent) {
        let itp = ITPEvent::InteractionMessage(event.clone());
        let _ = self.append_event(&event.session_id, &itp);
    }

    fn on_session_end(&self, event: &SessionEndEvent) {
        let itp = ITPEvent::SessionEnd(event.clone());
        let _ = self.append_event(&event.session_id, &itp);
    }

    fn on_agent_state(&self, event: &AgentStateSnapshotEvent) {
        let itp = ITPEvent::AgentStateSnapshot(event.clone());
        let _ = self.append_event(&event.session_id, &itp);
    }
}
