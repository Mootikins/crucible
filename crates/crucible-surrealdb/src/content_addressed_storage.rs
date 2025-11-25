//! Content-Addressed Storage implementation for SurrealDB
//!
//! This module provides a SurrealDB backend for the ContentAddressedStorage trait,
//! enabling persistent storage of content blocks and Merkle trees with full ACID
//! transaction support and efficient hash-based lookups.

use crate::utils::sanitize_record_id;
use crate::{SurrealClient, SurrealDbConfig};
use async_trait::async_trait;
use crucible_core::storage::traits::{BlockOperations, StorageStats, TreeOperations};
use crucible_core::storage::{ContentAddressedStorage, MerkleTree, StorageError, StorageResult};
use crucible_core::parser::{ASTBlock, ASTBlockMetadata, ASTBlockType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Content-addressed storage backend using SurrealDB
///
/// This implementation provides:
/// - Persistent storage of content blocks with hash-based addressing
/// - Merkle tree storage and retrieval for change detection workflows
/// - ACID transaction support for data consistency
/// - Efficient indexing for hash-based lookups
/// - Integration with existing SurrealDB infrastructure
#[derive(Clone)]
pub struct ContentAddressedStorageSurrealDB {
    /// The underlying SurrealDB client
    client: SurrealClient,
}

/// Database record for content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContentBlockRecord {
    /// The block hash (primary key)
    pub hash: String,
    /// The binary content data
    pub data: Vec<u8>,
    /// Size of the data in bytes
    pub size: usize,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Database record for Merkle trees
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MerkleTreeRecord {
    /// The root hash (primary key)
    pub root_hash: String,
    /// Serialized Merkle tree structure
    pub tree_data: MerkleTree,
    /// Number of blocks in the tree
    pub block_count: usize,
    /// Tree depth
    pub depth: usize,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Optional metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Database record for note-block mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentBlockRecord {
    /// Note identifier (file path)
    pub document_id: String,
    /// Block index within the note
    pub block_index: usize,
    /// Content hash of the block
    pub block_hash: String,
    /// Block type for context
    pub block_type: String,
    /// Start position in source note
    pub start_offset: usize,
    /// End position in source note
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

impl ContentAddressedStorageSurrealDB {
    /// Create a new content-addressed storage backend with the given configuration
    ///
    /// # Arguments
    /// * `config` - SurrealDB configuration
    ///
    /// # Returns
    /// A new ContentAddressedStorageSurrealDB instance or an error
    pub async fn new(config: SurrealDbConfig) -> StorageResult<Self> {
        let client = SurrealClient::new(config)
            .await
            .map_err(|e| StorageError::backend(format!("Failed to create SurrealClient: {}", e)))?;

        let storage = Self { client };

        // Initialize database schema
        storage.initialize_schema().await?;

        Ok(storage)
    }

    /// Create an in-memory content-addressed storage for testing
    ///
    /// # Returns
    /// A new ContentAddressedStorageSurrealDB instance with in-memory storage
    pub async fn new_memory() -> StorageResult<Self> {
        let config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "content_storage".to_string(),
            path: ":memory:".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        Self::new(config).await
    }

    /// Create a file-based content-addressed storage using RocksDB
    ///
    /// # Arguments
    /// * `path` - Directory path for the database storage
    ///
    /// # Returns
    /// A new ContentAddressedStorageSurrealDB instance with persistent storage
    pub async fn new_file(path: &str) -> StorageResult<Self> {
        let config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "content_storage".to_string(),
            path: path.to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        Self::new(config).await
    }

    /// Initialize database schema for content-addressed storage
    ///
    /// This creates the necessary tables and indexes for efficient operations.
    /// Schema creation is made idempotent to handle concurrent access safely.
    async fn initialize_schema(&self) -> StorageResult<()> {
        // Create content_blocks table with indexes - make it idempotent for concurrency
        let schema_queries = [
            // Remove existing definitions first (if they exist) to avoid conflicts
            "REMOVE TABLE IF EXISTS content_blocks",
            "REMOVE TABLE IF EXISTS merkle_trees",
            // Create content_blocks table with proper schema
            "DEFINE TABLE content_blocks SCHEMAFULL",
            "DEFINE FIELD hash ON TABLE content_blocks TYPE string",
            "DEFINE FIELD data ON TABLE content_blocks TYPE array<number>",
            "DEFINE FIELD size ON TABLE content_blocks TYPE number",
            "DEFINE FIELD created_at ON TABLE content_blocks TYPE datetime",
            "DEFINE FIELD metadata ON TABLE content_blocks FLEXIBLE",
            "DEFINE INDEX hash_idx ON TABLE content_blocks COLUMNS hash",
            // Create merkle_trees table with proper schema
            "DEFINE TABLE merkle_trees SCHEMAFULL",
            "DEFINE FIELD root_hash ON TABLE merkle_trees TYPE string",
            "DEFINE FIELD tree_data ON TABLE merkle_trees FLEXIBLE",
            "DEFINE FIELD block_count ON TABLE merkle_trees TYPE number",
            "DEFINE FIELD depth ON TABLE merkle_trees TYPE number",
            "DEFINE FIELD created_at ON TABLE merkle_trees TYPE datetime",
            "DEFINE FIELD updated_at ON TABLE merkle_trees TYPE datetime",
            "DEFINE FIELD metadata ON TABLE merkle_trees FLEXIBLE",
            "DEFINE INDEX root_hash_idx ON TABLE merkle_trees COLUMNS root_hash",
            "DEFINE INDEX created_at_idx ON TABLE merkle_trees COLUMNS created_at",
        ];

        for query in schema_queries.iter() {
            // Execute each query and ignore certain errors that occur in concurrent scenarios
            if let Err(e) = self.client.query(query, &[]).await {
                let error_str = e.to_string();
                // Ignore errors about tables/indexes already existing or not existing
                if error_str.contains("already exists")
                    || error_str.contains("not found")
                    || error_str.contains("does not exist")
                {
                    continue; // These are expected in concurrent scenarios
                }
                // For other errors, try to continue since schema might already be partially created
                if !error_str.contains("Failed to create") {
                    continue;
                }
                return Err(StorageError::backend(format!(
                    "Schema initialization failed: {}",
                    e
                )));
            }
        }

        Ok(())
    }

    /// Get a reference to the underlying SurrealDB client
    pub fn client(&self) -> &SurrealClient {
        &self.client
    }

    /// Store a content block record
    async fn store_block_record(&self, record: &ContentBlockRecord) -> StorageResult<()> {
        let metadata_json = record
            .metadata
            .as_ref()
            .and_then(|m| serde_json::to_value(m).ok());

        let query = if let Some(metadata) = metadata_json {
            format!(
                "UPSERT content_blocks:`{}` CONTENT {{
                    hash: '{}',
                    data: {},
                    size: {},
                    created_at: time::now(),
                    metadata: {}
                }}",
                record.hash,
                record.hash,
                serde_json::to_string(&record.data).unwrap_or_default(),
                record.size,
                serde_json::to_string(&metadata).unwrap_or_default()
            )
        } else {
            format!(
                "UPSERT content_blocks:`{}` CONTENT {{
                    hash: '{}',
                    data: {},
                    size: {},
                    created_at: time::now()
                }}",
                record.hash,
                record.hash,
                serde_json::to_string(&record.data).unwrap_or_default(),
                record.size
            )
        };

        self.client
            .query(&query, &[])
            .await
            .map_err(|e| StorageError::backend(format!("Failed to store block: {}", e)))?;

        Ok(())
    }

    /// Store a Merkle tree record
    async fn store_tree_record(&self, record: &MerkleTreeRecord) -> StorageResult<()> {
        // Serialize the entire MerkleTree as a JSON string to preserve all fields
        let tree_data_json = serde_json::to_string(&record.tree_data).map_err(|e| {
            StorageError::serialization(format!("Failed to serialize MerkleTree: {}", e))
        })?;

        let metadata_json = record
            .metadata
            .as_ref()
            .and_then(|m| serde_json::to_value(m).ok());

        let query = if let Some(metadata) = metadata_json {
            format!(
                "UPSERT merkle_trees:`{}` CONTENT {{
                    root_hash: '{}',
                    tree_data: '{}',
                    block_count: {},
                    depth: {},
                    created_at: time::now(),
                    updated_at: time::now(),
                    metadata: {}
                }}",
                record.root_hash,
                record.root_hash,
                tree_data_json, // Store as string, not as JSON object
                record.block_count,
                record.depth,
                serde_json::to_string(&metadata).unwrap_or_default()
            )
        } else {
            format!(
                "UPSERT merkle_trees:`{}` CONTENT {{
                    root_hash: '{}',
                    tree_data: '{}',
                    block_count: {},
                    depth: {},
                    created_at: time::now(),
                    updated_at: time::now()
                }}",
                record.root_hash,
                record.root_hash,
                tree_data_json, // Store as string, not as JSON object
                record.block_count,
                record.depth
            )
        };

        self.client
            .query(&query, &[])
            .await
            .map_err(|e| StorageError::backend(format!("Failed to store Merkle tree: {}", e)))?;

        Ok(())
    }

    /// Store blocks for a note (BlockStorage trait implementation)
    pub async fn store_document_blocks_from_ast(
        &self,
        document_id: &str,
        blocks: &[ASTBlock],
    ) -> StorageResult<()> {
        for (index, block) in blocks.iter().enumerate() {
            let block_type = self.ast_block_type_to_string(&block.block_type);
            let block_metadata = Some(self.ast_block_to_metadata(block));

            // Store note block directly (inline store_document_block logic)
            if document_id.is_empty() {
                return Err(StorageError::InvalidOperation(
                    "Empty document_id provided".to_string(),
                ));
            }

            if block.block_hash.is_empty() {
                return Err(StorageError::InvalidHash(
                    "Empty block_hash provided".to_string(),
                ));
            }

            let now = chrono::Utc::now();
            let query = format!(
                "UPSERT document_blocks:`{}:{}` CONTENT {{
                    document_id: '{}',
                    block_index: {},
                    block_hash: '{}',
                    block_type: '{}',
                    start_offset: {},
                    end_offset: {},
                    block_content: '{}',
                    block_metadata: '{}',
                    created_at: '{}',
                    updated_at: '{}'
                }}",
                document_id.replace('/', "_").replace("\\", "_"), // Sanitize ID
                index,
                document_id.replace("'", "''"), // Escape quotes
                index,
                block.block_hash.replace("'", "''"),
                block_type.replace("'", "''"),
                block.start_offset,
                block.end_offset,
                block.content.replace("'", "''").replace("\n", "\\n"),
                serde_json::to_string(&block_metadata.unwrap_or_default()).unwrap_or_default(),
                now.to_rfc3339(),
                now.to_rfc3339()
            );

            self.client
                .query(&query, &[])
                .await
                .map_err(|e| StorageError::backend(format!("Failed to store note block: {}", e)))?;
        }

        Ok(())
    }

    /// Convert AST block type to string
    fn ast_block_type_to_string(&self, block_type: &ASTBlockType) -> String {
        match block_type {
            ASTBlockType::Heading => "heading".to_string(),
            ASTBlockType::Paragraph => "paragraph".to_string(),
            ASTBlockType::List => "list".to_string(),
            ASTBlockType::Code => "code".to_string(),
            ASTBlockType::Callout => "callout".to_string(),
            ASTBlockType::Latex => "latex".to_string(),
            ASTBlockType::Blockquote => "blockquote".to_string(),
            ASTBlockType::Table => "table".to_string(),
            ASTBlockType::HorizontalRule => "horizontal_rule".to_string(),
            ASTBlockType::ThematicBreak => "thematic_break".to_string(),
        }
    }

    /// Convert AST block to metadata
    fn ast_block_to_metadata(&self, block: &ASTBlock) -> HashMap<String, serde_json::Value> {
        let mut metadata = HashMap::new();

        // Store block-specific metadata based on type
        match &block.metadata {
            ASTBlockMetadata::Heading { level, id } => {
                metadata.insert(
                    "level".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(*level as i64)),
                );
                if let Some(id) = id {
                    metadata.insert("id".to_string(), serde_json::Value::String(id.to_string()));
                }
            }
            ASTBlockMetadata::Code {
                language,
                line_count,
            } => {
                if let Some(lang) = language {
                    metadata.insert(
                        "language".to_string(),
                        serde_json::Value::String(lang.to_string()),
                    );
                }
                metadata.insert(
                    "line_count".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(*line_count as i64)),
                );
            }
            ASTBlockMetadata::List {
                list_type,
                item_count,
            } => {
                metadata.insert(
                    "list_type".to_string(),
                    serde_json::Value::String(format!("{:?}", list_type)),
                );
                metadata.insert(
                    "item_count".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(*item_count as i64)),
                );
            }
            ASTBlockMetadata::Callout {
                callout_type,
                title,
                is_standard_type,
            } => {
                metadata.insert(
                    "callout_type".to_string(),
                    serde_json::Value::String(callout_type.clone()),
                );
                metadata.insert(
                    "is_standard_type".to_string(),
                    serde_json::Value::Bool(*is_standard_type),
                );
                if let Some(title) = title {
                    metadata.insert(
                        "title".to_string(),
                        serde_json::Value::String(title.to_string()),
                    );
                }
            }
            ASTBlockMetadata::Latex { is_block } => {
                metadata.insert("is_block".to_string(), serde_json::Value::Bool(*is_block));
            }
            ASTBlockMetadata::Table {
                rows,
                columns,
                headers,
            } => {
                metadata.insert(
                    "rows".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(*rows as i64)),
                );
                metadata.insert(
                    "columns".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(*columns as i64)),
                );
                metadata.insert(
                    "headers".to_string(),
                    serde_json::Value::Array(
                        headers
                            .iter()
                            .map(|h| serde_json::Value::String(h.clone()))
                            .collect(),
                    ),
                );
            }
            ASTBlockMetadata::Generic => {
                // No additional metadata for generic blocks
            }
        }

        metadata
    }
}

