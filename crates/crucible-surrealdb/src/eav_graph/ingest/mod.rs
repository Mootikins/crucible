//! Note Ingestion with Advanced Embed Processing
//!
//! This module provides high-level functionality for ingesting parsed documents into the
//! EAV+Graph schema with comprehensive support for embed processing and wikilink resolution.
//!
//! ## Embed Processing Features
//!
//! The ingestor supports sophisticated embed processing including:
//!
//! ### Embed Type Classification
//! - **Images**: PNG, JPG, SVG, WebP, etc. with format-specific metadata
//! - **Videos**: MP4, AVI, MOV, WebM, plus platform detection (YouTube, Vimeo)
//! - **Audio**: MP3, WAV, M4A, OGG, plus platform detection (SoundCloud, Spotify)
//! - **Documents**: PDF files with note-specific processing
//! - **Notes**: Markdown files with internal note embedding
//! - **External**: URLs and web content with security validation
//!
//! ### Embed Variant Support
//! - Simple embeds: `![[Note]]`
//! - Heading references: `![[Note#Section]]`
//! - Block references: `![[Note^block-id]]`
//! - Complex combinations: `![[Note#Section^block-id|Alias]]`
//! - External URLs: `![[https://example.com/video.mp4]]`
//!
//! ### Security & Validation
//! - URL scheme validation with security checks
//! - Malicious content detection
//! - File extension validation
//! - Content type verification
//! - Error recovery mechanisms

mod helpers;

#[cfg(test)]
mod tests;

use anyhow::Result;
use blake3::Hasher;
use crucible_core::content_category::ContentCategory;
use crucible_core::parser::ParsedNote;
use crucible_core::storage::{Relation as CoreRelation, RelationStorage};
use crucible_merkle::{HybridMerkleTree, MerkleStore};
use serde_json::{Map, Value};

use super::store::EAVGraphStore;
use super::types::{
    AttributeValue, BlockNode, Entity, EntityRecord, EntityTag, EntityTagRecord, EntityType,
    Property, PropertyRecord, RecordId, TagRecord,
};
use helpers::{classify_content, extract_timestamps};

/// High-level helper for writing parsed documents into the EAV+Graph schema.
///
/// # Usage Example
///
/// ```ignore
/// # use crucible_surrealdb::eav_graph::{NoteIngestor, EAVGraphStore};
/// # use crucible_surrealdb::SurrealClient;
/// # use crucible_core::parser::ParsedNote;
/// # async fn example() -> anyhow::Result<()> {
/// # let client = SurrealClient::new_memory().await?;
/// # let store = EAVGraphStore::new(client);
/// let ingestor = NoteIngestor::new(&store);
/// # let parsed_doc = ParsedNote::default();
/// let entity_id = ingestor.ingest(&parsed_doc, "note.md").await?;
/// # Ok(())
/// # }
/// ```
///
/// The ingestor automatically:
/// 1. Creates or updates the note entity
/// 2. Processes and stores all note blocks
/// 3. Extracts and resolves wikilinks and embeds
/// 4. Creates relations with comprehensive metadata
/// 5. Handles tags and hierarchical structures
/// 6. Validates and classifies embed content
pub struct NoteIngestor<'a> {
    store: &'a EAVGraphStore,
    merkle_store: Option<Box<dyn MerkleStore>>,
}

impl<'a> NoteIngestor<'a> {
    pub fn new(store: &'a EAVGraphStore) -> Self {
        Self {
            store,
            merkle_store: None,
        }
    }

    /// Create a new ingestor with Merkle tree storage enabled
    ///
    /// This enables automatic persistence of Merkle trees during ingestion
    /// for efficient incremental change detection.
    ///
    /// # Example
    ///
    #[allow(dead_code)]
    pub fn with_merkle_store(
        store: &'a EAVGraphStore,
        merkle_store: Box<dyn crucible_merkle::MerkleStore>,
    ) -> Self {
        Self {
            store,
            merkle_store: Some(merkle_store),
        }
    }

    pub async fn ingest(
        &self,
        doc: &ParsedNote,
        relative_path: &str,
    ) -> Result<RecordId<EntityRecord>> {
        let entity_id = self.note_entity_id(relative_path);

        // Extract timestamps from frontmatter with FS fallback
        let (created_at, updated_at) = extract_timestamps(doc);

        let mut entity = Entity::new(entity_id.clone(), EntityType::Note)
            .with_content_hash(doc.content_hash.clone());
        entity.created_at = created_at;
        entity.updated_at = updated_at;
        entity.data = Some(self.entity_payload(doc, relative_path));

        self.store.upsert_entity(&entity).await?;

        for property in self.core_properties(&entity_id, doc, relative_path) {
            self.store.upsert_property(&property).await?;
        }

        let blocks = self.build_blocks(&entity_id, doc);
        self.store.replace_blocks(&entity_id, &blocks).await?;

        // Extract and store relations from wikilinks and embeds with resolution
        let relations = self
            .extract_relations_with_resolution(&entity_id, doc)
            .await?;
        for relation in relations {
            self.store.store_relation(relation).await?;
        }

        // Extract and store inline link relations
        let inline_link_relations = self.extract_inline_link_relations(&entity_id, doc);
        for relation in inline_link_relations {
            self.store.store_relation(relation).await?;
        }

        // Extract and store footnote relations
        let footnote_relations = self.extract_footnote_relations(&entity_id, doc);
        for relation in footnote_relations {
            self.store.store_relation(relation).await?;
        }

        // Compute and store section hashes (with optional Merkle tree persistence)
        let section_properties = self
            .compute_section_properties(&entity_id, doc, relative_path)
            .await?;
        for property in section_properties {
            self.store.upsert_property(&property).await?;
        }

        // Store tags and create tag associations
        self.store_tags(doc, &entity_id).await?;

        Ok(entity_id)
    }

    /// Ingest an enriched note (with embeddings, Merkle tree, and metadata)
    ///
    /// This is the Phase 5 (Storage) integration point for the enrichment pipeline.
    /// It stores all enrichment data atomically:
    /// - Parsed note content (via existing `ingest()`)
    /// - Vector embeddings for changed blocks
    /// - Merkle tree (for future change detection)
    /// - Enrichment metadata
    /// - Inferred relations
    ///
    /// # Arguments
    ///
    /// * `enriched` - The enriched note from the enrichment pipeline
    /// * `relative_path` - Path relative to the vault root
    ///
    /// # Returns
    ///
    /// The entity ID of the stored note
    ///
    /// # Example
    ///
    pub async fn ingest_enriched(
        &self,
        enriched: &crucible_enrichment::EnrichedNoteWithTree<HybridMerkleTree>,
        relative_path: &str,
    ) -> Result<RecordId<EntityRecord>> {
        use crate::kiln_integration::{store_embeddings_batch, EmbeddingData};

        // Step 1: Ingest the parsed note using existing logic
        let entity_id = self.ingest(&enriched.core.parsed, relative_path).await?;

        // Step 2: Store Merkle tree (if merkle_store is configured)
        if let Some(merkle_store) = &self.merkle_store {
            merkle_store
                .store(relative_path, &enriched.merkle_tree)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to store Merkle tree: {}", e))?;
        }

        // Step 3: Store embeddings in batch (reduces transaction conflicts)
        let note_id = &entity_id.id;
        if !enriched.core.embeddings.is_empty() {
            let embedding_data: Vec<EmbeddingData> = enriched
                .core
                .embeddings
                .iter()
                .map(|e| EmbeddingData {
                    vector: e.vector.clone(),
                    model: e.model.clone(),
                    block_id: e.block_id.clone(),
                    dimensions: e.dimensions,
                })
                .collect();

            store_embeddings_batch(&self.store.client, note_id, &embedding_data).await?;
        }

        // Step 4: Store enrichment metadata as properties
        use super::types::{Property, PropertyNamespace};
        use crucible_core::storage::AttributeValue;

        // Metadata namespace for enrichment-computed properties
        let _metadata_namespace = PropertyNamespace("enrichment".to_string());

        // Store reading time (always present)
        self.store
            .upsert_property(&Property::new(
                self.property_id(&entity_id, "enrichment", "reading_time"),
                entity_id.clone(),
                "enrichment",
                "reading_time",
                AttributeValue::Number(enriched.core.metadata.reading_time_minutes as f64),
            ))
            .await?;

        // Store complexity score (always present as f32)
        self.store
            .upsert_property(&Property::new(
                self.property_id(&entity_id, "enrichment", "complexity_score"),
                entity_id.clone(),
                "enrichment",
                "complexity_score",
                AttributeValue::Number(enriched.core.metadata.complexity_score as f64),
            ))
            .await?;

        // Store language if detected
        if let Some(language) = &enriched.core.metadata.language {
            self.store
                .upsert_property(&Property::new(
                    self.property_id(&entity_id, "enrichment", "language"),
                    entity_id.clone(),
                    "enrichment",
                    "language",
                    AttributeValue::Text(language.clone()),
                ))
                .await?;
        }

        // Step 5: Store inferred relations
        // For now, we skip storing inferred relations as the relation system
        // is primarily designed for explicit wikilinks and tags.
        // Inferred relations from semantic similarity can be added in the future
        // when there's a clear use case for them in queries.

        Ok(entity_id)
    }

    /// Resolve a wikilink target to entity IDs by searching the vault
    ///
    /// Searches for files matching the wikilink target. Returns:
    /// - Ok((Some(entity_id), vec![])) - Exactly one match found
    /// - Ok((None, candidates)) - Multiple ambiguous matches (stores candidates)
    /// - Ok((None, vec![])) - No matches found
    ///
    /// For ambiguous links, candidates are returned separately for metadata storage.
    async fn resolve_wikilink_target(
        &self,
        target: &str,
        _heading_ref: Option<&str>,
        _block_ref: Option<&str>,
    ) -> Result<(Option<RecordId<EntityRecord>>, Vec<String>)> {
        // Query database for all note entities and filter in Rust
        // SurrealDB returns the id in the record's id field, not in the data
        let query = "SELECT * FROM entities WHERE type = 'note' LIMIT 100";

        let result = self
            .store
            .client
            .query(query, &[])
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query entities: {}", e))?;

        // Extract candidate paths from results - filter by target match
        let target_lower = target.to_lowercase();
        let mut candidates: Vec<(String, String)> = Vec::new();

        for record in &result.records {
            // Extract the relative_path from data field
            if let Some(data_obj) = record.data.get("data").and_then(|v| v.as_object()) {
                if let Some(path) = data_obj.get("relative_path").and_then(|v| v.as_str()) {
                    // Check if path contains or ends with target (case-insensitive)
                    let path_lower = path.to_lowercase();
                    if path_lower.contains(&target_lower) || path_lower.ends_with(&target_lower) {
                        // The ID is in the record's id field from SurrealDB
                        // For our query, we can derive it from the path since we know the pattern
                        // The entity ID format is "entities:note:path"
                        let entity_id_str =
                            format!("note:{}", path.replace('\\', "/").replace(':', "_"));

                        candidates.push((path.to_string(), entity_id_str));
                    }
                }
            }
        }

        match candidates.len() {
            0 => {
                // No matches - emit warning
                tracing::warn!("Unresolved wikilink '{}' - no matching files found", target);
                Ok((None, vec![]))
            }
            1 => {
                // Exact match - return entity ID
                let (path, id_str) = &candidates[0];
                tracing::debug!("Resolved wikilink '{}' to entity '{}'", target, path);
                let entity_id = RecordId::new("entities", id_str.clone());
                Ok((Some(entity_id), vec![]))
            }
            _ => {
                // Ambiguous - return candidates for metadata
                let candidate_paths: Vec<String> =
                    candidates.iter().map(|(path, _)| path.clone()).collect();

                tracing::warn!(
                    "Ambiguous wikilink '{}' - found {} candidates: {}",
                    target,
                    candidates.len(),
                    candidate_paths.join(", ")
                );
                Ok((None, candidate_paths))
            }
        }
    }

