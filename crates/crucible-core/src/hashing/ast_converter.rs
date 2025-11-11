//! AST Block to HashedBlock conversion for Merkle tree construction
//!
//! This module provides a focused implementation for converting AST blocks extracted
//! from markdown documents into the `HashedBlock` format required by the Merkle tree
//! implementation. This separation follows the Single Responsibility Principle (SRP),
//! isolating conversion logic from hashing logic.
//!
//! # Architecture
//!
//! The `ASTBlockConverter` is designed to be:
//! - **Single-Purpose**: Converts AST blocks to HashedBlocks - nothing more
//! - **Algorithm-Agnostic**: Uses generic HashingAlgorithm trait for flexibility
//! - **Batch-Optimized**: Supports efficient batch processing of multiple blocks
//! - **Zero-Copy Where Possible**: Minimizes allocations during conversion
//!
//! # Design Rationale
//!
//! Previously, this conversion logic lived inside `BlockHasher`, violating SRP by
//! mixing hashing concerns with conversion concerns. This refactoring:
//! - Makes `BlockHasher` focus solely on hashing operations
//! - Makes conversion logic reusable and testable independently
//! - Improves code maintainability and clarity
//! - Follows SOLID principles (SRP in particular)
//!
//! # Examples
//!
//! ```rust
//! use crucible_core::hashing::ast_converter::ASTBlockConverter;
//! use crucible_core::hashing::algorithm::Blake3Algorithm;
//! use crucible_parser::types::{ASTBlock, ASTBlockType, ASTBlockMetadata};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let converter = ASTBlockConverter::new(Blake3Algorithm);
//!
//!     let blocks = vec![
//!         ASTBlock::new(
//!             ASTBlockType::Heading,
//!             "Title".to_string(),
//!             0,
//!             5,
//!             ASTBlockMetadata::heading(1, Some("title".to_string())),
//!         ),
//!         ASTBlock::new(
//!             ASTBlockType::Paragraph,
//!             "Content".to_string(),
//!             10,
//!             17,
//!             ASTBlockMetadata::generic(),
//!         ),
//!     ];
//!
//!     let hashed_blocks = converter.convert_batch(&blocks).await?;
//!     println!("Converted {} blocks", hashed_blocks.len());
//!
//!     Ok(())
//! }
//! ```

use std::marker::PhantomData;

use crate::hashing::algorithm::HashingAlgorithm;
use crate::storage::HashedBlock;
use crate::types::hashing::HashError;

// Import AST block types from the parser crate
use crucible_parser::types::ASTBlock;

/// Converter for transforming AST blocks into HashedBlock format
///
/// This struct provides efficient conversion of AST blocks (parsed markdown structure)
/// into the `HashedBlock` format required by the Merkle tree implementation. It uses
/// a generic hashing algorithm to compute block hashes during conversion.
///
/// # Generic Parameters
///
/// * `A` - The hashing algorithm to use (implements `HashingAlgorithm`)
///
/// # Design Notes
///
/// This converter is intentionally separate from `BlockHasher` to follow SRP:
/// - `BlockHasher` handles all hashing operations
/// - `ASTBlockConverter` handles format conversion only
///
/// While both use hashing, their responsibilities are distinct:
/// - Hashing: Computing cryptographic digests of content
/// - Converting: Transforming data structures between formats
///
/// # Performance Characteristics
///
/// - **Memory**: O(n) where n is the number of blocks
/// - **Allocations**: Minimal - reuses AST block content where possible
/// - **Parallel Processing**: Batch operations can leverage concurrent hashing
///
/// # Thread Safety
///
/// The `ASTBlockConverter` is `Send + Sync` and can be safely shared across threads.
/// All operations are async and non-blocking.
#[derive(Debug, Clone)]
pub struct ASTBlockConverter<A: HashingAlgorithm> {
    /// The hashing algorithm to use for computing block hashes
    algorithm: A,
    /// PhantomData marker for generic parameter
    _phantom: PhantomData<A>,
}

