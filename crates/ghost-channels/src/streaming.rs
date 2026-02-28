//! Streaming formatter: chunk buffering, edit throttle (Req 22 AC3).

use std::time::{Duration, Instant};

/// Streaming formatter with chunk buffering and edit throttle.
pub struct StreamingFormatter {
    buffer: String,
    last_flush: Instant,
    throttle: Duration,
}

impl StreamingFormatter {
    pub fn new(throttle: Duration) -> Self {
        Self {
            buffer: String::new(),
            last_flush: Instant::now(),
            throttle,
        }
    }

    /// Add a chunk to the buffer.
    pub fn push_chunk(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
    }

    /// Check if the buffer should be flushed (throttle elapsed).
    pub fn should_flush(&self) -> bool {
        self.last_flush.elapsed() >= self.throttle
    }

    /// Flush the buffer, returning accumulated content.
    pub fn flush(&mut self) -> String {
        self.last_flush = Instant::now();
        std::mem::take(&mut self.buffer)
    }

    /// Get current buffer content without flushing.
    pub fn peek(&self) -> &str {
        &self.buffer
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for StreamingFormatter {
    fn default() -> Self {
        Self::new(Duration::from_millis(100))
    }
}
