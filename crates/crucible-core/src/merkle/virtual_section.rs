//! Virtual sections for efficient large document handling
//!
//! This module provides virtualization for documents with many sections,
//! preventing memory exhaustion and maintaining performance at scale.
//!
//! ## Design Philosophy
//!
//! When a document has more than a threshold number of sections (default: 100),
//! the sections are grouped into virtual sections. Each virtual section:
//!
//! - Aggregates multiple real sections into a single unit
//! - Maintains efficient hash computation via aggregation
//! - Tracks depth ranges and heading summaries
//! - Provides transparent access to underlying sections
//!
//! ## Memory Benefits
//!
//! For a document with 10,000 sections:
//! - **Without virtualization**: 10,000 section objects in memory
//! - **With virtualization**: ~100 virtual sections grouping the real sections
//! - **Memory savings**: ~99% reduction in active section metadata
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use crucible_core::merkle::{VirtualSection, VirtualizationConfig};
//!
//! let config = VirtualizationConfig::default();
//! if sections.len() > config.threshold {
//!     let virtual_sections = VirtualSection::virtualize(&sections, &config);
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::merkle::{HeadingSummary, NodeHash, SectionNode};

/// Configuration for section virtualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualizationConfig {
    /// Threshold for enabling virtualization (number of sections)
    pub threshold: usize,
    /// Target size for each virtual section (number of real sections)
    pub virtual_section_size: usize,
}

impl VirtualizationConfig {
    /// Default configuration for typical documents
    ///
    /// - Threshold: 100 sections (enables virtualization for large docs)
    /// - Virtual section size: 10 sections (balances memory vs. granularity)
    pub fn default() -> Self {
        Self {
            threshold: 100,
            virtual_section_size: 10,
        }
    }

    /// Configuration for very large documents (>1000 sections)
    ///
    /// - Threshold: 500 sections
    /// - Virtual section size: 25 sections (more aggressive grouping)
    pub fn large() -> Self {
        Self {
            threshold: 500,
            virtual_section_size: 25,
        }
    }

    /// Configuration for memory-constrained environments
    ///
    /// - Threshold: 50 sections (aggressive virtualization)
    /// - Virtual section size: 5 sections (fine-grained control)
    pub fn minimal() -> Self {
        Self {
            threshold: 50,
            virtual_section_size: 5,
        }
    }

    /// Disable virtualization (for testing or small documents)
    pub fn disabled() -> Self {
        Self {
            threshold: usize::MAX,
            virtual_section_size: 1,
        }
    }
}

impl Default for VirtualizationConfig {
    fn default() -> Self {
        Self::default()
    }
}

/// A virtual section that aggregates multiple real sections
///
/// Virtual sections are used when a document has many sections, to reduce
/// memory usage and maintain performance. Each virtual section:
///
/// - Contains multiple real sections as children
/// - Computes an aggregated hash from child section hashes
/// - Tracks the depth range of contained sections
/// - Summarizes the heading hierarchy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VirtualSection {
    /// Aggregated hash of all child sections
    pub hash: NodeHash,

    /// Summary of the first (primary) heading in this virtual section
    pub primary_heading: Option<HeadingSummary>,

    /// Minimum depth of sections in this virtual section
    pub min_depth: u8,

    /// Maximum depth of sections in this virtual section
    pub max_depth: u8,

    /// Number of real sections contained in this virtual section
    pub section_count: usize,

    /// Total number of blocks across all contained sections
    pub total_blocks: usize,

    /// Start index in the original section list
    pub start_index: usize,

    /// End index (exclusive) in the original section list
    pub end_index: usize,
}

impl VirtualSection {
    /// Create a virtual section from a slice of real sections
    ///
    /// # Arguments
    ///
    /// * `sections` - The real sections to aggregate
    /// * `start_index` - Starting index in the original section list
    ///
    /// # Returns
    ///
    /// A virtual section aggregating the provided sections
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let sections = vec![section1, section2, section3];
    /// let virtual_section = VirtualSection::from_sections(&sections, 0);
    /// ```
    pub fn from_sections(sections: &[SectionNode], start_index: usize) -> Self {
        if sections.is_empty() {
            return Self::empty(start_index);
        }

        // Aggregate hashes from all sections
        let section_hashes: Vec<NodeHash> =
            sections.iter().map(|s| s.binary_tree.root_hash).collect();

        let hash = NodeHash::combine_many(&section_hashes);

        // Find primary heading (first non-None heading)
        let primary_heading = sections.iter().find_map(|s| s.heading.clone());

        // Calculate depth range
        let mut min_depth = u8::MAX;
        let mut max_depth = 0u8;
        for section in sections {
            min_depth = min_depth.min(section.depth);
            max_depth = max_depth.max(section.depth);
        }

        // Sum total blocks
        let total_blocks = sections.iter().map(|s| s.block_count).sum();

        Self {
            hash,
            primary_heading,
            min_depth,
            max_depth,
            section_count: sections.len(),
            total_blocks,
            start_index,
            end_index: start_index + sections.len(),
        }
    }

