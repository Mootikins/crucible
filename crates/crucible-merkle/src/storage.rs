//! Storage abstraction for Merkle trees
//!
//! This module provides traits for persisting and retrieving Merkle trees
//! with different storage backends (SurrealDB, filesystem, in-memory, etc.).
//!
//! # Design Principles
//!
//! - **Backend Agnostic**: Core Merkle logic doesn't depend on specific storage
//! - **Async First**: All operations are async for database/filesystem I/O
//! - **Incremental Updates**: Support efficient partial tree updates
//! - **Testability**: In-memory implementation for testing without dependencies
//!
//! # Example
//!
//! ```rust
//! use crucible_merkle::{MerkleStore, InMemoryMerkleStore, HybridMerkleTree};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Use in-memory store for testing
//! let store = InMemoryMerkleStore::new();
//!
//! // Store a tree
//! let tree = HybridMerkleTree::default();
//! store.store("my-doc", &tree).await?;
//!
//! // Retrieve it
//! let retrieved = store.retrieve("my-doc").await?;
//! assert_eq!(tree.root_hash, retrieved.root_hash);
//! # Ok(())
//! # }
//! ```

use crate::HybridMerkleTree;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Error types for storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    /// Tree not found in storage
    #[error("Tree not found: {0}")]
    NotFound(String),

    /// Serialization or deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Storage backend error (database, filesystem, etc.)
    #[error("Storage error: {0}")]
    Storage(String),

    /// Invalid operation (e.g., out of bounds index)
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Version mismatch during deserialization
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },
}

/// Convenient Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Metadata about a stored Merkle tree
///
/// This provides quick access to tree information without loading
/// the full tree structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeMetadata {
    /// Unique identifier for this tree (typically document path)
    pub id: String,

    /// Root hash as hex string
    pub root_hash: String,

    /// Total number of sections in the tree
    pub section_count: usize,

    /// Total number of blocks across all sections
    pub total_blocks: usize,

    /// Whether the tree uses virtualization for large documents
    pub is_virtualized: bool,

    /// Number of virtual sections (if virtualized)
    pub virtual_section_count: usize,

    /// Creation timestamp (RFC3339 format)
    pub created_at: String,

    /// Last update timestamp (RFC3339 format)
    pub updated_at: String,

    /// Optional user-defined metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Trait for Merkle tree persistence
///
/// Implementations provide storage for `HybridMerkleTree` instances,
/// supporting full CRUD operations plus incremental updates.
///
/// # Thread Safety
///
/// All methods take `&self`, so implementations must be internally synchronized.
/// Use `Arc<dyn MerkleStore>` for shared ownership across threads.
///
/// # Error Handling
///
/// - `NotFound`: Tree doesn't exist (retrieve, update, delete operations)
/// - `InvalidOperation`: Invalid input (e.g., section index out of bounds)
/// - `Storage`: Backend-specific errors (database connection, filesystem I/O)
/// - `Serialization`: Data encoding/decoding errors
#[async_trait]
pub trait MerkleStore: Send + Sync {
    /// Store a complete Merkle tree
    ///
    /// If a tree with this ID already exists, it will be replaced.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier (typically document path like "docs/readme.md")
    /// * `tree` - The tree to store
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Storage` if the backend operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crucible_merkle::{MerkleStore, InMemoryMerkleStore, HybridMerkleTree};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let store = InMemoryMerkleStore::new();
    /// let tree = HybridMerkleTree::default();
    /// store.store("my-doc", &tree).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn store(&self, id: &str, tree: &HybridMerkleTree) -> StorageResult<()>;

    /// Retrieve a complete Merkle tree
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    ///
    /// # Errors
    ///
    /// - `NotFound` if tree doesn't exist
    /// - `Storage` for backend errors
    /// - `Serialization` for data corruption
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crucible_merkle::{MerkleStore, InMemoryMerkleStore, HybridMerkleTree};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let store = InMemoryMerkleStore::new();
    /// # store.store("my-doc", &HybridMerkleTree::default()).await?;
    /// let tree = store.retrieve("my-doc").await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn retrieve(&self, id: &str) -> StorageResult<HybridMerkleTree>;

    /// Delete a tree and all associated data
    ///
    /// This is idempotent - deleting a non-existent tree is not an error.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Storage` if the backend operation fails.
    /// Does NOT error if the tree doesn't exist.
    async fn delete(&self, id: &str) -> StorageResult<()>;

    /// Get tree metadata without loading the full tree
    ///
    /// This is more efficient than `retrieve()` when you only need
    /// basic information like root hash or section count.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    ///
    /// # Returns
    ///
    /// - `Ok(Some(metadata))` if tree exists
    /// - `Ok(None)` if tree doesn't exist
    /// - `Err(...)` for backend errors
    async fn get_metadata(&self, id: &str) -> StorageResult<Option<TreeMetadata>>;

