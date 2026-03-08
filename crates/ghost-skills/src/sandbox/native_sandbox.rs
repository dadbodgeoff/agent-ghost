//! Native sandbox for builtin skills (Req 23 AC3).
//!
//! Native skills still run in-process, so the containment boundary must be
//! explicit and fail-closed. This module makes that boundary visible by
//! requiring each native execution path to declare an audited containment
//! profile and a capability allowlist.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeContainmentMode {
    ReadOnly,
    Transactional,
    HostInteraction,
}

#[derive(Debug, Clone)]
pub struct NativeContainmentProfile {
    pub mode: NativeContainmentMode,
    pub audited: bool,
    pub allowed_capabilities: HashSet<String>,
}

impl NativeContainmentProfile {
    pub fn new(
        mode: NativeContainmentMode,
        audited: bool,
        allowed_capabilities: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            mode,
            audited,
            allowed_capabilities: allowed_capabilities.into_iter().collect(),
        }
    }
}

/// Native sandbox for builtin skills.
pub struct NativeSandbox {
    granted_capabilities: HashSet<String>,
    mode: NativeContainmentMode,
    audited: bool,
}

impl NativeSandbox {
    pub fn new(capabilities: HashSet<String>) -> Self {
        Self {
            granted_capabilities: capabilities,
            mode: NativeContainmentMode::Transactional,
            audited: false,
        }
    }

    pub fn from_profile(profile: &NativeContainmentProfile) -> Result<Self, NativeSandboxError> {
        if profile.mode == NativeContainmentMode::HostInteraction && !profile.audited {
            return Err(NativeSandboxError::UnauditedHostInteraction);
        }

        Ok(Self {
            granted_capabilities: profile.allowed_capabilities.clone(),
            mode: profile.mode,
            audited: profile.audited,
        })
    }

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

    pub fn validate_tool_call(
        &self,
        tool_name: &str,
        required_capability: &str,
    ) -> Result<(), NativeSandboxError> {
        self.check_capability(required_capability)
            .map_err(|_| NativeSandboxError::ToolDenied {
                tool: tool_name.into(),
                required_capability: required_capability.into(),
            })
    }

    pub fn mode(&self) -> NativeContainmentMode {
        self.mode
    }

    pub fn audited(&self) -> bool {
        self.audited
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
    UnauditedHostInteraction,
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
            Self::UnauditedHostInteraction => {
                write!(
                    f,
                    "host-interacting native execution requires an audited profile"
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