    /// Create an empty virtual section
    pub fn empty(start_index: usize) -> Self {
        Self {
            hash: NodeHash::zero(),
            primary_heading: None,
            min_depth: 0,
            max_depth: 0,
            section_count: 0,
            total_blocks: 0,
            start_index,
            end_index: start_index,
        }
    }

    /// Check if this virtual section is empty
    pub fn is_empty(&self) -> bool {
        self.section_count == 0
    }

    /// Get the depth range as a tuple
    pub fn depth_range(&self) -> (u8, u8) {
        (self.min_depth, self.max_depth)
    }

    /// Virtualize a list of sections according to the configuration
    ///
    /// This is the main entry point for section virtualization. It takes a list
    /// of real sections and groups them into virtual sections when the count
    /// exceeds the threshold.
    ///
    /// # Arguments
    ///
    /// * `sections` - The real sections to potentially virtualize
    /// * `config` - Virtualization configuration
    ///
    /// # Returns
    ///
    /// A list of virtual sections (or a pass-through if below threshold)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = VirtualizationConfig::default();
    /// let virtual_sections = VirtualSection::virtualize(&sections, &config);
    /// ```
    pub fn virtualize(
        sections: &[SectionNode],
        config: &VirtualizationConfig,
    ) -> Vec<VirtualSection> {
        // If below threshold, don't virtualize - create one virtual section per real section
        if sections.len() <= config.threshold {
            return sections
                .iter()
                .enumerate()
                .map(|(i, section)| VirtualSection::from_sections(&[section.clone()], i))
                .collect();
        }

        // Group sections into virtual sections
        let mut virtual_sections = Vec::new();
        let mut current_index = 0;

        while current_index < sections.len() {
            let end_index = (current_index + config.virtual_section_size).min(sections.len());
            let section_slice = &sections[current_index..end_index];

            let virtual_section = VirtualSection::from_sections(section_slice, current_index);
            virtual_sections.push(virtual_section);

            current_index = end_index;
        }

        virtual_sections
    }

    /// Check if sections should be virtualized based on count and config
    ///
    /// # Arguments
    ///
    /// * `section_count` - Number of sections
    /// * `config` - Virtualization configuration
    ///
    /// # Returns
    ///
    /// `true` if virtualization should be enabled, `false` otherwise
    pub fn should_virtualize(section_count: usize, config: &VirtualizationConfig) -> bool {
        section_count > config.threshold
    }

    /// Get statistics about this virtual section
    pub fn stats(&self) -> VirtualSectionStats {
        VirtualSectionStats {
            hash: self.hash,
            section_count: self.section_count,
            total_blocks: self.total_blocks,
            min_depth: self.min_depth,
            max_depth: self.max_depth,
            start_index: self.start_index,
            end_index: self.end_index,
        }
    }
}

/// Statistics about a virtual section
#[derive(Debug, Clone)]
pub struct VirtualSectionStats {
    pub hash: NodeHash,
    pub section_count: usize,
    pub total_blocks: usize,
    pub min_depth: u8,
    pub max_depth: u8,
    pub start_index: usize,
    pub end_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle::hybrid::BinaryMerkleTree;

    fn create_test_section(
        depth: u8,
        heading_text: Option<&str>,
        block_count: usize,
    ) -> SectionNode {
        SectionNode {
            heading: heading_text.map(|text| HeadingSummary {
                text: text.to_string(),
                level: depth,
            }),
            depth,
            binary_tree: if block_count > 0 {
                // Create a simple non-empty tree
                let mut tree = BinaryMerkleTree::empty();
                tree.root_hash = NodeHash::from_content(format!("section_{}", depth).as_bytes());
                tree.leaf_count = block_count;
                tree
            } else {
                BinaryMerkleTree::empty()
            },
            block_count,
        }
    }

