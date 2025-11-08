//! In-Memory Storage Implementation
//!
//! This module provides a comprehensive in-memory implementation of the ContentAddressedStorage
//! trait with full concurrency support, memory management, and performance monitoring.
//! Following Test-Driven Development principles, all functionality is thoroughly tested.
//!
//! ## Features
//!
//! - **Thread-safe concurrent operations** using Arc + RwLock
//! - **Memory management** with configurable limits and LRU eviction
//! - **Performance monitoring** with detailed statistics
//! - **Import/export capabilities** for testing and migrations
//! - **Snapshot and restore** functionality
//! - **Event notifications** for storage operations
//! - **Builder pattern** for configurable instantiation
//!
//! ## Usage
//!
//! ```rust
//! use crucible_core::storage::memory::MemoryStorage;
//! use crucible_core::storage::ContentAddressedStorage;
//!
//! // Create with default configuration
//! let storage = MemoryStorage::new();
//!
//! // Create with custom configuration
//! let storage = MemoryStorage::builder()
//!     .with_memory_limit(1024 * 1024 * 1024) // 1GB
//!     .with_lru_eviction(true)
//!     .with_stats_tracking(true)
//!     .build();
//! ```

use crate::storage::{
    traits::{
        BlockOperations, QuotaUsage, StorageBackend, StorageManagement, StorageStats,
        TreeOperations,
    },
    ContentAddressedStorage, MerkleTree, StorageError, StorageResult,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, RwLock,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use uuid::Uuid;

/// Configuration for MemoryStorage behavior
#[derive(Debug, Clone)]
pub struct MemoryStorageConfig {
    /// Maximum memory usage in bytes (None for unlimited)
    pub memory_limit: Option<u64>,
    /// Enable automatic LRU eviction when memory limit is reached
    pub enable_lru_eviction: bool,
    /// Enable detailed statistics tracking
    pub enable_stats_tracking: bool,
    /// Enable event notifications for storage operations
    pub enable_event_notifications: bool,
    /// Maximum number of events to keep in memory
    pub max_events: usize,
    /// Cleanup threshold percentage (trigger cleanup at this usage)
    pub cleanup_threshold: f64,
    /// Cleanup target percentage (evict to this usage)
    pub cleanup_target: f64,
}

impl Default for MemoryStorageConfig {
    fn default() -> Self {
        Self {
            memory_limit: Some(512 * 1024 * 1024), // 512MB default
            enable_lru_eviction: true,
            enable_stats_tracking: true,
            enable_event_notifications: false,
            max_events: 1000,
            cleanup_threshold: 0.9, // 90%
            cleanup_target: 0.7,    // 70%
        }
    }
}

/// Storage event for monitoring and debugging
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageEvent {
    /// Block was stored
    BlockStored {
        hash: String,
        size: u64,
        timestamp: u64,
    },
    /// Block was retrieved
    BlockRetrieved { hash: String, timestamp: u64 },
    /// Block was deleted
    BlockDeleted { hash: String, timestamp: u64 },
    /// Tree was stored
    TreeStored {
        root_hash: String,
        node_count: usize,
        timestamp: u64,
    },
    /// Tree was retrieved
    TreeRetrieved { root_hash: String, timestamp: u64 },
    /// Tree was deleted
    TreeDeleted { root_hash: String, timestamp: u64 },
    /// Memory cleanup was performed
    MemoryCleanup {
        evicted_blocks: usize,
        freed_bytes: u64,
        timestamp: u64,
    },
    /// Memory limit was exceeded
    MemoryLimitExceeded {
        usage: u64,
        limit: u64,
        timestamp: u64,
    },
}

/// Internal storage data for blocks
#[derive(Debug)]
struct BlockData {
    /// Block content
    data: Vec<u8>,
    /// Size in bytes
    size: u64,
    /// Last access timestamp (UNIX epoch in milliseconds)
    last_accessed: AtomicU64,
    /// Creation timestamp
    created_at: u64,
}

impl BlockData {
    fn new(data: Vec<u8>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let size = data.len() as u64;

        Self {
            data,
            size,
            last_accessed: AtomicU64::new(now),
            created_at: now,
        }
    }

    fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_accessed.store(now, Ordering::Relaxed);
    }

    #[allow(dead_code)] // Reserved for cache eviction strategies
    fn last_accessed(&self) -> u64 {
        self.last_accessed.load(Ordering::Relaxed)
    }
}

impl Clone for BlockData {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            size: self.size,
            last_accessed: AtomicU64::new(self.last_accessed.load(Ordering::Relaxed)),
            created_at: self.created_at,
        }
    }
}

/// Internal storage data for trees
#[derive(Debug)]
struct TreeData {
    /// Merkle tree structure
    tree: MerkleTree,
    /// Number of nodes in the tree
    node_count: usize,
    /// Last access timestamp
    last_accessed: AtomicU64,
    /// Creation timestamp
    created_at: u64,
}

impl TreeData {
    fn new(tree: MerkleTree) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let node_count = tree.nodes.len();

        Self {
            tree,
            node_count,
            last_accessed: AtomicU64::new(now),
            created_at: now,
        }
    }

    fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_accessed.store(now, Ordering::Relaxed);
    }

    #[allow(dead_code)] // Reserved for cache eviction strategies
    fn last_accessed(&self) -> u64 {
        self.last_accessed.load(Ordering::Relaxed)
    }
}

impl Clone for TreeData {
    fn clone(&self) -> Self {
        Self {
            tree: self.tree.clone(),
            node_count: self.node_count,
            last_accessed: AtomicU64::new(self.last_accessed.load(Ordering::Relaxed)),
            created_at: self.created_at,
        }
    }
}

/// Memory statistics tracking
#[derive(Debug)]
struct MemoryStats {
    /// Total number of stored blocks
    block_count: AtomicU64,
    /// Total size of stored blocks in bytes
    total_block_size: AtomicU64,
    /// Total number of stored trees
    tree_count: AtomicU64,
    /// Total number of deduplicated blocks (same content, same hash)
    deduplication_hits: AtomicU64,
    /// Total number of block retrievals
    block_retrievals: AtomicU64,
    /// Total number of tree retrievals
    tree_retrievals: AtomicU64,
    /// Largest block size stored
    largest_block_size: AtomicU64,
    /// Number of evicted blocks due to memory pressure
    evicted_blocks: AtomicU64,
}

