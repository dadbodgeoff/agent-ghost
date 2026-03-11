//! Filesystem tool — scoped read/write operations.
//!
//! All paths are resolved relative to the agent's workspace root.
//! Path traversal outside the workspace is rejected.

use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsError {
    #[error("path traversal denied: {0}")]
    PathTraversal(String),
    #[error("read failed: {0}")]
    ReadFailed(String),
    #[error("write failed: {0}")]
    WriteFailed(String),
}

#[derive(Debug, Clone)]
pub enum FilesystemExecutionBackend {
    InProcess,
    ProcessHelper {
        helper_executable: String,
    },
    Container {
        image: String,
        workspace_dir: String,
        read_only_workspace: bool,
    },
}

/// Filesystem tool scoped to a workspace root.
pub struct FilesystemTool {
    workspace_root: PathBuf,
    allow_absolute_paths: bool,
    execution_backend: FilesystemExecutionBackend,
}

impl FilesystemTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        let workspace_root = workspace_root.canonicalize().unwrap_or(workspace_root);
        Self {
            workspace_root,
            allow_absolute_paths: false,
            execution_backend: FilesystemExecutionBackend::InProcess,
        }
    }

    pub fn new_unrestricted(workspace_root: PathBuf) -> Self {
        let workspace_root = workspace_root.canonicalize().unwrap_or(workspace_root);
        Self {
            workspace_root,
            allow_absolute_paths: true,
            execution_backend: FilesystemExecutionBackend::InProcess,
        }
    }

    pub fn with_execution_backend(mut self, execution_backend: FilesystemExecutionBackend) -> Self {
        self.execution_backend = execution_backend;
        self
    }

    pub fn set_execution_backend(&mut self, execution_backend: FilesystemExecutionBackend) {
        self.execution_backend = execution_backend;
    }

    /// Resolve and validate a path within the workspace.
    fn resolve(&self, relative: &str) -> Result<PathBuf, FsError> {
        if self.allow_absolute_paths {
            return Ok(self.resolve_unrestricted(relative));
        }

        let mut normalized = PathBuf::new();
        for component in Path::new(relative).components() {
            match component {
                Component::CurDir => {}
                Component::Normal(part) => normalized.push(part),
                Component::ParentDir => {
                    if !normalized.pop() {
                        return Err(FsError::PathTraversal(relative.to_string()));
                    }
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(FsError::PathTraversal(relative.to_string()));
                }
            }
        }

        Ok(self.workspace_root.join(normalized))
    }

    fn resolve_unrestricted(&self, path: &str) -> PathBuf {
        let input = Path::new(path);
        let mut resolved = if input.is_absolute() {
            PathBuf::new()
        } else {
            self.workspace_root.clone()
        };

        for component in input.components() {
            match component {
                Component::Prefix(prefix) => resolved.push(prefix.as_os_str()),
                Component::RootDir => {
                    if resolved.as_os_str().is_empty() {
                        resolved.push(std::path::MAIN_SEPARATOR_STR);
                    } else {
                        resolved.push(std::path::MAIN_SEPARATOR_STR);
                    }
                }
                Component::CurDir => {}
                Component::Normal(part) => resolved.push(part),
                Component::ParentDir => {
                    let _ = resolved.pop();
                }
            }
        }

        if resolved.as_os_str().is_empty() {
            self.workspace_root.clone()
        } else {
            resolved
        }
    }

    /// Read a file within the workspace.
    pub fn read_file(&self, relative: &str) -> Result<String, FsError> {
        let path = self.resolve(relative)?;
        match &self.execution_backend {
            FilesystemExecutionBackend::InProcess => {
                std::fs::read_to_string(&path).map_err(|e| FsError::ReadFailed(e.to_string()))
            }
            FilesystemExecutionBackend::ProcessHelper { helper_executable } => self
                .read_via_process_helper(helper_executable, relative)
                .map_err(FsError::ReadFailed),
            FilesystemExecutionBackend::Container {
                image,
                workspace_dir,
                ..
            } => self
                .read_via_container(image, workspace_dir, &path)
                .map_err(FsError::ReadFailed),
        }
    }

    /// Write a file within the workspace.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<(), FsError> {
        let path = self.resolve(relative)?;
        match &self.execution_backend {
            FilesystemExecutionBackend::InProcess => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| FsError::WriteFailed(e.to_string()))?;
                }
                std::fs::write(&path, content).map_err(|e| FsError::WriteFailed(e.to_string()))
            }
            FilesystemExecutionBackend::ProcessHelper { helper_executable } => self
                .write_via_process_helper(helper_executable, relative, content)
                .map_err(FsError::WriteFailed),
            FilesystemExecutionBackend::Container {
                image,
                workspace_dir,
                read_only_workspace,
            } => self
                .write_via_container(image, workspace_dir, *read_only_workspace, &path, content)
                .map_err(FsError::WriteFailed),
        }
    }

    /// List directory contents within the workspace.
    pub fn list_dir(&self, relative: &str) -> Result<Vec<String>, FsError> {
        let path = self.resolve(relative)?;
        match &self.execution_backend {
            FilesystemExecutionBackend::InProcess => {
                let entries =
                    std::fs::read_dir(&path).map_err(|e| FsError::ReadFailed(e.to_string()))?;

                let mut names = Vec::new();
                for entry in entries.flatten() {
                    names.push(entry.file_name().to_string_lossy().to_string());
                }
                Ok(names)
            }
            FilesystemExecutionBackend::ProcessHelper { helper_executable } => self
                .list_via_process_helper(helper_executable, relative)
                .map_err(FsError::ReadFailed),
            FilesystemExecutionBackend::Container {
                image,
                workspace_dir,
                ..
            } => self
                .list_via_container(image, workspace_dir, &path)
                .map_err(FsError::ReadFailed),
        }
    }

    fn read_via_process_helper(
        &self,
        helper_executable: &str,
        relative: &str,
    ) -> Result<String, String> {
        let output =
            self.run_process_helper(helper_executable, "sandbox-fs-read", relative, None)?;
        String::from_utf8(output).map_err(|e| e.to_string())
    }

    fn write_via_process_helper(
        &self,
        helper_executable: &str,
        relative: &str,
        content: &str,
    ) -> Result<(), String> {
        self.run_process_helper(
            helper_executable,
            "sandbox-fs-write",
            relative,
            Some(content.as_bytes()),
        )?;
        Ok(())
    }

    fn list_via_process_helper(
        &self,
        helper_executable: &str,
        relative: &str,
    ) -> Result<Vec<String>, String> {
        let output =
            self.run_process_helper(helper_executable, "sandbox-fs-list", relative, None)?;
        let stdout = String::from_utf8(output).map_err(|e| e.to_string())?;
        Ok(parse_list_output(&stdout))
    }

    fn run_process_helper(
        &self,
        helper_executable: &str,
        subcommand: &str,
        relative: &str,
        stdin_bytes: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        let mut command = Command::new(helper_executable);
        command
            .arg(subcommand)
            .arg("--cwd")
            .arg(&self.workspace_root)
            .arg("--path")
            .arg(relative)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.allow_absolute_paths {
            command.arg("--allow-absolute-paths");
        }
        if stdin_bytes.is_some() {
            command.stdin(Stdio::piped());
        }

        let mut child = command.spawn().map_err(|e| e.to_string())?;
        if let Some(bytes) = stdin_bytes {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| "process helper stdin unavailable".to_string())?;
            use std::io::Write;
            stdin.write_all(bytes).map_err(|e| e.to_string())?;
        }

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    fn read_via_container(
        &self,
        image: &str,
        workspace_dir: &str,
        resolved_path: &Path,
    ) -> Result<String, String> {
        let target = self.container_target_path(resolved_path)?;
        let output = self.run_container_command(
            image,
            workspace_dir,
            false,
            &target,
            "cat -- \"$TARGET\"",
            None,
        )?;
        String::from_utf8(output).map_err(|e| e.to_string())
    }

    fn write_via_container(
        &self,
        image: &str,
        workspace_dir: &str,
        read_only_workspace: bool,
        resolved_path: &Path,
        content: &str,
    ) -> Result<(), String> {
        if read_only_workspace {
            return Err("filesystem backend is read-only".into());
        }
        let target = self.container_target_path(resolved_path)?;
        self.run_container_command(
            image,
            workspace_dir,
            true,
            &target,
            "mkdir -p -- \"$(dirname \"$TARGET\")\" && cat > \"$TARGET\"",
            Some(content.as_bytes()),
        )?;
        Ok(())
    }

    fn list_via_container(
        &self,
        image: &str,
        workspace_dir: &str,
        resolved_path: &Path,
    ) -> Result<Vec<String>, String> {
        let target = self.container_target_path(resolved_path)?;
        let output = self.run_container_command(
            image,
            workspace_dir,
            false,
            &target,
            "if [ -d \"$TARGET\" ]; then ls -1A -- \"$TARGET\"; else exit 1; fi",
            None,
        )?;
        let stdout = String::from_utf8(output).map_err(|e| e.to_string())?;
        Ok(parse_list_output(&stdout))
    }

    fn run_container_command(
        &self,
        image: &str,
        workspace_dir: &str,
        writable: bool,
        target: &str,
        script: &str,
        stdin_bytes: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        let mount_source = if self.allow_absolute_paths {
            "/"
        } else {
            workspace_dir
        };
        let mount_target = if self.allow_absolute_paths {
            "/host"
        } else {
            "/workspace"
        };
        let mount_mode = if writable { "rw" } else { "ro" };

        let mut command = Command::new("docker");
        command.arg("run").arg("--rm");
        if stdin_bytes.is_some() {
            command.arg("-i");
        }
        command
            .arg("--workdir")
            .arg(mount_target)
            .arg("--volume")
            .arg(format!("{mount_source}:{mount_target}:{mount_mode}"))
            .arg("--env")
            .arg(format!("TARGET={target}"))
            .arg(image)
            .arg("sh")
            .arg("-lc")
            .arg(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin_bytes.is_some() {
            command.stdin(Stdio::piped());
        }

        let mut child = command.spawn().map_err(|e| e.to_string())?;
        if let Some(bytes) = stdin_bytes {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| "container stdin unavailable".to_string())?;
            use std::io::Write;
            stdin.write_all(bytes).map_err(|e| e.to_string())?;
        }

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                Err(format!(
                    "container command exited with status {}",
                    output.status
                ))
            } else {
                Err(stderr)
            }
        }
    }

    fn container_target_path(&self, resolved_path: &Path) -> Result<String, String> {
        if self.allow_absolute_paths {
            return Ok(format!("/host{}", resolved_path.display()));
        }

        let relative = resolved_path
            .strip_prefix(&self.workspace_root)
            .map_err(|e| e.to_string())?;
        if relative.as_os_str().is_empty() {
            Ok("/workspace".to_string())
        } else {
            Ok(format!("/workspace/{}", relative.display()))
        }
    }
}

