use serde::{Deserialize, Serialize};

use crate::merkle::{NodeHash, VirtualSection, VirtualizationConfig};
use crucible_parser::types::{BlockHash, Heading, Paragraph, ParsedNote};

/// Hybrid Merkle tree that groups note content into semantic sections and
/// stores block-level hashing inside each section.
///
/// ## Hash Strategy
///
/// - **Content hashes** (leaves): Full 32-byte `BlockHash` for content integrity
/// - **Tree hashes** (nodes): Compact 16-byte `NodeHash` for structure and change detection
///
/// This dual-hash approach optimizes memory usage while maintaining content integrity.
///
/// ## Virtualization
///
/// For documents with many sections (>100 by default), sections are automatically
/// grouped into virtual sections to prevent memory exhaustion while maintaining
/// performance and accuracy.
#[derive(Debug, Clone)]
pub struct HybridMerkleTree {
    pub root_hash: NodeHash,
    pub sections: Vec<SectionNode>,
    pub total_blocks: usize,
    /// Virtual sections for large documents (None if not virtualized)
    pub virtual_sections: Option<Vec<VirtualSection>>,
    /// Whether this tree uses virtualization
    pub is_virtualized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SectionNode {
    pub heading: Option<HeadingSummary>,
    pub depth: u8,
    pub binary_tree: BinaryMerkleTree,
    pub block_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeadingSummary {
    pub text: String,
    pub level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinaryMerkleTree {
    pub root_hash: NodeHash,
    pub nodes: Vec<MerkleNode>,
    pub height: usize,
    pub leaf_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MerkleNode {
    /// Leaf node storing content hash (32 bytes for integrity)
    Leaf {
        hash: BlockHash,
        block_index: usize,
    },
    /// Internal node with compact hash (16 bytes for efficiency)
    Internal {
        hash: NodeHash,
        left: usize,
        right: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridDiff {
    pub root_hash_changed: bool,
    pub changed_sections: Vec<SectionChange>,
    pub added_sections: usize,
    pub removed_sections: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionChange {
    pub section_index: usize,
    pub heading: Option<HeadingSummary>,
}

impl HybridMerkleTree {
    /// Create a Merkle tree from a document without virtualization
    ///
    /// This is the default method that maintains backward compatibility.
    /// For large documents, use `from_document_with_config()` instead.
    pub fn from_document(doc: &ParsedNote) -> Self {
        Self::from_document_with_config(doc, &VirtualizationConfig::disabled())
    }

    /// Create a Merkle tree from a document with custom virtualization config
    ///
    /// This method enables automatic virtualization for large documents based
    /// on the provided configuration.
    ///
    /// # Arguments
    ///
    /// * `doc` - The parsed document
    /// * `config` - Virtualization configuration
    ///
    /// # Returns
    ///
    /// A new HybridMerkleTree, potentially with virtual sections
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = VirtualizationConfig::default();
    /// let tree = HybridMerkleTree::from_document_with_config(&doc, &config);
    /// ```
    pub fn from_document_with_config(doc: &ParsedNote, config: &VirtualizationConfig) -> Self {
        let (sections, total_blocks) = build_sections(doc);

        // Check if virtualization should be enabled
        let should_virtualize = VirtualSection::should_virtualize(sections.len(), config);

        if should_virtualize {
            // Create virtual sections
            let virtual_sections = VirtualSection::virtualize(&sections, config);

            // Compute root hash from virtual section hashes
            let virtual_hashes: Vec<NodeHash> = virtual_sections
                .iter()
                .map(|vs| vs.hash)
                .collect();
            let root_hash = NodeHash::combine_many(&virtual_hashes);

            Self {
                root_hash,
                sections,
                total_blocks,
                virtual_sections: Some(virtual_sections),
                is_virtualized: true,
            }
        } else {
            // No virtualization needed
            let section_hashes: Vec<NodeHash> = sections
                .iter()
                .map(|section| section.binary_tree.root_hash)
                .collect();
            let root_hash = NodeHash::combine_many(&section_hashes);

            Self {
                root_hash,
                sections,
                total_blocks,
                virtual_sections: None,
                is_virtualized: false,
            }
        }
    }

    /// Create a tree with automatic virtualization using default config
    ///
    /// This is a convenience method that uses the default virtualization
    /// configuration (threshold: 100 sections).
    pub fn from_document_auto(doc: &ParsedNote) -> Self {
        Self::from_document_with_config(doc, &VirtualizationConfig::default())
    }

    /// Get the number of sections (virtual or real)
    pub fn section_count(&self) -> usize {
        if self.is_virtualized {
            self.virtual_sections.as_ref().map_or(0, |vs| vs.len())
        } else {
            self.sections.len()
        }
    }

    /// Get the actual (real) section count
    pub fn real_section_count(&self) -> usize {
        self.sections.len()
    }

    /// Compute differences between two Merkle trees
    ///
    /// This uses an optimized O(n) algorithm that pre-allocates capacity
    /// and only clones when necessary.
    ///
    /// # Performance
    ///
    /// - **Time complexity**: O(n) where n is the number of sections
    /// - **Space complexity**: O(k) where k is the number of changes
    /// - **Optimizations**: Pre-allocated vector, early termination on hash match
    #[inline]
    pub fn diff(&self, other: &HybridMerkleTree) -> HybridDiff {
        // Early exit if trees are identical
        if self.root_hash == other.root_hash {
            return HybridDiff {
                root_hash_changed: false,
                changed_sections: Vec::new(),
                added_sections: 0,
                removed_sections: 0,
            };
        }

        // Pre-allocate with worst-case capacity (min of both lengths)
        let capacity = self.sections.len().min(other.sections.len());
        let mut changed_sections = Vec::with_capacity(capacity);

        // Optimized comparison using zip (O(n))
        for (idx, (left, right)) in self.sections.iter().zip(other.sections.iter()).enumerate() {
            if left.binary_tree.root_hash != right.binary_tree.root_hash {
                changed_sections.push(SectionChange {
                    section_index: idx,
                    heading: right.heading.clone(),
                });
            }
        }

        HybridDiff {
            root_hash_changed: true,
            changed_sections,
            added_sections: other.sections.len().saturating_sub(self.sections.len()),
            removed_sections: self.sections.len().saturating_sub(other.sections.len()),
        }
    }
}

impl Default for HybridMerkleTree {
    /// Creates an empty Merkle tree with zero hash
    ///
    /// This is primarily useful for testing and initialization scenarios.
    fn default() -> Self {
        Self {
            root_hash: NodeHash::zero(),
            sections: vec![SectionNode {
                heading: None,
                depth: 0,
                binary_tree: BinaryMerkleTree::empty(),
                block_count: 0,
            }],
            total_blocks: 0,
            virtual_sections: None,
            is_virtualized: false,
        }
    }
}

impl BinaryMerkleTree {
    pub fn empty() -> Self {
        Self {
            root_hash: NodeHash::zero(),
            nodes: Vec::new(),
            height: 0,
            leaf_count: 0,
        }
    }

    pub fn from_blocks(blocks: &[(usize, BlockHash)]) -> Self {
        if blocks.is_empty() {
            return Self::empty();
        }

        let mut nodes = Vec::new();
        let mut current_level = Vec::new();

        // Create leaf nodes with BlockHash for content integrity
        for (index, hash) in blocks {
            let node_index = nodes.len();
            nodes.push(MerkleNode::Leaf {
                hash: *hash,
                block_index: *index,
            });
            current_level.push(node_index);
        }

        let mut height = 0;
        while current_level.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in current_level.chunks(2) {
                let left_idx = chunk[0];
                let right_idx = if chunk.len() == 2 { chunk[1] } else { chunk[0] };

                // Combine hashes: convert BlockHash to NodeHash for internal nodes
                let combined = match (&nodes[left_idx], &nodes[right_idx]) {
                    (MerkleNode::Leaf { hash: left, .. }, MerkleNode::Leaf { hash: right, .. }) => {
                        // Convert BlockHash to NodeHash for tree structure
                        let left_node = NodeHash::from_content(left.as_bytes());
                        let right_node = NodeHash::from_content(right.as_bytes());
                        NodeHash::combine(&left_node, &right_node)
                    }
                    (MerkleNode::Leaf { hash: left, .. }, MerkleNode::Internal { hash: right, .. }) => {
                        let left_node = NodeHash::from_content(left.as_bytes());
                        NodeHash::combine(&left_node, right)
                    }
                    (MerkleNode::Internal { hash: left, .. }, MerkleNode::Leaf { hash: right, .. }) => {
                        let right_node = NodeHash::from_content(right.as_bytes());
                        NodeHash::combine(left, &right_node)
                    }
                    (MerkleNode::Internal { hash: left, .. }, MerkleNode::Internal { hash: right, .. }) => {
                        NodeHash::combine(left, right)
                    }
                };

                let node_index = nodes.len();
                nodes.push(MerkleNode::Internal {
                    hash: combined,
                    left: left_idx,
                    right: right_idx,
                });
                next_level.push(node_index);
            }

            current_level = next_level;
            height += 1;
        }

        let root_index = current_level[0];
        let root_hash = match &nodes[root_index] {
            MerkleNode::Leaf { hash, .. } => NodeHash::from_content(hash.as_bytes()),
            MerkleNode::Internal { hash, .. } => *hash,
        };

        Self {
            root_hash,
            nodes,
            height,
            leaf_count: blocks.len(),
        }
    }
}

impl HybridDiff {
    pub fn is_empty(&self) -> bool {
        !self.root_hash_changed
            && self.changed_sections.is_empty()
            && self.added_sections == 0
            && self.removed_sections == 0
    }
}

// Removed hash() method - leaves and internals have different hash types now

fn build_sections(doc: &ParsedNote) -> (Vec<SectionNode>, usize) {
    let mut nodes = Vec::new();

    for heading in &doc.content.headings {
        nodes.push((heading.offset, NodeRef::Heading(heading)));
    }
    for paragraph in &doc.content.paragraphs {
        nodes.push((paragraph.offset, NodeRef::Paragraph(paragraph)));
    }

    nodes.sort_by_key(|(offset, _)| *offset);

    let mut sections = Vec::new();
    let mut stack: Vec<SectionBuilder> = vec![SectionBuilder::root()];
    let mut block_index = 0;

    for (_, node) in nodes {
        match node {
            NodeRef::Heading(heading) => {
                close_sections_until(&mut stack, heading.level, &mut sections);
                stack.push(SectionBuilder::from_heading(heading));
            }
            NodeRef::Paragraph(paragraph) => {
                if paragraph.content.trim().is_empty() {
                    continue;
                }

                if let Some(builder) = stack.last_mut() {
                    builder.add_block(block_index, paragraph.content.clone());
                    block_index += 1;
                }
            }
        }
    }

    while let Some(section) = stack.pop() {
        sections.push(section.into_section());
    }

    sections.reverse();
    (sections, block_index)
}

fn close_sections_until(
    stack: &mut Vec<SectionBuilder>,
    target_level: u8,
    finished: &mut Vec<SectionNode>,
) {
    while let Some(current) = stack.last() {
        if current.depth == 0 || current.depth < target_level {
            break;
        }
        let section = stack.pop().unwrap().into_section();
        finished.push(section);
    }
}

// aggregate_hashes and combine_pair removed - now using NodeHash::combine_many

fn hash_block_content(content: &str) -> BlockHash {
    let digest = blake3::hash(content.as_bytes());
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(digest.as_bytes());
    BlockHash::new(bytes)
}

enum NodeRef<'a> {
    Heading(&'a Heading),
    Paragraph(&'a Paragraph),
}

struct SectionBuilder {
    heading: Option<HeadingSummary>,
    depth: u8,
    blocks: Vec<(usize, String)>,
}

impl SectionBuilder {
    fn root() -> Self {
        Self {
            heading: None,
            depth: 0,
            blocks: Vec::new(),
        }
    }

    fn from_heading(heading: &Heading) -> Self {
        Self {
            heading: Some(HeadingSummary {
                text: heading.text.clone(),
                level: heading.level,
            }),
            depth: heading.level,
            blocks: Vec::new(),
        }
    }

    fn add_block(&mut self, index: usize, content: String) {
        self.blocks.push((index, content));
    }

    fn into_section(mut self) -> SectionNode {
        let hashed_blocks: Vec<(usize, BlockHash)> = self
            .blocks
            .drain(..)
            .map(|(idx, content)| (idx, hash_block_content(&content)))
            .collect();

        let binary_tree = BinaryMerkleTree::from_blocks(&hashed_blocks);
        SectionNode {
            heading: self.heading,
            depth: self.depth,
            block_count: hashed_blocks.len(),
            binary_tree,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_parser::types::{NoteContent, Heading, Paragraph};
    use std::path::PathBuf;

    fn build_document() -> ParsedNote {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("sample.md");
        doc.content = NoteContent::default();

        doc.content.headings = vec![
            Heading {
                level: 1,
                text: "Intro".to_string(),
                offset: 0,
                id: Some("intro".to_string()),
            },
            Heading {
                level: 2,
                text: "Details".to_string(),
                offset: 50,
                id: Some("details".to_string()),
            },
        ];

        doc.content.paragraphs = vec![
            Paragraph::new("Intro paragraph".to_string(), 10),
            Paragraph::new("Detail paragraph".to_string(), 60),
        ];

        doc
    }

    #[test]
    fn builds_sections_from_document() {
        let doc = build_document();
        let tree = HybridMerkleTree::from_document(&doc);

        assert_eq!(tree.total_blocks, 2);
        assert_eq!(tree.sections.len(), 3); // root + two headings
        assert!(!tree.root_hash.is_zero());
    }

    #[test]
    fn diff_identifies_changed_sections() {
        let doc_a = build_document();
        let mut doc_b = build_document();
        doc_b.content.paragraphs[1].content = "Detail paragraph updated".to_string();

        let tree_a = HybridMerkleTree::from_document(&doc_a);
        let tree_b = HybridMerkleTree::from_document(&doc_b);

        let diff = tree_a.diff(&tree_b);
        assert!(diff.root_hash_changed);
        assert_eq!(diff.changed_sections.len(), 1);
    }

    #[test]
    fn test_section_hash_integration() {
        let doc = build_document();
        let tree = HybridMerkleTree::from_document(&doc);

        // Verify tree structure: root -> sections -> blocks
        assert_eq!(tree.sections.len(), 3, "Should have root + 2 heading sections");

        // Verify root hash is computed from section hashes
        let section_hashes: Vec<NodeHash> = tree.sections
            .iter()
            .map(|s| s.binary_tree.root_hash)
            .collect();
        let expected_root = NodeHash::combine_many(&section_hashes);
        assert_eq!(tree.root_hash, expected_root,
                   "Root hash should be aggregation of section hashes");

        // Verify sections with content have non-zero hashes
        // Section 0 is root (may be empty), sections 1 and 2 have content
        for (i, section) in tree.sections.iter().enumerate() {
            if section.block_count > 0 {
                assert!(!section.binary_tree.root_hash.is_zero(),
                        "Section {} with {} blocks should have non-zero hash",
                        i, section.block_count);
            }
        }
    }

    #[test]
    fn test_hash_changes_when_section_content_changes() {
        let doc_original = build_document();
        let tree_original = HybridMerkleTree::from_document(&doc_original);
        let original_root_hash = tree_original.root_hash;

        // Modify content in the second section (Details)
        let mut doc_modified = build_document();
        doc_modified.content.paragraphs[1].content = "Modified detail paragraph".to_string();
        let tree_modified = HybridMerkleTree::from_document(&doc_modified);

        // Root hash should change
        assert_ne!(tree_modified.root_hash, original_root_hash,
                   "Root hash must change when section content changes");

        // The modified section's hash should change
        assert_ne!(tree_modified.sections[2].binary_tree.root_hash,
                   tree_original.sections[2].binary_tree.root_hash,
                   "Modified section hash must change");

        // Unmodified sections should have same hash
        assert_eq!(tree_modified.sections[0].binary_tree.root_hash,
                   tree_original.sections[0].binary_tree.root_hash,
                   "Unmodified root section hash should remain stable");

        assert_eq!(tree_modified.sections[1].binary_tree.root_hash,
                   tree_original.sections[1].binary_tree.root_hash,
                   "Unmodified intro section hash should remain stable");
    }

    #[test]
    fn test_hash_stability_when_unrelated_sections_change() {
        let doc_original = build_document();
        let tree_original = HybridMerkleTree::from_document(&doc_original);

        // Modify only the first section (Intro)
        let mut doc_modified = build_document();
        doc_modified.content.paragraphs[0].content = "Modified intro paragraph".to_string();
        let tree_modified = HybridMerkleTree::from_document(&doc_modified);

        // The first section should change
        assert_ne!(tree_modified.sections[1].binary_tree.root_hash,
                   tree_original.sections[1].binary_tree.root_hash,
                   "Modified Intro section hash should change");

        // The second section (Details) should remain unchanged
        assert_eq!(tree_modified.sections[2].binary_tree.root_hash,
                   tree_original.sections[2].binary_tree.root_hash,
                   "Unrelated Details section hash must remain stable");

        // Root section should remain unchanged (has no content)
        assert_eq!(tree_modified.sections[0].binary_tree.root_hash,
                   tree_original.sections[0].binary_tree.root_hash,
                   "Root section hash should remain stable");
    }

    #[test]
    fn test_multiple_sections_with_hierarchy() {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("complex.md");
        doc.content = NoteContent::default();

        // Create a more complex note with nested sections
        doc.content.headings = vec![
            Heading {
                level: 1,
                text: "Chapter 1".to_string(),
                offset: 0,
                id: Some("ch1".to_string()),
            },
            Heading {
                level: 2,
                text: "Section 1.1".to_string(),
                offset: 100,
                id: Some("s1-1".to_string()),
            },
            Heading {
                level: 2,
                text: "Section 1.2".to_string(),
                offset: 200,
                id: Some("s1-2".to_string()),
            },
            Heading {
                level: 1,
                text: "Chapter 2".to_string(),
                offset: 300,
                id: Some("ch2".to_string()),
            },
        ];

        doc.content.paragraphs = vec![
            Paragraph::new("Chapter 1 intro".to_string(), 10),
            Paragraph::new("Section 1.1 content".to_string(), 110),
            Paragraph::new("Section 1.2 content".to_string(), 210),
            Paragraph::new("Chapter 2 content".to_string(), 310),
        ];

        let tree = HybridMerkleTree::from_document(&doc);

        // Debug: Print section structure
        // for (i, section) in tree.sections.iter().enumerate() {
        //     eprintln!("Section {}: depth={}, heading={:?}, blocks={}",
        //              i, section.depth, section.heading, section.block_count);
        // }

        // Should have root + 4 heading sections = 5 total
        assert_eq!(tree.sections.len(), 5);
        assert_eq!(tree.total_blocks, 4);

        // The actual structure based on the stack-based algorithm:
        // Sections are closed when a new heading of equal or lower level is encountered
        // and they're added in the order they're closed (reversed at the end)
        // So the final order is: root, Section 1.1, Section 1.2, Chapter 1, Chapter 2

        // Find sections by their heading text instead of assuming order
        let find_section = |heading_text: Option<&str>| -> usize {
            tree.sections.iter().position(|s| {
                match (&s.heading, heading_text) {
                    (Some(h), Some(text)) => h.text == text,
                    (None, None) => true,
                    _ => false,
                }
            }).expect(&format!("Section {:?} not found", heading_text))
        };

        let root_idx = find_section(None);
        let ch1_idx = find_section(Some("Chapter 1"));
        let s11_idx = find_section(Some("Section 1.1"));
        let s12_idx = find_section(Some("Section 1.2"));
        let ch2_idx = find_section(Some("Chapter 2"));

        // Verify section depths
        assert_eq!(tree.sections[root_idx].depth, 0, "Root section");
        assert_eq!(tree.sections[ch1_idx].depth, 1, "Chapter 1");
        assert_eq!(tree.sections[s11_idx].depth, 2, "Section 1.1");
        assert_eq!(tree.sections[s12_idx].depth, 2, "Section 1.2");
        assert_eq!(tree.sections[ch2_idx].depth, 1, "Chapter 2");

        // Modify only Section 1.2
        let mut doc_modified = doc.clone();
        doc_modified.content.paragraphs[2].content = "Modified Section 1.2".to_string();
        let tree_modified = HybridMerkleTree::from_document(&doc_modified);

        // Only Section 1.2's hash should change
        assert_eq!(tree_modified.sections[root_idx].binary_tree.root_hash,
                   tree.sections[root_idx].binary_tree.root_hash,
                   "Root unchanged");
        assert_eq!(tree_modified.sections[ch1_idx].binary_tree.root_hash,
                   tree.sections[ch1_idx].binary_tree.root_hash,
                   "Chapter 1 unchanged");
        assert_eq!(tree_modified.sections[s11_idx].binary_tree.root_hash,
                   tree.sections[s11_idx].binary_tree.root_hash,
                   "Section 1.1 unchanged");
        assert_ne!(tree_modified.sections[s12_idx].binary_tree.root_hash,
                   tree.sections[s12_idx].binary_tree.root_hash,
                   "Section 1.2 changed");
        assert_eq!(tree_modified.sections[ch2_idx].binary_tree.root_hash,
                   tree.sections[ch2_idx].binary_tree.root_hash,
                   "Chapter 2 unchanged");
    }

    #[test]
    fn test_empty_sections_have_zero_hash() {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("empty.md");
        doc.content = NoteContent::default();

        // Heading with no content
        doc.content.headings = vec![
            Heading {
                level: 1,
                text: "Empty Section".to_string(),
                offset: 0,
                id: Some("empty".to_string()),
            },
        ];
        // No paragraphs

        let tree = HybridMerkleTree::from_document(&doc);

        // Should have root + 1 heading section
        assert_eq!(tree.sections.len(), 2);
        assert_eq!(tree.total_blocks, 0);

        // Empty sections should have zero hash
        assert!(tree.sections[0].binary_tree.root_hash.is_zero(),
                "Empty root section should have zero hash");
        assert!(tree.sections[1].binary_tree.root_hash.is_zero(),
                "Empty heading section should have zero hash");
    }

    #[test]
    fn test_section_binary_tree_structure() {
        let doc = build_document();
        let tree = HybridMerkleTree::from_document(&doc);

        // Check that each section's binary tree has correct structure
        for section in &tree.sections {
            if section.block_count > 0 {
                assert!(!section.binary_tree.nodes.is_empty(),
                        "Non-empty section should have nodes");
                assert_eq!(section.binary_tree.leaf_count, section.block_count,
                          "Leaf count should match block count");

                // Verify root node exists and is either a leaf (1 block) or internal (>1 blocks)
                if section.block_count == 1 {
                    assert_eq!(section.binary_tree.height, 0,
                              "Single block should have height 0");
                } else {
                    assert!(section.binary_tree.height > 0,
                           "Multiple blocks should have height > 0");
                }
            } else {
                assert!(section.binary_tree.nodes.is_empty(),
                       "Empty section should have no nodes");
                assert_eq!(section.binary_tree.height, 0);
            }
        }
    }

    // Virtualization tests

    #[test]
    fn test_tree_without_virtualization() {
        let doc = build_document();
        let tree = HybridMerkleTree::from_document(&doc);

        // Should not be virtualized (small document)
        assert!(!tree.is_virtualized);
        assert!(tree.virtual_sections.is_none());
        assert_eq!(tree.section_count(), tree.real_section_count());
    }

    #[test]
    fn test_tree_with_virtualization_disabled() {
        let doc = build_document();
        let config = VirtualizationConfig::disabled();
        let tree = HybridMerkleTree::from_document_with_config(&doc, &config);

        // Should not be virtualized even with many sections
        assert!(!tree.is_virtualized);
        assert!(tree.virtual_sections.is_none());
    }

    #[test]
    fn test_tree_with_virtualization_enabled() {
        // Create a document with many sections (above threshold)
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("large.md");
        doc.content = NoteContent::default();

        // Create 150 headings (above default threshold of 100)
        for i in 0..150 {
            doc.content.headings.push(Heading {
                level: ((i % 3) + 1) as u8,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });

            // Add a paragraph for each heading
            doc.content.paragraphs.push(Paragraph::new(
                format!("Content for section {}", i),
                i * 100 + 10,
            ));
        }

        let config = VirtualizationConfig::default();
        let tree = HybridMerkleTree::from_document_with_config(&doc, &config);

        // Should be virtualized
        assert!(tree.is_virtualized);
        assert!(tree.virtual_sections.is_some());

        // Should have virtual sections
        let virtual_sections = tree.virtual_sections.as_ref().unwrap();
        assert!(virtual_sections.len() > 0);
        assert!(virtual_sections.len() < tree.real_section_count());

        // Verify section count methods
        assert_eq!(tree.section_count(), virtual_sections.len());
        assert!(tree.real_section_count() > tree.section_count());
    }

    #[test]
    fn test_tree_virtualization_auto() {
        // Create large document
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("large.md");
        doc.content = NoteContent::default();

        for i in 0..120 {
            doc.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });
            doc.content.paragraphs.push(Paragraph::new(
                format!("Content {}", i),
                i * 100 + 10,
            ));
        }

        let tree = HybridMerkleTree::from_document_auto(&doc);

        // Should be virtualized with default config
        assert!(tree.is_virtualized);
        assert!(tree.virtual_sections.is_some());
    }

    #[test]
    fn test_virtualized_tree_hash_correctness() {
        // Create large document
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("test.md");
        doc.content = NoteContent::default();

        for i in 0..150 {
            doc.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });
            doc.content.paragraphs.push(Paragraph::new(
                format!("Content {}", i),
                i * 100 + 10,
            ));
        }

        let config = VirtualizationConfig::default();
        let tree = HybridMerkleTree::from_document_with_config(&doc, &config);

        // Verify root hash is computed from virtual sections
        if let Some(ref virtual_sections) = tree.virtual_sections {
            let virtual_hashes: Vec<NodeHash> = virtual_sections
                .iter()
                .map(|vs| vs.hash)
                .collect();
            let expected_root = NodeHash::combine_many(&virtual_hashes);

            assert_eq!(tree.root_hash, expected_root,
                      "Root hash should be aggregation of virtual section hashes");
        }
    }

    #[test]
    fn test_virtualization_threshold_boundary() {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("boundary.md");
        doc.content = NoteContent::default();

        let config = VirtualizationConfig::default();

        // Test below threshold (should NOT virtualize)
        // Note: build_sections creates a root section + heading sections
        // So 99 headings = 100 sections (root + 99 headings)
        for i in 0..99 {
            doc.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });
            doc.content.paragraphs.push(Paragraph::new("Content".to_string(), i * 100 + 10));
        }

        let tree_at_threshold = HybridMerkleTree::from_document_with_config(&doc, &config);
        assert!(!tree_at_threshold.is_virtualized, "Should not virtualize at threshold (100 sections)");

        // Add two more sections to go above threshold
        doc.content.headings.push(Heading {
            level: 1,
            text: "Section 99".to_string(),
            offset: 9900,
            id: Some("s99".to_string()),
        });
        doc.content.paragraphs.push(Paragraph::new("Content".to_string(), 9910));

        doc.content.headings.push(Heading {
            level: 1,
            text: "Section 100".to_string(),
            offset: 10000,
            id: Some("s100".to_string()),
        });
        doc.content.paragraphs.push(Paragraph::new("Content".to_string(), 10010));

        let tree_above_threshold = HybridMerkleTree::from_document_with_config(&doc, &config);
        assert!(tree_above_threshold.is_virtualized, "Should virtualize above threshold (102 sections)");
    }

    #[test]
    fn test_backward_compatibility() {
        let doc = build_document();

        // Old method (from_document) should work exactly as before
        let tree_old = HybridMerkleTree::from_document(&doc);

        // Should not be virtualized
        assert!(!tree_old.is_virtualized);
        assert!(tree_old.virtual_sections.is_none());

        // All existing tests should still pass
        assert_eq!(tree_old.total_blocks, 2);
        assert_eq!(tree_old.sections.len(), 3);
        assert!(!tree_old.root_hash.is_zero());
    }

    #[test]
    fn test_large_document_memory_efficiency() {
        // Simulate very large document
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("huge.md");
        doc.content = NoteContent::default();

        for i in 0..1000 {
            doc.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });
            doc.content.paragraphs.push(Paragraph::new(
                format!("Content {}", i),
                i * 100 + 10,
            ));
        }

        let tree = HybridMerkleTree::from_document_auto(&doc);

        // Should be virtualized
        assert!(tree.is_virtualized);

        // Virtual section count should be much smaller than real section count
        let reduction_ratio = tree.section_count() as f64 / tree.real_section_count() as f64;
        assert!(reduction_ratio < 0.2, "Should have significant reduction in active sections");

        // Should have ~100 virtual sections (1000 / 10)
        assert!(tree.section_count() > 90 && tree.section_count() < 110);
    }

    // Performance tests

    #[test]
    fn test_performance_diff_identical_trees() {
        // Build a large tree
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("perf.md");
        doc.content = NoteContent::default();

        for i in 0..1000 {
            doc.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });
            doc.content.paragraphs.push(Paragraph::new("Content".to_string(), i * 100 + 10));
        }

