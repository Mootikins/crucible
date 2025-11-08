//! Simple block hashing implementation for Phase 2 optimize-data-flow
//!
//! This module provides a lightweight block hashing implementation that doesn't
//! depend on crucible-core to avoid circular dependencies. It uses BLAKE3
//! for consistent, fast hashing of AST blocks.

use crate::types::{ASTBlock, ASTBlockMetadata};
use blake3::Hasher;
use serde::Serialize;

/// Simple block hasher for AST blocks
///
/// This implementation provides block hashing functionality without depending
/// on crucible-core to avoid circular dependencies.
#[derive(Debug, Clone)]
pub struct SimpleBlockHasher {
    _private: (),
}

impl SimpleBlockHasher {
    /// Create a new SimpleBlockHasher
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for SimpleBlockHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleBlockHasher {
    /// Hash a single AST block
    pub async fn hash_block(&self, block: &ASTBlock) -> Result<crate::types::BlockHash, String> {
        let serialized = self.serialize_block(block)?;
        let hash_bytes = self.compute_hash(&serialized).await?;
        Ok(crate::types::BlockHash::new(hash_bytes))
    }

    /// Hash multiple AST blocks in parallel
    pub async fn hash_blocks_batch(
        &self,
        blocks: &[ASTBlock],
    ) -> Result<Vec<crate::types::BlockHash>, String> {
        let mut results = Vec::with_capacity(blocks.len());

        // Process blocks concurrently for better performance
        let futures: Vec<_> = blocks.iter().map(|block| self.hash_block(block)).collect();
        let hash_results = futures::future::join_all(futures).await;

        for result in hash_results {
            results.push(result?);
        }

        Ok(results)
    }

    /// Serialize an AST block to a deterministic string format
    fn serialize_block(&self, block: &ASTBlock) -> Result<String, String> {
        let serializable = SerializableBlock::from_ast_block(block);

        // Serialize to JSON with stable ordering
        let serialized = serde_json::to_string(&serializable)
            .map_err(|e| format!("Failed to serialize block: {}", e))?;

        Ok(serialized)
    }

    /// Compute BLAKE3 hash of serialized content
    async fn compute_hash(&self, content: &str) -> Result<[u8; 32], String> {
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        Ok(result.as_bytes().to_owned())
    }

    /// Build a simple Merkle tree from block hashes
    pub async fn build_merkle_root(
        &self,
        blocks: &[ASTBlock],
    ) -> Result<crate::types::BlockHash, String> {
        if blocks.is_empty() {
            return Err("Cannot build Merkle tree from empty block list".to_string());
        }

        // Hash all blocks
        let block_hashes = self.hash_blocks_batch(blocks).await?;

        // Build Merkle tree
        let mut current_level = block_hashes;

        while current_level.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in current_level.chunks(2) {
                if chunk.len() == 2 {
                    // Combine two hashes
                    let combined = format!("{}{}", chunk[0].to_hex(), chunk[1].to_hex());
                    let combined_hash = self.compute_hash(&combined).await?;
                    next_level.push(crate::types::BlockHash::new(combined_hash));
                } else {
                    // Odd number of nodes, promote the last one
                    next_level.push(chunk[0].clone());
                }
            }

            current_level = next_level;
        }

        Ok(current_level[0].clone())
    }
}

/// Serializable representation of an AST block for consistent hashing
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
            ASTBlockMetadata::Generic => Self::Generic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ASTBlockType, ListType};

    #[tokio::test]
    async fn test_simple_block_hasher() {
        let hasher = SimpleBlockHasher::new();

        let metadata = ASTBlockMetadata::heading(1, Some("test".to_string()));
        let block = ASTBlock::new(
            ASTBlockType::Heading,
            "Test Heading".to_string(),
            0,
            12,
            metadata,
        );

        let hash = hasher.hash_block(&block).await.unwrap();
        assert!(!hash.is_zero());

        // Test determinism
        let hash2 = hasher.hash_block(&block).await.unwrap();
        assert_eq!(hash, hash2);
    }

    #[tokio::test]
    async fn test_batch_hashing() {
        let hasher = SimpleBlockHasher::new();

        let blocks = vec![
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

        let hashes = hasher.hash_blocks_batch(&blocks).await.unwrap();
        assert_eq!(hashes.len(), 2);
        assert_ne!(hashes[0], hashes[1]);
    }

    #[tokio::test]
    async fn test_merkle_root() {
        let hasher = SimpleBlockHasher::new();

        let blocks = vec![
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

        let merkle_root = hasher.build_merkle_root(&blocks).await.unwrap();
        assert!(!merkle_root.is_zero());

        // Test single block
        let single_block = vec![blocks[0].clone()];
        let single_root = hasher.build_merkle_root(&single_block).await.unwrap();
        let single_hash = hasher.hash_block(&blocks[0]).await.unwrap();
        assert_eq!(single_root, single_hash);
    }

    #[tokio::test]
    async fn test_empty_blocks_error() {
        let hasher = SimpleBlockHasher::new();
        let blocks: Vec<ASTBlock> = vec![];

        let result = hasher.build_merkle_root(&blocks).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_serialization_format() {
        let metadata = ASTBlockMetadata::heading(1, Some("test".to_string()));
        let block = ASTBlock::new(ASTBlockType::Heading, "Test".to_string(), 0, 4, metadata);

        let hasher = SimpleBlockHasher::new();
        let serialized = hasher.serialize_block(&block).unwrap();

        // Verify JSON format
        assert!(serialized.contains("\"block_type\":\"heading\""));
        assert!(serialized.contains("\"content\":\"Test\""));
        assert!(serialized.contains("\"type\":\"Heading\""));
        assert!(serialized.contains("\"level\":1"));
        assert!(serialized.contains("\"id\":\"test\""));
    }
}
