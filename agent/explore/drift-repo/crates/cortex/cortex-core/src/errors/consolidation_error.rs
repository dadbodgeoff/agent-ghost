/// Consolidation subsystem errors.
#[derive(Debug, thiserror::Error)]
pub enum ConsolidationError {
    #[error("clustering failed: {reason}")]
    ClusteringFailed { reason: String },

    #[error("recall gate failed: score {score:.3} below threshold {threshold:.3}")]
    RecallGateFailed { score: f64, threshold: f64 },

    #[error("merge failed: {reason}")]
    MergeFailed { reason: String },

    #[error("quality below threshold: {metric} = {value:.3}, min = {threshold:.3}")]
    QualityBelowThreshold {
        metric: String,
        value: f64,
        threshold: f64,
    },
}
