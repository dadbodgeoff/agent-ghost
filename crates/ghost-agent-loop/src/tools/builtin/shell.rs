//! Shell tool — sandboxed command execution, capability-scoped.
//!
//! Executes shell commands within the agent's sandbox. Scoped by
//! capability grants in ghost.yml. Captures both stdout and stderr.

use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("command not allowed by capability scope: {0}")]
    NotAllowed(String),
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("timeout after {0}s")]
    Timeout(u64),
}

/// Shell tool configuration.
#[derive(Debug, Clone)]
pub struct ShellToolConfig {
    /// Allowed command prefixes (capability-scoped).
    pub allowed_prefixes: Vec<String>,
    /// Working directory for commands.
    pub working_dir: String,
    /// Default timeout.
    pub timeout: Duration,
    /// Execution backend used to run the shell command.
    pub execution_backend: ShellExecutionBackend,
}

impl Default for ShellToolConfig {
    fn default() -> Self {
        Self {
            allowed_prefixes: vec![],
            working_dir: ".".into(),
            timeout: Duration::from_secs(30),
            execution_backend: ShellExecutionBackend::InProcess,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ShellExecutionBackend {
    InProcess,
    ProcessHelper {
        helper_executable: String,
    },
    Container {
        image: String,
        workspace_dir: String,
        read_only_workspace: bool,
        network_access: bool,
    },
}

/// Execute a shell command within sandbox constraints.
///
/// Returns (stdout, stderr) tuple. Both streams are captured.
pub async fn execute_shell(
    command: &str,
    config: &ShellToolConfig,
) -> Result<(String, String), ShellError> {
    // Fail closed: shell execution stays disabled until an explicit prefix
    // allowlist is provided.
    if config.allowed_prefixes.is_empty() {
        return Err(ShellError::NotAllowed(
            "shell tool disabled: no allowed prefixes configured".into(),
        ));
    }

    let allowed = config
        .allowed_prefixes
        .iter()
        .any(|prefix| command.starts_with(prefix.as_str()));
    if !allowed {
        return Err(ShellError::NotAllowed(command.to_string()));
    }

    // Execute with timeout
    let result = tokio::time::timeout(config.timeout, async {
        match &config.execution_backend {
            ShellExecutionBackend::InProcess => {
                let mut process = tokio::process::Command::new("sh");
                process.kill_on_drop(true);
                process
                    .arg("-c")
                    .arg(command)
                    .current_dir(&config.working_dir)
                    .output()
                    .await
                    .map_err(|e| ShellError::ExecutionFailed(e.to_string()))
            }
            ShellExecutionBackend::ProcessHelper { helper_executable } => {
                let mut process = tokio::process::Command::new(helper_executable);
                process.kill_on_drop(true);
                process
                    .arg("sandbox-shell")
                    .arg("--cwd")
                    .arg(&config.working_dir)
                    .arg("--timeout-secs")
                    .arg(config.timeout.as_secs().to_string())
                    .arg("--command")
                    .arg(command)
                    .output()
                    .await
                    .map_err(|e| ShellError::ExecutionFailed(e.to_string()))
            }
            ShellExecutionBackend::Container {
                image,
                workspace_dir,
                read_only_workspace,
                network_access,
            } => {
                let mount_mode = if *read_only_workspace { "ro" } else { "rw" };
                let mut process = tokio::process::Command::new("docker");
                process.kill_on_drop(true);
                process
                    .arg("run")
                    .arg("--rm")
                    .arg("--workdir")
                    .arg("/workspace")
                    .arg("--volume")
                    .arg(format!("{workspace_dir}:/workspace:{mount_mode}"));
                if !network_access {
                    process.arg("--network").arg("none");
                }
                process
                    .arg(image)
                    .arg("sh")
                    .arg("-lc")
                    .arg(command)
                    .output()
                    .await
                    .map_err(|e| ShellError::ExecutionFailed(e.to_string()))
            }
        }
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok((stdout, stderr))
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(ShellError::Timeout(config.timeout.as_secs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shell_denied_when_unconfigured() {
        let error = execute_shell("echo blocked", &ShellToolConfig::default())
            .await
            .unwrap_err();

        assert!(matches!(error, ShellError::NotAllowed(_)));
    }
}