impl<A: HashingAlgorithm> ASTBlockConverter<A> {
    /// Create a new ASTBlockConverter with the specified hashing algorithm
    ///
    /// # Arguments
    ///
    /// * `algorithm` - The hash algorithm implementation to use for block hashing
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::hashing::ast_converter::ASTBlockConverter;
    /// use crucible_core::hashing::algorithm::Blake3Algorithm;
    ///
    /// let converter = ASTBlockConverter::new(Blake3Algorithm);
    /// ```
    pub fn new(algorithm: A) -> Self {
        Self {
            algorithm,
            _phantom: PhantomData,
        }
    }

    /// Convert a single AST block to HashedBlock format
    ///
    /// This method computes the hash of the AST block content and converts it
    /// to the `HashedBlock` format used by the Merkle tree implementation.
    ///
    /// # Arguments
    ///
    /// * `block` - The AST block to convert
    /// * `index` - The index of this block in the note (used for ordering)
    /// * `is_last` - Whether this is the last block in the sequence
    ///
    /// # Returns
    ///
    /// The converted `HashedBlock` or an error if hashing fails
    ///
    /// # Errors
    ///
    /// Returns `HashError` if:
    /// - Block content hashing fails
    /// - Hash computation produces invalid length
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::hashing::ast_converter::ASTBlockConverter;
    /// use crucible_core::hashing::algorithm::Blake3Algorithm;
    /// use crucible_parser::types::{ASTBlock, ASTBlockType, ASTBlockMetadata};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let converter = ASTBlockConverter::new(Blake3Algorithm);
    ///
    ///     let block = ASTBlock::new(
    ///         ASTBlockType::Heading,
    ///         "Title".to_string(),
    ///         0,
    ///         5,
    ///         ASTBlockMetadata::heading(1, Some("title".to_string())),
    ///     );
    ///
    ///     let hashed_block = converter.convert(&block, 0, false).await?;
    ///     println!("Converted block with hash: {}", hashed_block.hash);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn convert(
        &self,
        block: &ASTBlock,
        index: usize,
        is_last: bool,
    ) -> Result<HashedBlock, HashError> {
        // Hash the block content using the algorithm
        let hash_bytes = self.hash_block_content(block).await?;

        // Convert hash bytes to hex string
        let hash_hex = hex::encode(hash_bytes);

        // Create HashedBlock from the AST block
        let hashed_block = HashedBlock::new(
            hash_hex,
            block.content.as_bytes().to_vec(),
            index,
            block.start_offset,
            is_last,
        );

        Ok(hashed_block)
    }

    /// Convert multiple AST blocks to HashedBlock format in batch
    ///
    /// This is the primary method for converting AST blocks to the format required
    /// by Merkle tree construction. It processes blocks sequentially but uses async
    /// hashing for better performance.
    ///
    /// # Arguments
    ///
    /// * `blocks` - Vector of AST blocks to convert
    ///
    /// # Returns
    ///
    /// Vector of HashedBlock instances in the same order as input, or error if
    /// any conversion fails
    ///
    /// # Errors
    ///
    /// Returns `HashError` if any block conversion fails
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::hashing::ast_converter::ASTBlockConverter;
    /// use crucible_core::hashing::algorithm::Blake3Algorithm;
    /// use crucible_parser::types::{ASTBlock, ASTBlockType, ASTBlockMetadata};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let converter = ASTBlockConverter::new(Blake3Algorithm);
    ///
    ///     let blocks = vec![
    ///         ASTBlock::new(
    ///             ASTBlockType::Heading,
    ///             "Title".to_string(),
    ///             0,
    ///             5,
    ///             ASTBlockMetadata::heading(1, None),
    ///         ),
    ///         ASTBlock::new(
    ///             ASTBlockType::Paragraph,
    ///             "Content".to_string(),
    ///             10,
    ///             17,
    ///             ASTBlockMetadata::generic(),
    ///         ),
    ///     ];
    ///
    ///     let hashed_blocks = converter.convert_batch(&blocks).await?;
    ///     assert_eq!(hashed_blocks.len(), 2);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn convert_batch(&self, blocks: &[ASTBlock]) -> Result<Vec<HashedBlock>, HashError> {
        let mut hashed_blocks = Vec::with_capacity(blocks.len());

        for (index, block) in blocks.iter().enumerate() {
            let is_last = index == blocks.len() - 1;
            let hashed_block = self.convert(block, index, is_last).await?;
            hashed_blocks.push(hashed_block);
        }

        Ok(hashed_blocks)
    }

    /// Hash the content of an AST block
    ///
    /// This is an internal method that computes the hash of a block's content.
    /// It uses the generic algorithm to compute the cryptographic digest.
    ///
    /// IMPORTANT: This method hashes the serialized JSON representation of the block
    /// (including metadata) to match the behavior of BlockHasher::hash_ast_block.
    /// This ensures consistency between direct block hashing and Merkle tree construction.
    ///
    /// # Arguments
    ///
    /// * `block` - The AST block whose content should be hashed
    ///
    /// # Returns
    ///
    /// The hash bytes or an error if hashing fails
    ///
    /// # Errors
    ///
    /// Returns `HashError` if the hash length is invalid or serialization fails
    async fn hash_block_content(&self, block: &ASTBlock) -> Result<Vec<u8>, HashError> {
        // Serialize the block to JSON (matching BlockHasher::serialize_block behavior)
        let serialized = self.serialize_block(block)?;

        // Use the algorithm to hash the serialized block
        let hash_bytes = self.algorithm.hash(serialized.as_bytes());

        // Validate hash length
        if hash_bytes.len() != 32 {
            return Err(HashError::InvalidLength {
                len: hash_bytes.len(),
            });
        }

        Ok(hash_bytes)
    }

    /// Serialize an AST block to a deterministic string format
    ///
    /// This method mirrors BlockHasher::serialize_block to ensure consistent hashing.
    fn serialize_block(&self, block: &ASTBlock) -> Result<String, HashError> {
        use serde::Serialize;

        // Create a serializable representation matching BlockHasher's SerializableBlock
        #[derive(Serialize)]
        struct SerializableBlock<'a> {
            block_type: &'a str,
            content: &'a str,
            metadata: SerializableMetadata<'a>,
            start_offset: usize,
            end_offset: usize,
        }

        #[derive(Serialize)]
        #[serde(tag = "type")]
        enum SerializableMetadata<'a> {
            Heading {
                level: u8,
                id: Option<&'a str>,
            },
            Code {
                language: Option<&'a str>,
                line_count: usize,
            },
            List {
                list_type: String,
                item_count: usize,
            },
            Callout {
                callout_type: &'a str,
                title: Option<&'a str>,
                is_standard_type: bool,
            },
            Latex {
                is_block: bool,
            },
            Table {
                rows: usize,
                columns: usize,
                headers: &'a [String],
            },
            Generic,
        }

        let metadata = match &block.metadata {
            crucible_parser::types::ASTBlockMetadata::Heading { level, id } => {
                SerializableMetadata::Heading {
                    level: *level,
                    id: id.as_deref(),
                }
            }
            crucible_parser::types::ASTBlockMetadata::Code {
                language,
                line_count,
            } => SerializableMetadata::Code {
                language: language.as_deref(),
                line_count: *line_count,
            },
            crucible_parser::types::ASTBlockMetadata::List {
                list_type,
                item_count,
            } => SerializableMetadata::List {
                list_type: format!("{:?}", list_type),
                item_count: *item_count,
            },
            crucible_parser::types::ASTBlockMetadata::Callout {
                callout_type,
                title,
                is_standard_type,
            } => SerializableMetadata::Callout {
                callout_type,
                title: title.as_deref(),
                is_standard_type: *is_standard_type,
            },
            crucible_parser::types::ASTBlockMetadata::Latex { is_block } => {
                SerializableMetadata::Latex {
                    is_block: *is_block,
                }
            }
            crucible_parser::types::ASTBlockMetadata::Table {
                rows,
                columns,
                headers,
            } => SerializableMetadata::Table {
                rows: *rows,
                columns: *columns,
                headers,
            },
            crucible_parser::types::ASTBlockMetadata::Generic => SerializableMetadata::Generic,
        };

        let serializable = SerializableBlock {
            block_type: block.type_name(),
            content: &block.content,
            metadata,
            start_offset: block.start_offset,
            end_offset: block.end_offset,
        };

        // Serialize to JSON with stable ordering
        serde_json::to_string(&serializable).map_err(|e| HashError::IoError {
            error: format!("Failed to serialize block: {}", e),
        })
    }

    /// Get the algorithm name for diagnostics
    ///
    /// This method returns the name of the hashing algorithm being used,
    /// which is useful for debugging and logging purposes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::hashing::ast_converter::ASTBlockConverter;
    /// use crucible_core::hashing::algorithm::Blake3Algorithm;
    ///
    /// let converter = ASTBlockConverter::new(Blake3Algorithm);
    /// assert_eq!(converter.algorithm_name(), "blake3");
    /// ```
    pub fn algorithm_name(&self) -> &'static str {
        self.algorithm.algorithm_name()
    }

    /// Get statistics about a batch conversion
    ///
    /// This method analyzes a set of AST blocks and returns useful statistics
    /// about them without performing the actual conversion. Useful for
    /// performance monitoring and optimization.
    ///
    /// # Arguments
    ///
    /// * `blocks` - Vector of AST blocks to analyze
    ///
    /// # Returns
    ///
    /// Conversion statistics including block counts, sizes, and types
    pub fn analyze_batch(&self, blocks: &[ASTBlock]) -> ConversionStats {
        let mut stats = ConversionStats::default();

        for block in blocks {
            stats.total_blocks += 1;
            stats.total_content_bytes += block.content.len();
            stats.total_span_bytes += block.length();

            // Track block types
            let type_name = block.type_name().to_string();
            *stats.block_type_counts.entry(type_name).or_insert(0) += 1;

            // Track empty blocks
            if block.is_empty() {
                stats.empty_blocks += 1;
            }
        }

        stats
    }
}

