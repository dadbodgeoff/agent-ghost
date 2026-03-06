//! macOS accessibility tree backend using AppleScript + System Events.
//!
//! Queries the platform accessibility tree via `System Events` AppleScript.
//! This approach trades raw speed for simplicity — avoids complex CoreFoundation
//! FFI while providing reliable access to UI elements.
//!
//! Requires the "Accessibility" permission in System Settings > Privacy.

#![cfg(target_os = "macos")]

use std::process::Command;

use crate::platform::accessibility_backend::{AccessibilityBackend, AccessibilityNode};

pub struct MacOsAccessibilityBackend;

impl MacOsAccessibilityBackend {
    pub fn new() -> Self { Self }
}

impl Default for MacOsAccessibilityBackend {
    fn default() -> Self { Self::new() }
}

impl AccessibilityBackend for MacOsAccessibilityBackend {
    fn query(
        &self,
        window: Option<&str>,
        role: Option<&str>,
        query: Option<&str>,
        max_depth: u32,
    ) -> Result<Vec<AccessibilityNode>, String> {
        let depth = max_depth.min(10); // Cap depth to avoid runaway scripts.

        // Determine which process to query.
        let process_clause = if let Some(w) = window {
            format!(r#"first process whose name is "{w}""#)
        } else {
            "first process whose frontmost is true".to_string()
        };

        let script = format!(
            r#"
            use AppleScript version "2.4"
            use framework "Foundation"

            set output to ""

            tell application "System Events"
                try
                    set proc to {process_clause}
                    set procPID to unix id of proc
                    tell proc
                        try
                            set wins to every window
                            if (count of wins) > 0 then
                                set win to item 1 of wins
                                my walkElements(win, 0, {depth})
                            end if
                        end try
                    end tell
                on error errMsg
                    return "ERROR:" & errMsg
                end try
            end tell

            return my getOutput()

            property _output : ""

            on walkElements(elem, currentDepth, maxDepth)
                if currentDepth > maxDepth then return
                try
                    set elemRole to role of elem
                    set elemName to ""
                    try
                        set elemName to name of elem
                    end try
                    set elemTitle to ""
                    try
                        set elemTitle to title of elem
                    end try
                    set elemValue to ""
                    try
                        set elemValue to value of elem as text
                    end try
                    set elemEnabled to true
                    try
                        set elemEnabled to enabled of elem
                    end try
                    set elemPos to {{0, 0}}
                    try
                        set elemPos to position of elem
                    end try
                    set elemSize to {{0, 0}}
                    try
                        set elemSize to size of elem
                    end try

                    set posX to item 1 of elemPos
                    set posY to item 2 of elemPos
                    set sizeW to item 1 of elemSize
                    set sizeH to item 2 of elemSize

                    set my _output to my _output & elemRole & "|" & elemName & "|" & elemTitle & "|" & elemValue & "|" & posX & "|" & posY & "|" & sizeW & "|" & sizeH & "|" & elemEnabled & linefeed

                    try
                        set children to every UI element of elem
                        repeat with child in children
                            my walkElements(child, currentDepth + 1, maxDepth)
                        end repeat
                    end try
                end try
            end walkElements

            on getOutput()
                return my _output
            end getOutput
            "#
        );

        let output = run_osascript(&script)?;

        if output.starts_with("ERROR:") {
            return Err(output[6..].trim().to_string());
        }

        let mut nodes = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            if let Some(node) = parse_accessibility_line(line) {
                // Apply role filter.
                if let Some(r) = role {
                    if !node.role.to_lowercase().contains(&r.to_lowercase()) {
                        continue;
                    }
                }
                // Apply text query filter.
                if let Some(q) = query {
                    let q_lower = q.to_lowercase();
                    let matches = node.name.as_ref().map_or(false, |n| n.to_lowercase().contains(&q_lower))
                        || node.title.as_ref().map_or(false, |t| t.to_lowercase().contains(&q_lower))
                        || node.value.as_ref().map_or(false, |v| v.to_lowercase().contains(&q_lower));
                    if !matches {
                        continue;
                    }
                }
                nodes.push(node);
            }
        }

        Ok(nodes)
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
        if stderr.contains("not allowed assistive access") || stderr.contains("osascript is not allowed") {
            return Err(
                "Accessibility permission required. Enable it in System Settings > Privacy & Security > Accessibility.".into()
            );
        }
        return Err(format!("osascript error: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Parse a pipe-delimited accessibility node line.
/// Format: role|name|title|value|x|y|width|height|enabled
fn parse_accessibility_line(line: &str) -> Option<AccessibilityNode> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() < 9 { return None; }

    Some(AccessibilityNode {
        role: parts[0].to_string(),
        name: non_empty(parts[1]),
        title: non_empty(parts[2]),
        value: non_empty(parts[3]),
        x: parts[4].parse().unwrap_or(0),
        y: parts[5].parse().unwrap_or(0),
        width: parts[6].parse().unwrap_or(0),
        height: parts[7].parse().unwrap_or(0),
        enabled: parts[8].to_lowercase() == "true",
    })
}

fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "missing value" {
        None
    } else {
        Some(trimmed.to_string())
    }
}
