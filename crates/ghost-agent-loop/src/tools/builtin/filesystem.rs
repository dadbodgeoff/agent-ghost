//! Filesystem tool — scoped read/write operations.
//!
//! All paths are resolved relative to the agent's workspace root.
//! Path traversal outside the workspace is rejected.

use std::path::PathBuf;

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

/// Filesystem tool scoped to a workspace root.
pub struct FilesystemTool {
    workspace_root: PathBuf,
}

impl FilesystemTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Resolve and validate a path within the workspace.
    fn resolve(&self, relative: &str) -> Result<PathBuf, FsError> {
        let resolved = self.workspace_root.join(relative);
        let canonical = resolved
            .canonicalize()
            .unwrap_or_else(|_| resolved.clone());

        // Ensure path is within workspace
        if !canonical.starts_with(&self.workspace_root) {
            return Err(FsError::PathTraversal(relative.to_string()));
        }

        Ok(canonical)
    }

    /// Read a file within the workspace.
    pub fn read_file(&self, relative: &str) -> Result<String, FsError> {
        let path = self.resolve(relative)?;
        std::fs::read_to_string(&path).map_err(|e| FsError::ReadFailed(e.to_string()))
    }

    /// Write a file within the workspace.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<(), FsError> {
        let path = self.resolve(relative)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| FsError::WriteFailed(e.to_string()))?;
        }
        std::fs::write(&path, content).map_err(|e| FsError::WriteFailed(e.to_string()))
    }

    /// List directory contents within the workspace.
    pub fn list_dir(&self, relative: &str) -> Result<Vec<String>, FsError> {
        let path = self.resolve(relative)?;
        let entries = std::fs::read_dir(&path)
            .map_err(|e| FsError::ReadFailed(e.to_string()))?;

        let mut names = Vec::new();
        for entry in entries {
            if let Ok(entry) = entry {
                names.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        Ok(names)
    }
}
