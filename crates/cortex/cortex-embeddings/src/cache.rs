//! In-memory embedding cache with capacity limit.
//!
//! Simple HashMap-based cache with FIFO eviction when capacity is exceeded.
//! No external dependencies — uses std collections only.

use std::collections::{HashMap, VecDeque};

/// In-memory embedding cache.
///
/// Keys are blake3 content hashes. Values are embedding vectors.
/// When capacity is exceeded, the oldest entry is evicted (FIFO).
pub struct EmbeddingCache {
    entries: HashMap<String, Vec<f32>>,
    order: VecDeque<String>,
    capacity: usize,
}

impl EmbeddingCache {
    /// Create a new cache with the given max entry count.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Get an embedding by content hash.
    pub fn get(&self, content_hash: &str) -> Option<&Vec<f32>> {
        self.entries.get(content_hash)
    }

    /// Insert an embedding keyed by content hash.
    ///
    /// If the cache is at capacity, the oldest entry is evicted.
    pub fn insert(&mut self, content_hash: String, embedding: Vec<f32>) {
        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.entries.entry(content_hash.clone())
        {
            // Already cached — update in place.
            entry.insert(embedding);
            return;
        }

        // Evict oldest if at capacity.
        while self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }

        self.order.push_back(content_hash.clone());
        self.entries.insert(content_hash, embedding);
    }

    /// Number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut cache = EmbeddingCache::new(100);
        let hash = "abc123".to_string();
        let vec = vec![1.0, 2.0, 3.0];
        cache.insert(hash.clone(), vec.clone());
        assert_eq!(cache.get(&hash), Some(&vec));
    }

    #[test]
    fn miss_returns_none() {
        let cache = EmbeddingCache::new(100);
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn evicts_oldest_at_capacity() {
        let mut cache = EmbeddingCache::new(2);
        cache.insert("a".to_string(), vec![1.0]);
        cache.insert("b".to_string(), vec![2.0]);
        cache.insert("c".to_string(), vec![3.0]); // Should evict "a".
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), Some(&vec![2.0]));
        assert_eq!(cache.get("c"), Some(&vec![3.0]));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn duplicate_insert_does_not_evict() {
        let mut cache = EmbeddingCache::new(2);
        cache.insert("a".to_string(), vec![1.0]);
        cache.insert("b".to_string(), vec![2.0]);
        cache.insert("a".to_string(), vec![1.5]); // Update, not new.
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("a"), Some(&vec![1.5]));
        assert_eq!(cache.get("b"), Some(&vec![2.0]));
    }
}
