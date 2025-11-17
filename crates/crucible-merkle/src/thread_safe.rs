//! Thread-safe wrapper for concurrent Merkle tree access
//!
//! This module provides a thread-safe wrapper around `HybridMerkleTree` that enables
//! safe concurrent access from multiple threads using `Arc<RwLock<T>>`.
//!
//! ## Design Goals
//!
//! - **Safe concurrency**: Multiple readers or single writer access pattern
//! - **Low contention**: Read-heavy workloads benefit from RwLock's multiple readers
//! - **Ergonomic API**: Convenient methods for common operations
//! - **Clone-friendly**: `Arc` enables cheap cloning for sharing across threads
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use crucible_core::merkle::{ThreadSafeMerkleTree, VirtualizationConfig};
//!
//! let tree = ThreadSafeMerkleTree::new_auto(&doc);
//!
//! // Clone for sharing across threads
//! let tree_clone = tree.clone();
//!
//! // Read access (multiple readers allowed)
//! let hash = tree.read_hash().unwrap();
//!
//! // Write access (exclusive)
//! tree.update(&new_doc).unwrap();
//! ```

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::sync::Arc;

use crate::{HybridMerkleTree, NodeHash, VirtualizationConfig};
use crucible_parser::types::ParsedNote;

/// Thread-safe wrapper around HybridMerkleTree
///
/// This type uses `Arc<RwLock<HybridMerkleTree>>` to provide safe concurrent access:
/// - Multiple threads can read simultaneously
/// - Only one thread can write at a time
/// - Uses `parking_lot::RwLock` for better performance than std RwLock
///
/// ## Performance Characteristics
///
/// - **Read operations**: Lock-free for multiple readers (high concurrency)
/// - **Write operations**: Exclusive lock required (serialized)
/// - **Cloning**: Cheap (just increments Arc refcount)
///
/// ## Example
///
/// ```rust,ignore
/// use std::thread;
///
/// let tree = ThreadSafeMerkleTree::new_auto(&doc);
///
/// // Share across threads
/// let handles: Vec<_> = (0..4).map(|_| {
///     let tree_clone = tree.clone();
///     thread::spawn(move || {
///         // Read access from multiple threads
///         let hash = tree_clone.read_hash().unwrap();
///     })
/// }).collect();
///
/// for handle in handles {
///     handle.join().unwrap();
/// }
/// ```
#[derive(Clone)]
pub struct ThreadSafeMerkleTree {
    inner: Arc<RwLock<HybridMerkleTree>>,
}