/// Statistics about AST block conversion operations
///
/// This struct provides useful metrics about conversion operations for
/// performance monitoring, optimization, and debugging purposes.
#[derive(Debug, Clone, Default)]
pub struct ConversionStats {
    /// Total number of blocks in the batch
    pub total_blocks: usize,
    /// Total number of bytes in block content
    pub total_content_bytes: usize,
    /// Total number of bytes spanned by blocks in source
    pub total_span_bytes: usize,
    /// Number of empty blocks (no content)
    pub empty_blocks: usize,
    /// Count of blocks by type
    pub block_type_counts: std::collections::HashMap<String, usize>,
}

impl ConversionStats {
    /// Create new empty statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Get average content size per block
    pub fn avg_content_size(&self) -> f64 {
        if self.total_blocks == 0 {
            0.0
        } else {
            self.total_content_bytes as f64 / self.total_blocks as f64
        }
    }

    /// Get average span size per block
    pub fn avg_span_size(&self) -> f64 {
        if self.total_blocks == 0 {
            0.0
        } else {
            self.total_span_bytes as f64 / self.total_blocks as f64
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
            "Blocks: {}, Avg content: {:.1} bytes, Avg span: {:.1} bytes, \
             Empty: {:.1}%, Most common: {}",
            self.total_blocks,
            self.avg_content_size(),
            self.avg_span_size(),
            self.empty_block_percentage(),
            most_common
        )
    }
}

