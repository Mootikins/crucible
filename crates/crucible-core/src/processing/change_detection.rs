//! Change Detection Storage
//!
//! This module provides traits and types for tracking file changes to enable
//! efficient incremental processing. Used by the enrichment pipeline's Phase 1
//! (quick filter) to skip unchanged files.
//!
//! # Design Principles
//!
//! - **Backend Agnostic**: Core logic doesn't depend on specific storage
//! - **Fast Lookups**: Optimized for quick "has this file changed?" checks
//! - **SOLID**: Traits in core, implementations in infrastructure layer
//!
//! # Example
//!
//! ```rust
//! use crucible_core::processing::{ChangeDetectionStore, FileState, InMemoryChangeDetectionStore};
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let store = InMemoryChangeDetectionStore::new();
//! let path = Path::new("test.md");
//!
//! // Check if file changed
//! let previous_state = store.get_file_state(path).await?;
//!
//! // ... process file ...
//!
//! // Update state after processing
//! let new_state = FileState {
//!     file_hash: "abc123".to_string(),
//!     modified_time: std::time::SystemTime::now(),
//!     file_size: 1024,
//! };
//! store.store_file_state(path, new_state).await?;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use thiserror::Error;
use tokio::sync::RwLock;

/// Error types for change detection storage operations
#[derive(Debug, Error)]
pub enum ChangeDetectionError {
    /// Storage backend error (database, filesystem, etc.)
    #[error("Storage error: {0}")]
    Storage(String),

    /// Path-related error
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Serialization or deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Convenient Result type for change detection operations
pub type ChangeDetectionResult<T> = Result<T, ChangeDetectionError>;

/// File state snapshot for change detection
///
/// Contains information needed to detect if a file has changed since
/// the last time it was processed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    /// BLAKE3 hash of the file contents
    pub file_hash: String,

    /// Last modified timestamp
    #[serde(with = "systemtime_serde")]
    pub modified_time: SystemTime,

    /// File size in bytes
    pub file_size: u64,
}

impl FileState {
    /// Create a new file state
    pub fn new(file_hash: String, modified_time: SystemTime, file_size: u64) -> Self {
        Self {
            file_hash,
            modified_time,
            file_size,
        }
    }

    /// Check if this state differs from another (indicating file changed)
    pub fn differs_from(&self, other: &FileState) -> bool {
        self.file_hash != other.file_hash
            || self.modified_time != other.modified_time
            || self.file_size != other.file_size
    }
}

/// Trait for file state persistence
///
/// Implementations provide storage for file state snapshots, enabling
/// the enrichment pipeline to skip processing unchanged files.
///
/// # Thread Safety
///
/// All methods take `&self`, so implementations must be internally synchronized.
/// Use `Arc<dyn ChangeDetectionStore>` for shared ownership across threads.
#[async_trait]
pub trait ChangeDetectionStore: Send + Sync {
    /// Get the last known state of a file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file (can be relative or absolute)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(state))` if state exists
    /// - `Ok(None)` if file has never been processed
    /// - `Err(...)` for storage backend errors
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crucible_core::processing::{ChangeDetectionStore, InMemoryChangeDetectionStore};
    /// # use std::path::Path;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let store = InMemoryChangeDetectionStore::new();
    /// let path = Path::new("notes/readme.md");
    /// let state = store.get_file_state(path).await?;
    ///
    /// if state.is_none() {
    ///     println!("File has never been processed");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn get_file_state(&self, path: &Path) -> ChangeDetectionResult<Option<FileState>>;

    /// Store the current state of a file
    ///
    /// This should be called after successfully processing a file to
    /// record its state for future change detection.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file (can be relative or absolute)
    /// * `state` - The file's current state
    ///
    /// # Errors
    ///
    /// Returns `ChangeDetectionError::Storage` if the backend operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crucible_core::processing::{ChangeDetectionStore, FileState, InMemoryChangeDetectionStore};
    /// # use std::path::Path;
    /// # use std::time::SystemTime;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let store = InMemoryChangeDetectionStore::new();
    /// let path = Path::new("notes/readme.md");
    /// let state = FileState::new(
    ///     "abc123".to_string(),
    ///     SystemTime::now(),
    ///     1024
    /// );
    ///
    /// store.store_file_state(path, state).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn store_file_state(&self, path: &Path, state: FileState) -> ChangeDetectionResult<()>;

    /// Delete stored state for a file
    ///
    /// This is idempotent - deleting state for a file that has no state is not an error.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file
    ///
    /// # Errors
    ///
    /// Returns `ChangeDetectionError::Storage` if the backend operation fails.
    /// Does NOT error if the file has no stored state.
    async fn delete_file_state(&self, path: &Path) -> ChangeDetectionResult<()>;

    /// List all files with stored state
    ///
    /// Returns paths of all files that have been processed and have stored state.
    /// Useful for discovery, debugging, and maintenance operations.
    ///
    /// # Returns
    ///
    /// Vector of file paths.
    ///
    /// # Performance
    ///
    /// This may be expensive for large databases. Consider pagination
    /// for production use cases with thousands of files.
    async fn list_tracked_files(&self) -> ChangeDetectionResult<Vec<PathBuf>>;
}

