/// Retrieval subsystem errors.
#[derive(Debug, thiserror::Error)]
pub enum RetrievalError {
    #[error("budget exceeded: needed {needed} tokens, available {available}")]
    BudgetExceeded { needed: usize, available: usize },

    #[error("no results found for query")]
    NoResults,

    #[error("search failed: {reason}")]
    SearchFailed { reason: String },

    #[error("ranking failed: {reason}")]
    RankingFailed { reason: String },
}
