//! Block Storage Operations for Content-Addressed Storage
//!
//! This module provides high-level operations for storing and retrieving blocks
//! in the content-addressed storage system with document awareness.
//!
//! The storage is generic over the hashing algorithm, allowing for flexible
//! algorithm selection (BLAKE3, SHA256, etc.) following the Open/Closed Principle.

use crate::content_addressed_storage::ContentAddressedStorageSurrealDB;
use crate::SurrealDbConfig;
use async_trait::async_trait;
use crucible_core::hashing::algorithm::HashingAlgorithm;
use crucible_core::storage::{StorageError, StorageResult};
use crucible_parser::types::{ASTBlock, ASTBlockType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::marker::PhantomData;

/// Block storage interface for document-aware operations
#[async_trait]
pub trait BlockStorage: Send + Sync {
    /// Store blocks for a document
    async fn store_document_blocks(
        &self,
        document_id: &str,
        blocks: &[ASTBlock],
    ) -> StorageResult<()>;

    /// Get all blocks for a document
    async fn get_document_blocks(&self, document_id: &str) -> StorageResult<Vec<StoredBlock>>;

    /// Find documents containing a specific block hash
    async fn find_documents_with_block(&self, block_hash: &str) -> StorageResult<Vec<String>>;

    /// Get block content by hash
    async fn get_block_by_hash(&self, block_hash: &str) -> StorageResult<Option<StoredBlock>>;

    /// Delete all blocks for a document
    async fn delete_document_blocks(&self, document_id: &str) -> StorageResult<()>;

    /// Get deduplication statistics
    async fn get_deduplication_stats(&self) -> StorageResult<DeduplicationStats>;
}

/// A stored block with document context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBlock {
    /// Document identifier (file path)
    pub document_id: String,
    /// Block index within the document
    pub block_index: usize,
    /// Content hash of the block
    pub block_hash: String,
    /// Block type for context
    pub block_type: String,
    /// Start position in source document
    pub start_offset: usize,
    /// End position in source document
    pub end_offset: usize,
    /// Block content
    pub block_content: String,
    /// Block metadata
    pub block_metadata: HashMap<String, serde_json::Value>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Deduplication statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationStats {
    /// Total number of unique blocks
    pub total_unique_blocks: usize,
    /// Total number of block instances (including duplicates)
    pub total_block_instances: usize,
    /// Number of duplicate blocks
    pub duplicate_blocks: usize,
    /// Storage saved by deduplication (in characters)
    pub storage_saved: usize,
    /// Deduplication ratio (0.0 to 1.0)
    pub deduplication_ratio: f64,
    /// Most duplicated blocks
    pub most_duplicated: Vec<DuplicateBlockInfo>,
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
    /// Storage saved by deduplication
    pub storage_saved: usize,
}

/// Block storage implementation using SurrealDB with generic hashing algorithm
///
/// # Generic Parameters
///
/// * `A` - The hashing algorithm to use (implements `HashingAlgorithm`)
///
/// # Examples
///
/// ```rust,no_run
/// use crucible_surrealdb::block_storage::BlockStorageSurrealDB;
/// use crucible_core::hashing::algorithm::Blake3Algorithm;
/// use crucible_surrealdb::SurrealDbConfig;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = SurrealDbConfig {
///     namespace: "test".to_string(),
///     database: "test".to_string(),
///     path: ":memory:".to_string(),
///     max_connections: Some(10),
///     timeout_seconds: Some(30),
/// };
///
/// let storage = BlockStorageSurrealDB::new(Blake3Algorithm, config).await?;
/// # Ok(())
/// # }
/// ```
pub struct BlockStorageSurrealDB<A: HashingAlgorithm> {
    storage: ContentAddressedStorageSurrealDB,
    algorithm: A,
    _phantom: PhantomData<A>,
}

impl<A: HashingAlgorithm> BlockStorageSurrealDB<A> {
    /// Create a new block storage instance with the specified hashing algorithm
    ///
    /// # Arguments
    ///
    /// * `algorithm` - The hashing algorithm to use for content addressing
    /// * `config` - SurrealDB configuration
    ///
    /// # Returns
    ///
    /// A new BlockStorageSurrealDB instance or error
    pub async fn new(algorithm: A, config: SurrealDbConfig) -> StorageResult<Self> {
        let storage = ContentAddressedStorageSurrealDB::new(config).await?;
        Ok(Self {
            storage,
            algorithm,
            _phantom: PhantomData,
        })
    }

    /// Create an in-memory block storage for testing with the specified algorithm
    ///
    /// # Arguments
    ///
    /// * `algorithm` - The hashing algorithm to use for content addressing
    ///
    /// # Returns
    ///
    /// A new BlockStorageSurrealDB instance with in-memory storage
    pub async fn new_memory(algorithm: A) -> StorageResult<Self> {
        let storage = ContentAddressedStorageSurrealDB::new_memory().await?;
        Ok(Self {
            storage,
            algorithm,
            _phantom: PhantomData,
        })
    }

    /// Get the hashing algorithm used by this storage
    pub fn algorithm(&self) -> &A {
        &self.algorithm
    }

    /// Convert AST block type to string
    fn block_type_to_string(block_type: &ASTBlockType) -> String {
        match block_type {
            ASTBlockType::Heading => "heading".to_string(),
            ASTBlockType::Paragraph => "paragraph".to_string(),
            ASTBlockType::List => "list".to_string(),
            ASTBlockType::Code => "code".to_string(),
            ASTBlockType::Quote => "quote".to_string(),
            ASTBlockType::Callout => "callout".to_string(),
            ASTBlockType::Table => "table".to_string(),
            ASTBlockType::Math => "math".to_string(),
            ASTBlockType::Metadata => "metadata".to_string(),
            ASTBlockType::Frontmatter => "frontmatter".to_string(),
            ASTBlockType::Other => "other".to_string(),
        }
    }

    /// Convert AST block to metadata
    fn ast_block_to_metadata(block: &ASTBlock) -> HashMap<String, serde_json::Value> {
        let mut metadata = HashMap::new();

        // Store block-specific metadata
        metadata.insert("level".to_string(), serde_json::Value::Number(
            serde_json::Number::from(block.metadata.level as i64)
        ));

        // Store any additional metadata from the AST block
        if !block.metadata.additional.is_empty() {
            for (key, value) in &block.metadata.additional {
                metadata.insert(key.clone(), serde_json::to_value(value).unwrap_or_default());
            }
        }

        metadata
    }
}

#[async_trait]
impl<A: HashingAlgorithm> BlockStorage for BlockStorageSurrealDB<A> {
    async fn store_document_blocks(
        &self,
        document_id: &str,
        blocks: &[ASTBlock],
    ) -> StorageResult<()> {
        for (index, block) in blocks.iter().enumerate() {
            let block_type = Self::block_type_to_string(&block.block_type);
            let block_metadata = Some(Self::ast_block_to_metadata(block));

            self.storage
                .store_document_block(
                    document_id,
                    index,
                    &block.block_hash,
                    &block_type,
                    block.start_offset,
                    block.end_offset,
                    &block.content,
                    block_metadata,
                )
                .await?;
        }

        Ok(())
    }

    async fn get_document_blocks(&self, document_id: &str) -> StorageResult<Vec<StoredBlock>> {
        let records = self.storage.get_document_blocks(document_id).await?;

        let mut blocks = Vec::new();
        for record in records {
            let block = StoredBlock {
                document_id: record.document_id,
                block_index: record.block_index,
                block_hash: record.block_hash,
                block_type: record.block_type,
                start_offset: record.start_offset,
                end_offset: record.end_offset,
                block_content: record.block_content,
                block_metadata: record.block_metadata,
                created_at: record.created_at,
                updated_at: record.updated_at,
            };
            blocks.push(block);
        }

        Ok(blocks)
    }

    async fn find_documents_with_block(&self, block_hash: &str) -> StorageResult<Vec<String>> {
        self.storage.find_documents_with_block(block_hash).await
    }

    async fn get_block_by_hash(&self, block_hash: &str) -> StorageResult<Option<StoredBlock>> {
        if let Some(record) = self.storage.get_block_by_hash(block_hash).await? {
            let block = StoredBlock {
                document_id: record.document_id,
                block_index: record.block_index,
                block_hash: record.block_hash,
                block_type: record.block_type,
                start_offset: record.start_offset,
                end_offset: record.end_offset,
                block_content: record.block_content,
                block_metadata: record.block_metadata,
                created_at: record.created_at,
                updated_at: record.updated_at,
            };
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }

    async fn delete_document_blocks(&self, document_id: &str) -> StorageResult<()> {
        self.storage.delete_document_blocks(document_id).await?;
        Ok(())
    }

    async fn get_deduplication_stats(&self) -> StorageResult<DeduplicationStats> {
        let duplicate_stats = self.storage.get_block_deduplication_stats().await?;

        let total_unique_blocks = duplicate_stats.len();
        let total_block_instances = duplicate_stats.values().sum();
        let duplicate_blocks = total_block_instances.saturating_sub(total_unique_blocks);

        // Calculate storage saved by deduplication
        let storage_saved = duplicate_stats
            .iter()
            .map(|(hash, count)| {
                // Assume average block size of 200 characters
                let block_size = 200;
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
                    documents: Vec::new(), // Would need additional query to populate
                    storage_saved: 200 * (count - 1), // Assume average block size
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
            storage_saved,
            deduplication_ratio,
            most_duplicated,
        })
    }
}

/// Type alias for BlockStorageSurrealDB using BLAKE3 hashing algorithm
///
/// This is the recommended default for most use cases due to BLAKE3's
/// superior performance characteristics.
pub type Blake3BlockStorage = BlockStorageSurrealDB<crucible_core::hashing::algorithm::Blake3Algorithm>;

/// Type alias for BlockStorageSurrealDB using SHA256 hashing algorithm
///
/// Use this for compatibility with existing systems that require SHA256.
pub type Sha256BlockStorage = BlockStorageSurrealDB<crucible_core::hashing::algorithm::Sha256Algorithm>;

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::hashing::algorithm::Blake3Algorithm;
    use crucible_parser::types::{ASTBlock, ASTBlockMetadata, ASTBlockType};

    /// Create a test AST block
    fn create_test_ast_block(
        content: &str,
        block_type: ASTBlockType,
        start_offset: usize,
        end_offset: usize,
    ) -> ASTBlock {
        use crucible_core::hashing::blake3::Blake3Hasher;
        use crucible_core::storage::ContentHasher;

        let hasher = Blake3Hasher::new();
        let block_hash = hasher.hash_block(content.as_bytes()).to_string();

        ASTBlock {
            block_type,
            content: content.to_string(),
            start_offset,
            end_offset,
            block_hash,
            metadata: ASTBlockMetadata {
                level: 1,
                additional: HashMap::new(),
            },
        }
    }

    #[tokio::test]
    async fn test_store_and_get_document_blocks() {
        let storage = BlockStorageSurrealDB::new_memory(Blake3Algorithm).await.unwrap();

        let document_id = "test/document.md";
        let blocks = vec![
            create_test_ast_block("# Test Heading", ASTBlockType::Heading, 0, 13),
            create_test_ast_block("Test paragraph content.", ASTBlockType::Paragraph, 14, 38),
        ];

        // Store blocks
        storage.store_document_blocks(document_id, &blocks).await.unwrap();

        // Retrieve blocks
        let retrieved_blocks = storage.get_document_blocks(document_id).await.unwrap();
        assert_eq!(retrieved_blocks.len(), 2);

        // Verify block order
        assert_eq!(retrieved_blocks[0].block_index, 0);
        assert_eq!(retrieved_blocks[1].block_index, 1);

        // Verify block content
        assert_eq!(retrieved_blocks[0].block_content, "# Test Heading");
        assert_eq!(retrieved_blocks[1].block_content, "Test paragraph content.");

        // Verify block types
        assert_eq!(retrieved_blocks[0].block_type, "heading");
        assert_eq!(retrieved_blocks[1].block_type, "paragraph");
    }

    #[tokio::test]
    async fn test_find_documents_with_block() {
        let storage = BlockStorageSurrealDB::new_memory(Blake3Algorithm).await.unwrap();

        let document1 = "doc1.md";
        let document2 = "doc2.md";
        let common_content = "Common block content";
        let common_hash = {
            use crucible_core::hashing::blake3::Blake3Hasher;
            use crucible_core::storage::ContentHasher;
            let hasher = Blake3Hasher::new();
            hasher.hash_block(common_content.as_bytes()).to_string()
        };

        let block1 = create_test_ast_block(common_content, ASTBlockType::Paragraph, 0, common_content.len());
        let block2 = create_test_ast_block(common_content, ASTBlockType::Paragraph, 0, common_content.len());

        // Store same block in two documents
        storage.store_document_blocks(document1, &[block1]).await.unwrap();
        storage.store_document_blocks(document2, &[block2]).await.unwrap();

        // Find documents containing the block
        let documents = storage.find_documents_with_block(&common_hash).await.unwrap();
        assert_eq!(documents.len(), 2);
        assert!(documents.contains(&document1.to_string()));
        assert!(documents.contains(&document2.to_string()));
    }

    #[tokio::test]
    async fn test_get_block_by_hash() {
        let storage = BlockStorageSurrealDB::new_memory(Blake3Algorithm).await.unwrap();

        let document_id = "test.md";
        let block = create_test_ast_block("Test content", ASTBlockType::Paragraph, 0, 12);
        let block_hash = block.block_hash.clone();

        storage.store_document_blocks(document_id, &[block]).await.unwrap();

        let retrieved_block = storage.get_block_by_hash(&block_hash).await.unwrap();
        assert!(retrieved_block.is_some());

        let block = retrieved_block.unwrap();
        assert_eq!(block.block_content, "Test content");
        assert_eq!(block.block_type, "paragraph");
        assert_eq!(block.document_id, document_id);
    }

    #[tokio::test]
    async fn test_delete_document_blocks() {
        let storage = BlockStorageSurrealDB::new_memory(Blake3Algorithm).await.unwrap();

        let document_id = "test.md";
        let blocks = vec![
            create_test_ast_block("Block 1", ASTBlockType::Paragraph, 0, 6),
            create_test_ast_block("Block 2", ASTBlockType::Paragraph, 7, 13),
        ];

        storage.store_document_blocks(document_id, &blocks).await.unwrap();

        // Verify blocks exist
        let retrieved = storage.get_document_blocks(document_id).await.unwrap();
        assert_eq!(retrieved.len(), 2);

        // Delete blocks
        storage.delete_document_blocks(document_id).await.unwrap();

        // Verify blocks are deleted
        let retrieved = storage.get_document_blocks(document_id).await.unwrap();
        assert_eq!(retrieved.len(), 0);
    }

    #[tokio::test]
    async fn test_deduplication_stats() {
        let storage = BlockStorageSurrealDB::new_memory(Blake3Algorithm).await.unwrap();

        let common_content = "Shared block content";
        let common_block = create_test_ast_block(common_content, ASTBlockType::Paragraph, 0, common_content.len());

        // Store same block in multiple documents
        storage.store_document_blocks("doc1.md", &[common_block.clone()]).await.unwrap();
        storage.store_document_blocks("doc2.md", &[common_block.clone()]).await.unwrap();
        storage.store_document_blocks("doc3.md", &[common_block.clone()]).await.unwrap();

        // Get deduplication stats
        let stats = storage.get_deduplication_stats().await.unwrap();

        assert_eq!(stats.total_unique_blocks, 1);
        assert_eq!(stats.total_block_instances, 3);
        assert_eq!(stats.duplicate_blocks, 2);
        assert!(stats.deduplication_ratio > 0.0);
        assert_eq!(stats.most_duplicated.len(), 1);
        assert_eq!(stats.most_duplicated[0].occurrence_count, 3);
    }
}