fn parse_list_output(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_file_rejects_nonexistent_path_traversal() {
        let root = tempfile::tempdir().unwrap();
        let fs = FilesystemTool::new(root.path().to_path_buf());

        let error = fs.write_file("../escape/new.txt", "blocked").unwrap_err();

        assert!(matches!(error, FsError::PathTraversal(_)));
    }

    #[tokio::test]
    async fn write_file_allows_valid_in_workspace_create() {
        let root = tempfile::tempdir().unwrap();
        let fs = FilesystemTool::new(root.path().to_path_buf());

        fs.write_file("nested/created.txt", "ok").unwrap();

        let created = root.path().join("nested").join("created.txt");
        assert_eq!(std::fs::read_to_string(created).unwrap(), "ok");
    }

    #[tokio::test]
    async fn unrestricted_mode_allows_absolute_paths() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("absolute.txt");
        std::fs::write(&target, "ok").unwrap();

        let fs = FilesystemTool::new_unrestricted(root.path().to_path_buf());

        assert_eq!(
            fs.read_file(target.to_string_lossy().as_ref()).unwrap(),
            "ok"
        );
    }

    #[test]
    fn container_target_uses_workspace_mount_for_scoped_paths() {
        let root = tempfile::tempdir().unwrap();
        let fs = FilesystemTool::new(root.path().to_path_buf());
        let target = fs.resolve("nested/file.txt").unwrap();

        assert_eq!(
            fs.container_target_path(&target).unwrap(),
            "/workspace/nested/file.txt"
        );
    }

    #[test]
    fn container_target_uses_host_mount_for_unrestricted_paths() {
        let root = tempfile::tempdir().unwrap();
        let fs = FilesystemTool::new_unrestricted(root.path().to_path_buf());
        let target = fs.resolve("nested/file.txt").unwrap();

        assert_eq!(
            fs.container_target_path(&target).unwrap(),
            format!("/host{}", target.display())
        );
    }
}
