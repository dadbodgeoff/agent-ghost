use serde::{Deserialize, Serialize};

/// Information about the active embedding model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelInfo {
    pub name: String,
    pub dimensions: usize,
    pub status: EmbeddingModelStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingModelStatus {
    Active,
    Migrating,
    Unavailable,
}