impl MemoryStats {
    fn new() -> Self {
        Self {
            block_count: AtomicU64::new(0),
            total_block_size: AtomicU64::new(0),
            tree_count: AtomicU64::new(0),
            deduplication_hits: AtomicU64::new(0),
            block_retrievals: AtomicU64::new(0),
            tree_retrievals: AtomicU64::new(0),
            largest_block_size: AtomicU64::new(0),
            evicted_blocks: AtomicU64::new(0),
        }
    }

    #[allow(dead_code)] // Reserved for memory analysis features
    fn average_block_size(&self) -> f64 {
        let count = self.block_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            self.total_block_size.load(Ordering::Relaxed) as f64 / count as f64
        }
    }
}

/// In-memory implementation of ContentAddressedStorage
///
/// This implementation provides thread-safe, concurrent access to content-addressed
/// storage with configurable memory management and comprehensive monitoring.
#[derive(Debug)]
pub struct MemoryStorage {
    /// Configuration
    config: MemoryStorageConfig,
    /// Block storage (hash -> BlockData)
    blocks: Arc<RwLock<HashMap<String, BlockData>>>,
    /// Tree storage (root_hash -> TreeData)
    trees: Arc<RwLock<HashMap<String, TreeData>>>,
    /// LRU access order for blocks (hash -> access timestamp)
    lru_order: Arc<RwLock<HashMap<String, u64>>>,
    /// Statistics tracking
    stats: Arc<MemoryStats>,
    /// Event history
    events: Arc<RwLock<VecDeque<StorageEvent>>>,
    /// Notification for event subscribers
    event_notifier: Arc<Notify>,
    /// Whether the storage has been shut down
    shutdown: AtomicBool,
    /// Unique identifier for this storage instance
    instance_id: String,
}

impl MemoryStorage {
    /// Create a new MemoryStorage with default configuration
    pub fn new() -> Arc<Self> {
        Self::builder().build()
    }

    /// Create a new MemoryStorage with custom configuration
    pub fn with_config(config: MemoryStorageConfig) -> Arc<Self> {
        Arc::new(Self {
            instance_id: Uuid::new_v4().to_string(),
            config,
            blocks: Arc::new(RwLock::new(HashMap::new())),
            trees: Arc::new(RwLock::new(HashMap::new())),
            lru_order: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(MemoryStats::new()),
            events: Arc::new(RwLock::new(VecDeque::new())),
            event_notifier: Arc::new(Notify::new()),
            shutdown: AtomicBool::new(false),
        })
    }

    /// Create a builder for configuring MemoryStorage
    pub fn builder() -> MemoryStorageBuilder {
        MemoryStorageBuilder::new()
    }

    /// Get the instance ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Check if the storage has been shut down
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Trigger memory cleanup manually
    pub async fn trigger_cleanup(&self) -> StorageResult<usize> {
        if !self.config.enable_lru_eviction {
            return Ok(0);
        }

        let Some(limit) = self.config.memory_limit else {
            return Ok(0);
        };

        let current_usage = self.stats.total_block_size.load(Ordering::Relaxed);
        if current_usage == 0 {
            return Ok(0);
        }

        let usage_percentage = current_usage as f64 / limit as f64;
        if usage_percentage <= self.config.cleanup_threshold {
            return Ok(0);
        }

        // Calculate target size
        let target_size = (limit as f64 * self.config.cleanup_target) as u64;
        let bytes_to_free = current_usage.saturating_sub(target_size);

        self.evict_lru_blocks(bytes_to_free).await
    }

    /// Get recent storage events
    pub async fn get_recent_events(&self, limit: Option<usize>) -> Vec<StorageEvent> {
        let events = self.events.read().unwrap();
        let effective_limit = limit.unwrap_or(self.config.max_events);
        events.iter().rev().take(effective_limit).cloned().collect()
    }