    /// Extract relations from wikilinks with target resolution and advanced embed variant support.
    ///
    /// This function processes all wikilinks and embeds in a note, creating relations with
    /// comprehensive metadata including embed type classification, validation, and variant support.
    ///
    /// # Processing Pipeline
    ///
    /// 1. **Validation**: Each embed target is validated for security and format compliance
    /// 2. **Resolution**: Targets are resolved to actual entities when possible
    /// 3. **Classification**: Embed types are classified (image, video, audio, etc.)
    /// 4. **Metadata Enhancement**: Rich metadata is added for each embed variant
    /// 5. **Relation Creation**: Relations are created with full embed context
    ///
    /// # Embed Variants Supported
    ///
    /// - Simple wikilinks: `[[Note]]`
    /// - Simple embeds: `![[Image.png]]`
    /// - Heading references: `[[Note#Section]]` or `![[Note#Section]]`
    /// - Block references: `[[Note^block-id]]` or `![[Note^block-id]]`
    /// - Complex combinations: `![[Note#Section^block-id|Alias]]`
    /// - External URLs: `![[https://example.com/content]]`
    ///
    /// # Error Handling
    ///
    /// Invalid embeds are not discarded but instead marked with error metadata
    /// to provide feedback and enable recovery suggestions.
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The source entity ID for the relations
    /// * `doc` - The parsed note containing wikilinks and embeds
    ///
    /// # Returns
    ///
    /// A vector of relations with comprehensive embed metadata
    async fn extract_relations_with_resolution(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedNote,
    ) -> Result<Vec<CoreRelation>> {
        let mut relations = Vec::with_capacity(doc.wikilinks.len());
        let from_entity_id = format!("{}:{}", entity_id.table, entity_id.id);

        for wikilink in &doc.wikilinks {
            // Validate embed target before processing with comprehensive error handling
            if let Err(validation_error) = self.validate_embed_target(&wikilink.target) {
                tracing::warn!(
                    "Invalid embed target '{}' at offset {}: {}",
                    wikilink.target,
                    wikilink.offset,
                    validation_error
                );

                // Create error metadata and continue processing with error information
                let error_metadata =
                    self.handle_embed_processing_error(&validation_error, wikilink);

                // Still create relation but mark as invalid
                let relation_type = if wikilink.is_embed {
                    "embed"
                } else {
                    "wikilink"
                };
                let mut relation = CoreRelation::new(
                    from_entity_id.clone(),
                    None, // No valid target
                    relation_type,
                );

                // Add error metadata to relation
                relation.metadata = serde_json::Value::Object(error_metadata);

                // Mark as invalid embed
                if let Some(metadata_obj) = relation.metadata.as_object_mut() {
                    metadata_obj.insert("validation_failed".to_string(), serde_json::json!(true));
                }

                relations.push(relation);
                continue; // Skip normal processing for this invalid embed
            }

            // Resolve the target
            let (resolved_target, candidates) = self
                .resolve_wikilink_target(
                    &wikilink.target,
                    wikilink.heading_ref.as_deref(),
                    wikilink.block_ref.as_deref(),
                )
                .await?;

            let relation_type = if wikilink.is_embed {
                "embed"
            } else {
                "wikilink"
            };

            let mut relation = CoreRelation::new(
                from_entity_id.clone(),
                resolved_target.map(|id| format!("{}:{}", id.table, id.id)),
                relation_type,
            );

            // Enhanced metadata processing for advanced wikilink variants
            let mut metadata = self.process_wikilink_metadata(wikilink).await?;

            // Universal content classification for all link types
            let content_category = classify_content(&wikilink.target);
            metadata.insert(
                "content_category".to_string(),
                serde_json::json!(content_category.as_str()),
            );

            // For embeds, preserve the embed flag and add external flag if applicable
            if wikilink.is_embed {
                metadata.insert("is_embed".to_string(), serde_json::json!(true));

                // Classify and set embed type
                let embed_type = self.classify_embed_type(&wikilink.target);
                metadata.insert("embed_type".to_string(), serde_json::json!(embed_type));

                // Add external flag for external URLs
                let is_external = wikilink.target.starts_with("http://")
                    || wikilink.target.starts_with("https://");
                if is_external {
                    metadata.insert("is_external".to_string(), serde_json::json!(true));
                }

                // For external embed types, set content_category to "external" as fallback for unknown extensions
                if embed_type == "external" {
                    metadata.insert(
                        "content_category".to_string(),
                        serde_json::json!("external"),
                    );
                } else {
                    // Process content-specific embed logic for rich metadata
                    self.process_content_specific_embed(&mut metadata, wikilink);
                }
            }

            // Store candidates if ambiguous
            if !candidates.is_empty() {
                metadata.insert("candidates".to_string(), serde_json::json!(candidates));
                metadata.insert("ambiguous".to_string(), serde_json::json!(true));
            }

            relation.metadata = serde_json::Value::Object(metadata);
            relations.push(relation);
        }

        Ok(relations)
    }

    /// Process enhanced metadata for wikilinks with advanced variant support
    ///
    /// Handles complex wikilink combinations including headings, block references,
    /// aliases, and their various interactions.
    async fn process_wikilink_metadata(
        &self,
        wikilink: &crucible_core::parser::Wikilink,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut metadata = serde_json::Map::new();

        // Basic metadata
        metadata.insert("offset".to_string(), serde_json::json!(wikilink.offset));
        metadata.insert(
            "target".to_string(),
            serde_json::json!(wikilink.target.clone()),
        );

        // Alias processing - support for complex alias scenarios
        if let Some(alias) = &wikilink.alias {
            metadata.insert("alias".to_string(), serde_json::json!(alias));
            metadata.insert("has_alias".to_string(), serde_json::json!(true));

            // Detect if alias is a transformation of target (e.g., different case)
            if alias.to_lowercase() == wikilink.target.to_lowercase() {
                metadata.insert("alias_is_case_variant".to_string(), serde_json::json!(true));
            }
        } else {
            metadata.insert("has_alias".to_string(), serde_json::json!(false));
        }

        // Heading reference processing with validation
        if let Some(heading_ref) = &wikilink.heading_ref {
            metadata.insert("heading_ref".to_string(), serde_json::json!(heading_ref));
            metadata.insert("has_heading_ref".to_string(), serde_json::json!(true));

            // Validate heading reference format
            if self.is_valid_heading_reference(heading_ref) {
                metadata.insert("heading_ref_valid".to_string(), serde_json::json!(true));

                // Extract heading level if available
                if let Some(level) = self.extract_heading_level(heading_ref) {
                    metadata.insert("heading_level".to_string(), serde_json::json!(level));
                }
            } else {
                metadata.insert("heading_ref_valid".to_string(), serde_json::json!(false));
                metadata.insert(
                    "heading_ref_error".to_string(),
                    serde_json::json!("Invalid heading reference format"),
                );
            }
        } else {
            metadata.insert("has_heading_ref".to_string(), serde_json::json!(false));
        }

        // Block reference processing with validation
        if let Some(block_ref) = &wikilink.block_ref {
            metadata.insert("block_ref".to_string(), serde_json::json!(block_ref));
            metadata.insert("has_block_ref".to_string(), serde_json::json!(true));

            // Validate block reference format
            if self.is_valid_block_reference(block_ref) {
                metadata.insert("block_ref_valid".to_string(), serde_json::json!(true));

                // Extract block reference type (e.g., carrot hash, etc.)
                metadata.insert(
                    "block_ref_type".to_string(),
                    serde_json::json!(self.classify_block_reference(block_ref)),
                );
            } else {
                metadata.insert("block_ref_valid".to_string(), serde_json::json!(false));
                metadata.insert(
                    "block_ref_error".to_string(),
                    serde_json::json!("Invalid block reference format"),
                );
            }
        } else {
            metadata.insert("has_block_ref".to_string(), serde_json::json!(false));
        }

        // Wikilink complexity analysis
        let complexity_score = self.calculate_wikilink_complexity(wikilink);
        metadata.insert(
            "complexity_score".to_string(),
            serde_json::json!(complexity_score),
        );

        // Variant classification
        let variant_type = self.classify_wikilink_variant(wikilink);
        metadata.insert("variant_type".to_string(), serde_json::json!(variant_type));

        Ok(metadata)
    }

    /// Add embed-specific variant metadata for advanced embed scenarios
    ///
    /// Processes embed variants like `![[Note#Section|Alias]]`, `![[Note^block-id|Alias]]`,
    /// and `![[Note#Section^block-id|Alias]]` combinations.
    #[allow(dead_code)]
    fn add_embed_variant_metadata(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        wikilink: &crucible_core::parser::Wikilink,
    ) {
        // Identify embed variant patterns
        let has_heading = wikilink.heading_ref.is_some();
        let has_block = wikilink.block_ref.is_some();
        let has_alias = wikilink.alias.is_some();

        // Classify embed variant type
        let embed_variant = match (has_heading, has_block, has_alias) {
            (true, true, true) => "heading_block_alias",
            (true, true, false) => "heading_block",
            (true, false, true) => "heading_alias",
            (false, true, true) => "block_alias",
            (true, false, false) => "heading_only",
            (false, true, false) => "block_only",
            (false, false, true) => "alias_only",
            (false, false, false) => "simple",
        };
        metadata.insert(
            "embed_variant".to_string(),
            serde_json::json!(embed_variant),
        );

        // Add embed processing hints
        if has_heading && has_block {
            metadata.insert(
                "requires_dual_resolution".to_string(),
                serde_json::json!(true),
            );
            metadata.insert(
                "resolution_priority".to_string(),
                serde_json::json!("heading_first"),
            );
        }

        // Add display preference hints
        if has_alias {
            metadata.insert("display_preference".to_string(), serde_json::json!("alias"));
        } else if has_heading {
            metadata.insert(
                "display_preference".to_string(),
                serde_json::json!("heading"),
            );
        } else {
            metadata.insert(
                "display_preference".to_string(),
                serde_json::json!("target"),
            );
        }

        // Add content type hints for rendering
        metadata.insert(
            "render_hints".to_string(),
            serde_json::json!(self.get_render_hints(wikilink)),
        );
    }