/// Type alias for commonly used BLAKE3 converter
pub type Blake3ASTBlockConverter = ASTBlockConverter<crate::hashing::algorithm::Blake3Algorithm>;

/// Type alias for commonly used SHA256 converter
pub type Sha256ASTBlockConverter = ASTBlockConverter<crate::hashing::algorithm::Sha256Algorithm>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::algorithm::{Blake3Algorithm, Sha256Algorithm};
    use crucible_parser::types::{ASTBlockMetadata, ASTBlockType, ListType};

    #[tokio::test]
    async fn test_converter_creation() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);
        assert_eq!(converter.algorithm_name(), "blake3");

        let sha256_converter = ASTBlockConverter::new(Sha256Algorithm);
        assert_eq!(sha256_converter.algorithm_name(), "sha256");
    }

    #[tokio::test]
    async fn test_single_block_conversion() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);

        let block = ASTBlock::new(
            ASTBlockType::Heading,
            "Test Heading".to_string(),
            0,
            12,
            ASTBlockMetadata::heading(1, Some("test".to_string())),
        );

        let hashed_block = converter.convert(&block, 0, false).await.unwrap();

        assert_eq!(hashed_block.index, 0);
        assert_eq!(hashed_block.offset, 0);
        assert!(!hashed_block.is_last);
        assert_eq!(hashed_block.data, block.content.as_bytes());
        assert!(!hashed_block.hash.is_empty());
        assert_eq!(hashed_block.hash.len(), 64); // 32 bytes * 2 hex chars
    }

    #[tokio::test]
    async fn test_batch_conversion() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);

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

        let hashed_blocks = converter.convert_batch(&blocks).await.unwrap();

        assert_eq!(hashed_blocks.len(), 3);

        // Check first block
        assert_eq!(hashed_blocks[0].index, 0);
        assert_eq!(hashed_blocks[0].offset, 0);
        assert!(!hashed_blocks[0].is_last);
        assert_eq!(hashed_blocks[0].data, blocks[0].content.as_bytes());

        // Check second block
        assert_eq!(hashed_blocks[1].index, 1);
        assert_eq!(hashed_blocks[1].offset, 10);
        assert!(!hashed_blocks[1].is_last);
        assert_eq!(hashed_blocks[1].data, blocks[1].content.as_bytes());

        // Check third block (last)
        assert_eq!(hashed_blocks[2].index, 2);
        assert_eq!(hashed_blocks[2].offset, 30);
        assert!(hashed_blocks[2].is_last);
        assert_eq!(hashed_blocks[2].data, blocks[2].content.as_bytes());

        // Verify all hashes are unique
        assert_ne!(hashed_blocks[0].hash, hashed_blocks[1].hash);
        assert_ne!(hashed_blocks[1].hash, hashed_blocks[2].hash);
        assert_ne!(hashed_blocks[0].hash, hashed_blocks[2].hash);
    }

    #[tokio::test]
    async fn test_empty_batch_conversion() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);
        let blocks: Vec<ASTBlock> = vec![];

        let hashed_blocks = converter.convert_batch(&blocks).await.unwrap();
        assert_eq!(hashed_blocks.len(), 0);
    }

    #[tokio::test]
    async fn test_deterministic_hashing() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);

        let block = ASTBlock::new(
            ASTBlockType::Paragraph,
            "Test content".to_string(),
            0,
            12,
            ASTBlockMetadata::generic(),
        );

        let hashed1 = converter.convert(&block, 0, false).await.unwrap();
        let hashed2 = converter.convert(&block, 0, false).await.unwrap();

        // Same block should produce same hash
        assert_eq!(hashed1.hash, hashed2.hash);
    }

    #[tokio::test]
    async fn test_different_content_different_hash() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);

        let block1 = ASTBlock::new(
            ASTBlockType::Paragraph,
            "First content".to_string(),
            0,
            13,
            ASTBlockMetadata::generic(),
        );

        let block2 = ASTBlock::new(
            ASTBlockType::Paragraph,
            "Second content".to_string(),
            0,
            14,
            ASTBlockMetadata::generic(),
        );

        let hashed1 = converter.convert(&block1, 0, false).await.unwrap();
        let hashed2 = converter.convert(&block2, 0, false).await.unwrap();

        // Different content should produce different hashes
        assert_ne!(hashed1.hash, hashed2.hash);
    }

    #[tokio::test]
    async fn test_algorithm_consistency() {
        let blake3_converter = ASTBlockConverter::new(Blake3Algorithm);
        let sha256_converter = ASTBlockConverter::new(Sha256Algorithm);

        let block = ASTBlock::new(
            ASTBlockType::Code,
            "fn main() {}".to_string(),
            0,
            12,
            ASTBlockMetadata::code(Some("rust".to_string()), 1),
        );

        let blake3_hashed = blake3_converter.convert(&block, 0, false).await.unwrap();
        let sha256_hashed = sha256_converter.convert(&block, 0, false).await.unwrap();

        // Different algorithms should produce different hashes
        assert_ne!(blake3_hashed.hash, sha256_hashed.hash);

        // But data and metadata should be the same
        assert_eq!(blake3_hashed.data, sha256_hashed.data);
        assert_eq!(blake3_hashed.index, sha256_hashed.index);
        assert_eq!(blake3_hashed.offset, sha256_hashed.offset);
    }

    #[test]
    fn test_conversion_stats() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Title".to_string(),
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
            ASTBlock::new(
                ASTBlockType::Code,
                "code".to_string(),
                25,
                29,
                ASTBlockMetadata::code(None, 1),
            ),
            ASTBlock::new(
                ASTBlockType::Paragraph,
                "".to_string(), // Empty block
                35,
                35,
                ASTBlockMetadata::generic(),
            ),
        ];

        let stats = converter.analyze_batch(&blocks);

        assert_eq!(stats.total_blocks, 4);
        assert_eq!(stats.empty_blocks, 1);
        assert_eq!(stats.empty_block_percentage(), 25.0);

        // Check block type counts
        assert_eq!(stats.block_type_counts.get("heading"), Some(&1));
        assert_eq!(stats.block_type_counts.get("paragraph"), Some(&2));
        assert_eq!(stats.block_type_counts.get("code"), Some(&1));

        // Check most common type
        let (most_common, count) = stats.most_common_type().unwrap();
        assert_eq!(most_common, "paragraph");
        assert_eq!(count, 2);

        // Check summary
        let summary = stats.summary();
        assert!(summary.contains("Blocks: 4"));
        assert!(summary.contains("Empty: 25.0%"));
        assert!(summary.contains("paragraph (2 blocks)"));
    }

    #[test]
    fn test_conversion_stats_empty() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);
        let blocks: Vec<ASTBlock> = vec![];

        let stats = converter.analyze_batch(&blocks);

        assert_eq!(stats.total_blocks, 0);
        assert_eq!(stats.empty_blocks, 0);
        assert_eq!(stats.avg_content_size(), 0.0);
        assert_eq!(stats.avg_span_size(), 0.0);
        assert_eq!(stats.empty_block_percentage(), 0.0);
        assert!(stats.most_common_type().is_none());
    }

    #[tokio::test]
    async fn test_various_block_types() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);

        let blocks = vec![
            ASTBlock::new(
                ASTBlockType::Heading,
                "Heading".to_string(),
                0,
                7,
                ASTBlockMetadata::heading(2, Some("heading".to_string())),
            ),
            ASTBlock::new(
                ASTBlockType::List,
                "- Item 1\n- Item 2".to_string(),
                10,
                28,
                ASTBlockMetadata::list(ListType::Unordered, 2),
            ),
            ASTBlock::new(
                ASTBlockType::Callout,
                "Important note".to_string(),
                30,
                44,
                ASTBlockMetadata::callout("note".to_string(), None, true),
            ),
            ASTBlock::new(
                ASTBlockType::Latex,
                "E = mc^2".to_string(),
                50,
                58,
                ASTBlockMetadata::latex(false),
            ),
        ];

        let hashed_blocks = converter.convert_batch(&blocks).await.unwrap();

        assert_eq!(hashed_blocks.len(), 4);

        // Verify each block converted correctly
        for (i, (original, hashed)) in blocks.iter().zip(hashed_blocks.iter()).enumerate() {
            assert_eq!(hashed.index, i);
            assert_eq!(hashed.offset, original.start_offset);
            assert_eq!(hashed.data, original.content.as_bytes());
            assert_eq!(hashed.is_last, i == blocks.len() - 1);
        }
    }

    #[tokio::test]
    async fn test_large_content_block() {
        let converter = ASTBlockConverter::new(Blake3Algorithm);

        // Create a block with large content
        let large_content = "x".repeat(100_000);
        let block = ASTBlock::new(
            ASTBlockType::Code,
            large_content.clone(),
            0,
            large_content.len(),
            ASTBlockMetadata::code(Some("text".to_string()), 1),
        );

        let hashed_block = converter.convert(&block, 0, true).await.unwrap();

        assert_eq!(hashed_block.data.len(), large_content.len());
        assert!(!hashed_block.hash.is_empty());
        assert_eq!(hashed_block.hash.len(), 64);
    }

    #[test]
    fn test_type_aliases() {
        let blake3_converter: Blake3ASTBlockConverter = ASTBlockConverter::new(Blake3Algorithm);
        assert_eq!(blake3_converter.algorithm_name(), "blake3");

        let sha256_converter: Sha256ASTBlockConverter = ASTBlockConverter::new(Sha256Algorithm);
        assert_eq!(sha256_converter.algorithm_name(), "sha256");
    }
}
