//! In-memory cache for fetched web content

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// In-memory cache for fetched content
pub struct FetchCache {
    entries: HashMap<String, CacheEntry>,
    ttl: Duration,
    max_entries: usize,
}

struct CacheEntry {
    content: String,
    fetched_at: Instant,
}

impl FetchCache {
    /// Create a new cache with specified TTL and max entries
    #[must_use]
    pub fn new(ttl_secs: u64, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
            max_entries,
        }
    }

    /// Get cached content if present and not expired
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&str> {
        self.entries.get(url).and_then(|entry| {
            if entry.fetched_at.elapsed() < self.ttl {
                Some(entry.content.as_str())
            } else {
                None
            }
        })
    }

    /// Insert content into cache, evicting oldest if at capacity
    pub fn insert(&mut self, url: String, content: String) {
        // Evict oldest if at capacity
        if self.entries.len() >= self.max_entries {
            self.evict_oldest();
        }
        self.entries.insert(
            url,
            CacheEntry {
                content,
                fetched_at: Instant::now(),
            },
        );
    }

    /// Remove expired entries
    pub fn cleanup(&mut self) {
        self.entries
            .retain(|_, entry| entry.fetched_at.elapsed() < self.ttl);
    }

    /// Get number of entries in cache
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.fetched_at)
            .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = FetchCache::new(60, 10);
        cache.insert("https://example.com".to_string(), "# Hello".to_string());

        assert_eq!(cache.get("https://example.com"), Some("# Hello"));
        assert_eq!(cache.get("https://other.com"), None);
    }

    #[test]
    fn test_cache_expiry() {
        let mut cache = FetchCache::new(0, 10); // 0 second TTL
        cache.insert("https://example.com".to_string(), "# Hello".to_string());

        // Immediately expired due to 0 TTL
        sleep(Duration::from_millis(10));
        assert_eq!(cache.get("https://example.com"), None);
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = FetchCache::new(60, 2); // Max 2 entries
        cache.insert("https://one.com".to_string(), "one".to_string());
        cache.insert("https://two.com".to_string(), "two".to_string());
        cache.insert("https://three.com".to_string(), "three".to_string());

        // Should have evicted oldest
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("https://one.com"), None);
        assert!(cache.get("https://two.com").is_some() || cache.get("https://three.com").is_some());
    }

    #[test]
    fn test_cache_cleanup() {
        let mut cache = FetchCache::new(0, 10); // 0 second TTL
        cache.insert("https://example.com".to_string(), "content".to_string());

        sleep(Duration::from_millis(10));
        cache.cleanup();

        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_update_existing() {
        let mut cache = FetchCache::new(60, 10);
        cache.insert("https://example.com".to_string(), "old".to_string());
        cache.insert("https://example.com".to_string(), "new".to_string());

        assert_eq!(cache.get("https://example.com"), Some("new"));
        assert_eq!(cache.len(), 1);
    }
}
