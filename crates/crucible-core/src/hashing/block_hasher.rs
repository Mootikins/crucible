//! AST Block hashing implementation using generic hashing algorithms
//!
//! This module provides a specialized implementation for hashing AST blocks extracted
//! from markdown documents. It now uses a generic HashingAlgorithm trait for flexibility
//! and supports pluggable hash algorithms (BLAKE3, SHA256, etc.).
//!
//! # Architecture
//!
//! The `BlockHasher` is designed to be:
//! - **Content-Aware**: Serializes both content and metadata for comprehensive hashing
//! - **Algorithm-Agnostic**: Uses generic HashingAlgorithm trait (OCP compliant)
//! - **Batch-Optimized**: Supports efficient batch processing of multiple blocks
//! - **Metadata-Inclusive**: Includes block type and metadata in hash computation
//!
//! # Block Serialization
//!
//! Blocks are serialized using a structured format that includes:
//! - Block type (heading, paragraph, code, etc.)
//! - Block content text
//! - Block metadata (level, language, etc.)
//! - Position information for context

use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;

use crate::hashing::algorithm::HashingAlgorithm;
use crate::hashing::ast_converter::ASTBlockConverter;
use crate::storage::{ContentHasher as StorageContentHasher, HashedBlock, MerkleTree};
use crate::traits::change_detection::ContentHasher;
use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

// Import AST block types from the parser crate
use crate::parser::types::{ASTBlock, ASTBlockMetadata};

#[cfg(test)]
use crate::parser::types::ASTBlockType;

/// Maximum serialization buffer size for block content
const MAX_SERIALIZATION_SIZE: usize = 10 * 1024 * 1024; // 10MB

/// Implementation of the ContentHasher trait for AST block operations
///
/// This struct provides efficient AST block hashing using a pluggable hashing algorithm.
/// It serializes block content and metadata in a deterministic way to ensure
/// consistent hashes across different parsing sessions.
///
/// # Generic Parameters
///
/// * `A` - The hashing algorithm to use (implements `HashingAlgorithm`)
///
/// # Performance Characteristics
///
/// - **BLAKE3**: ~10-20 MB/s on typical block sizes
/// - **SHA256**: ~5-10 MB/s on typical block sizes
/// - **Memory Usage**: O(block_size) for serialization + hash state
/// - **Parallel Processing**: Batch operations use futures for concurrent hashing
///
/// # Serialization Format
///
/// Blocks are serialized to JSON with the following structure:
/// ```json
/// {
///   "type": "heading|paragraph|code|...",
///   "content": "block content text",
///   "metadata": { ... type-specific metadata ... },
///   "start_offset": 0,
///   "end_offset": 100
/// }
/// ```
///
/// # Thread Safety
///
/// The `BlockHasher` is `Send + Sync` and can be safely shared across threads.
/// All operations are async and non-blocking.
#[derive(Debug, Clone)]
pub struct BlockHasher<A: HashingAlgorithm> {
    algorithm: A,
    /// Legacy algorithm enum for compatibility (will be removed in future)
    legacy_algorithm: HashAlgorithm,
    /// Converter for transforming AST blocks to HashedBlock format
    converter: ASTBlockConverter<A>,
}

impl<A: HashingAlgorithm> BlockHasher<A> {
    /// Create a new BlockHasher with the specified hashing algorithm
    ///
    /// # Arguments
    ///
    /// * `algorithm` - The hash algorithm implementation to use
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::hashing::block_hasher::BlockHasher;
    /// use crucible_core::hashing::algorithm::Blake3Algorithm;
    ///
    /// let hasher = BlockHasher::new(Blake3Algorithm);
    /// ```
    pub fn new(algorithm: A) -> Self {
        // Map algorithm name to legacy enum for backwards compatibility
        // Use case-insensitive comparison to handle both "blake3" and "BLAKE3"
        let name = algorithm.algorithm_name();
        let legacy_algorithm = if name.eq_ignore_ascii_case("blake3") {
            HashAlgorithm::Blake3
        } else if name.eq_ignore_ascii_case("sha256") {
            HashAlgorithm::Sha256
        } else {
            HashAlgorithm::Blake3 // Default fallback
        };

        // Create converter with the same algorithm
        let converter = ASTBlockConverter::new(algorithm.clone());

        Self {
            algorithm,
            legacy_algorithm,
            converter,
        }
    }

    /// Serialize an AST block to a deterministic string format
    ///
    /// This method converts the block to a structured JSON representation
    /// that includes all relevant information for hashing.
    ///
    /// # Arguments
    ///
    /// * `block` - The AST block to serialize
    ///
    /// # Returns
    ///
    /// Serialized block string or error if serialization fails
    fn serialize_block(&self, block: &ASTBlock) -> Result<String, HashError> {
        // Create a serializable representation of the block
        let serializable = SerializableBlock::from_ast_block(block);

        // Serialize to JSON with stable ordering
        let serialized = serde_json::to_string(&serializable).map_err(|e| HashError::IoError {
            error: format!("Failed to serialize block: {}", e),
        })?;

        // Check size limits to prevent memory issues
        if serialized.len() > MAX_SERIALIZATION_SIZE {
            return Err(HashError::IoError {
                error: format!("Block serialization too large: {} bytes", serialized.len()),
            });
        }

        Ok(serialized)
    }

    /// Compute hash of serialized block content using the generic algorithm
    async fn hash_serialized_content(&self, content: &str) -> Result<Vec<u8>, HashError> {
        // Use the generic algorithm trait
        Ok(self.algorithm.hash(content.as_bytes()))
    }

    /// Hash a single AST block with metadata
    ///
    /// This is the primary method for hashing individual blocks. It includes
    /// both content and metadata in the hash computation.
    ///
    /// # Arguments
    ///
    /// * `block` - The AST block to hash
    ///
    /// # Returns
    ///
    /// The block hash
    ///
    /// # Errors
    ///
    /// Returns `HashError` if serialization or hashing fails
    pub async fn hash_ast_block(&self, block: &ASTBlock) -> Result<BlockHash, HashError> {
        let serialized = self.serialize_block(block)?;
        let hash_bytes = self.hash_serialized_content(&serialized).await?;

        if hash_bytes.len() != 32 {
            return Err(HashError::InvalidLength {
                len: hash_bytes.len(),
            });
        }

        let mut array = [0u8; 32];
        array.copy_from_slice(&hash_bytes);
        Ok(BlockHash::new(array))
    }

