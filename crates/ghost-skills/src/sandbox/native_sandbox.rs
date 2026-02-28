//! Native sandbox for builtin skills (Req 23 AC3).
//!
//! Builtin skills run in-process but are still capability-scoped
//! at the Rust API level. No WASM overhead for trusted code.

use std::collections::HashSet;

/// Native sandbox for builtin skills.
///
/// Unlike WasmSandbox, native skills run in the gateway process.
/// Capability validation happens at the Rust API boundary rather
/// than at the WASM import level.
pub struct NativeSandbox {
    granted_capabilities: HashSet<String>,
}

impl NativeSandbox {
    pub fn new(capabilities: HashSet<String>) -> Self {
        Self {
            granted_capabilities: capabilities,
        }
    }

    /// Check if a capability is granted before allowing a native API call.
    pub fn check_capability(&self, capability: &str) -> Result<(), NativeSandboxError> {
        if self.granted_capabilities.contains(capability) {
            Ok(())
        } else {
            Err(NativeSandboxError::CapabilityDenied {
                requested: capability.into(),
                granted: self.granted_capabilities.iter().cloned().collect(),
            })
        }
    }

    /// Validate that a tool call is within the granted capability scope.
    pub fn validate_tool_call(
        &self,
        tool_name: &str,
        required_capability: &str,
    ) -> Result<(), NativeSandboxError> {
        self.check_capability(required_capability).map_err(|_| {
            NativeSandboxError::ToolDenied {
                tool: tool_name.into(),
                required_capability: required_capability.into(),
            }
        })
    }

    pub fn granted_capabilities(&self) -> &HashSet<String> {
        &self.granted_capabilities
    }
}

/// Errors from native sandbox capability checks.
#[derive(Debug, Clone)]
pub enum NativeSandboxError {
    CapabilityDenied {
        requested: String,
        granted: Vec<String>,
    },
    ToolDenied {
        tool: String,
        required_capability: String,
    },
}

impl std::fmt::Display for NativeSandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CapabilityDenied { requested, granted } => {
                write!(
                    f,
                    "Capability '{}' denied. Granted: {:?}",
                    requested, granted
                )
            }
            Self::ToolDenied {
                tool,
                required_capability,
            } => {
                write!(
                    f,
                    "Tool '{}' denied: requires capability '{}'",
                    tool, required_capability
                )
            }
        }
    }
}

impl Default for NativeSandbox {
    fn default() -> Self {
        Self::new(HashSet::new())
    }
}