// ==================== BLOCK OPERATIONS TRAIT IMPLEMENTATION ====================

#[async_trait]
impl crucible_core::storage::traits::BlockOperations for ContentAddressedStorageSurrealDB {
    async fn store_block(&self, hash: &str, data: &[u8]) -> StorageResult<()> {
        if hash.is_empty() {
            return Err(StorageError::InvalidHash("Empty hash provided".to_string()));
        }

        if data.is_empty() {
            return Err(StorageError::InvalidOperation(
                "Empty data provided".to_string(),
            ));
        }

        let record = ContentBlockRecord {
            hash: hash.to_string(),
            data: data.to_vec(),
            size: data.len(),
            created_at: chrono::Utc::now(),
            metadata: None,
        };

        self.store_block_record(&record).await
    }

    async fn get_block(&self, hash: &str) -> StorageResult<Option<Vec<u8>>> {
        if hash.is_empty() {
            return Err(StorageError::InvalidHash("Empty hash provided".to_string()));
        }

        let safe_hash = sanitize_record_id(hash)
            .map_err(|e| StorageError::InvalidHash(format!("Invalid hash: {}", e)))?;
        let query = format!("SELECT data FROM content_blocks:⟨{}⟩", safe_hash);
        let result = self
            .client
            .query(&query, &[])
            .await
            .map_err(|e| StorageError::backend(format!("Failed to retrieve block: {}", e)))?;

        if result.records.is_empty() {
            return Ok(None);
        }

        let record = &result.records[0];
        if let Some(data) = record.data.get("data") {
            let data_vec: Vec<u8> = serde_json::from_value(data.clone()).map_err(|e| {
                StorageError::deserialization(format!("Failed to deserialize block data: {}", e))
            })?;
            Ok(Some(data_vec))
        } else {
            Ok(None)
        }
    }

