//! Agent-facing OAuth tools registered in the ToolRegistry.
//!
//! The agent interacts with OAuth connections via opaque ref_ids.
//! Raw tokens are NEVER exposed — the broker handles token injection.

use ghost_llm::provider::ToolSchema;

use super::registry::RegisteredTool;

/// Tool name constants.
pub const OAUTH_API_CALL: &str = "oauth_api_call";
pub const OAUTH_LIST_CONNECTIONS: &str = "oauth_list_connections";

/// Create the `oauth_api_call` tool registration.
///
/// Agent provides: ref_id, method, url, headers (optional), body (optional).
/// Broker injects Bearer token, executes, returns response.
pub fn oauth_api_call_tool() -> RegisteredTool {
    RegisteredTool {
        name: OAUTH_API_CALL.to_string(),
        description: "Execute an API call through an OAuth connection. The broker injects the Bearer token — you never see it.".to_string(),
        schema: ToolSchema {
            name: OAUTH_API_CALL.to_string(),
            description: "Execute an authenticated API call via OAuth broker".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "ref_id": {
                        "type": "string",
                        "description": "Opaque OAuth connection reference ID"
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                        "description": "HTTP method"
                    },
                    "url": {
                        "type": "string",
                        "description": "Target API URL"
                    },
                    "headers": {
                        "type": "object",
                        "description": "Additional HTTP headers (optional)",
                        "additionalProperties": { "type": "string" }
                    },
                    "body": {
                        "type": "string",
                        "description": "Request body (optional)"
                    }
                },
                "required": ["ref_id", "method", "url"]
            }),
        },
        capability: "oauth".to_string(),
        hidden_at_level: 3, // Hidden at intervention level 3+
        timeout_secs: 30,
    }
}

/// Create the `oauth_list_connections` tool registration.
///
/// Returns a list of active OAuth connections with ref_ids, provider names,
/// scopes, and status. No tokens are ever included.
pub fn oauth_list_connections_tool() -> RegisteredTool {
    RegisteredTool {
        name: OAUTH_LIST_CONNECTIONS.to_string(),
        description: "List active OAuth connections (ref_ids, providers, scopes, status). No tokens are shown.".to_string(),
        schema: ToolSchema {
            name: OAUTH_LIST_CONNECTIONS.to_string(),
            description: "List active OAuth connections".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        capability: "oauth".to_string(),
        hidden_at_level: 5, // Always visible (read-only)
        timeout_secs: 10,
    }
}