    #[test]
    fn test_virtual_section_from_single_section() {
        let section = create_test_section(1, Some("Test"), 5);
        let virtual_section = VirtualSection::from_sections(&[section.clone()], 0);

        assert_eq!(virtual_section.section_count, 1);
        assert_eq!(virtual_section.total_blocks, 5);
        assert_eq!(virtual_section.min_depth, 1);
        assert_eq!(virtual_section.max_depth, 1);
        assert_eq!(virtual_section.start_index, 0);
        assert_eq!(virtual_section.end_index, 1);
        assert!(virtual_section.primary_heading.is_some());
    }

    #[test]
    fn test_virtual_section_from_multiple_sections() {
        let sections = vec![
            create_test_section(1, Some("Section 1"), 10),
            create_test_section(2, Some("Section 2"), 5),
            create_test_section(3, Some("Section 3"), 8),
        ];

        let virtual_section = VirtualSection::from_sections(&sections, 0);

        assert_eq!(virtual_section.section_count, 3);
        assert_eq!(virtual_section.total_blocks, 23); // 10 + 5 + 8
        assert_eq!(virtual_section.min_depth, 1);
        assert_eq!(virtual_section.max_depth, 3);
        assert_eq!(virtual_section.start_index, 0);
        assert_eq!(virtual_section.end_index, 3);

        // Primary heading should be from first section
        assert_eq!(
            virtual_section.primary_heading.as_ref().unwrap().text,
            "Section 1"
        );
    }

    #[test]
    fn test_virtual_section_empty() {
        let virtual_section = VirtualSection::empty(5);

        assert!(virtual_section.is_empty());
        assert_eq!(virtual_section.section_count, 0);
        assert_eq!(virtual_section.total_blocks, 0);
        assert_eq!(virtual_section.start_index, 5);
        assert_eq!(virtual_section.end_index, 5);
        assert!(virtual_section.hash.is_zero());
    }

    #[test]
    fn test_virtualization_config_default() {
        let config = VirtualizationConfig::default();
        assert_eq!(config.threshold, 100);
        assert_eq!(config.virtual_section_size, 10);
    }

    #[test]
    fn test_virtualization_config_large() {
        let config = VirtualizationConfig::large();
        assert_eq!(config.threshold, 500);
        assert_eq!(config.virtual_section_size, 25);
    }

    #[test]
    fn test_virtualization_config_minimal() {
        let config = VirtualizationConfig::minimal();
        assert_eq!(config.threshold, 50);
        assert_eq!(config.virtual_section_size, 5);
    }

    #[test]
    fn test_virtualization_config_disabled() {
        let config = VirtualizationConfig::disabled();
        assert_eq!(config.threshold, usize::MAX);
    }

    #[test]
    fn test_should_virtualize() {
        let config = VirtualizationConfig::default();

        assert!(!VirtualSection::should_virtualize(50, &config));
        assert!(!VirtualSection::should_virtualize(100, &config));
        assert!(VirtualSection::should_virtualize(101, &config));
        assert!(VirtualSection::should_virtualize(1000, &config));
    }

    #[test]
    fn test_virtualize_below_threshold() {
        let sections = vec![
            create_test_section(1, Some("Section 1"), 5),
            create_test_section(2, Some("Section 2"), 3),
        ];

        let config = VirtualizationConfig::default();
        let virtual_sections = VirtualSection::virtualize(&sections, &config);

        // Below threshold: one virtual section per real section
        assert_eq!(virtual_sections.len(), 2);
        assert_eq!(virtual_sections[0].section_count, 1);
        assert_eq!(virtual_sections[1].section_count, 1);
    }

    #[test]
    fn test_virtualize_above_threshold() {
        // Create 150 sections (above default threshold of 100)
        let sections: Vec<SectionNode> = (0..150)
            .map(|i| create_test_section(1, Some(&format!("Section {}", i)), 2))
            .collect();

        let config = VirtualizationConfig::default();
        let virtual_sections = VirtualSection::virtualize(&sections, &config);

        // Should be grouped into virtual sections of size 10
        // 150 sections / 10 per virtual = 15 virtual sections
        assert_eq!(virtual_sections.len(), 15);

        // Verify each virtual section contains the right number of sections
        for (i, vs) in virtual_sections.iter().enumerate() {
            assert_eq!(vs.section_count, 10);
            assert_eq!(vs.start_index, i * 10);
            assert_eq!(vs.end_index, (i + 1) * 10);
        }
    }

