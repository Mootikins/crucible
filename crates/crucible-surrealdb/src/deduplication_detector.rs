//! Duplicate Block Detection for Storage Backends
//!
//! This module implements advanced deduplication detection and analysis
//! that works with any storage backend implementing the DeduplicationStorage trait.
//!
//! # Generic Design
//!
//! The `DeduplicationDetector` is generic over the storage backend, following
//! the Open-Closed Principle (OCP) from SOLID principles. This allows it to work
//! with different storage implementations without modification.
//!
//! # Examples
//!
//! ```rust,no_run
//! use crucible_surrealdb::{ContentAddressedStorageSurrealDB, SurrealDbConfig};
//! use crucible_surrealdb::deduplication_detector::DeduplicationDetector;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = SurrealDbConfig::memory();
//!     let storage = ContentAddressedStorageSurrealDB::new(config).await?;
//!
//!     // Create detector with generic storage backend
//!     let detector = DeduplicationDetector::new(storage);
//!
//!     // Use detector...
//!     Ok(())
//! }
//! ```

use crate::content_addressed_storage::ContentAddressedStorageSurrealDB;
use async_trait::async_trait;
use chrono::Utc;
use crucible_core::storage::deduplication_traits::DeduplicationStats;
use crucible_core::storage::{
    BlockInfo, DeduplicationStorage, DuplicateBlockInfo, StorageResult, StorageUsageStats,
};
use std::collections::HashMap;

/// Generic deduplication detector that works with any storage backend
///
/// This struct implements advanced deduplication detection and analysis
/// for storage backends that implement the `DeduplicationStorage` trait.
///
/// # Generic Parameters
///
/// * `S` - The storage backend type (implements `DeduplicationStorage`)
///
/// # Thread Safety
///
/// The detector is `Send + Sync` if the underlying storage is `Send + Sync`,
/// allowing it to be safely shared across threads.
#[derive(Debug, Clone)]
pub struct DeduplicationDetector<S: DeduplicationStorage> {
    storage: S,
    /// Average block size for estimations when content is not available
    average_block_size: usize,
}

impl<S: DeduplicationStorage> DeduplicationDetector<S> {
    /// Create a new deduplication detector with the given storage backend
    ///
    /// # Arguments
    ///
    /// * `storage` - The storage backend implementing `DeduplicationStorage`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use crucible_surrealdb::{ContentAddressedStorageSurrealDB, SurrealDbConfig};
    /// use crucible_surrealdb::deduplication_detector::DeduplicationDetector;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = SurrealDbConfig::memory();
    ///     let storage = ContentAddressedStorageSurrealDB::new(config).await?;
    ///     let detector = DeduplicationDetector::new(storage);
    ///     Ok(())
    /// }
    /// ```
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            average_block_size: 200, // Default estimate based on typical markdown blocks
        }
    }

    /// Create a detector with a custom average block size for better estimates
    ///
    /// # Arguments
    ///
    /// * `storage` - The storage backend implementing `DeduplicationStorage`
    /// * `average_block_size` - Average block size in bytes for estimations
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use crucible_surrealdb::{ContentAddressedStorageSurrealDB, SurrealDbConfig};
    /// use crucible_surrealdb::deduplication_detector::DeduplicationDetector;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = SurrealDbConfig::memory();
    ///     let storage = ContentAddressedStorageSurrealDB::new(config).await?;
    ///     let detector = DeduplicationDetector::with_average_block_size(storage, 500);
    ///     Ok(())
    /// }
    /// ```
    pub fn with_average_block_size(storage: S, average_block_size: usize) -> Self {
        Self {
            storage,
            average_block_size,
        }
    }

    /// Get a reference to the underlying storage backend
    ///
    /// This method allows access to the storage backend for operations
    /// not covered by the deduplication interface.
    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Estimate block size based on content length
    fn estimate_block_size(&self, content: &str) -> usize {
        if content.is_empty() {
            self.average_block_size
        } else {
            content.len()
        }
    }

    /// Generate content preview
    fn generate_content_preview(&self, content: &str, max_length: usize) -> String {
        if content.len() <= max_length {
            content.to_string()
        } else {
            // Ensure we don't exceed max_length including the "..."
            let truncate_at = max_length.saturating_sub(3);
            format!("{}...", &content[..truncate_at])
        }
    }
}

#[async_trait]
impl<S: DeduplicationStorage> DeduplicationStorage for DeduplicationDetector<S> {
    async fn find_documents_with_block(&self, block_hash: &str) -> StorageResult<Vec<String>> {
        self.storage.find_documents_with_block(block_hash).await
    }

