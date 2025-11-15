//! LRU caching for Merkle tree nodes and sections
//!
//! This module provides bounded memory management for Merkle tree operations,
//! preventing memory exhaustion when working with large documents or many trees.
//!
//! ## Design Goals
//!
//! - **Bounded memory**: Configurable cache size prevents unbounded growth
//! - **Thread-safe**: Safe for concurrent access across multiple threads
//! - **High hit ratio**: >80% cache hits for typical document processing workloads
//! - **Performance**: Fast lookups with LRU eviction strategy
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use crucible_core::merkle::{MerkleCache, NodeHash};
//!
//! let cache = MerkleCache::new(1000); // 1000 node capacity
//!
//! let hash = NodeHash::from_content(b"data");
//! cache.put_node(hash, node_data);
//!
//! if let Some(data) = cache.get_node(&hash) {
//!     // Cache hit!
//! }
//! ```

use lru::LruCache;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::merkle::{NodeHash, SectionNode};

/// Configuration for Merkle tree caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum number of nodes to cache
    pub node_capacity: usize,
    /// Maximum number of sections to cache
    pub section_capacity: usize,
}

impl CacheConfig {
    /// Default cache configuration for typical workloads
    ///
    /// Based on analysis of document processing patterns:
    /// - Average document: ~100-500 blocks
    /// - Average Merkle tree: ~200-1000 nodes
    /// - Default capacity handles ~10-20 concurrent documents
    pub fn default() -> Self {
        Self {
            node_capacity: 1000,
            section_capacity: 500,
        }
    }

    /// Configuration for small documents (<100 blocks)
    pub fn small() -> Self {
        Self {
            node_capacity: 500,
            section_capacity: 250,
        }
    }

    /// Configuration for large documents (>1000 blocks)
    pub fn large() -> Self {
        Self {
            node_capacity: 5000,
            section_capacity: 2500,
        }
    }

    /// Configuration for memory-constrained environments
    pub fn minimal() -> Self {
        Self {
            node_capacity: 100,
            section_capacity: 50,
        }
    }
}

/// Cached Merkle tree node data
///
/// Stores minimal information needed for tree operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedNode {
    /// Hash of this node
    pub hash: NodeHash,
    /// Whether this is a leaf node
    pub is_leaf: bool,
    /// For leaf nodes: block index
    pub block_index: Option<usize>,
    /// For internal nodes: left and right child hashes
    pub children: Option<(NodeHash, NodeHash)>,
}

/// Statistics about cache performance
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total number of cache lookups
    pub total_lookups: u64,
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Current number of cached nodes
    pub node_count: usize,
    /// Current number of cached sections
    pub section_count: usize,
}

impl CacheStats {
    /// Calculate cache hit ratio (0.0 to 1.0)
    pub fn hit_ratio(&self) -> f64 {
        if self.total_lookups == 0 {
            0.0
        } else {
            self.hits as f64 / self.total_lookups as f64
        }
    }

    /// Calculate cache miss ratio (0.0 to 1.0)
    pub fn miss_ratio(&self) -> f64 {
        1.0 - self.hit_ratio()
    }

    /// Check if hit ratio meets target threshold
    pub fn meets_target(&self, target_ratio: f64) -> bool {
        self.hit_ratio() >= target_ratio
    }
}

/// Thread-safe LRU cache for Merkle tree nodes and sections
///
/// This cache uses the LRU (Least Recently Used) eviction strategy to maintain
/// bounded memory usage while optimizing for frequently accessed nodes.
///
/// ## Thread Safety
///
/// The cache uses `parking_lot::Mutex` for interior mutability, allowing safe
/// concurrent access from multiple threads. Read and write operations acquire
/// locks, but the fast mutex implementation minimizes contention.
///
/// When sharing across threads, wrap the entire cache in `Arc`:
/// ```rust,ignore
/// let cache = Arc::new(MerkleCache::new(1000));
/// let cache_clone = Arc::clone(&cache);
/// ```
///
/// ## Memory Management
///
/// - **Bounded capacity**: Configured at creation, prevents unlimited growth
/// - **Automatic eviction**: Least recently used entries are evicted when capacity is reached
/// - **Separate node and section caches**: Different access patterns optimized separately
pub struct MerkleCache {
    /// Cache for tree nodes
    nodes: Mutex<LruCache<NodeHash, CachedNode>>,
    /// Cache for sections
    sections: Mutex<LruCache<NodeHash, SectionNode>>,
    /// Cache statistics
    stats: Mutex<CacheStats>,
    /// Cache configuration
    config: CacheConfig,
}