    /// Hash multiple AST blocks in parallel
    ///
    /// This method efficiently processes multiple blocks using concurrent hashing
    /// for better performance on multi-core systems.
    ///
    /// # Arguments
    ///
    /// * `blocks` - Vector of AST blocks to hash
    ///
    /// # Returns
    ///
    /// Vector of block hashes in the same order as input
    ///
    /// # Errors
    ///
    /// Returns `HashError` if any block hashing fails
    pub async fn hash_ast_blocks_batch(
        &self,
        blocks: &[ASTBlock],
    ) -> Result<Vec<BlockHash>, HashError> {
        let mut results = Vec::with_capacity(blocks.len());

        // Process blocks concurrently for better performance
        let futures: Vec<_> = blocks
            .iter()
            .map(|block| self.hash_ast_block(block))
            .collect();
        let hash_results = futures::future::join_all(futures).await;

        for result in hash_results {
            results.push(result?);
        }

        Ok(results)
    }

    /// Create comprehensive block hash info with metadata
    ///
    /// This method hashes a block and includes important metadata for
    /// change detection and block-level operations.
    ///
    /// # Arguments
    ///
    /// * `block` - The AST block to hash
    ///
    /// # Returns
    ///
    /// Complete block hash information
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    pub async fn hash_ast_block_info(&self, block: &ASTBlock) -> Result<BlockHashInfo, HashError> {
        let content_hash = self.hash_ast_block(block).await?;
        let block_type = block.type_name().to_string();

        Ok(BlockHashInfo::new(
            content_hash,
            block_type,
            block.start_offset,
            block.end_offset,
            self.legacy_algorithm,
        ))
    }

    /// Verify that a block's hash matches the expected value
    ///
    /// This method is useful for integrity checking and validation.
    ///
    /// # Arguments
    ///
    /// * `block` - The AST block to verify
    /// * `expected_hash` - Expected hash value
    ///
    /// # Returns
    ///
    /// `true` if the hash matches, `false` otherwise
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    pub async fn verify_ast_block_hash(
        &self,
        block: &ASTBlock,
        expected_hash: &BlockHash,
    ) -> Result<bool, HashError> {
        match self.hash_ast_block(block).await {
            Ok(actual_hash) => Ok(actual_hash == *expected_hash),
            Err(_) => Ok(false),
        }
    }

    /// Get hash statistics for a batch of blocks
    ///
    /// This method provides useful statistics about hash computation
    /// for performance monitoring and optimization.
    ///
    /// # Arguments
    ///
    /// * `blocks` - Vector of AST blocks to analyze
    ///
    /// # Returns
    ///
    /// Statistics about the block batch
    pub fn analyze_batch(&self, blocks: &[ASTBlock]) -> BlockHashStats {
        let mut stats = BlockHashStats::new();

        for block in blocks {
            stats.total_blocks += 1;
            stats.total_content_chars += block.content.len();
            stats.total_span_chars += block.length();

            // Track block types
            let type_name = block.type_name();
            *stats
                .block_type_counts
                .entry(type_name.to_string())
                .or_insert(0) += 1;

            // Track empty blocks
            if block.is_empty() {
                stats.empty_blocks += 1;
            }
        }

        stats
    }

    // ==================== MERKLE TREE CONSTRUCTION METHODS ====================

    /// Convert AST blocks to HashedBlock format for Merkle tree construction
    ///
    /// This method delegates to the `ASTBlockConverter` to convert AST blocks
    /// to the HashedBlock format expected by the MerkleTree implementation.
    /// This follows the Single Responsibility Principle by separating hashing
    /// concerns from conversion concerns.
    ///
    /// # Arguments
    ///
    /// * `blocks` - Vector of AST blocks to convert
    ///
    /// # Returns
    ///
    /// Vector of HashedBlock instances or error if conversion fails
    ///
    /// # Errors
    ///
    /// Returns `HashError` if block conversion fails
    ///
    /// # Design Note
    ///
    /// This method now uses `ASTBlockConverter` internally, following SRP.
    /// The converter handles all conversion logic, while BlockHasher focuses
    /// on hashing operations.
    pub async fn ast_blocks_to_hashed_blocks(
        &self,
        blocks: &[ASTBlock],
    ) -> Result<Vec<HashedBlock>, HashError> {
        // Delegate to the converter for AST block conversion
        self.converter.convert_batch(blocks).await
    }

    /// Build a Merkle tree from a collection of AST blocks
    ///
    /// This is the primary method for creating Merkle trees from parsed AST blocks.
    /// It hashes the blocks, converts them to the appropriate format, and constructs
    /// a binary Merkle tree for efficient change detection.
    ///
    /// # Arguments
    ///
    /// * `blocks` - Vector of AST blocks to include in the tree
    ///
    /// # Returns
    ///
    /// Constructed Merkle tree or error if tree creation fails
    ///
    /// # Errors
    ///
    /// Returns `HashError` if block hashing or tree construction fails
    pub async fn build_merkle_tree_from_blocks(
        &self,
        blocks: &[ASTBlock],
    ) -> Result<MerkleTree, HashError> {
        if blocks.is_empty() {
            return Err(HashError::IoError {
                error: "Cannot build Merkle tree from empty block list".to_string(),
            });
        }

        // Convert AST blocks to HashedBlock format
        let hashed_blocks = self.ast_blocks_to_hashed_blocks(blocks).await?;

        // Build Merkle tree using this BlockHasher as the ContentHasher
        let tree =
            MerkleTree::from_blocks(&hashed_blocks, self).map_err(|e| HashError::IoError {
                error: format!("Failed to build Merkle tree: {}", e),
            })?;

        Ok(tree)
    }

    /// Build a Merkle tree from pre-computed block hashes
    ///
    /// This method is useful when you already have block hashes and want
    /// to construct a Merkle tree without re-hashing the content.
    ///
    /// # Arguments
    ///
    /// * `block_hashes` - Vector of block hashes with their indices
    ///
    /// # Returns
    ///
    /// Constructed Merkle tree or error if tree creation fails
    ///
    /// # Errors
    ///
    /// Returns `HashError` if tree construction fails
    pub async fn build_merkle_tree_from_hashes(
        &self,
        block_hashes: &[(usize, BlockHash)],
    ) -> Result<MerkleTree, HashError> {
        if block_hashes.is_empty() {
            return Err(HashError::IoError {
                error: "Cannot build Merkle tree from empty hash list".to_string(),
            });
        }

        // Convert to HashedBlock format
        let hashed_blocks: Result<Vec<HashedBlock>, _> = block_hashes
            .iter()
            .enumerate()
            .map(|(array_index, &(block_index, block_hash))| {
                Ok(HashedBlock::new(
                    block_hash.to_hex(),
                    format!("block_{}", block_index).into_bytes(), // Placeholder data
                    block_index,
                    0, // Offset not available from hash only
                    array_index == block_hashes.len() - 1,
                ))
            })
            .collect();

        let hashed_blocks = hashed_blocks.map_err(|e: HashError| e)?;

        // Build Merkle tree
        let tree =
            MerkleTree::from_blocks(&hashed_blocks, self).map_err(|e| HashError::IoError {
                error: format!("Failed to build Merkle tree from hashes: {}", e),
            })?;

        Ok(tree)
    }