impl ThreadSafeMerkleTree {
    /// Create a new thread-safe Merkle tree without virtualization
    ///
    /// This maintains backward compatibility with the original API.
    ///
    /// # Arguments
    ///
    /// * `doc` - The parsed document
    ///
    /// # Returns
    ///
    /// A new thread-safe Merkle tree
    pub fn new(doc: &ParsedNote) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HybridMerkleTree::from_document(doc))),
        }
    }

    /// Create a new thread-safe Merkle tree with custom configuration
    ///
    /// # Arguments
    ///
    /// * `doc` - The parsed document
    /// * `config` - Virtualization configuration
    ///
    /// # Returns
    ///
    /// A new thread-safe Merkle tree with the specified configuration
    pub fn new_with_config(doc: &ParsedNote, config: &VirtualizationConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HybridMerkleTree::from_document_with_config(
                doc, config,
            ))),
        }
    }

    /// Create a new thread-safe Merkle tree with automatic virtualization
    ///
    /// This is the recommended constructor for most use cases.
    ///
    /// # Arguments
    ///
    /// * `doc` - The parsed document
    ///
    /// # Returns
    ///
    /// A new thread-safe Merkle tree with default virtualization config
    pub fn new_auto(doc: &ParsedNote) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HybridMerkleTree::from_document_auto(doc))),
        }
    }

    /// Create a thread-safe wrapper around an existing Merkle tree
    ///
    /// # Arguments
    ///
    /// * `tree` - The Merkle tree to wrap
    ///
    /// # Returns
    ///
    /// A new thread-safe wrapper
    pub fn from_tree(tree: HybridMerkleTree) -> Self {
        Self {
            inner: Arc::new(RwLock::new(tree)),
        }
    }

    /// Get a read lock on the tree
    ///
    /// Multiple readers can hold this lock simultaneously.
    ///
    /// # Returns
    ///
    /// A read guard that dereferences to `HybridMerkleTree`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let guard = tree.read();
    /// println!("Hash: {}", guard.root_hash);
    /// ```
    pub fn read(&self) -> RwLockReadGuard<'_, HybridMerkleTree> {
        self.inner.read()
    }

    /// Get a write lock on the tree
    ///
    /// Only one writer can hold this lock at a time.
    ///
    /// # Returns
    ///
    /// A write guard that dereferences to `HybridMerkleTree`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut guard = tree.write();
    /// // Modify the tree...
    /// ```
    pub fn write(&self) -> RwLockWriteGuard<'_, HybridMerkleTree> {
        self.inner.write()
    }

    /// Get the root hash (convenience method)
    ///
    /// This acquires a read lock internally.
    ///
    /// # Returns
    ///
    /// The root hash of the Merkle tree
    pub fn read_hash(&self) -> NodeHash {
        self.inner.read().root_hash
    }

    /// Get the total block count (convenience method)
    ///
    /// This acquires a read lock internally.
    ///
    /// # Returns
    ///
    /// The total number of blocks in the tree
    pub fn read_block_count(&self) -> usize {
        self.inner.read().total_blocks
    }

    /// Get the section count (convenience method)
    ///
    /// This acquires a read lock internally.
    ///
    /// # Returns
    ///
    /// The number of sections (virtual or real)
    pub fn read_section_count(&self) -> usize {
        self.inner.read().section_count()
    }

    /// Check if the tree is virtualized (convenience method)
    ///
    /// This acquires a read lock internally.
    ///
    /// # Returns
    ///
    /// `true` if the tree uses virtualization, `false` otherwise
    pub fn is_virtualized(&self) -> bool {
        self.inner.read().is_virtualized
    }

    /// Update the tree with a new document
    ///
    /// This replaces the entire tree with a new one built from the document.
    /// The virtualization configuration is preserved.
    ///
    /// # Arguments
    ///
    /// * `doc` - The new parsed document
    ///
    /// # Returns
    ///
    /// The previous root hash (useful for change detection)
    pub fn update(&self, doc: &ParsedNote) -> NodeHash {
        let mut guard = self.write();
        let old_hash = guard.root_hash;

        // Preserve virtualization config
        let config = if guard.is_virtualized {
            VirtualizationConfig::default()
        } else {
            VirtualizationConfig::disabled()
        };

        *guard = HybridMerkleTree::from_document_with_config(doc, &config);
        old_hash
    }

    /// Update the tree with a new document and custom config
    ///
    /// This replaces the entire tree with a new one built from the document
    /// using the specified configuration.
    ///
    /// # Arguments
    ///
    /// * `doc` - The new parsed document
    /// * `config` - The virtualization configuration
    ///
    /// # Returns
    ///
    /// The previous root hash
    pub fn update_with_config(&self, doc: &ParsedNote, config: &VirtualizationConfig) -> NodeHash {
        let mut guard = self.write();
        let old_hash = guard.root_hash;
        *guard = HybridMerkleTree::from_document_with_config(doc, config);
        old_hash
    }

    /// Clone the underlying tree (creates a new independent copy)
    ///
    /// This is different from cloning the ThreadSafeMerkleTree itself,
    /// which just increments the Arc refcount. This method creates a
    /// complete new copy of the tree data.
    ///
    /// # Returns
    ///
    /// A new independent Merkle tree
    pub fn clone_tree(&self) -> HybridMerkleTree {
        self.inner.read().clone()
    }

    /// Get statistics about the tree
    ///
    /// # Returns
    ///
    /// A snapshot of tree statistics
    pub fn stats(&self) -> TreeStats {
        let guard = self.read();
        TreeStats {
            root_hash: guard.root_hash,
            total_blocks: guard.total_blocks,
            section_count: guard.section_count(),
            real_section_count: guard.real_section_count(),
            is_virtualized: guard.is_virtualized,
            virtual_section_count: guard.virtual_sections.as_ref().map(|v| v.len()),
        }
    }
}