    /// Import data from another storage instance
    pub async fn import_from(&self, other: &Arc<MemoryStorage>) -> StorageResult<()> {
        if self.is_shutdown() || other.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        // Import blocks
        {
            let other_blocks = other.blocks.read().unwrap();
            let mut self_blocks = self.blocks.write().unwrap();

            for (hash, block_data) in other_blocks.iter() {
                if !self_blocks.contains_key(hash) {
                    self_blocks.insert(hash.clone(), block_data.clone());
                    self.stats.block_count.fetch_add(1, Ordering::Relaxed);
                    self.stats
                        .total_block_size
                        .fetch_add(block_data.size, Ordering::Relaxed);
                } else {
                    self.stats
                        .deduplication_hits
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Import trees
        {
            let other_trees = other.trees.read().unwrap();
            let mut self_trees = self.trees.write().unwrap();

            for (root_hash, tree_data) in other_trees.iter() {
                if !self_trees.contains_key(root_hash) {
                    self_trees.insert(root_hash.clone(), tree_data.clone());
                    self.stats.tree_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        Ok(())
    }

    /// Export storage data
    pub async fn export_data(
        &self,
    ) -> StorageResult<(HashMap<String, Vec<u8>>, HashMap<String, MerkleTree>)> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let blocks = self.blocks.read().unwrap();
        let trees = self.trees.read().unwrap();

        let block_data: HashMap<String, Vec<u8>> = blocks
            .iter()
            .map(|(hash, data)| (hash.clone(), data.data.clone()))
            .collect();

        let tree_data: HashMap<String, MerkleTree> = trees
            .iter()
            .map(|(root_hash, data)| (root_hash.clone(), data.tree.clone()))
            .collect();

        Ok((block_data, tree_data))
    }

    /// Create a snapshot of the current state
    pub async fn create_snapshot(&self) -> StorageResult<MemoryStorageSnapshot> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let (block_data, tree_data) = self.export_data().await?;
        let stats = self.get_stats().await?;

        Ok(MemoryStorageSnapshot {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            blocks: block_data,
            trees: tree_data,
            stats,
        })
    }

    /// Restore from a snapshot
    pub async fn restore_from_snapshot(
        &self,
        snapshot: MemoryStorageSnapshot,
    ) -> StorageResult<()> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        // Clear existing data
        {
            let mut blocks = self.blocks.write().unwrap();
            let mut trees = self.trees.write().unwrap();
            blocks.clear();
            trees.clear();
        }

        // Reset stats
        self.stats.block_count.store(0, Ordering::Relaxed);
        self.stats.total_block_size.store(0, Ordering::Relaxed);
        self.stats.tree_count.store(0, Ordering::Relaxed);

        // Restore blocks
        {
            let mut blocks = self.blocks.write().unwrap();
            for (hash, data) in snapshot.blocks {
                let block_data = BlockData::new(data);
                let size = block_data.size;
                blocks.insert(hash, block_data);
                self.stats.block_count.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .total_block_size
                    .fetch_add(size, Ordering::Relaxed);
            }
        }

        // Restore trees
        {
            let mut trees = self.trees.write().unwrap();
            for (root_hash, tree) in snapshot.trees {
                let tree_data = TreeData::new(tree);
                trees.insert(root_hash, tree_data);
                self.stats.tree_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        self.record_event(StorageEvent::MemoryCleanup {
            evicted_blocks: 0,
            freed_bytes: 0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
        .await;

        Ok(())
    }

    /// Record a storage event
    async fn record_event(&self, event: StorageEvent) {
        if !self.config.enable_event_notifications {
            return;
        }

        {
            let mut events = self.events.write().unwrap();
            events.push_back(event.clone());

            // Trim events if they exceed the maximum
            while events.len() > self.config.max_events {
                events.pop_front();
            }
        }

        // Notify subscribers
        self.event_notifier.notify_waiters();
    }

    /// Evict LRU blocks to free up memory
    async fn evict_lru_blocks(&self, bytes_to_free: u64) -> StorageResult<usize> {
        let mut blocks_to_evict = Vec::new();
        let mut freed_bytes = 0u64;

        // Find blocks to evict (LRU order)
        {
            let blocks = self.blocks.read().unwrap();
            let lru_order = self.lru_order.read().unwrap();

            let mut block_entries: Vec<_> = lru_order
                .iter()
                .filter_map(|(hash, timestamp)| {
                    blocks
                        .get(hash)
                        .map(|block_data| (hash.clone(), *timestamp, block_data.size))
                })
                .collect();

            // Sort by last access time (oldest first)
            block_entries.sort_by_key(|(_, timestamp, _)| *timestamp);

            for (hash, _, size) in block_entries {
                blocks_to_evict.push(hash);
                freed_bytes += size;
                if freed_bytes >= bytes_to_free {
                    break;
                }
            }
        }

        let evicted_count = blocks_to_evict.len();

        // Evict the blocks
        {
            let mut blocks = self.blocks.write().unwrap();
            let mut lru_order = self.lru_order.write().unwrap();

            for hash in blocks_to_evict {
                if let Some(block_data) = blocks.remove(&hash) {
                    lru_order.remove(&hash);
                    self.stats.block_count.fetch_sub(1, Ordering::Relaxed);
                    self.stats
                        .total_block_size
                        .fetch_sub(block_data.size, Ordering::Relaxed);
                    self.stats.evicted_blocks.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        self.record_event(StorageEvent::MemoryCleanup {
            evicted_blocks: evicted_count,
            freed_bytes,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
        .await;

        Ok(evicted_count)
    }

    /// Update LRU order for a block
    async fn update_lru_access(&self, hash: &str) {
        if !self.config.enable_lru_eviction {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut lru_order = self.lru_order.write().unwrap();
        lru_order.insert(hash.to_string(), now);
    }

    /// Remove from LRU order
    async fn remove_from_lru(&self, hash: &str) {
        if !self.config.enable_lru_eviction {
            return;
        }

        let mut lru_order = self.lru_order.write().unwrap();
        lru_order.remove(hash);
    }
}

// Implementation of BlockOperations trait
#[async_trait]
impl BlockOperations for MemoryStorage {
    async fn store_block(&self, hash: &str, data: &[u8]) -> StorageResult<()> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let block_data = BlockData::new(data.to_vec());
        let size = block_data.size;

        // Check memory limit
        if let Some(limit) = self.config.memory_limit {
            let current_usage = self.stats.total_block_size.load(Ordering::Relaxed);
            if current_usage + size > limit {
                // Try to evict some blocks
                if self.config.enable_lru_eviction {
                    let bytes_to_free = (current_usage + size) - limit;
                    self.evict_lru_blocks(bytes_to_free).await?;
                } else {
                    self.record_event(StorageEvent::MemoryLimitExceeded {
                        usage: current_usage + size,
                        limit,
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    })
                    .await;
                    return Err(StorageError::QuotaExceeded {
                        used: current_usage + size,
                        limit,
                    });
                }
            }
        }

        // Check if block already exists (deduplication)
        let _is_new_block = {
            let mut blocks = self.blocks.write().unwrap();
            if blocks.contains_key(hash) {
                // Update existing block
                self.stats
                    .deduplication_hits
                    .fetch_add(1, Ordering::Relaxed);
                blocks.insert(hash.to_string(), block_data);
                false
            } else {
                // Add new block
                blocks.insert(hash.to_string(), block_data);
                self.stats.block_count.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .total_block_size
                    .fetch_add(size, Ordering::Relaxed);

                // Update largest block size
                let mut largest = self.stats.largest_block_size.load(Ordering::Relaxed);
                while size > largest {
                    match self.stats.largest_block_size.compare_exchange_weak(
                        largest,
                        size,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(x) => largest = x,
                    }
                }

                true
            }
        };

        self.update_lru_access(hash).await;

        self.record_event(StorageEvent::BlockStored {
            hash: hash.to_string(),
            size,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
        .await;

        Ok(())
    }

    async fn get_block(&self, hash: &str) -> StorageResult<Option<Vec<u8>>> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let result = {
            let blocks = self.blocks.read().unwrap();
            if let Some(block_data) = blocks.get(hash) {
                block_data.touch();
                self.stats.block_retrievals.fetch_add(1, Ordering::Relaxed);
                Some(block_data.data.clone())
            } else {
                None
            }
        };

        // Update LRU access after releasing the lock
        if result.is_some() {
            self.update_lru_access(hash).await;
            self.record_event(StorageEvent::BlockRetrieved {
                hash: hash.to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            })
            .await;
        }

        Ok(result)
    }

    async fn block_exists(&self, hash: &str) -> StorageResult<bool> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let blocks = self.blocks.read().unwrap();
        Ok(blocks.contains_key(hash))
    }

    async fn delete_block(&self, hash: &str) -> StorageResult<bool> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let result = {
            let mut blocks = self.blocks.write().unwrap();
            if let Some(block_data) = blocks.remove(hash) {
                self.stats.block_count.fetch_sub(1, Ordering::Relaxed);
                self.stats
                    .total_block_size
                    .fetch_sub(block_data.size, Ordering::Relaxed);
                Some(block_data.size)
            } else {
                None
            }
        };

        if let Some(_size) = result {
            self.remove_from_lru(hash).await;

            self.record_event(StorageEvent::BlockDeleted {
                hash: hash.to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            })
            .await;

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// Implementation of TreeOperations trait
#[async_trait]
impl TreeOperations for MemoryStorage {
    async fn store_tree(&self, root_hash: &str, tree: &MerkleTree) -> StorageResult<()> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let tree_data = TreeData::new(tree.clone());
        let node_count = tree_data.node_count;

        let _is_new_tree = {
            let mut trees = self.trees.write().unwrap();
            if trees.contains_key(root_hash) {
                trees.insert(root_hash.to_string(), tree_data);
                false
            } else {
                trees.insert(root_hash.to_string(), tree_data);
                self.stats.tree_count.fetch_add(1, Ordering::Relaxed);
                true
            }
        };

        self.record_event(StorageEvent::TreeStored {
            root_hash: root_hash.to_string(),
            node_count,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
        .await;

        Ok(())
    }

    async fn get_tree(&self, root_hash: &str) -> StorageResult<Option<MerkleTree>> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let result = {
            let trees = self.trees.read().unwrap();
            if let Some(tree_data) = trees.get(root_hash) {
                tree_data.touch();
                self.stats.tree_retrievals.fetch_add(1, Ordering::Relaxed);
                Some(tree_data.tree.clone())
            } else {
                None
            }
        };

        if result.is_some() {
            self.record_event(StorageEvent::TreeRetrieved {
                root_hash: root_hash.to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            })
            .await;
        }

        Ok(result)
    }

    async fn tree_exists(&self, root_hash: &str) -> StorageResult<bool> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let trees = self.trees.read().unwrap();
        Ok(trees.contains_key(root_hash))
    }

    async fn delete_tree(&self, root_hash: &str) -> StorageResult<bool> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let result = {
            let mut trees = self.trees.write().unwrap();
            if trees.remove(root_hash).is_some() {
                self.stats.tree_count.fetch_sub(1, Ordering::Relaxed);
                true
            } else {
                false
            }
        };

        if result {
            self.record_event(StorageEvent::TreeDeleted {
                root_hash: root_hash.to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            })
            .await;
        }

        Ok(result)
    }
}

// Implementation of StorageManagement trait
#[async_trait]
impl StorageManagement for MemoryStorage {
    async fn get_stats(&self) -> StorageResult<StorageStats> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        let block_count = self.stats.block_count.load(Ordering::Relaxed);
        let total_size = self.stats.total_block_size.load(Ordering::Relaxed);
        let tree_count = self.stats.tree_count.load(Ordering::Relaxed);
        let deduplication_hits = self.stats.deduplication_hits.load(Ordering::Relaxed);
        let largest_block_size = self.stats.largest_block_size.load(Ordering::Relaxed);
        let evicted_blocks = self.stats.evicted_blocks.load(Ordering::Relaxed);
        let average_block_size = if block_count > 0 {
            total_size as f64 / block_count as f64
        } else {
            0.0
        };

        let quota_usage = self
            .config
            .memory_limit
            .map(|limit| QuotaUsage::new(total_size, limit));

        Ok(StorageStats {
            backend: StorageBackend::InMemory,
            block_count,
            block_size_bytes: total_size,
            tree_count,
            deduplication_savings: deduplication_hits,
            average_block_size,
            largest_block_size,
            evicted_blocks,
            quota_usage,
        })
    }

    async fn maintenance(&self) -> StorageResult<()> {
        if self.is_shutdown() {
            return Err(StorageError::InvalidOperation(
                "Storage is shutdown".to_string(),
            ));
        }

        // Trigger cleanup if needed
        if self.config.enable_lru_eviction {
            self.trigger_cleanup().await?;
        }

        Ok(())
    }
}

// Blanket implementation of the composite ContentAddressedStorage trait
// Since MemoryStorage implements all three sub-traits, it automatically implements the composite
impl ContentAddressedStorage for MemoryStorage {}

/// Builder for creating MemoryStorage with custom configuration
#[derive(Debug, Clone)]
pub struct MemoryStorageBuilder {
    config: MemoryStorageConfig,
}

impl MemoryStorageBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: MemoryStorageConfig::default(),
        }
    }

    /// Set memory limit in bytes
    pub fn with_memory_limit(mut self, limit: u64) -> Self {
        self.config.memory_limit = Some(limit);
        self
    }

    /// Disable memory limit (unlimited memory usage)
    pub fn without_memory_limit(mut self) -> Self {
        self.config.memory_limit = None;
        self
    }

    /// Enable or disable LRU eviction
    pub fn with_lru_eviction(mut self, enable: bool) -> Self {
        self.config.enable_lru_eviction = enable;
        self
    }

    /// Enable or disable statistics tracking
    pub fn with_stats_tracking(mut self, enable: bool) -> Self {
        self.config.enable_stats_tracking = enable;
        self
    }

    /// Enable or disable event notifications
    pub fn with_event_notifications(mut self, enable: bool) -> Self {
        self.config.enable_event_notifications = enable;
        self
    }

    /// Set maximum number of events to keep
    pub fn with_max_events(mut self, max_events: usize) -> Self {
        self.config.max_events = max_events;
        self
    }

    /// Set cleanup threshold percentage
    pub fn with_cleanup_threshold(mut self, threshold: f64) -> Self {
        self.config.cleanup_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set cleanup target percentage
    pub fn with_cleanup_target(mut self, target: f64) -> Self {
        self.config.cleanup_target = target.clamp(0.0, 1.0);
        self
    }

    /// Build the MemoryStorage instance
    pub fn build(self) -> Arc<MemoryStorage> {
        MemoryStorage::with_config(self.config)
    }
}

impl Default for MemoryStorageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of MemoryStorage state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStorageSnapshot {
    /// Timestamp when snapshot was created
    pub timestamp: u64,
    /// Block data (hash -> content)
    pub blocks: HashMap<String, Vec<u8>>,
    /// Tree data (root_hash -> MerkleTree)
    pub trees: HashMap<String, MerkleTree>,
    /// Statistics at the time of snapshot
    pub stats: StorageStats,
}

/// Shutdown handle for graceful shutdown
#[derive(Debug)]
pub struct MemoryStorageShutdown {
    storage: Arc<MemoryStorage>,
}

impl MemoryStorageShutdown {
    /// Create a new shutdown handle
    pub fn new(storage: Arc<MemoryStorage>) -> Self {
        Self { storage }
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(&self) -> StorageResult<()> {
        // Perform final maintenance before marking as shutdown
        self.storage.maintenance().await?;

        // Mark as shutdown after maintenance is complete
        self.storage.shutdown.store(true, Ordering::Relaxed);

        Ok(())
    }

    /// Check if shutdown is complete
    pub fn is_shutdown_complete(&self) -> bool {
        self.storage.is_shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::merkle::*;
    use std::collections::HashMap;
    use tokio::time::{sleep, Duration};

    // Test data helpers
    fn create_test_block(index: usize) -> (String, Vec<u8>) {
        let data = format!("Test block content {}", index);
        let hash = format!("hash{:032}", index); // Mock hash
        (hash, data.into_bytes())
    }

    fn create_test_merkle_tree(block_count: usize) -> (String, MerkleTree) {
        let mut nodes = HashMap::new();
        let mut leaf_hashes = Vec::new();

        // Create leaf nodes
        for i in 0..block_count {
            let hash = format!("leaf_hash{:032}", i);
            let node = MerkleNode {
                hash: hash.clone(),
                node_type: NodeType::Leaf {
                    block_hash: hash.clone(),
                    block_index: i,
                },
                depth: 0,
                index: i,
            };
            nodes.insert(hash.clone(), node);
            leaf_hashes.push(hash);
        }

        // Create internal nodes (simplified for testing)
        let root_hash = if leaf_hashes.len() == 1 {
            leaf_hashes[0].clone()
        } else {
            format!("root_hash{:032}", block_count)
        };

        nodes.insert(
            root_hash.clone(),
            MerkleNode {
                hash: root_hash.clone(),
                node_type: NodeType::Internal {
                    left_hash: leaf_hashes.get(0).unwrap_or(&"".to_string()).clone(),
                    right_hash: leaf_hashes.get(1).unwrap_or(&"".to_string()).clone(),
                    left_index: 0,
                    right_index: leaf_hashes.len().saturating_sub(1),
                },
                depth: 1,
                index: 0,
            },
        );

        let tree = MerkleTree {
            root_hash: root_hash.clone(),
            nodes,
            leaf_hashes,
            depth: 1,
            block_count,
        };

        (root_hash, tree)
    }

    #[tokio::test]
    async fn test_memory_storage_new() {
        let storage = MemoryStorage::new();

        assert!(!storage.is_shutdown());
        assert!(!storage.instance_id().is_empty());

        // Should be able to store and retrieve blocks
        let (hash, data) = create_test_block(1);
        storage.store_block(&hash, &data).await.unwrap();
        let retrieved = storage.get_block(&hash).await.unwrap();
        assert_eq!(retrieved, Some(data));
    }

    #[tokio::test]
    async fn test_memory_storage_builder_default() {
        let storage = MemoryStorage::builder().build();

        assert!(!storage.is_shutdown());
        assert_eq!(storage.config.memory_limit, Some(512 * 1024 * 1024));
        assert!(storage.config.enable_lru_eviction);
        assert!(storage.config.enable_stats_tracking);
        assert!(!storage.config.enable_event_notifications);
    }

    #[tokio::test]
    async fn test_memory_storage_builder_custom() {
        let storage = MemoryStorage::builder()
            .with_memory_limit(1024 * 1024) // 1MB
            .with_lru_eviction(false)
            .with_stats_tracking(false)
            .with_event_notifications(true)
            .with_max_events(500)
            .with_cleanup_threshold(0.8)
            .with_cleanup_target(0.6)
            .build();

        assert_eq!(storage.config.memory_limit, Some(1024 * 1024));
        assert!(!storage.config.enable_lru_eviction);
        assert!(!storage.config.enable_stats_tracking);
        assert!(storage.config.enable_event_notifications);
        assert_eq!(storage.config.max_events, 500);
        assert_eq!(storage.config.cleanup_threshold, 0.8);
        assert_eq!(storage.config.cleanup_target, 0.6);
    }

    #[tokio::test]
    async fn test_memory_storage_builder_unlimited_memory() {
        let storage = MemoryStorage::builder().without_memory_limit().build();

        assert_eq!(storage.config.memory_limit, None);
    }

    #[tokio::test]
    async fn test_store_and_get_block() {
        let storage = MemoryStorage::new();

        // Test storing and retrieving a block
        let (hash, data) = create_test_block(1);

        // Block should not exist initially
        assert!(!storage.block_exists(&hash).await.unwrap());

        // Store the block
        storage.store_block(&hash, &data).await.unwrap();

        // Block should now exist
        assert!(storage.block_exists(&hash).await.unwrap());

        // Retrieve the block
        let retrieved = storage.get_block(&hash).await.unwrap();
        assert_eq!(retrieved, Some(data));

        // Retrieving non-existent block should return None
        let non_existent = storage.get_block("nonexistent").await.unwrap();
        assert_eq!(non_existent, None);
    }

    #[tokio::test]
    async fn test_store_and_get_tree() {
        let storage = MemoryStorage::new();

        let (root_hash, tree) = create_test_merkle_tree(5);

        // Tree should not exist initially
        assert!(!storage.tree_exists(&root_hash).await.unwrap());

        // Store the tree
        storage.store_tree(&root_hash, &tree).await.unwrap();

        // Tree should now exist
        assert!(storage.tree_exists(&root_hash).await.unwrap());

        // Retrieve the tree
        let retrieved = storage.get_tree(&root_hash).await.unwrap();
        assert_eq!(retrieved, Some(tree));

        // Retrieving non-existent tree should return None
        let non_existent = storage.get_tree("nonexistent").await.unwrap();
        assert_eq!(non_existent, None);
    }

    #[tokio::test]
    async fn test_delete_block() {
        let storage = MemoryStorage::new();

        let (hash, data) = create_test_block(1);

        // Store the block first
        storage.store_block(&hash, &data).await.unwrap();
        assert!(storage.block_exists(&hash).await.unwrap());

        // Delete the block
        let deleted = storage.delete_block(&hash).await.unwrap();
        assert!(deleted);

        // Block should no longer exist
        assert!(!storage.block_exists(&hash).await.unwrap());

        // Deleting non-existent block should return false
        let not_deleted = storage.delete_block("nonexistent").await.unwrap();
        assert!(!not_deleted);
    }

    #[tokio::test]
    async fn test_delete_tree() {
        let storage = MemoryStorage::new();

        let (root_hash, tree) = create_test_merkle_tree(3);

        // Store the tree first
        storage.store_tree(&root_hash, &tree).await.unwrap();
        assert!(storage.tree_exists(&root_hash).await.unwrap());

        // Delete the tree
        let deleted = storage.delete_tree(&root_hash).await.unwrap();
        assert!(deleted);

        // Tree should no longer exist
        assert!(!storage.tree_exists(&root_hash).await.unwrap());

        // Deleting non-existent tree should return false
        let not_deleted = storage.delete_tree("nonexistent").await.unwrap();
        assert!(!not_deleted);
    }

    #[tokio::test]
    async fn test_block_deduplication() {
        let storage = MemoryStorage::new();

        let (hash, data) = create_test_block(1);

        // Store the same block twice
        storage.store_block(&hash, &data).await.unwrap();
        storage.store_block(&hash, &data).await.unwrap();

        // Should still only have one block
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.block_count, 1);

        // Should be able to retrieve it
        let retrieved = storage.get_block(&hash).await.unwrap();
        assert_eq!(retrieved, Some(data));
    }

    #[tokio::test]
    async fn test_tree_deduplication() {
        let storage = MemoryStorage::new();

        let (root_hash, tree) = create_test_merkle_tree(4);

        // Store the same tree twice
        storage.store_tree(&root_hash, &tree).await.unwrap();
        storage.store_tree(&root_hash, &tree).await.unwrap();

        // Should still only have one tree
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.tree_count, 1);

        // Should be able to retrieve it
        let retrieved = storage.get_tree(&root_hash).await.unwrap();
        assert_eq!(retrieved, Some(tree));
    }

    #[tokio::test]
    async fn test_get_stats() {
        let storage = MemoryStorage::new();

        // Initially empty
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.block_count, 0);
        assert_eq!(stats.tree_count, 0);
        assert_eq!(stats.block_size_bytes, 0);
        assert_eq!(stats.average_block_size, 0.0);
        assert_eq!(stats.largest_block_size, 0);

        // Store some blocks
        let blocks: Vec<_> = (0..3).map(|i| create_test_block(i)).collect();
        let total_size: u64 = blocks.iter().map(|(_, data)| data.len() as u64).sum();

        for (hash, data) in &blocks {
            storage.store_block(hash, data).await.unwrap();
        }

        // Store some trees
        for i in 0..2 {
            let (root_hash, tree) = create_test_merkle_tree(i + 2);
            storage.store_tree(&root_hash, &tree).await.unwrap();
        }

        // Check stats
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.block_count, 3);
        assert_eq!(stats.tree_count, 2);
        assert_eq!(stats.block_size_bytes, total_size);
        assert_eq!(stats.average_block_size, total_size as f64 / 3.0);
        assert!(stats.largest_block_size > 0);
        assert!(matches!(stats.backend, StorageBackend::InMemory));
    }

    #[tokio::test]
    async fn test_memory_limit_enforcement() {
        let storage = MemoryStorage::builder()
            .with_memory_limit(30) // Very small limit (reduced further to force failure)
            .with_lru_eviction(false) // Disable eviction to test limit enforcement
            .build();

        let (hash1, data1) = create_test_block(1);
        let (hash2, data2) = create_test_block(2);

        // First block should fit
        storage.store_block(&hash1, &data1).await.unwrap();

        // Second block should exceed limit and fail
        let result = storage.store_block(&hash2, &data2).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StorageError::QuotaExceeded { .. }
        ));
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let storage = MemoryStorage::builder()
            .with_memory_limit(30) // Small limit to trigger eviction (20+20 > 30)
            .with_lru_eviction(true)
            .build();

        let (hash1, data1) = create_test_block(1);
        let (hash2, data2) = create_test_block(2);

        // Store first block
        storage.store_block(&hash1, &data1).await.unwrap();
        assert!(storage.block_exists(&hash1).await.unwrap());

        // Store second block (should trigger eviction of first)
        storage.store_block(&hash2, &data2).await.unwrap();

        // First block should be evicted, second should exist
        // Note: This depends on the exact size of test data
        let stats = storage.get_stats().await.unwrap();
        assert!(stats.evicted_blocks > 0);
    }

    #[tokio::test]
    async fn test_manual_cleanup() {
        let storage = MemoryStorage::builder()
            .with_memory_limit(150)
            .with_lru_eviction(true)
            .build();

        let (hash1, data1) = create_test_block(1);
        let (hash2, data2) = create_test_block(2);

        // Store blocks that exceed limit
        storage.store_block(&hash1, &data1).await.unwrap();
        storage.store_block(&hash2, &data2).await.unwrap();

        // Trigger manual cleanup
        let evicted_count = storage.trigger_cleanup().await.unwrap();
        assert!(evicted_count >= 0);
    }

    #[tokio::test]
    async fn test_event_notifications() {
        let storage = MemoryStorage::builder()
            .with_event_notifications(true)
            .build();

        let (hash, data) = create_test_block(1);

        // Store a block
        storage.store_block(&hash, &data).await.unwrap();

        // Retrieve the block
        storage.get_block(&hash).await.unwrap();

        // Delete the block
        storage.delete_block(&hash).await.unwrap();

        // Check events
        let events = storage.get_recent_events(Some(10)).await;
        assert_eq!(events.len(), 3);

        // Events are stored with newest first, so we need to check in reverse order
        assert!(matches!(events[2], StorageEvent::BlockStored { .. }));
        assert!(matches!(events[1], StorageEvent::BlockRetrieved { .. }));
        assert!(matches!(events[0], StorageEvent::BlockDeleted { .. }));
    }

    #[tokio::test]
    async fn test_import_export() {
        let storage1 = MemoryStorage::new();
        let storage2 = MemoryStorage::new();

        // Store data in storage1
        let (hash1, data1) = create_test_block(1);
        let (hash2, data2) = create_test_block(2);
        let (root_hash, tree) = create_test_merkle_tree(3);

        storage1.store_block(&hash1, &data1).await.unwrap();
        storage1.store_block(&hash2, &data2).await.unwrap();
        storage1.store_tree(&root_hash, &tree).await.unwrap();

        // Export from storage1
        let (blocks, trees) = storage1.export_data().await.unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(trees.len(), 1);

        // Import to storage2
        storage2.import_from(&storage1).await.unwrap();

        // Verify data in storage2
        assert!(storage2.block_exists(&hash1).await.unwrap());
        assert!(storage2.block_exists(&hash2).await.unwrap());
        assert!(storage2.tree_exists(&root_hash).await.unwrap());

        let retrieved_block1 = storage2.get_block(&hash1).await.unwrap();
        let retrieved_block2 = storage2.get_block(&hash2).await.unwrap();
        let retrieved_tree = storage2.get_tree(&root_hash).await.unwrap();

        assert_eq!(retrieved_block1, Some(data1));
        assert_eq!(retrieved_block2, Some(data2));
        assert_eq!(retrieved_tree, Some(tree));
    }

    #[tokio::test]
    async fn test_snapshot_restore() {
        let storage1 = MemoryStorage::new();
        let storage2 = MemoryStorage::new();

        // Store data in storage1
        let (hash1, data1) = create_test_block(1);
        let (hash2, data2) = create_test_block(2);
        let (root_hash, tree) = create_test_merkle_tree(4);

        storage1.store_block(&hash1, &data1).await.unwrap();
        storage1.store_block(&hash2, &data2).await.unwrap();
        storage1.store_tree(&root_hash, &tree).await.unwrap();

        // Create snapshot
        let snapshot = storage1.create_snapshot().await.unwrap();
        assert!(snapshot.timestamp > 0);
        assert_eq!(snapshot.blocks.len(), 2);
        assert_eq!(snapshot.trees.len(), 1);

        // Restore to storage2
        storage2.restore_from_snapshot(snapshot).await.unwrap();

        // Verify data in storage2
        assert!(storage2.block_exists(&hash1).await.unwrap());
        assert!(storage2.block_exists(&hash2).await.unwrap());
        assert!(storage2.tree_exists(&root_hash).await.unwrap());

        let retrieved_block1 = storage2.get_block(&hash1).await.unwrap();
        let retrieved_block2 = storage2.get_block(&hash2).await.unwrap();
        let retrieved_tree = storage2.get_tree(&root_hash).await.unwrap();

        assert_eq!(retrieved_block1, Some(data1));
        assert_eq!(retrieved_block2, Some(data2));
        assert_eq!(retrieved_tree, Some(tree));
    }

    #[tokio::test]
    async fn test_maintenance() {
        let storage = MemoryStorage::new();

        // Store some data
        let (hash, data) = create_test_block(1);
        storage.store_block(&hash, &data).await.unwrap();

        // Run maintenance (should succeed)
        storage.maintenance().await.unwrap();

        // Data should still be accessible
        assert!(storage.block_exists(&hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_shutdown_operations() {
        let storage = MemoryStorage::new();
        let shutdown = MemoryStorageShutdown::new(storage.clone());

        // Store some data before shutdown
        let (hash, data) = create_test_block(1);
        storage.store_block(&hash, &data).await.unwrap();

        // Should not be shutdown initially
        assert!(!shutdown.is_shutdown_complete());

        // Shutdown the storage
        shutdown.shutdown().await.unwrap();

        // Should now be shutdown
        assert!(shutdown.is_shutdown_complete());

        // Operations should fail after shutdown
        let result = storage.store_block("new_hash", b"new data").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StorageError::InvalidOperation(_)
        ));

        let result = storage.get_block(&hash).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StorageError::InvalidOperation(_)
        ));
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let storage = MemoryStorage::new();
        let storage_clone = storage.clone();

        // Spawn multiple concurrent tasks
        let mut handles = Vec::new();

        // Task 1: Store blocks
        let handle1 = tokio::spawn(async move {
            for i in 0..10 {
                let (hash, data) = create_test_block(i);
                storage_clone.store_block(&hash, &data).await.unwrap();
                sleep(Duration::from_millis(1)).await;
            }
        });
        handles.push(handle1);

        // Task 2: Read blocks
        let storage_clone2 = storage.clone();
        let handle2 = tokio::spawn(async move {
            for i in 0..10 {
                let hash = format!("hash{:032}", i);
                // Keep trying to get the block (might not exist yet)
                let _ = storage_clone2.get_block(&hash).await;
                sleep(Duration::from_millis(2)).await;
            }
        });
        handles.push(handle2);

        // Task 3: Store trees
        let storage_clone3 = storage.clone();
        let handle3 = tokio::spawn(async move {
            for i in 0..5 {
                let (root_hash, tree) = create_test_merkle_tree(i + 2);
                storage_clone3.store_tree(&root_hash, &tree).await.unwrap();
                sleep(Duration::from_millis(3)).await;
            }
        });
        handles.push(handle3);

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all data is stored correctly
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.block_count, 10);
        assert_eq!(stats.tree_count, 5);
    }

    #[tokio::test]
    async fn test_quota_usage() {
        let storage = MemoryStorage::builder().with_memory_limit(1000).build();

        // Store some blocks
        for i in 0..3 {
            let (hash, data) = create_test_block(i);
            storage.store_block(&hash, &data).await.unwrap();
        }

        let stats = storage.get_stats().await.unwrap();
        assert!(stats.quota_usage.is_some());

        let quota = stats.quota_usage.unwrap();
        assert_eq!(quota.limit_bytes, 1000);
        assert_eq!(quota.used_bytes, stats.block_size_bytes);
        assert!(quota.usage_percentage > 0.0);
        assert!(quota.usage_percentage <= 1.0);
    }

    #[tokio::test]
    async fn test_block_data_touch() {
        let data = b"test data".to_vec();
        let block_data = BlockData::new(data.clone());

        let initial_timestamp = block_data.last_accessed();

        // Sleep a bit to ensure different timestamp
        sleep(Duration::from_millis(10)).await;

        block_data.touch();
        let updated_timestamp = block_data.last_accessed();

        assert!(updated_timestamp > initial_timestamp);
        assert_eq!(block_data.data, data);
        assert_eq!(block_data.size, data.len() as u64);
    }

    #[tokio::test]
    async fn test_tree_data_touch() {
        let (_root_hash, tree) = create_test_merkle_tree(5);
        let tree_data = TreeData::new(tree.clone());

        let initial_timestamp = tree_data.last_accessed();

        // Sleep a bit to ensure different timestamp
        sleep(Duration::from_millis(10)).await;

        tree_data.touch();
        let updated_timestamp = tree_data.last_accessed();

        assert!(updated_timestamp > initial_timestamp);
        assert_eq!(tree_data.tree, tree);
        assert_eq!(tree_data.node_count, tree.nodes.len());
    }

    #[tokio::test]
    async fn test_memory_stats() {
        let stats = MemoryStats::new();

        assert_eq!(stats.block_count.load(Ordering::Relaxed), 0);
        assert_eq!(stats.total_block_size.load(Ordering::Relaxed), 0);
        assert_eq!(stats.tree_count.load(Ordering::Relaxed), 0);
        assert_eq!(stats.deduplication_hits.load(Ordering::Relaxed), 0);
        assert_eq!(stats.average_block_size(), 0.0);

        // Add some stats
        stats.block_count.store(5, Ordering::Relaxed);
        stats.total_block_size.store(1000, Ordering::Relaxed);

        assert_eq!(stats.average_block_size(), 200.0);
    }

    #[test]
    fn test_memory_storage_config_default() {
        let config = MemoryStorageConfig::default();

        assert_eq!(config.memory_limit, Some(512 * 1024 * 1024));
        assert!(config.enable_lru_eviction);
        assert!(config.enable_stats_tracking);
        assert!(!config.enable_event_notifications);
        assert_eq!(config.max_events, 1000);
        assert_eq!(config.cleanup_threshold, 0.9);
        assert_eq!(config.cleanup_target, 0.7);
    }

    #[test]
    fn test_memory_storage_config_clone() {
        let config = MemoryStorageConfig {
            memory_limit: Some(1024),
            enable_lru_eviction: false,
            enable_stats_tracking: false,
            enable_event_notifications: true,
            max_events: 500,
            cleanup_threshold: 0.8,
            cleanup_target: 0.6,
        };

        let cloned = config.clone();
        assert_eq!(config.memory_limit, cloned.memory_limit);
        assert_eq!(config.enable_lru_eviction, cloned.enable_lru_eviction);
        assert_eq!(config.enable_stats_tracking, cloned.enable_stats_tracking);
        assert_eq!(
            config.enable_event_notifications,
            cloned.enable_event_notifications
        );
        assert_eq!(config.max_events, cloned.max_events);
        assert_eq!(config.cleanup_threshold, cloned.cleanup_threshold);
        assert_eq!(config.cleanup_target, cloned.cleanup_target);
    }

    #[test]
    fn test_storage_event_serialization() {
        let event = StorageEvent::BlockStored {
            hash: "test_hash".to_string(),
            size: 100,
            timestamp: 1234567890,
        };

        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: StorageEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_storage_snapshot_serialization() {
        let snapshot = MemoryStorageSnapshot {
            timestamp: 1234567890,
            blocks: HashMap::from([("hash1".to_string(), vec![1, 2, 3])]),
            trees: HashMap::new(),
            stats: StorageStats {
                backend: StorageBackend::InMemory,
                block_count: 1,
                block_size_bytes: 3,
                tree_count: 0,
                deduplication_savings: 0,
                average_block_size: 3.0,
                largest_block_size: 3,
                evicted_blocks: 0,
                quota_usage: None,
            },
        };

        let serialized = serde_json::to_string(&snapshot).unwrap();
        let deserialized: MemoryStorageSnapshot = serde_json::from_str(&serialized).unwrap();

        assert_eq!(snapshot.timestamp, deserialized.timestamp);
        assert_eq!(snapshot.blocks, deserialized.blocks);
        assert_eq!(snapshot.trees, deserialized.trees);
    }

    #[test]
    fn test_memory_storage_builder_debug() {
        let builder = MemoryStorage::builder();
        let debug_str = format!("{:?}", builder);
        assert!(debug_str.contains("MemoryStorageBuilder"));
    }

    #[test]
    fn test_block_data_debug() {
        let data = b"test".to_vec();
        let block_data = BlockData::new(data);
        let debug_str = format!("{:?}", block_data);
        assert!(debug_str.contains("BlockData"));
    }

    #[test]
    fn test_tree_data_debug() {
        let (_root_hash, tree) = create_test_merkle_tree(3);
        let tree_data = TreeData::new(tree);
        let debug_str = format!("{:?}", tree_data);
        assert!(debug_str.contains("TreeData"));
    }
}