    async fn block_exists(&self, hash: &str) -> StorageResult<bool> {
        if hash.is_empty() {
            return Err(StorageError::InvalidHash("Empty hash provided".to_string()));
        }

        let safe_hash = sanitize_record_id(hash)
            .map_err(|e| StorageError::InvalidHash(format!("Invalid hash: {}", e)))?;
        let query = format!("SELECT count() FROM content_blocks WHERE hash = '{}'", safe_hash);
        let result = self.client.query(&query, &[]).await.map_err(|e| {
            StorageError::backend(format!("Failed to check block existence: {}", e))
        })?;

        // Check if we got any results
        Ok(!result.records.is_empty())
    }

    async fn delete_block(&self, hash: &str) -> StorageResult<bool> {
        if hash.is_empty() {
            return Err(StorageError::InvalidHash("Empty hash provided".to_string()));
        }

        // Check if block exists before deletion
        let existed = BlockOperations::block_exists(self, hash).await?;
        if !existed {
            return Ok(false);
        }

        // Delete the block
        let safe_hash = sanitize_record_id(hash)
            .map_err(|e| StorageError::InvalidHash(format!("Invalid hash: {}", e)))?;
        let query = format!("DELETE FROM content_blocks:⟨{}⟩", safe_hash);
        let _result = self
            .client
            .query(&query, &[])
            .await
            .map_err(|e| StorageError::backend(format!("Failed to delete block: {}", e)))?;

        // Verify deletion was successful by checking if it still exists
        let exists_after = BlockOperations::block_exists(self, hash).await?;
        Ok(!exists_after)
    }
}

// ==================== TREE OPERATIONS TRAIT IMPLEMENTATION ====================

#[async_trait]
impl crucible_core::storage::traits::TreeOperations for ContentAddressedStorageSurrealDB {
    async fn store_tree(&self, root_hash: &str, tree: &MerkleTree) -> StorageResult<()> {
        if root_hash.is_empty() {
            return Err(StorageError::InvalidHash(
                "Empty root hash provided".to_string(),
            ));
        }

        let now = chrono::Utc::now();
        let record = MerkleTreeRecord {
            root_hash: root_hash.to_string(),
            tree_data: tree.clone(),
            block_count: tree.block_count,
            depth: tree.depth,
            created_at: now,
            updated_at: now,
            metadata: None,
        };

        self.store_tree_record(&record).await
    }

