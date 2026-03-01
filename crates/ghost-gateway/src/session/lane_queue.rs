//! Per-session serialized request queue (Req 26 AC1).
//! Depth limit default 5, backpressure (reject with 429 when full).

use std::collections::VecDeque;

use dashmap::DashMap;
use uuid::Uuid;

/// A single lane queue for one session.
#[derive(Debug, Clone)]
pub struct LaneQueue {
    queue: VecDeque<QueuedRequest>,
    depth_limit: usize,
    processing: bool,
}

/// A queued request.
#[derive(Debug, Clone)]
pub struct QueuedRequest {
    pub request_id: Uuid,
    pub payload: String,
}

impl LaneQueue {
    pub fn new(depth_limit: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            depth_limit,
            processing: false,
        }
    }

    /// Enqueue a request. Returns Err if queue is full (backpressure).
    pub fn enqueue(&mut self, request: QueuedRequest) -> Result<(), QueuedRequest> {
        if self.queue.len() >= self.depth_limit {
            return Err(request);
        }
        self.queue.push_back(request);
        Ok(())
    }

    /// Dequeue the next request for processing.
    pub fn dequeue(&mut self) -> Option<QueuedRequest> {
        if self.processing {
            return None; // Already processing one
        }
        if let Some(req) = self.queue.pop_front() {
            self.processing = true;
            Some(req)
        } else {
            None
        }
    }

    /// Mark current processing as complete.
    pub fn complete(&mut self) {
        self.processing = false;
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn is_processing(&self) -> bool {
        self.processing
    }
}

/// Manager for all lane queues (one per session).
pub struct LaneQueueManager {
    queues: DashMap<Uuid, LaneQueue>,
    default_depth: usize,
}

impl LaneQueueManager {
    pub fn new(default_depth: usize) -> Self {
        Self {
            queues: DashMap::new(),
            default_depth,
        }
    }

    /// Get or create a lane queue for a session.
    pub fn enqueue(&self, session_id: Uuid, request: QueuedRequest) -> Result<(), QueuedRequest> {
        let mut entry = self
            .queues
            .entry(session_id)
            .or_insert_with(|| LaneQueue::new(self.default_depth));
        entry.enqueue(request)
    }

    /// Dequeue from a session's lane.
    pub fn dequeue(&self, session_id: Uuid) -> Option<QueuedRequest> {
        self.queues.get_mut(&session_id)?.dequeue()
    }

    /// Mark processing complete for a session.
    pub fn complete(&self, session_id: Uuid) {
        if let Some(mut queue) = self.queues.get_mut(&session_id) {
            queue.complete();
        }
    }

    /// Remove idle session queues.
    pub fn prune_empty(&self) {
        self.queues.retain(|_, q| !q.is_empty() || q.is_processing());
    }
}

impl Default for LaneQueueManager {
    fn default() -> Self {
        Self::new(5)
    }
}
