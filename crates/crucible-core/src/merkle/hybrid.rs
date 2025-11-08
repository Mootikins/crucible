use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::parser::types::{Heading, Paragraph, ParsedDocument};
use crate::types::BlockHash;

/// Hybrid Merkle tree that groups document content into semantic sections and
/// stores block-level hashing inside each section.
#[derive(Debug, Clone)]
pub struct HybridMerkleTree {
    pub root_hash: BlockHash,
    pub sections: Vec<SectionNode>,
    pub total_blocks: usize,
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
    pub root_hash: BlockHash,
    pub nodes: Vec<MerkleNode>,
    pub height: usize,
    pub leaf_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MerkleNode {
    Leaf {
        hash: BlockHash,
        block_index: usize,
    },
    Internal {
        hash: BlockHash,
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
    pub fn from_document(doc: &ParsedDocument) -> Self {
        let (sections, total_blocks) = build_sections(doc);
        let section_hashes: Vec<BlockHash> = sections
            .iter()
            .map(|section| section.binary_tree.root_hash.clone())
            .collect();
        let root_hash = aggregate_hashes(&section_hashes);

        Self {
            root_hash,
            sections,
            total_blocks,
        }
    }

    pub fn diff(&self, other: &HybridMerkleTree) -> HybridDiff {
        let mut changed_sections = Vec::new();

        for (idx, (left, right)) in self.sections.iter().zip(other.sections.iter()).enumerate() {
            if left.binary_tree.root_hash != right.binary_tree.root_hash {
                changed_sections.push(SectionChange {
                    section_index: idx,
                    heading: right.heading.clone(),
                });
            }
        }

        HybridDiff {
            root_hash_changed: self.root_hash != other.root_hash,
            changed_sections,
            added_sections: other.sections.len().saturating_sub(self.sections.len()),
            removed_sections: self.sections.len().saturating_sub(other.sections.len()),
        }
    }
}

impl BinaryMerkleTree {
    pub fn empty() -> Self {
        Self {
            root_hash: BlockHash::zero(),
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

        for (index, hash) in blocks {
            let node_index = nodes.len();
            nodes.push(MerkleNode::Leaf {
                hash: hash.clone(),
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
                let combined = combine_pair(nodes[left_idx].hash(), nodes[right_idx].hash());
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
        let root_hash = nodes[root_index].hash().clone();

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

impl MerkleNode {
    pub fn hash(&self) -> &BlockHash {
        match self {
            MerkleNode::Leaf { hash, .. } => hash,
            MerkleNode::Internal { hash, .. } => hash,
        }
    }
}

fn build_sections(doc: &ParsedDocument) -> (Vec<SectionNode>, usize) {
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

fn aggregate_hashes(hashes: &[BlockHash]) -> BlockHash {
    if hashes.is_empty() {
        return BlockHash::zero();
    }

    let mut level: Vec<BlockHash> = hashes.to_vec();
    while level.len() > 1 {
        let mut next = Vec::new();
        for chunk in level.chunks(2) {
            let left = &chunk[0];
            let right = if chunk.len() == 2 {
                &chunk[1]
            } else {
                &chunk[0]
            };
            next.push(combine_pair(left, right));
        }
        level = next;
    }
    level.pop().unwrap()
}

fn combine_pair(left: &BlockHash, right: &BlockHash) -> BlockHash {
    let mut hasher = Hasher::new();
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    let digest = hasher.finalize();
    let mut array = [0u8; 32];
    array.copy_from_slice(digest.as_bytes());
    BlockHash::new(array)
}

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
    use crate::parser::types::{DocumentContent, Heading, Paragraph};
    use std::path::PathBuf;

    fn build_document() -> ParsedDocument {
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("sample.md");
        doc.content = DocumentContent::default();

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
}
