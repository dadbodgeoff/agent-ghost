//! macOS window management backend using AppleScript via `osascript`.
//!
//! Uses `System Events` for window enumeration, focus, and resize.
//! Uses `open -a` for app launching.
//!
//! Requires the "Accessibility" permission in System Settings > Privacy.

#![cfg(target_os = "macos")]

use std::process::Command;

use crate::platform::window_backend::{LaunchResult, WindowBackend, WindowInfo};

pub struct MacOsWindowBackend;

impl MacOsWindowBackend {
    pub fn new() -> Self { Self }
}

impl Default for MacOsWindowBackend {
    fn default() -> Self { Self::new() }
}

impl WindowBackend for MacOsWindowBackend {
    fn list_windows(&self, app_filter: Option<&str>) -> Result<Vec<WindowInfo>, String> {
        let script = r#"
            set output to ""
            tell application "System Events"
                repeat with proc in (every process whose visible is true)
                    set procName to name of proc
                    set procPID to unix id of proc
                    try
                        repeat with win in (every window of proc)
                            set winName to name of win
                            set winPos to position of win
                            set winSize to size of win
                            set output to output & procName & "|" & winName & "|" & procPID & "|" & (item 1 of winPos) & "|" & (item 2 of winPos) & "|" & (item 1 of winSize) & "|" & (item 2 of winSize) & linefeed
                        end repeat
                    end try
                end repeat
            end tell
            return output
        "#;

        let output = run_osascript(script)?;
        let mut windows = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 7 { continue; }

            let app = parts[0].to_string();

            // Apply filter if provided.
            if let Some(filter) = app_filter {
                if !app.to_lowercase().contains(&filter.to_lowercase()) {
                    continue;
                }
            }

            windows.push(WindowInfo {
                app,
                title: parts[1].to_string(),
                pid: parts[2].parse().unwrap_or(0),
                x: parts[3].parse().unwrap_or(0),
                y: parts[4].parse().unwrap_or(0),
                width: parts[5].parse().unwrap_or(0),
                height: parts[6].parse().unwrap_or(0),
            });
        }