    async fn get_document_blocks(&self, document_id: &str) -> StorageResult<Vec<BlockInfo>> {
        let records = self.storage.get_document_blocks(document_id).await?;

        let mut blocks = Vec::new();
        for record in records {
            blocks.push(BlockInfo {
                block_hash: record.block_hash,
                document_id: record.document_id,
                block_index: record.block_index,
                block_type: record.block_type,
                block_content: record.block_content,
                start_offset: record.start_offset,
                end_offset: record.end_offset,
                block_metadata: record.block_metadata,
                created_at: record.created_at,
                updated_at: record.updated_at,
            });
        }

        Ok(blocks)
    }

    async fn get_block_by_hash(&self, block_hash: &str) -> StorageResult<Option<BlockInfo>> {
        if let Some(record) = self.storage.get_block_by_hash(block_hash).await? {
            Ok(Some(BlockInfo {
                block_hash: record.block_hash,
                document_id: record.document_id,
                block_index: record.block_index,
                block_type: record.block_type,
                block_content: record.block_content,
                start_offset: record.start_offset,
                end_offset: record.end_offset,
                block_metadata: record.block_metadata,
                created_at: record.created_at,
                updated_at: record.updated_at,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_block_deduplication_stats(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<HashMap<String, usize>> {
        self.storage
            .get_block_deduplication_stats(block_hashes)
            .await
    }

    async fn get_all_block_deduplication_stats(&self) -> StorageResult<HashMap<String, usize>> {
        self.storage.get_all_block_deduplication_stats().await
    }

    async fn get_all_deduplication_stats(&self) -> StorageResult<DeduplicationStats> {
        // Get all deduplication statistics from storage
        let duplicate_stats = self.storage.get_all_block_deduplication_stats().await?;

        let total_unique_blocks = duplicate_stats.len();
        let total_block_instances: usize = duplicate_stats.values().sum();
        let duplicate_blocks = total_block_instances.saturating_sub(total_unique_blocks);

        // Calculate storage saved by deduplication
        let storage_saved = duplicate_stats.values().map(|count| {
                // Estimate block size - could be improved with actual content queries
                let block_size = self.average_block_size;
                block_size * (count - 1)
            })
            .sum();

        let deduplication_ratio = if total_block_instances > 0 {
            duplicate_blocks as f64 / total_block_instances as f64
        } else {
            0.0
        };

        // Find most duplicated blocks
        let mut most_duplicated: Vec<DuplicateBlockInfo> = duplicate_stats
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(hash, count)| {
                DuplicateBlockInfo {
                    block_hash: hash.clone(),
                    occurrence_count: count,
                    documents: Vec::new(), // Would need additional query
                    estimated_block_size: self.average_block_size,
                    storage_saved: self.average_block_size * (count - 1),
                    content_preview: "".to_string(), // Would need content query
                    block_type: "unknown".to_string(), // Would need content query
                    first_seen: Utc::now(),
                    last_seen: Utc::now(),
                }
            })
            .collect();

        // Sort by occurrence count (descending)
        most_duplicated.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));

        // Keep only top 10
        most_duplicated.truncate(10);

        Ok(DeduplicationStats {
            total_unique_blocks,
            total_block_instances,
            duplicate_blocks,
            deduplication_ratio,
            total_storage_saved: storage_saved,
            most_duplicated_blocks: most_duplicated,
            block_type_distribution: HashMap::new(), // Would need additional analysis
            average_block_size: self.average_block_size,
            calculated_at: Utc::now(),
        })
    }

    async fn find_duplicate_blocks(
        &self,
        min_occurrences: usize,
    ) -> StorageResult<Vec<DuplicateBlockInfo>> {
        // Get all duplicate blocks
        let all_stats = self.storage.get_all_block_deduplication_stats().await?;

        let mut duplicates = Vec::new();

        for (block_hash, &occurrence_count) in &all_stats {
            if occurrence_count >= min_occurrences {
                // Get documents containing this block
                let documents = self.storage.find_documents_with_block(block_hash).await?;

                // Get block info for content preview and type
                let block_info = self.get_block_by_hash(block_hash).await?;

                let (block_type, content_preview, estimated_size) = if let Some(info) = block_info {
                    (
                        info.block_type,
                        self.generate_content_preview(&info.block_content, 100),
                        self.estimate_block_size(&info.block_content),
                    )
                } else {
                    (
                        "unknown".to_string(),
                        "".to_string(),
                        self.average_block_size,
                    )
                };

                duplicates.push(DuplicateBlockInfo {
                    block_hash: block_hash.to_string(),
                    occurrence_count,
                    documents,
                    estimated_block_size: estimated_size,
                    storage_saved: estimated_size * (occurrence_count - 1),
                    content_preview,
                    block_type,
                    first_seen: Utc::now(), // Would need timestamp queries
                    last_seen: Utc::now(),
                });
            }
        }

        // Sort by occurrence count (descending)
        duplicates.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));

        Ok(duplicates)
    }

    async fn get_storage_usage_stats(&self) -> StorageResult<StorageUsageStats> {
        // Get comprehensive deduplication stats
        let dedup_stats = self.get_all_deduplication_stats().await?;

        let storage_efficiency = if dedup_stats.total_block_instances > 0 {
            dedup_stats.total_unique_blocks as f64 / dedup_stats.total_block_instances as f64
        } else {
            1.0
        };

        Ok(StorageUsageStats {
            total_block_storage: dedup_stats.total_block_instances * dedup_stats.average_block_size,
            deduplication_savings: dedup_stats.total_storage_saved,
            stored_block_count: dedup_stats.total_block_instances,
            unique_block_count: dedup_stats.total_unique_blocks,
            average_block_size: dedup_stats.average_block_size,
            storage_efficiency,
            calculated_at: Utc::now(),
        })
    }

    async fn find_documents_with_blocks(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<HashMap<String, Vec<String>>> {
        self.storage.find_documents_with_blocks(block_hashes).await
    }

    async fn get_blocks_by_hashes(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<HashMap<String, BlockInfo>> {
        let records = self.storage.get_blocks_by_hashes(block_hashes).await?;

        let mut hash_to_block = HashMap::new();
        for (hash, record) in records {
            hash_to_block.insert(
                hash,
                BlockInfo {
                    block_hash: record.block_hash,
                    document_id: record.document_id,
                    block_index: record.block_index,
                    block_type: record.block_type,
                    block_content: record.block_content,
                    start_offset: record.start_offset,
                    end_offset: record.end_offset,
                    block_metadata: record.block_metadata,
                    created_at: record.created_at,
                    updated_at: record.updated_at,
                },
            );
        }

        Ok(hash_to_block)
    }
}

// ==================== TYPE ALIASES FOR BACKWARDS COMPATIBILITY ====================

/// Type alias for SurrealDB-based deduplication detector
///
/// This provides backwards compatibility with existing code that uses
/// `SurrealDeduplicationDetector`. New code should prefer the generic
/// `DeduplicationDetector<S>` where appropriate.
pub type SurrealDeduplicationDetector = DeduplicationDetector<ContentAddressedStorageSurrealDB>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurrealDbConfig;

    #[tokio::test]
    async fn test_create_deduplication_detector() {
        let config = SurrealDbConfig::memory();
        let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();
        let detector = DeduplicationDetector::new(storage);

        // Test creation succeeds
        assert_eq!(detector.average_block_size, 200);
    }

    #[tokio::test]
    async fn test_create_with_custom_block_size() {
        let config = SurrealDbConfig::memory();
        let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();
        let detector = DeduplicationDetector::with_average_block_size(storage, 500);

        assert_eq!(detector.average_block_size, 500);
    }

    #[tokio::test]
    async fn test_type_alias_compatibility() {
        let config = SurrealDbConfig::memory();
        let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();

        // Test that the type alias works correctly
        let detector: SurrealDeduplicationDetector = SurrealDeduplicationDetector::new(storage);
        assert_eq!(detector.average_block_size, 200);
    }

    #[tokio::test]
    async fn test_generate_content_preview() {
        let config = SurrealDbConfig::memory();
        let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();
        let detector = DeduplicationDetector::new(storage);

        let short_content = "Short content";
        let preview = detector.generate_content_preview(short_content, 50);
        assert_eq!(preview, short_content);

        let long_content = "This is a very long content that should be truncated";
        let preview = detector.generate_content_preview(long_content, 20);
        assert_eq!(preview, "This is a very lo...");
    }

    #[tokio::test]
    async fn test_estimate_block_size() {
        let config = SurrealDbConfig::memory();
        let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();
        let detector = DeduplicationDetector::new(storage);

        let empty_content = "";
        let size = detector.estimate_block_size(empty_content);
        assert_eq!(size, 200); // Should use average size

        let content = "This is some content";
        let size = detector.estimate_block_size(content);
        assert_eq!(size, content.len());
    }

    #[tokio::test]
    async fn test_storage_accessor() {
        let config = SurrealDbConfig::memory();
        let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();
        let detector = DeduplicationDetector::new(storage.clone());

        // Test that we can access the underlying storage
        let storage_ref = detector.storage();

        // Verify it's the same type
        let _: &ContentAddressedStorageSurrealDB = storage_ref;
    }
}