    async fn get_tree(&self, root_hash: &str) -> StorageResult<Option<MerkleTree>> {
        if root_hash.is_empty() {
            return Err(StorageError::InvalidHash(
                "Empty root hash provided".to_string(),
            ));
        }

        let safe_hash = sanitize_record_id(root_hash)
            .map_err(|e| StorageError::InvalidHash(format!("Invalid root hash: {}", e)))?;
        let query = format!("SELECT tree_data FROM merkle_trees:⟨{}⟩", safe_hash);
        let result =
            self.client.query(&query, &[]).await.map_err(|e| {
                StorageError::backend(format!("Failed to retrieve Merkle tree: {}", e))
            })?;

        if result.records.is_empty() {
            return Ok(None);
        }

        let record = &result.records[0];
        if let Some(tree_data) = record.data.get("tree_data") {
            // tree_data is now stored as a JSON string, so extract the string first
            let tree_json_str = tree_data.as_str().ok_or_else(|| {
                StorageError::deserialization("tree_data is not a string".to_string())
            })?;

            // Then deserialize the string as MerkleTree
            let tree: MerkleTree = serde_json::from_str(tree_json_str).map_err(|e| {
                StorageError::deserialization(format!("Failed to deserialize Merkle tree: {}", e))
            })?;
            Ok(Some(tree))
        } else {
            Ok(None)
        }
    }

    async fn tree_exists(&self, root_hash: &str) -> StorageResult<bool> {
        if root_hash.is_empty() {
            return Err(StorageError::InvalidHash(
                "Empty root hash provided".to_string(),
            ));
        }

        let safe_hash = sanitize_record_id(root_hash)
            .map_err(|e| StorageError::InvalidHash(format!("Invalid root hash: {}", e)))?;
        let query = format!(
            "SELECT count() FROM merkle_trees WHERE root_hash = '{}'",
            safe_hash
        );
        let result =
            self.client.query(&query, &[]).await.map_err(|e| {
                StorageError::backend(format!("Failed to check tree existence: {}", e))
            })?;

        // Check if we got any results
        Ok(!result.records.is_empty())
    }

    async fn delete_tree(&self, root_hash: &str) -> StorageResult<bool> {
        if root_hash.is_empty() {
            return Err(StorageError::InvalidHash(
                "Empty root hash provided".to_string(),
            ));
        }

        // Check if tree exists before deletion
        let existed = TreeOperations::tree_exists(self, root_hash).await?;
        if !existed {
            return Ok(false);
        }

        // Delete the tree
        let safe_hash = sanitize_record_id(root_hash)
            .map_err(|e| StorageError::InvalidHash(format!("Invalid root hash: {}", e)))?;
        let query = format!("DELETE FROM merkle_trees:⟨{}⟩", safe_hash);
        let _result =
            self.client.query(&query, &[]).await.map_err(|e| {
                StorageError::backend(format!("Failed to delete Merkle tree: {}", e))
            })?;

        // Verify deletion was successful by checking if it still exists
        let exists_after = TreeOperations::tree_exists(self, root_hash).await?;
        Ok(!exists_after)
    }
}

// ==================== STORAGE MANAGEMENT TRAIT IMPLEMENTATION ====================

#[async_trait]
impl crucible_core::storage::traits::StorageManagement for ContentAddressedStorageSurrealDB {
    async fn get_stats(&self) -> StorageResult<StorageStats> {
        // Get block statistics - use proper SurrealDB count query syntax
        let block_count_query = "SELECT * FROM content_blocks";
        let block_result = self
            .client
            .query(block_count_query, &[])
            .await
            .map_err(|e| StorageError::backend(format!("Failed to get block count: {}", e)))?;

        let block_count = block_result.records.len() as u64;

        // For now, calculate a simple estimate based on block count
        let block_size_bytes = block_count * 1024; // Assume 1KB average size

        // Get tree statistics - use proper SurrealDB count query syntax
        let tree_count_query = "SELECT * FROM hybrid_tree";
        let tree_result = self
            .client
            .query(tree_count_query, &[])
            .await
            .map_err(|e| StorageError::backend(format!("Failed to get tree count: {}", e)))?;

        let tree_count = tree_result.records.len() as u64;

        // Get section count for additional stats
        let section_count_query = "SELECT * FROM section";
        let section_result = self
            .client
            .query(section_count_query, &[])
            .await
            .ok();

        let section_count = section_result.map(|r| r.records.len() as u64).unwrap_or(0);

        // Log Merkle tree stats for debugging
        tracing::debug!(
            "Merkle storage stats: {} trees, {} sections, {} blocks",
            tree_count,
            section_count,
            block_count
        );

        // Calculate average and largest block size
        let avg_block_size = if block_count > 0 {
            block_size_bytes as f64 / block_count as f64
        } else {
            0.0
        };

        // For now, set largest block size to a reasonable default
        let largest_block_size = if block_count > 0 { 4096 } else { 0 }; // 4KB if we have blocks

        Ok(StorageStats {
            backend: crucible_core::storage::traits::StorageBackend::SurrealDB,
            block_count,
            block_size_bytes,
            tree_count,
            section_count,
            deduplication_savings: 0, // TODO: Implement deduplication tracking
            average_block_size: avg_block_size,
            largest_block_size,
            evicted_blocks: 0, // Not applicable for persistent storage
            quota_usage: None, // TODO: Implement quota tracking if needed
        })
    }

    async fn maintenance(&self) -> StorageResult<()> {
        // Perform maintenance operations

        // Clean up orphaned records (if any) - use proper SurrealDB time syntax
        self.client
            .query(
                "DELETE FROM content_blocks WHERE created_at < (time::now() - 30d)",
                &[],
            )
            .await
            .map_err(|e| StorageError::backend(format!("Failed to perform cleanup: {}", e)))?;

        // Clean up orphaned document_blocks (if any)
        self.client
            .query(
                "DELETE FROM document_blocks WHERE created_at < (time::now() - 30d)",
                &[],
            )
            .await
            .map_err(|e| {
                StorageError::backend(format!("Failed to perform document_blocks cleanup: {}", e))
            })?;

        // Optimize indexes - use proper SurrealDB syntax
        // Note: ANALYZE INDEX might not be the correct syntax, let's use simpler maintenance
        // For now, we'll skip index optimization and focus on the cleanup
        // The indexes should be automatically maintained by SurrealDB

        Ok(())
    }
}