    /// Validate heading reference format
    ///
    /// Checks if heading reference follows valid markdown heading patterns.
    fn is_valid_heading_reference(&self, heading_ref: &str) -> bool {
        if heading_ref.trim().is_empty() {
            return false;
        }

        // Basic validation - should not contain invalid characters
        !heading_ref.contains('\n')
            && !heading_ref.contains('\r')
            && !heading_ref.contains('\t')
            && !heading_ref.chars().any(|c| c.is_control())
    }

    /// Extract heading level from heading reference if detectable
    ///
    /// Attempts to extract heading level patterns like "## Heading" -> level 2.
    fn extract_heading_level(&self, heading_ref: &str) -> Option<i32> {
        // Look for patterns like "# Heading", "## Heading", etc.
        let trimmed = heading_ref.trim();
        if trimmed.starts_with('#') {
            let mut level = 0;
            for char in trimmed.chars() {
                if char == '#' {
                    level += 1;
                } else {
                    break;
                }
            }
            Some(level)
        } else {
            None
        }
    }

    /// Validate block reference format
    ///
    /// Checks if block reference follows valid block reference patterns.
    fn is_valid_block_reference(&self, block_ref: &str) -> bool {
        if block_ref.trim().is_empty() {
            return false;
        }

        // Basic validation - allow carrot hash (^block-id) and other common patterns
        let trimmed = block_ref.trim();

        // Should not contain invalid characters
        if trimmed.contains('\n') || trimmed.contains('\r') || trimmed.contains('\t') {
            return false;
        }

        // Allow common block reference patterns
        trimmed.starts_with('^')
            || trimmed
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    /// Classify block reference type
    ///
    /// Identifies the type of block reference being used.
    fn classify_block_reference(&self, block_ref: &str) -> &'static str {
        let trimmed = block_ref.trim();

        if trimmed.starts_with('^') {
            "carrot_hash"
        } else if trimmed.chars().all(|c| c.is_alphanumeric()) {
            "alphanumeric"
        } else if trimmed.contains('-') || trimmed.contains('_') {
            "slug"
        } else {
            "unknown"
        }
    }

    /// Calculate wikilink complexity score
    ///
    /// Assigns a complexity score based on the combination of features used.
    fn calculate_wikilink_complexity(&self, wikilink: &crucible_core::parser::Wikilink) -> i32 {
        let mut score = 1; // Base score for simple wikilink

        if wikilink.is_embed {
            score += 2; // Embeds are more complex
        }

        if wikilink.alias.is_some() {
            score += 2;
        }

        if wikilink.heading_ref.is_some() {
            score += 3; // Heading references are complex
        }

        if wikilink.block_ref.is_some() {
            score += 3; // Block references are complex
        }

        if self.is_external_url(&wikilink.target) {
            score += 1; // External URLs add complexity
        }

        score
    }

