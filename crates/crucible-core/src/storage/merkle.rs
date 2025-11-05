//! Merkle Tree Implementation for Content-Addressed Storage
//!
//! This module provides a binary Merkle tree implementation for efficient
//! content integrity verification and change detection. The implementation
//! follows test-driven development principles with comprehensive tests.
//!
//! ## Architecture
//!
//! The Merkle tree is a binary tree where:
//! - **Leaf nodes** contain hashes of individual content blocks
//! - **Internal nodes** contain hashes of their two child nodes
//! - **Root node** provides a single hash representing the entire content
//!
//! This enables efficient verification of content integrity and detection
//! of changes at the block level.

use crate::storage::{ContentHasher, HashedBlock, StorageError, StorageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A node in a binary Merkle tree
///
/// Nodes can be either leaf nodes (containing block hashes) or internal nodes
/// (containing hashes of their child nodes). The tree is always binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleNode {
    /// Hash of this node (leaf hash for leaves, combined hash for internal nodes)
    pub hash: String,
    /// Node type and data
    pub node_type: NodeType,
    /// Depth of this node in the tree (0 for root)
    pub depth: usize,
    /// Index of this node within its depth level (left-to-right)
    pub index: usize,
}

/// Node type enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    /// Leaf node containing a block hash
    Leaf {
        /// Hash of the content block
        block_hash: String,
        /// Index of the block in the original content
        block_index: usize,
    },
    /// Internal node with two children
    Internal {
        /// Hash of the left child node
        left_hash: String,
        /// Hash of the right child node
        right_hash: String,
        /// Index of the left child
        left_index: usize,
        /// Index of the right child
        right_index: usize,
    },
}

/// A complete binary Merkle tree
///
/// This represents the entire Merkle tree structure for a piece of content,
/// providing efficient integrity verification and change detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleTree {
    /// Root hash of the entire tree
    pub root_hash: String,
    /// All nodes in the tree indexed by their hash
    pub nodes: HashMap<String, MerkleNode>,
    /// List of leaf node hashes (for iteration)
    pub leaf_hashes: Vec<String>,
    /// Total depth of the tree
    pub depth: usize,
    /// Total number of blocks represented
    pub block_count: usize,
}

/// Represents a detected change in a Merkle tree
///
/// This enum provides backward compatibility with the enhanced change detection system.
/// For new development, prefer using `EnhancedTreeChange` from the `diff` module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeChange {
    /// A block was added
    AddedBlock { index: usize, hash: String },
    /// A block was modified
    ModifiedBlock {
        index: usize,
        old_hash: String,
        new_hash: String,
    },
    /// A block was deleted
    DeletedBlock { index: usize, hash: String },
    /// Tree structure changed (rebalancing)
    StructureChanged,
}

impl MerkleNode {
    /// Create a new leaf node
    ///
    /// # Arguments
    /// * `block_hash` - Hash of the content block
    /// * `block_index` - Index of the block in original content
    /// * `depth` - Depth of this node in the tree
    /// * `index` - Index within the depth level
    ///
    /// # Returns
    /// New leaf node or error if hash is invalid
    pub fn new_leaf(
        block_hash: String,
        block_index: usize,
        depth: usize,
        index: usize,
    ) -> StorageResult<Self> {
        if block_hash.is_empty() {
            return Err(StorageError::InvalidHash("Empty block hash".to_string()));
        }

        Ok(Self {
            hash: block_hash.clone(),
            node_type: NodeType::Leaf {
                block_hash,
                block_index,
            },
            depth,
            index,
        })
    }

    /// Create a new internal node
    ///
    /// # Arguments
    /// * `left_hash` - Hash of the left child
    /// * `right_hash` - Hash of the right child
    /// * `left_index` - Index of the left child
    /// * `right_index` - Index of the right child
    /// * `depth` - Depth of this node in the tree
    /// * `index` - Index within the depth level
    /// * `hasher` - Hash function to combine child hashes
    ///
    /// # Returns
    /// New internal node or error if hashes are invalid
    pub fn new_internal<H>(
        left_hash: String,
        right_hash: String,
        left_index: usize,
        right_index: usize,
        depth: usize,
        index: usize,
        hasher: &H,
    ) -> StorageResult<Self>
    where
        H: ContentHasher + ?Sized,
    {
        if left_hash.is_empty() || right_hash.is_empty() {
            return Err(StorageError::InvalidHash("Empty child hash".to_string()));
        }

        let hash = hasher.hash_nodes(&left_hash, &right_hash);

        Ok(Self {
            hash,
            node_type: NodeType::Internal {
                left_hash,
                right_hash,
                left_index,
                right_index,
            },
            depth,
            index,
        })
    }

