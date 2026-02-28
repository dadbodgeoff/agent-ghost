/// Causal graph errors.
#[derive(Debug, thiserror::Error)]
pub enum CausalError {
    #[error("cycle detected in causal graph: {path}")]
    CycleDetected { path: String },

    #[error("traversal depth exceeded: max {max_depth}, reached {depth}")]
    TraversalDepthExceeded { max_depth: usize, depth: usize },

    #[error("invalid relation: {reason}")]
    InvalidRelation { reason: String },

    #[error("graph inconsistency: {details}")]
    GraphInconsistency { details: String },
}
