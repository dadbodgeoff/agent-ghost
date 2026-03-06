//! Abstraction over window management for testability.
//!
//! The `WindowBackend` trait provides a uniform interface for window
//! enumeration, focus, resize, and app launch. Platform-specific
//! backends implement this trait.
//!
//! Production uses `MacOsWindowBackend`; tests use `MockWindowBackend`.

use std::sync::{Arc, Mutex};

/// Information about a visible window.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub title: String,
    pub app: String,
    pub pid: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Result of a successful app launch.
#[derive(Debug, Clone)]
pub struct LaunchResult {
    pub pid: u32,
    pub app: String,
}

/// Trait abstracting window management operations.
///
/// Production uses platform-specific backends; tests use `MockWindowBackend`.
pub trait WindowBackend: Send + Sync {
    /// List all visible windows, optionally filtered by app name.
    fn list_windows(&self, app_filter: Option<&str>) -> Result<Vec<WindowInfo>, String>;

    /// Bring a window to the foreground by title, app, or PID.
    fn focus_window(
        &self,
        title: Option<&str>,
        app: Option<&str>,
        pid: Option<u32>,
    ) -> Result<WindowInfo, String>;

    /// Resize and/or move a window.
    fn resize_window(
        &self,
        title: Option<&str>,
        app: Option<&str>,
        x: Option<i32>,
        y: Option<i32>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<WindowInfo, String>;

    /// Launch an application by name or path.
    fn launch_app(&self, app: &str, args: &[String]) -> Result<LaunchResult, String>;
}

/// Actions recorded by `MockWindowBackend` for test assertions.
#[derive(Debug, Clone, PartialEq)]
pub enum MockWindowAction {
    ListWindows(Option<String>),
    FocusWindow {
        title: Option<String>,
        app: Option<String>,
        pid: Option<u32>,
    },
    ResizeWindow {
        title: Option<String>,
        app: Option<String>,
        x: Option<i32>,
        y: Option<i32>,
        width: Option<u32>,
        height: Option<u32>,
    },
    LaunchApp(String, Vec<String>),
}

/// Mock window backend for tests.
///
/// Returns pre-configured windows and records all calls for assertions.
#[derive(Debug, Clone)]
pub struct MockWindowBackend {
    windows: Vec<WindowInfo>,
    actions: Arc<Mutex<Vec<MockWindowAction>>>,
}

impl MockWindowBackend {
    pub fn new(windows: Vec<WindowInfo>) -> Self {
        Self {
            windows,
            actions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn actions(&self) -> Vec<MockWindowAction> {
        self.actions.lock().unwrap().clone()
    }
}

impl WindowBackend for MockWindowBackend {
    fn list_windows(&self, app_filter: Option<&str>) -> Result<Vec<WindowInfo>, String> {
        self.actions.lock().unwrap().push(MockWindowAction::ListWindows(
            app_filter.map(|s| s.to_string()),
        ));
        let filtered: Vec<WindowInfo> = if let Some(filter) = app_filter {
            let f = filter.to_lowercase();
            self.windows.iter()
                .filter(|w| w.app.to_lowercase().contains(&f))
                .cloned()
                .collect()
        } else {
            self.windows.clone()
        };
        Ok(filtered)
    }

    fn focus_window(
        &self,
        title: Option<&str>,
        app: Option<&str>,
        pid: Option<u32>,
    ) -> Result<WindowInfo, String> {
        self.actions.lock().unwrap().push(MockWindowAction::FocusWindow {
            title: title.map(|s| s.to_string()),
            app: app.map(|s| s.to_string()),
            pid,
        });
        // Return the first matching window.
        let win = self.windows.iter().find(|w| {
            title.map_or(true, |t| w.title.contains(t))
                && app.map_or(true, |a| w.app == a)
                && pid.map_or(true, |p| w.pid == p)
        });
        win.cloned().ok_or_else(|| "no matching window found".into())
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
        self.actions.lock().unwrap().push(MockWindowAction::ResizeWindow {
            title: title.map(|s| s.to_string()),
            app: app.map(|s| s.to_string()),
            x, y, width, height,
        });
        let win = self.windows.first()
            .ok_or_else(|| "no windows available".to_string())?;
        Ok(WindowInfo {
            title: win.title.clone(),
            app: win.app.clone(),
            pid: win.pid,
            x: x.unwrap_or(win.x),
            y: y.unwrap_or(win.y),
            width: width.unwrap_or(win.width),
            height: height.unwrap_or(win.height),
        })
    }

    fn launch_app(&self, app: &str, args: &[String]) -> Result<LaunchResult, String> {
        self.actions.lock().unwrap().push(MockWindowAction::LaunchApp(
            app.to_string(),
            args.to_vec(),
        ));
        Ok(LaunchResult {
            pid: 99999,
            app: app.to_string(),
        })
    }
}
