//! Abstraction over platform accessibility tree APIs for testability.
//!
//! The `AccessibilityBackend` trait provides a uniform interface for
//! querying UI elements from the platform accessibility tree.
//!
//! Production uses platform-specific backends (e.g., `MacOsAccessibilityBackend`);
//! tests use `MockAccessibilityBackend`.

use std::sync::{Arc, Mutex};

/// A node from the platform accessibility tree.
#[derive(Debug, Clone)]
pub struct AccessibilityNode {
    pub role: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub value: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub enabled: bool,
}

/// Trait abstracting platform accessibility tree queries.
pub trait AccessibilityBackend: Send + Sync {
    /// Query the accessibility tree, optionally filtered by window, role, or text.
    fn query(
        &self,
        window: Option<&str>,
        role: Option<&str>,
        query: Option<&str>,
        max_depth: u32,
    ) -> Result<Vec<AccessibilityNode>, String>;
}

/// Stub backend for unsupported platforms. Returns an error.
pub struct StubAccessibilityBackend;

impl AccessibilityBackend for StubAccessibilityBackend {
    fn query(
        &self,
        _window: Option<&str>,
        _role: Option<&str>,
        _query: Option<&str>,
        _max_depth: u32,
    ) -> Result<Vec<AccessibilityNode>, String> {
        Err("accessibility tree not supported on this platform".into())
    }
}

/// Actions recorded by `MockAccessibilityBackend` for test assertions.
#[derive(Debug, Clone, PartialEq)]
pub struct MockAccessibilityQuery {
    pub window: Option<String>,
    pub role: Option<String>,
    pub query: Option<String>,
    pub max_depth: u32,
}

/// Mock accessibility backend for tests.
pub struct MockAccessibilityBackend {
    nodes: Vec<AccessibilityNode>,
    queries: Arc<Mutex<Vec<MockAccessibilityQuery>>>,
}

impl MockAccessibilityBackend {
    pub fn new(nodes: Vec<AccessibilityNode>) -> Self {
        Self {
            nodes,
            queries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn queries(&self) -> Vec<MockAccessibilityQuery> {
        self.queries.lock().unwrap().clone()
    }
}

impl AccessibilityBackend for MockAccessibilityBackend {
    fn query(
        &self,
        window: Option<&str>,
        role: Option<&str>,
        query: Option<&str>,
        max_depth: u32,
    ) -> Result<Vec<AccessibilityNode>, String> {
        self.queries.lock().unwrap().push(MockAccessibilityQuery {
            window: window.map(|s| s.to_string()),
            role: role.map(|s| s.to_string()),
            query: query.map(|s| s.to_string()),
            max_depth,
        });

        let filtered: Vec<AccessibilityNode> = self
            .nodes
            .iter()
            .filter(|n| {
                role.map_or(true, |r| n.role.to_lowercase() == r.to_lowercase())
                    && query.map_or(true, |q| {
                        let q_lower = q.to_lowercase();
                        n.name
                            .as_ref()
                            .map_or(false, |name| name.to_lowercase().contains(&q_lower))
                            || n.title
                                .as_ref()
                                .map_or(false, |t| t.to_lowercase().contains(&q_lower))
                            || n.value
                                .as_ref()
                                .map_or(false, |v| v.to_lowercase().contains(&q_lower))
                    })
            })
            .cloned()
            .collect();

        Ok(filtered)
    }
}