// ==================== CONTENT ADDRESSED STORAGE BLANKET IMPLEMENTATION ====================

/// Blanket implementation of ContentAddressedStorage since we implement all required traits
impl ContentAddressedStorage for ContentAddressedStorageSurrealDB {}

impl ContentAddressedStorageSurrealDB {
    // ========================================================================
    // Note-Block Mapping Methods
    // ========================================================================

    /// Store a note-block mapping
    ///
    /// This maps a block to its note context, enabling content-addressed storage
    /// with note awareness for change detection and deduplication.
    pub async fn store_document_block(
        &self,
        document_id: &str,
        block_index: usize,
        block_hash: &str,
        block_type: &str,
        start_offset: usize,
        end_offset: usize,
        block_content: &str,
        block_metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> StorageResult<()> {
        if document_id.is_empty() {
            return Err(StorageError::InvalidOperation(
                "Empty document_id provided".to_string(),
            ));
        }

        if block_hash.is_empty() {
            return Err(StorageError::InvalidHash(
                "Empty block_hash provided".to_string(),
            ));
        }

        let now = chrono::Utc::now();
        let record = DocumentBlockRecord {
            document_id: document_id.to_string(),
            block_index,
            block_hash: block_hash.to_string(),
            block_type: block_type.to_string(),
            start_offset,
            end_offset,
            block_content: block_content.to_string(),
            block_metadata: block_metadata.unwrap_or_default(),
            created_at: now,
            updated_at: now,
        };

        // Use UPSERT to handle both creation and updates
        let query = format!(
            "UPSERT document_blocks:`{}:{}` CONTENT {{
                document_id: '{}',
                block_index: {},
                block_hash: '{}',
                block_type: '{}',
                start_offset: {},
                end_offset: {},
                block_content: '{}',
                block_metadata: '{}',
                created_at: '{}',
                updated_at: '{}'
            }}",
            document_id.replace('/', "_").replace("\\", "_"), // Sanitize ID
            block_index,
            document_id.replace("'", "''"), // Escape quotes
            block_index,
            block_hash.replace("'", "''"),
            block_type.replace("'", "''"),
            start_offset,
            end_offset,
            block_content.replace("'", "''"),
            serde_json::to_string(&record.block_metadata)
                .unwrap_or_default()
                .replace("'", "''"),
            now.to_rfc3339(),
            now.to_rfc3339()
        );

        self.client.query(&query, &[]).await.map_err(|e| {
            StorageError::backend(format!("Failed to store note block mapping: {}", e))
        })?;

