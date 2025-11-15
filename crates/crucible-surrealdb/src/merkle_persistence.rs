//! SurrealDB Persistence Layer for HybridMerkleTree
//!
//! This module provides efficient storage and retrieval of Hybrid Merkle trees
//! with binary storage format and path sharding for production scale.
//!
//! ## Features
//!
//! - **Binary Storage**: Efficient serialization with bincode
//! - **Path Sharding**: Distribute tree data across database for scale
//! - **Virtual Section Support**: Handle large documents with virtualization
//! - **Metadata Preservation**: Store all section and node metadata
//! - **Incremental Updates**: Support for partial tree updates

use crate::{DbError, DbResult, RecordId, SurrealClient};
use crucible_core::merkle::{HybridMerkleTree, NodeHash, SectionNode, VirtualSection};
use crucible_core::parser::ParsedNote;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Database record for Hybrid Merkle tree metadata
///
/// This stores the core tree structure and configuration without the full
/// section data, which is stored separately for efficiency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridTreeRecord {
    /// Unique identifier for this tree (typically document path)
    pub id: String,
    /// Root hash of the entire tree
    pub root_hash: String,
    /// Total number of sections in the tree
    pub section_count: usize,
    /// Total number of blocks across all sections
    pub total_blocks: usize,
    /// Whether the tree is using virtual sections
    pub is_virtualized: bool,
    /// Number of virtual sections (if virtualized)
    pub virtual_section_count: Option<usize>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Optional metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Database record for a section node
///
/// Sections are stored separately from the tree record to enable efficient
/// partial updates and path sharding across the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionRecord {
    /// Tree ID this section belongs to
    pub tree_id: String,
    /// Section index within the tree
    pub section_index: usize,
    /// Section hash (16-byte NodeHash as hex string)
    pub section_hash: String,
    /// Heading text (None for root section)
    pub heading: Option<String>,
    /// Heading depth (0 for root)
    pub depth: u8,
    /// Starting block index
    pub start_block: usize,
    /// Ending block index (exclusive)
    pub end_block: usize,
    /// Binary-encoded section data (full SectionNode)
    pub section_data: Vec<u8>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Database record for virtual sections
///
/// Virtual sections aggregate multiple sections for large documents,
/// reducing memory usage and improving performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualSectionRecord {
    /// Tree ID this virtual section belongs to
    pub tree_id: String,
    /// Virtual section index
    pub virtual_index: usize,
    /// Aggregated hash
    pub hash: String,
    /// Primary heading summary
    pub primary_heading: Option<String>,
    /// Minimum section depth
    pub min_depth: u8,
    /// Maximum section depth
    pub max_depth: u8,
    /// Number of sections aggregated
    pub section_count: usize,
    /// Total blocks across aggregated sections
    pub total_blocks: usize,
    /// Start index of first section
    pub start_index: usize,
    /// End index of last section
    pub end_index: usize,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Persistence layer for Hybrid Merkle trees
pub struct MerklePersistence {
    client: SurrealClient,
}

impl MerklePersistence {
    /// Create a new persistence layer
    pub fn new(client: SurrealClient) -> Self {
        Self { client }
    }