    /// Update tree incrementally
    ///
    /// Only updates the specified sections, which is more efficient than
    /// storing the entire tree. This is useful when processing document
    /// changes detected via `tree.diff()`.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    /// * `tree` - The updated tree (with new root hash)
    /// * `changed_sections` - Indices of sections that changed
    ///
    /// # Errors
    ///
    /// - `InvalidOperation` if any section index >= tree.sections.len()
    /// - `NotFound` if tree doesn't exist
    /// - `Storage` for backend errors
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crucible_merkle::{MerkleStore, InMemoryMerkleStore, HybridMerkleTree};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let store = InMemoryMerkleStore::new();
    /// # let old_tree = HybridMerkleTree::default();
    /// # store.store("my-doc", &old_tree).await?;
    /// # let new_tree = HybridMerkleTree::default();
    /// // Detect changes
    /// let diff = new_tree.diff(&old_tree);
    /// let changed_indices: Vec<usize> = diff.changed_sections
    ///     .iter()
    ///     .map(|c| c.section_index)
    ///     .collect();
    ///
    /// // Update only changed sections
    /// store.update_incremental("my-doc", &new_tree, &changed_indices).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn update_incremental(
        &self,
        id: &str,
        tree: &HybridMerkleTree,
        changed_sections: &[usize],
    ) -> StorageResult<()>;

    /// List all stored trees
    ///
    /// Returns metadata for all trees, ordered by update time (newest first).
    /// Useful for discovery, debugging, and maintenance operations.
    ///
    /// # Returns
    ///
    /// Vector of tree metadata, sorted by `updated_at` descending.
    ///
    /// # Performance
    ///
    /// This may be expensive for large databases. Consider pagination
    /// for production use cases with thousands of trees.
    async fn list_trees(&self) -> StorageResult<Vec<TreeMetadata>>;
}

/// In-memory implementation of MerkleStore
///
/// Useful for testing and scenarios where persistence isn't needed.
/// All data is lost when the instance is dropped.
///
/// # Thread Safety
///
/// Uses `Arc<RwLock<...>>` internally, so clones share the same storage.
///
/// # Example
///
/// ```rust
/// use crucible_merkle::{MerkleStore, InMemoryMerkleStore, HybridMerkleTree};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let store = InMemoryMerkleStore::new();
///
/// // Store and retrieve
/// let tree = HybridMerkleTree::default();
/// store.store("test-id", &tree).await?;
/// let retrieved = store.retrieve("test-id").await?;
///
/// // List all trees
/// let all_trees = store.list_trees().await?;
/// assert_eq!(all_trees.len(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct InMemoryMerkleStore {
    trees: Arc<RwLock<HashMap<String, (HybridMerkleTree, TreeMetadata)>>>,
}