    /// Compare two Merkle trees and return changes
    ///
    /// This method provides change detection by comparing two Merkle trees
    /// and identifying differences at the block level.
    ///
    /// # Arguments
    ///
    /// * `old_tree` - Previous version of the Merkle tree
    /// * `new_tree` - Current version of the Merkle tree
    ///
    /// # Returns
    ///
    /// List of detected changes
    pub fn compare_merkle_trees(
        &self,
        old_tree: &MerkleTree,
        new_tree: &MerkleTree,
    ) -> Vec<crate::storage::TreeChange> {
        old_tree.compare_with(new_tree)
    }

    /// Verify that a Merkle tree correctly represents the given AST blocks
    ///
    /// This method validates that a Merkle tree accurately represents the
    /// hash values of the provided AST blocks.
    ///
    /// # Arguments
    ///
    /// * `tree` - Merkle tree to verify
    /// * `blocks` - Original AST blocks
    ///
    /// # Returns
    ///
    /// `true` if the tree is valid, `false` otherwise
    pub async fn verify_merkle_tree(
        &self,
        tree: &MerkleTree,
        blocks: &[ASTBlock],
    ) -> Result<bool, HashError> {
        // Verify tree integrity
        if let Err(e) = tree.verify_integrity(self) {
            return Err(HashError::IoError {
                error: format!("Tree integrity verification failed: {}", e),
            });
        }

        // Re-hash blocks and compare with tree leaves
        let hashed_blocks = self.ast_blocks_to_hashed_blocks(blocks).await?;

        // Check if block count matches
        if hashed_blocks.len() != tree.leaf_hashes.len() {
            return Ok(false);
        }

        // Compare each block hash with corresponding leaf
        for (i, hashed_block) in hashed_blocks.iter().enumerate() {
            if let Some(leaf_node) = tree.get_leaf(i) {
                if leaf_node.hash != hashed_block.hash {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Get statistics about Merkle tree construction
    ///
    /// This method provides useful statistics about the tree construction
    /// process for performance monitoring and optimization.
    ///
    /// # Arguments
    ///
    /// * `tree` - The constructed Merkle tree
    ///
    /// # Returns
    ///
    /// Statistics about the tree
    pub fn get_merkle_tree_stats(&self, tree: &MerkleTree) -> MerkleTreeStats {
        MerkleTreeStats {
            root_hash: tree.root_hash.clone(),
            block_count: tree.block_count,
            tree_depth: tree.depth,
            node_count: tree.nodes.len(),
            leaf_count: tree.leaf_hashes.len(),
            algorithm: self.legacy_algorithm,
        }
    }
}

#[async_trait]
impl<A: HashingAlgorithm> ContentHasher for BlockHasher<A> {
    fn algorithm(&self) -> HashAlgorithm {
        self.legacy_algorithm
    }

    async fn hash_file(&self, _path: &std::path::Path) -> Result<FileHash, HashError> {
        // BlockHasher doesn't support file hashing directly
        Err(HashError::IoError {
            error: "BlockHasher doesn't support file hashing".to_string(),
        })
    }

    async fn hash_files_batch(
        &self,
        _paths: &[std::path::PathBuf],
    ) -> Result<Vec<FileHash>, HashError> {
        // BlockHasher doesn't support file hashing directly
        Err(HashError::IoError {
            error: "BlockHasher doesn't support file hashing".to_string(),
        })
    }

    async fn hash_block(&self, content: &str) -> Result<BlockHash, HashError> {
        // Simple content hashing without AST block structure
        let hash_bytes = self.hash_serialized_content(content).await?;

        if hash_bytes.len() != 32 {
            return Err(HashError::InvalidLength {
                len: hash_bytes.len(),
            });
        }

        let mut array = [0u8; 32];
        array.copy_from_slice(&hash_bytes);
        Ok(BlockHash::new(array))
    }

    async fn hash_blocks_batch(&self, contents: &[String]) -> Result<Vec<BlockHash>, HashError> {
        let mut results = Vec::with_capacity(contents.len());

        // Process blocks concurrently
        let futures: Vec<_> = contents
            .iter()
            .map(|content| {
                use crate::traits::change_detection::ContentHasher as CDContentHasher;
                CDContentHasher::hash_block(self, content)
            })
            .collect();
        let hash_results = futures::future::join_all(futures).await;

        for result in hash_results {
            results.push(result?);
        }

        Ok(results)
    }

    async fn hash_file_info(
        &self,
        _path: &std::path::Path,
        _relative_path: String,
    ) -> Result<FileHashInfo, HashError> {
        // BlockHasher doesn't support file hashing directly
        Err(HashError::IoError {
            error: "BlockHasher doesn't support file hashing".to_string(),
        })
    }

    async fn hash_block_info(
        &self,
        content: &str,
        block_type: String,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<BlockHashInfo, HashError> {
        use crate::traits::change_detection::ContentHasher as CDContentHasher;
        let content_hash = CDContentHasher::hash_block(self, content).await?;

        Ok(BlockHashInfo::new(
            content_hash,
            block_type,
            start_offset,
            end_offset,
            self.legacy_algorithm,
        ))
    }

    async fn verify_file_hash(
        &self,
        _path: &std::path::Path,
        _expected_hash: &FileHash,
    ) -> Result<bool, HashError> {
        // BlockHasher doesn't support file hashing directly
        Err(HashError::IoError {
            error: "BlockHasher doesn't support file hashing".to_string(),
        })
    }

    async fn verify_block_hash(
        &self,
        content: &str,
        expected_hash: &BlockHash,
    ) -> Result<bool, HashError> {
        use crate::traits::change_detection::ContentHasher as CDContentHasher;
        match CDContentHasher::hash_block(self, content).await {
            Ok(actual_hash) => Ok(actual_hash == *expected_hash),
            Err(_) => Ok(false),
        }
    }
}

/// Serializable representation of an AST block for consistent hashing
///
/// This struct provides a stable serialization format that includes all
/// relevant block information for computing consistent hashes.
#[derive(Debug, Clone, Serialize)]
struct SerializableBlock {
    /// Block type as string
    pub block_type: String,
    /// Block content text
    pub content: String,
    /// Block-specific metadata
    pub metadata: SerializableMetadata,
    /// Start position in source
    pub start_offset: usize,
    /// End position in source
    pub end_offset: usize,
}

impl SerializableBlock {
    /// Create a serializable block from an AST block
    fn from_ast_block(block: &ASTBlock) -> Self {
        Self {
            block_type: block.type_name().to_string(),
            content: block.content.clone(),
            metadata: SerializableMetadata::from_ast_metadata(&block.metadata),
            start_offset: block.start_offset,
            end_offset: block.end_offset,
        }
    }
}

/// Serializable representation of block metadata
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum SerializableMetadata {
    /// Heading metadata
    Heading { level: u8, id: Option<String> },
    /// Code block metadata
    Code {
        language: Option<String>,
        line_count: usize,
    },
    /// List metadata
    List {
        list_type: String,
        item_count: usize,
    },
    /// Callout metadata
    Callout {
        callout_type: String,
        title: Option<String>,
        is_standard_type: bool,
    },
    /// LaTeX metadata
    Latex { is_block: bool },
    /// Table metadata
    Table {
        rows: usize,
        columns: usize,
        headers: Vec<String>,
    },
    /// Generic metadata
    Generic,
}

impl SerializableMetadata {
    /// Create serializable metadata from AST block metadata
    fn from_ast_metadata(metadata: &ASTBlockMetadata) -> Self {
        match metadata {
            ASTBlockMetadata::Heading { level, id } => Self::Heading {
                level: *level,
                id: id.clone(),
            },
            ASTBlockMetadata::Code {
                language,
                line_count,
            } => Self::Code {
                language: language.clone(),
                line_count: *line_count,
            },
            ASTBlockMetadata::List {
                list_type,
                item_count,
            } => Self::List {
                list_type: format!("{:?}", list_type),
                item_count: *item_count,
            },
            ASTBlockMetadata::Callout {
                callout_type,
                title,
                is_standard_type,
            } => Self::Callout {
                callout_type: callout_type.clone(),
                title: title.clone(),
                is_standard_type: *is_standard_type,
            },
            ASTBlockMetadata::Latex { is_block } => Self::Latex {
                is_block: *is_block,
            },
            ASTBlockMetadata::Table {
                rows,
                columns,
                headers,
            } => Self::Table {
                rows: *rows,
                columns: *columns,
                headers: headers.clone(),
            },
            ASTBlockMetadata::Generic => Self::Generic,
        }
    }
}

// ==================== STORAGE CONTENTHASHER IMPLEMENTATION ====================

impl<A: HashingAlgorithm> StorageContentHasher for BlockHasher<A> {
    fn hash_block(&self, data: &[u8]) -> String {
        // Use the generic algorithm
        let hash_bytes = self.algorithm.hash(data);
        self.algorithm.to_hex(&hash_bytes)
    }

    fn hash_nodes(&self, left: &str, right: &str) -> String {
        // Use the generic algorithm's hash_nodes method
        let left_bytes = self.algorithm.from_hex(left).unwrap_or_default();
        let right_bytes = self.algorithm.from_hex(right).unwrap_or_default();
        let hash_bytes = self.algorithm.hash_nodes(&left_bytes, &right_bytes);
        self.algorithm.to_hex(&hash_bytes)
    }

    fn algorithm_name(&self) -> &'static str {
        self.algorithm.algorithm_name()
    }

    fn hash_length(&self) -> usize {
        self.algorithm.hash_length()
    }
}

/// Statistics about Merkle tree construction
#[derive(Debug, Clone, Default)]
pub struct BlockHashStats {
    /// Total number of blocks in the batch
    pub total_blocks: usize,
    /// Total number of characters in block content
    pub total_content_chars: usize,
    /// Total number of characters spanned by blocks
    pub total_span_chars: usize,
    /// Number of empty blocks (no content)
    pub empty_blocks: usize,
    /// Count of blocks by type
    pub block_type_counts: HashMap<String, usize>,
}

impl BlockHashStats {
    /// Create new empty statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Get average content length per block
    pub fn avg_content_length(&self) -> f64 {
        if self.total_blocks == 0 {
            0.0
        } else {
            self.total_content_chars as f64 / self.total_blocks as f64
        }
    }

    /// Get average span length per block
    pub fn avg_span_length(&self) -> f64 {
        if self.total_blocks == 0 {
            0.0
        } else {
            self.total_span_chars as f64 / self.total_blocks as f64
        }
    }

    /// Get percentage of empty blocks
    pub fn empty_block_percentage(&self) -> f64 {
        if self.total_blocks == 0 {
            0.0
        } else {
            (self.empty_blocks as f64 / self.total_blocks as f64) * 100.0
        }
    }

    /// Get the most common block type
    pub fn most_common_type(&self) -> Option<(String, usize)> {
        self.block_type_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(type_name, &count)| (type_name.clone(), count))
    }

    /// Get a summary string of the statistics
    pub fn summary(&self) -> String {
        let most_common = self
            .most_common_type()
            .map(|(t, c)| format!("{} ({} blocks)", t, c))
            .unwrap_or_else(|| "none".to_string());

        format!(
            "Blocks: {}, Avg content: {:.1} chars, Avg span: {:.1} chars, \
             Empty: {:.1}%, Most common: {}",
            self.total_blocks,
            self.avg_content_length(),
            self.avg_span_length(),
            self.empty_block_percentage(),
            most_common
        )
    }
}

/// Statistics about Merkle tree construction for Phase 2 optimize-data-flow
#[derive(Debug, Clone)]
pub struct MerkleTreeStats {
    /// Root hash of the constructed tree
    pub root_hash: String,
    /// Total number of blocks in the tree
    pub block_count: usize,
    /// Depth of the tree (root to deepest leaf)
    pub tree_depth: usize,
    /// Total number of nodes in the tree
    pub node_count: usize,
    /// Number of leaf nodes (should equal block_count)
    pub leaf_count: usize,
    /// Hash algorithm used for construction
    pub algorithm: HashAlgorithm,
}

impl MerkleTreeStats {
    /// Create a new MerkleTreeStats instance
    pub fn new(
        root_hash: String,
        block_count: usize,
        tree_depth: usize,
        node_count: usize,
        leaf_count: usize,
        algorithm: HashAlgorithm,
    ) -> Self {
        Self {
            root_hash,
            block_count,
            tree_depth,
            node_count,
            leaf_count,
            algorithm,
        }
    }

    /// Get the efficiency ratio of the tree (nodes/blocks)
    pub fn efficiency_ratio(&self) -> f64 {
        if self.block_count == 0 {
            0.0
        } else {
            self.node_count as f64 / self.block_count as f64
        }
    }

    /// Check if the tree is balanced (efficient binary tree)
    pub fn is_balanced(&self) -> bool {
        if self.block_count <= 1 {
            return true;
        }

        // For a balanced binary tree, depth should be close to log2(blocks) + 1
        let expected_depth = (self.block_count as f64).log2().ceil() as usize + 1;
        self.tree_depth <= expected_depth + 1 // Allow some tolerance
    }

    /// Get a summary string of the statistics
    pub fn summary(&self) -> String {
        format!(
            "Merkle Tree: {} blocks, {} depth, {} nodes, {} efficiency, algorithm: {}",
            self.block_count,
            self.tree_depth,
            self.node_count,
            format!("{:.2}", self.efficiency_ratio()),
            self.algorithm
        )
    }
}

/// Type alias for commonly used BLAKE3 block hasher
pub type Blake3BlockHasher = BlockHasher<crate::hashing::algorithm::Blake3Algorithm>;

/// Type alias for commonly used SHA256 block hasher
pub type Sha256BlockHasher = BlockHasher<crate::hashing::algorithm::Sha256Algorithm>;

/// Helper function to create a BLAKE3 block hasher
///
/// This function replaces the previous `BLAKE3_BLOCK_HASHER` constant,
/// which could not be const due to the converter initialization.
///
/// # Examples
///
/// ```rust
/// use crucible_core::hashing::block_hasher::new_blake3_block_hasher;
///
/// let hasher = new_blake3_block_hasher();
/// ```
pub fn new_blake3_block_hasher() -> Blake3BlockHasher {
    BlockHasher::new(crate::hashing::algorithm::Blake3Algorithm)
}

/// Helper function to create a SHA256 block hasher
///
/// This function replaces the previous `SHA256_BLOCK_HASHER` constant,
/// which could not be const due to the converter initialization.
///
/// # Examples
///
/// ```rust
/// use crucible_core::hashing::block_hasher::new_sha256_block_hasher;
///
/// let hasher = new_sha256_block_hasher();
/// ```
pub fn new_sha256_block_hasher() -> Sha256BlockHasher {
    BlockHasher::new(crate::hashing::algorithm::Sha256Algorithm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::types::ListType;

    #[tokio::test]
    async fn test_block_hasher_creation() {
        use crate::hashing::algorithm::{Blake3Algorithm, Sha256Algorithm};

        let hasher = BlockHasher::new(Blake3Algorithm);
        assert_eq!(hasher.algorithm(), HashAlgorithm::Blake3);

        let sha256_hasher = BlockHasher::new(Sha256Algorithm);
        assert_eq!(sha256_hasher.algorithm(), HashAlgorithm::Sha256);
    }

    #[tokio::test]
    async fn test_heading_block_hashing() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let metadata = ASTBlockMetadata::heading(1, Some("test-heading".to_string()));
        let block = ASTBlock::new(
            ASTBlockType::Heading,
            "Test Heading".to_string(),
            0,
            12,
            metadata,
        );

        let hash = hasher.hash_ast_block(&block).await.unwrap();

        // Verify hash is deterministic
        let hash2 = hasher.hash_ast_block(&block).await.unwrap();
        assert_eq!(hash, hash2);

        // Verify hash is non-zero
        assert!(!hash.is_zero());
    }

    #[tokio::test]
    async fn test_code_block_hashing() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let metadata = ASTBlockMetadata::code(Some("rust".to_string()), 3);
        let block = ASTBlock::new(
            ASTBlockType::Code,
            "let x = 42;\nprintln!(\"Hello\");".to_string(),
            20,
            45,
            metadata,
        );

        let hash = hasher.hash_ast_block(&block).await.unwrap();
        assert!(!hash.is_zero());

        // Verify different content produces different hash
        let different_block = ASTBlock::new(
            ASTBlockType::Code,
            "let y = 24;".to_string(),
            20,
            30,
            ASTBlockMetadata::code(Some("rust".to_string()), 1),
        );
        let different_hash = hasher.hash_ast_block(&different_block).await.unwrap();
        assert_ne!(hash, different_hash);
    }

    #[tokio::test]
    async fn test_paragraph_block_hashing() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let metadata = ASTBlockMetadata::generic();
        let block = ASTBlock::new(
            ASTBlockType::Paragraph,
            "This is a test paragraph with some content.".to_string(),
            100,
            144,
            metadata,
        );

        let hash = hasher.hash_ast_block(&block).await.unwrap();
        assert!(!hash.is_zero());

        // Test verification
        let is_valid = hasher.verify_ast_block_hash(&block, &hash).await.unwrap();
        assert!(is_valid);

        // Test verification with wrong hash
        let wrong_hash = BlockHash::new([0u8; 32]);
        let is_invalid = hasher
            .verify_ast_block_hash(&block, &wrong_hash)
            .await
            .unwrap();
        assert!(!is_invalid);
    }

    #[tokio::test]
    async fn test_list_block_hashing() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let metadata = ASTBlockMetadata::list(ListType::Ordered, 3);
        let block = ASTBlock::new(
            ASTBlockType::List,
            "First item\nSecond item\nThird item".to_string(),
            50,
            100,
            metadata,
        );

        let hash = hasher.hash_ast_block(&block).await.unwrap();
        assert!(!hash.is_zero());
    }

