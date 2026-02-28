//! Bearer token authentication from GHOST_TOKEN env var.

/// Validate a bearer token against GHOST_TOKEN env var.
pub fn validate_token(token: &str) -> bool {
    match std::env::var("GHOST_TOKEN") {
        Ok(expected) => {
            // Constant-time comparison
            if token.len() != expected.len() {
                return false;
            }
            token
                .bytes()
                .zip(expected.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
        }
        Err(_) => {
            tracing::warn!("GHOST_TOKEN not set — authentication disabled");
            true // No token configured = no auth
        }
    }
}
