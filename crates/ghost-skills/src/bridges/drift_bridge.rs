//! DriftMCPBridge: register Drift MCP tools as first-party skills (A2.10).
//!
//! Discovers MCP tool definitions from a Drift server and wraps them
//! as SkillDefinitions so they appear in the skill registry alongside
//! native and WASM skills.

use serde::{Deserialize, Serialize};

/// A Drift MCP tool definition discovered from a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub server_url: String,
}

/// Bridge that registers Drift MCP tools as first-party skills.
pub struct DriftMCPBridge {
    server_url: String,
    tools: Vec<DriftToolDefinition>,
}

impl DriftMCPBridge {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            tools: Vec::new(),
        }
    }

    /// Discover tools from the Drift MCP server.
    /// In production, this calls the MCP `tools/list` endpoint.
    pub async fn discover(&mut self) -> Result<&[DriftToolDefinition], String> {
        tracing::info!(url = %self.server_url, "Discovering Drift MCP tools");
        // Placeholder: in production, connect to MCP server and list tools.
        // Each discovered tool is wrapped as a DriftToolDefinition.
        Ok(&self.tools)
    }

    /// Register a manually-defined tool (for testing or static config).
    pub fn register_tool(&mut self, tool: DriftToolDefinition) {
        self.tools.push(tool);
    }

    /// Get all registered tools.
    pub fn tools(&self) -> &[DriftToolDefinition] {
        &self.tools
    }

    /// Execute a tool call by proxying to the Drift MCP server.
    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| format!("Tool not found: {tool_name}"))?;

        tracing::debug!(
            tool = %tool.name,
            server = %tool.server_url,
            "Executing Drift MCP tool"
        );

        // Placeholder: in production, send MCP `tools/call` request.
        let _ = arguments;
        Ok(serde_json::json!({"status": "executed", "tool": tool_name}))
    }
}