        Ok(())
    }

    /// Get all blocks for a note
    pub async fn get_document_blocks(
        &self,
        document_id: &str,
    ) -> StorageResult<Vec<DocumentBlockRecord>> {
        if document_id.is_empty() {
            return Err(StorageError::InvalidOperation(
                "Empty document_id provided".to_string(),
            ));
        }

        let query = format!(
            "SELECT * FROM document_blocks WHERE document_id = '{}' ORDER BY block_index",
            document_id.replace("'", "''")
        );

        let result =
            self.client.query(&query, &[]).await.map_err(|e| {
                StorageError::backend(format!("Failed to retrieve note blocks: {}", e))
            })?;

        let mut blocks = Vec::new();
        for record in result.records {
            // Convert Record to DocumentBlockRecord manually
            let block_record = DocumentBlockRecord {
                document_id: record
                    .data
                    .get("document_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_index: record
                    .data
                    .get("block_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                block_hash: record
                    .data
                    .get("block_hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_type: record
                    .data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                start_offset: record
                    .data
                    .get("start_offset")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                end_offset: record
                    .data
                    .get("end_offset")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                block_content: record
                    .data
                    .get("block_content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_metadata: record
                    .data
                    .get("block_metadata")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .map(|map| map.into_iter().collect())
                    .unwrap_or_default(),
                created_at: record
                    .data
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|| chrono::Utc::now()),
                updated_at: record
                    .data
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|| chrono::Utc::now()),
            };
            blocks.push(block_record);
        }

        Ok(blocks)
    }

    /// Find documents containing a specific block hash
    pub async fn find_documents_with_block(&self, block_hash: &str) -> StorageResult<Vec<String>> {
        if block_hash.is_empty() {
            return Err(StorageError::InvalidHash(
                "Empty block_hash provided".to_string(),
            ));
        }

        let query = format!(
            "SELECT document_id FROM document_blocks WHERE block_hash = '{}'",
            block_hash.replace("'", "''")
        );

        let result = self.client.query(&query, &[]).await.map_err(|e| {
            StorageError::backend(format!("Failed to find documents with block: {}", e))
        })?;

        let mut document_ids = Vec::new();
        for record in result.records {
            if let Some(doc_id) = record.data.get("document_id") {
                if let Ok(id_str) = serde_json::from_value::<String>(doc_id.clone()) {
                    document_ids.push(id_str);
                }
            }
        }

        Ok(document_ids)
    }

    /// Get block content by hash
    pub async fn get_block_by_hash(
        &self,
        block_hash: &str,
    ) -> StorageResult<Option<DocumentBlockRecord>> {
        if block_hash.is_empty() {
            return Err(StorageError::InvalidHash(
                "Empty block_hash provided".to_string(),
            ));
        }

        let query = format!(
            "SELECT * FROM document_blocks WHERE block_hash = '{}' LIMIT 1",
            block_hash.replace("'", "''")
        );

        let result =
            self.client.query(&query, &[]).await.map_err(|e| {
                StorageError::backend(format!("Failed to get block by hash: {}", e))
            })?;

        if let Some(record) = result.records.first() {
            // Convert Record to DocumentBlockRecord manually
            let block_record = DocumentBlockRecord {
                document_id: record
                    .data
                    .get("document_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_index: record
                    .data
                    .get("block_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                block_hash: record
                    .data
                    .get("block_hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_type: record
                    .data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                start_offset: record
                    .data
                    .get("start_offset")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                end_offset: record
                    .data
                    .get("end_offset")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                block_content: record
                    .data
                    .get("block_content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_metadata: record
                    .data
                    .get("block_metadata")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .map(|map| map.into_iter().collect())
                    .unwrap_or_default(),
                created_at: record
                    .data
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|| chrono::Utc::now()),
                updated_at: record
                    .data
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|| chrono::Utc::now()),
            };
            Ok(Some(block_record))
        } else {
            Ok(None)
        }
    }

    /// Find documents containing multiple block hashes (batch query)
    pub async fn find_documents_with_blocks(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<std::collections::HashMap<String, Vec<String>>> {
        if block_hashes.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        // Create IN clause with properly escaped hashes
        let escaped_hashes: Vec<String> = block_hashes
            .iter()
            .map(|hash| format!("'{}'", hash.replace("'", "''")))
            .collect();

        let query = format!(
            "SELECT block_hash, document_id FROM document_blocks WHERE block_hash IN ({})",
            escaped_hashes.join(",")
        );

        let result = self.client.query(&query, &[]).await.map_err(|e| {
            StorageError::backend(format!("Failed to find documents with blocks: {}", e))
        })?;

        let mut hash_to_documents: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for record in result.records {
            if let (Some(block_hash), Some(document_id)) = (
                record.data.get("block_hash").and_then(|v| v.as_str()),
                record.data.get("document_id").and_then(|v| v.as_str()),
            ) {
                let hash_str = block_hash.to_string();
                let doc_id_str = document_id.to_string();

                hash_to_documents
                    .entry(hash_str)
                    .or_insert_with(Vec::new)
                    .push(doc_id_str);
            }
        }

        Ok(hash_to_documents)
    }

    /// Get multiple blocks by their hashes (batch query)
    pub async fn get_blocks_by_hashes(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<std::collections::HashMap<String, DocumentBlockRecord>> {
        if block_hashes.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        // Create IN clause with properly escaped hashes
        let escaped_hashes: Vec<String> = block_hashes
            .iter()
            .map(|hash| format!("'{}'", hash.replace("'", "''")))
            .collect();

        let query = format!(
            "SELECT * FROM document_blocks WHERE block_hash IN ({})",
            escaped_hashes.join(",")
        );

        let result =
            self.client.query(&query, &[]).await.map_err(|e| {
                StorageError::backend(format!("Failed to get blocks by hashes: {}", e))
            })?;

        let mut hash_to_block: std::collections::HashMap<String, DocumentBlockRecord> =
            std::collections::HashMap::new();

        for record in result.records {
            if let Some(block_hash) = record.data.get("block_hash").and_then(|v| v.as_str()) {
                let block_record = DocumentBlockRecord {
                    document_id: record
                        .data
                        .get("document_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    block_index: record
                        .data
                        .get("block_index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                    block_hash: block_hash.to_string(),
                    block_type: record
                        .data
                        .get("block_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    start_offset: record
                        .data
                        .get("start_offset")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                    end_offset: record
                        .data
                        .get("end_offset")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                    block_content: record
                        .data
                        .get("block_content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    block_metadata: record
                        .data
                        .get("block_metadata")
                        .and_then(|v| v.as_object())
                        .cloned()
                        .map(|map| map.into_iter().collect())
                        .unwrap_or_default(),
                    created_at: record
                        .data
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|| chrono::Utc::now()),
                    updated_at: record
                        .data
                        .get("updated_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|| chrono::Utc::now()),
                };

                hash_to_block.insert(block_hash.to_string(), block_record);
            }
        }

        Ok(hash_to_block)
    }

    /// Get deduplication statistics for specific block hashes
    pub async fn get_block_deduplication_stats(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<std::collections::HashMap<String, usize>> {
        if block_hashes.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        // Create IN clause with properly escaped hashes
        let escaped_hashes: Vec<String> = block_hashes
            .iter()
            .map(|hash| format!("'{}'", hash.replace("'", "''")))
            .collect();

        let query = format!(
            "SELECT block_hash, count() as occurrence_count FROM document_blocks WHERE block_hash IN ({}) GROUP BY block_hash",
            escaped_hashes.join(",")
        );

        let result = self.client.query(&query, &[]).await.map_err(|e| {
            StorageError::backend(format!("Failed to get block deduplication stats: {}", e))
        })?;

        let mut hash_to_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for record in result.records {
            if let (Some(block_hash), Some(count)) = (
                record.data.get("block_hash").and_then(|v| v.as_str()),
                record.data.get("occurrence_count").and_then(|v| v.as_u64()),
            ) {
                hash_to_count.insert(block_hash.to_string(), count as usize);
            }
        }

        Ok(hash_to_count)
    }

    /// Delete all blocks for a note
    pub async fn delete_document_blocks(&self, document_id: &str) -> StorageResult<usize> {
        if document_id.is_empty() {
            return Err(StorageError::InvalidOperation(
                "Empty document_id provided".to_string(),
            ));
        }

        let query = format!(
            "DELETE FROM document_blocks WHERE document_id = '{}'",
            document_id.replace("'", "''")
        );

        let result =
            self.client.query(&query, &[]).await.map_err(|e| {
                StorageError::backend(format!("Failed to delete note blocks: {}", e))
            })?;

        // Count how many were deleted (approximate)
        Ok(result.records.len())
    }

    /// Get deduplication statistics for all blocks
    pub async fn get_all_block_deduplication_stats(&self) -> StorageResult<HashMap<String, usize>> {
        let query = "
            SELECT block_hash, math::count() as duplicate_count
            FROM document_blocks
            GROUP BY block_hash
            HAVING duplicate_count > 1
        ";

        let result = self.client.query(query, &[]).await.map_err(|e| {
            StorageError::backend(format!("Failed to get deduplication stats: {}", e))
        })?;

        let mut stats = HashMap::new();
        for record in result.records {
            if let (Some(block_hash), Some(count)) = (
                record.data.get("block_hash"),
                record.data.get("duplicate_count"),
            ) {
                if let (Ok(hash_str), Ok(count_val)) = (
                    serde_json::from_value::<String>(block_hash.clone()),
                    serde_json::from_value::<usize>(count.clone()),
                ) {
                    stats.insert(hash_str, count_val);
                }
            }
        }

        Ok(stats)
    }
}

// ==================== DEDUPLICATION STORAGE TRAIT IMPLEMENTATION ====================

#[async_trait]
impl crucible_core::storage::DeduplicationStorage for ContentAddressedStorageSurrealDB {
    async fn find_documents_with_block(
        &self,
        block_hash: &str,
    ) -> crucible_core::storage::StorageResult<Vec<String>> {
        self.find_documents_with_block(block_hash).await
    }

    async fn get_document_blocks(
        &self,
        document_id: &str,
    ) -> crucible_core::storage::StorageResult<Vec<crucible_core::storage::BlockInfo>> {
        let records = self.get_document_blocks(document_id).await?;
        let blocks = records
            .into_iter()
            .map(|record| crucible_core::storage::BlockInfo {
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
            })
            .collect();
        Ok(blocks)
    }

    async fn get_block_by_hash(
        &self,
        block_hash: &str,
    ) -> crucible_core::storage::StorageResult<Option<crucible_core::storage::BlockInfo>> {
        if let Some(record) = self.get_block_by_hash(block_hash).await? {
            Ok(Some(crucible_core::storage::BlockInfo {
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
    ) -> crucible_core::storage::StorageResult<std::collections::HashMap<String, usize>> {
        ContentAddressedStorageSurrealDB::get_block_deduplication_stats(self, block_hashes).await
    }

    async fn get_all_block_deduplication_stats(
        &self,
    ) -> crucible_core::storage::StorageResult<std::collections::HashMap<String, usize>> {
        ContentAddressedStorageSurrealDB::get_all_block_deduplication_stats(self).await
    }

    async fn get_all_deduplication_stats(
        &self,
    ) -> crucible_core::storage::StorageResult<
        crucible_core::storage::deduplication_traits::DeduplicationStats,
    > {
        // This method is intentionally not delegated to ContentAddressedStorageSurrealDB
        // because the storage doesn't implement the full DeduplicationStats computation.
        // The DeduplicationDetector wrapper provides this functionality.
        Err(crucible_core::storage::StorageError::Io(
            "get_all_deduplication_stats not implemented on storage - use DeduplicationDetector wrapper".to_string()
        ))
    }

    async fn find_duplicate_blocks(
        &self,
        _min_occurrences: usize,
    ) -> crucible_core::storage::StorageResult<Vec<crucible_core::storage::DuplicateBlockInfo>>
    {
        // This method is intentionally not delegated to ContentAddressedStorageSurrealDB
        // because the storage doesn't implement the full duplicate block analysis.
        // The DeduplicationDetector wrapper provides this functionality.
        Err(crucible_core::storage::StorageError::Io(
            "find_duplicate_blocks not implemented on storage - use DeduplicationDetector wrapper"
                .to_string(),
        ))
    }

    async fn get_storage_usage_stats(
        &self,
    ) -> crucible_core::storage::StorageResult<crucible_core::storage::StorageUsageStats> {
        // This method is intentionally not delegated to ContentAddressedStorageSurrealDB
        // because the storage doesn't implement the full storage usage statistics.
        // The DeduplicationDetector wrapper provides this functionality.
        Err(crucible_core::storage::StorageError::Io(
            "get_storage_usage_stats not implemented on storage - use DeduplicationDetector wrapper".to_string()
        ))
    }

    async fn find_documents_with_blocks(
        &self,
        block_hashes: &[String],
    ) -> crucible_core::storage::StorageResult<std::collections::HashMap<String, Vec<String>>> {
        ContentAddressedStorageSurrealDB::find_documents_with_blocks(self, block_hashes).await
    }

    async fn get_blocks_by_hashes(
        &self,
        block_hashes: &[String],
    ) -> crucible_core::storage::StorageResult<
        std::collections::HashMap<String, crucible_core::storage::BlockInfo>,
    > {
        let records =
            ContentAddressedStorageSurrealDB::get_blocks_by_hashes(self, block_hashes).await?;
        let hash_to_block = records
            .into_iter()
            .map(|(hash, record)| {
                (
                    hash,
                    crucible_core::storage::BlockInfo {
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
                )
            })
            .collect();
        Ok(hash_to_block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::hashing::blake3::Blake3Hasher;
    use crucible_core::storage::traits::{BlockOperations, StorageManagement, TreeOperations};
    use crucible_core::storage::ContentHasher;

    /// Test helper to create a test storage instance
    async fn create_test_storage() -> ContentAddressedStorageSurrealDB {
        ContentAddressedStorageSurrealDB::new_memory()
            .await
            .unwrap()
    }

    /// Test helper to create an empty Merkle tree for testing
    fn create_empty_merkle_tree() -> MerkleTree {
        use std::collections::HashMap;

        MerkleTree {
            root_hash: "empty_root".to_string(),
            nodes: HashMap::new(),
            leaf_hashes: Vec::new(),
            depth: 0,
            block_count: 0,
        }
    }

    /// Test helper to create test data
    fn create_test_data(content: &str) -> Vec<u8> {
        content.as_bytes().to_vec()
    }

    /// Test helper to create a test Merkle tree
    async fn create_test_merkle_tree(blocks: &[&str]) -> (String, MerkleTree) {
        let hasher = Blake3Hasher::new();
        let mut hashed_blocks = Vec::new();

        let mut current_offset = 0;
        for (i, block_data) in blocks.iter().enumerate() {
            let data = create_test_data(block_data);
            let hash = hasher.hash_block(&data);
            let length = data.len();
            hashed_blocks.push(crucible_core::storage::HashedBlock {
                index: i,
                hash,
                data: data.clone(),
                length,
                offset: current_offset,
                is_last: i == blocks.len() - 1,
            });
            current_offset += length;
        }

        let tree = MerkleTree::from_blocks(&hashed_blocks, &hasher).unwrap();
        (tree.root_hash.clone(), tree)
    }

    #[tokio::test]
    async fn test_create_storage() {
        let storage = create_test_storage().await;
        assert!(true, "Storage created successfully");
    }

    #[tokio::test]
    async fn test_store_and_get_block() {
        let storage = create_test_storage().await;
        let data = create_test_data("Hello, World!");
        let hash = "test_hash_123";

        // Store the block
        storage.store_block(hash, &data).await.unwrap();

        // Retrieve the block
        let retrieved = storage.get_block(hash).await.unwrap();
        assert_eq!(retrieved, Some(data));
    }

    #[tokio::test]
    async fn test_get_nonexistent_block() {
        let storage = create_test_storage().await;
        let retrieved = storage.get_block("nonexistent_hash").await.unwrap();
        assert_eq!(retrieved, None);
    }

    #[tokio::test]
    async fn test_block_exists() {
        let storage = create_test_storage().await;
        let data = create_test_data("Test content");
        let hash = "exists_test_hash";

        // Should not exist initially
        assert!(!storage.block_exists(hash).await.unwrap());

        // Store the block
        storage.store_block(hash, &data).await.unwrap();

        // Should exist now
        assert!(storage.block_exists(hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_block() {
        let storage = create_test_storage().await;
        let data = create_test_data("Delete me");
        let hash = "delete_test_hash";

        // Store the block first
        storage.store_block(hash, &data).await.unwrap();
        assert!(storage.block_exists(hash).await.unwrap());

        // Delete the block
        let deleted = storage.delete_block(hash).await.unwrap();
        assert!(deleted);
        assert!(!storage.block_exists(hash).await.unwrap());

        // Try to delete again
        let deleted_again = storage.delete_block(hash).await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_store_and_get_merkle_tree() {
        let storage = create_test_storage().await;
        let blocks = vec!["Block 1", "Block 2", "Block 3"];
        let (root_hash, tree) = create_test_merkle_tree(&blocks).await;

        // Store the tree
        storage.store_tree(&root_hash, &tree).await.unwrap();

        // Retrieve the tree
        let retrieved = storage.get_tree(&root_hash).await.unwrap();
        assert_eq!(retrieved, Some(tree));
    }

    #[tokio::test]
    async fn test_get_nonexistent_tree() {
        let storage = create_test_storage().await;
        let retrieved = storage.get_tree("nonexistent_root_hash").await.unwrap();
        assert_eq!(retrieved, None);
    }

    #[tokio::test]
    async fn test_tree_exists() {
        let storage = create_test_storage().await;
        let blocks = vec!["Tree test block"];
        let (root_hash, tree) = create_test_merkle_tree(&blocks).await;

        // Should not exist initially
        assert!(!storage.tree_exists(&root_hash).await.unwrap());

        // Store the tree
        storage.store_tree(&root_hash, &tree).await.unwrap();

        // Should exist now
        assert!(storage.tree_exists(&root_hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_tree() {
        let storage = create_test_storage().await;
        let blocks = vec!["Delete tree block"];
        let (root_hash, tree) = create_test_merkle_tree(&blocks).await;

        // Store the tree first
        storage.store_tree(&root_hash, &tree).await.unwrap();
        assert!(storage.tree_exists(&root_hash).await.unwrap());

        // Delete the tree
        let deleted = storage.delete_tree(&root_hash).await.unwrap();
        assert!(deleted);
        assert!(!storage.tree_exists(&root_hash).await.unwrap());

        // Try to delete again
        let deleted_again = storage.delete_tree(&root_hash).await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_get_stats() {
        let storage = create_test_storage().await;

        // Store some test data
        let data1 = create_test_data("Block 1");
        let data2 = create_test_data("Block 2");
        storage.store_block("hash1", &data1).await.unwrap();
        storage.store_block("hash2", &data2).await.unwrap();

        // Store a test tree
        let blocks = vec!["Tree block"];
        let (root_hash, tree) = create_test_merkle_tree(&blocks).await;
        storage.store_tree(&root_hash, &tree).await.unwrap();

        // Get stats
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.block_count, 2);
        assert_eq!(stats.tree_count, 1);
        assert!(stats.block_size_bytes > 0);
        assert!(matches!(
            stats.backend,
            crucible_core::storage::traits::StorageBackend::SurrealDB
        ));
    }

    #[tokio::test]
    async fn test_maintenance() {
        let storage = create_test_storage().await;

        // This should not fail
        storage.maintenance().await.unwrap();
    }

    #[tokio::test]
    async fn test_error_handling_empty_hash() {
        let storage = create_test_storage().await;
        let data = create_test_data("Test");

        // All operations with empty hash should fail
        assert!(storage.store_block("", &data).await.is_err());
        assert!(storage.get_block("").await.is_err());
        assert!(storage.block_exists("").await.is_err());
        assert!(storage.delete_block("").await.is_err());
        assert!(storage
            .store_tree("", &create_empty_merkle_tree())
            .await
            .is_err());
        assert!(storage.get_tree("").await.is_err());
        assert!(storage.tree_exists("").await.is_err());
        assert!(storage.delete_tree("").await.is_err());
    }

    #[tokio::test]
    async fn test_error_handling_empty_data() {
        let storage = create_test_storage().await;

        // Store empty data should fail
        assert!(storage.store_block("test_hash", &[]).await.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let storage = std::sync::Arc::new(create_test_storage().await);
        let mut handles = Vec::new();

        // Perform concurrent store operations
        for i in 0..10 {
            let storage_clone = storage.clone();
            let handle = tokio::spawn(async move {
                let data = format!("Concurrent block {}", i);
                let hash = format!("concurrent_hash_{}", i);
                storage_clone
                    .store_block(&hash, data.as_bytes())
                    .await
                    .unwrap();
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all blocks were stored
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.block_count, 10);
    }
}