        let tree = HybridMerkleTree::from_document(&doc);

        // Time the diff operation
        let start = std::time::Instant::now();
        let diff = tree.diff(&tree);
        let elapsed = start.elapsed();

        // Should be empty (identical trees)
        assert!(diff.is_empty());

        // Should be very fast (early exit on hash match)
        assert!(
            elapsed.as_millis() < 5,
            "Diff of identical trees should be <5ms, took {:?}",
            elapsed
        );
    }

    #[test]
    fn test_performance_tree_construction() {
        // Create a large document with 10,000 blocks
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("large_perf.md");
        doc.content = NoteContent::default();

        // Create 5,000 sections with 2 blocks each = 10,000 blocks
        for i in 0..5000 {
            doc.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 200,
                id: Some(format!("s{}", i)),
            });
            doc.content.paragraphs.push(Paragraph::new(
                format!("Content {} part 1", i),
                i * 200 + 10,
            ));
            doc.content.paragraphs.push(Paragraph::new(
                format!("Content {} part 2", i),
                i * 200 + 50,
            ));
        }

        // Time tree construction
        let start = std::time::Instant::now();
        let tree = HybridMerkleTree::from_document_auto(&doc);
        let elapsed = start.elapsed();

        // Verify tree is correct
        assert_eq!(tree.total_blocks, 10000);
        assert!(tree.is_virtualized);

        // Performance target: <50ms for 10K blocks in release mode
        // Allow up to 100ms in debug mode (typical overhead is ~3-4x)
        let target_ms = if cfg!(debug_assertions) { 100 } else { 50 };
        assert!(
            elapsed.as_millis() < target_ms,
            "Tree construction should be <{}ms for 10K blocks, took {:?}",
            target_ms,
            elapsed
        );

        println!(
            "Tree construction for 10K blocks: {:?} (target: <50ms)",
            elapsed
        );
    }

    #[test]
    fn test_performance_diff_with_changes() {
        // Build two large trees with some changes
        let mut doc1 = ParsedNote::default();
        doc1.path = PathBuf::from("perf1.md");
        doc1.content = NoteContent::default();

        for i in 0..1000 {
            doc1.content.headings.push(Heading {
                level: 1,
                text: format!("Section {}", i),
                offset: i * 100,
                id: Some(format!("s{}", i)),
            });
            doc1.content.paragraphs.push(Paragraph::new(
                format!("Content {}", i),
                i * 100 + 10,
            ));
        }

        let tree1 = HybridMerkleTree::from_document(&doc1);

        // Create modified version (change every 10th section)
        let mut doc2 = doc1.clone();
        for i in (0..1000).step_by(10) {
            doc2.content.paragraphs[i].content = format!("Modified content {}", i);
        }

        let tree2 = HybridMerkleTree::from_document(&doc2);

        // Time the diff operation
        let start = std::time::Instant::now();
        let diff = tree1.diff(&tree2);
        let elapsed = start.elapsed();

        // Should detect changes
        assert!(!diff.is_empty());
        assert!(diff.root_hash_changed);

        // Should be fast (<10ms for 1000 sections in release, <30ms in debug)
        let target_ms = if cfg!(debug_assertions) { 30 } else { 10 };
        assert!(
            elapsed.as_millis() < target_ms,
            "Diff should be <{}ms for 1000 sections, took {:?}",
            target_ms,
            elapsed
        );

        println!("Diff for 1000 sections: {:?} (target: <10ms)", elapsed);
    }

    #[test]
    fn test_performance_hash_operations() {
        // Test hash combination performance
        let hashes: Vec<NodeHash> = (0..10000)
            .map(|i| NodeHash::from_content(format!("content {}", i).as_bytes()))
            .collect();

        let start = std::time::Instant::now();
        let combined = NodeHash::combine_many(&hashes);
        let elapsed = start.elapsed();

        assert!(!combined.is_zero());

        // Should be fast (<20ms for 10K hashes in release, <50ms in debug)
        let target_ms = if cfg!(debug_assertions) { 50 } else { 20 };
        assert!(
            elapsed.as_millis() < target_ms,
            "Hash combination should be <{}ms for 10K hashes, took {:?}",
            target_ms,
            elapsed
        );

        println!(
            "Hash combination for 10K hashes: {:?} (target: <20ms)",
            elapsed
        );
    }
}