    /// Store a complete Hybrid Merkle tree
    ///
    /// This stores the tree metadata, all sections, and virtual sections (if applicable)
    /// using efficient binary encoding and path sharding.
    ///
    /// # Arguments
    ///
    /// * `tree_id` - Unique identifier for the tree (typically document path)
    /// * `tree` - The Hybrid Merkle tree to store
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn store_tree(&self, tree_id: &str, tree: &HybridMerkleTree) -> DbResult<()> {
        let now = chrono::Utc::now();

        // 1. Store tree metadata
        let tree_record = HybridTreeRecord {
            id: tree_id.to_string(),
            root_hash: tree.root_hash.to_hex(),
            section_count: tree.sections.len(),
            total_blocks: tree.total_blocks,
            is_virtualized: tree.is_virtualized,
            virtual_section_count: tree
                .virtual_sections
                .as_ref()
                .map(|vs| vs.len()),
            created_at: now,
            updated_at: now,
            metadata: None,
        };

        self.client
            .execute_query(&format!(
                "UPDATE hybrid_tree:{} CONTENT $tree",
                sanitize_id(tree_id)
            ))
            .bind(("tree", tree_record))
            .await
            .map_err(|e| DbError::query(format!("Failed to store tree metadata: {}", e)))?;

        // 2. Store sections with binary encoding
        let mut cumulative_blocks = 0;
        for (index, section) in tree.sections.iter().enumerate() {
            // Serialize the full section node using bincode for efficiency
            let section_data = bincode::serialize(section)
                .map_err(|e| DbError::serialization(format!("Failed to serialize section: {}", e)))?;

            let start_block = cumulative_blocks;
            let end_block = cumulative_blocks + section.block_count;
            cumulative_blocks = end_block;

            let section_record = SectionRecord {
                tree_id: tree_id.to_string(),
                section_index: index,
                section_hash: section.binary_tree.root_hash.to_hex(),
                heading: section.heading.as_ref().map(|h| h.text.clone()),
                depth: section.depth,
                start_block,
                end_block,
                section_data,
                created_at: now,
            };

            // Use path sharding: section_{tree_id}_{index}
            self.client
                .execute_query(&format!(
                    "UPDATE section:{{tree_id: $tree_id, index: $index}} CONTENT $section"
                ))
                .bind(("tree_id", tree_id))
                .bind(("index", index))
                .bind(("section", section_record))
                .await
                .map_err(|e| DbError::query(format!("Failed to store section {}: {}", index, e)))?;
        }

        // 3. Store virtual sections if present
        if let Some(virtual_sections) = &tree.virtual_sections {
            for (index, vsection) in virtual_sections.iter().enumerate() {
                let virtual_record = VirtualSectionRecord {
                    tree_id: tree_id.to_string(),
                    virtual_index: index,
                    hash: vsection.hash.to_hex(),
                    primary_heading: vsection.primary_heading.as_ref().map(|h| h.text.clone()),
                    min_depth: vsection.min_depth,
                    max_depth: vsection.max_depth,
                    section_count: vsection.section_count,
                    total_blocks: vsection.total_blocks,
                    start_index: vsection.start_index,
                    end_index: vsection.end_index,
                    created_at: now,
                };

                self.client
                    .execute_query(&format!(
                        "UPDATE virtual_section:{{tree_id: $tree_id, index: $index}} CONTENT $vsection"
                    ))
                    .bind(("tree_id", tree_id))
                    .bind(("index", index))
                    .bind(("vsection", virtual_record))
                    .await
                    .map_err(|e| {
                        DbError::query(format!("Failed to store virtual section {}: {}", index, e))
                    })?;
            }
        }

        Ok(())
    }