        Ok(windows)
    }

    fn focus_window(
        &self,
        title: Option<&str>,
        app: Option<&str>,
        pid: Option<u32>,
    ) -> Result<WindowInfo, String> {
        // First, resolve the app name.
        let app_name = if let Some(a) = app {
            a.to_string()
        } else if let Some(p) = pid {
            resolve_app_name_by_pid(p)?
        } else if let Some(t) = title {
            resolve_app_name_by_title(t)?
        } else {
            return Err("at least one of title, app, or pid required".into());
        };

        // Activate the app and optionally focus a specific window by title.
        if let Some(t) = title {
            let script = format!(
                r#"
                tell application "System Events"
                    set proc to first process whose name is "{app_name}"
                    try
                        perform action "AXRaise" of (first window of proc whose name contains "{t}")
                    end try
                    set frontmost of proc to true
                end tell
                "#
            );
            run_osascript(&script)?;
        } else {
            let script = format!(
                r#"tell application "{app_name}" to activate"#
            );
            run_osascript(&script)?;
        }

        // Query the now-focused window info.
        let script = format!(
            r#"
            tell application "System Events"
                set proc to first process whose name is "{app_name}"
                set win to front window of proc
                set winName to name of win
                set winPos to position of win
                set winSize to size of win
                set procPID to unix id of proc
                return "{app_name}" & "|" & winName & "|" & procPID & "|" & (item 1 of winPos) & "|" & (item 2 of winPos) & "|" & (item 1 of winSize) & "|" & (item 2 of winSize)
            end tell
            "#
        );

        let output = run_osascript(&script)?;
        parse_window_line(&output).ok_or_else(|| "failed to parse focused window info".into())
    }

    fn resize_window(
        &self,
        title: Option<&str>,
        app: Option<&str>,
        x: Option<i32>,
        y: Option<i32>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<WindowInfo, String> {
        let app_name = app.or(title)
            .ok_or("at least one of 'title' or 'app' must be provided")?;

        let mut commands = Vec::new();
        if x.is_some() || y.is_some() {
            let pos_x = x.unwrap_or(0);
            let pos_y = y.unwrap_or(0);
            commands.push(format!("set position of win to {{{pos_x}, {pos_y}}}"));
        }
        if width.is_some() || height.is_some() {
            let w = width.unwrap_or(800);
            let h = height.unwrap_or(600);
            commands.push(format!("set size of win to {{{w}, {h}}}"));
        }

        let win_selector = if let Some(t) = title {
            format!(r#"first window of proc whose name contains "{t}""#)
        } else {
            "front window of proc".to_string()
        };

        let cmds = commands.join("\n                        ");

        let script = format!(
            r#"
            tell application "System Events"
                set proc to first process whose name is "{app_name}"
                set win to {win_selector}
                {cmds}
                set winName to name of win
                set winPos to position of win
                set winSize to size of win
                set procPID to unix id of proc
                return "{app_name}" & "|" & winName & "|" & procPID & "|" & (item 1 of winPos) & "|" & (item 2 of winPos) & "|" & (item 1 of winSize) & "|" & (item 2 of winSize)
            end tell
            "#
        );

        let output = run_osascript(&script)?;
        parse_window_line(&output).ok_or_else(|| "failed to parse resized window info".into())
    }

    fn launch_app(&self, app: &str, args: &[String]) -> Result<LaunchResult, String> {
        // Use `open -a` for app bundles, fall back to direct path.
        let mut cmd = Command::new("open");
        cmd.arg("-a").arg(app);

        if !args.is_empty() {
            cmd.arg("--args");
            for arg in args {
                cmd.arg(arg);
            }
        }

        let output = cmd.output().map_err(|e| format!("failed to launch app: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("open -a '{app}' failed: {stderr}"));
        }

        // Try to get the PID of the launched app.
        let pid = get_pid_for_app(app).unwrap_or(0);

        Ok(LaunchResult {
            pid,
            app: app.to_string(),
        })
    }
}

/// Run an AppleScript via `osascript` and return stdout.
fn run_osascript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("osascript failed to execute: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Check for common permission error.
        if stderr.contains("not allowed assistive access") || stderr.contains("osascript is not allowed") {
            return Err(
                "Accessibility permission required. Enable it in System Settings > Privacy & Security > Accessibility.".into()
            );
        }
        return Err(format!("osascript error: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Parse a pipe-delimited window info line.
fn parse_window_line(line: &str) -> Option<WindowInfo> {
    let parts: Vec<&str> = line.trim().split('|').collect();
    if parts.len() < 7 { return None; }
    Some(WindowInfo {
        app: parts[0].to_string(),
        title: parts[1].to_string(),
        pid: parts[2].parse().ok()?,
        x: parts[3].parse().ok()?,
        y: parts[4].parse().ok()?,
        width: parts[5].parse().ok()?,
        height: parts[6].parse().ok()?,
    })
}

/// Resolve an app name from a PID using System Events.
fn resolve_app_name_by_pid(pid: u32) -> Result<String, String> {
    let script = format!(
        r#"tell application "System Events" to return name of first process whose unix id is {pid}"#
    );
    run_osascript(&script)
}

/// Resolve an app name from a window title.
fn resolve_app_name_by_title(title: &str) -> Result<String, String> {
    let script = format!(
        r#"
        tell application "System Events"
            repeat with proc in (every process whose visible is true)
                try
                    repeat with win in (every window of proc)
                        if name of win contains "{title}" then
                            return name of proc
                        end if
                    end repeat
                end try
            end repeat
        end tell
        return "not found"
        "#
    );
    let result = run_osascript(&script)?;
    if result == "not found" {
        Err(format!("no window found with title containing '{title}'"))
    } else {
        Ok(result)
    }
}

/// Get PID for a named app using pgrep.
fn get_pid_for_app(app: &str) -> Option<u32> {
    let output = Command::new("pgrep")
        .arg("-x")
        .arg(app)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next()?.trim().parse().ok()
}