impl MerkleCache {
    /// Create a new cache with default configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cache = MerkleCache::new(1000);
    /// ```
    pub fn new(capacity: usize) -> Self {
        Self::with_config(CacheConfig {
            node_capacity: capacity,
            section_capacity: capacity / 2,
        })
    }

    /// Create a new cache with custom configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = CacheConfig::large();
    /// let cache = MerkleCache::with_config(config);
    /// ```
    pub fn with_config(config: CacheConfig) -> Self {
        let node_capacity = NonZeroUsize::new(config.node_capacity)
            .unwrap_or(NonZeroUsize::new(1000).unwrap());
        let section_capacity = NonZeroUsize::new(config.section_capacity)
            .unwrap_or(NonZeroUsize::new(500).unwrap());

        Self {
            nodes: Mutex::new(LruCache::new(node_capacity)),
            sections: Mutex::new(LruCache::new(section_capacity)),
            stats: Mutex::new(CacheStats::default()),
            config,
        }
    }

    /// Get a cached node by hash
    ///
    /// # Arguments
    ///
    /// * `hash` - The node hash to look up
    ///
    /// # Returns
    ///
    /// The cached node if found, or `None` if not in cache
    pub fn get_node(&self, hash: &NodeHash) -> Option<CachedNode> {
        let mut nodes = self.nodes.lock();
        let mut stats = self.stats.lock();

        stats.total_lookups += 1;

        if let Some(node) = nodes.get(hash) {
            stats.hits += 1;
            Some(node.clone())
        } else {
            stats.misses += 1;
            None
        }
    }

    /// Put a node into the cache
    ///
    /// If the cache is at capacity, the least recently used node will be evicted.
    ///
    /// # Arguments
    ///
    /// * `hash` - The node hash (key)
    /// * `node` - The node data to cache
    pub fn put_node(&self, hash: NodeHash, node: CachedNode) {
        let mut nodes = self.nodes.lock();
        nodes.put(hash, node);
    }

    /// Get a cached section by hash
    ///
    /// # Arguments
    ///
    /// * `hash` - The section root hash to look up
    ///
    /// # Returns
    ///
    /// The cached section if found, or `None` if not in cache
    pub fn get_section(&self, hash: &NodeHash) -> Option<SectionNode> {
        let mut sections = self.sections.lock();
        let mut stats = self.stats.lock();

        stats.total_lookups += 1;

        if let Some(section) = sections.get(hash) {
            stats.hits += 1;
            Some(section.clone())
        } else {
            stats.misses += 1;
            None
        }
    }

    /// Put a section into the cache
    ///
    /// # Arguments
    ///
    /// * `hash` - The section root hash (key)
    /// * `section` - The section data to cache
    pub fn put_section(&self, hash: NodeHash, section: SectionNode) {
        let mut sections = self.sections.lock();
        sections.put(hash, section);
    }

    /// Clear all cached data
    ///
    /// This removes all nodes and sections from the cache but preserves
    /// the cache statistics.
    pub fn clear(&self) {
        self.nodes.lock().clear();
        self.sections.lock().clear();
    }

    /// Reset cache statistics
    ///
    /// This clears all performance counters but preserves cached data.
    pub fn reset_stats(&self) {
        *self.stats.lock() = CacheStats::default();
    }

    /// Get current cache statistics
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let stats = cache.stats();
    /// println!("Hit ratio: {:.2}%", stats.hit_ratio() * 100.0);
    /// ```
    pub fn stats(&self) -> CacheStats {
        let mut stats = self.stats.lock().clone();
        stats.node_count = self.nodes.lock().len();
        stats.section_count = self.sections.lock().len();
        stats
    }