    #[tokio::test]
    async fn test_callout_block_hashing() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let metadata =
            ASTBlockMetadata::callout("note".to_string(), Some("Important Note".to_string()), true);
        let block = ASTBlock::new(
            ASTBlockType::Callout,
            "This is an important callout message".to_string(),
            200,
            238,
            metadata,
        );

        let hash = hasher.hash_ast_block(&block).await.unwrap();
        assert!(!hash.is_zero());
    }

    #[tokio::test]
    async fn test_batch_hashing() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Title".to_string(),
                0,
                5,
                ASTBlockMetadata::heading(1, Some("title".to_string())),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "First paragraph".to_string(),
                10,
                26,
                ASTBlockMetadata::generic(),
            ),
            ASTBlock::new(
                ASTBlockType::Code,
                "let x = 1;".to_string(),
                30,
                40,
                ASTBlockMetadata::code(Some("rust".to_string()), 1),
            ),
        ];

        let hashes = hasher.hash_ast_blocks_batch(&blocks).await.unwrap();
        assert_eq!(hashes.len(), 3);

        // Verify all hashes are non-zero and different
        for (i, hash) in hashes.iter().enumerate() {
            assert!(!hash.is_zero(), "Hash {} should not be zero", i);
        }

        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[1], hashes[2]);
        assert_ne!(hashes[0], hashes[2]);
    }

    #[tokio::test]
    async fn test_block_info_hashing() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let metadata = ASTBlockMetadata::heading(2, Some("subtitle".to_string()));
        let block = ASTBlock::new(
            ASTBlockType::Heading,
            "Subtitle".to_string(),
            50,
            58,
            metadata,
        );

        let info = hasher.hash_ast_block_info(&block).await.unwrap();

        assert_eq!(info.block_type, "heading");
        assert_eq!(info.start_offset, 50);
        assert_eq!(info.end_offset, 58);
        assert_eq!(info.content_length(), 8);
        assert_eq!(info.algorithm, HashAlgorithm::Blake3);
        assert!(!info.content_hash.is_zero());
    }

    #[tokio::test]
    async fn test_content_hasher_trait_compatibility() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        // Test simple content hashing
        let content = "Hello, world!";
        use crate::traits::change_detection::ContentHasher as CDContentHasher;
        let hash = CDContentHasher::hash_block(&hasher, content).await.unwrap();
        assert!(!hash.is_zero());

        // Test batch content hashing
        let contents = vec![
            "First content".to_string(),
            "Second content".to_string(),
            "Third content".to_string(),
        ];
        let hashes = hasher.hash_blocks_batch(&contents).await.unwrap();
        assert_eq!(hashes.len(), 3);

        // Test content verification
        let is_valid = hasher.verify_block_hash(content, &hash).await.unwrap();
        assert!(is_valid);

        // Test content hash info
        let info = hasher
            .hash_block_info(content, "test".to_string(), 0, content.len())
            .await
            .unwrap();
        assert_eq!(info.block_type, "test");
        assert_eq!(info.start_offset, 0);
        assert_eq!(info.end_offset, content.len());
        assert_eq!(info.content_hash, hash);
    }

    #[tokio::test]
    async fn test_empty_block_handling() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let metadata = ASTBlockMetadata::generic();
        let empty_block =
            ASTBlock::new(ASTBlockType::Paragraph, "".to_string(), 100, 100, metadata);

        let hash = hasher.hash_ast_block(&empty_block).await.unwrap();

        // Even empty blocks should have non-zero hashes (due to metadata)
        assert!(!hash.is_zero());

        // But empty blocks should be detected as empty
        assert!(empty_block.is_empty());
    }

    #[test]
    fn test_serialization_format() {
        let metadata = ASTBlockMetadata::heading(1, Some("test".to_string()));
        let block = ASTBlock::new(ASTBlockType::Heading, "Test".to_string(), 0, 4, metadata);

        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let serialized = hasher.serialize_block(&block).unwrap();

        // Verify JSON format
        assert!(serialized.contains("\"block_type\":\"heading\""));
        assert!(serialized.contains("\"content\":\"Test\""));
        assert!(serialized.contains("\"type\":\"Heading\""));
        assert!(serialized.contains("\"level\":1"));
        assert!(serialized.contains("\"id\":\"test\""));
        assert!(serialized.contains("\"start_offset\":0"));
        assert!(serialized.contains("\"end_offset\":4"));
    }

    #[test]
    fn test_block_hash_stats() {
        let mut stats = BlockHashStats::new();

        stats.total_blocks = 10;
        stats.total_content_chars = 500;
        stats.total_span_chars = 800;
        stats.empty_blocks = 2;
        stats.block_type_counts.insert("paragraph".to_string(), 5);
        stats.block_type_counts.insert("heading".to_string(), 3);
        stats.block_type_counts.insert("code".to_string(), 2);

        assert_eq!(stats.avg_content_length(), 50.0);
        assert_eq!(stats.avg_span_length(), 80.0);
        assert_eq!(stats.empty_block_percentage(), 20.0);

        let most_common = stats.most_common_type().unwrap();
        assert_eq!(most_common, ("paragraph".to_string(), 5));

        let summary = stats.summary();
        assert!(summary.contains("Blocks: 10"));
        assert!(summary.contains("Avg content: 50.0"));
        assert!(summary.contains("paragraph (5 blocks)"));
    }

    #[test]
    fn test_helper_functions() {
        let blake3_hasher = new_blake3_block_hasher();
        assert_eq!(blake3_hasher.algorithm(), HashAlgorithm::Blake3);

        let sha256_hasher = new_sha256_block_hasher();
        assert_eq!(sha256_hasher.algorithm(), HashAlgorithm::Sha256);
    }

    #[tokio::test]
    async fn test_error_handling() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        // Test file hashing (not supported)
        let result = hasher.hash_file(std::path::Path::new("test.txt")).await;
        assert!(result.is_err());

        // Test file verification (not supported)
        let hash = FileHash::new([1u8; 32]);
        let result = hasher
            .verify_file_hash(std::path::Path::new("test.txt"), &hash)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_algorithm_consistency() {
        use crate::hashing::algorithm::{Blake3Algorithm, Sha256Algorithm};
        let blake3_hasher = BlockHasher::new(Blake3Algorithm);
        let sha256_hasher = BlockHasher::new(Sha256Algorithm);

        let metadata = ASTBlockMetadata::generic();
        let block = ASTBlock::new(
            ASTBlockType::Paragraph,
            "Test content".to_string(),
            0,
            12,
            metadata,
        );

        let blake3_hash = blake3_hasher.hash_ast_block(&block).await.unwrap();
        let sha256_hash = sha256_hasher.hash_ast_block(&block).await.unwrap();

        // Different algorithms should produce different hashes
        assert_ne!(blake3_hash, sha256_hash);

        // But same algorithm should produce same hash
        let blake3_hash2 = blake3_hasher.hash_ast_block(&block).await.unwrap();
        assert_eq!(blake3_hash, blake3_hash2);
    }

    #[test]
    fn test_large_content_protection() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        // Create a block with extremely large content
        let large_content = "x".repeat(MAX_SERIALIZATION_SIZE + 1);
        let metadata = ASTBlockMetadata::generic();
        let large_block = ASTBlock::new(
            ASTBlockType::Paragraph,
            large_content,
            0,
            MAX_SERIALIZATION_SIZE + 1,
            metadata,
        );

        let result = hasher.serialize_block(&large_block);
        assert!(result.is_err());
    }

    // ==================== MERKLE TREE TESTS ====================

    #[tokio::test]
    async fn test_merkle_tree_single_block() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let metadata = ASTBlockMetadata::heading(1, Some("title".to_string()));
        let block = ASTBlock::new(
            ASTBlockType::Heading,
            "Single Block Test".to_string(),
            0,
            16,
            metadata,
        );

        let tree = hasher
            .build_merkle_tree_from_blocks(&[block.clone()])
            .await
            .unwrap();

        assert_eq!(tree.block_count, 1);
        assert_eq!(tree.depth, 0);
        assert_eq!(tree.leaf_hashes.len(), 1);

        // Verify root hash matches the single block hash
        let block_hash = hasher.hash_ast_block(&block).await.unwrap();
        assert_eq!(tree.root_hash, block_hash.to_hex());

        // Verify tree integrity
        assert!(tree.verify_integrity(&hasher).is_ok());
    }

    #[tokio::test]
    async fn test_merkle_tree_multiple_blocks() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Title".to_string(),
                0,
                5,
                ASTBlockMetadata::heading(1, Some("title".to_string())),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "First paragraph content".to_string(),
                10,
                35,
                ASTBlockMetadata::generic(),
            ),
            ASTBlock::new(
                ASTBlockType::Code,
                "let x = 42;".to_string(),
                40,
                52,
                ASTBlockMetadata::code(Some("rust".to_string()), 1),
            ),
        ];

        let tree = hasher.build_merkle_tree_from_blocks(&blocks).await.unwrap();

        assert_eq!(tree.block_count, 3);
        assert_eq!(tree.leaf_hashes.len(), 3);
        assert!(tree.depth > 0);

        // Verify tree integrity
        assert!(tree.verify_integrity(&hasher).is_ok());

        // Verify each leaf corresponds to a block
        for (i, block) in blocks.iter().enumerate() {
            let leaf = tree.get_leaf(i).expect("Leaf should exist");
            let expected_hash = hasher.hash_ast_block(block).await.unwrap();
            assert_eq!(leaf.hash, expected_hash.to_hex());
        }
    }

    #[tokio::test]
    async fn test_merkle_tree_empty_blocks_error() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);
        let blocks: Vec<ASTBlock> = vec![];

        let result = hasher.build_merkle_tree_from_blocks(&blocks).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HashError::IoError { .. }));
    }

    #[tokio::test]
    async fn test_merkle_tree_from_precomputed_hashes() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Heading".to_string(),
                0,
                7,
                ASTBlockMetadata::heading(1, None),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "Paragraph".to_string(),
                10,
                19,
                ASTBlockMetadata::generic(),
            ),
        ];

        // Pre-compute hashes
        let hashes: Vec<(usize, BlockHash)> = blocks
            .iter()
            .enumerate()
            .map(|(i, _block)| {
                // This is a simplified version - in practice you'd use hasher.hash_ast_block
                (i, BlockHash::new([i as u8; 32]))
            })
            .collect();

        let tree = hasher.build_merkle_tree_from_hashes(&hashes).await.unwrap();

        assert_eq!(tree.block_count, 2);
        assert!(tree.depth > 0);
        assert!(tree.verify_integrity(&hasher).is_ok());
    }

    #[tokio::test]
    async fn test_merkle_tree_verification() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Test Title".to_string(),
                0,
                10,
                ASTBlockMetadata::heading(1, Some("test".to_string())),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "Test content".to_string(),
                15,
                28,
                ASTBlockMetadata::generic(),
            ),
        ];

        let tree = hasher.build_merkle_tree_from_blocks(&blocks).await.unwrap();

        // Verify with correct blocks
        let is_valid = hasher.verify_merkle_tree(&tree, &blocks).await.unwrap();
        assert!(is_valid);

        // Verify with modified blocks
        let mut modified_blocks = blocks.clone();
        modified_blocks[1].content = "Modified content".to_string();

        let is_invalid = hasher
            .verify_merkle_tree(&tree, &modified_blocks)
            .await
            .unwrap();
        assert!(!is_invalid);

        // Verify with wrong number of blocks
        let is_invalid = hasher
            .verify_merkle_tree(&tree, &blocks[0..1])
            .await
            .unwrap();
        assert!(!is_invalid);
    }

    #[tokio::test]
    async fn test_merkle_tree_comparison() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks1 = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Title".to_string(),
                0,
                5,
                ASTBlockMetadata::heading(1, None),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "Content".to_string(),
                10,
                17,
                ASTBlockMetadata::generic(),
            ),
        ];

        let blocks2 = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Title".to_string(),
                0,
                5,
                ASTBlockMetadata::heading(1, None),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "Modified Content".to_string(), // Different content
                10,
                25,
                ASTBlockMetadata::generic(),
            ),
        ];

        let tree1 = hasher
            .build_merkle_tree_from_blocks(&blocks1)
            .await
            .unwrap();
        let tree2 = hasher
            .build_merkle_tree_from_blocks(&blocks2)
            .await
            .unwrap();

        // Trees should be different
        assert_ne!(tree1.root_hash, tree2.root_hash);

        // Compare and detect changes
        let changes = hasher.compare_merkle_trees(&tree1, &tree2);
        assert!(!changes.is_empty());
    }

    #[tokio::test]
    async fn test_ast_blocks_to_hashed_blocks_conversion() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Heading".to_string(),
                0,
                7,
                ASTBlockMetadata::heading(2, Some("subtitle".to_string())),
            ),
            ASTBlock::new(
                ASTBlockType::Code,
                "console.log('Hello');".to_string(),
                15,
                37,
                ASTBlockMetadata::code(Some("javascript".to_string()), 1),
            ),
        ];

        let hashed_blocks = hasher.ast_blocks_to_hashed_blocks(&blocks).await.unwrap();

        assert_eq!(hashed_blocks.len(), 2);

        // Check first block
        assert_eq!(hashed_blocks[0].index, 0);
        assert_eq!(hashed_blocks[0].offset, 0);
        assert!(hashed_blocks[0].is_last == false);
        assert_eq!(hashed_blocks[0].data, blocks[0].content.as_bytes());

        // Check second block
        assert_eq!(hashed_blocks[1].index, 1);
        assert_eq!(hashed_blocks[1].offset, 15);
        assert!(hashed_blocks[1].is_last == true);
        assert_eq!(hashed_blocks[1].data, blocks[1].content.as_bytes());

        // Verify hashes are correct
        for (_i, (hashed_block, original_block)) in
            hashed_blocks.iter().zip(blocks.iter()).enumerate()
        {
            let expected_hash = hasher.hash_ast_block(original_block).await.unwrap();
            assert_eq!(hashed_block.hash, expected_hash.to_hex());
        }
    }

    #[tokio::test]
    async fn test_merkle_tree_statistics() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "H1".to_string(),
                0,
                2,
                ASTBlockMetadata::heading(1, None),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "P1".to_string(),
                5,
                7,
                ASTBlockMetadata::generic(),
            ),
            ASTBlock::new(
                ASTBlockType::Code,
                "code".to_string(),
                10,
                14,
                ASTBlockMetadata::code(None, 1),
            ),
            ASTBlock::new(
                ASTBlockType::Heading,
                "H2".to_string(),
                18,
                20,
                ASTBlockMetadata::heading(1, None),
            ),
        ];

        let tree = hasher.build_merkle_tree_from_blocks(&blocks).await.unwrap();
        let stats = hasher.get_merkle_tree_stats(&tree);

        assert_eq!(stats.block_count, 4);
        assert_eq!(stats.leaf_count, 4);
        assert!(stats.tree_depth > 0);
        assert!(stats.node_count >= 4); // At least as many nodes as leaves
        assert_eq!(stats.algorithm, HashAlgorithm::Blake3);
        assert!(!stats.root_hash.is_empty());

        // Check efficiency ratio (should be reasonable for a small tree)
        assert!(stats.efficiency_ratio() > 0.0);

        // Check if tree is balanced (for small number of blocks, should be balanced)
        assert!(stats.is_balanced());

        // Check summary
        let summary = stats.summary();
        assert!(summary.contains("4 blocks"));
        assert!(summary.contains("blake3"));
    }

    #[tokio::test]
    async fn test_merkle_tree_large_block_count() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        // Create a larger number of blocks to test tree construction
        let mut blocks = Vec::new();
        for i in 0..16 {
            blocks.push(ASTBlock::new(
                ASTBlockType::Paragraph,
                format!("Block {} content", i),
                i * 20,
                i * 20 + 15,
                ASTBlockMetadata::generic(),
            ));
        }

        let tree = hasher.build_merkle_tree_from_blocks(&blocks).await.unwrap();

        assert_eq!(tree.block_count, 16);
        assert_eq!(tree.leaf_hashes.len(), 16);
        assert!(tree.depth >= 4); // Should have multiple levels for 16 blocks

        // Verify tree integrity
        assert!(tree.verify_integrity(&hasher).is_ok());

        // Check stats
        let stats = hasher.get_merkle_tree_stats(&tree);
        assert!(stats.is_balanced());
        assert!(stats.efficiency_ratio() < 2.0); // Should be reasonably efficient
    }

    #[tokio::test]
    async fn test_merkle_tree_with_different_block_types() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "# Main Title".to_string(),
                0,
                12,
                ASTBlockMetadata::heading(1, Some("main-title".to_string())),
            ),
            ASTBlock::new(
                ASTBlockType::Callout,
                "This is important!".to_string(),
                15,
                34,
                ASTBlockMetadata::callout("warning".to_string(), None, true),
            ),
            ASTBlock::new(
                ASTBlockType::Code,
                "fn main() {\n    println!(\"Hello\");\n}".to_string(),
                40,
                73,
                ASTBlockMetadata::code(Some("rust".to_string()), 3),
            ),
            ASTBlock::new(
                ASTBlockType::List,
                "- Item 1\n- Item 2\n- Item 3".to_string(),
                75,
                103,
                ASTBlockMetadata::list(crate::parser::types::ListType::Unordered, 3),
            ),
            ASTBlock::new(
                ASTBlockType::Latex,
                "E = mc^2".to_string(),
                105,
                113,
                ASTBlockMetadata::latex(false),
            ),
        ];

        let tree = hasher.build_merkle_tree_from_blocks(&blocks).await.unwrap();

        assert_eq!(tree.block_count, 5);
        assert!(tree.verify_integrity(&hasher).is_ok());

        // Verify all different block types are handled correctly
        for (i, _block) in blocks.iter().enumerate() {
            let _is_valid = hasher
                .verify_merkle_tree(&tree, &blocks[i..i + 1])
                .await
                .unwrap();
            // Note: This would need a more sophisticated verification for individual blocks
            // since verify_merkle_tree expects the complete block set
        }

        let stats = hasher.get_merkle_tree_stats(&tree);
        assert!(stats.summary().contains("5 blocks"));
    }

    #[tokio::test]
    async fn test_merkle_tree_algorithm_consistency() {
        use crate::hashing::algorithm::{Blake3Algorithm, Sha256Algorithm};
        let blake3_hasher = BlockHasher::new(Blake3Algorithm);
        let sha256_hasher = BlockHasher::new(Sha256Algorithm);

        // Use multiple blocks to ensure internal nodes are created
        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "Test content one".to_string(),
                0,
                15,
                ASTBlockMetadata::generic(),
            ),
            ASTBlock::new(
                ASTBlockType::Heading,
                "Test heading".to_string(),
                20,
                32,
                ASTBlockMetadata::heading(2, Some("test".to_string())),
            ),
            ASTBlock::new(
                ASTBlockType::Code,
                "let x = 42;".to_string(),
                40,
                52,
                ASTBlockMetadata::code(Some("rust".to_string()), 1),
            ),
        ];

        let blake3_tree = blake3_hasher
            .build_merkle_tree_from_blocks(&blocks)
            .await
            .unwrap();
        let sha256_tree = sha256_hasher
            .build_merkle_tree_from_blocks(&blocks)
            .await
            .unwrap();

        // Different algorithms should produce different root hashes
        assert_ne!(blake3_tree.root_hash, sha256_tree.root_hash);

        // But each tree should be valid with its respective hasher
        assert!(blake3_tree.verify_integrity(&blake3_hasher).is_ok());
        assert!(sha256_tree.verify_integrity(&sha256_hasher).is_ok());

        // Cross-verification should fail for trees with internal nodes
        // Note: For single block trees, cross-verification might not fail since there are no internal nodes
        if blake3_tree.depth > 0 && sha256_tree.depth > 0 {
            assert!(blake3_tree.verify_integrity(&sha256_hasher).is_err());
            assert!(sha256_tree.verify_integrity(&blake3_hasher).is_err());
        }

        // Check stats reflect the correct algorithm
        let blake3_stats = blake3_hasher.get_merkle_tree_stats(&blake3_tree);
        let sha256_stats = sha256_hasher.get_merkle_tree_stats(&sha256_tree);
        assert_eq!(blake3_stats.algorithm, HashAlgorithm::Blake3);
        assert_eq!(sha256_stats.algorithm, HashAlgorithm::Sha256);
    }

    #[test]
    fn test_merkle_tree_stats_struct() {
        let stats = MerkleTreeStats::new("abc123".to_string(), 4, 3, 7, 4, HashAlgorithm::Blake3);

        assert_eq!(stats.root_hash, "abc123");
        assert_eq!(stats.block_count, 4);
        assert_eq!(stats.tree_depth, 3);
        assert_eq!(stats.node_count, 7);
        assert_eq!(stats.leaf_count, 4);
        assert_eq!(stats.algorithm, HashAlgorithm::Blake3);

        // Test efficiency ratio
        assert_eq!(stats.efficiency_ratio(), 7.0 / 4.0);

        // Test balanced check
        assert!(stats.is_balanced());

        // Test summary
        let summary = stats.summary();
        assert!(summary.contains("4 blocks"));
        assert!(summary.contains("3 depth"));
        assert!(summary.contains("7 nodes"));
        assert!(summary.contains("blake3"));
    }

    #[test]
    fn test_storage_content_hasher_implementation() {
        use crate::hashing::algorithm::Blake3Algorithm;
        let hasher = BlockHasher::new(Blake3Algorithm);

        // Test hash_block
        let data = b"Hello, World!";
        use crate::storage::ContentHasher as StorageContentHasher;
        let hash1 = StorageContentHasher::hash_block(&hasher, data);
        let hash2 = StorageContentHasher::hash_block(&hasher, data);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // 32 bytes * 2 hex chars

        // Test hash_nodes
        let left = "abc123";
        let right = "def456";
        let combined_hash = hasher.hash_nodes(left, right);
        assert!(!combined_hash.is_empty());
        assert_eq!(combined_hash.len(), 64);

        // Test algorithm metadata
        assert_eq!(hasher.algorithm_name(), "blake3");
        assert_eq!(hasher.hash_length(), 32);
    }

    #[test]
    fn test_merkle_tree_stats_edge_cases() {
        // Single block tree
        let single_stats =
            MerkleTreeStats::new("single".to_string(), 1, 0, 1, 1, HashAlgorithm::Blake3);
        assert!(single_stats.is_balanced());
        assert_eq!(single_stats.efficiency_ratio(), 1.0);

        // Empty tree edge case (shouldn't happen in practice but test anyway)
        let empty_stats = MerkleTreeStats::new("".to_string(), 0, 0, 0, 0, HashAlgorithm::Sha256);
        assert_eq!(empty_stats.efficiency_ratio(), 0.0);
        assert!(empty_stats.is_balanced()); // Empty tree is considered balanced
    }
}