    /// Retrieve a complete Hybrid Merkle tree
    ///
    /// This reconstructs the tree from stored metadata, sections, and virtual sections.
    ///
    /// # Arguments
    ///
    /// * `tree_id` - Unique identifier for the tree
    ///
    /// # Returns
    ///
    /// The reconstructed Hybrid Merkle tree or an error
    pub async fn retrieve_tree(&self, tree_id: &str) -> DbResult<HybridMerkleTree> {
        // 1. Retrieve tree metadata
        let tree_record: Option<HybridTreeRecord> = self
            .client
            .execute_query(&format!("SELECT * FROM hybrid_tree:{}", sanitize_id(tree_id)))
            .await
            .map_err(|e| DbError::query(format!("Failed to retrieve tree metadata: {}", e)))?
            .take(0)
            .map_err(|e| DbError::query(format!("Failed to parse tree metadata: {}", e)))?;

        let tree_record = tree_record.ok_or_else(|| {
            DbError::not_found(format!("Tree not found: {}", tree_id))
        })?;

        // 2. Retrieve sections
        let section_records: Vec<SectionRecord> = self
            .client
            .execute_query("SELECT * FROM section WHERE tree_id = $tree_id ORDER BY section_index")
            .bind(("tree_id", tree_id))
            .await
            .map_err(|e| DbError::query(format!("Failed to retrieve sections: {}", e)))?
            .take(0)
            .map_err(|e| DbError::query(format!("Failed to parse sections: {}", e)))?;

        // Deserialize sections from binary data
        let sections: Result<Vec<SectionNode>, _> = section_records
            .iter()
            .map(|record| {
                bincode::deserialize(&record.section_data)
                    .map_err(|e| DbError::serialization(format!("Failed to deserialize section: {}", e)))
            })
            .collect();
        let sections = sections?;

        // 3. Retrieve virtual sections if present
        let virtual_sections = if tree_record.is_virtualized {
            let virtual_records: Vec<VirtualSectionRecord> = self
                .client
                .execute_query("SELECT * FROM virtual_section WHERE tree_id = $tree_id ORDER BY virtual_index")
                .bind(("tree_id", tree_id))
                .await
                .map_err(|e| DbError::query(format!("Failed to retrieve virtual sections: {}", e)))?
                .take(0)
                .map_err(|e| DbError::query(format!("Failed to parse virtual sections: {}", e)))?;

            Some(
                virtual_records
                    .into_iter()
                    .map(|record| -> Result<VirtualSection, DbError> {
                        Ok(VirtualSection {
                            hash: NodeHash::from_hex(&record.hash)
                                .map_err(|e| DbError::serialization(format!("Invalid hash: {}", e)))?,
                            primary_heading: record.primary_heading.map(|text| {
                                crucible_core::merkle::HeadingSummary {
                                    text,
                                    level: record.min_depth,
                                }
                            }),
                            min_depth: record.min_depth,
                            max_depth: record.max_depth,
                            section_count: record.section_count,
                            total_blocks: record.total_blocks,
                            start_index: record.start_index,
                            end_index: record.end_index,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };

        // 4. Reconstruct the tree
        let root_hash = NodeHash::from_hex(&tree_record.root_hash)
            .map_err(|e| DbError::serialization(format!("Invalid root hash: {}", e)))?;

        Ok(HybridMerkleTree {
            root_hash,
            sections,
            total_blocks: tree_record.total_blocks,
            virtual_sections,
            is_virtualized: tree_record.is_virtualized,
        })
    }

    /// Delete a tree and all its associated data
    ///
    /// This removes the tree metadata, all sections, and virtual sections.
    ///
    /// # Arguments
    ///
    /// * `tree_id` - Unique identifier for the tree to delete
    pub async fn delete_tree(&self, tree_id: &str) -> DbResult<()> {
        // Delete in reverse order: virtual sections, sections, then tree
        self.client
            .execute_query("DELETE FROM virtual_section WHERE tree_id = $tree_id")
            .bind(("tree_id", tree_id))
            .await
            .map_err(|e| DbError::query(format!("Failed to delete virtual sections: {}", e)))?;

        self.client
            .execute_query("DELETE FROM section WHERE tree_id = $tree_id")
            .bind(("tree_id", tree_id))
            .await
            .map_err(|e| DbError::query(format!("Failed to delete sections: {}", e)))?;

        self.client
            .execute_query(&format!("DELETE hybrid_tree:{}", sanitize_id(tree_id)))
            .await
            .map_err(|e| DbError::query(format!("Failed to delete tree metadata: {}", e)))?;

        Ok(())
    }

    /// Update a tree incrementally based on changes
    ///
    /// This is more efficient than storing the entire tree when only a few sections changed.
    ///
    /// # Arguments
    ///
    /// * `tree_id` - Unique identifier for the tree
    /// * `tree` - The updated tree
    /// * `changed_section_indices` - Indices of sections that changed
    ///
    /// # Errors
    ///
    /// Returns an error if any section index is out of bounds
    pub async fn update_tree_incremental(
        &self,
        tree_id: &str,
        tree: &HybridMerkleTree,
        changed_section_indices: &[usize],
    ) -> DbResult<()> {
        // Validate all indices are in bounds before making any changes
        for &index in changed_section_indices {
            if index >= tree.sections.len() {
                return Err(DbError::InvalidOperation(format!(
                    "Invalid section index {}: tree '{}' has {} sections",
                    index,
                    tree_id,
                    tree.sections.len()
                )));
            }
        }

        let now = chrono::Utc::now();

        // 1. Update tree metadata
        self.client
            .execute_query(&format!(
                "UPDATE hybrid_tree:{} SET root_hash = $root_hash, updated_at = $updated_at",
                sanitize_id(tree_id)
            ))
            .bind(("root_hash", tree.root_hash.to_hex()))
            .bind(("updated_at", now))
            .await
            .map_err(|e| DbError::query(format!("Failed to update tree metadata: {}", e)))?;

        // 2. Update only changed sections
        // First compute cumulative block ranges
        let mut cumulative_blocks = 0;
        let mut block_ranges: Vec<(usize, usize)> = Vec::with_capacity(tree.sections.len());
        for section in &tree.sections {
            let start = cumulative_blocks;
            let end = cumulative_blocks + section.block_count;
            block_ranges.push((start, end));
            cumulative_blocks = end;
        }

        for &index in changed_section_indices {
            // Safe to index directly - we validated bounds above
            let section = &tree.sections[index];
            let section_data = bincode::serialize(section).map_err(|e| {
                DbError::serialization(format!("Failed to serialize section: {}", e))
            })?;

            let (start_block, end_block) = block_ranges[index];

            let section_record = SectionRecord {
                tree_id: tree_id.to_string(),
                section_index: index,
                section_hash: section.binary_tree.root_hash.to_hex(),
                heading: section.heading.as_ref().map(|h| h.text.clone()),
                depth: section.depth,
                start_block,
                end_block,
                section_data,
                created_at: now,
            };

            self.client
                .execute_query(&format!(
                    "UPDATE section:{{tree_id: $tree_id, index: $index}} CONTENT $section"
                ))
                .bind(("tree_id", tree_id))
                .bind(("index", index))
                .bind(("section", section_record))
                .await
                .map_err(|e| {
                    DbError::query(format!("Failed to update section {}: {}", index, e))
                })?;
        }

        Ok(())
    }

    /// Get tree metadata without loading full section data
    ///
    /// This is useful for quick lookups and comparisons.
    ///
    /// # Arguments
    ///
    /// * `tree_id` - Unique identifier for the tree
    ///
    /// # Returns
    ///
    /// Tree metadata or None if not found
    pub async fn get_tree_metadata(&self, tree_id: &str) -> DbResult<Option<HybridTreeRecord>> {
        self.client
            .execute_query(&format!("SELECT * FROM hybrid_tree:{}", sanitize_id(tree_id)))
            .await
            .map_err(|e| DbError::query(format!("Failed to retrieve tree metadata: {}", e)))?
            .take(0)
            .map_err(|e| DbError::query(format!("Failed to parse tree metadata: {}", e)))
    }

    /// List all stored trees
    ///
    /// # Returns
    ///
    /// List of tree metadata records
    pub async fn list_trees(&self) -> DbResult<Vec<HybridTreeRecord>> {
        self.client
            .execute_query("SELECT * FROM hybrid_tree ORDER BY updated_at DESC")
            .await
            .map_err(|e| DbError::query(format!("Failed to list trees: {}", e)))?
            .take(0)
            .map_err(|e| DbError::query(format!("Failed to parse tree list: {}", e)))
    }
}

/// Sanitize and validate ID for use in SurrealDB record IDs
///
/// This provides defense-in-depth protection against SQL injection and malformed IDs:
/// - Validates length (1-255 characters)
/// - Rejects control characters and null bytes
/// - Sanitizes filesystem and SQL injection characters
/// - Ensures only safe characters remain
///
/// # Arguments
///
/// * `id` - The identifier to sanitize
///
/// # Returns
///
/// Sanitized identifier safe for use in SurrealDB queries
///
/// # Panics
///
/// Panics if the ID is empty or longer than 255 characters. This is intentional
/// as these should be caught during development/testing.
fn sanitize_id(id: &str) -> String {
    // Validate length
    assert!(
        !id.is_empty() && id.len() <= 255,
        "Tree ID must be between 1 and 255 characters, got {} characters",
        id.len()
    );

    // Check for control characters and null bytes (security risk)
    assert!(
        !id.chars().any(|c| c.is_control() || c == '\0'),
        "Tree ID contains invalid control characters or null bytes"
    );

    // Sanitize: Replace all potentially dangerous characters with underscores
    // This includes:
    // - Filesystem separators: / \ :
    // - SQL injection characters: ' ; --
    // - Wildcards and special chars: * ? " < > |
    // - Whitespace (replace with underscore for clarity)
    id.chars()
        .map(|c| match c {
            // Filesystem separators
            '/' | '\\' | ':' => '_',
            // SQL injection risks
            '\'' | ';' | '-' if id.contains("--") => '_',
            // Wildcards and special characters
            '*' | '?' | '"' | '<' | '>' | '|' => '_',
            // Whitespace
            c if c.is_whitespace() => '_',
            // Allow alphanumeric, underscore, period, and hyphen
            c if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' => c,
            // Replace anything else with underscore
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurrealDbConfig;
    use crucible_core::parser::ParsedNote;
    use crucible_parser::types::{Heading, NoteContent, Paragraph};
    use std::path::PathBuf;

    fn create_test_note(content: &str) -> ParsedNote {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("test.md");
        doc.content = NoteContent::default();

        // Parse simple markdown structure
        let lines: Vec<&str> = content.lines().collect();
        let mut offset = 0;

        for line in lines {
            if line.starts_with("# ") {
                doc.content.headings.push(Heading {
                    level: 1,
                    text: line[2..].to_string(),
                    offset,
                    id: Some(line[2..].to_lowercase().replace(' ', "-")),
                });
            } else if !line.is_empty() {
                doc.content.paragraphs.push(Paragraph::new(line.to_string(), offset));
            }
            offset += line.len() + 1;
        }

        doc
    }

    async fn create_test_client() -> SurrealClient {
        let config = SurrealDbConfig {
            address: "memory".to_string(),
            namespace: "test".to_string(),
            database: "test".to_string(),
            username: "root".to_string(),
            password: "root".to_string(),
        };
        SurrealClient::new(config).await.unwrap()
    }

    #[tokio::test]
    async fn test_store_and_retrieve_simple_tree() {
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);

        // Create a simple test document
        let doc = create_test_note("# Heading\n\nContent here.");
        let tree = HybridMerkleTree::from_document(&doc);

        // Store the tree
        persistence
            .store_tree("test_doc_1", &tree)
            .await
            .expect("Failed to store tree");

        // Retrieve the tree
        let retrieved = persistence
            .retrieve_tree("test_doc_1")
            .await
            .expect("Failed to retrieve tree");

        // Verify root hash matches
        assert_eq!(tree.root_hash, retrieved.root_hash);
        assert_eq!(tree.sections.len(), retrieved.sections.len());
        assert_eq!(tree.total_blocks, retrieved.total_blocks);
    }

    #[tokio::test]
    async fn test_incremental_update() {
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);

        // Create and store initial tree
        let doc = create_test_note("# Heading\n\nOriginal content.");
        let tree = HybridMerkleTree::from_document(&doc);
        persistence.store_tree("test_doc_2", &tree).await.unwrap();

        // Create updated tree
        let updated_doc = create_test_note("# Heading\n\nUpdated content.");
        let updated_tree = HybridMerkleTree::from_document(&updated_doc);

        // Update incrementally (assume section 0 changed)
        persistence
            .update_tree_incremental("test_doc_2", &updated_tree, &[0])
            .await
            .expect("Failed to update tree");

        // Retrieve and verify
        let retrieved = persistence.retrieve_tree("test_doc_2").await.unwrap();
        assert_eq!(updated_tree.root_hash, retrieved.root_hash);
    }

    #[tokio::test]
    async fn test_delete_tree() {
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);

        let doc = create_test_note("# Test\n\nContent.");
        let tree = HybridMerkleTree::from_document(&doc);

        persistence.store_tree("test_doc_3", &tree).await.unwrap();
        persistence.delete_tree("test_doc_3").await.unwrap();

        // Verify tree is gone
        let result = persistence.get_tree_metadata("test_doc_3").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_trees() {
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);

        let doc1 = create_test_note("# Doc 1\n\nContent.");
        let doc2 = create_test_note("# Doc 2\n\nContent.");

        let tree1 = HybridMerkleTree::from_document(&doc1);
        let tree2 = HybridMerkleTree::from_document(&doc2);

        persistence.store_tree("list_test_1", &tree1).await.unwrap();
        persistence.store_tree("list_test_2", &tree2).await.unwrap();

        let trees = persistence.list_trees().await.unwrap();
        assert!(trees.len() >= 2);
    }

    #[test]
    fn test_sanitize_id() {
        // Basic filesystem path sanitization
        assert_eq!(sanitize_id("path/to/file.md"), "path_to_file.md");
        assert_eq!(sanitize_id("C:\\Windows\\path"), "C__Windows_path");
        assert_eq!(sanitize_id("normal_id"), "normal_id");

        // SQL injection attempts
        assert_eq!(sanitize_id("test'; DROP TABLE--"), "test___DROP_TABLE__");
        assert_eq!(sanitize_id("test'OR'1'='1"), "test_OR_1___1");

        // Whitespace handling
        assert_eq!(sanitize_id("file with spaces.md"), "file_with_spaces.md");
        assert_eq!(sanitize_id("tab\tseparated"), "tab_separated");

        // Special characters
        assert_eq!(sanitize_id("test<script>alert()</script>"), "test_script_alert___script_");
        assert_eq!(sanitize_id("wildcards*?.txt"), "wildcards___.txt");

        // Valid characters preserved
        assert_eq!(sanitize_id("valid-file_name.123.md"), "valid-file_name.123.md");
        assert_eq!(sanitize_id("UPPERCASE"), "UPPERCASE");
    }

    #[test]
    #[should_panic(expected = "Tree ID must be between 1 and 255 characters")]
    fn test_sanitize_id_empty() {
        sanitize_id("");
    }

    #[test]
    #[should_panic(expected = "Tree ID must be between 1 and 255 characters")]
    fn test_sanitize_id_too_long() {
        let long_id = "a".repeat(256);
        sanitize_id(&long_id);
    }

    #[test]
    #[should_panic(expected = "contains invalid control characters")]
    fn test_sanitize_id_null_byte() {
        sanitize_id("test\0file");
    }

    #[test]
    #[should_panic(expected = "contains invalid control characters")]
    fn test_sanitize_id_control_chars() {
        sanitize_id("test\x01\x02file");
    }

    #[tokio::test]
    async fn test_update_incremental_invalid_index() {
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);

        let doc = create_test_note("# Test\n\nContent.");
        let tree = HybridMerkleTree::from_document(&doc);

        // Try to update with out-of-bounds index
        let result = persistence
            .update_tree_incremental("test_bounds", &tree, &[999])
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid section index"));
        assert!(err_msg.contains("999"));
    }
}
