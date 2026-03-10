//! ToolRegistry — register, lookup, schema generation, level-filtered schemas (A2.7).

use std::collections::{BTreeMap, BTreeSet};

use ghost_llm::provider::ToolSchema;

/// A registered tool with metadata.
#[derive(Debug, Clone)]
pub struct RegisteredTool {
    pub name: String,
    pub description: String,
    pub schema: ToolSchema,
    /// Required capability to invoke this tool.
    pub capability: String,
    /// Minimum intervention level at which this tool is hidden.
    /// 0 = always visible, 4 = hidden at L4 only, 5 = never hidden.
    pub hidden_at_level: u8,
    /// Timeout in seconds (default 30).
    pub timeout_secs: u64,
}

/// Registry of all available tools.
pub struct ToolRegistry {
    tools: BTreeMap<String, RegisteredTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    /// Register a tool.
    pub fn register(&mut self, tool: RegisteredTool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Look up a tool by name.
    pub fn lookup(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.get(name)
    }

    /// Get all tool schemas.
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema.clone()).collect()
    }

    /// Get tool schemas filtered by intervention level.
    /// Higher level → fewer tools exposed.
    pub fn schemas_filtered(&self, intervention_level: u8) -> Vec<ToolSchema> {
        self.tools
            .values()
            .filter(|t| t.hidden_at_level > intervention_level)
            .map(|t| t.schema.clone())
            .collect()
    }

    /// Get all registered tool names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Collect the unique capability names required by the registered tools.
    pub fn required_capabilities(&self) -> Vec<String> {
        self.tools
            .values()
            .filter_map(|tool| {
                let capability = tool.capability.trim();
                (!capability.is_empty()).then(|| capability.to_string())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