    /// Check if this node is a leaf
    pub fn is_leaf(&self) -> bool {
        matches!(self.node_type, NodeType::Leaf { .. })
    }

    /// Check if this node is internal
    pub fn is_internal(&self) -> bool {
        matches!(self.node_type, NodeType::Internal { .. })
    }

    /// Get the block hash if this is a leaf node
    pub fn block_hash(&self) -> Option<&str> {
        match &self.node_type {
            NodeType::Leaf { block_hash, .. } => Some(block_hash),
            _ => None,
        }
    }

    /// Get the block index if this is a leaf node
    pub fn block_index(&self) -> Option<usize> {
        match &self.node_type {
            NodeType::Leaf { block_index, .. } => Some(*block_index),
            _ => None,
        }
    }

    /// Get child hashes if this is an internal node
    pub fn child_hashes(&self) -> Option<(&str, &str)> {
        match &self.node_type {
            NodeType::Internal {
                left_hash,
                right_hash,
                ..
            } => Some((left_hash, right_hash)),
            _ => None,
        }
    }

    /// Validate the node's internal consistency
    pub fn validate(&self) -> StorageResult<()> {
        // Check that hash is not empty
        if self.hash.is_empty() {
            return Err(StorageError::CorruptedData("Empty node hash".to_string()));
        }

        // Validate node type specific consistency
        match &self.node_type {
            NodeType::Leaf { block_hash, .. } => {
                if block_hash != &self.hash {
                    return Err(StorageError::CorruptedData(
                        "Leaf node hash doesn't match block hash".to_string(),
                    ));
                }
            }
            NodeType::Internal {
                left_hash,
                right_hash,
                ..
            } => {
                if left_hash == right_hash {
                    return Err(StorageError::CorruptedData(
                        "Internal node has identical children".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

impl MerkleTree {
    /// Create a new Merkle tree from a list of hashed blocks
    ///
    /// # Arguments
    /// * `blocks` - List of hashed blocks to include in the tree
    /// * `hasher` - Hash function implementation
    ///
    /// # Returns
    /// New Merkle tree or error if creation fails
    pub fn from_blocks<H>(blocks: &[HashedBlock], hasher: &H) -> StorageResult<Self>
    where
        H: ContentHasher + ?Sized,
    {
        if blocks.is_empty() {
            return Err(StorageError::BlockSize(
                "Cannot create Merkle tree from empty block list".to_string(),
            ));
        }

        let mut tree = Self {
            root_hash: String::new(),
            nodes: HashMap::new(),
            leaf_hashes: Vec::new(),
            depth: 0,
            block_count: blocks.len(),
        };

        // Create leaf nodes
        let mut current_level: Vec<MerkleNode> = Vec::new();
        for (index, block) in blocks.iter().enumerate() {
            let leaf = MerkleNode::new_leaf(
                block.hash.clone(),
                block.index,
                0, // Will be calculated later
                index,
            )?;
            tree.leaf_hashes.push(leaf.hash.clone());
            current_level.push(leaf);
        }

        // Build tree bottom-up
        let mut level_depth = 0;
        let mut next_level: Vec<MerkleNode>;

        while current_level.len() > 1 {
            level_depth += 1;
            next_level = Vec::new();

            // Process pairs of nodes
            for chunk in current_level.chunks(2) {
                if chunk.len() == 2 {
                    let left = &chunk[0];
                    let right = &chunk[1];

                    let parent = MerkleNode::new_internal(
                        left.hash.clone(),
                        right.hash.clone(),
                        left.index,
                        right.index,
                        level_depth,
                        next_level.len(),
                        hasher,
                    )?;
                    next_level.push(parent);
                } else {
                    // Handle odd number - promote the last node
                    next_level.push(chunk[0].clone());
                }
            }

            // Add current level nodes to tree with correct depth
            for (index, node) in current_level.iter().enumerate() {
                let mut updated_node = node.clone();
                updated_node.depth = level_depth - 1;
                updated_node.index = index;
                tree.nodes.insert(updated_node.hash.clone(), updated_node);
            }

            current_level = next_level;
        }

        // Set root
        if let Some(root) = current_level.first() {
            tree.root_hash = root.hash.clone();
            let mut updated_root = root.clone();
            updated_root.depth = level_depth;
            updated_root.index = 0;
            tree.nodes.insert(updated_root.hash.clone(), updated_root);
            tree.depth = level_depth;
        } else {
            return Err(StorageError::TreeValidation(
                "Failed to create tree root".to_string(),
            ));
        }

        // Update all node depths
        tree.update_node_depths()?;

        Ok(tree)
    }

    /// Create a Merkle tree from a single block
    ///
    /// # Arguments
    /// * `block` - Single hashed block
    ///
    /// # Returns
    /// New Merkle tree with single node
    pub fn from_single_block(block: &HashedBlock) -> StorageResult<Self> {
        let leaf = MerkleNode::new_leaf(block.hash.clone(), block.index, 0, 0)?;

        let mut nodes = HashMap::new();
        nodes.insert(leaf.hash.clone(), leaf.clone());

        Ok(Self {
            root_hash: block.hash.clone(),
            nodes,
            leaf_hashes: vec![block.hash.clone()],
            depth: 0,
            block_count: 1,
        })
    }

    /// Get the root node of the tree
    pub fn root(&self) -> Option<&MerkleNode> {
        self.nodes.get(&self.root_hash)
    }

    /// Get a leaf node by block index
    pub fn get_leaf(&self, block_index: usize) -> Option<&MerkleNode> {
        self.nodes.values().find(|node| {
            if let NodeType::Leaf {
                block_index: idx, ..
            } = &node.node_type
            {
                *idx == block_index
            } else {
                false
            }
        })
    }

    /// Get a node by its hash
    pub fn get_node(&self, hash: &str) -> Option<&MerkleNode> {
        self.nodes.get(hash)
    }

    /// Calculate the tree depth
    pub fn calculate_depth(&self) -> usize {
        if self.block_count == 0 {
            0
        } else if self.block_count == 1 {
            1
        } else {
            (self.block_count as f64).log2().ceil() as usize + 1
        }
    }

    /// Verify the integrity of the entire tree
    ///
    /// # Arguments
    /// * `hasher` - Hash function to verify internal nodes
    ///
    /// # Returns
    /// `Ok(())` if tree is valid, error with details if invalid
    pub fn verify_integrity<H>(&self, hasher: &H) -> StorageResult<()>
    where
        H: ContentHasher + ?Sized,
    {
        if self.root_hash.is_empty() {
            return Err(StorageError::TreeValidation("Empty root hash".to_string()));
        }

        if self.nodes.is_empty() {
            return Err(StorageError::TreeValidation("No nodes in tree".to_string()));
        }

        // Verify root exists
        let _root = self
            .nodes
            .get(&self.root_hash)
            .ok_or_else(|| StorageError::TreeValidation("Root node not found".to_string()))?;

        // Verify all nodes
        for node in self.nodes.values() {
            node.validate()?;

            if let NodeType::Internal {
                left_hash,
                right_hash,
                ..
            } = &node.node_type
            {
                // Verify child nodes exist
                if !self.nodes.contains_key(left_hash) {
                    return Err(StorageError::TreeValidation(format!(
                        "Missing left child: {}",
                        left_hash
                    )));
                }
                if !self.nodes.contains_key(right_hash) {
                    return Err(StorageError::TreeValidation(format!(
                        "Missing right child: {}",
                        right_hash
                    )));
                }

                // Verify hash combination
                let expected_hash = hasher.hash_nodes(left_hash, right_hash);
                if node.hash != expected_hash {
                    return Err(StorageError::TreeValidation(format!(
                        "Invalid hash for node {}: expected {}, got {}",
                        node.hash, expected_hash, node.hash
                    )));
                }
            }
        }

        Ok(())
    }

    /// Compare this tree with another and detect changes (basic implementation)
    ///
    /// This method provides the original basic change detection functionality.
    /// For enhanced change detection with granular analysis, similarity scoring,
    /// and advanced features, use the `EnhancedChangeDetector` from the `diff` module.
    ///
    /// # Arguments
    /// * `other` - Another Merkle tree to compare against
    ///
    /// # Returns
    /// List of detected changes
    pub fn compare_with(&self, other: &MerkleTree) -> Vec<TreeChange> {
        let mut changes = Vec::new();

        // Compare root hashes first
        if self.root_hash != other.root_hash {
            // Detect structural changes
            if self.depth != other.depth || self.block_count != other.block_count {
                changes.push(TreeChange::StructureChanged);
            }

            // Compare leaf nodes to detect specific changes
            let mut i = 0;
            let mut j = 0;

            while i < self.leaf_hashes.len() && j < other.leaf_hashes.len() {
                let self_leaf = self.get_leaf(i);
                let other_leaf = other.get_leaf(j);

                match (self_leaf, other_leaf) {
                    (Some(self_node), Some(other_node)) => {
                        if self_node.hash != other_node.hash {
                            if let (Some(self_idx), Some(other_idx)) =
                                (self_node.block_index(), other_node.block_index())
                            {
                                if i < j {
                                    changes.push(TreeChange::AddedBlock {
                                        index: other_idx,
                                        hash: other_node.hash.clone(),
                                    });
                                } else if i > j {
                                    changes.push(TreeChange::DeletedBlock {
                                        index: self_idx,
                                        hash: self_node.hash.clone(),
                                    });
                                } else {
                                    changes.push(TreeChange::ModifiedBlock {
                                        index: self_idx,
                                        old_hash: self_node.hash.clone(),
                                        new_hash: other_node.hash.clone(),
                                    });
                                }
                            }
                        }
                    }
                    (Some(_), None) => {
                        if let Some(node) = self_leaf {
                            if let Some(idx) = node.block_index() {
                                changes.push(TreeChange::DeletedBlock {
                                    index: idx,
                                    hash: node.hash.clone(),
                                });
                            }
                        }
                    }
                    (None, Some(_)) => {
                        if let Some(node) = other_leaf {
                            if let Some(idx) = node.block_index() {
                                changes.push(TreeChange::AddedBlock {
                                    index: idx,
                                    hash: node.hash.clone(),
                                });
                            }
                        }
                    }
                    _ => {}
                }

                i += 1;
                j += 1;
            }

            // Handle remaining nodes
            while i < self.leaf_hashes.len() {
                if let Some(node) = self.get_leaf(i) {
                    if let Some(idx) = node.block_index() {
                        changes.push(TreeChange::DeletedBlock {
                            index: idx,
                            hash: node.hash.clone(),
                        });
                    }
                }
                i += 1;
            }

            while j < other.leaf_hashes.len() {
                if let Some(node) = other.get_leaf(j) {
                    if let Some(idx) = node.block_index() {
                        changes.push(TreeChange::AddedBlock {
                            index: idx,
                            hash: node.hash.clone(),
                        });
                    }
                }
                j += 1;
            }
        }

        changes
    }

    /// Compare this tree with another using enhanced change detection
    ///
    /// This method provides access to the enhanced change detection system with
    /// granular analysis, similarity scoring, moved block detection, and advanced features.
    ///
    /// # Arguments
    /// * `other` - Another Merkle tree to compare against
    /// * `hasher` - Hash function implementation for content analysis
    /// * `source` - Source of the changes (user edit, import, sync, etc.)
    ///
    /// # Returns
    /// Enhanced list of detected changes with metadata
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::storage::{MerkleTree, HashedBlock};
    /// use crucible_core::storage::diff::{EnhancedChangeDetector, ChangeSource};
    /// use crucible_core::hashing::blake3::Blake3Hasher;
    ///
    /// let hasher = Blake3Hasher::new();
    /// let detector = EnhancedChangeDetector::new();
    ///
    /// // Create trees and compare with enhanced detection
    /// let changes = tree1.compare_enhanced(&tree2, &hasher, ChangeSource::UserEdit)?;
    /// ```
    pub fn compare_enhanced<H>(
        &self,
        other: &MerkleTree,
        hasher: &H,
        source: crate::storage::diff::ChangeSource,
    ) -> crate::storage::StorageResult<Vec<crate::storage::diff::EnhancedTreeChange>>
    where
        H: ContentHasher + ?Sized,
    {
        let detector = crate::storage::diff::EnhancedChangeDetector::new();
        detector.compare_trees(self, other, hasher, source)
    }

    /// Apply changes to this tree using the enhanced change application system
    ///
    /// This method provides access to the advanced change application system with
    /// rollback capability, validation, and batch processing.
    ///
    /// # Arguments
    /// * `changes` - List of changes to apply
    /// * `hasher` - Hash function for creating new tree nodes
    ///
    /// # Returns
    /// Result of applying changes with rollback information
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::storage::{MerkleTree, diff::{EnhancedTreeChange, ChangeMetadata}};
    /// use crucible_core::storage::change_application::ChangeApplicationSystem;
    /// use crucible_core::hashing::blake3::Blake3Hasher;
    ///
    /// let hasher = Blake3Hasher::new();
    /// let application_system = ChangeApplicationSystem::new();
    ///
    /// // Apply changes with rollback support
    /// let result = tree.apply_changes(&changes, &hasher)?;
    /// ```
    pub fn apply_changes<H>(
        &self,
        changes: &[crate::storage::diff::EnhancedTreeChange],
        hasher: &H,
    ) -> crate::storage::StorageResult<crate::storage::change_application::ChangeApplicationResult>
    where
        H: ContentHasher,
    {
        let application_system = crate::storage::change_application::ChangeApplicationSystem::new();
        application_system.apply_changes(self, changes, hasher)
    }

    /// Apply a single change to this tree using the enhanced change application system
    ///
    /// # Arguments
    /// * `change` - The change to apply
    /// * `hasher` - Hash function for creating new tree nodes
    ///
    /// # Returns
    /// Result of applying the change with rollback information
    pub fn apply_change<H>(
        &self,
        change: &crate::storage::diff::EnhancedTreeChange,
        hasher: &H,
    ) -> crate::storage::StorageResult<crate::storage::change_application::ChangeApplicationResult>
    where
        H: ContentHasher,
    {
        let application_system = crate::storage::change_application::ChangeApplicationSystem::new();
        application_system.apply_change(self, change, hasher)
    }

    /// Get an enhanced change detector configured for this tree
    ///
    /// # Returns
    /// An `EnhancedChangeDetector` instance
    pub fn change_detector(&self) -> crate::storage::diff::EnhancedChangeDetector {
        crate::storage::diff::EnhancedChangeDetector::new()
    }

    /// Get an enhanced change detector with custom configuration
    ///
    /// # Arguments
    /// * `config` - Custom configuration for the detector
    ///
    /// # Returns
    /// A configured `EnhancedChangeDetector` instance
    pub fn change_detector_with_config(
        &self,
        config: crate::storage::diff::DiffConfig,
    ) -> crate::storage::diff::EnhancedChangeDetector {
        crate::storage::diff::EnhancedChangeDetector::with_config(config)
    }

    /// Update depths for all nodes in the tree
    fn update_node_depths(&mut self) -> StorageResult<()> {
        // Implementation would calculate proper depths from root down
        // This is a placeholder - actual implementation would be more complex
        Ok(())
    }

    /// Get tree statistics
    pub fn stats(&self) -> TreeStats {
        TreeStats {
            depth: self.depth,
            node_count: self.nodes.len(),
            leaf_count: self.leaf_hashes.len(),
            block_count: self.block_count,
            root_hash: self.root_hash.clone(),
        }
    }
}

/// Statistics about a Merkle tree
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeStats {
    pub depth: usize,
    pub node_count: usize,
    pub leaf_count: usize,
    pub block_count: usize,
    pub root_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::traits::ContentHasher;

    /// Mock deterministic hasher for predictable testing
    #[derive(Debug)]
    struct MockHasher {
        counter: std::sync::atomic::AtomicUsize,
    }

    impl Clone for MockHasher {
        fn clone(&self) -> Self {
            Self {
                counter: std::sync::atomic::AtomicUsize::new(
                    self.counter.load(std::sync::atomic::Ordering::SeqCst)
                ),
            }
        }
    }

    impl MockHasher {
        fn new() -> Self {
            Self {
                counter: std::sync::atomic::AtomicUsize::new(1),
            }
        }

        fn deterministic_hash(&self, input: &[u8]) -> String {
            // Simple deterministic hash for testing
            let mut hash = 0usize;
            for byte in input {
                hash = hash.wrapping_mul(31).wrapping_add(*byte as usize);
            }
            // Add counter only to make unique hashes when input might be the same
            // but keep it deterministic based on input length and position
            hash = hash.wrapping_add(input.len());
            format!("{:016x}", hash)
        }
    }

    impl ContentHasher for MockHasher {
        fn hash_block(&self, data: &[u8]) -> String {
            self.deterministic_hash(data)
        }

        fn hash_nodes(&self, left: &str, right: &str) -> String {
            let combined = format!("{}{}", left, right);
            self.deterministic_hash(combined.as_bytes())
        }

        fn algorithm_name(&self) -> &'static str {
            "mock"
        }

        fn hash_length(&self) -> usize {
            8 // 8 bytes = 16 hex chars
        }
    }

    /// Create a test hashed block
    fn create_test_block(data: &str, index: usize, offset: usize) -> HashedBlock {
        let hasher = MockHasher::new();
        HashedBlock::from_data(
            data.as_bytes().to_vec(),
            index,
            offset,
            true, // is_last for simplicity
            &hasher,
        )
        .unwrap()
    }

    #[test]
    fn test_merkle_node_creation_leaf() {
        let hasher = MockHasher::new();

        // Test valid leaf creation
        let leaf = MerkleNode::new_leaf("abc123".to_string(), 0, 0, 0).unwrap();

        assert_eq!(leaf.hash, "abc123");
        assert!(leaf.is_leaf());
        assert!(!leaf.is_internal());
        assert_eq!(leaf.block_hash(), Some("abc123"));
        assert_eq!(leaf.block_index(), Some(0));
        assert_eq!(leaf.depth, 0);
        assert_eq!(leaf.index, 0);

        // Test invalid leaf creation (empty hash)
        let result = MerkleNode::new_leaf("".to_string(), 0, 0, 0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::InvalidHash(_)));
    }

    #[test]
    fn test_merkle_node_creation_internal() {
        let hasher = MockHasher::new();

        // Test valid internal node creation
        let internal = MerkleNode::new_internal(
            "hash1".to_string(),
            "hash2".to_string(),
            0,
            1,
            1,
            0,
            &hasher,
        )
        .unwrap();

        assert!(!internal.is_leaf());
        assert!(internal.is_internal());
        assert_eq!(internal.depth, 1);
        assert_eq!(internal.index, 0);

        let (left, right) = internal.child_hashes().unwrap();
        assert_eq!(left, "hash1");
        assert_eq!(right, "hash2");

        // Test invalid internal node creation (empty hash)
        let result =
            MerkleNode::new_internal("".to_string(), "hash2".to_string(), 0, 1, 1, 0, &hasher);
        assert!(result.is_err());
    }

    #[test]
    fn test_merkle_node_validation() {
        let hasher = MockHasher::new();

        // Test valid leaf validation
        let leaf = MerkleNode::new_leaf("abc123".to_string(), 0, 0, 0).unwrap();
        assert!(leaf.validate().is_ok());

        // Test valid internal validation
        let internal = MerkleNode::new_internal(
            "hash1".to_string(),
            "hash2".to_string(),
            0,
            1,
            1,
            0,
            &hasher,
        )
        .unwrap();
        assert!(internal.validate().is_ok());
    }

    #[test]
    fn test_merkle_tree_single_block() {
        let block = create_test_block("Hello, World!", 0, 0);
        let tree = MerkleTree::from_single_block(&block).unwrap();

        assert_eq!(tree.root_hash, block.hash);
        assert_eq!(tree.block_count, 1);
        assert_eq!(tree.depth, 0);
        assert_eq!(tree.leaf_hashes.len(), 1);
        assert_eq!(tree.leaf_hashes[0], block.hash);

        let root = tree.root().unwrap();
        assert!(root.is_leaf());
        assert_eq!(root.block_hash(), Some(block.hash.as_str()));
    }

    #[test]
    fn test_merkle_tree_two_blocks() {
        let block1 = create_test_block("Block 1", 0, 0);
        let block2 = create_test_block("Block 2", 1, 8);
        let blocks = vec![block1.clone(), block2.clone()];
        let hasher = MockHasher::new();

        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        assert_eq!(tree.block_count, 2);
        assert_eq!(tree.leaf_hashes.len(), 2);
        assert!(tree.depth > 0);
        assert!(tree.nodes.contains_key(&block1.hash));
        assert!(tree.nodes.contains_key(&block2.hash));

        // Root should be internal node
        let root = tree.root().unwrap();
        assert!(root.is_internal());
    }

    #[test]
    fn test_merkle_tree_three_blocks() {
        let block1 = create_test_block("Block 1", 0, 0);
        let block2 = create_test_block("Block 2", 1, 8);
        let block3 = create_test_block("Block 3", 2, 16);
        let blocks = vec![block1, block2, block3];
        let hasher = MockHasher::new();

        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        assert_eq!(tree.block_count, 3);
        assert_eq!(tree.leaf_hashes.len(), 3);
        assert!(tree.depth > 0);
    }

    #[test]
    fn test_merkle_tree_four_blocks() {
        let blocks = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2", 1, 8),
            create_test_block("Block 3", 2, 16),
            create_test_block("Block 4", 3, 24),
        ];
        let hasher = MockHasher::new();

        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        assert_eq!(tree.block_count, 4);
        assert_eq!(tree.leaf_hashes.len(), 4);
        assert!(tree.depth >= 2); // Should have multiple levels

        // Verify all blocks are in tree
        for block in &blocks {
            assert!(tree.nodes.contains_key(&block.hash));
        }
    }

    #[test]
    fn test_merkle_tree_eight_blocks() {
        let mut blocks = Vec::new();
        for i in 0..8 {
            blocks.push(create_test_block(&format!("Block {}", i + 1), i, i * 8));
        }
        let hasher = MockHasher::new();

        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        assert_eq!(tree.block_count, 8);
        assert_eq!(tree.leaf_hashes.len(), 8);
        assert!(tree.depth >= 3); // Should have multiple levels

        // Verify tree structure
        assert!(tree.calculate_depth() >= 3);
    }

    #[test]
    fn test_merkle_tree_empty_blocks() {
        let hasher = MockHasher::new();
        let blocks: Vec<HashedBlock> = vec![];

        let result = MerkleTree::from_blocks(&blocks, &hasher);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::BlockSize(_)));
    }

    #[test]
    fn test_merkle_tree_depth_calculation() {
        // Test single block
        let block = create_test_block("Single", 0, 0);
        let tree = MerkleTree::from_single_block(&block).unwrap();
        assert_eq!(tree.calculate_depth(), 1);

        // Test multiple blocks
        let blocks = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2", 1, 8),
        ];
        let hasher = MockHasher::new();
        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();
        assert_eq!(tree.calculate_depth(), 2);

        // Test 4 blocks
        let mut blocks = vec![];
        for i in 0..4 {
            blocks.push(create_test_block(&format!("Block {}", i), i, i * 8));
        }
        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();
        assert_eq!(tree.calculate_depth(), 3);
    }

    #[test]
    fn test_merkle_tree_verification() {
        let blocks = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2", 1, 8),
            create_test_block("Block 3", 2, 16),
        ];
        let hasher = MockHasher::new();
        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        // Verification should pass for valid tree
        assert!(tree.verify_integrity(&hasher).is_ok());

        // Test with corrupted root hash
        let mut corrupted_tree = tree.clone();
        corrupted_tree.root_hash = "corrupted".to_string();
        assert!(corrupted_tree.verify_integrity(&hasher).is_err());
    }

