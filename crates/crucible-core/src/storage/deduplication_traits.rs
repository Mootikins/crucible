//! Deduplication Traits for Enhanced Storage Operations
//!
//! This module defines traits for advanced deduplication operations that extend
//! the basic content-addressed storage functionality. These traits enable
//! efficient duplicate detection, analysis, and storage optimization.

use crate::storage::StorageResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait for storage backends that support deduplication queries
#[async_trait]
pub trait DeduplicationStorage: Send + Sync {
    /// Find all documents containing a specific block hash
    async fn find_documents_with_block(&self, block_hash: &str) -> StorageResult<Vec<String>>;

    /// Get all blocks for a specific document
    async fn get_document_blocks(&self, document_id: &str) -> StorageResult<Vec<BlockInfo>>;

    /// Get block content by hash
    async fn get_block_by_hash(&self, block_hash: &str) -> StorageResult<Option<BlockInfo>>;

    /// Get deduplication statistics for specific block hashes
    async fn get_block_deduplication_stats(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<HashMap<String, usize>>;

    /// Get deduplication statistics for all blocks (hash -> count mapping)
    /// This is a low-level method that returns raw counts for all unique blocks
    async fn get_all_block_deduplication_stats(&self) -> StorageResult<HashMap<String, usize>>;

    /// Get comprehensive deduplication statistics for all blocks
    async fn get_all_deduplication_stats(&self) -> StorageResult<DeduplicationStats>;

    /// Find duplicate blocks above a minimum occurrence threshold
    async fn find_duplicate_blocks(
        &self,
        min_occurrences: usize,
    ) -> StorageResult<Vec<DuplicateBlockInfo>>;

    /// Get storage usage statistics
    async fn get_storage_usage_stats(&self) -> StorageResult<StorageUsageStats>;

    /// Batch query for multiple block hashes
    async fn find_documents_with_blocks(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<HashMap<String, Vec<String>>>;

    /// Get blocks by hashes (batch query)
    async fn get_blocks_by_hashes(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<HashMap<String, BlockInfo>>;
}

/// Information about a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    /// Block hash
    pub block_hash: String,
    /// Document ID containing the block
    pub document_id: String,
    /// Block index within document
    pub block_index: usize,
    /// Block type
    pub block_type: String,
    /// Block content
    pub block_content: String,
    /// Start offset in source document
    pub start_offset: usize,
    /// End offset in source document
    pub end_offset: usize,
    /// Block metadata
    pub block_metadata: HashMap<String, serde_json::Value>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// Comprehensive deduplication statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationStats {
    /// Total unique blocks in system
    pub total_unique_blocks: usize,
    /// Total block instances (including duplicates)
    pub total_block_instances: usize,
    /// Number of duplicate blocks
    pub duplicate_blocks: usize,
    /// Overall deduplication ratio
    pub deduplication_ratio: f64,
    /// Total storage saved by deduplication
    pub total_storage_saved: usize,
    /// Most duplicated blocks
    pub most_duplicated_blocks: Vec<DuplicateBlockInfo>,
    /// Block type distribution
    pub block_type_distribution: HashMap<String, usize>,
    /// Average block size
    pub average_block_size: usize,
    /// Statistics calculated at
    pub calculated_at: DateTime<Utc>,
}

/// Information about a duplicate block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateBlockInfo {
    /// Block hash
    pub block_hash: String,
    /// Number of occurrences
    pub occurrence_count: usize,
    /// Documents containing this block
    pub documents: Vec<String>,
    /// Estimated block size
    pub estimated_block_size: usize,
    /// Storage saved by deduplication
    pub storage_saved: usize,
    /// Block content preview (first 100 chars)
    pub content_preview: String,
    /// Block type
    pub block_type: String,
    /// First seen timestamp
    pub first_seen: DateTime<Utc>,
    /// Last seen timestamp
    pub last_seen: DateTime<Utc>,
}

/// Storage usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUsageStats {
    /// Total storage used for blocks
    pub total_block_storage: usize,
    /// Storage saved by deduplication
    pub deduplication_savings: usize,
    /// Number of stored blocks
    pub stored_block_count: usize,
    /// Number of unique blocks
    pub unique_block_count: usize,
    /// Average block size
    pub average_block_size: usize,
    /// Storage efficiency ratio (unique / total)
    pub storage_efficiency: f64,
    /// Statistics calculated at
    pub calculated_at: DateTime<Utc>,
}

/// Extension trait that combines basic storage with deduplication capabilities
pub trait DeduplicationCapable {
    /// Get a reference to the deduplication storage interface
    fn as_deduplication_storage(&self) -> &dyn DeduplicationStorage;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_info_serialization() {
        let block_info = BlockInfo {
            block_hash: "test_hash".to_string(),
            document_id: "test_doc.md".to_string(),
            block_index: 0,
            block_type: "heading".to_string(),
            block_content: "# Test".to_string(),
            start_offset: 0,
            end_offset: 6,
            block_metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Test serialization
        let json = serde_json::to_string(&block_info).unwrap();
        let deserialized: BlockInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(block_info.block_hash, deserialized.block_hash);
        assert_eq!(block_info.document_id, deserialized.document_id);
        assert_eq!(block_info.block_type, deserialized.block_type);
    }

    #[test]
    fn test_deduplication_stats() {
        let stats = DeduplicationStats {
            total_unique_blocks: 100,
            total_block_instances: 150,
            duplicate_blocks: 50,
            deduplication_ratio: 0.33,
            total_storage_saved: 10000,
            most_duplicated_blocks: Vec::new(),
            block_type_distribution: HashMap::new(),
            average_block_size: 200,
            calculated_at: Utc::now(),
        };

        assert_eq!(stats.total_unique_blocks, 100);
        assert_eq!(stats.deduplication_ratio, 0.33);
        assert!(stats.total_storage_saved > 0);
    }
}
