use serde::{Deserialize, Serialize};

/// Signals derived from the currently active file and its context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileSignals {
    /// Path to the currently active file.
    pub active_file: Option<String>,
    /// Import/dependency paths extracted from the active file.
    pub imports: Vec<String>,
    /// Symbol names (functions, classes, types) in the active file.
    pub symbols: Vec<String>,
    /// Directory containing the active file.
    pub directory: Option<String>,
}

impl FileSignals {
    /// Collect file signals from a file path and its parsed metadata.
    pub fn gather(active_file: Option<&str>, imports: Vec<String>, symbols: Vec<String>) -> Self {
        let directory = active_file.and_then(|f| {
            std::path::Path::new(f)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
        });
        Self {
            active_file: active_file.map(String::from),
            imports,
            symbols,
            directory,
        }
    }

    /// Returns all file paths relevant to this signal (active + imports).
    pub fn relevant_paths(&self) -> Vec<&str> {
        let mut paths: Vec<&str> = self.imports.iter().map(|s| s.as_str()).collect();
        if let Some(ref f) = self.active_file {
            paths.push(f.as_str());
        }
        paths
    }
}