    /// Get the cache configuration
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Get current node cache size
    pub fn node_count(&self) -> usize {
        self.nodes.lock().len()
    }

    /// Get current section cache size
    pub fn section_count(&self) -> usize {
        self.sections.lock().len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.node_count() == 0 && self.section_count() == 0
    }

    /// Create a new cache with the same configuration but independent state
    ///
    /// This creates a new cache with the same configuration but independent
    /// LRU state and statistics. Useful for creating per-thread caches.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cache1 = Arc::new(MerkleCache::new(1000));
    /// let cache2 = Arc::new(cache1.new_with_same_config()); // Independent cache
    /// ```
    pub fn new_with_same_config(&self) -> Self {
        Self::with_config(self.config.clone())
    }
}

impl Default for MerkleCache {
    fn default() -> Self {
        Self::with_config(CacheConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle::hybrid::HeadingSummary;
    use crate::merkle::hybrid::BinaryMerkleTree;

    #[test]
    fn test_cache_creation() {
        let cache = MerkleCache::new(1000);
        assert_eq!(cache.node_count(), 0);
        assert_eq!(cache.section_count(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_config() {
        let config = CacheConfig::default();
        assert_eq!(config.node_capacity, 1000);
        assert_eq!(config.section_capacity, 500);

        let small = CacheConfig::small();
        assert_eq!(small.node_capacity, 500);

        let large = CacheConfig::large();
        assert_eq!(large.node_capacity, 5000);
    }

    #[test]
    fn test_node_caching() {
        let cache = MerkleCache::new(10);

        let hash = NodeHash::from_content(b"test node");
        let node = CachedNode {
            hash,
            is_leaf: true,
            block_index: Some(0),
            children: None,
        };

        // Initially not in cache
        assert!(cache.get_node(&hash).is_none());

        // Put and retrieve
        cache.put_node(hash, node.clone());
        assert_eq!(cache.node_count(), 1);

        let cached = cache.get_node(&hash).unwrap();
        assert_eq!(cached.hash, hash);
        assert!(cached.is_leaf);
        assert_eq!(cached.block_index, Some(0));
    }

    #[test]
    fn test_section_caching() {
        let cache = MerkleCache::new(10);

        let hash = NodeHash::from_content(b"test section");
        let section = SectionNode {
            heading: Some(HeadingSummary {
                text: "Test".to_string(),
                level: 1,
            }),
            depth: 1,
            binary_tree: BinaryMerkleTree::empty(),
            block_count: 0,
        };

        // Initially not in cache
        assert!(cache.get_section(&hash).is_none());

        // Put and retrieve
        cache.put_section(hash, section.clone());
        assert_eq!(cache.section_count(), 1);

        let cached = cache.get_section(&hash).unwrap();
        assert_eq!(cached.heading.as_ref().unwrap().text, "Test");
        assert_eq!(cached.depth, 1);
    }

    #[test]
    fn test_lru_eviction() {
        let cache = MerkleCache::new(3); // Small capacity

        // Add 4 nodes (one over capacity)
        for i in 0..4 {
            let hash = NodeHash::from_content(format!("node {}", i).as_bytes());
            let node = CachedNode {
                hash,
                is_leaf: true,
                block_index: Some(i),
                children: None,
            };
            cache.put_node(hash, node);
        }

        // Cache should have exactly 3 nodes
        assert_eq!(cache.node_count(), 3);

        // First node should have been evicted
        let first_hash = NodeHash::from_content(b"node 0");
        assert!(cache.get_node(&first_hash).is_none());

        // Last 3 nodes should be present
        for i in 1..4 {
            let hash = NodeHash::from_content(format!("node {}", i).as_bytes());
            assert!(cache.get_node(&hash).is_some());
        }
    }

    #[test]
    fn test_cache_stats() {
        let cache = MerkleCache::new(10);

        let hash1 = NodeHash::from_content(b"node 1");
        let hash2 = NodeHash::from_content(b"node 2");

        let node = CachedNode {
            hash: hash1,
            is_leaf: true,
            block_index: Some(0),
            children: None,
        };

        cache.put_node(hash1, node);

        // One hit (hash1 in cache)
        assert!(cache.get_node(&hash1).is_some());

        // One miss (hash2 not in cache)
        assert!(cache.get_node(&hash2).is_none());

        let stats = cache.stats();
        assert_eq!(stats.total_lookups, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_ratio(), 0.5);
        assert_eq!(stats.miss_ratio(), 0.5);
    }

    #[test]
    fn test_cache_clear() {
        let cache = MerkleCache::new(10);

        // Add some nodes
        for i in 0..5 {
            let hash = NodeHash::from_content(format!("node {}", i).as_bytes());
            let node = CachedNode {
                hash,
                is_leaf: true,
                block_index: Some(i),
                children: None,
            };
            cache.put_node(hash, node);
        }

        assert_eq!(cache.node_count(), 5);

        cache.clear();
        assert_eq!(cache.node_count(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_stats_reset() {
        let cache = MerkleCache::new(10);

        let hash = NodeHash::from_content(b"test");
        let node = CachedNode {
            hash,
            is_leaf: true,
            block_index: Some(0),
            children: None,
        };

        cache.put_node(hash, node);
        cache.get_node(&hash);

        let stats = cache.stats();
        assert!(stats.total_lookups > 0);

        cache.reset_stats();

        let stats = cache.stats();
        assert_eq!(stats.total_lookups, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(MerkleCache::new(100));
        let mut handles = vec![];

        // Spawn multiple threads accessing the cache
        for thread_id in 0..4 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..25 {
                    let hash = NodeHash::from_content(
                        format!("thread {} node {}", thread_id, i).as_bytes()
                    );
                    let node = CachedNode {
                        hash,
                        is_leaf: true,
                        block_index: Some(i),
                        children: None,
                    };
                    cache_clone.put_node(hash, node);

                    // Try to get it back
                    assert!(cache_clone.get_node(&hash).is_some());
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Cache should have all nodes (within capacity)
        assert_eq!(cache.node_count(), 100);
    }

    #[test]
    fn test_shared_vs_independent_cache() {
        use std::sync::Arc;

        let cache1 = Arc::new(MerkleCache::new(10));

        let hash = NodeHash::from_content(b"test");
        let node = CachedNode {
            hash,
            is_leaf: true,
            block_index: Some(0),
            children: None,
        };

        cache1.put_node(hash, node);

        // Arc::clone shares storage
        let cache2 = Arc::clone(&cache1);
        assert!(cache2.get_node(&hash).is_some());

        // new_with_same_config creates independent cache
        let cache3 = cache1.new_with_same_config();
        assert!(cache3.get_node(&hash).is_none());
    }

    #[test]
    fn test_hit_ratio_calculation() {
        let stats = CacheStats {
            total_lookups: 100,
            hits: 85,
            misses: 15,
            node_count: 50,
            section_count: 25,
        };

        // Use approximate equality for floating point comparison
        assert!((stats.hit_ratio() - 0.85).abs() < 1e-10);
        assert!((stats.miss_ratio() - 0.15).abs() < 1e-10);
        assert!(stats.meets_target(0.80));
        assert!(!stats.meets_target(0.90));
    }

    #[test]
    fn test_internal_node_caching() {
        let cache = MerkleCache::new(10);

        let hash = NodeHash::from_content(b"internal node");
        let left = NodeHash::from_content(b"left child");
        let right = NodeHash::from_content(b"right child");

        let node = CachedNode {
            hash,
            is_leaf: false,
            block_index: None,
            children: Some((left, right)),
        };

        cache.put_node(hash, node);

        let cached = cache.get_node(&hash).unwrap();
        assert!(!cached.is_leaf);
        assert_eq!(cached.block_index, None);
        assert_eq!(cached.children, Some((left, right)));
    }
}
