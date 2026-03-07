//! Marketplace error types.

#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("storage error: {0}")]
    Storage(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid state transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },

    #[error("insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: i64, available: i64 },

    #[error("validation error: {0}")]
    Validation(String),
}

impl From<cortex_core::models::error::CortexError> for MarketplaceError {
    fn from(e: cortex_core::models::error::CortexError) -> Self {
        MarketplaceError::Storage(e.to_string())
    }
}

pub type MarketplaceResult<T> = Result<T, MarketplaceError>;
