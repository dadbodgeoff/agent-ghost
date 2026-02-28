//! RFC 3161 timestamp authority anchor — stub (NotImplemented).

/// RFC 3161 anchor — not yet implemented.
pub struct RFC3161Anchor;

impl RFC3161Anchor {
    pub fn new() -> Self {
        Self
    }

    /// Attempt to anchor — returns NotImplemented error.
    pub fn anchor(&self, _merkle_root: &[u8; 32]) -> Result<(), &'static str> {
        Err("RFC 3161 anchoring not yet implemented")
    }
}

impl Default for RFC3161Anchor {
    fn default() -> Self {
        Self::new()
    }
}