impl std::fmt::Debug for ThreadSafeMerkleTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let guard = self.read();
        f.debug_struct("ThreadSafeMerkleTree")
            .field("root_hash", &guard.root_hash)
            .field("total_blocks", &guard.total_blocks)
            .field("section_count", &guard.section_count())
            .field("is_virtualized", &guard.is_virtualized)
            .finish()
    }
}

/// Statistics snapshot for a Merkle tree
#[derive(Debug, Clone, PartialEq)]
pub struct TreeStats {
    /// Root hash of the tree
    pub root_hash: NodeHash,
    /// Total number of content blocks
    pub total_blocks: usize,
    /// Number of sections (virtual or real)
    pub section_count: usize,
    /// Actual number of real sections
    pub real_section_count: usize,
    /// Whether virtualization is enabled
    pub is_virtualized: bool,
    /// Number of virtual sections (if virtualized)
    pub virtual_section_count: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_parser::types::{Heading, NoteContent, Paragraph};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;

    fn build_test_doc() -> ParsedNote {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("test.md");
        doc.content = NoteContent::default();

        doc.content.headings = vec![
            Heading {
                level: 1,
                text: "Section 1".to_string(),
                offset: 0,
                id: Some("s1".to_string()),
            },
            Heading {
                level: 2,
                text: "Section 2".to_string(),
                offset: 50,
                id: Some("s2".to_string()),
            },
        ];

        doc.content.paragraphs = vec![
            Paragraph::new("Content 1".to_string(), 10),
            Paragraph::new("Content 2".to_string(), 60),
        ];

        doc
    }