    /// Classify wikilink variant type
    ///
    /// Categorizes the wikilink based on its structure and purpose.
    fn classify_wikilink_variant(
        &self,
        wikilink: &crucible_core::parser::Wikilink,
    ) -> &'static str {
        if wikilink.is_embed {
            if self.is_external_url(&wikilink.target) {
                "external_embed"
            } else {
                "internal_embed"
            }
        } else if self.is_external_url(&wikilink.target) {
            "external_link"
        } else {
            "internal_link"
        }
    }

    /// Get render hints for wikilink based on its characteristics
    ///
    /// Provides hints for rendering engines about how to display this wikilink.
    fn get_render_hints(&self, wikilink: &crucible_core::parser::Wikilink) -> serde_json::Value {
        let mut hints = serde_json::Map::new();

        // Embed-specific hints
        if wikilink.is_embed {
            hints.insert("is_embed".to_string(), serde_json::json!(true));

            // Add size hints based on content type
            let embed_type = self.classify_embed_type(&wikilink.target);
            match embed_type {
                "image" => {
                    hints.insert("suggested_width".to_string(), serde_json::json!("medium"));
                    hints.insert("suggested_height".to_string(), serde_json::json!("auto"));
                }
                "video" => {
                    hints.insert("suggested_width".to_string(), serde_json::json!("large"));
                    hints.insert("suggested_height".to_string(), serde_json::json!("medium"));
                    hints.insert("show_controls".to_string(), serde_json::json!(true));
                }
                "audio" => {
                    hints.insert("show_controls".to_string(), serde_json::json!(true));
                    hints.insert("suggested_width".to_string(), serde_json::json!("medium"));
                }
                "pdf" => {
                    hints.insert("suggested_width".to_string(), serde_json::json!("large"));
                    hints.insert("suggested_height".to_string(), serde_json::json!("large"));
                }
                _ => {
                    hints.insert("suggested_width".to_string(), serde_json::json!("auto"));
                    hints.insert("suggested_height".to_string(), serde_json::json!("auto"));
                }
            }
        }

        // Reference-specific hints
        if wikilink.heading_ref.is_some() {
            hints.insert("scroll_to_heading".to_string(), serde_json::json!(true));
        }

        if wikilink.block_ref.is_some() {
            hints.insert("highlight_block".to_string(), serde_json::json!(true));
        }

        serde_json::Value::Object(hints)
    }

    /// Store tags and create tag associations for a note
    ///
    /// Extracts tags from ParsedNote, ensures hierarchical structure exists,
    /// and creates entity_tag associations.
    async fn store_tags(&self, doc: &ParsedNote, entity_id: &RecordId<EntityRecord>) -> Result<()> {
        // Collect unique tags
        let unique_tags: std::collections::HashSet<String> = doc
            .all_tags()
            .into_iter()
            .filter_map(|tag| {
                let trimmed = tag.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();

        if unique_tags.is_empty() {
            return Ok(());
        }

        // Delete existing tag associations for this entity
        self.store.delete_entity_tags(entity_id).await?;

        // Create tags and associations
        for tag in unique_tags {
            if let Some(tag_id) = self.ensure_tag_hierarchy(&tag).await? {
                let mapping = EntityTag {
                    id: Some(self.entity_tag_record_id(entity_id, &tag_id)),
                    entity_id: entity_id.clone(),
                    tag_id,
                    source: "parser".into(),
                    confidence: 1.0,
                };
                self.store.upsert_entity_tag(&mapping).await?;
            }
        }

        Ok(())
    }

    /// Ensure tag hierarchy exists in database
    ///
    /// Creates parent tags if they don't exist (e.g., #project/ai/nlp creates project, project/ai, project/ai/nlp)
    async fn ensure_tag_hierarchy(&self, tag_path: &str) -> Result<Option<RecordId<TagRecord>>> {
        let segments: Vec<String> = tag_path
            .trim()
            .trim_start_matches('#')
            .split('/')
            .filter_map(|seg| {
                let trimmed = seg.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();

        if segments.is_empty() {
            return Ok(None);
        }

        let mut current_path = String::new();
        let mut parent: Option<RecordId<TagRecord>> = None;

        for (depth, segment) in segments.iter().enumerate() {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(segment);

            let tag_id = RecordId::new("tags", current_path.clone());
            let tag = super::types::Tag {
                id: Some(tag_id.clone()),
                name: current_path.clone(),
                parent_id: parent.clone(),
                path: current_path.clone(),
                depth: depth as i32,
                description: None,
                color: None,
                icon: None,
            };

            self.store.upsert_tag(&tag).await?;
            parent = Some(tag_id);
        }

        Ok(parent)
    }

    /// Generate entity_tag record ID from entity and tag IDs
    fn entity_tag_record_id(
        &self,
        entity_id: &RecordId<EntityRecord>,
        tag_id: &RecordId<TagRecord>,
    ) -> RecordId<EntityTagRecord> {
        let entity_part = entity_id.id.replace(':', "_");
        let tag_part = tag_id.id.replace(':', "_");
        RecordId::new("entity_tags", format!("{}:{}", entity_part, tag_part))
    }

    /// Generate a property record ID from entity ID, namespace, and key
    fn property_id(
        &self,
        entity_id: &RecordId<EntityRecord>,
        namespace: &str,
        key: &str,
    ) -> RecordId<PropertyRecord> {
        RecordId::new(
            "properties",
            format!("{}:{}:{}", entity_id.id, namespace, key),
        )
    }

    /// Compute section properties from the note's Merkle tree
    async fn compute_section_properties(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedNote,
        relative_path: &str,
    ) -> Result<Vec<Property>> {
        // Build the hybrid Merkle tree to extract sections
        let merkle_tree = HybridMerkleTree::from_document(doc);

        // Persist the tree if storage is enabled
        if let Some(ref merkle_store) = self.merkle_store {
            // Try to get the old tree for incremental updates
            if let Ok(Some(old_metadata)) = merkle_store.get_metadata(relative_path).await {
                // If root hash changed, do incremental update
                if old_metadata.root_hash != merkle_tree.root_hash.to_hex() {
                    // Compare trees to find changed sections
                    if let Ok(old_tree) = merkle_store.retrieve(relative_path).await {
                        let diff = merkle_tree.diff(&old_tree);
                        let changed_indices: Vec<usize> = diff
                            .changed_sections
                            .iter()
                            .map(|change| change.section_index)
                            .collect();

                        merkle_store
                            .update_incremental(relative_path, &merkle_tree, &changed_indices)
                            .await?;
                    } else {
                        // Old tree retrieval failed, store new tree
                        merkle_store.store(relative_path, &merkle_tree).await?;
                    }
                }
                // If hash unchanged, no update needed
            } else {
                // No existing tree, store the new one
                merkle_store.store(relative_path, &merkle_tree).await?;
            }
        }

        // Pre-calculate capacity: 2 base properties + 2 per section
        // Base: tree_root_hash, total_sections
        // Per section: section_{n}_hash, section_{n}_metadata
        let capacity = 2 + (merkle_tree.sections.len() * 2);
        let mut props = Vec::with_capacity(capacity);

        // Store the root hash as a property
        props.push(Property::new(
            self.property_id(entity_id, "section", "tree_root_hash"),
            entity_id.clone(),
            "section",
            "tree_root_hash",
            AttributeValue::Text(merkle_tree.root_hash.to_hex()),
        ));

        // Store total section count
        props.push(Property::new(
            self.property_id(entity_id, "section", "total_sections"),
            entity_id.clone(),
            "section",
            "total_sections",
            AttributeValue::Number(merkle_tree.sections.len() as f64),
        ));

        // Store metadata for each section
        for (index, section) in merkle_tree.sections.iter().enumerate() {
            // Store section hash
            let hash_key = format!("section_{}_hash", index);
            props.push(Property::new(
                self.property_id(entity_id, "section", &hash_key),
                entity_id.clone(),
                "section",
                &hash_key,
                AttributeValue::Text(section.binary_tree.root_hash.to_hex()),
            ));

            // Store section metadata as JSON
            let metadata_key = format!("section_{}_metadata", index);
            let mut metadata = serde_json::Map::new();
            metadata.insert(
                "block_count".to_string(),
                serde_json::json!(section.block_count),
            );
            metadata.insert("depth".to_string(), serde_json::json!(section.depth));

            if let Some(heading) = &section.heading {
                metadata.insert("heading_text".to_string(), serde_json::json!(heading.text));
                metadata.insert(
                    "heading_level".to_string(),
                    serde_json::json!(heading.level),
                );
            }

            props.push(Property::new(
                self.property_id(entity_id, "section", &metadata_key),
                entity_id.clone(),
                "section",
                &metadata_key,
                AttributeValue::Json(Value::Object(metadata)),
            ));
        }

        Ok(props)
    }

    /// Compute core note properties
    fn core_properties(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedNote,
        relative_path: &str,
    ) -> Vec<Property> {
        // Pre-calculate capacity: 4 base properties + 1 if frontmatter exists
        // Base: path, relative_path, title, tags
        // Optional: frontmatter
        let capacity = if doc.frontmatter.is_some() { 5 } else { 4 };
        let mut props = Vec::with_capacity(capacity);

        props.push(Property::new(
            self.property_id(entity_id, "core", "path"),
            entity_id.clone(),
            "core",
            "path",
            AttributeValue::Text(doc.path.to_string_lossy().to_string()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "relative_path"),
            entity_id.clone(),
            "core",
            "relative_path",
            AttributeValue::Text(relative_path.to_string()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "title"),
            entity_id.clone(),
            "core",
            "title",
            AttributeValue::Text(doc.title()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "tags"),
            entity_id.clone(),
            "core",
            "tags",
            AttributeValue::Json(Value::Array(
                doc.all_tags()
                    .into_iter()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            )),
        ));

        if let Some(frontmatter) = &doc.frontmatter {
            let fm_value = Value::Object(
                frontmatter
                    .properties()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<serde_json::Map<_, _>>(),
            );
            props.push(Property::new(
                self.property_id(entity_id, "core", "frontmatter"),
                entity_id.clone(),
                "core",
                "frontmatter",
                AttributeValue::Json(fm_value),
            ));
        }

        props
    }

    /// Generate entity payload data from parsed note
    fn entity_payload(&self, doc: &ParsedNote, relative_path: &str) -> Value {
        let tags = doc
            .all_tags()
            .into_iter()
            .map(Value::String)
            .collect::<Vec<_>>();

        let frontmatter_value = doc.frontmatter.as_ref().map(|fm| {
            let map = fm
                .properties()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Map<_, _>>();
            Value::Object(map)
        });

        let mut payload = Map::new();
        payload.insert(
            "path".to_string(),
            Value::String(doc.path.to_string_lossy().into_owned()),
        );
        payload.insert(
            "relative_path".to_string(),
            Value::String(relative_path.to_string()),
        );
        payload.insert("title".to_string(), Value::String(doc.title()));
        payload.insert("tags".to_string(), Value::Array(tags));
        if let Some(frontmatter) = frontmatter_value {
            payload.insert("frontmatter".to_string(), frontmatter);
        }
        payload.insert(
            "parsed_at".to_string(),
            Value::String(doc.parsed_at.to_rfc3339()),
        );
        payload.insert(
            "file_size".to_string(),
            Value::Number(serde_json::Number::from(doc.file_size)),
        );
        payload.insert(
            "content_hash".to_string(),
            Value::String(doc.content_hash.clone()),
        );
        payload.insert(
            "wikilink_count".to_string(),
            Value::Number(serde_json::Number::from(doc.wikilinks.len() as u64)),
        );
        payload.insert(
            "created_via".to_string(),
            Value::String("parser".to_string()),
        );

        Value::Object(payload)
    }

    /// Generate entity ID from relative path
    fn note_entity_id(&self, relative_path: &str) -> RecordId<EntityRecord> {
        let normalized = relative_path
            .trim_start_matches(std::path::MAIN_SEPARATOR)
            .replace('\\', "/")
            .replace(':', "_");
        RecordId::new("entities", format!("note:{}", normalized))
    }

    /// Extract inline link relations from the note
    fn extract_inline_link_relations(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedNote,
    ) -> Vec<CoreRelation> {
        let capacity = doc.inline_links.len();
        let mut relations = Vec::with_capacity(capacity);
        let from_entity_id = format!("{}:{}", entity_id.table, entity_id.id);

        for inline_link in &doc.inline_links {
            // Create relation for inline link
            // For external URLs, to_entity_id is None (no entity to link to)
            // For relative links, we could potentially resolve them to entities
            let to_entity_id = if inline_link.is_relative() {
                // Could parse relative path to entity ID
                // For now, just store the URL in metadata
                None
            } else {
                None // External URLs don't have entity IDs
            };

            let mut relation = CoreRelation::new(from_entity_id.clone(), to_entity_id, "link");

            // Add metadata about the inline link
            let mut metadata = serde_json::Map::new();
            metadata.insert("url".to_string(), serde_json::json!(inline_link.url));
            metadata.insert("text".to_string(), serde_json::json!(inline_link.text));
            if let Some(title) = &inline_link.title {
                metadata.insert("title".to_string(), serde_json::json!(title));
            }
            metadata.insert("offset".to_string(), serde_json::json!(inline_link.offset));
            metadata.insert(
                "is_external".to_string(),
                serde_json::json!(inline_link.is_external()),
            );

            // Universal content classification for inline links
            let content_category = classify_content(&inline_link.url);
            metadata.insert(
                "content_category".to_string(),
                serde_json::json!(content_category.as_str()),
            );

            relation.metadata = serde_json::Value::Object(metadata);

            relations.push(relation);
        }

        relations
    }

    /// Determine embed type based on file extension and URL patterns.
    ///
    /// This function analyzes embed targets to classify them into appropriate content types.
    /// It supports both local files and external URLs with intelligent content detection.
    ///
    /// # Classification Algorithm
    ///
    /// 1. **URL Detection**: First determines if the target is an external URL
    /// 2. **Platform Detection**: For URLs, detects known platforms (YouTube, Vimeo, etc.)
    /// 3. **Extension Analysis**: For files, analyzes file extensions for content type
    /// 4. **Fallback**: Uses "note" for wikilinks without extensions, "external" for unknown URLs
    ///
    /// # Supported Content Types
    ///
    /// ## Image Formats
    /// - **Web**: PNG, JPG, JPEG, GIF, SVG, WebP, AVIF
    /// - **Legacy**: BMP, ICO, TIFF, PSD
    ///
    /// ## Video Formats
    /// - **Modern**: MP4, WebM, M4V, OGV
    /// - **Legacy**: AVI, MOV, MKV, WMV, FLV, 3GP
    /// - **Platforms**: YouTube, Vimeo
    ///
    /// ## Audio Formats
    /// - **Modern**: MP3, WAV, FLAC, OGG, OPUS
    /// - **Legacy**: M4A, AAC, WMA, AIFF
    /// - **Platforms**: SoundCloud, Spotify
    ///
    /// ## Note Formats
    /// - **PDF**: Portable Note Format
    /// - **Office**: DOC, DOCX, PPT, PPTX, XLSX, ODT, ODS, ODP
    ///
    /// ## Code & Text
    /// - **Documents**: MD, MARKDOWN, TXT, RST, ADOC
    /// - **Code**: JS, TS, PY, JAVA, CPP, C, GO, RS, PHP, RB, HTML, CSS
    ///
    /// # Arguments
    ///
    /// * `target` - The embed target (file path or URL)
    ///
    /// # Returns
    ///
    /// A string representing the classified embed type
    fn classify_embed_type(&self, target: &str) -> &'static str {
        // Check for URLs first with enhanced scheme validation
        if self.is_external_url(target) {
            // For external URLs, try to infer from URL extension or content patterns
            self.classify_external_url_embed_type(target)
        } else {
            // For local files, check extension
            self.classify_local_file_embed_type(target)
        }
    }

    /// Check if a target is an external URL with scheme validation
    ///
    /// Validates URL schemes and identifies external content sources.
    /// Supports: http, https, ftp, ftps, git, ssh, and other common protocols.
    fn is_external_url(&self, target: &str) -> bool {
        let target_lower = target.to_lowercase();

        // List of supported URL schemes
        const VALID_SCHEMES: &[&str] = &[
            "http://", "https://", "ftp://", "ftps://", "git://", "ssh://", "mailto:", "tel:",
            "data:", "file://",
        ];

        VALID_SCHEMES
            .iter()
            .any(|scheme| target_lower.starts_with(scheme))
    }

    /// Classify external URL embed types with enhanced content detection
    ///
    /// Goes beyond simple extension-based detection to identify common external
    /// content platforms and services.
    ///
    /// Performance optimized to minimize string allocations and use efficient
    /// pattern matching for common platforms.
    fn classify_external_url_embed_type(&self, target: &str) -> &'static str {
        // Use case-insensitive matching efficiently without multiple allocations
        let target_lower = target.to_lowercase();

        // Special handling for ambiguous extensions based on filename context
        if target_lower.contains("audio.webm") {
            return "audio";
        }

        // Check for known external content platforms first (most common first)
        if target_lower.contains("youtube.com/watch")
            || target_lower.contains("youtu.be/")
            || target_lower.contains("youtube.com/embed/")
            || target_lower.contains("youtube.com/shorts/")
        {
            return "youtube";
        }

        if target_lower.contains("vimeo.com/") && !target_lower.contains("vimeo.com/channels/") {
            return "vimeo";
        }

        if target_lower.contains("twitch.tv/videos") || target_lower.contains("twitch.tv/") {
            return "twitch";
        }

        if target_lower.contains("soundcloud.com/") {
            return "soundcloud";
        }

        if target_lower.contains("spotify.com/")
            && (target_lower.contains("/track/")
                || target_lower.contains("/album/")
                || target_lower.contains("/playlist/"))
        {
            return "spotify";
        }

        if target_lower.contains("twitter.com/")
            || target_lower.contains("x.com/")
            || target_lower.contains("t.co/")
        {
            return "twitter";
        }

        if target_lower.contains("github.com/") {
            return "github";
        }

        // Check for known image hosting services
        if target_lower.contains("imgur.com/") || target_lower.contains("picsum.photos/") {
            return "imgur";
        }

        // Fall back to extension-based detection for general URLs
        if let Some(extension) = target_lower.split('.').next_back() {
            match extension {
                // Image formats
                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" | "avif"
                | "tiff" => "image",

                // Note formats
                "pdf" => "pdf",
                "doc" | "docx" | "odt" | "rtf" => "external", // Note types

                // Audio formats
                "mp3" | "wav" | "m4a" | "ogg" | "flac" | "aac" | "wma" | "opus" => "audio",

                // Video formats
                "mp4" | "avi" | "mov" | "mkv" | "webm" | "m4v" | "wmv" | "flv" | "ogv" | "3gp" => {
                    "video"
                }

                // Archive formats
                "zip" | "rar" | "7z" | "tar" | "gz" => "external",

                // Code formats
                "js" | "ts" | "py" | "java" | "cpp" | "c" | "go" | "rs" | "php" | "rb"
                | "swift" | "kt" => "note",

                // Data formats
                "json" | "xml" | "yaml" | "yml" | "csv" => "note",

                // Configuration formats
                "toml" | "ini" | "conf" => "note",

                _ => "external",
            }
        } else {
            // URL without extension - classify based on domain patterns or default to external
            "external"
        }
    }

    /// Classify local file embed types with enhanced pattern matching
    ///
    /// Handles local file paths with improved extension detection and
    /// special case handling for different file naming patterns.
    fn classify_local_file_embed_type(&self, target: &str) -> &'static str {
        let target_lower = target.to_lowercase();

        // Check if there's actually a dot (i.e., a real extension)
        if target_lower.contains('.') {
            if let Some(extension) = target_lower.split('.').next_back() {
                // Special handling for ambiguous extensions based on filename context
                if extension == "webm" && target_lower.contains("audio") {
                    return "audio";
                }
                // Handle common file extensions with case-insensitive matching
                match extension {
                    // Image formats
                    "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" | "avif"
                    | "tiff" | "psd" => "image",

                    // Note formats
                    "pdf" => "pdf",
                    "doc" | "docx" | "odt" | "rtf" | "pages" => "external",

                    // Audio formats
                    "mp3" | "wav" | "m4a" | "ogg" | "flac" | "aac" | "wma" | "opus" | "aiff" => {
                        "audio"
                    }

                    // Video formats
                    "mp4" | "avi" | "mov" | "mkv" | "webm" | "m4v" | "wmv" | "flv" | "ogv"
                    | "3gp" => "video",

                    // Text and note formats
                    "md" | "markdown" | "txt" | "rst" | "adoc" => "note",

                    // Data formats (often embedded as documentation)
                    "json" | "xml" | "yaml" | "yml" | "csv" => "note",

                    // Configuration formats
                    "toml" | "ini" | "conf" => "note",

                    // Spreadsheet formats
                    "xlsx" | "xls" | "ods" => "external",

                    // Presentation formats
                    "pptx" | "ppt" | "key" | "odp" => "external",

                    // Archive formats
                    "zip" | "rar" | "7z" | "tar" | "gz" => "external",

                    // Code formats (often embedded as documentation)
                    "js" | "ts" | "py" | "java" | "cpp" | "c" | "go" | "rs" | "php" | "rb"
                    | "swift" | "kt" | "html" | "css" => "note",

                    _ => "external",
                }
            } else {
                "external"
            }
        } else {
            // No extension at all - assume it's a note (wikilink without extension)
            "note"
        }
    }

    /// Validate and normalize embed target for processing.
    ///
    /// This function performs comprehensive security and format validation on embed targets
    /// to ensure they are safe and well-formed before processing.
    ///
    /// # Validation Rules
    ///
    /// ## Basic Validation
    /// - Target cannot be empty or contain only whitespace
    /// - Target length must be reasonable (< 2048 characters to prevent DoS)
    /// - No invalid control characters (except tab, newline, carriage return)
    ///
    /// ## External URL Validation
    /// - Scheme validation against supported protocols
    /// - Structural validation for URL format
    /// - Security checks against dangerous schemes (javascript:, data:text/html, etc.)
    /// - Detection of suspicious patterns and potential attacks
    /// - Protocol-specific validation (HTTP, FTP, mailto, etc.)
    ///
    /// ## Local File Validation
    /// - Path traversal attack prevention
    /// - File extension validation
    /// - Path format compliance
    ///
    /// # Security Considerations
    ///
    /// This function implements multiple layers of security validation:
    /// - **XSS Prevention**: Blocks dangerous URL schemes like javascript:
    /// - **Path Traversal Prevention**: Validates local file paths
    /// - **Injection Prevention**: Validates URL encoding and structure
    /// - **DoS Prevention**: Limits target length and validates formats
    ///
    /// # Arguments
    ///
    /// * `target` - The embed target to validate
    ///
    /// # Returns
    ///
    /// `Ok(())` if the target is valid, `Err(String)` with detailed error message otherwise
    fn validate_embed_target(&self, target: &str) -> Result<(), String> {
        // Basic validation
        if target.trim().is_empty() {
            return Err("Embed target cannot be empty".to_string());
        }

        // Check for excessively long targets (potential DoS or malformed content)
        if target.len() > 2048 {
            return Err("Embed target is excessively long (>2048 characters)".to_string());
        }

        // Check for control characters and other invalid content
        if target
            .chars()
            .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
        {
            return Err("Embed target contains invalid control characters".to_string());
        }

        // External URL validation
        if self.is_external_url(target) {
            self.validate_external_url_embed_target(target)
        } else {
            self.validate_local_file_embed_target(target)
        }
    }

    /// Validate external URL embed targets with comprehensive checks
    ///
    /// Performs security and structural validation on external URLs.
    fn validate_external_url_embed_target(&self, target: &str) -> Result<(), String> {
        let target_lower = target.to_lowercase();

        // Basic URL structure validation
        if let Some(scheme_end) = target_lower.find("://") {
            let scheme = &target_lower[..scheme_end];
            if !self.is_supported_url_scheme(scheme) {
                return Err(format!("Unsupported URL scheme: {}", scheme));
            }
        } else {
            return Err("Invalid URL format - missing scheme".to_string());
        }

        // Check for common URL issues
        if target.contains(' ') && !target.starts_with("data:") {
            return Err("URL contains unencoded spaces - may be malformed".to_string());
        }

        // Security checks for dangerous schemes
        if target_lower.starts_with("javascript:")
            || target_lower.starts_with("data:text/html")
            || target_lower.starts_with("vbscript:")
            || target_lower.starts_with("file://")
        {
            return Err("Unsupported or potentially dangerous URL scheme".to_string());
        }

        // Check for suspicious patterns
        if self.contains_suspicious_url_patterns(target) {
            return Err("URL contains potentially suspicious patterns".to_string());
        }

        // Validate specific URL types
        if target_lower.starts_with("http://") || target_lower.starts_with("https://") {
            self.validate_http_url(target)
        } else if target_lower.starts_with("ftp://") || target_lower.starts_with("ftps://") {
            self.validate_ftp_url(target)
        } else if target_lower.starts_with("mailto:") {
            self.validate_mailto_url(target)
        } else {
            // Other schemes get basic validation only
            Ok(())
        }
    }

    /// Validate local file embed targets
    ///
    /// Performs validation on local file paths and references.
    fn validate_local_file_embed_target(&self, target: &str) -> Result<(), String> {
        // Check for path traversal attempts
        if target.contains("../") || target.contains("..\\") {
            return Err("Path traversal detected in local file reference".to_string());
        }

        // Check for absolute paths that might be problematic
        if target.starts_with('/')
            || (target.chars().nth(1).is_some()
                && target
                    .chars()
                    .skip(1)
                    .collect::<String>()
                    .starts_with(":\\"))
        {
            return Err("Absolute paths not allowed for local file embeds".to_string());
        }

        // Check for invalid characters in filenames
        let invalid_chars = ['<', '>', ':', '"', '|', '?', '*', '\0'];
        if target.chars().any(|c| invalid_chars.contains(&c)) {
            return Err("Local file path contains invalid characters".to_string());
        }

        // Check for excessively deep paths
        let path_depth = target.matches('/').count() + target.matches('\\').count();
        if path_depth > 20 {
            return Err("File path is too deep (>20 levels)".to_string());
        }

        Ok(())
    }

    /// Check if URL scheme is supported
    fn is_supported_url_scheme(&self, scheme: &str) -> bool {
        matches!(
            scheme,
            "http" | "https" | "ftp" | "ftps" | "git" | "ssh" | "mailto" | "tel" | "data"
        )
    }

    /// Check for suspicious URL patterns
    fn contains_suspicious_url_patterns(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();

        // Check for common attack patterns
        url_lower.contains("<script") ||
        url_lower.contains("javascript:") ||
        url_lower.contains("vbscript:") ||
        url_lower.contains("data:text/html") ||
        url_lower.contains("data:text/javascript") ||
        url_lower.contains("data:application/javascript") ||
        url_lower.contains("onclick=") ||
        url_lower.contains("onerror=") ||
        url_lower.contains("onload=") ||
        // Check for double-encoded URLs (might indicate evasion attempts)
        url_lower.contains("%25") ||
        // Check for excessive encoding
        url_lower.matches("%[0-9a-f]{2}").count() > 10
    }

    /// Validate HTTP/HTTPS URLs with specific checks
    fn validate_http_url(&self, url: &str) -> Result<(), String> {
        let url_lower = url.to_lowercase();

        // Check for localhost/private network access (if this should be restricted)
        if url_lower.contains("localhost")
            || url_lower.contains("127.0.0.1")
            || url_lower.contains("0.0.0.0")
            || url_lower.contains("::1")
            || url_lower.starts_with("http://192.168.")
            || url_lower.starts_with("http://10.")
            || url_lower.starts_with("http://172.16.")
        {
            // Note: This might be too restrictive for some use cases
            // Consider making this configurable
            return Err("Access to localhost/private networks not allowed".to_string());
        }

        // Check for missing host
        if let Some(after_scheme) = url_lower.split("://").nth(1) {
            if after_scheme.is_empty() || after_scheme.starts_with('/') {
                return Err("URL missing host/domain".to_string());
            }
        }

        Ok(())
    }

    /// Validate FTP URLs with specific checks
    fn validate_ftp_url(&self, url: &str) -> Result<(), String> {
        // Basic FTP validation
        if let Some(after_scheme) = url.split("://").nth(1) {
            if after_scheme.is_empty() {
                return Err("FTP URL missing host".to_string());
            }

            // Check for anonymous FTP (might be restricted)
            if after_scheme.contains("anonymous@") {
                return Err("Anonymous FTP access not allowed".to_string());
            }
        }

        Ok(())
    }

    /// Validate mailto URLs with specific checks
    fn validate_mailto_url(&self, url: &str) -> Result<(), String> {
        if !url.starts_with("mailto:") {
            return Err("Invalid mailto URL format".to_string());
        }

        let email_part = &url[7..]; // Remove "mailto:"
        if email_part.is_empty() {
            return Err("Mailto URL missing email address".to_string());
        }

        // Basic email validation
        if !email_part.contains('@') || email_part.starts_with('@') || email_part.ends_with('@') {
            return Err("Invalid email address in mailto URL".to_string());
        }

        Ok(())
    }

    /// Enhanced error handling for embed processing with detailed error reporting
    ///
    /// Provides comprehensive error context and recovery suggestions.
    fn handle_embed_processing_error(
        &self,
        error: &str,
        wikilink: &crucible_core::parser::Wikilink,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut error_metadata = serde_json::Map::new();

        // Basic error information
        error_metadata.insert(
            "error_type".to_string(),
            serde_json::json!("embed_validation_error"),
        );
        error_metadata.insert("error_message".to_string(), serde_json::json!(error));
        error_metadata.insert(
            "error_timestamp".to_string(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );

        // Context information - sanitize target to remove problematic characters
        let sanitized_target = wikilink
            .target
            .replace('\x00', "[NULL]")
            .replace('\x01', "[SOH]")
            .replace('\x1f', "[US]");
        error_metadata.insert("target".to_string(), serde_json::json!(sanitized_target));
        error_metadata.insert("offset".to_string(), serde_json::json!(wikilink.offset));
        error_metadata.insert("is_embed".to_string(), serde_json::json!(wikilink.is_embed));

        // Error classification for better handling
        let error_category = self.classify_embed_error(error);
        error_metadata.insert(
            "error_category".to_string(),
            serde_json::json!(error_category),
        );

        // Recovery suggestions
        let suggestions = self.get_embed_error_recovery_suggestions(error, wikilink);
        error_metadata.insert(
            "recovery_suggestions".to_string(),
            serde_json::json!(suggestions),
        );

        // Severity assessment
        let severity = self.assess_embed_error_severity(error, wikilink);
        error_metadata.insert("error_severity".to_string(), serde_json::json!(severity));

        error_metadata
    }

    /// Classify embed errors into categories for better handling
    fn classify_embed_error(&self, error: &str) -> &'static str {
        let error_lower = error.to_lowercase();

        // Security-related errors should be checked first
        if error_lower.contains("dangerous") || error_lower.contains("suspicious") {
            "security_risk"
        } else if error_lower.contains("traversal") {
            "security_path_traversal"
        } else if error_lower.contains("localhost") || error_lower.contains("private") {
            "security_private_network"
        } else if error_lower.contains("empty") {
            "validation_empty_target"
        } else if error_lower.contains("scheme")
            && (error_lower.contains("unsupported") || error_lower.contains("dangerous"))
        {
            "security_risk"
        } else if error_lower.contains("scheme") {
            "validation_invalid_scheme"
        } else if error_lower.contains("malformed") || error_lower.contains("format") {
            "validation_malformed_url"
        } else if error_lower.contains("long") {
            "validation_too_long"
        } else if error_lower.contains("character") {
            "validation_invalid_characters"
        } else {
            "validation_unknown"
        }
    }

    /// Get recovery suggestions for embed errors
    fn get_embed_error_recovery_suggestions(
        &self,
        error: &str,
        _wikilink: &crucible_core::parser::Wikilink,
    ) -> Vec<String> {
        let error_lower = error.to_lowercase();
        let mut suggestions = Vec::new();

        if error_lower.contains("empty") {
            suggestions.push("Add a valid target to the embed".to_string());
            suggestions.push("Remove the empty embed syntax".to_string());
        } else if error_lower.contains("scheme") {
            suggestions.push("Use a supported URL scheme (http, https, ftp, etc.)".to_string());
            suggestions.push("For local files, use relative paths without URL schemes".to_string());
        } else if error_lower.contains("spaces") {
            suggestions.push("Encode spaces in URLs using %20".to_string());
            suggestions.push("Use quotes around URLs with spaces".to_string());
        } else if error_lower.contains("dangerous") || error_lower.contains("suspicious") {
            suggestions.push("Use a different, safer URL".to_string());
            suggestions.push("Verify the URL is from a trusted source".to_string());
        } else if error_lower.contains("traversal") {
            suggestions.push("Use relative paths without ../ sequences".to_string());
            suggestions.push("Place files in accessible directories".to_string());
        } else if error_lower.contains("localhost") {
            suggestions.push("Use public URLs instead of localhost".to_string());
            suggestions.push("Upload content to a public server".to_string());
        } else if error_lower.contains("long") {
            suggestions.push("Use shorter URLs or URL shortening services".to_string());
            suggestions.push("Simplify the file path".to_string());
        } else {
            suggestions.push("Check the embed syntax is correct".to_string());
            suggestions.push("Verify the target exists and is accessible".to_string());
            suggestions.push("Try using a different format or source".to_string());
        }

        suggestions
    }

    /// Assess error severity for prioritization
    fn assess_embed_error_severity(
        &self,
        error: &str,
        _wikilink: &crucible_core::parser::Wikilink,
    ) -> &'static str {
        let error_lower = error.to_lowercase();

        if error_lower.contains("dangerous")
            || error_lower.contains("suspicious")
            || error_lower.contains("traversal")
        {
            "critical"
        } else if error_lower.contains("scheme") || error_lower.contains("malformed") {
            "high"
        } else if error_lower.contains("empty") || error_lower.contains("format") {
            "medium"
        } else {
            "low"
        }
    }

    /// Extract metadata from external URLs for enhanced embed processing
    ///
    /// Attempts to extract useful metadata from external URLs without
    /// making network requests. Uses pattern matching on known services.
    #[allow(dead_code)]
    fn extract_external_url_metadata(
        &self,
        target: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut metadata = serde_json::Map::new();
        let target_lower = target.to_lowercase();

        // Add URL scheme information
        if let Some(scheme_end) = target_lower.find("://") {
            if let Some(scheme) = target.get(..scheme_end) {
                metadata.insert("url_scheme".to_string(), serde_json::json!(scheme));
            }
        }

        // Detect specific services and add service-specific metadata
        if target_lower.contains("youtube.com/watch")
            || target_lower.contains("youtu.be/")
            || target_lower.contains("youtube.com/embed/")
            || target_lower.contains("youtube.com/shorts/")
        {
            metadata.insert("service".to_string(), serde_json::json!("youtube"));
            if let Some(video_id) = self.extract_youtube_video_id(target) {
                metadata.insert("video_id".to_string(), serde_json::json!(video_id));
            }
        } else if target_lower.contains("vimeo.com/")
            && !target_lower.contains("vimeo.com/channels/")
        {
            metadata.insert("service".to_string(), serde_json::json!("vimeo"));
        } else if target_lower.contains("soundcloud.com/") {
            metadata.insert("service".to_string(), serde_json::json!("soundcloud"));
        } else if target_lower.contains("spotify.com/")
            && (target_lower.contains("/track/")
                || target_lower.contains("/album/")
                || target_lower.contains("/playlist/"))
        {
            metadata.insert("service".to_string(), serde_json::json!("spotify"));
        } else if target_lower.contains("drive.google.com/") {
            metadata.insert("service".to_string(), serde_json::json!("google_drive"));
        } else if target_lower.contains("dropbox.com/") {
            metadata.insert("service".to_string(), serde_json::json!("dropbox"));
        } else if target_lower.contains("gist.githubusercontent.com/") {
            metadata.insert("service".to_string(), serde_json::json!("github"));
        } else if target_lower.contains("github.com/") {
            metadata.insert("service".to_string(), serde_json::json!("github"));
        } else if target_lower.contains("linkedin.com/") {
            metadata.insert("service".to_string(), serde_json::json!("linkedin"));
        } else if target_lower.contains("facebook.com/") {
            metadata.insert("service".to_string(), serde_json::json!("facebook"));
        } else if target_lower.contains("wikipedia.org") {
            metadata.insert("service".to_string(), serde_json::json!("wikipedia"));
        } else if target_lower.contains("pastebin.com/") {
            metadata.insert("service".to_string(), serde_json::json!("pastebin"));
        } else if target_lower.contains("twitch.tv/") {
            metadata.insert("service".to_string(), serde_json::json!("twitch"));
        } else if target_lower.contains("imgur.com/") {
            metadata.insert("service".to_string(), serde_json::json!("imgur"));
        } else if target_lower.contains("news.")
            || target_lower.contains("ycombinator.com/")
            || target_lower.contains("bbc.com/news")
        {
            metadata.insert("service".to_string(), serde_json::json!("news"));
        } else if target_lower.contains("medium.com/") {
            metadata.insert("service".to_string(), serde_json::json!("medium"));
        } else if target_lower.contains("twitter.com/") || target_lower.contains("x.com/") {
            metadata.insert("service".to_string(), serde_json::json!("twitter"));
        } else if target_lower.contains("docs.")
            || target_lower.contains("documentation")
            || target_lower.contains("/docs/")
        {
            metadata.insert("service".to_string(), serde_json::json!("documentation"));
        }

        metadata
    }

    /// Extract YouTube video ID from URL
    fn extract_youtube_video_id(&self, url: &str) -> Option<String> {
        let url_lower = url.to_lowercase();

        if url_lower.contains("youtube.com/watch") {
            // Extract v parameter
            if let Some(start) = url_lower.find("v=") {
                let start = start + 2;
                if let Some(end) = url[start..].find('&') {
                    Some(url[start..start + end].to_string())
                } else {
                    Some(url[start..].to_string())
                }
            } else {
                None
            }
        } else if url_lower.contains("youtu.be/") {
            // Extract ID from short URL
            if let Some(start) = url_lower.find("youtu.be/") {
                let start = start + 9;
                if let Some(end) = url[start..].find('?') {
                    Some(url[start..start + end].to_string())
                } else {
                    Some(url[start..].to_string())
                }
            } else {
                None
            }
        } else if url_lower.contains("youtube.com/embed/") {
            // Extract ID from embed URL
            if let Some(start) = url_lower.find("youtube.com/embed/") {
                let start = start + 18;
                if let Some(end) = url[start..].find('?') {
                    Some(url[start..start + end].to_string())
                } else {
                    Some(url[start..].to_string())
                }
            } else {
                None
            }
        } else if url_lower.contains("youtube.com/shorts/") {
            // Extract ID from shorts URL
            if let Some(start) = url_lower.find("youtube.com/shorts/") {
                let start = start + 20;
                if let Some(end) = url[start..].find('?') {
                    Some(url[start..start + end].to_string())
                } else {
                    Some(url[start..].to_string())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Process content-specific embed logic for different media types
    ///
    /// Adds specialized processing rules and metadata enrichment for different
    /// content types based on their unique characteristics and requirements.
    fn process_content_specific_embed(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        wikilink: &crucible_core::parser::Wikilink,
    ) {
        let embed_type = self.classify_embed_type(&wikilink.target);

        match embed_type {
            "image" => self.process_image_embed(metadata, &wikilink.target),
            "video" => self.process_video_embed(metadata, &wikilink.target),
            "audio" => self.process_audio_embed(metadata, &wikilink.target),
            "pdf" => self.process_pdf_embed(metadata, &wikilink.target),
            "external" => self.process_external_embed(metadata, &wikilink.target),
            "note" => self.process_note_embed(metadata, &wikilink.target),
            _ => {} // No special processing for unknown types
        }
    }

    /// Process image-specific embed logic
    ///
    /// Adds image-specific metadata like dimensions, format information, and display hints.
    fn process_image_embed(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        target: &str,
    ) {
        metadata.insert(
            "content_category".to_string(),
            serde_json::json!(ContentCategory::Image.as_str()),
        );
        metadata.insert("media_type".to_string(), serde_json::json!("image"));

        // Extract image format information
        if let Some(extension) = target.to_lowercase().split('.').next_back() {
            metadata.insert("image_format".to_string(), serde_json::json!(extension));

            // Add format-specific hints
            match extension {
                "svg" => {
                    metadata.insert("is_vector".to_string(), serde_json::json!(true));
                    metadata.insert("scalable".to_string(), serde_json::json!(true));
                    metadata.insert(
                        "background_transparent".to_string(),
                        serde_json::json!(true),
                    );
                }
                "png" => {
                    metadata.insert("is_vector".to_string(), serde_json::json!(false));
                    metadata.insert("supports_transparency".to_string(), serde_json::json!(true));
                    metadata.insert("lossless".to_string(), serde_json::json!(true));
                }
                "jpg" | "jpeg" => {
                    metadata.insert("is_vector".to_string(), serde_json::json!(false));
                    metadata.insert(
                        "supports_transparency".to_string(),
                        serde_json::json!(false),
                    );
                    metadata.insert("lossy".to_string(), serde_json::json!(true));
                    metadata.insert("good_for_photos".to_string(), serde_json::json!(true));
                }
                "gif" => {
                    metadata.insert("is_vector".to_string(), serde_json::json!(false));
                    metadata.insert("animated".to_string(), serde_json::json!(true));
                    metadata.insert("limited_colors".to_string(), serde_json::json!(true));
                }
                "webp" => {
                    metadata.insert("is_vector".to_string(), serde_json::json!(false));
                    metadata.insert("modern_format".to_string(), serde_json::json!(true));
                    metadata.insert("good_compression".to_string(), serde_json::json!(true));
                }
                _ => {
                    // Default for unknown image formats
                    metadata.insert("is_vector".to_string(), serde_json::json!(false));
                }
            }
        }

        // Add display and processing hints
        metadata.insert(
            "display_hint".to_string(),
            serde_json::json!("inline_block"),
        );
        metadata.insert("loading_hint".to_string(), serde_json::json!("lazy"));
        metadata.insert("alt_text_required".to_string(), serde_json::json!(true));
    }

    /// Process video-specific embed logic
    ///
    /// Adds video-specific metadata like service information, quality hints, and playback controls.
    fn process_video_embed(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        target: &str,
    ) {
        metadata.insert(
            "content_category".to_string(),
            serde_json::json!(ContentCategory::Video.as_str()),
        );
        metadata.insert("media_type".to_string(), serde_json::json!("video"));

        // Check for specific video services
        let target_lower = target.to_lowercase();
        if target_lower.contains("youtube.com/watch")
            || target_lower.contains("youtu.be/")
            || target_lower.contains("youtube.com/embed/")
            || target_lower.contains("youtube.com/shorts/")
        {
            metadata.insert("service".to_string(), serde_json::json!("youtube"));
            metadata.insert("requires_embed_player".to_string(), serde_json::json!(true));
            metadata.insert(
                "auto_play_policy".to_string(),
                serde_json::json!("user_gesture_required"),
            );
        } else if target_lower.contains("vimeo.com/")
            && !target_lower.contains("vimeo.com/channels/")
        {
            metadata.insert("service".to_string(), serde_json::json!("vimeo"));
            metadata.insert("requires_embed_player".to_string(), serde_json::json!(true));
        } else {
            // Direct video file or unrecognized platform
            if !metadata.contains_key("service") {
                metadata.insert("service".to_string(), serde_json::json!("direct"));
            }
            metadata.insert(
                "requires_embed_player".to_string(),
                serde_json::json!(false),
            );

            // Extract video format information
            if let Some(extension) = target_lower.split('.').next_back() {
                metadata.insert("video_format".to_string(), serde_json::json!(extension));

                match extension {
                    "mp4" => {
                        metadata
                            .insert("container_format".to_string(), serde_json::json!("MPEG-4"));
                        metadata.insert("widely_supported".to_string(), serde_json::json!(true));
                    }
                    "webm" => {
                        metadata.insert("container_format".to_string(), serde_json::json!("WebM"));
                        metadata.insert("browser_optimized".to_string(), serde_json::json!(true));
                    }
                    "mov" => {
                        metadata.insert(
                            "container_format".to_string(),
                            serde_json::json!("QuickTime"),
                        );
                        metadata.insert("apple_optimized".to_string(), serde_json::json!(true));
                    }
                    _ => {}
                }
            }
        }

        // Add playback and display hints
        metadata.insert("display_hint".to_string(), serde_json::json!("block"));
        metadata.insert("default_controls".to_string(), serde_json::json!(true));
        metadata.insert("responsive_sizing".to_string(), serde_json::json!(true));
        metadata.insert("loading_hint".to_string(), serde_json::json!("lazy"));
    }

    /// Process audio-specific embed logic
    ///
    /// Adds audio-specific metadata like format information, playback controls, and service details.
    fn process_audio_embed(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        target: &str,
    ) {
        metadata.insert(
            "content_category".to_string(),
            serde_json::json!(ContentCategory::Audio.as_str()),
        );
        metadata.insert("media_type".to_string(), serde_json::json!("audio"));

        // Check for specific audio services
        let target_lower = target.to_lowercase();
        if target_lower.contains("soundcloud.com/") {
            metadata.insert("service".to_string(), serde_json::json!("soundcloud"));
            metadata.insert("requires_embed_player".to_string(), serde_json::json!(true));
        } else if target_lower.contains("spotify.com/")
            && (target_lower.contains("/track/")
                || target_lower.contains("/album/")
                || target_lower.contains("/playlist/"))
        {
            metadata.insert("service".to_string(), serde_json::json!("spotify"));
            metadata.insert("requires_embed_player".to_string(), serde_json::json!(true));
            metadata.insert("account_required".to_string(), serde_json::json!(true));
        } else {
            // Direct audio file or unrecognized platform
            if !metadata.contains_key("service") {
                metadata.insert("service".to_string(), serde_json::json!("direct"));
            }
            metadata.insert(
                "requires_embed_player".to_string(),
                serde_json::json!(false),
            );

            // Extract audio format information
            if let Some(extension) = target_lower.split('.').next_back() {
                metadata.insert("audio_format".to_string(), serde_json::json!(extension));

                match extension {
                    "mp3" => {
                        metadata.insert(
                            "codec".to_string(),
                            serde_json::json!("MPEG-1/2/2.5 Layer 3"),
                        );
                        metadata.insert("widely_supported".to_string(), serde_json::json!(true));
                        metadata.insert("lossy".to_string(), serde_json::json!(true));
                    }
                    "wav" => {
                        metadata.insert("codec".to_string(), serde_json::json!("WAV"));
                        metadata.insert("uncompressed".to_string(), serde_json::json!(true));
                        metadata.insert("lossless".to_string(), serde_json::json!(true));
                    }
                    "flac" => {
                        metadata.insert("codec".to_string(), serde_json::json!("FLAC"));
                        metadata.insert("lossless".to_string(), serde_json::json!(true));
                        metadata.insert("compressed".to_string(), serde_json::json!(true));
                    }
                    "ogg" => {
                        metadata.insert("codec".to_string(), serde_json::json!("Ogg Vorbis"));
                        metadata.insert("open_source".to_string(), serde_json::json!(true));
                    }
                    _ => {}
                }
            }
        }

        // Add playback and display hints
        metadata.insert(
            "display_hint".to_string(),
            serde_json::json!("inline_block"),
        );
        metadata.insert("default_controls".to_string(), serde_json::json!(true));
        metadata.insert(
            "auto_play_policy".to_string(),
            serde_json::json!("user_gesture_required"),
        );
    }

    /// Process PDF-specific embed logic
    ///
    /// Adds PDF-specific metadata like viewer requirements, display options, and interaction hints.
    fn process_pdf_embed(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        target: &str,
    ) {
        metadata.insert(
            "content_category".to_string(),
            serde_json::json!(ContentCategory::PDF.as_str()),
        );
        metadata.insert("media_type".to_string(), serde_json::json!("pdf"));

        // PDF-specific processing
        metadata.insert("requires_pdf_viewer".to_string(), serde_json::json!(true));
        metadata.insert("paginated".to_string(), serde_json::json!(true));
        metadata.insert("text_searchable".to_string(), serde_json::json!(true));
        metadata.insert("printable".to_string(), serde_json::json!(true));

        // Add display and interaction hints
        metadata.insert("display_hint".to_string(), serde_json::json!("block"));
        metadata.insert("default_width".to_string(), serde_json::json!("100%"));
        metadata.insert("aspect_ratio".to_string(), serde_json::json!("letter")); // Default to letter size
        metadata.insert("scrollable".to_string(), serde_json::json!(true));

        // Extract potential metadata from filename
        if let Some(filename) = target.split('/').next_back() {
            if filename.to_lowercase().contains("slide")
                || filename.to_lowercase().contains("presentation")
            {
                metadata.insert("likely_presentation".to_string(), serde_json::json!(true));
                metadata.insert(
                    "display_mode_hint".to_string(),
                    serde_json::json!("presentation"),
                );
            } else if filename.to_lowercase().contains("form") {
                metadata.insert("likely_form".to_string(), serde_json::json!(true));
                metadata.insert("interactive".to_string(), serde_json::json!(true));
            }
        }

        // Add security and accessibility hints
        metadata.insert(
            "accessibility_features".to_string(),
            serde_json::json!(vec!["text_to_speech", "screen_reader", "high_contrast"]),
        );
        metadata.insert(
            "security_considerations".to_string(),
            serde_json::json!(vec!["external_links", "javascript", "embedded_content"]),
        );
    }

    /// Process external embed logic (non-media external content)
    ///
    /// Handles external URLs that don't fit into specific media categories.
    fn process_external_embed(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        target: &str,
    ) {
        metadata.insert("media_type".to_string(), serde_json::json!("external"));

        let target_lower = target.to_lowercase();

        // Categorize external content types
        if target_lower.contains("twitter.com/") || target_lower.contains("x.com/") {
            metadata.insert(
                "external_type".to_string(),
                serde_json::json!("social_media_post"),
            );
            metadata.insert("service".to_string(), serde_json::json!("twitter"));
            metadata.insert("requires_javascript".to_string(), serde_json::json!(true));
        } else if target_lower.contains("github.com/") {
            metadata.insert(
                "external_type".to_string(),
                serde_json::json!("code_repository"),
            );
            metadata.insert("service".to_string(), serde_json::json!("github"));
            metadata.insert("interactive".to_string(), serde_json::json!(true));
        } else if target_lower.contains("wikipedia.org") {
            metadata.insert(
                "external_type".to_string(),
                serde_json::json!("encyclopedia_article"),
            );
            metadata.insert("service".to_string(), serde_json::json!("wikipedia"));
            metadata.insert("educational".to_string(), serde_json::json!(true));
        } else if target_lower.contains("news") || target_lower.contains("article") {
            metadata.insert(
                "external_type".to_string(),
                serde_json::json!("news_article"),
            );
            metadata.insert("informational".to_string(), serde_json::json!(true));
        } else {
            metadata.insert("external_type".to_string(), serde_json::json!("webpage"));
            metadata.insert("informational".to_string(), serde_json::json!(true));
        }

        // Add external content handling hints
        metadata.insert("requires_internet".to_string(), serde_json::json!(true));
        metadata.insert("may_change_over_time".to_string(), serde_json::json!(true));
        metadata.insert(
            "potentially_unavailable".to_string(),
            serde_json::json!(true),
        );
        metadata.insert(
            "display_hint".to_string(),
            serde_json::json!("iframe_or_link"),
        );

        // Add privacy and security considerations
        metadata.insert(
            "privacy_considerations".to_string(),
            serde_json::json!(vec!["cookies", "tracking", "third_party_content"]),
        );
        metadata.insert(
            "security_considerations".to_string(),
            serde_json::json!(vec!["external_scripts", "mixed_content"]),
        );
    }

    /// Process note embed logic (markdown files)
    ///
    /// Handles embedded markdown files and notes with note-specific metadata.
    fn process_note_embed(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        target: &str,
    ) {
        metadata.insert(
            "content_category".to_string(),
            serde_json::json!(ContentCategory::Note.as_str()),
        );
        metadata.insert("media_type".to_string(), serde_json::json!("note"));

        // Note-specific processing
        metadata.insert("text_based".to_string(), serde_json::json!(true));
        metadata.insert("searchable".to_string(), serde_json::json!(true));
        metadata.insert("editable".to_string(), serde_json::json!(true));
        metadata.insert(
            "version_control_friendly".to_string(),
            serde_json::json!(true),
        );

        // Extract format information
        if let Some(extension) = target.to_lowercase().split('.').next_back() {
            match extension {
                "md" => {
                    metadata.insert("format".to_string(), serde_json::json!("markdown"));
                    metadata.insert("supports_formatting".to_string(), serde_json::json!(true));
                    metadata.insert("supports_code_blocks".to_string(), serde_json::json!(true));
                }
                "markdown" => {
                    metadata.insert("format".to_string(), serde_json::json!("markdown"));
                    metadata.insert("supports_formatting".to_string(), serde_json::json!(true));
                }
                "txt" => {
                    metadata.insert("format".to_string(), serde_json::json!("plain_text"));
                    metadata.insert("supports_formatting".to_string(), serde_json::json!(false));
                }
                "rst" => {
                    metadata.insert("format".to_string(), serde_json::json!("restructured_text"));
                    metadata.insert("documentation_format".to_string(), serde_json::json!(true));
                }
                _ => {
                    metadata.insert("format".to_string(), serde_json::json!("unknown_text"));
                }
            }
        } else {
            // No extension - assume markdown
            metadata.insert("format".to_string(), serde_json::json!("markdown"));
            metadata.insert("supports_formatting".to_string(), serde_json::json!(true));
        }

        // Add display and processing hints
        metadata.insert(
            "display_hint".to_string(),
            serde_json::json!("inline_or_block"),
        );
        metadata.insert("renderable".to_string(), serde_json::json!(true));
        metadata.insert("syntax_highlighting".to_string(), serde_json::json!(true));

        // Extract potential content hints from filename/path
        let target_lower = target.to_lowercase();
        if target_lower.contains("readme") {
            metadata.insert("likely_readme".to_string(), serde_json::json!(true));
            metadata.insert("project_documentation".to_string(), serde_json::json!(true));
        } else if target_lower.contains("changelog") || target_lower.contains("changes") {
            metadata.insert("likely_changelog".to_string(), serde_json::json!(true));
            metadata.insert("version_history".to_string(), serde_json::json!(true));
        } else if target_lower.contains("todo") || target_lower.contains("tasks") {
            metadata.insert("likely_task_list".to_string(), serde_json::json!(true));
            metadata.insert("actionable".to_string(), serde_json::json!(true));
        }

        // Add accessibility features
        metadata.insert(
            "accessibility_features".to_string(),
            serde_json::json!(vec!["screen_reader", "high_contrast", "text_sizing"]),
        );
    }

    /// Extract footnote relations from the note
    fn extract_footnote_relations(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedNote,
    ) -> Vec<CoreRelation> {
        let capacity = doc.footnotes.references.len();
        let mut relations = Vec::with_capacity(capacity);
        let from_entity_id = format!("{}:{}", entity_id.table, entity_id.id);

        for footnote_ref in &doc.footnotes.references {
            // Only create relations for references that have definitions
            if let Some(definition) = doc.footnotes.definitions.get(&footnote_ref.identifier) {
                let mut relation = CoreRelation::new(
                    from_entity_id.clone(),
                    None, // Footnotes are self-referential (within the same note)
                    "footnote",
                );

                // Add metadata about the footnote
                let mut metadata = serde_json::Map::new();
                metadata.insert(
                    "label".to_string(),
                    serde_json::json!(footnote_ref.identifier),
                );
                metadata.insert("content".to_string(), serde_json::json!(definition.content));
                metadata.insert(
                    "ref_offset".to_string(),
                    serde_json::json!(footnote_ref.offset),
                );
                metadata.insert(
                    "def_offset".to_string(),
                    serde_json::json!(definition.offset),
                );
                if let Some(order) = footnote_ref.order_number {
                    metadata.insert("order".to_string(), serde_json::json!(order));
                }

                relation.metadata = serde_json::Value::Object(metadata);

                relations.push(relation);
            }
        }

        relations
    }

    /// Create block with metadata
    fn make_block_with_metadata(
        &self,
        entity_id: &RecordId<EntityRecord>,
        suffix: &str,
        block_index: i32,
        block_type: &str,
        content: &str,
        metadata: Value,
    ) -> BlockNode {
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        let hash = hasher.finalize().to_hex().to_string();

        let mut block = BlockNode::new(
            RecordId::new("blocks", format!("{}:{}", entity_id.id, suffix)),
            entity_id.clone(),
            block_index,
            block_type,
            content,
            hash,
        );
        block.metadata = metadata;
        block
    }

    /// Build blocks from note content
    fn build_blocks(&self, entity_id: &RecordId<EntityRecord>, doc: &ParsedNote) -> Vec<BlockNode> {
        // Pre-calculate capacity: sum of all content types
        // Headings + code_blocks + lists + callouts + latex + paragraphs (upper bound)
        // Note: Actual paragraph count may be lower due to filtering empty ones
        let capacity = doc.content.headings.len()
            + doc.content.paragraphs.len()
            + doc.content.code_blocks.len()
            + doc.content.lists.len()
            + doc.callouts.len()
            + doc.content.latex_expressions.len();
        let mut blocks = Vec::with_capacity(capacity);
        let mut index = 0;

        // Headings with metadata (level + text)
        for heading in &doc.content.headings {
            let metadata = serde_json::json!({
                "level": heading.level,
                "text": heading.text.clone()
            });
            blocks.push(self.make_block_with_metadata(
                entity_id,
                &format!("h{}", index),
                index,
                "heading",
                &heading.text,
                metadata,
            ));
            index += 1;
        }

        // Paragraphs (non-empty only)
        for paragraph in &doc.content.paragraphs {
            if paragraph.content.trim().is_empty() {
                continue;
            }
            blocks.push(self.make_block_with_metadata(
                entity_id,
                &format!("p{}", index),
                index,
                "paragraph",
                &paragraph.content,
                serde_json::json!({}),
            ));
            index += 1;
        }

        // Code blocks with language + line count metadata
        for code_block in &doc.content.code_blocks {
            let metadata = serde_json::json!({
                "language": code_block.language.clone().unwrap_or_default(),
                "line_count": code_block.content.lines().count()
            });
            blocks.push(self.make_block_with_metadata(
                entity_id,
                &format!("code{}", index),
                index,
                "code",
                &code_block.content,
                metadata,
            ));
            index += 1;
        }

        // Lists with type + item count metadata
        for list in &doc.content.lists {
            let metadata = serde_json::json!({
                "type": match list.list_type {
                    crucible_core::parser::ListType::Ordered => "ordered",
                    crucible_core::parser::ListType::Unordered => "unordered",
                },
                "item_count": list.items.len()
            });

            // Serialize list as text (simple approach for now)
            let list_text = list
                .items
                .iter()
                .map(|item| {
                    if let Some(task_status) = &item.task_status {
                        let check = match task_status {
                            crucible_core::parser::TaskStatus::Pending => " ",
                            crucible_core::parser::TaskStatus::Completed => "x",
                        };
                        format!("- [{}] {}", check, item.content)
                    } else {
                        format!("- {}", item.content)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            blocks.push(self.make_block_with_metadata(
                entity_id,
                &format!("list{}", index),
                index,
                "list",
                &list_text,
                metadata,
            ));
            index += 1;
        }

        // Callouts with type + title metadata
        for callout in &doc.callouts {
            let metadata = serde_json::json!({
                "callout_type": callout.callout_type.clone(),
                "title": callout.title.clone().unwrap_or_default()
            });
            blocks.push(self.make_block_with_metadata(
                entity_id,
                &format!("callout{}", index),
                index,
                "callout",
                &callout.content,
                metadata,
            ));
            index += 1;
        }

        // LaTeX expressions with inline/block flag
        for latex in &doc.content.latex_expressions {
            let metadata = serde_json::json!({
                "inline": !latex.is_block,
                "display_mode": latex.is_block
            });
            blocks.push(self.make_block_with_metadata(
                entity_id,
                &format!("latex{}", index),
                index,
                "latex",
                &latex.expression,
                metadata,
            ));
            index += 1;
        }

        blocks
    }
}

// Implement EnrichedNoteStore trait for NoteIngestor
#[async_trait::async_trait]
impl<'a> crucible_core::EnrichedNoteStore for NoteIngestor<'a> {
    async fn store_enriched(
        &self,
        enriched: &crucible_core::enrichment::EnrichedNote,
        relative_path: &str,
    ) -> Result<()> {
        // Build merkle tree from parsed note (infrastructure concern)
        let merkle_tree = crucible_merkle::HybridMerkleTree::from_document(&enriched.parsed);

        // Wrap core type with infrastructure-specific merkle tree
        let enriched_with_tree = crucible_enrichment::EnrichedNoteWithTree {
            core: enriched.clone(),
            merkle_tree,
        };

        // Delegate to existing ingest_enriched implementation
        self.ingest_enriched(&enriched_with_tree, relative_path)
            .await?;
        Ok(())
    }

    async fn note_exists(&self, relative_path: &str) -> Result<bool> {
        // Check if note entity exists by querying the database directly
        let entity_id = self.note_entity_id(relative_path);

        // Use direct SQL query to check entity existence
        let sql = format!("SELECT * FROM {} LIMIT 1", entity_id);
        match self.store.client.query(&sql, &[]).await {
            Ok(result) => Ok(!result.records.is_empty()),
            Err(_) => Ok(false),
        }
    }
}
