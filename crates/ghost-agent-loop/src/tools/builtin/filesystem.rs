//! Filesystem tool — scoped read/write operations.
//!
//! All paths are resolved relative to the agent's workspace root.
//! Path traversal outside the workspace is rejected.

use std::path::{Component, Path, PathBuf};

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
        let workspace_root = workspace_root.canonicalize().unwrap_or(workspace_root);
        Self { workspace_root }
    }

    /// Resolve and validate a path within the workspace.
    fn resolve(&self, relative: &str) -> Result<PathBuf, FsError> {
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

    /// Read a file within the workspace.
    pub fn read_file(&self, relative: &str) -> Result<String, FsError> {
        let path = self.resolve(relative)?;
        std::fs::read_to_string(&path).map_err(|e| FsError::ReadFailed(e.to_string()))
    }

    /// Write a file within the workspace.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<(), FsError> {
        let path = self.resolve(relative)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| FsError::WriteFailed(e.to_string()))?;
        }
        std::fs::write(&path, content).map_err(|e| FsError::WriteFailed(e.to_string()))
    }

    /// List directory contents within the workspace.
    pub fn list_dir(&self, relative: &str) -> Result<Vec<String>, FsError> {
        let path = self.resolve(relative)?;
        let entries = std::fs::read_dir(&path).map_err(|e| FsError::ReadFailed(e.to_string()))?;

        let mut names = Vec::new();
        for entry in entries.flatten() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_file_rejects_nonexistent_path_traversal() {
        let root = tempfile::tempdir().unwrap();
        let fs = FilesystemTool::new(root.path().to_path_buf());

        let error = fs.write_file("../escape/new.txt", "blocked").unwrap_err();

        assert!(matches!(error, FsError::PathTraversal(_)));
    }

    #[test]
    fn write_file_allows_valid_in_workspace_create() {
        let root = tempfile::tempdir().unwrap();
        let fs = FilesystemTool::new(root.path().to_path_buf());

        fs.write_file("nested/created.txt", "ok").unwrap();

        let created = root.path().join("nested").join("created.txt");
        assert_eq!(std::fs::read_to_string(created).unwrap(), "ok");
    }
}