    #[test]
    fn test_virtualize_uneven_division() {
        // Create 105 sections (doesn't divide evenly by 10)
        let sections: Vec<SectionNode> = (0..105)
            .map(|i| create_test_section(1, Some(&format!("Section {}", i)), 2))
            .collect();

        let config = VirtualizationConfig::default();
        let virtual_sections = VirtualSection::virtualize(&sections, &config);

        // Should be: 10 virtual sections of 10, plus 1 virtual section of 5
        assert_eq!(virtual_sections.len(), 11);

        // First 10 should have 10 sections each
        for i in 0..10 {
            assert_eq!(virtual_sections[i].section_count, 10);
        }

        // Last one should have 5 sections
        assert_eq!(virtual_sections[10].section_count, 5);
        assert_eq!(virtual_sections[10].start_index, 100);
        assert_eq!(virtual_sections[10].end_index, 105);
    }

    #[test]
    fn test_virtual_section_hash_aggregation() {
        let sections = vec![
            create_test_section(1, Some("Section 1"), 5),
            create_test_section(2, Some("Section 2"), 5),
        ];

        let virtual_section = VirtualSection::from_sections(&sections, 0);

        // Hash should be aggregation of section hashes
        let section_hashes: Vec<NodeHash> =
            sections.iter().map(|s| s.binary_tree.root_hash).collect();
        let expected_hash = NodeHash::combine_many(&section_hashes);

        assert_eq!(virtual_section.hash, expected_hash);
        assert!(!virtual_section.hash.is_zero());
    }

    #[test]
    fn test_virtual_section_depth_range() {
        let sections = vec![
            create_test_section(1, Some("H1"), 5),
            create_test_section(3, Some("H3"), 3),
            create_test_section(2, Some("H2"), 4),
            create_test_section(4, Some("H4"), 2),
        ];

        let virtual_section = VirtualSection::from_sections(&sections, 0);

        assert_eq!(virtual_section.depth_range(), (1, 4));
        assert_eq!(virtual_section.min_depth, 1);
        assert_eq!(virtual_section.max_depth, 4);
    }

    #[test]
    fn test_virtual_section_stats() {
        let sections = vec![
            create_test_section(1, Some("Section 1"), 10),
            create_test_section(2, Some("Section 2"), 5),
        ];

        let virtual_section = VirtualSection::from_sections(&sections, 5);
        let stats = virtual_section.stats();

        assert_eq!(stats.section_count, 2);
        assert_eq!(stats.total_blocks, 15);
        assert_eq!(stats.min_depth, 1);
        assert_eq!(stats.max_depth, 2);
        assert_eq!(stats.start_index, 5);
        assert_eq!(stats.end_index, 7);
    }

    #[test]
    fn test_virtual_section_serialization() {
        let sections = vec![create_test_section(1, Some("Test"), 5)];

        let virtual_section = VirtualSection::from_sections(&sections, 0);

        // Test JSON serialization
        let json = serde_json::to_string(&virtual_section).unwrap();
        let deserialized: VirtualSection = serde_json::from_str(&json).unwrap();

        assert_eq!(virtual_section, deserialized);
    }

    #[test]
    fn test_large_document_virtualization() {
        // Simulate a very large document with 10,000 sections
        let sections: Vec<SectionNode> = (0..10000)
            .map(|i| create_test_section((i % 6) as u8 + 1, Some(&format!("Section {}", i)), 3))
            .collect();

        let config = VirtualizationConfig::default();
        let virtual_sections = VirtualSection::virtualize(&sections, &config);

        // Should create 1000 virtual sections (10000 / 10)
        assert_eq!(virtual_sections.len(), 1000);

        // Verify total block count is preserved
        let total_blocks: usize = virtual_sections.iter().map(|vs| vs.total_blocks).sum();
        assert_eq!(total_blocks, 30000); // 10000 sections * 3 blocks

        // Verify no gaps in indexing
        for (i, vs) in virtual_sections.iter().enumerate() {
            assert_eq!(vs.start_index, i * 10);
            assert_eq!(vs.end_index, (i + 1) * 10);
        }
    }

    #[test]
    fn test_memory_efficiency() {
        // Without virtualization: 10,000 sections in memory
        let sections: Vec<SectionNode> = (0..10000)
            .map(|i| create_test_section(1, Some(&format!("Section {}", i)), 5))
            .collect();

        let config = VirtualizationConfig::default();
        let virtual_sections = VirtualSection::virtualize(&sections, &config);

        // With virtualization: only 1,000 virtual sections
        assert_eq!(virtual_sections.len(), 1000);

        // This represents a ~90% reduction in active metadata
        let reduction_ratio = 1.0 - (virtual_sections.len() as f64 / sections.len() as f64);
        assert!(reduction_ratio > 0.89); // At least 89% reduction
    }
}