/// In-memory implementation of ChangeDetectionStore
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
/// use crucible_core::processing::{ChangeDetectionStore, FileState, InMemoryChangeDetectionStore};
/// use std::path::Path;
/// use std::time::SystemTime;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let store = InMemoryChangeDetectionStore::new();
/// let path = Path::new("test.md");
///
/// let state = FileState::new("hash123".to_string(), SystemTime::now(), 1024);
/// store.store_file_state(path, state.clone()).await?;
///
/// let retrieved = store.get_file_state(path).await?;
/// assert_eq!(retrieved, Some(state));
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct InMemoryChangeDetectionStore {
    states: Arc<RwLock<HashMap<PathBuf, FileState>>>,
}

impl InMemoryChangeDetectionStore {
    /// Create a new empty in-memory store
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the number of stored file states
    ///
    /// Useful for testing and debugging.
    pub async fn len(&self) -> usize {
        self.states.read().await.len()
    }

    /// Check if the store is empty
    pub async fn is_empty(&self) -> bool {
        self.states.read().await.is_empty()
    }
}

impl Default for InMemoryChangeDetectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChangeDetectionStore for InMemoryChangeDetectionStore {
    async fn get_file_state(&self, path: &Path) -> ChangeDetectionResult<Option<FileState>> {
        Ok(self.states.read().await.get(path).cloned())
    }

    async fn store_file_state(&self, path: &Path, state: FileState) -> ChangeDetectionResult<()> {
        self.states
            .write()
            .await
            .insert(path.to_path_buf(), state);
        Ok(())
    }

    async fn delete_file_state(&self, path: &Path) -> ChangeDetectionResult<()> {
        self.states.write().await.remove(path);
        Ok(())
    }

    async fn list_tracked_files(&self) -> ChangeDetectionResult<Vec<PathBuf>> {
        Ok(self.states.read().await.keys().cloned().collect())
    }
}

/// Serde serialization for SystemTime
mod systemtime_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time
            .duration_since(UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_retrieve_file_state() {
        let store = InMemoryChangeDetectionStore::new();
        let path = Path::new("test.md");

        let state = FileState::new("hash123".to_string(), SystemTime::now(), 1024);

        // Store
        store.store_file_state(path, state.clone()).await.unwrap();

        // Retrieve
        let retrieved = store.get_file_state(path).await.unwrap();
        assert_eq!(retrieved, Some(state));
    }

    #[tokio::test]
    async fn test_get_nonexistent_file_state() {
        let store = InMemoryChangeDetectionStore::new();
        let path = Path::new("nonexistent.md");

        let state = store.get_file_state(path).await.unwrap();
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_delete_file_state() {
        let store = InMemoryChangeDetectionStore::new();
        let path = Path::new("test.md");

        let state = FileState::new("hash123".to_string(), SystemTime::now(), 1024);

        // Store and delete
        store.store_file_state(path, state).await.unwrap();
        store.delete_file_state(path).await.unwrap();

        // Should not exist
        let retrieved = store.get_file_state(path).await.unwrap();
        assert!(retrieved.is_none());

        // Delete again should not error (idempotent)
        store.delete_file_state(path).await.unwrap();
    }

    #[tokio::test]
    async fn test_list_tracked_files() {
        let store = InMemoryChangeDetectionStore::new();

        // Empty at first
        let files = store.list_tracked_files().await.unwrap();
        assert_eq!(files.len(), 0);

        // Add multiple files
        for i in 0..3 {
            let path = PathBuf::from(format!("file-{}.md", i));
            let state = FileState::new(format!("hash{}", i), SystemTime::now(), 1024);
            store.store_file_state(&path, state).await.unwrap();
        }

        let files = store.list_tracked_files().await.unwrap();
        assert_eq!(files.len(), 3);
    }

    #[tokio::test]
    async fn test_file_state_differs_from() {
        let state1 = FileState::new("hash1".to_string(), SystemTime::now(), 1024);
        let state2 = FileState::new("hash2".to_string(), SystemTime::now(), 1024);
        let state3 = state1.clone();

        assert!(state1.differs_from(&state2));
        assert!(!state1.differs_from(&state3));
    }

    #[tokio::test]
    async fn test_clone_shares_storage() {
        let store1 = InMemoryChangeDetectionStore::new();
        let store2 = store1.clone();

        let path = Path::new("test.md");
        let state = FileState::new("hash123".to_string(), SystemTime::now(), 1024);

        store1.store_file_state(path, state.clone()).await.unwrap();

        // Should be visible in clone
        let retrieved = store2.get_file_state(path).await.unwrap();
        assert_eq!(retrieved, Some(state));
    }

    #[tokio::test]
    async fn test_serialization() {
        let state = FileState::new("hash123".to_string(), SystemTime::now(), 1024);

        // Serialize
        let json = serde_json::to_string(&state).unwrap();

        // Deserialize
        let deserialized: FileState = serde_json::from_str(&json).unwrap();

        assert_eq!(state.file_hash, deserialized.file_hash);
        assert_eq!(state.file_size, deserialized.file_size);
        // SystemTime comparison may have minor differences, so check it's close
        let duration = state
            .modified_time
            .duration_since(deserialized.modified_time)
            .unwrap_or_else(|_| {
                deserialized
                    .modified_time
                    .duration_since(state.modified_time)
                    .unwrap()
            });
        assert!(duration.as_secs() < 1);
    }
}
