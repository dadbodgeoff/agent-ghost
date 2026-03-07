//! UserManager — loads USER.md (A2.9).
//!
//! Agent can PROPOSE updates via ProposalValidator, but cannot directly modify.

use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UserError {
    #[error("USER.md not found at {path}")]
    NotFound { path: String },
    #[error("failed to read USER.md: {0}")]
    ReadError(String),
}

/// Loaded user document.
#[derive(Debug, Clone)]
pub struct UserDocument {
    pub content: String,
}

/// Manages the USER.md document.
pub struct UserManager {
    document: Option<UserDocument>,
}

impl UserManager {
    pub fn new() -> Self {
        Self { document: None }
    }

    /// Load USER.md from the given path.
    pub fn load(&mut self, path: &Path) -> Result<&UserDocument, UserError> {
        if !path.exists() {
            return Err(UserError::NotFound {
                path: path.display().to_string(),
            });
        }

        let content =
            std::fs::read_to_string(path).map_err(|e| UserError::ReadError(e.to_string()))?;

        self.document = Some(UserDocument { content });
        // SAFETY: we just assigned `Some` to self.document above
        Ok(self.document.as_ref().expect("document was just set"))
    }

    pub fn document(&self) -> Option<&UserDocument> {
        self.document.as_ref()
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