impl InMemoryMerkleStore {
    /// Create a new empty in-memory store
    pub fn new() -> Self {
        Self {
            trees: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the number of stored trees
    ///
    /// Useful for testing and debugging.
    pub async fn len(&self) -> usize {
        self.trees.read().await.len()
    }

    /// Check if the store is empty
    pub async fn is_empty(&self) -> bool {
        self.trees.read().await.is_empty()
    }

    /// Create metadata from a tree
    fn create_metadata(id: &str, tree: &HybridMerkleTree, now: String) -> TreeMetadata {
        TreeMetadata {
            id: id.to_string(),
            root_hash: tree.root_hash.to_hex(),
            section_count: tree.sections.len(),
            total_blocks: tree.total_blocks,
            is_virtualized: tree.is_virtualized,
            virtual_section_count: tree.virtual_sections.as_ref().map_or(0, |v| v.len()),
            created_at: now.clone(),
            updated_at: now,
            metadata: None,
        }
    }
}

impl Default for InMemoryMerkleStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MerkleStore for InMemoryMerkleStore {
    async fn store(&self, id: &str, tree: &HybridMerkleTree) -> StorageResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut trees = self.trees.write().await;

        // Preserve created_at if tree already exists
        let created_at = trees
            .get(id)
            .map(|(_, meta)| meta.created_at.clone())
            .unwrap_or_else(|| now.clone());

        let mut metadata = Self::create_metadata(id, tree, now);
        metadata.created_at = created_at;

        trees.insert(id.to_string(), (tree.clone(), metadata));
        Ok(())
    }

    async fn retrieve(&self, id: &str) -> StorageResult<HybridMerkleTree> {
        self.trees
            .read()
            .await
            .get(id)
            .map(|(tree, _)| tree.clone())
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    async fn delete(&self, id: &str) -> StorageResult<()> {
        self.trees.write().await.remove(id);
        Ok(())
    }

    async fn get_metadata(&self, id: &str) -> StorageResult<Option<TreeMetadata>> {
        Ok(self
            .trees
            .read()
            .await
            .get(id)
            .map(|(_, meta)| meta.clone()))
    }

    async fn update_incremental(
        &self,
        id: &str,
        tree: &HybridMerkleTree,
        changed_sections: &[usize],
    ) -> StorageResult<()> {
        // Validate section indices
        for &index in changed_sections {
            if index >= tree.sections.len() {
                return Err(StorageError::InvalidOperation(format!(
                    "Invalid section index {}: tree has {} sections",
                    index,
                    tree.sections.len()
                )));
            }
        }

        // For in-memory implementation, just replace the entire tree
        // (A real database would update only the specified sections)
        self.store(id, tree).await
    }

    async fn list_trees(&self) -> StorageResult<Vec<TreeMetadata>> {
        let trees = self.trees.read().await;
        let mut metadata: Vec<TreeMetadata> =
            trees.values().map(|(_, meta)| meta.clone()).collect();

        // Sort by updated_at descending (newest first)
        metadata.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HybridMerkleTree;
    use crucible_core::parser::ParsedNote;

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let store = InMemoryMerkleStore::new();
        let tree = HybridMerkleTree::default();

        // Store
        store.store("test-id", &tree).await.unwrap();

        // Retrieve
        let retrieved = store.retrieve("test-id").await.unwrap();
        assert_eq!(tree.root_hash, retrieved.root_hash);
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent() {
        let store = InMemoryMerkleStore::new();

        let result = store.retrieve("nonexistent").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryMerkleStore::new();
        let tree = HybridMerkleTree::default();

        // Store and delete
        store.store("test-id", &tree).await.unwrap();
        store.delete("test-id").await.unwrap();

        // Should not exist
        let result = store.retrieve("test-id").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));

        // Delete again should not error (idempotent)
        store.delete("test-id").await.unwrap();
    }

    #[tokio::test]
    async fn test_get_metadata() {
        let store = InMemoryMerkleStore::new();
        let tree = HybridMerkleTree::default();

        // Before storing
        let metadata = store.get_metadata("test-id").await.unwrap();
        assert!(metadata.is_none());

        // After storing
        store.store("test-id", &tree).await.unwrap();
        let metadata = store.get_metadata("test-id").await.unwrap();
        assert!(metadata.is_some());

        let meta = metadata.unwrap();
        assert_eq!(meta.id, "test-id");
        assert_eq!(meta.root_hash, tree.root_hash.to_hex());
        assert_eq!(meta.section_count, tree.sections.len());
        assert_eq!(meta.total_blocks, tree.total_blocks);
    }

    #[tokio::test]
    async fn test_update_incremental_validation() {
        let store = InMemoryMerkleStore::new();
        let doc = ParsedNote::default();
        let tree = HybridMerkleTree::from_document(&doc);

        store.store("test-id", &tree).await.unwrap();

        // Invalid section index
        let result = store.update_incremental("test-id", &tree, &[999]).await;

        assert!(matches!(result, Err(StorageError::InvalidOperation(_))));
    }

    #[tokio::test]
    async fn test_list_trees() {
        let store = InMemoryMerkleStore::new();

        // Empty at first
        let trees = store.list_trees().await.unwrap();
        assert_eq!(trees.len(), 0);

        // Add multiple trees
        for i in 0..3 {
            let tree = HybridMerkleTree::default();
            store.store(&format!("tree-{}", i), &tree).await.unwrap();
        }

        let trees = store.list_trees().await.unwrap();
        assert_eq!(trees.len(), 3);
    }

    #[tokio::test]
    async fn test_store_preserves_created_at() {
        let store = InMemoryMerkleStore::new();
        let tree = HybridMerkleTree::default();

        // First store
        store.store("test-id", &tree).await.unwrap();
        let meta1 = store.get_metadata("test-id").await.unwrap().unwrap();

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Update
        store.store("test-id", &tree).await.unwrap();
        let meta2 = store.get_metadata("test-id").await.unwrap().unwrap();

        // created_at should be the same, updated_at should be different
        assert_eq!(meta1.created_at, meta2.created_at);
        assert_ne!(meta1.updated_at, meta2.updated_at);
    }

    #[tokio::test]
    async fn test_clone_shares_storage() {
        let store1 = InMemoryMerkleStore::new();
        let store2 = store1.clone();

        let tree = HybridMerkleTree::default();
        store1.store("test-id", &tree).await.unwrap();

        // Should be visible in clone
        let retrieved = store2.retrieve("test-id").await.unwrap();
        assert_eq!(tree.root_hash, retrieved.root_hash);
    }
}
