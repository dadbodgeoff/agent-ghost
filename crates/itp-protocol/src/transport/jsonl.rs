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
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
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
            .read(true)
            .open(&path)?;

        // Advisory lock for concurrent write safety (cross-platform)
        lock_exclusive(&file)?;

        let json = serde_json::to_string(event).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        writeln!(file, "{}", json)?;

        unlock(&file)?;
        Ok(())
    }
}

// ── Cross-platform file locking ─────────────────────────────────────────

#[cfg(unix)]
fn lock_exclusive(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn unlock(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    let ret = unsafe { libc::flock(fd, libc::LOCK_UN) };
    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn lock_exclusive(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        LockFileEx, LOCKFILE_EXCLUSIVE_LOCK,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let handle = file.as_raw_handle();
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let ret = unsafe {
        LockFileEx(handle, LOCKFILE_EXCLUSIVE_LOCK, 0, u32::MAX, u32::MAX, &mut overlapped)
    };
    if ret == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn unlock(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let handle = file.as_raw_handle();
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let ret = unsafe {
        UnlockFileEx(handle, 0, u32::MAX, u32::MAX, &mut overlapped)
    };
    if ret == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

impl ITPAdapter for JsonlTransport {
    fn on_session_start(&self, event: &SessionStartEvent) {
        let itp = ITPEvent::SessionStart(event.clone());
        if let Err(e) = self.append_event(&event.session_id, &itp) {
            tracing::error!(session_id = %event.session_id, error = %e, "failed to persist ITP SessionStart event");
        }
    }

    fn on_message(&self, event: &InteractionMessageEvent) {
        let itp = ITPEvent::InteractionMessage(event.clone());
        if let Err(e) = self.append_event(&event.session_id, &itp) {
            tracing::error!(session_id = %event.session_id, error = %e, "failed to persist ITP InteractionMessage event");
        }
    }

    fn on_session_end(&self, event: &SessionEndEvent) {
        let itp = ITPEvent::SessionEnd(event.clone());
        if let Err(e) = self.append_event(&event.session_id, &itp) {
            tracing::error!(session_id = %event.session_id, error = %e, "failed to persist ITP SessionEnd event");
        }
    }

    fn on_agent_state(&self, event: &AgentStateSnapshotEvent) {
        let itp = ITPEvent::AgentStateSnapshot(event.clone());
        if let Err(e) = self.append_event(&event.session_id, &itp) {
            tracing::error!(session_id = %event.session_id, error = %e, "failed to persist ITP AgentStateSnapshot event");
        }
    }
}