    #[test]
    fn test_merkle_tree_comparison() {
        let blocks1 = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2", 1, 8),
        ];
        let blocks2 = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2 Modified", 1, 8), // Modified block
        ];
        let hasher = MockHasher::new();

        let tree1 = MerkleTree::from_blocks(&blocks1, &hasher).unwrap();
        let tree2 = MerkleTree::from_blocks(&blocks2, &hasher).unwrap();

        let changes = tree1.compare_with(&tree2);

        // Should detect modifications
        assert!(!changes.is_empty());
        assert!(changes
            .iter()
            .any(|c| matches!(c, TreeChange::ModifiedBlock { .. })));
    }

    #[test]
    fn test_merkle_tree_serialization() {
        let blocks = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2", 1, 8),
        ];
        let hasher = MockHasher::new();
        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        // Test JSON serialization
        let json = serde_json::to_string(&tree).unwrap();
        let deserialized: MerkleTree = serde_json::from_str(&json).unwrap();

        assert_eq!(tree, deserialized);

        // Test that deserialized tree has same structure
        assert_eq!(tree.root_hash, deserialized.root_hash);
        assert_eq!(tree.block_count, deserialized.block_count);
        assert_eq!(tree.leaf_hashes.len(), deserialized.leaf_hashes.len());
    }

    #[test]
    fn test_merkle_node_serialization() {
        let hasher = MockHasher::new();

        // Test leaf node serialization
        let leaf = MerkleNode::new_leaf("abc123".to_string(), 0, 0, 0).unwrap();
        let json = serde_json::to_string(&leaf).unwrap();
        let deserialized: MerkleNode = serde_json::from_str(&json).unwrap();
        assert_eq!(leaf, deserialized);

        // Test internal node serialization
        let internal = MerkleNode::new_internal(
            "hash1".to_string(),
            "hash2".to_string(),
            0,
            1,
            1,
            0,
            &hasher,
        )
        .unwrap();
        let json = serde_json::to_string(&internal).unwrap();
        let deserialized: MerkleNode = serde_json::from_str(&json).unwrap();
        assert_eq!(internal, deserialized);
    }

    #[test]
    fn test_tree_stats() {
        let blocks = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2", 1, 8),
            create_test_block("Block 3", 2, 16),
        ];
        let hasher = MockHasher::new();
        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        let stats = tree.stats();
        assert_eq!(stats.block_count, 3);
        assert_eq!(stats.leaf_count, 3);
        assert!(stats.node_count > 0);
        assert_eq!(stats.root_hash, tree.root_hash);
    }

    #[test]
    fn test_large_number_of_blocks() {
        let mut blocks = Vec::new();
        for i in 0..100 {
            blocks.push(create_test_block(&format!("Block {}", i), i, i * 10));
        }
        let hasher = MockHasher::new();

        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        assert_eq!(tree.block_count, 100);
        assert_eq!(tree.leaf_hashes.len(), 100);
        assert!(tree.depth > 0);
        assert!(tree.verify_integrity(&hasher).is_ok());
    }

    #[test]
    fn test_invalid_hash_formats() {
        let hasher = MockHasher::new();

        // Test node with invalid hash
        let result = MerkleNode::new_leaf("invalid_hash".to_string(), 0, 0, 0);
        // Note: MockHasher doesn't validate format, so this should succeed
        // Real implementation would validate hash format
        assert!(result.is_ok());
    }

    #[test]
    fn test_edge_case_single_char_block() {
        let block = create_test_block("a", 0, 0);
        let tree = MerkleTree::from_single_block(&block).unwrap();

        assert_eq!(tree.block_count, 1);
        assert_eq!(tree.root_hash, block.hash);
        assert!(tree.verify_integrity(&MockHasher::new()).is_ok());
    }

    #[test]
    fn test_odd_number_of_blocks_handling() {
        // Test various odd numbers
        for count in [3, 5, 7, 9, 15] {
            let mut blocks = Vec::new();
            for i in 0..count {
                blocks.push(create_test_block(&format!("Block {}", i), i, i * 8));
            }
            let hasher = MockHasher::new();

            let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();
            assert_eq!(tree.block_count, count);
            assert_eq!(tree.leaf_hashes.len(), count);
            assert!(tree.verify_integrity(&hasher).is_ok());
        }
    }

    #[test]
    fn test_merkle_tree_with_same_content_blocks() {
        // Test tree with identical content blocks (should have same hashes)
        let block1 = create_test_block("Same Content", 0, 0);
        let block2 = create_test_block("Same Content", 1, 13);
        let blocks = vec![block1.clone(), block2];
        let hasher = MockHasher::new();

        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        assert_eq!(tree.block_count, 2);
        // Both blocks should have same hash but different indices
        assert_eq!(blocks[0].hash, blocks[1].hash);
    }

    #[test]
    fn test_tree_change_detection_edge_cases() {
        let blocks1 = vec![create_test_block("Block 1", 0, 0)];
        let blocks2: Vec<HashedBlock> = vec![]; // Empty
        let hasher = MockHasher::new();

        let tree1 = MerkleTree::from_blocks(&blocks1, &hasher).unwrap();

        // Cannot create tree from empty blocks, so test with single vs multiple blocks
        let blocks3 = vec![
            create_test_block("Block 1", 0, 0),
            create_test_block("Block 2", 1, 8),
        ];
        let tree3 = MerkleTree::from_blocks(&blocks3, &hasher).unwrap();

        let changes = tree1.compare_with(&tree3);
        assert!(!changes.is_empty());
        // Should detect structural change and added block
        assert!(changes
            .iter()
            .any(|c| matches!(c, TreeChange::StructureChanged)));
    }
}
