//! Explicit guest ABI for external WASM skills.

/// Required linear memory export.
pub const MEMORY_EXPORT: &str = "memory";
/// Required allocator export used to copy JSON input into guest memory.
pub const ALLOC_EXPORT: &str = "alloc";
/// Required execution export. Signature: `(i32, i32) -> i64`.
pub const RUN_EXPORT: &str = "run";

/// Decoded pointer/length pair returned from `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestBuffer {
    pub pointer: u32,
    pub length: u32,
}

/// Decode a packed `(pointer, length)` result returned from the guest.
pub fn unpack_guest_buffer(packed: i64) -> GuestBuffer {
    let raw = packed as u64;
    GuestBuffer {
        pointer: (raw >> 32) as u32,
        length: raw as u32,
    }
}
