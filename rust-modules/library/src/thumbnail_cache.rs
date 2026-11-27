//! Bounded, backend-neutral thumbnail byte cache for the library layer.
//!
//! The cache stores opaque encoded image bytes. Decoding, rendering and fetching remain adapter
//! responsibilities, so this module has no image, SMB, network or UI dependencies.

use std::collections::{HashMap, VecDeque};

/// An in-memory least-recently-used cache bounded by encoded byte size.
#[derive(Debug)]
pub struct ThumbnailCache {
    max_bytes: usize,
    used_bytes: usize,
    entries: HashMap<String, Vec<u8>>,
    recency: VecDeque<String>,
}

impl ThumbnailCache {
    /// Create a cache with a hard upper bound on retained encoded bytes.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            used_bytes: 0,
            entries: HashMap::new(),
            recency: VecDeque::new(),
        }
    }

    pub fn max_bytes(&self) -> usize { self.max_bytes }

    /// Change the memory budget immediately, evicting least-recently-used entries if needed.
    pub fn set_max_bytes(&mut self, max_bytes: usize) {
        self.max_bytes = max_bytes;
        while self.used_bytes > self.max_bytes {
            let Some(oldest) = self.recency.pop_front() else { break; };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.used_bytes = self.used_bytes.saturating_sub(removed.len());
            }
        }
    }
    pub fn used_bytes(&self) -> usize { self.used_bytes }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Return cached bytes and promote the key to most-recently-used.
    pub fn get(&mut self, key: &str) -> Option<&[u8]> {
        if !self.entries.contains_key(key) { return None; }
        self.promote(key);
        self.entries.get(key).map(Vec::as_slice)
    }

    /// Insert or replace an entry. Oversized values are rejected without disturbing the cache.
    /// Least-recently-used entries are evicted until the new value fits.
    pub fn insert(&mut self, key: impl Into<String>, bytes: Vec<u8>) -> bool {
        let key = key.into();
        if bytes.len() > self.max_bytes { return false; }
        self.remove(&key);
        while self.used_bytes.saturating_add(bytes.len()) > self.max_bytes {
            let Some(oldest) = self.recency.pop_front() else { break; };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.used_bytes = self.used_bytes.saturating_sub(removed.len());
            }
        }
        self.used_bytes = self.used_bytes.saturating_add(bytes.len());
        self.entries.insert(key.clone(), bytes);
        self.recency.push_back(key);
        true
    }

    /// Remove one key and return whether it existed.
    pub fn remove(&mut self, key: &str) -> bool {
        self.recency.retain(|candidate| candidate != key);
        if let Some(bytes) = self.entries.remove(key) {
            self.used_bytes = self.used_bytes.saturating_sub(bytes.len());
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
        self.used_bytes = 0;
    }

    /// Keep only entries accepted by the predicate, releasing stale thumbnail bytes.
    /// The relative LRU order of retained entries is preserved.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        let mut used_bytes = 0usize;
        self.entries.retain(|key, bytes| {
            if keep(key) {
                used_bytes = used_bytes.saturating_add(bytes.len());
                true
            } else {
                false
            }
        });
        let entries = &self.entries;
        self.recency.retain(|key| entries.contains_key(key));
        self.used_bytes = used_bytes;
    }

    fn promote(&mut self, key: &str) {
        self.recency.retain(|candidate| candidate != key);
        self.recency.push_back(key.to_owned());
    }
}

impl Default for ThumbnailCache {
    fn default() -> Self { Self::new(8 * 1024 * 1024) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_is_byte_bounded_and_evicts_least_recently_used() {
        let mut cache = ThumbnailCache::new(7);
        assert!(cache.insert("a", vec![1, 2, 3]));
        assert!(cache.insert("b", vec![4, 5]));
        assert_eq!(cache.get("a"), Some(&[1, 2, 3][..]));
        assert!(cache.insert("c", vec![6, 7, 8, 9]));
        assert!(cache.get("a").is_some(), "a was promoted before insertion");
        assert!(cache.get("b").is_none(), "b is the least-recently-used entry");
        assert_eq!(cache.used_bytes(), 7);
        assert!(cache.used_bytes() <= cache.max_bytes());
    }

    #[test]
    fn replacement_and_removal_keep_byte_accounting_exact() {
        let mut cache = ThumbnailCache::new(10);
        assert!(cache.insert("poster", vec![1, 2, 3]));
        assert!(cache.insert("poster", vec![4, 5]));
        assert_eq!(cache.used_bytes(), 2);
        assert!(cache.remove("poster"));
        assert!(!cache.remove("poster"));
        assert_eq!(cache.used_bytes(), 0);
    }

    #[test]
    fn oversized_insert_is_rejected_without_evicting_existing_entries() {
        let mut cache = ThumbnailCache::new(4);
        assert!(cache.insert("keep", vec![1, 2]));
        assert!(!cache.insert("too-large", vec![0; 5]));
        assert!(cache.get("keep").is_some());
        assert_eq!(cache.used_bytes(), 2);
    }
 
    #[test]
    fn shrinking_budget_evicts_lru_entries_and_keeps_accounting_valid() {
        let mut cache = ThumbnailCache::new(10);
        assert!(cache.insert("a", vec![1, 2, 3]));
        assert!(cache.insert("b", vec![4, 5, 6]));
        assert!(cache.insert("c", vec![7, 8, 9]));
        assert_eq!(cache.get("a"), Some(&[1, 2, 3][..]));

        cache.set_max_bytes(5);

        assert_eq!(cache.max_bytes(), 5);
        assert!(cache.get("a").is_some(), "recently used a should remain");
        assert!(cache.get("b").is_none(), "least recently used b should be evicted first");
        assert!(cache.get("c").is_none(), "c is evicted to satisfy the new budget");
        assert_eq!(cache.used_bytes(), 3);
        assert!(cache.used_bytes() <= cache.max_bytes());
    }

    #[test]
    fn increasing_budget_does_not_discard_cached_entries() {
        let mut cache = ThumbnailCache::new(3);
        assert!(cache.insert("a", vec![1, 2, 3]));
        cache.set_max_bytes(8);
        assert_eq!(cache.get("a"), Some(&[1, 2, 3][..]));
        assert_eq!(cache.used_bytes(), 3);
    }

    #[test]
    fn clear_releases_every_cached_byte() {
        let mut cache = ThumbnailCache::new(10);
        cache.insert("one", vec![1, 2, 3]);
        cache.insert("two", vec![4, 5]);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.used_bytes(), 0);
    }

    #[test]
    fn retain_prunes_stale_entries_and_preserves_lru_order() {
        let mut cache = ThumbnailCache::new(10);
        assert!(cache.insert("film/a", vec![1, 2]));
        assert!(cache.insert("film/stale", vec![3, 4, 5]));
        assert!(cache.insert("film/b", vec![6, 7]));
        assert!(cache.insert("other/c", vec![8, 9]));
        assert_eq!(cache.get("film/a"), Some(&[1, 2][..]));

        cache.retain(|key| key != "film/stale" && key.starts_with("film/"));

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.used_bytes(), 4);
        assert!(cache.get("film/a").is_some());
        assert!(cache.get("film/b").is_some());
        assert!(cache.get("film/stale").is_none());
        assert!(cache.get("other/c").is_none());

        assert!(cache.insert("film/c", vec![10, 11, 12, 13, 14, 15, 16]));
        assert!(cache.get("film/a").is_none(), "a was the oldest retained entry");
        assert!(cache.get("film/b").is_some(), "b retained its newer LRU position");
        assert_eq!(cache.used_bytes(), 9);
    }
}