    #[test]
    fn test_thread_safe_tree_creation() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        // Should be able to read
        let hash = tree.read_hash();
        assert!(!hash.is_zero());
    }

    #[test]
    fn test_thread_safe_tree_new_auto() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new_auto(&doc);

        // Should be created successfully
        assert!(!tree.is_virtualized());
        assert_eq!(tree.read_block_count(), 2);
    }

    #[test]
    fn test_thread_safe_tree_with_config() {
        let doc = build_test_doc();
        let config = VirtualizationConfig::disabled();
        let tree = ThreadSafeMerkleTree::new_with_config(&doc, &config);

        assert!(!tree.is_virtualized());
    }

    #[test]
    fn test_read_access() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        // Get read lock
        let guard = tree.read();
        assert_eq!(guard.total_blocks, 2);
        assert_eq!(guard.sections.len(), 3);
    }

    #[test]
    fn test_write_access() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        // Get write lock
        let mut guard = tree.write();
        let old_hash = guard.root_hash;

        // Modify something (in real code, you'd modify the tree)
        assert_eq!(guard.total_blocks, 2);

        drop(guard);

        // Verify we can read again
        let new_hash = tree.read_hash();
        assert_eq!(old_hash, new_hash);
    }

    #[test]
    fn test_convenience_methods() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        // Test convenience methods
        assert!(!tree.read_hash().is_zero());
        assert_eq!(tree.read_block_count(), 2);
        assert_eq!(tree.read_section_count(), 3);
        assert!(!tree.is_virtualized());
    }

    #[test]
    fn test_update() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        let old_hash = tree.read_hash();

        // Create modified document
        let mut new_doc = build_test_doc();
        new_doc.content.paragraphs[0].content = "Modified content".to_string();

        // Update the tree
        let returned_hash = tree.update(&new_doc);
        assert_eq!(returned_hash, old_hash);

        // Hash should have changed
        let new_hash = tree.read_hash();
        assert_ne!(new_hash, old_hash);
    }

    #[test]
    fn test_update_with_config() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        let mut new_doc = build_test_doc();
        new_doc.content.paragraphs[0].content = "Modified".to_string();

        let config = VirtualizationConfig::disabled();
        tree.update_with_config(&new_doc, &config);

        assert!(!tree.is_virtualized());
    }

    #[test]
    fn test_clone_tree() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        let cloned = tree.clone_tree();
        assert_eq!(cloned.root_hash, tree.read_hash());
        assert_eq!(cloned.total_blocks, tree.read_block_count());
    }

    #[test]
    fn test_stats() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        let stats = tree.stats();
        assert_eq!(stats.total_blocks, 2);
        assert_eq!(stats.section_count, 3);
        assert_eq!(stats.real_section_count, 3);
        assert!(!stats.is_virtualized);
        assert_eq!(stats.virtual_section_count, None);
    }

    #[test]
    fn test_thread_safe_clone() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        // Clone should share the same underlying data
        let tree_clone = tree.clone();

        let hash1 = tree.read_hash();
        let hash2 = tree_clone.read_hash();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_concurrent_reads() {
        let doc = build_test_doc();
        let tree = Arc::new(ThreadSafeMerkleTree::new(&doc));

        let mut handles = vec![];

        // Spawn multiple reader threads
        for _ in 0..10 {
            let tree_clone = Arc::clone(&tree);
            let handle = thread::spawn(move || {
                // Each thread reads the hash multiple times
                for _ in 0..100 {
                    let _hash = tree_clone.read_hash();
                    let _count = tree_clone.read_block_count();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Tree should still be valid
        assert!(!tree.read_hash().is_zero());
    }

    #[test]
    fn test_concurrent_read_write() {
        let doc = build_test_doc();
        let tree = Arc::new(ThreadSafeMerkleTree::new(&doc));

        let mut handles = vec![];

        // Spawn reader threads
        for _ in 0..5 {
            let tree_clone = Arc::clone(&tree);
            let handle = thread::spawn(move || {
                for _ in 0..50 {
                    let _hash = tree_clone.read_hash();
                }
            });
            handles.push(handle);
        }

        // Spawn writer threads
        for i in 0..3 {
            let tree_clone = Arc::clone(&tree);
            let handle = thread::spawn(move || {
                for j in 0..20 {
                    let mut new_doc = build_test_doc();
                    new_doc.content.paragraphs[0].content = format!("Content {} {}", i, j);
                    tree_clone.update(&new_doc);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Tree should still be valid
        assert!(!tree.read_hash().is_zero());
        assert_eq!(tree.read_block_count(), 2);
    }

    #[test]
    fn test_debug_format() {
        let doc = build_test_doc();
        let tree = ThreadSafeMerkleTree::new(&doc);

        let debug_str = format!("{:?}", tree);
        assert!(debug_str.contains("ThreadSafeMerkleTree"));
        assert!(debug_str.contains("root_hash"));
        assert!(debug_str.contains("total_blocks"));
    }

    #[test]
    fn test_from_tree() {
        let doc = build_test_doc();
        let hybrid_tree = HybridMerkleTree::from_document(&doc);
        let original_hash = hybrid_tree.root_hash;

        let thread_safe = ThreadSafeMerkleTree::from_tree(hybrid_tree);

        assert_eq!(thread_safe.read_hash(), original_hash);
    }

    #[test]
    fn test_stats_with_virtualization() {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("large.md");
        doc.content = NoteContent::default();

        // Create enough sections to trigger virtualization
        for i in 0..120 {
            doc.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });
            doc.content
                .paragraphs
                .push(Paragraph::new("Content".to_string(), i * 100 + 10));
        }

        let tree = ThreadSafeMerkleTree::new_auto(&doc);
        let stats = tree.stats();

        assert!(stats.is_virtualized);
        assert!(stats.virtual_section_count.is_some());
        assert!(stats.section_count < stats.real_section_count);
    }
}
