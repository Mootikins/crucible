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

use crate::{DbError, DbResult, SurrealClient};
use crucible_merkle::{HybridMerkleTree, NodeHash, SectionNode, VirtualSection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current serialization format version
///
/// Increment this when making breaking changes to SectionNode serialization.
/// This enables migration support for backward compatibility.
const SECTION_FORMAT_VERSION: u32 = 1;

/// Escape a string for use in SurrealDB single-quoted string literals.
///
/// SurrealDB treats backslash as an escape character in single-quoted strings.
/// Valid escape sequences are: `\\`, `\'`, `/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\u`.
/// Any other backslash sequence (like `\ `) causes a parse error.
///
/// This function escapes backslashes and single quotes to make arbitrary strings
/// safe for embedding in SurrealQL queries.
fn escape_surql_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_section_count: Option<usize>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Optional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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

        // Build UPSERT query with inline JSON (avoiding NONE parameter issue)
        // Build content fields, conditionally including optional fields
        // Note: We store the original tree_id as 'document_path' since SurrealDB doesn't allow
        // an 'id' field in CONTENT when a specific record ID is specified
        let mut content_fields = format!(
            "document_path: '{}',
            root_hash: '{}',
            section_count: {},
            total_blocks: {},
            is_virtualized: {},
            created_at: '{}',
            updated_at: '{}'",
            escape_surql_string(&tree_record.id),
            tree_record.root_hash,
            tree_record.section_count,
            tree_record.total_blocks,
            tree_record.is_virtualized,
            tree_record.created_at.to_rfc3339(),
            tree_record.updated_at.to_rfc3339()
        );

        if let Some(virtual_section_count) = tree_record.virtual_section_count {
            content_fields.push_str(&format!(
                ",\n            virtual_section_count: {}",
                virtual_section_count
            ));
        }

        if let Some(metadata) = &tree_record.metadata {
            let metadata_json = serde_json::to_string(metadata)
                .map_err(|e| DbError::Internal(format!("Failed to serialize metadata: {}", e)))?;
            content_fields.push_str(&format!(",\n            metadata: {}", metadata_json));
        }

        // Use URL-encoded ID for the record ID to safely handle paths with spaces and special chars
        // This replaces the problematic sanitize_id() which lost information
        let record_id = urlencoding::encode(tree_id).to_string();

        // Use the old query pattern but with URL-encoded ID instead of sanitized ID
        // This preserves the original path while safely handling special characters
        let query = format!(
            "UPSERT hybrid_tree:`{}` CONTENT {{
                {}
            }}",
            record_id, content_fields
        );

        self.client
            .query(&query, &[])
            .await
            .map_err(|e| DbError::Query(format!("Failed to store tree metadata: {}", e)))?;

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

            // Use path sharding: section_{tree_id}_{index}
            self.client
                .query(
                    "UPSERT section:{tree_id: $tree_id, index: $index} CONTENT $section",
                    &[serde_json::json!({
                        "tree_id": tree_id,
                        "index": index,
                        "section": section_record,
                    })],
                )
                .await
                .map_err(|e| DbError::Query(format!("Failed to store section {}: {}", index, e)))?;
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
                        "UPSERT virtual_section:{tree_id: $tree_id, index: $index} CONTENT $virtual_section",
                        &[serde_json::json!({
                            "tree_id": tree_id,
                            "index": index,
                            "virtual_section": virtual_record,
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
        // 1. Retrieve tree metadata using URL-encoded ID
        let record_id = urlencoding::encode(tree_id).to_string();
        let query_result = self
            .client
            .query(&format!("SELECT * FROM hybrid_tree:`{}`", record_id), &[])
            .await
            .map_err(|e| DbError::Query(format!("Failed to retrieve tree metadata: {}", e)))?;

        let tree_record: HybridTreeRecord = query_result
            .records
            .first()
            .ok_or_else(|| DbError::NotFound(format!("Tree not found: {}", tree_id)))
            .and_then(|record| {
                // Map 'document_path' field to 'id' for deserialization
                // (We store as document_path because SurrealDB doesn't allow 'id' in CONTENT)
                let mut data = record.data.clone();
                if let Some(doc_path) = data.remove("document_path") {
                    data.insert("id".to_string(), doc_path);
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
                                crucible_merkle::HeadingSummary {
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

        let record_id = urlencoding::encode(tree_id).to_string();
        self.client
            .query(&format!("DELETE hybrid_tree:`{}`", record_id), &[])
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

        // 1. Update tree metadata using URL-encoded ID
        let record_id = urlencoding::encode(tree_id).to_string();
        let query = format!(
            "UPDATE hybrid_tree:`{}` SET root_hash = '{}', updated_at = '{}'",
            record_id,
            tree.root_hash.to_hex(),
            now.to_rfc3339()
        );

        self.client
            .query(&query, &[])
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
        let record_id = urlencoding::encode(tree_id).to_string();
        let query_result = self
            .client
            .query(&format!("SELECT * FROM hybrid_tree:`{}`", record_id), &[])
            .await
            .map_err(|e| DbError::Query(format!("Failed to retrieve tree metadata: {}", e)))?;

        query_result
            .records
            .first()
            .map(|record| {
                // Map 'document_path' field to 'id' for deserialization
                // (We store as document_path because SurrealDB doesn't allow 'id' in CONTENT)
                let mut data = record.data.clone();
                if let Some(doc_path) = data.remove("document_path") {
                    data.insert("id".to_string(), doc_path);
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
                // Map 'document_path' field to 'id' for deserialization
                // (We store as document_path because SurrealDB doesn't allow 'id' in CONTENT)
                let mut data = record.data.clone();
                if let Some(doc_path) = data.remove("document_path") {
                    data.insert("id".to_string(), doc_path);
                }
                serde_json::from_value(serde_json::to_value(&data).unwrap())
                    .map_err(|e| DbError::Query(format!("Failed to parse tree record: {}", e)))
            })
            .collect::<Result<Vec<_>, _>>()
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
/// Convert tree ID to URL-encoded record ID for SurrealDB
///
/// This preserves the original path (including spaces and special characters)
/// while making it safe for use as a database record ID.
///
/// # Arguments
///
/// * `tree_id` - The tree identifier (usually a file path)
///
/// # Returns
///
/// URL-encoded string safe for use in SurrealDB queries
///
/// # Panics
///
/// Panics if the ID is empty or longer than 255 characters.
#[allow(dead_code)]
fn tree_record_id(tree_id: &str) -> String {
    assert!(
        !tree_id.is_empty() && tree_id.len() <= 255,
        "Tree ID must be between 1 and 255 characters, got {} characters",
        tree_id.len()
    );
    urlencoding::encode(tree_id).to_string()
}

/// Deprecated: Convert tree ID to URL-encoded record ID
///
/// This helper function URL-encodes tree IDs for safe use as SurrealDB record IDs.
/// Unlike the deprecated `sanitize_id()`, this preserves the original path.
#[allow(dead_code)]
fn tree_record_id_helper(tree_id: &str) -> String {
    tree_record_id(tree_id)
}

/// Sanitized identifier safe for use in SurrealDB queries
///
/// # Deprecated
///
/// This function is deprecated because it loses information by converting spaces
/// to underscores, causing collisions. Use `tree_record_id()` instead.
///
/// # Panics
///
/// Panics if the ID is empty or longer than 255 characters. This is intentional
/// as these should be caught during development/testing.
#[allow(dead_code)]
fn sanitize_id(id: &str) -> String {
    // Validate length
    assert!(
        !id.is_empty() && id.len() <= 255,
        "Tree ID must be between 1 and 255 characters, got {} characters",
        id.len()
    );

    // Sanitize: Replace all potentially dangerous characters with underscores
    // Note: We sanitize control characters rather than rejecting them,
    // as the sanitization logic below handles them appropriately
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

// Implement the MerkleStore trait for SurrealDB backend
#[async_trait::async_trait]
impl crucible_merkle::MerkleStore for MerklePersistence {
    async fn store(&self, id: &str, tree: &HybridMerkleTree) -> crucible_merkle::StorageResult<()> {
        self.store_tree(id, tree)
            .await
            .map_err(|e| crucible_merkle::StorageError::Storage(e.to_string()))
    }

    async fn retrieve(&self, id: &str) -> crucible_merkle::StorageResult<HybridMerkleTree> {
        self.retrieve_tree(id).await.map_err(|e| match e {
            DbError::NotFound(_) => crucible_merkle::StorageError::NotFound(id.to_string()),
            _ => crucible_merkle::StorageError::Storage(e.to_string()),
        })
    }

    async fn delete(&self, id: &str) -> crucible_merkle::StorageResult<()> {
        self.delete_tree(id)
            .await
            .map_err(|e| crucible_merkle::StorageError::Storage(e.to_string()))
    }

    async fn get_metadata(
        &self,
        id: &str,
    ) -> crucible_merkle::StorageResult<Option<crucible_merkle::TreeMetadata>> {
        let meta = self
            .get_tree_metadata(id)
            .await
            .map_err(|e| crucible_merkle::StorageError::Storage(e.to_string()))?;

        Ok(meta.map(|m| crucible_merkle::TreeMetadata {
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
    ) -> crucible_merkle::StorageResult<()> {
        self.update_tree_incremental(id, tree, changed_sections)
            .await
            .map_err(|e| match e {
                DbError::InvalidOperation(_) => {
                    crucible_merkle::StorageError::InvalidOperation(e.to_string())
                }
                DbError::NotFound(_) => crucible_merkle::StorageError::NotFound(id.to_string()),
                _ => crucible_merkle::StorageError::Storage(e.to_string()),
            })
    }

    async fn list_trees(
        &self,
    ) -> crucible_merkle::StorageResult<Vec<crucible_merkle::TreeMetadata>> {
        let trees = self
            .list_trees()
            .await
            .map_err(|e| crucible_merkle::StorageError::Storage(e.to_string()))?;

        Ok(trees
            .into_iter()
            .map(|m| crucible_merkle::TreeMetadata {
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
    use crucible_core::parser::{Heading, NoteContent, Paragraph};
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
            max_connections: Some(10),
            timeout_seconds: Some(30),
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
        assert_eq!(
            sanitize_id("test<script>alert()</script>"),
            "test_script_alert____script_"
        );
        assert_eq!(sanitize_id("wildcards*?.txt"), "wildcards__.txt");

        // Valid characters preserved
        assert_eq!(
            sanitize_id("valid-file_name.123.md"),
            "valid-file_name.123.md"
        );
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
    fn test_sanitize_id_null_byte() {
        // Null bytes are sanitized to underscores
        assert_eq!(sanitize_id("test\0file"), "test_file");
    }

    #[test]
    fn test_sanitize_id_control_chars() {
        // Control characters are sanitized to underscores
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

    // BUG #1 TESTS: File paths with spaces cause SurrealDB parse errors
    // These tests demonstrate the backslash escaping bug documented in BUGS.md
    // Expected to FAIL until parameterized queries are implemented

    #[tokio::test]
    async fn test_store_tree_with_spaces_in_path() {
        // Bug: File paths with spaces are sanitized to underscores, losing the original path
        // This means files stored with "Project Notes/File.md" can't be retrieved with that exact path

        // Given: A file path with spaces (like real Obsidian files)
        let original_path = "Projects/Rune MCP/YouTube Transcript Tool - Implementation.md";
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);
        let doc = create_test_note("# Test\n\nContent with some text.");
        let tree = HybridMerkleTree::from_document(&doc);

        // When: We store the tree using the original path
        let result = persistence.store_tree(original_path, &tree).await;
        assert!(result.is_ok(), "Store should succeed");

        // Then: We should be able to retrieve using the EXACT SAME path
        // Currently this works because sanitize_id is called on both store and retrieve
        let retrieved = persistence.retrieve_tree(original_path).await;
        assert!(retrieved.is_ok(), "Retrieve should work with original path");

        // And: The tree metadata should store the original unsanitized path
        let metadata = persistence.get_tree_metadata(original_path).await.unwrap();
        assert!(metadata.is_some(), "Metadata should exist");
        let metadata = metadata.unwrap();

        // THIS IS THE ACTUAL BUG: metadata.id is sanitized to underscores
        // Expected: "Projects/Rune MCP/YouTube Transcript Tool - Implementation.md"
        // Actual: "Projects_Rune_MCP_YouTube_Transcript_Tool_-_Implementation.md"
        assert_eq!(
            metadata.id, original_path,
            "BUG: Path is sanitized! Expected spaces preserved but got underscores"
        );
    }

    #[tokio::test]
    async fn test_store_tree_with_multiple_spaces_and_special_chars() {
        // Given: A complex path with multiple spaces and special characters
        let tree_id = "My Projects/Research Notes/AI & ML/Deep Learning - Part 1.md";
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);
        let doc = create_test_note("# Deep Learning\n\n## Introduction\n\nSome content here.");
        let tree = HybridMerkleTree::from_document(&doc);

        // When: We store the tree
        let result = persistence.store_tree(tree_id, &tree).await;

        // Then: It should succeed
        assert!(
            result.is_ok(),
            "Should handle complex paths with spaces and special chars, got: {:?}",
            result.err()
        );

        // And: Retrieval should work
        let retrieved = persistence.retrieve_tree(tree_id).await;
        assert!(retrieved.is_ok(), "Should retrieve complex path");
        assert_eq!(tree.root_hash, retrieved.unwrap().root_hash);
    }

    #[tokio::test]
    async fn test_update_tree_incremental_with_spaces() {
        // Given: A tree stored with spaces in path
        let tree_id = "Notes/Daily Notes/2025-01-15 Meeting Notes.md";
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);

        // Store initial tree
        let doc_v1 = create_test_note("# Meeting\n\nInitial notes.");
        let tree_v1 = HybridMerkleTree::from_document(&doc_v1);
        persistence.store_tree(tree_id, &tree_v1).await.unwrap();

        // When: We update with new content
        let doc_v2 =
            create_test_note("# Meeting\n\nInitial notes.\n\n## Action Items\n\nNew section.");
        let tree_v2 = HybridMerkleTree::from_document(&doc_v2);
        let result = persistence
            .update_tree_incremental(tree_id, &tree_v2, &[0])
            .await;

        // Then: Update should succeed
        assert!(
            result.is_ok(),
            "Should update tree with spaces in path, got: {:?}",
            result.err()
        );

        // And: Retrieved tree should have new hash
        let retrieved = persistence.retrieve_tree(tree_id).await.unwrap();
        assert_eq!(
            tree_v2.root_hash, retrieved.root_hash,
            "Should have updated hash"
        );
        assert_ne!(
            tree_v1.root_hash, retrieved.root_hash,
            "Hash should have changed"
        );
    }

    #[tokio::test]
    async fn test_delete_tree_with_spaces() {
        // Given: A tree stored with spaces
        let tree_id = "Archive/Old Projects/Legacy System.md";
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);
        let doc = create_test_note("# Legacy\n\nOld content.");
        let tree = HybridMerkleTree::from_document(&doc);

        persistence.store_tree(tree_id, &tree).await.unwrap();

        // Verify it exists
        assert!(persistence.retrieve_tree(tree_id).await.is_ok());

        // When: We delete the tree
        let result = persistence.delete_tree(tree_id).await;

        // Then: Deletion should succeed
        assert!(
            result.is_ok(),
            "Should delete tree with spaces in path, got: {:?}",
            result.err()
        );

        // And: Tree should no longer exist
        let retrieved = persistence.retrieve_tree(tree_id).await;
        assert!(retrieved.is_err(), "Tree should not exist after deletion");
    }

    #[tokio::test]
    async fn test_paths_without_spaces_still_work() {
        // Regression test: ensure normal paths without spaces still work
        let tree_id = "projects/notes/test.md";
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);
        let doc = create_test_note("# Test\n\nNormal content.");
        let tree = HybridMerkleTree::from_document(&doc);

        // When/Then: All operations should still work
        persistence.store_tree(tree_id, &tree).await.unwrap();
        let retrieved = persistence.retrieve_tree(tree_id).await.unwrap();
        assert_eq!(tree.root_hash, retrieved.root_hash);
        persistence.delete_tree(tree_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_path_collision_bug_spaces_vs_underscores() {
        // BUG: sanitize_id() causes collisions between "My File.md" and "My_File.md"
        // Both paths get sanitized to "My_File.md" causing data loss

        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);

        // Given: Two different files, one with spaces and one with underscores
        let path_with_spaces = "My Project Notes.md";
        let path_with_underscores = "My_Project_Notes.md";

        let doc1 = create_test_note("# File One\n\nContent from file with spaces.");
        let doc2 = create_test_note("# File Two\n\nDifferent content from file with underscores.");

        let tree1 = HybridMerkleTree::from_document(&doc1);
        let tree2 = HybridMerkleTree::from_document(&doc2);

        // Store first file
        persistence
            .store_tree(path_with_spaces, &tree1)
            .await
            .unwrap();

        // Store second file - this will OVERWRITE the first due to ID collision
        persistence
            .store_tree(path_with_underscores, &tree2)
            .await
            .unwrap();

        // Try to retrieve both
        let retrieved1 = persistence.retrieve_tree(path_with_spaces).await.unwrap();
        let retrieved2 = persistence
            .retrieve_tree(path_with_underscores)
            .await
            .unwrap();

        // BUG: Both retrievals return the SAME tree (tree2)!
        // This assertion SHOULD FAIL but currently PASSES due to the bug
        assert_ne!(
            retrieved1.root_hash, retrieved2.root_hash,
            "BUG: Paths with spaces and underscores collide! Both resolve to same tree"
        );
    }

    #[tokio::test]
    async fn test_store_tree_with_backslash_in_path() {
        // Bug reproduction: File paths with literal backslash-space sequences
        // (like "YouTube\ Transcript\ Tool.md") cause SurrealDB parse errors
        // because backslash is an escape character in single-quoted strings.
        //
        // Error: "Invalid escape character ` `, valid characters are `\`, `'`, `/`, `b`, `f`, `n`, `r`, `t`, or `u`"

        // Given: A file path with literal backslash characters (not shell escaping)
        let tree_id = r"Projects/Rune MCP/YouTube\ Transcript\ Tool\ -\ Implementation.md";
        let client = create_test_client().await;
        let persistence = MerklePersistence::new(client);
        let doc = create_test_note("# YouTube Transcript Tool\n\nResearch notes.");
        let tree = HybridMerkleTree::from_document(&doc);

        // When: We store the tree
        let result = persistence.store_tree(tree_id, &tree).await;

        // Then: It should succeed (currently fails with parse error)
        assert!(
            result.is_ok(),
            "Should handle paths with backslash characters, got: {:?}",
            result.err()
        );

        // And: We should be able to retrieve it
        let retrieved = persistence.retrieve_tree(tree_id).await;
        assert!(
            retrieved.is_ok(),
            "Should retrieve tree with backslash in path"
        );
        assert_eq!(tree.root_hash, retrieved.unwrap().root_hash);

        // And: The metadata should preserve the original path
        let metadata = persistence.get_tree_metadata(tree_id).await.unwrap();
        assert!(metadata.is_some(), "Metadata should exist");
        assert_eq!(
            metadata.unwrap().id,
            tree_id,
            "Path should be preserved exactly"
        );
    }
}
