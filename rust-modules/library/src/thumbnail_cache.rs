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
    fn clear_releases_every_cached_byte() {
        let mut cache = ThumbnailCache::new(10);
        cache.insert("one", vec![1, 2, 3]);
        cache.insert("two", vec![4, 5]);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.used_bytes(), 0);
    }
}
