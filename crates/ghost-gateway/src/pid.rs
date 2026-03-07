//! PID file protocol for gateway lifecycle management.
//!
//! Prevents port conflicts by detecting stale processes before startup.
//! The PID file at `~/.ghost/data/gateway.pid` records the running
//! gateway's PID, port, and start time.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Information stored in the PID file.
#[derive(Debug, Serialize, Deserialize)]
pub struct PidInfo {
    pub pid: u32,
    pub port: u16,
    pub started_at: String,
}

/// Action the caller should take after pre-launch check.
#[derive(Debug)]
pub enum PreLaunchAction {
    /// No existing process — safe to start.
    ProceedWithStartup,
    /// Healthy gateway already running at this URL — reuse it.
    ReuseExisting { url: String },
    /// Stale PID file (process dead) — cleaned up, safe to start.
    CleanedStaleProcess { old_pid: u32 },
    /// Process alive but unresponsive — killed and cleaned up.
    KilledUnresponsive { old_pid: u32 },
}

/// Returns the canonical PID file path: `~/.ghost/data/gateway.pid`.
pub fn pid_file_path() -> PathBuf {
    let home = crate::bootstrap::shellexpand_tilde("~/.ghost/data/gateway.pid");
    PathBuf::from(home)
}

/// Reads and parses the PID file. Returns `None` if missing or corrupt.
/// Corrupt files are deleted automatically.
pub fn read_pid_file() -> Option<PidInfo> {
    let path = pid_file_path();
    let content = std::fs::read_to_string(&path).ok()?;
    if content.trim().is_empty() {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    match serde_json::from_str::<PidInfo>(&content) {
        Ok(info) if info.pid > 0 => Some(info),
        _ => {
            tracing::warn!(path = %path.display(), "Corrupt PID file — removing");
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// Writes the PID file atomically (write to .tmp, then rename).
/// WP4-E: Acquires an exclusive flock on the PID file to detect stale processes.
pub fn write_pid_file(port: u16) -> std::io::Result<()> {
    let path = pid_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let info = PidInfo {
        pid: std::process::id(),
        port,
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string_pretty(&info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let tmp_path = path.with_extension("pid.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &path)?;

    // WP4-E: Acquire exclusive flock — held for process lifetime.
    // This allows other processes to detect stale PIDs via non-blocking flock.
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::File::open(&path)?;
        let fd = file.as_raw_fd();
        let rc = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            // Leak the file handle so the lock is held for the process lifetime.
            std::mem::forget(file);
        } else {
            tracing::warn!("Failed to acquire flock on PID file — another instance may hold it");
        }
    }
    Ok(())
}

/// WP4-E: Check if the PID file is locked (process holding it is alive).
/// Returns true if the flock is held (process alive), false if stale.
#[cfg(unix)]
pub fn is_pid_file_locked() -> bool {
    use std::os::unix::io::AsRawFd;
    let path = pid_file_path();
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let fd = file.as_raw_fd();
    // Try non-blocking exclusive lock. If it succeeds, the previous process is dead.
    let rc = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        // We got the lock — previous process is dead. Release it.
        unsafe { libc::flock(fd, libc::LOCK_UN) };
        false
    } else {
        // Lock is held — process is alive.
        true
    }
}

#[cfg(not(unix))]
pub fn is_pid_file_locked() -> bool {
    false // Conservative: assume not locked on non-Unix
}

/// Removes the PID file. Safe to call multiple times.
pub fn remove_pid_file() {
    let path = pid_file_path();
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

/// Checks if a process is alive using `kill(pid, 0)`.
#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    false // Conservative: assume dead on non-Unix
}

/// Sends SIGTERM to a process.
#[cfg(unix)]
fn send_sigterm(pid: u32) {
    unsafe { libc::kill(pid as i32, libc::SIGTERM); }
}

#[cfg(not(unix))]
fn send_sigterm(_pid: u32) {}

/// Sends SIGKILL to a process.
#[cfg(unix)]
fn send_sigkill(pid: u32) {
    unsafe { libc::kill(pid as i32, libc::SIGKILL); }
}

#[cfg(not(unix))]
fn send_sigkill(_pid: u32) {}

/// Full pre-launch check: read PID file, check if process alive,
/// verify health, kill if unresponsive.
pub async fn pre_launch_check(expected_port: u16) -> PreLaunchAction {
    let info = match read_pid_file() {
        Some(info) => info,
        None => return PreLaunchAction::ProceedWithStartup,
    };

    let old_pid = info.pid;

    // WP4-E: Check flock first — more reliable than kill(pid, 0) after SIGKILL.
    if !is_pid_file_locked() && !is_process_alive(old_pid) {
        tracing::info!(pid = old_pid, "Stale PID file — process is dead (flock released)");
        remove_pid_file();
        return PreLaunchAction::CleanedStaleProcess { old_pid };
    }

    // Check if the recorded process is still alive.
    if !is_process_alive(old_pid) {
        tracing::info!(pid = old_pid, "Stale PID file — process is dead");
        remove_pid_file();
        return PreLaunchAction::CleanedStaleProcess { old_pid };
    }

    // Process is alive. Check if it's serving on the expected port.
    let health_url = format!("http://127.0.0.1:{}/api/health", expected_port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_default();

    if let Ok(resp) = client.get(&health_url).send().await {
        if resp.status().is_success() {
            return PreLaunchAction::ReuseExisting {
                url: format!("http://127.0.0.1:{}", expected_port),
            };
        }
    }

    // Process alive but not responding — kill it.
    tracing::warn!(pid = old_pid, "Gateway process alive but unresponsive — sending SIGTERM");
    send_sigterm(old_pid);

    // Wait up to 3 seconds for graceful exit.
    for _ in 0..12 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if !is_process_alive(old_pid) {
            remove_pid_file();
            return PreLaunchAction::KilledUnresponsive { old_pid };
        }
    }

    // Force kill.
    tracing::warn!(pid = old_pid, "Gateway process did not exit — sending SIGKILL");
    send_sigkill(old_pid);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    remove_pid_file();

    PreLaunchAction::KilledUnresponsive { old_pid }
}
