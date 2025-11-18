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

use crate::utils::sanitize_record_id as sanitize_record_id_result;
use crate::{DbError, DbResult, RecordId, SurrealClient};
use crucible_core::merkle::{HybridMerkleTree, NodeHash, SectionNode, VirtualSection};
use crucible_core::parser::ParsedNote;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current serialization format version
///
/// Increment this when making breaking changes to SectionNode serialization.
/// This enables migration support for backward compatibility.
const SECTION_FORMAT_VERSION: u32 = 1;

/// Versioned wrapper for SectionNode binary serialization
///
/// This enables detecting format changes and migrating old data.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionedSection {
    /// Format version number
    version: u32,
    /// The actual section data
    data: SectionNode,
}

/// Database record for Hybrid Merkle tree metadata
///
/// This stores the core tree structure and configuration without the full
/// section data, which is stored separately for efficiency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone)]
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
            virtual_section_count: tree.virtual_sections.as_ref().map(|vs| vs.len()),
            created_at: now,
            updated_at: now,
            metadata: None,
        };

        // Use UPSERT to insert or update the tree record
        let safe_id = sanitize_id(tree_id);
        self.client
            .query(
                &format!("UPSERT hybrid_tree:{} CONTENT $tree", safe_id),
                &[serde_json::json!({"tree": tree_record})],
            )
            .await
            .map_err(|e| DbError::Query(format!("Failed to upsert tree metadata: {}", e)))?;

        // 2. Store sections with binary encoding
        let mut cumulative_blocks = 0;
        for (index, section) in tree.sections.iter().enumerate() {
            // Wrap section in versioned envelope before serialization
            let versioned = VersionedSection {
                version: SECTION_FORMAT_VERSION,
                data: section.clone(),
            };
            let section_data = bincode::serialize(&versioned)
                .map_err(|e| DbError::Internal(format!("Failed to serialize section: {}", e)))?;

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

            // Use UPSERT with path sharding: section_{tree_id}_{index}
            self.client
                .query(
                    "UPSERT section:{tree_id: $tree_id, index: $index} CONTENT $section",
                    &[serde_json::json!({
                        "tree_id": tree_id,
                        "index": index,
                        "section": section_record
                    })],
                )
                .await
                .map_err(|e| DbError::Query(format!("Failed to upsert section {}: {}", index, e)))?;
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
                    .query(
                        "UPSERT virtual_section:{tree_id: $tree_id, index: $index} CONTENT $vsection",
                        &[serde_json::json!({
                            "tree_id": tree_id,
                            "index": index,
                            "vsection": virtual_record
                        })]
                    )
                    .await
                    .map_err(|e| {
                        DbError::Query(format!("Failed to store virtual section {}: {}", index, e))
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
        let safe_id = sanitize_id(tree_id);
        let query_result = self
            .client
            .query(
                &format!("SELECT * FROM hybrid_tree:{}", safe_id),
                &[],
            )
            .await
            .map_err(|e| DbError::Query(format!("Failed to retrieve tree metadata: {}", e)))?;

        let tree_record: HybridTreeRecord = query_result
            .records
            .first()
            .ok_or_else(|| DbError::NotFound(format!("Tree not found: {}", tree_id)))
            .and_then(|record| {
                // Extract the ID from the record and add it to the data
                let mut data = record.data.clone();
                if let Some(ref record_id) = record.id {
                    // Extract just the ID part from "table:id" format
                    let id_str = record_id.0.split(':').last().unwrap_or(&record_id.0);
                    data.insert("id".to_string(), serde_json::Value::String(id_str.to_string()));
                }
                serde_json::from_value(serde_json::to_value(&data).unwrap())
                    .map_err(|e| DbError::Query(format!("Failed to parse tree metadata: {}", e)))
            })?;

        // 2. Retrieve sections
        let query_result = self
            .client
            .query(
                "SELECT * FROM section WHERE tree_id = $tree_id ORDER BY section_index",
                &[serde_json::json!({"tree_id": tree_id})],
            )
            .await
            .map_err(|e| DbError::Query(format!("Failed to retrieve sections: {}", e)))?;

        let section_records: Vec<SectionRecord> = query_result
            .records
            .iter()
            .map(|record| {
                serde_json::from_value(serde_json::to_value(&record.data).unwrap())
                    .map_err(|e| DbError::Query(format!("Failed to parse section: {}", e)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Deserialize sections from binary data with version checking
        let sections: Result<Vec<SectionNode>, _> = section_records
            .iter()
            .map(|record| {
                let versioned: VersionedSection = bincode::deserialize(&record.section_data)
                    .map_err(|e| {
                        DbError::Internal(format!("Failed to deserialize section: {}", e))
                    })?;

                // Check version and migrate if needed
                if versioned.version != SECTION_FORMAT_VERSION {
                    // For now, we only have version 1, so this is an error
                    // In the future, add migration logic here
                    return Err(DbError::Internal(format!(
                        "Unsupported section format version: expected {}, got {}",
                        SECTION_FORMAT_VERSION, versioned.version
                    )));
                }

                Ok(versioned.data)
            })
            .collect();
        let sections = sections?;

        // 3. Retrieve virtual sections if present
        let virtual_sections = if tree_record.is_virtualized {
            let query_result = self
                .client
                .query(
                    "SELECT * FROM virtual_section WHERE tree_id = $tree_id ORDER BY virtual_index",
                    &[serde_json::json!({"tree_id": tree_id})],
                )
                .await
                .map_err(|e| {
                    DbError::Query(format!("Failed to retrieve virtual sections: {}", e))
                })?;

            let virtual_records: Vec<VirtualSectionRecord> = query_result
                .records
                .iter()
                .map(|record| {
                    serde_json::from_value(serde_json::to_value(&record.data).unwrap()).map_err(
                        |e| DbError::Query(format!("Failed to parse virtual section: {}", e)),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;

            Some(
                virtual_records
                    .into_iter()
                    .map(|record| -> Result<VirtualSection, DbError> {
                        Ok(VirtualSection {
                            hash: NodeHash::from_hex(&record.hash)
                                .map_err(|e| DbError::Internal(format!("Invalid hash: {}", e)))?,
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
            .map_err(|e| DbError::Internal(format!("Invalid root hash: {}", e)))?;

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
            .query(
                "DELETE FROM virtual_section WHERE tree_id = $tree_id",
                &[serde_json::json!({"tree_id": tree_id})],
            )
            .await
            .map_err(|e| DbError::Query(format!("Failed to delete virtual sections: {}", e)))?;

        self.client
            .query(
                "DELETE FROM section WHERE tree_id = $tree_id",
                &[serde_json::json!({"tree_id": tree_id})],
            )
            .await
            .map_err(|e| DbError::Query(format!("Failed to delete sections: {}", e)))?;

        let safe_id = sanitize_id(tree_id);
        self.client
            .query(&format!("DELETE hybrid_tree:{}", safe_id), &[])
            .await
            .map_err(|e| DbError::Query(format!("Failed to delete tree metadata: {}", e)))?;

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
            .query(
                &format!(
                    "UPDATE hybrid_tree:{} SET root_hash = $root_hash, updated_at = $updated_at",
                    sanitize_id(tree_id)
                ),
                &[serde_json::json!({
                    "root_hash": tree.root_hash.to_hex(),
                    "updated_at": now
                })],
            )
            .await
            .map_err(|e| DbError::Query(format!("Failed to update tree metadata: {}", e)))?;

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
            let versioned = VersionedSection {
                version: SECTION_FORMAT_VERSION,
                data: section.clone(),
            };
            let section_data = bincode::serialize(&versioned)
                .map_err(|e| DbError::Internal(format!("Failed to serialize section: {}", e)))?;

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
                .query(
                    "UPDATE section:{tree_id: $tree_id, index: $index} CONTENT $section",
                    &[serde_json::json!({
                        "tree_id": tree_id,
                        "index": index,
                        "section": section_record
                    })],
                )
                .await
                .map_err(|e| {
                    DbError::Query(format!("Failed to update section {}: {}", index, e))
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
        let safe_id = sanitize_id(tree_id);
        let query_result = self
            .client
            .query(
                &format!("SELECT * FROM hybrid_tree:{}", safe_id),
                &[],
            )
            .await
            .map_err(|e| DbError::Query(format!("Failed to retrieve tree metadata: {}", e)))?;

        query_result
            .records
            .first()
            .map(|record| {
                // Extract the ID from the record and add it to the data
                let mut data = record.data.clone();
                if let Some(ref record_id) = record.id {
                    // Extract just the ID part from "table:id" format
                    let id_str = record_id.0.split(':').last().unwrap_or(&record_id.0);
                    data.insert("id".to_string(), serde_json::Value::String(id_str.to_string()));
                }
                serde_json::from_value(serde_json::to_value(&data).unwrap())
                    .map_err(|e| DbError::Query(format!("Failed to parse tree metadata: {}", e)))
            })
            .transpose()
    }

    /// List all stored trees
    ///
    /// # Returns
    ///
    /// List of tree metadata records
    pub async fn list_trees(&self) -> DbResult<Vec<HybridTreeRecord>> {
        let query_result = self
            .client
            .query("SELECT * FROM hybrid_tree ORDER BY updated_at DESC", &[])
            .await
            .map_err(|e| DbError::Query(format!("Failed to list trees: {}", e)))?;

        query_result
            .records
            .iter()
            .map(|record| {
                // Extract the ID from the record and add it to the data
                let mut data = record.data.clone();
                if let Some(ref record_id) = record.id {
                    // Extract just the ID part from "table:id" format
                    let id_str = record_id.0.split(':').last().unwrap_or(&record_id.0);
                    data.insert("id".to_string(), serde_json::Value::String(id_str.to_string()));
                }
                serde_json::from_value(serde_json::to_value(&data).unwrap())
                    .map_err(|e| DbError::Query(format!("Failed to parse tree record: {}", e)))
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

/// Sanitize and validate ID for use in SurrealDB record IDs
///
/// This is a wrapper around the shared `sanitize_record_id` function
/// that panics on error for backward compatibility with existing code.
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
/// Panics if the ID is empty, longer than 255 characters, or contains invalid characters.
/// This is intentional as these should be caught during development/testing.
fn sanitize_id(id: &str) -> String {
    sanitize_record_id_result(id)
        .unwrap_or_else(|e| panic!("Invalid tree ID '{}': {}", id, e))
}

// Implement the MerkleStore trait for SurrealDB backend
#[async_trait::async_trait]
impl crucible_core::merkle::MerkleStore for MerklePersistence {
    async fn store(
        &self,
        id: &str,
        tree: &HybridMerkleTree,
    ) -> crucible_core::merkle::StorageResult<()> {
        self.store_tree(id, tree)
            .await
            .map_err(|e| crucible_core::merkle::StorageError::Storage(e.to_string()))
    }

    async fn retrieve(&self, id: &str) -> crucible_core::merkle::StorageResult<HybridMerkleTree> {
        self.retrieve_tree(id).await.map_err(|e| match e {
            DbError::NotFound(_) => crucible_core::merkle::StorageError::NotFound(id.to_string()),
            _ => crucible_core::merkle::StorageError::Storage(e.to_string()),
        })
    }

    async fn delete(&self, id: &str) -> crucible_core::merkle::StorageResult<()> {
        self.delete_tree(id)
            .await
            .map_err(|e| crucible_core::merkle::StorageError::Storage(e.to_string()))
    }

    async fn get_metadata(
        &self,
        id: &str,
    ) -> crucible_core::merkle::StorageResult<Option<crucible_core::merkle::TreeMetadata>> {
        let meta = self
            .get_tree_metadata(id)
            .await
            .map_err(|e| crucible_core::merkle::StorageError::Storage(e.to_string()))?;

        Ok(meta.map(|m| crucible_core::merkle::TreeMetadata {
            id: m.id,
            root_hash: m.root_hash,
            section_count: m.section_count,
            total_blocks: m.total_blocks,
            is_virtualized: m.is_virtualized,
            virtual_section_count: m.virtual_section_count.unwrap_or(0),
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
            metadata: m.metadata,
        }))
    }

    async fn update_incremental(
        &self,
        id: &str,
        tree: &HybridMerkleTree,
        changed_sections: &[usize],
    ) -> crucible_core::merkle::StorageResult<()> {
        self.update_tree_incremental(id, tree, changed_sections)
            .await
            .map_err(|e| match e {
                DbError::InvalidOperation(_) => {
                    crucible_core::merkle::StorageError::InvalidOperation(e.to_string())
                }
                DbError::NotFound(_) => {
                    crucible_core::merkle::StorageError::NotFound(id.to_string())
                }
                _ => crucible_core::merkle::StorageError::Storage(e.to_string()),
            })
    }

    async fn list_trees(
        &self,
    ) -> crucible_core::merkle::StorageResult<Vec<crucible_core::merkle::TreeMetadata>> {
        let trees = self
            .list_trees()
            .await
            .map_err(|e| crucible_core::merkle::StorageError::Storage(e.to_string()))?;

        Ok(trees
            .into_iter()
            .map(|m| crucible_core::merkle::TreeMetadata {
                id: m.id,
                root_hash: m.root_hash,
                section_count: m.section_count,
                total_blocks: m.total_blocks,
                is_virtualized: m.is_virtualized,
                virtual_section_count: m.virtual_section_count.unwrap_or(0),
                created_at: m.created_at.to_rfc3339(),
                updated_at: m.updated_at.to_rfc3339(),
                metadata: m.metadata,
            })
            .collect())
    }
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
                doc.content
                    .paragraphs
                    .push(Paragraph::new(line.to_string(), offset));
            }
            offset += line.len() + 1;
        }

        doc
    }

    async fn create_test_client() -> SurrealClient {
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            namespace: "test".to_string(),
            database: "test".to_string(),
            max_connections: None,
            timeout_seconds: None,
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
        // Input: "test<script>alert()</script>"
        // < -> _, script, > -> _, alert, ( -> _, ) -> _, < -> _, / -> _, script, > -> _
        assert_eq!(
            sanitize_id("test<script>alert()</script>"),
            "test_script_alert____script_"
        );
        // Input: "wildcards*?.txt" - * -> _, ? -> _, .txt
        assert_eq!(sanitize_id("wildcards*?.txt"), "wildcards__.txt");

        // Valid characters preserved
        assert_eq!(
            sanitize_id("valid-file_name.123.md"),
            "valid-file_name.123.md"
        );
        assert_eq!(sanitize_id("UPPERCASE"), "UPPERCASE");
    }

    #[test]
    #[should_panic(expected = "Record ID cannot be empty")]
    fn test_sanitize_id_empty() {
        sanitize_id("");
    }

    #[test]
    #[should_panic(expected = "Record ID must be between 1 and 255 characters")]
    fn test_sanitize_id_too_long() {
        let long_id = "a".repeat(256);
        sanitize_id(&long_id);
    }

    #[test]
    #[should_panic(expected = "Record ID contains invalid control characters")]
    fn test_sanitize_id_null_byte() {
        sanitize_id("test\0file");
    }

    #[test]
    fn test_sanitize_id_control_chars() {
        // Control characters (except null) are sanitized to underscores
        assert_eq!(sanitize_id("test\x01\x02file"), "test__file");
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
