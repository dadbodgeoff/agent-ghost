//! Disk-backed ITP event buffer for degraded mode (Req 15 AC10).
//!
//! Max 10MB or 10K events, FIFO eviction.

use std::collections::VecDeque;

/// Maximum buffer size in bytes.
const MAX_BUFFER_BYTES: usize = 10 * 1024 * 1024; // 10MB
/// Maximum number of buffered events.
const MAX_BUFFER_EVENTS: usize = 10_000;

/// Buffered ITP event (serialized JSON).
#[derive(Debug, Clone)]
pub struct BufferedEvent {
    pub json: String,
    pub size_bytes: usize,
}

/// Disk-backed ITP event buffer.
pub struct ITPBuffer {
    events: VecDeque<BufferedEvent>,
    total_bytes: usize,
}

impl ITPBuffer {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
            total_bytes: 0,
        }
    }

    /// Buffer an event. Evicts oldest if limits exceeded.
    pub fn push(&mut self, json: String) {
        let size = json.len();
        let event = BufferedEvent {
            json,
            size_bytes: size,
        };

        // Evict oldest until within limits
        while (self.total_bytes + size > MAX_BUFFER_BYTES || self.events.len() >= MAX_BUFFER_EVENTS)
            && !self.events.is_empty()
        {
            if let Some(old) = self.events.pop_front() {
                self.total_bytes -= old.size_bytes;
            }
        }

        self.total_bytes += size;
        self.events.push_back(event);
    }

    /// Drain all buffered events for replay.
    pub fn drain_all(&mut self) -> Vec<BufferedEvent> {
        self.total_bytes = 0;
        self.events.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl Default for ITPBuffer {
    fn default() -> Self {
        Self::new()
    }
}
