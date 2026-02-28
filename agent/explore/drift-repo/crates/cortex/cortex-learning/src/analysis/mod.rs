//! Correction analysis: diff analysis, categorization, and category mapping.

pub mod categorizer;
pub mod category_mapping;
pub mod diff_analyzer;

pub use categorizer::{categorize, CorrectionCategory};
pub use category_mapping::{build_typed_content, map_category, CategoryMapping};
pub use diff_analyzer::{analyze_diff, DiffAnalysis};
