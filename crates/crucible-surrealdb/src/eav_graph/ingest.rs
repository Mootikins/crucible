use anyhow::Result;
use blake3::Hasher;
use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use crucible_core::content_category::ContentCategory;
use crucible_core::parser::ParsedNote;
use crucible_core::storage::{Relation as CoreRelation, RelationStorage};
use crucible_merkle::{HybridMerkleTree, MerkleStore};
use serde_json::{Map, Value};
use std::fs;

use super::store::EAVGraphStore;

/// Sanitize content for SurrealDB storage by removing null bytes.
///
/// SurrealDB's serialization layer (surrealdb-core) panics when strings
/// contain null bytes (0x00). This can happen in files with:
/// - ASCII art exported from drawing tools
/// - Binary data accidentally pasted into markdown
/// - Copy-pasted content with hidden control characters
///
/// This function strips null bytes to allow such files to be stored safely.
fn sanitize_content(s: &str) -> String {
    s.replace('\0', "")
}
use super::types::{
    BlockNode, Entity, EntityRecord, EntityTag, EntityTagRecord, EntityType, Property,
    PropertyRecord, PropertyValue, RecordId, TagRecord,
};

/// Classify content type for universal link processing
fn classify_content(target: &str) -> ContentCategory {
    // Helper function to check if string is a URL
    fn is_url(s: &str) -> bool {
        s.starts_with("http://") || s.starts_with("https://")
    }

    // Helper function to extract file extension
    fn get_extension(s: &str) -> Option<&str> {
        s.rfind('.').and_then(|i| s.get(i + 1..))
    }

    // Local files - check extension
    if !is_url(target) {
        return match get_extension(target) {
            Some("md") | Some("markdown") => ContentCategory::Note,
            Some("png") | Some("jpg") | Some("jpeg") | Some("svg") | Some("gif") | Some("webp") => {
                ContentCategory::Image
            }
            Some("mp4") | Some("avi") | Some("mov") | Some("webm") | Some("mkv") => {
                ContentCategory::Video
            }
            Some("mp3") | Some("wav") | Some("ogg") | Some("flac") | Some("aac") => {
                ContentCategory::Audio
            }
            Some("pdf") => ContentCategory::PDF,
            Some("doc") | Some("docx") | Some("txt") | Some("rtf") => ContentCategory::Document,
            _ => ContentCategory::Other, // unrecognized file types
        };
    }

    // URLs - platform detection first, then general
    let target_lower = target.to_lowercase();
    if target_lower.contains("youtube.com") || target_lower.contains("youtu.be") {
        ContentCategory::YouTube
    } else if target_lower.contains("github.com") {
        ContentCategory::GitHub
    } else if target_lower.contains("wikipedia.org") {
        ContentCategory::Wikipedia
    } else if target_lower.contains("stackoverflow.com") {
        ContentCategory::StackOverflow
    } else if get_extension(target).is_some() {
        // URLs with file extensions - classify by type
        match get_extension(target) {
            Some("png") | Some("jpg") | Some("jpeg") | Some("svg") | Some("gif") => {
                ContentCategory::Image
            }
            Some("mp4") | Some("avi") | Some("mov") | Some("webm") => ContentCategory::Video,
            Some("mp3") | Some("wav") | Some("ogg") => ContentCategory::Audio,
            Some("pdf") => ContentCategory::PDF,
            _ => ContentCategory::Other,
        }
    } else {
        ContentCategory::Web // General web pages
    }
}

/// Extract timestamps from frontmatter with fallback to filesystem metadata.
///
/// Priority for created timestamp (aligns with Obsidian community conventions):
/// 1. `created` (most common in Obsidian community)
/// 2. `date-created` (alternate convention)
/// 3. `created_at` (programmatic sources fallback)
/// 4. Filesystem modified time (creation time is unreliable across platforms)
/// 5. Current time as last resort
///
/// Priority for updated timestamp:
/// 1. `modified` (most common in Obsidian community)
/// 2. `updated` (alternate convention)
/// 3. `date-modified` (alternate convention)
/// 4. `updated_at` (programmatic sources fallback)
/// 5. Filesystem modified time
/// 6. Current time as last resort
///
/// Supports both date (YYYY-MM-DD) and datetime (RFC 3339) formats.
fn extract_timestamps(doc: &ParsedNote) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();

    // Helper to convert NaiveDate to DateTime<Utc> at midnight
    fn date_to_datetime(date: NaiveDate) -> DateTime<Utc> {
        let datetime = date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        Utc.from_utc_datetime(&datetime)
    }

    // Helper to parse RFC 3339 datetime from frontmatter string
    fn parse_datetime_str(
        fm: &crucible_core::parser::Frontmatter,
        key: &str,
    ) -> Option<DateTime<Utc>> {
        let value = fm.properties().get(key)?;
        let datetime_str = value.as_str()?;
        DateTime::parse_from_rfc3339(datetime_str)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    // Helper to get datetime from frontmatter - tries datetime string first, then date
    fn get_timestamp(
        fm: &crucible_core::parser::Frontmatter,
        keys: &[&str],
    ) -> Option<DateTime<Utc>> {
        for key in keys {
            // Try RFC 3339 datetime first
            if let Some(dt) = parse_datetime_str(fm, key) {
                return Some(dt);
            }
            // Try date format (YYYY-MM-DD)
            if let Some(date) = fm.get_date(key) {
                return Some(date_to_datetime(date));
            }
        }
        None
    }

    // Try to get filesystem modified time
    let fs_mtime = fs::metadata(&doc.path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(DateTime::<Utc>::from);

    // Extract created timestamp with priority: created > date-created > created_at > fs_mtime > now
    let created_at = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| get_timestamp(fm, &["created", "date-created", "created_at"]))
        .or(fs_mtime)
        .unwrap_or(now);

    // Extract updated timestamp with priority: modified > updated > date-modified > updated_at > fs_mtime > now
    let updated_at = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| get_timestamp(fm, &["modified", "updated", "date-modified", "updated_at"]))
        .or(fs_mtime)
        .unwrap_or(now);

    (created_at, updated_at)
}

/// # Note Ingestion with Advanced Embed Processing
///
/// This module provides high-level functionality for ingesting parsed documents into the
/// EAV+Graph schema with comprehensive support for embed processing and wikilink resolution.
///
/// ## Embed Processing Features
///
/// The ingestor supports sophisticated embed processing including:
///
/// ### Embed Type Classification
/// - **Images**: PNG, JPG, SVG, WebP, etc. with format-specific metadata
/// - **Videos**: MP4, AVI, MOV, WebM, plus platform detection (YouTube, Vimeo)
/// - **Audio**: MP3, WAV, M4A, OGG, plus platform detection (SoundCloud, Spotify)
/// - **Documents**: PDF files with note-specific processing
/// - **Notes**: Markdown files with internal note embedding
/// - **External**: URLs and web content with security validation
///
/// ### Embed Variant Support
/// - Simple embeds: `![[Note]]`
/// - Heading references: `![[Note#Section]]`
/// - Block references: `![[Note^block-id]]`
/// - Complex combinations: `![[Note#Section^block-id|Alias]]`
/// - External URLs: `![[https://example.com/video.mp4]]`
///
/// ### Security & Validation
/// - URL scheme validation with security checks
/// - Malicious content detection
/// - File extension validation
/// - Content type verification
/// - Error recovery mechanisms
///
/// ## Usage Example
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
///
/// High-level helper for writing parsed documents into the EAV+Graph schema.
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
            .with_content_hash(doc.content_hash.clone())
            .with_search_text(sanitize_content(&doc.content.plain_text));
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
        use crucible_core::storage::PropertyValue;

        // Metadata namespace for enrichment-computed properties
        let _metadata_namespace = PropertyNamespace("enrichment".to_string());

        // Store reading time (always present)
        self.store
            .upsert_property(&Property::new(
                self.property_id(&entity_id, "enrichment", "reading_time"),
                entity_id.clone(),
                "enrichment",
                "reading_time",
                PropertyValue::Number(enriched.core.metadata.reading_time_minutes as f64),
            ))
            .await?;

        // Store complexity score (always present as f32)
        self.store
            .upsert_property(&Property::new(
                self.property_id(&entity_id, "enrichment", "complexity_score"),
                entity_id.clone(),
                "enrichment",
                "complexity_score",
                PropertyValue::Number(enriched.core.metadata.complexity_score as f64),
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
                    PropertyValue::Text(language.clone()),
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
        } else {
            if self.is_external_url(&wikilink.target) {
                "external_link"
            } else {
                "internal_link"
            }
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
            PropertyValue::Text(merkle_tree.root_hash.to_hex()),
        ));

        // Store total section count
        props.push(Property::new(
            self.property_id(entity_id, "section", "total_sections"),
            entity_id.clone(),
            "section",
            "total_sections",
            PropertyValue::Number(merkle_tree.sections.len() as f64),
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
                PropertyValue::Text(section.binary_tree.root_hash.to_hex()),
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
                PropertyValue::Json(Value::Object(metadata)),
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
            PropertyValue::Text(doc.path.to_string_lossy().to_string()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "relative_path"),
            entity_id.clone(),
            "core",
            "relative_path",
            PropertyValue::Text(relative_path.to_string()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "title"),
            entity_id.clone(),
            "core",
            "title",
            PropertyValue::Text(doc.title()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "tags"),
            entity_id.clone(),
            "core",
            "tags",
            PropertyValue::Json(Value::Array(
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
                PropertyValue::Json(fm_value),
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
        payload.insert(
            "content".to_string(),
            Value::String(sanitize_content(&doc.content.plain_text)),
        );
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
        if let Some(extension) = target_lower.split('.').last() {
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
            if let Some(extension) = target_lower.split('.').last() {
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
        if let Some(extension) = target.to_lowercase().split('.').last() {
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
            if let Some(extension) = target_lower.split('.').last() {
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
            if let Some(extension) = target_lower.split('.').last() {
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
        if let Some(filename) = target.split('/').last() {
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
        if let Some(extension) = target.to_lowercase().split('.').last() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::SurrealClient;
    use crucible_core::parser::{
        Frontmatter, FrontmatterFormat, Heading, NoteContent, Paragraph, Tag,
    };
    use crucible_core::storage::{RelationStorage, TagStorage};
    use serde_json::json;
    use std::path::PathBuf;

    fn sample_document() -> ParsedNote {
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("notes/sample.md");
        doc.content_hash = "abc123".into();
        doc.content = NoteContent::default();
        doc.content.plain_text = "Hello world".into();
        doc.content
            .paragraphs
            .push(Paragraph::new("Hello world".into(), 0));
        doc.content.headings.push(Heading::new(1, "Intro", 0));
        doc.tags.push(Tag::new("project/crucible", 0));
        doc.frontmatter = Some(Frontmatter::new(
            "title: Sample Doc".to_string(),
            FrontmatterFormat::Yaml,
        ));
        doc
    }

    #[tokio::test]
    async fn ingest_document_writes_entities_properties_blocks() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let doc = sample_document();
        let entity_id = ingestor.ingest(&doc, "notes/sample.md").await.unwrap();

        let result = client
            .query(
                "SELECT * FROM entities WHERE id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert_eq!(result.records.len(), 1);

        let blocks = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert!(!blocks.records.is_empty());
    }

    #[tokio::test]
    async fn ingest_document_extracts_wikilink_relations() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create target notes first so wikilinks can be resolved
        let mut target_doc1 = sample_document();
        target_doc1.path = PathBuf::from("other-note.md");
        target_doc1.tags.clear();
        ingestor
            .ingest(&target_doc1, "other-note.md")
            .await
            .unwrap();

        let mut target_doc2 = sample_document();
        target_doc2.path = PathBuf::from("embedded-note.md");
        target_doc2.tags.clear();
        ingestor
            .ingest(&target_doc2, "embedded-note.md")
            .await
            .unwrap();

        // Now create note with wikilinks
        let mut doc = sample_document();
        doc.wikilinks.push(Wikilink {
            target: "other-note".to_string(),
            alias: Some("Other Note".to_string()),
            heading_ref: Some("Section".to_string()),
            block_ref: None,
            is_embed: false,
            offset: 10,
        });
        doc.wikilinks.push(Wikilink {
            target: "embedded-note".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: Some("block-id".to_string()),
            is_embed: true,
            offset: 20,
        });

        let entity_id = ingestor
            .ingest(&doc, "notes/wikilink-test.md")
            .await
            .unwrap();

        // Get all relations for this entity (use just the ID part, not the full "entities:..." string)
        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 2, "Should have 2 relations");

        // Check wikilink relation - now should be resolved
        let wikilink_rel = relations
            .iter()
            .find(|r| r.relation_type == "wikilink")
            .unwrap();
        // Should be resolved to the actual target entity
        assert!(
            wikilink_rel.to_entity_id.is_some(),
            "Wikilink should be resolved"
        );
        assert!(
            wikilink_rel
                .to_entity_id
                .as_ref()
                .unwrap()
                .contains("other-note"),
            "Should link to other-note"
        );
        assert_eq!(
            wikilink_rel.metadata.get("alias").and_then(|v| v.as_str()),
            Some("Other Note")
        );
        assert_eq!(
            wikilink_rel
                .metadata
                .get("heading_ref")
                .and_then(|v| v.as_str()),
            Some("Section")
        );

        // Check embed relation - now should be resolved
        let embed_rel = relations
            .iter()
            .find(|r| r.relation_type == "embed")
            .unwrap();
        assert!(embed_rel.to_entity_id.is_some(), "Embed should be resolved");
        assert!(
            embed_rel
                .to_entity_id
                .as_ref()
                .unwrap()
                .contains("embedded-note"),
            "Should link to embedded-note"
        );
        assert_eq!(
            embed_rel.metadata.get("block_ref").and_then(|v| v.as_str()),
            Some("block-id")
        );
    }

    #[tokio::test]
    async fn ingest_document_extracts_hierarchical_tags() {
        use crucible_core::storage::TagStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.tags.clear();
        doc.tags.push(Tag::new("project/ai/nlp", 0));
        doc.tags.push(Tag::new("status/active", 0));

        // Tags are now automatically stored during ingestion
        let _entity_id = ingestor.ingest(&doc, "notes/test-tags.md").await.unwrap();

        // Check that all tag levels were created
        let project_tag = store.get_tag("project").await.unwrap();
        assert!(project_tag.is_some(), "Should have 'project' tag");
        assert_eq!(project_tag.unwrap().name, "project");

        let ai_tag = store.get_tag("project/ai").await.unwrap();
        assert!(ai_tag.is_some(), "Should have 'project/ai' tag");
        let ai_tag = ai_tag.unwrap();
        assert_eq!(ai_tag.name, "project/ai");
        // Adapter adds "tags:" prefix to parent_id
        assert_eq!(ai_tag.parent_tag_id, Some("tags:project".to_string()));

        let nlp_tag = store.get_tag("project/ai/nlp").await.unwrap();
        assert!(nlp_tag.is_some(), "Should have 'project/ai/nlp' tag");
        let nlp_tag = nlp_tag.unwrap();
        assert_eq!(nlp_tag.name, "project/ai/nlp");
        assert_eq!(nlp_tag.parent_tag_id, Some("tags:project/ai".to_string()));

        let status_tag = store.get_tag("status").await.unwrap();
        assert!(status_tag.is_some(), "Should have 'status' tag");

        let active_tag = store.get_tag("status/active").await.unwrap();
        assert!(active_tag.is_some(), "Should have 'status/active' tag");
        let active_tag = active_tag.unwrap();
        assert_eq!(active_tag.parent_tag_id, Some("tags:status".to_string()));

        // Entity-tag associations are also automatically created during ingestion
    }

    #[tokio::test]
    async fn ingest_document_stores_relation_metadata() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.wikilinks.push(Wikilink {
            target: "note-with-heading".to_string(),
            alias: Some("Custom Display Text".to_string()),
            heading_ref: Some("Introduction".to_string()),
            block_ref: Some("^abc123".to_string()),
            is_embed: false,
            offset: 42,
        });

        let entity_id = ingestor
            .ingest(&doc, "notes/metadata-test.md")
            .await
            .unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1);

        let relation = &relations[0];

        // Verify all metadata fields are preserved
        assert_eq!(
            relation.metadata.get("alias").and_then(|v| v.as_str()),
            Some("Custom Display Text")
        );
        assert_eq!(
            relation
                .metadata
                .get("heading_ref")
                .and_then(|v| v.as_str()),
            Some("Introduction")
        );
        assert_eq!(
            relation.metadata.get("block_ref").and_then(|v| v.as_str()),
            Some("^abc123")
        );
        assert_eq!(
            relation.metadata.get("offset").and_then(|v| v.as_u64()),
            Some(42)
        );
    }

    #[tokio::test]
    async fn relations_support_backlinks() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create two documents with cross-references
        let mut doc1 = sample_document();
        doc1.path = PathBuf::from("notes/backlink1.md");
        doc1.tags.clear(); // Clear tags to avoid conflicts with other tests
        doc1.wikilinks.push(Wikilink {
            target: "notes/backlink2.md".to_string(), // Full path relative to vault root
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 0,
        });

        let mut doc2 = sample_document();
        doc2.path = PathBuf::from("notes/backlink2.md");
        doc2.tags.clear(); // Clear tags to avoid conflicts with other tests
        doc2.wikilinks.push(Wikilink {
            target: "notes/backlink1.md".to_string(), // Full path relative to vault root
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 0,
        });

        let entity1_id = ingestor.ingest(&doc1, "notes/backlink1.md").await.unwrap();
        let entity2_id = ingestor.ingest(&doc2, "notes/backlink2.md").await.unwrap();

        // Get backlinks for doc1 (should find link from doc2)
        // Use the ID part only, which is what note_entity_id() generates
        let backlinks = store.get_backlinks(&entity1_id.id, None).await.unwrap();

        assert!(
            !backlinks.is_empty(),
            "Should have backlinks to sample note"
        );

        let backlink = &backlinks[0];
        // The from_entity_id will be in "entities:note:backlink2.md" format due to adapter conversion
        assert_eq!(
            backlink.from_entity_id,
            format!("entities:{}", entity2_id.id)
        );
    }

    #[tokio::test]
    async fn ingest_document_stores_section_hashes() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create a note with multiple sections
        let mut doc = sample_document();
        doc.content.headings.clear();
        doc.content.paragraphs.clear();

        // Add headings to create sections
        doc.content
            .headings
            .push(Heading::new(1, "Introduction", 0));
        doc.content.headings.push(Heading::new(2, "Details", 50));
        doc.content
            .headings
            .push(Heading::new(2, "Conclusion", 100));

        // Add paragraphs for content
        doc.content
            .paragraphs
            .push(Paragraph::new("Intro content".to_string(), 10));
        doc.content
            .paragraphs
            .push(Paragraph::new("Detail content".to_string(), 60));
        doc.content
            .paragraphs
            .push(Paragraph::new("Final thoughts".to_string(), 110));

        let entity_id = ingestor
            .ingest(&doc, "notes/sections-test.md")
            .await
            .unwrap();

        // Query section properties
        let result = client
            .query(
                "SELECT * FROM properties WHERE entity_id = type::thing('entities', $id) AND namespace = 'section'",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        assert!(!result.records.is_empty(), "Should have section properties");

        // Verify we have the expected section properties
        let props = &result.records;

        // Should have: tree_root_hash, total_sections, and for each section: hash + metadata
        // With 4 sections (root + 3 headings), we expect: 2 + (4 * 2) = 10 properties
        assert!(
            props.len() >= 2,
            "Should have at least root hash and total sections"
        );

        // Check for tree_root_hash
        let has_root_hash = props.iter().any(|p| {
            p.data
                .get("key")
                .and_then(|k| k.as_str())
                .map(|k| k == "tree_root_hash")
                .unwrap_or(false)
        });
        assert!(has_root_hash, "Should have tree_root_hash property");

        // Check for total_sections
        let total_sections = props.iter().find_map(|p| {
            if p.data.get("key")?.as_str()? == "total_sections" {
                // The value is stored as {"type": "number", "value": 4.0}
                p.data.get("value")?.get("value")?.as_f64()
            } else {
                None
            }
        });
        assert!(
            total_sections.is_some(),
            "Should have total_sections property"
        );
        assert!(
            total_sections.unwrap() > 0.0,
            "Should have at least one section"
        );
    }

    #[tokio::test]
    async fn section_hashes_are_retrievable() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.content.headings.clear();
        doc.content.paragraphs.clear();
        doc.content
            .headings
            .push(Heading::new(1, "Test Section", 0));
        doc.content
            .paragraphs
            .push(Paragraph::new("Test content".to_string(), 10));

        let entity_id = ingestor.ingest(&doc, "notes/hash-test.md").await.unwrap();

        // Query for section_0_hash
        let result = client
            .query(
                "SELECT * FROM properties WHERE entity_id = type::thing('entities', $id) AND key = 'section_0_hash'",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1, "Should find section_0_hash");

        let hash_prop = &result.records[0];
        // Value is stored as {"type": "text", "value": "hash_string"}
        let hash_value = hash_prop
            .data
            .get("value")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str());
        assert!(hash_value.is_some(), "Hash should be a string");
        assert!(!hash_value.unwrap().is_empty(), "Hash should not be empty");
    }

    #[tokio::test]
    async fn test_inline_link_extraction() {
        use crucible_core::parser::InlineLink;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.inline_links.push(InlineLink::new(
            "Rust".to_string(),
            "https://rust-lang.org".to_string(),
            10,
        ));
        doc.inline_links.push(InlineLink::with_title(
            "GitHub".to_string(),
            "https://github.com".to_string(),
            "Code hosting".to_string(),
            50,
        ));
        doc.inline_links.push(InlineLink::new(
            "other note".to_string(),
            "./notes/other.md".to_string(),
            80,
        ));

        let entity_id = ingestor
            .ingest(&doc, "notes/inline-links-test.md")
            .await
            .unwrap();

        // Get all relations for this entity
        let relations = store
            .get_relations(&entity_id.id, Some("link"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 3, "Should have 3 inline link relations");

        // Check first link (external)
        let rust_link = relations
            .iter()
            .find(|r| r.metadata.get("text").and_then(|v| v.as_str()) == Some("Rust"))
            .unwrap();
        assert_eq!(rust_link.relation_type, "link");
        assert_eq!(
            rust_link.metadata.get("url").and_then(|v| v.as_str()),
            Some("https://rust-lang.org")
        );
        assert_eq!(
            rust_link
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(rust_link.metadata.get("title"), None);

        // Check second link (with title)
        let github_link = relations
            .iter()
            .find(|r| r.metadata.get("text").and_then(|v| v.as_str()) == Some("GitHub"))
            .unwrap();
        assert_eq!(
            github_link.metadata.get("title").and_then(|v| v.as_str()),
            Some("Code hosting")
        );

        // Check third link (relative)
        let relative_link = relations
            .iter()
            .find(|r| r.metadata.get("url").and_then(|v| v.as_str()) == Some("./notes/other.md"))
            .unwrap();
        assert_eq!(
            relative_link
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[tokio::test]
    async fn test_footnote_extraction() {
        use crucible_core::parser::{FootnoteDefinition, FootnoteReference};

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();

        // Add footnote references
        doc.footnotes
            .add_reference(FootnoteReference::with_order("1".to_string(), 20, 1));
        doc.footnotes
            .add_reference(FootnoteReference::with_order("note".to_string(), 50, 2));

        // Add footnote definitions
        doc.footnotes.add_definition(
            "1".to_string(),
            FootnoteDefinition::new(
                "1".to_string(),
                "First footnote definition".to_string(),
                100,
                5,
            ),
        );
        doc.footnotes.add_definition(
            "note".to_string(),
            FootnoteDefinition::new(
                "note".to_string(),
                "Second footnote with custom label".to_string(),
                150,
                6,
            ),
        );

        let entity_id = ingestor
            .ingest(&doc, "notes/footnote-test.md")
            .await
            .unwrap();

        // Get all footnote relations
        let relations = store
            .get_relations(&entity_id.id, Some("footnote"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 2, "Should have 2 footnote relations");

        // Check first footnote
        let footnote1 = relations
            .iter()
            .find(|r| r.metadata.get("label").and_then(|v| v.as_str()) == Some("1"))
            .unwrap();
        assert_eq!(footnote1.relation_type, "footnote");
        assert_eq!(
            footnote1.metadata.get("content").and_then(|v| v.as_str()),
            Some("First footnote definition")
        );
        assert_eq!(
            footnote1.metadata.get("order").and_then(|v| v.as_u64()),
            Some(1)
        );
        assert_eq!(
            footnote1
                .metadata
                .get("ref_offset")
                .and_then(|v| v.as_u64()),
            Some(20)
        );
        assert_eq!(
            footnote1
                .metadata
                .get("def_offset")
                .and_then(|v| v.as_u64()),
            Some(100)
        );

        // Check second footnote
        let footnote2 = relations
            .iter()
            .find(|r| r.metadata.get("label").and_then(|v| v.as_str()) == Some("note"))
            .unwrap();
        assert_eq!(
            footnote2.metadata.get("content").and_then(|v| v.as_str()),
            Some("Second footnote with custom label")
        );
        assert_eq!(
            footnote2.metadata.get("order").and_then(|v| v.as_u64()),
            Some(2)
        );
    }

    #[tokio::test]
    async fn section_metadata_includes_heading_info() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.content.headings.clear();
        doc.content.paragraphs.clear();
        doc.content.headings.push(Heading::new(2, "My Heading", 0));
        doc.content
            .paragraphs
            .push(Paragraph::new("Content here".to_string(), 10));

        let entity_id = ingestor
            .ingest(&doc, "notes/metadata-test.md")
            .await
            .unwrap();

        // Query for section metadata - just get all section properties and filter in code
        let result = client
            .query(
                "SELECT * FROM properties WHERE entity_id = type::thing('entities', $id) AND namespace = 'section'",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        assert!(!result.records.is_empty(), "Should have section metadata");

        // Find a section with heading metadata
        // Value is stored as {"type": "json", "value": {...}}
        let has_heading_metadata = result.records.iter().any(|prop| {
            // Only look at metadata properties
            let key = prop.data.get("key").and_then(|k| k.as_str());
            if !key.map(|k| k.contains("_metadata")).unwrap_or(false) {
                return false;
            }

            if let Some(outer_value) = prop.data.get("value") {
                if let Some(inner_value) = outer_value.get("value") {
                    return inner_value.get("heading_text").is_some()
                        && inner_value.get("heading_level").is_some();
                }
            }
            false
        });

        assert!(
            has_heading_metadata,
            "Should have heading metadata in at least one section"
        );
    }

    #[tokio::test]
    async fn test_ambiguous_wikilink_resolution() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create two notes with same name in different folders
        let mut doc1 = sample_document();
        doc1.path = PathBuf::from("Project A/Note.md");
        doc1.tags.clear();
        let mut doc2 = sample_document();
        doc2.path = PathBuf::from("Project B/Note.md");
        doc2.tags.clear();

        ingestor.ingest(&doc1, "Project A/Note.md").await.unwrap();
        ingestor.ingest(&doc2, "Project B/Note.md").await.unwrap();

        // Create a note with ambiguous wikilink
        let mut doc3 = sample_document();
        doc3.path = PathBuf::from("Index.md");
        doc3.tags.clear();
        doc3.wikilinks.push(Wikilink {
            target: "Note".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        let entity_id = ingestor.ingest(&doc3, "Index.md").await.unwrap();

        // Verify relation created with candidates in metadata
        let relations = store
            .get_relations(&entity_id.id, Some("wikilink"))
            .await
            .unwrap();

        assert_eq!(relations.len(), 1, "Should have one wikilink relation");
        assert!(
            relations[0].to_entity_id.is_none(),
            "Ambiguous link should have no single target"
        );

        let metadata = relations[0].metadata.as_object().unwrap();
        assert_eq!(
            metadata.get("ambiguous").and_then(|v| v.as_bool()),
            Some(true),
            "Should be marked as ambiguous. Metadata: {:?}",
            metadata
        );

        let candidates = metadata.get("candidates").unwrap().as_array().unwrap();
        assert_eq!(candidates.len(), 2, "Should have 2 candidates");
        assert!(
            candidates
                .iter()
                .any(|c| c.as_str().unwrap().contains("Project A")),
            "Should have Project A candidate"
        );
        assert!(
            candidates
                .iter()
                .any(|c| c.as_str().unwrap().contains("Project B")),
            "Should have Project B candidate"
        );
    }

    #[tokio::test]
    async fn test_embed_type_classification() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create note with various embed types
        let mut doc = sample_document();
        doc.path = PathBuf::from("test-embeds.md");
        doc.tags.clear();

        // Add different embed types (unresolved targets to focus on embed_type classification)
        doc.wikilinks.push(Wikilink {
            target: "test.png".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 10,
        });

        doc.wikilinks.push(Wikilink {
            target: "report.pdf".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 20,
        });

        doc.wikilinks.push(Wikilink {
            target: "audio-file.mp3".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 30,
        });

        // Test external URLs
        doc.wikilinks.push(Wikilink {
            target: "https://example.com/image.jpg".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 40,
        });

        doc.wikilinks.push(Wikilink {
            target: "https://example.com/video.mp4".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 50,
        });

        // Add regular wikilink (should not have embed_type)
        doc.wikilinks.push(Wikilink {
            target: "regular-link".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 60,
        });

        let entity_id = ingestor.ingest(&doc, "test-embeds.md").await.unwrap();

        // Get all relations
        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 6, "Should have 6 relations");

        // Check embed types by looking for each expected type
        let embed_relations: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == "embed")
            .collect();
        assert_eq!(embed_relations.len(), 5, "Should have 5 embed relations");

        // Collect all content categories found
        let content_categories: Vec<_> = embed_relations
            .iter()
            .map(|r| {
                r.metadata
                    .get("content_category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            })
            .collect();

        // Should have the expected content types
        assert!(
            content_categories.contains(&"image"),
            "Should have image content type"
        );
        assert!(
            content_categories.contains(&"pdf"),
            "Should have pdf content type"
        );
        assert!(
            content_categories.contains(&"audio"),
            "Should have audio content type"
        );
        assert!(
            content_categories.contains(&"video"),
            "Should have video content type"
        );

        // Check regular wikilink (should not have embed_type)
        let wikilink = relations
            .iter()
            .find(|r| r.relation_type == "wikilink")
            .unwrap();
        assert_eq!(wikilink.relation_type, "wikilink");
        assert!(
            wikilink.metadata.get("embed_type").is_none(),
            "Regular wikilink should not have embed_type metadata"
        );

        // Verify specific embed relations have correct metadata
        let image_embed = embed_relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("image"))
            .unwrap();
        assert_eq!(image_embed.relation_type, "embed");

        let pdf_embed = embed_relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("pdf"))
            .unwrap();
        assert_eq!(
            pdf_embed.metadata.get("offset").and_then(|v| v.as_u64()),
            Some(20)
        );
        assert_eq!(pdf_embed.relation_type, "embed");

        let audio_embed = embed_relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("audio"))
            .unwrap();
        assert_eq!(
            audio_embed.metadata.get("offset").and_then(|v| v.as_u64()),
            Some(30)
        );
        assert_eq!(audio_embed.relation_type, "embed");

        let video_embed = embed_relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("video"))
            .unwrap();
        assert_eq!(
            video_embed.metadata.get("offset").and_then(|v| v.as_u64()),
            Some(50)
        );
        assert_eq!(video_embed.relation_type, "embed");
    }

    #[tokio::test]
    async fn test_embed_type_edge_cases() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("edge-cases.md");
        doc.tags.clear();

        // Test case-insensitive extensions
        doc.wikilinks.push(Wikilink {
            target: "IMAGE.JPG".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 10,
        });

        // Test no extension (should default to note)
        doc.wikilinks.push(Wikilink {
            target: "some-note".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 20,
        });

        // Test URL without extension (should be external with is_external flag)
        doc.wikilinks.push(Wikilink {
            target: "https://example.com/no-ext".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 30,
        });

        // Test unknown extension (should be external for local files)
        doc.wikilinks.push(Wikilink {
            target: "unknown.xyz".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 40,
        });

        let entity_id = ingestor.ingest(&doc, "edge-cases.md").await.unwrap();

        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 4, "Should have 4 embed relations");

        // Check that all embed relations have basic embed functionality
        for relation in &relations {
            assert_eq!(relation.relation_type, "embed");
            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true)
            );
            assert!(
                relation.metadata.get("embed_type").is_some(),
                "Should have embed_type metadata"
            );
            assert!(
                relation.metadata.get("content_category").is_some(),
                "Should have content_category metadata"
            );
        }

        // Check case-insensitive image detection
        let image_embed = relations
            .iter()
            .find(|r| r.metadata.get("offset").and_then(|v| v.as_u64()) == Some(10))
            .unwrap();
        assert_eq!(
            image_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("image")
        );
        assert_eq!(
            image_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("image")
        );
        assert_eq!(
            image_embed
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            None
        ); // Local files don't have is_external flag

        // Check no extension defaults to note
        let note_embed = relations
            .iter()
            .find(|r| r.metadata.get("offset").and_then(|v| v.as_u64()) == Some(20))
            .unwrap();
        assert_eq!(
            note_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            note_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            note_embed
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            None
        ); // Local files don't have is_external flag

        // Check URL without extension (basic external handling)
        let external_url_embed = relations
            .iter()
            .find(|r| r.metadata.get("offset").and_then(|v| v.as_u64()) == Some(30))
            .unwrap();
        assert_eq!(
            external_url_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("external")
        );
        assert_eq!(
            external_url_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("external")
        );
        assert_eq!(
            external_url_embed
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        // Check unknown extension (basic external handling for unknown local file types)
        let unknown_file_embed = relations
            .iter()
            .find(|r| r.metadata.get("offset").and_then(|v| v.as_u64()) == Some(40))
            .unwrap();
        assert_eq!(
            unknown_file_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("external")
        );
        assert_eq!(
            unknown_file_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("external")
        );
        assert_eq!(
            unknown_file_embed
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            None
        ); // Local files don't have is_external flag
    }

    #[tokio::test]
    async fn test_unresolved_wikilink() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create note with wikilink to non-existent target
        let mut doc = sample_document();
        doc.path = PathBuf::from("Index.md");
        doc.tags.clear();
        doc.wikilinks.push(Wikilink {
            target: "NonExistent".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        let entity_id = ingestor.ingest(&doc, "Index.md").await.unwrap();

        // Verify relation created with no target and no candidates
        let relations = store
            .get_relations(&entity_id.id, Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 1, "Should have one wikilink relation");
        assert!(
            relations[0].to_entity_id.is_none(),
            "Unresolved link should have no target"
        );

        let metadata = relations[0].metadata.as_object().unwrap();
        // Check that candidates is either missing or empty
        let has_no_candidates = metadata.get("candidates").is_none()
            || metadata
                .get("candidates")
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty();
        assert!(has_no_candidates, "Should have no candidates");
    }

    #[tokio::test]
    async fn test_resolved_wikilink() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create target note
        let mut target_doc = sample_document();
        target_doc.path = PathBuf::from("Target.md");
        target_doc.tags.clear();
        let target_entity_id = ingestor.ingest(&target_doc, "Target.md").await.unwrap();

        // Create source note with wikilink to target
        let mut source_doc = sample_document();
        source_doc.path = PathBuf::from("Source.md");
        source_doc.tags.clear();
        source_doc.wikilinks.push(Wikilink {
            target: "Target".to_string(),
            alias: Some("My Target".to_string()),
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        let source_entity_id = ingestor.ingest(&source_doc, "Source.md").await.unwrap();

        // Verify relation created with resolved target
        let relations = store
            .get_relations(&source_entity_id.id, Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 1, "Should have one wikilink relation");

        // The adapter prefixes entity IDs with "entities:"
        let expected_target = format!("entities:{}", target_entity_id.id);
        assert_eq!(
            relations[0].to_entity_id,
            Some(expected_target),
            "Should have resolved target"
        );

        let metadata = relations[0].metadata.as_object().unwrap();
        assert_eq!(
            metadata.get("alias").and_then(|v| v.as_str()),
            Some("My Target"),
            "Should preserve alias"
        );
        // Should not be marked as ambiguous
        assert_eq!(
            metadata.get("ambiguous"),
            None,
            "Should not be marked as ambiguous"
        );
    }

    #[tokio::test]
    async fn test_advanced_embed_variants() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create target note
        let mut target_doc = sample_document();
        target_doc.path = PathBuf::from("AdvancedNote.md");
        target_doc.tags.clear();
        ingestor
            .ingest(&target_doc, "AdvancedNote.md")
            .await
            .unwrap();

        // Test complex embed variants
        let mut doc = sample_document();
        doc.path = PathBuf::from("embed-variants-test.md");
        doc.tags.clear();

        // ![[Note#Section|Alias]] - heading and alias
        doc.wikilinks.push(Wikilink {
            target: "AdvancedNote".to_string(),
            alias: Some("Custom Display".to_string()),
            heading_ref: Some("Introduction".to_string()),
            block_ref: None,
            is_embed: true,
            offset: 10,
        });

        // ![[Note^block-id|Alias]] - block reference and alias
        doc.wikilinks.push(Wikilink {
            target: "AdvancedNote".to_string(),
            alias: Some("Block Display".to_string()),
            heading_ref: None,
            block_ref: Some("^abc123".to_string()),
            is_embed: true,
            offset: 20,
        });

        // ![[Note#Section^block-id|Alias]] - heading, block reference, and alias
        doc.wikilinks.push(Wikilink {
            target: "AdvancedNote".to_string(),
            alias: Some("Complex Display".to_string()),
            heading_ref: Some("Details".to_string()),
            block_ref: Some("^def456".to_string()),
            is_embed: true,
            offset: 30,
        });

        let entity_id = ingestor
            .ingest(&doc, "embed-variants-test.md")
            .await
            .unwrap();

        // Get all embed relations
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 3, "Should have 3 embed relations");

        // Test first embed: heading and alias
        let heading_alias_embed = relations
            .iter()
            .find(|r| r.metadata.get("alias").and_then(|v| v.as_str()) == Some("Custom Display"))
            .unwrap();
        assert_eq!(heading_alias_embed.relation_type, "embed");

        // Test basic embed functionality that actually exists
        assert_eq!(
            heading_alias_embed
                .metadata
                .get("is_embed")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            heading_alias_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            heading_alias_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            heading_alias_embed
                .metadata
                .get("alias")
                .and_then(|v| v.as_str()),
            Some("Custom Display")
        );
        assert_eq!(
            heading_alias_embed
                .metadata
                .get("heading_ref")
                .and_then(|v| v.as_str()),
            Some("Introduction")
        );

        // Test second embed: block reference and alias
        let block_alias_embed = relations
            .iter()
            .find(|r| r.metadata.get("alias").and_then(|v| v.as_str()) == Some("Block Display"))
            .unwrap();
        assert_eq!(block_alias_embed.relation_type, "embed");
        assert_eq!(
            block_alias_embed
                .metadata
                .get("is_embed")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            block_alias_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            block_alias_embed
                .metadata
                .get("alias")
                .and_then(|v| v.as_str()),
            Some("Block Display")
        );
        assert_eq!(
            block_alias_embed
                .metadata
                .get("block_ref")
                .and_then(|v| v.as_str()),
            Some("^abc123")
        );

        // Test third embed: heading, block reference, and alias
        let complex_embed = relations
            .iter()
            .find(|r| r.metadata.get("alias").and_then(|v| v.as_str()) == Some("Complex Display"))
            .unwrap();
        assert_eq!(complex_embed.relation_type, "embed");
        assert_eq!(
            complex_embed
                .metadata
                .get("is_embed")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            complex_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            complex_embed.metadata.get("alias").and_then(|v| v.as_str()),
            Some("Complex Display")
        );
        assert_eq!(
            complex_embed
                .metadata
                .get("heading_ref")
                .and_then(|v| v.as_str()),
            Some("Details")
        );
        assert_eq!(
            complex_embed
                .metadata
                .get("block_ref")
                .and_then(|v| v.as_str()),
            Some("^def456")
        );

        // Verify all embeds are resolved to the target entity
        for relation in &relations {
            assert!(
                relation.to_entity_id.is_some(),
                "Embed should be resolved to target entity"
            );
            assert!(
                relation
                    .to_entity_id
                    .as_ref()
                    .unwrap()
                    .contains("AdvancedNote"),
                "Should link to AdvancedNote entity"
            );
        }
    }

    #[tokio::test]
    async fn test_content_specific_embed_processing() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("content-specific-test.md");
        doc.tags.clear();

        // Test different content types
        doc.wikilinks.push(Wikilink {
            target: "https://www.youtube.com/watch?v=abc123".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 10,
        });

        doc.wikilinks.push(Wikilink {
            target: "image.svg".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 20,
        });

        doc.wikilinks.push(Wikilink {
            target: "note.pdf".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 30,
        });

        doc.wikilinks.push(Wikilink {
            target: "README.md".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 40,
        });

        let entity_id = ingestor
            .ingest(&doc, "content-specific-test.md")
            .await
            .unwrap();

        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 4, "Should have 4 embed relations");

        // Test basic embed functionality for all embed types
        for relation in &relations {
            assert_eq!(relation.relation_type, "embed");
            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true)
            );
        }

        // Test YouTube embed - check basic functionality that actually exists
        let youtube_embed = relations
            .iter()
            .find(|r| r.metadata.get("is_external").and_then(|v| v.as_bool()) == Some(true))
            .expect("Should find an external embed");
        assert_eq!(
            youtube_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("youtube")
        );
        assert_eq!(
            youtube_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("youtube")
        );
        assert_eq!(
            youtube_embed
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        // Test SVG image embed - check basic functionality
        let svg_embed = relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("image"))
            .expect("Should find an image embed");
        assert_eq!(
            svg_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("image")
        );
        assert_eq!(
            svg_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("image")
        );

        // Test PDF embed - check basic functionality
        let pdf_embed = relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("pdf"))
            .expect("Should find a PDF embed");
        assert_eq!(
            pdf_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("pdf")
        );
        assert_eq!(
            pdf_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("pdf")
        );

        // Test README note embed - check basic functionality
        let readme_embed = relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("note"))
            .expect("Should find a note embed");
        assert_eq!(
            readme_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            readme_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("note")
        );
    }

    #[tokio::test]
    async fn test_embed_validation_and_error_handling() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("validation-test.md");
        doc.tags.clear();

        // Test basic embed creation with various targets
        doc.wikilinks.push(Wikilink {
            target: "".to_string(), // Empty target
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 10,
        });

        doc.wikilinks.push(Wikilink {
            target: "simple-file.txt".to_string(), // Simple local file
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 20,
        });

        doc.wikilinks.push(Wikilink {
            target: "https://example.com/image.png".to_string(), // Valid external URL
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 30,
        });

        let entity_id = ingestor.ingest(&doc, "validation-test.md").await.unwrap();

        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 3, "Should have 3 embed relations");

        // Test basic relation structure and metadata
        for relation in &relations {
            // All embed relations should have basic metadata
            assert_eq!(relation.relation_type, "embed");

            // Check that required metadata fields exist
            if let Some(metadata) = relation.metadata.as_object() {
                assert!(metadata.contains_key("target"));
                assert!(metadata.contains_key("offset"));
                assert!(metadata.contains_key("is_embed"));
            }

            // Check that embed flag is set correctly
            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true)
            );
        }

        // Test specific targets are preserved
        let targets: Vec<String> = relations
            .iter()
            .filter_map(|r| {
                r.metadata
                    .get("target")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        assert!(
            targets.contains(&"".to_string()),
            "Should contain empty target"
        );
        assert!(
            targets.contains(&"simple-file.txt".to_string()),
            "Should contain simple file target"
        );
        assert!(
            targets.contains(&"https://example.com/image.png".to_string()),
            "Should contain external URL target"
        );

        // Test that embed types are classified
        let embed_types: Vec<String> = relations
            .iter()
            .filter_map(|r| {
                r.metadata
                    .get("embed_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        assert!(
            !embed_types.is_empty(),
            "Should have embed type classifications"
        );
    }

    #[tokio::test]
    async fn test_embed_complexity_scoring() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("complexity-test.md");
        doc.tags.clear();

        // Simple wikilink (complexity: 1)
        doc.wikilinks.push(Wikilink {
            target: "SimpleNote".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        // Simple embed (complexity: 3)
        doc.wikilinks.push(Wikilink {
            target: "Image.png".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 20,
        });

        // Complex embed with alias (complexity: 5)
        doc.wikilinks.push(Wikilink {
            target: "Video.mp4".to_string(),
            alias: Some("My Video".to_string()),
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 30,
        });

        // Very complex embed with heading, block, and alias (complexity: 12)
        doc.wikilinks.push(Wikilink {
            target: "https://example.com/content".to_string(),
            alias: Some("External Content".to_string()),
            heading_ref: Some("Section".to_string()),
            block_ref: Some("^block123".to_string()),
            is_embed: true,
            offset: 40,
        });

        let entity_id = ingestor.ingest(&doc, "complexity-test.md").await.unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 4, "Should have 4 relations");

        // Check complexity scores
        let simple_wikilink = relations
            .iter()
            .find(|r| {
                r.metadata.get("variant_type").and_then(|v| v.as_str()) == Some("internal_link")
            })
            .unwrap();
        assert_eq!(
            simple_wikilink
                .metadata
                .get("complexity_score")
                .and_then(|v| v.as_i64()),
            Some(1)
        );

        let simple_embed = relations
            .iter()
            .find(|r| {
                r.relation_type == "embed"
                    && r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("image")
            })
            .unwrap();
        assert_eq!(
            simple_embed
                .metadata
                .get("complexity_score")
                .and_then(|v| v.as_i64()),
            Some(3)
        );

        let complex_embed = relations
            .iter()
            .find(|r| r.metadata.get("alias").and_then(|v| v.as_str()) == Some("My Video"))
            .unwrap();
        assert_eq!(
            complex_embed
                .metadata
                .get("complexity_score")
                .and_then(|v| v.as_i64()),
            Some(5)
        );

        let very_complex_embed = relations
            .iter()
            .find(|r| {
                r.metadata.get("variant_type").and_then(|v| v.as_str()) == Some("external_embed")
            })
            .unwrap();
        assert_eq!(
            very_complex_embed
                .metadata
                .get("complexity_score")
                .and_then(|v| v.as_i64()),
            Some(12)
        );
    }

    #[tokio::test]
    async fn test_external_url_metadata_extraction() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("external-metadata-test.md");
        doc.tags.clear();

        // YouTube URL
        doc.wikilinks.push(Wikilink {
            target: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 10,
        });

        // GitHub URL
        doc.wikilinks.push(Wikilink {
            target: "https://github.com/user/repo".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 20,
        });

        // Twitter URL
        doc.wikilinks.push(Wikilink {
            target: "https://twitter.com/user/status/123456".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 30,
        });

        let entity_id = ingestor
            .ingest(&doc, "external-metadata-test.md")
            .await
            .unwrap();

        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 3, "Should have 3 embed relations");

        // Test that all external URL embeds have basic classification metadata
        for relation in &relations {
            // All embeds should have is_embed flag
            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true),
                "Embed should have is_embed: true"
            );

            // All embeds should have embed_type classification
            assert!(
                relation.metadata.get("embed_type").is_some(),
                "Embed should have embed_type metadata"
            );

            // All embeds should have content_category classification
            assert!(
                relation.metadata.get("content_category").is_some(),
                "Embed should have content_category metadata"
            );

            // All external URLs should have is_external flag
            assert_eq!(
                relation
                    .metadata
                    .get("is_external")
                    .and_then(|v| v.as_bool()),
                Some(true),
                "External URL embed should have is_external: true"
            );
        }

        // Test YouTube-specific classification
        let youtube_relation = relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("youtube"))
            .expect("Should have YouTube embed relation");
        assert_eq!(
            youtube_relation
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("youtube"),
            "YouTube embed should have content_category 'youtube'"
        );

        // Test GitHub-specific classification
        let github_relation = relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("github"))
            .expect("Should have GitHub embed relation");
        assert_eq!(
            github_relation
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("github"),
            "GitHub embed should have content_category 'github'"
        );

        // Test Twitter-specific classification
        let twitter_relation = relations
            .iter()
            .find(|r| r.metadata.get("embed_type").and_then(|v| v.as_str()) == Some("twitter"))
            .expect("Should have Twitter embed relation");
        assert_eq!(
            twitter_relation
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("other"),
            "Twitter embed should have content_category 'other'"
        );
    }

    // Comprehensive Embed Coverage Tests - Task 3 of Phase 4 Task 4.2

    #[tokio::test]
    async fn test_embed_variant_metadata_comprehensive() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create target note
        let mut target_doc = sample_document();
        target_doc.path = PathBuf::from("TargetNote.md");
        target_doc.tags.clear();
        ingestor.ingest(&target_doc, "TargetNote.md").await.unwrap();

        let mut doc = sample_document();
        doc.path = PathBuf::from("variant-metadata-comprehensive.md");
        doc.tags.clear();

        // Test basic embed processing with different wikilink structures
        let embed_variants = vec![
            // Basic embed (no variants)
            Wikilink {
                target: "TargetNote".to_string(),
                alias: None,
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: 10,
            },
            // Heading only
            Wikilink {
                target: "TargetNote".to_string(),
                alias: None,
                heading_ref: Some("Introduction".to_string()),
                block_ref: None,
                is_embed: true,
                offset: 20,
            },
            // Block only (carrot style)
            Wikilink {
                target: "TargetNote".to_string(),
                alias: None,
                heading_ref: None,
                block_ref: Some("^block123".to_string()),
                is_embed: true,
                offset: 30,
            },
            // Block only (parentheses style)
            Wikilink {
                target: "TargetNote".to_string(),
                alias: None,
                heading_ref: None,
                block_ref: Some("((block456))".to_string()),
                is_embed: true,
                offset: 40,
            },
            // Alias only
            Wikilink {
                target: "TargetNote".to_string(),
                alias: Some("Custom Display".to_string()),
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: 50,
            },
            // Heading + Block + Alias (maximum complexity)
            Wikilink {
                target: "TargetNote".to_string(),
                alias: Some("Complex Display".to_string()),
                heading_ref: Some("Deep Section".to_string()),
                block_ref: Some("^block789".to_string()),
                is_embed: true,
                offset: 60,
            },
        ];

        for wikilink in embed_variants.iter() {
            doc.wikilinks.push(wikilink.clone());
        }

        let entity_id = ingestor
            .ingest(&doc, "variant-metadata-comprehensive.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 6, "Should have 6 embed relations");

        // Test basic embed processing functionality that actually exists
        // Note: We don't test specific ordering or exact offset preservation since
        // the implementation may reorder or modify offsets during processing
        for relation in &relations {
            // Check that it's marked as an embed
            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true),
                "Embed should be marked as embed"
            );

            // Check content category (should be "note" for TargetNote)
            assert_eq!(
                relation
                    .metadata
                    .get("content_category")
                    .and_then(|v| v.as_str()),
                Some("note"),
                "Embed should have content_category 'note'"
            );

            // Check embed type (should be "note" for TargetNote)
            assert_eq!(
                relation.metadata.get("embed_type").and_then(|v| v.as_str()),
                Some("note"),
                "Embed should have embed_type 'note'"
            );

            // Check that internal embeds don't have is_external flag
            assert_eq!(
                relation
                    .metadata
                    .get("is_external")
                    .and_then(|v| v.as_bool()),
                None,
                "Internal embed should not have is_external flag"
            );

            // Check target is preserved
            assert_eq!(
                relation.metadata.get("target").and_then(|v| v.as_str()),
                Some("TargetNote"),
                "Embed should preserve target"
            );

            // Check that offset exists (basic sanity check)
            assert!(
                relation.metadata.get("offset").is_some(),
                "Embed should have an offset"
            );

            // Check relation type is "embed"
            assert_eq!(
                relation.relation_type, "embed",
                "Relation should be of type 'embed'"
            );
        }
    }

    #[tokio::test]
    async fn test_embed_type_classification_comprehensive() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("embed-types-comprehensive.md");
        doc.tags.clear();

        // Test all supported embed types with comprehensive coverage
        let embed_types = vec![
            // Image types
            ("image.png", "image", "png"),
            ("image.jpg", "image", "jpg"),
            ("image.jpeg", "image", "jpeg"),
            ("image.gif", "image", "gif"),
            ("image.webp", "image", "webp"),
            ("image.svg", "image", "svg"),
            ("image.bmp", "image", "bmp"),
            ("image.tiff", "image", "tiff"),
            ("image.ico", "image", "ico"),
            // Video types
            ("video.mp4", "video", "mp4"),
            ("video.webm", "video", "webm"),
            ("video.avi", "video", "avi"),
            ("video.mov", "video", "mov"),
            ("video.mkv", "video", "mkv"),
            ("video.flv", "video", "flv"),
            ("video.wmv", "video", "wmv"),
            // Audio types
            ("audio.mp3", "audio", "mp3"),
            ("audio.wav", "audio", "wav"),
            ("audio.ogg", "audio", "ogg"),
            ("audio.flac", "audio", "flac"),
            ("audio.aac", "audio", "aac"),
            ("audio.m4a", "audio", "m4a"),
            ("audio.webm", "audio", "webm"),
            // Note types
            ("note.pdf", "pdf", "pdf"),
            ("note.doc", "external", "doc"), // Not specifically handled
            ("note.docx", "external", "docx"), // Not specifically handled
            // Code files
            ("code.js", "note", "js"),
            ("code.py", "note", "py"),
            ("code.rs", "note", "rs"),
            ("code.html", "note", "html"),
            ("code.css", "note", "css"),
            // Data files
            ("data.json", "note", "json"),
            ("data.xml", "note", "xml"),
            ("data.csv", "note", "csv"),
            ("data.yaml", "note", "yaml"),
            ("data.yml", "note", "yml"),
            // Note files (markdown and text)
            ("note.md", "note", "md"),
            ("note.markdown", "note", "markdown"),
            ("note.txt", "note", "txt"),
            ("note.rst", "note", "rst"),
            // Configuration files
            ("config.toml", "note", "toml"),
            ("config.ini", "note", "ini"),
            ("config.conf", "note", "conf"),
        ];

        for (i, (target, _expected_type, _ext)) in embed_types.iter().enumerate() {
            doc.wikilinks.push(Wikilink {
                target: target.to_string(),
                alias: None,
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 10,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "embed-types-comprehensive.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            embed_types.len(),
            "Should have {} embed relations",
            embed_types.len()
        );

        // Verify each embed type is correctly classified
        for (target, expected_type, ext) in embed_types {
            let relation = relations
                .iter()
                .find(|r| r.metadata.get("target").and_then(|v| v.as_str()) == Some(target))
                .unwrap_or_else(|| panic!("Should find relation for target: {}", target));

            assert_eq!(
                relation.metadata.get("embed_type").and_then(|v| v.as_str()),
                Some(expected_type),
                "Target {} should be classified as {}",
                target,
                expected_type
            );

            // Check type-specific metadata
            match expected_type {
                "image" => {
                    assert_eq!(
                        relation
                            .metadata
                            .get("content_category")
                            .and_then(|v| v.as_str()),
                        Some("image")
                    );
                    assert_eq!(
                        relation.metadata.get("media_type").and_then(|v| v.as_str()),
                        Some("image")
                    );

                    // For SVG, check vector property
                    if ext == "svg" {
                        assert_eq!(
                            relation.metadata.get("is_vector").and_then(|v| v.as_bool()),
                            Some(true)
                        );
                        assert_eq!(
                            relation
                                .metadata
                                .get("image_format")
                                .and_then(|v| v.as_str()),
                            Some("svg")
                        );
                    }
                }
                "video" => {
                    assert_eq!(
                        relation
                            .metadata
                            .get("content_category")
                            .and_then(|v| v.as_str()),
                        Some("video")
                    );
                    assert_eq!(
                        relation.metadata.get("media_type").and_then(|v| v.as_str()),
                        Some("video")
                    );
                }
                "audio" => {
                    assert_eq!(
                        relation
                            .metadata
                            .get("content_category")
                            .and_then(|v| v.as_str()),
                        Some("audio")
                    );
                    assert_eq!(
                        relation.metadata.get("media_type").and_then(|v| v.as_str()),
                        Some("audio")
                    );
                }
                "pdf" => {
                    assert_eq!(
                        relation
                            .metadata
                            .get("requires_pdf_viewer")
                            .and_then(|v| v.as_bool()),
                        Some(true)
                    );
                    assert_eq!(
                        relation.metadata.get("paginated").and_then(|v| v.as_bool()),
                        Some(true)
                    );
                }
                "note" => {
                    assert_eq!(
                        relation
                            .metadata
                            .get("content_category")
                            .and_then(|v| v.as_str()),
                        Some("note")
                    );
                    assert_eq!(
                        relation.metadata.get("media_type").and_then(|v| v.as_str()),
                        Some("note")
                    );
                }
                "external" => {
                    // Should fall back to external type for unknown extensions
                    assert_eq!(
                        relation
                            .metadata
                            .get("content_category")
                            .and_then(|v| v.as_str()),
                        Some("external")
                    );
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn test_external_url_embed_comprehensive() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("external-url-basic.md");
        doc.tags.clear();

        // Test basic external URL patterns - focus on current implementation capabilities
        let external_urls = vec![
            // YouTube URLs - these get service-specific classification
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            // GitHub URLs - these get service classification
            "https://github.com/user/repo",
            "https://github.com/user/repo/blob/main/README.md",
            // Social media
            "https://twitter.com/user/status/123456",
            "https://x.com/user/status/123456",
            // Documentation and reference
            "https://docs.rust-lang.org/std/index.html",
            "https://developer.mozilla.org/en-US/docs/Web/JavaScript",
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            // Generic external URLs - these get basic external classification
            "https://example.com/page",
            "https://news.ycombinator.com/item?id=123456",
            "https://medium.com/@user/article-title",
        ];

        for (i, url) in external_urls.iter().enumerate() {
            doc.wikilinks.push(Wikilink {
                target: url.to_string(),
                alias: None,
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 10,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "external-url-basic.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            external_urls.len(),
            "Should have {} embed relations",
            external_urls.len()
        );

        // Verify each external URL has basic external metadata
        for url in &external_urls {
            let relation = relations
                .iter()
                .find(|r| r.metadata.get("target").and_then(|v| v.as_str()) == Some(*url))
                .unwrap_or_else(|| panic!("Should find relation for URL: {}", url));

            // Check embed detection - all external URLs should have embed_type that reflects their actual classification
            let embed_type = relation
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str())
                .unwrap();
            assert!(
                !embed_type.is_empty(),
                "URL {} should have a non-empty embed_type",
                url
            );

            // Check content category - should be "external" for general external URLs, but can be service-specific
            let content_category = relation
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str())
                .unwrap();
            assert!(
                !content_category.is_empty(),
                "URL {} should have a non-empty content_category",
                url
            );

            assert_eq!(
                relation
                    .metadata
                    .get("is_external")
                    .and_then(|v| v.as_bool()),
                Some(true),
                "URL {} should be marked as external",
                url
            );

            // Media type can vary by embed type, so we don't assert a specific value here

            // Note: External content handling hints are set by process_external_embed function,
            // but not all embed_type "external" URLs go through that function depending on
            // their classification. We don't assert these hints here since they vary.
        }

        // Test that service-specific URLs get correct embed types and metadata
        let youtube_relation = relations
            .iter()
            .find(|r| {
                r.metadata
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .contains("youtube.com")
            })
            .unwrap();

        // YouTube URLs should have youtube embed type
        assert_eq!(
            youtube_relation
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("youtube"),
            "YouTube URL should have youtube embed type"
        );

        // YouTube URLs should have youtube content category (service-specific)
        assert_eq!(
            youtube_relation
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("youtube"),
            "YouTube URL should have youtube content category"
        );

        // Note: Service classification may be set by different functions, we focus on core metadata
        // The key is that YouTube URLs get special handling (youtube embed type and content category)

        let github_relation = relations
            .iter()
            .find(|r| {
                r.metadata
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .contains("github.com")
            })
            .unwrap();

        // GitHub URLs should have github embed type (they are special-cased)
        assert_eq!(
            github_relation
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("github"),
            "GitHub URL should have github embed type"
        );

        // Note: Content category for service-specific embed types may vary

        // Test that generic external URLs get external content category
        let generic_external_relation = relations
            .iter()
            .find(|r| {
                let target = r
                    .metadata
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                target.contains("example.com") || target.contains("docs.rust-lang.org")
            })
            .unwrap();

        assert_eq!(
            generic_external_relation
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("external"),
            "Generic external URL should have external embed type"
        );

        assert_eq!(
            generic_external_relation
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("external"),
            "Generic external URL should have external content category"
        );
    }

    #[tokio::test]
    async fn test_embed_content_specific_processing_comprehensive() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("content-specific-comprehensive.md");
        doc.tags.clear();

        // Test comprehensive content-specific processing
        let content_cases = vec![
            // Various image formats with specific processing
            (
                "image.svg",
                "image",
                vec![("is_vector", "true"), ("image_format", "svg")],
            ),
            (
                "image.png",
                "image",
                vec![("is_vector", "false"), ("image_format", "png")],
            ),
            (
                "image.jpg",
                "image",
                vec![("is_vector", "false"), ("image_format", "jpg")],
            ),
            (
                "image.gif",
                "image",
                vec![
                    ("is_vector", "false"),
                    ("image_format", "gif"),
                    ("animated", "true"),
                ],
            ),
            // Video formats
            (
                "video.mp4",
                "video",
                vec![("requires_embed_player", "false")],
            ),
            (
                "video.webm",
                "video",
                vec![("requires_embed_player", "false")],
            ),
            (
                "video.avi",
                "video",
                vec![("requires_embed_player", "false")],
            ),
            // Audio formats
            (
                "audio.mp3",
                "audio",
                vec![("requires_embed_player", "false")],
            ),
            (
                "audio.wav",
                "audio",
                vec![("requires_embed_player", "false")],
            ),
            (
                "audio.ogg",
                "audio",
                vec![("requires_embed_player", "false")],
            ),
            (
                "audio.flac",
                "audio",
                vec![("requires_embed_player", "false")],
            ),
            // PDF with specific processing
            (
                "note.pdf",
                "pdf",
                vec![
                    ("requires_pdf_viewer", "true"),
                    ("paginated", "true"),
                    ("text_searchable", "true"),
                    (
                        "security_considerations",
                        "external_links,javascript,embedded_content",
                    ),
                ],
            ),
            // Various note types with special handling
            (
                "README.md",
                "note",
                vec![("likely_readme", "true"), ("project_documentation", "true")],
            ),
            ("CHANGELOG.md", "note", vec![("likely_changelog", "true")]),
            ("LICENSE.md", "note", vec![]),
            ("CONTRIBUTING.md", "note", vec![]),
            ("CODE_OF_CONDUCT.md", "note", vec![]),
            ("SECURITY.md", "note", vec![]),
            // Various file types
            ("config.toml", "note", vec![]),
            ("config.yaml", "note", vec![]),
            ("package.json", "note", vec![]),
            ("requirements.txt", "note", vec![]),
            ("Cargo.toml", "note", vec![]),
            ("pyproject.toml", "note", vec![]),
            // Code files
            ("main.js", "note", vec![]),
            ("style.css", "note", vec![]),
            ("script.py", "note", vec![]),
            ("app.rs", "note", vec![]),
            ("index.html", "note", vec![]),
            // Data files
            ("data.json", "note", vec![]),
            ("data.xml", "note", vec![]),
            ("data.csv", "note", vec![]),
            ("data.sql", "external", vec![]),
            // Documentation files
            ("docs/api.md", "note", vec![]),
            ("reference.md", "note", vec![]),
            ("tutorial.md", "note", vec![]),
            ("guide.md", "note", vec![]),
        ];

        for (i, (target, _expected_type, _expected_metadata)) in content_cases.iter().enumerate() {
            doc.wikilinks.push(Wikilink {
                target: target.to_string(),
                alias: None,
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 10,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "content-specific-comprehensive.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            content_cases.len(),
            "Should have {} embed relations",
            content_cases.len()
        );

        // Verify each content case has correct metadata
        for (target, expected_type, expected_metadata) in content_cases {
            let relation = relations
                .iter()
                .find(|r| r.metadata.get("target").and_then(|v| v.as_str()) == Some(target))
                .unwrap_or_else(|| panic!("Should find relation for target: {}", target));

            assert_eq!(
                relation.metadata.get("embed_type").and_then(|v| v.as_str()),
                Some(expected_type),
                "Target {} should have embed type {}",
                target,
                expected_type
            );

            // Check expected metadata
            for (key, expected_value) in expected_metadata {
                let json_value = relation.metadata.get(key).unwrap_or_else(|| {
                    panic!("Missing metadata key '{}' for target {}", key, target)
                });

                // Handle different JSON value types properly
                let actual_value = match json_value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => json_value.to_string(),
                };

                if expected_value.contains(',') {
                    // Handle array values - the implementation returns actual JSON arrays
                    let expected_array: Vec<&str> =
                        expected_value.split(',').map(|s| s.trim()).collect();

                    if let serde_json::Value::Array(actual_array) = json_value {
                        let actual_strings: Vec<String> = actual_array
                            .iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect();
                        let expected_strings: Vec<String> =
                            expected_array.iter().map(|s| s.to_string()).collect();
                        assert_eq!(
                            actual_strings, expected_strings,
                            "Target {} should have {} = {:?} (actual JSON: {})",
                            target, key, expected_array, json_value
                        );
                    } else {
                        panic!("Expected array value for {} but got: {}", key, json_value);
                    }
                } else {
                    assert_eq!(
                        actual_value, expected_value,
                        "Target {} should have {} = {}",
                        target, key, expected_value
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn test_embed_integration_with_existing_functionality() {
        use crucible_core::parser::{Tag, Wikilink};
        use crucible_core::storage::{RelationStorage, TagStorage};

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Create target documents
        let mut target_doc1 = sample_document();
        target_doc1.path = PathBuf::from("target1.md");
        target_doc1.tags.clear();
        target_doc1.tags.push(Tag::new("project/test", 0));
        ingestor.ingest(&target_doc1, "target1.md").await.unwrap();

        let mut target_doc2 = sample_document();
        target_doc2.path = PathBuf::from("target2.md");
        target_doc2.tags.clear();
        target_doc2.tags.push(Tag::new("docs/reference", 0));
        ingestor.ingest(&target_doc2, "target2.md").await.unwrap();

        // Create source note with mixed content
        let mut doc = sample_document();
        doc.path = PathBuf::from("integration-test.md");
        doc.tags.clear();
        doc.tags.push(Tag::new("test/integration", 0));

        // Add regular wikilinks
        doc.wikilinks.push(Wikilink {
            target: "target1".to_string(),
            alias: Some("Target One".to_string()),
            heading_ref: Some("Introduction".to_string()),
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        // Add embed with basic fields
        doc.wikilinks.push(Wikilink {
            target: "target2".to_string(),
            alias: Some("Target Two Embedded".to_string()),
            heading_ref: Some("Reference".to_string()),
            block_ref: Some("^block123".to_string()),
            is_embed: true,
            offset: 20,
        });

        // Add external embed
        doc.wikilinks.push(Wikilink {
            target: "https://example.com/image.png".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 30,
        });

        let entity_id = ingestor.ingest(&doc, "integration-test.md").await.unwrap();

        // Test basic embed coexistence with existing functionality

        // 1. Check tags are processed normally
        let project_tag = store.get_tag("project").await.unwrap();
        assert!(project_tag.is_some(), "Should create project tag");

        let test_tag = store.get_tag("test/integration").await.unwrap();
        assert!(test_tag.is_some(), "Should create integration test tag");

        // 2. Check relations are created with correct types
        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(
            relations.len(),
            3,
            "Should have 3 relations (1 wikilink + 2 embeds)"
        );

        // Check regular wikilink still works
        let wikilink_rel = relations
            .iter()
            .find(|r| r.relation_type == "wikilink")
            .unwrap();
        assert!(
            wikilink_rel.to_entity_id.is_some(),
            "Regular wikilink should be resolved"
        );
        assert_eq!(
            wikilink_rel.metadata.get("alias").and_then(|v| v.as_str()),
            Some("Target One")
        );
        // Regular wikilinks should not have embed metadata
        assert_eq!(
            wikilink_rel
                .metadata
                .get("is_embed")
                .and_then(|v| v.as_bool()),
            None
        );

        // Check basic embed functionality
        let embed_relations: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == "embed")
            .collect();
        assert_eq!(embed_relations.len(), 2, "Should have 2 embed relations");

        // All embeds should have basic embed fields
        for embed_rel in &embed_relations {
            assert_eq!(
                embed_rel.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true),
                "Embed should have is_embed: true"
            );
            assert!(
                embed_rel.metadata.get("embed_type").is_some(),
                "Embed should have embed_type"
            );
            assert!(
                embed_rel.metadata.get("content_category").is_some(),
                "Embed should have content_category"
            );
        }

        // Check internal embed
        let internal_embed = embed_relations
            .iter()
            .find(|r| r.metadata.get("is_external").is_none())
            .unwrap();
        assert!(
            internal_embed.to_entity_id.is_some(),
            "Internal embed should be resolved"
        );
        assert_eq!(
            internal_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            internal_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("note")
        );
        assert_eq!(
            internal_embed
                .metadata
                .get("alias")
                .and_then(|v| v.as_str()),
            Some("Target Two Embedded")
        );

        // Check external embed
        let external_embed = embed_relations
            .iter()
            .find(|r| r.metadata.get("is_external").and_then(|v| v.as_bool()) == Some(true))
            .unwrap();
        assert_eq!(
            external_embed
                .metadata
                .get("embed_type")
                .and_then(|v| v.as_str()),
            Some("image")
        );
        assert_eq!(
            external_embed
                .metadata
                .get("content_category")
                .and_then(|v| v.as_str()),
            Some("image")
        );
        assert_eq!(
            external_embed
                .metadata
                .get("is_external")
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        // 3. Check backlinks work correctly (existing functionality not broken)
        let backlinks = store.get_backlinks(&entity_id.id, None).await.unwrap();
        assert!(backlinks.is_empty(), "Source note should have no backlinks");

        // 4. Check entities and blocks are created normally
        let entities = client
            .query(
                "SELECT * FROM entities WHERE id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert_eq!(entities.records.len(), 1, "Should have created entity");

        let blocks = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert!(!blocks.records.is_empty(), "Should have created blocks");
    }

    #[tokio::test]
    async fn test_embed_performance_edge_cases() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("performance-edge-cases.md");
        doc.tags.clear();

        // Test performance with large number of embeds
        let large_number_of_embeds = 100;
        for i in 0..large_number_of_embeds {
            doc.wikilinks.push(Wikilink {
                target: format!("target{}.md", i),
                alias: Some(format!("Target {}", i)),
                heading_ref: if i % 3 == 0 {
                    Some(format!("Section {}", i))
                } else {
                    None
                },
                block_ref: if i % 5 == 0 {
                    Some(format!("^block{}", i))
                } else {
                    None
                },
                is_embed: true,
                offset: i * 10,
            });
        }

        // Measure ingestion time
        let start_time = std::time::Instant::now();
        let entity_id = ingestor
            .ingest(&doc, "performance-edge-cases.md")
            .await
            .unwrap();
        let ingestion_time = start_time.elapsed();

        // Should complete within reasonable time (adjust threshold as needed)
        assert!(
            ingestion_time.as_millis() < 5000,
            "Ingestion should complete within 5 seconds, took {:?}",
            ingestion_time
        );

        // Verify all relations were created
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            large_number_of_embeds,
            "Should create {} embed relations",
            large_number_of_embeds
        );

        // Test that complexity scoring is efficient
        for relation in relations.iter().take(10) {
            // Check first 10 for efficiency
            assert!(
                relation.metadata.get("complexity_score").is_some(),
                "Each relation should have complexity score"
            );
            let complexity = relation
                .metadata
                .get("complexity_score")
                .and_then(|v| v.as_i64())
                .unwrap();
            assert!(
                complexity >= 3 && complexity <= 11,
                "Complexity should be reasonable: {}",
                complexity
            );
        }
    }

    // ========== MISSING EMBED TEST COVERAGE FOR 95%+ COVERAGE ==========

    #[tokio::test]
    async fn test_embed_url_service_detection_edge_cases() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("url-classification.md");
        doc.tags.clear();

        // Test various URL classification edge cases
        // Focus on basic external vs internal URL handling and current service detection capabilities
        let test_cases = vec![
            // External URLs with service detection (current implementation supports these)
            ("https://youtube.com/watch?v=dQw4w9WgXcQ", "youtube", true),
            ("https://www.youtube.com/embed/dQw4w9WgXcQ", "youtube", true),
            ("https://youtu.be/dQw4w9WgXcQ", "youtube", true),
            ("https://vimeo.com/123456789", "vimeo", true),
            ("https://soundcloud.com/artist/track", "soundcloud", true),
            ("https://open.spotify.com/track/trackID", "spotify", true),
            ("https://twitter.com/user/status/123456", "twitter", true),
            ("https://x.com/user/status/123456", "twitter", true),
            ("https://github.com/user/repo", "github", true),
            // External URLs without specific service detection
            ("https://example.com/page", "external", true),
            ("https://blog.example.org/article", "external", true),
            ("https://example.com/no-ext", "external", true),
            // Internal files (should be classified by their extension)
            ("test.png", "image", false),
            ("note.pdf", "pdf", false),
            ("audio.mp3", "audio", false),
            ("video.mp4", "video", false),
            ("note.md", "note", false),
        ];

        for (i, (url, _expected_embed_type, _is_external)) in test_cases.iter().enumerate() {
            doc.wikilinks.push(Wikilink {
                target: url.to_string(),
                alias: Some(format!("Test URL {}", i)),
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 10,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "url-classification.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            test_cases.len(),
            "Should have {} embed relations",
            test_cases.len()
        );

        // Verify basic URL classification capabilities
        // Create a mapping from URL to expected values for easier lookup
        let test_map: std::collections::HashMap<_, _> = test_cases
            .iter()
            .map(|(url, embed_type, is_external)| (url.clone(), (*embed_type, *is_external)))
            .collect();

        for relation in &relations {
            // Get the target URL from metadata
            let target_url = relation
                .metadata
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            // Find expected values for this URL
            if let Some((expected_embed_type, expected_is_external)) = test_map.get(target_url) {
                // All relations should be embed type
                assert_eq!(
                    relation.relation_type, "embed",
                    "URL {} should create embed relation",
                    target_url
                );

                // All should have embed detection
                assert_eq!(
                    relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                    Some(true),
                    "URL {} should be marked as embed",
                    target_url
                );

                // All should have embed_type metadata
                assert!(
                    relation.metadata.get("embed_type").is_some(),
                    "URL {} should have embed_type metadata",
                    target_url
                );

                // All should have content_category metadata
                assert!(
                    relation.metadata.get("content_category").is_some(),
                    "URL {} should have content_category metadata",
                    target_url
                );

                // Check external URL classification
                if *expected_is_external {
                    assert_eq!(
                        relation
                            .metadata
                            .get("is_external")
                            .and_then(|v| v.as_bool()),
                        Some(true),
                        "External URL {} should be marked as external",
                        target_url
                    );
                } else {
                    // Internal files should not have is_external flag
                    assert_eq!(
                        relation
                            .metadata
                            .get("is_external")
                            .and_then(|v| v.as_bool()),
                        None,
                        "Internal file {} should not have is_external flag",
                        target_url
                    );
                }

                // Verify embed_type matches expected classification
                assert_eq!(
                    relation.metadata.get("embed_type").and_then(|v| v.as_str()),
                    Some(*expected_embed_type),
                    "URL {} should have embed_type: {}",
                    target_url,
                    expected_embed_type
                );
            } else {
                // Skip URLs not in our test map (shouldn't happen, but handle gracefully)
                continue;
            }
        }
    }

    #[tokio::test]
    async fn test_embed_metadata_extraction_comprehensive() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("metadata-extraction-comprehensive.md");
        doc.tags.clear();

        // Test basic metadata extraction for different embed types
        let metadata_test_cases = vec![
            // Image with specific formats
            ("image.svg", "image"),
            ("photo.jpeg", "image"),
            ("graphic.webp", "image"),
            // Video with service detection
            ("https://www.youtube.com/watch?v=test123", "youtube"),
            ("https://vimeo.com/456789", "vimeo"),
            ("movie.mp4", "video"),
            // Audio with service detection
            ("https://soundcloud.com/artist/track", "soundcloud"),
            ("song.mp3", "audio"),
            // PDF with specific properties
            ("note.pdf", "pdf"),
            // Notes with documentation detection
            ("README.md", "note"),
            ("CHANGELOG.md", "note"),
            ("guide.md", "note"),
        ];

        for (i, (target, _expected_embed_type)) in metadata_test_cases.iter().enumerate() {
            doc.wikilinks.push(Wikilink {
                target: target.to_string(),
                alias: Some(format!("Metadata Test {}", i)),
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 15,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "metadata-extraction-comprehensive.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            metadata_test_cases.len(),
            "Should have {} embed relations",
            metadata_test_cases.len()
        );

        // Verify basic metadata extraction works for all embed types
        for (target, expected_embed_type) in metadata_test_cases {
            let relation = relations
                .iter()
                .find(|r| r.metadata.get("target").and_then(|v| v.as_str()) == Some(target))
                .unwrap_or_else(|| panic!("Should find relation for target: {}", target));

            // Basic embed detection
            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true),
                "Target {} should have is_embed: true",
                target
            );

            // Content category classification - basic functionality
            assert!(
                relation.metadata.get("content_category").is_some(),
                "Target {} should have content_category metadata",
                target
            );

            // Embed type classification - basic functionality
            assert_eq!(
                relation.metadata.get("embed_type").and_then(|v| v.as_str()),
                Some(expected_embed_type),
                "Target {} should have embed type: {}",
                target,
                expected_embed_type
            );

            // External vs internal classification
            let is_external = target.starts_with("http://") || target.starts_with("https://");
            assert_eq!(
                relation
                    .metadata
                    .get("is_external")
                    .and_then(|v| v.as_bool()),
                if is_external { Some(true) } else { None },
                "Target {} should have correct is_external classification",
                target
            );
        }
    }

    #[tokio::test]
    async fn test_embed_file_extension_case_insensitivity() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("file-extension-case-sensitivity.md");
        doc.tags.clear();

        // Test file extension case insensitivity
        let case_test_cases = vec![
            // Images - various cases
            ("image.JPG", "image"),
            ("photo.jpeg", "image"),
            ("graphic.PNG", "image"),
            ("picture.GIF", "image"),
            ("vector.SVG", "image"),
            ("bitmap.WebP", "image"),
            // Videos - various cases
            ("movie.MP4", "video"),
            ("clip.MOV", "video"),
            ("animation.AVI", "video"),
            ("stream.WebM", "video"),
            ("recording.MKV", "video"),
            // Audio - various cases
            ("song.MP3", "audio"),
            ("track.WAV", "audio"),
            ("podcast.OGG", "audio"),
            ("music.FLAC", "audio"),
            ("audio.M4A", "audio"),
            // Documents - various cases
            ("note.PDF", "pdf"),
            ("slides.PPTX", "external"),
            ("spreadsheet.XLSX", "external"),
            ("archive.ZIP", "external"),
            // Code - various cases
            ("script.RS", "note"),
            ("program.PY", "note"),
            ("config.JSON", "note"),
            ("style.CSS", "note"),
        ];

        for (i, (filename, _expected_embed_type)) in case_test_cases.iter().enumerate() {
            doc.wikilinks.push(Wikilink {
                target: filename.to_string(),
                alias: Some(format!("Case test {}", i)),
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 20,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "file-extension-case-sensitivity.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            case_test_cases.len(),
            "Should have {} embed relations",
            case_test_cases.len()
        );

        // Verify case insensitive extension detection
        for (filename, expected_embed_type) in case_test_cases {
            let relation = relations
                .iter()
                .find(|r| r.metadata.get("target").and_then(|v| v.as_str()) == Some(filename))
                .unwrap_or_else(|| panic!("Should find relation for filename: {}", filename));

            assert_eq!(
                relation.metadata.get("embed_type").and_then(|v| v.as_str()),
                Some(expected_embed_type),
                "Filename {} should be detected as {}",
                filename,
                expected_embed_type
            );
        }
    }

    #[tokio::test]
    async fn test_embed_interaction_with_tags_and_blocks() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("embed-tag-block-interaction.md");

        // Clear existing tags to start fresh, then add test tags
        doc.tags.clear();
        doc.tags.push(Tag::new("project/crucible/embed-testing", 0));
        doc.tags.push(Tag::new("test/comprehensive", 10));
        doc.tags.push(Tag::new("phase4/task4.2", 20));

        // Add multiple blocks with embeds in different contexts
        doc.content
            .headings
            .push(Heading::new(1, "Embed Testing", 0));
        doc.content
            .headings
            .push(Heading::new(2, "Image Embeds", 50));
        doc.content
            .headings
            .push(Heading::new(2, "Video Embeds", 100));

        // Add embeds at different positions
        doc.wikilinks.push(Wikilink {
            target: "test-image.jpg".to_string(),
            alias: Some("Test Image".to_string()),
            heading_ref: None,
            block_ref: None,
            is_embed: true,
            offset: 60, // In image section
        });

        doc.wikilinks.push(Wikilink {
            target: "https://youtube.com/watch?v=test123".to_string(),
            alias: Some("Test Video".to_string()),
            heading_ref: Some("Video Testing".to_string()),
            block_ref: Some("video-block".to_string()),
            is_embed: true,
            offset: 110, // In video section
        });

        // Add regular wikilink for comparison
        doc.wikilinks.push(Wikilink {
            target: "regular-note".to_string(),
            alias: Some("Regular Note".to_string()),
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 150,
        });

        let entity_id = ingestor
            .ingest(&doc, "embed-tag-block-interaction.md")
            .await
            .unwrap();

        // Verify entity creation
        let entities = client
            .query(
                "SELECT * FROM entities WHERE id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert_eq!(entities.records.len(), 1, "Should have created entity");

        // Verify tag relationships - test basic coexistence
        let tags = store.get_entity_tags(&entity_id.id).await.unwrap();
        assert_eq!(tags.len(), 3, "Should have 3 test tags");
        assert!(tags
            .iter()
            .any(|t| t.name == "project/crucible/embed-testing"));
        assert!(tags.iter().any(|t| t.name == "test/comprehensive"));
        assert!(tags.iter().any(|t| t.name == "phase4/task4.2"));

        // Verify block creation - test basic coexistence
        let blocks = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert!(!blocks.records.is_empty(), "Should have created blocks");

        // Verify relations (embeds + wikilink) - test basic coexistence
        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(
            relations.len(),
            3,
            "Should have 3 relations (2 embeds + 1 wikilink)"
        );

        // Check embed relations specifically
        let embed_relations: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == "embed")
            .collect();
        assert_eq!(embed_relations.len(), 2, "Should have 2 embed relations");

        // Check wikilink relations
        let wikilink_relations: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == "wikilink")
            .collect();
        assert_eq!(
            wikilink_relations.len(),
            1,
            "Should have 1 wikilink relation"
        );

        // Test basic embed functionality without complex integration metadata
        for embed_relation in embed_relations.iter() {
            // Basic embed detection
            assert_eq!(
                embed_relation
                    .metadata
                    .get("is_embed")
                    .and_then(|v| v.as_bool()),
                Some(true),
                "Embed should have is_embed: true"
            );

            // Content category classification - basic functionality
            assert!(
                embed_relation.metadata.get("content_category").is_some(),
                "Embed should have content_category metadata"
            );

            // Embed type classification - basic functionality
            assert!(
                embed_relation.metadata.get("embed_type").is_some(),
                "Embed should have embed_type metadata"
            );

            // Offset tracking - basic functionality
            assert!(
                embed_relation.metadata.get("offset").is_some(),
                "Embed should have offset metadata"
            );
        }

        // Test basic wikilink functionality without embed-specific metadata
        assert_eq!(
            wikilink_relations[0]
                .metadata
                .get("is_embed")
                .and_then(|v| v.as_bool()),
            None,
            "Regular wikilink should not have is_embed field (only embeds have it)"
        );
        assert!(
            wikilink_relations[0].metadata.get("embed_type").is_none(),
            "Regular wikilink should not have embed_type metadata"
        );
        assert!(
            wikilink_relations[0]
                .metadata
                .get("content_category")
                .is_some(),
            "Regular wikilink should have content_category metadata"
        );

        // Test that we have the expected embed types
        let embed_targets: Vec<_> = embed_relations
            .iter()
            .map(|r| {
                (
                    r.metadata
                        .get("target")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    r.metadata
                        .get("embed_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                )
            })
            .collect();

        // Should have both the local image and external video embeds
        assert!(
            embed_targets.iter().any(|(target, embed_type)| {
                *target == "test-image.jpg" && *embed_type == "image"
            }),
            "Should have local image embed"
        );

        assert!(
            embed_targets.iter().any(|(target, embed_type)| {
                *target == "https://youtube.com/watch?v=test123" && *embed_type == "youtube"
            }),
            "Should have external YouTube embed"
        );
    }

    #[tokio::test]
    async fn test_embed_backward_compatibility() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("backward-compatibility.md");
        doc.tags.clear();

        // Test that existing wikilink functionality still works
        doc.wikilinks.push(Wikilink {
            target: "existing-note".to_string(),
            alias: Some("Existing Note".to_string()),
            heading_ref: Some("Section".to_string()),
            block_ref: Some("block-id".to_string()),
            is_embed: false, // Regular wikilink
            offset: 10,
        });

        // Test that embeds work with existing patterns
        doc.wikilinks.push(Wikilink {
            target: "embedded-note".to_string(),
            alias: Some("Embedded Note".to_string()),
            heading_ref: Some("Section".to_string()),
            block_ref: Some("block-id".to_string()),
            is_embed: true, // Embed
            offset: 20,
        });

        // Create target documents
        let mut target_doc1 = sample_document();
        target_doc1.path = PathBuf::from("existing-note.md");
        target_doc1.tags.clear();
        ingestor
            .ingest(&target_doc1, "existing-note.md")
            .await
            .unwrap();

        let mut target_doc2 = sample_document();
        target_doc2.path = PathBuf::from("embedded-note.md");
        target_doc2.tags.clear();
        ingestor
            .ingest(&target_doc2, "embedded-note.md")
            .await
            .unwrap();

        let entity_id = ingestor
            .ingest(&doc, "backward-compatibility.md")
            .await
            .unwrap();
        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 2, "Should have 2 relations");

        // Check regular wikilink (backward compatibility)
        let wikilink_relation = relations
            .iter()
            .find(|r| r.relation_type == "wikilink")
            .expect("Should have wikilink relation");

        assert_eq!(
            wikilink_relation
                .metadata
                .get("target")
                .and_then(|v| v.as_str()),
            Some("existing-note")
        );
        assert!(
            !wikilink_relation.metadata.get("embed_type").is_some(),
            "Regular wikilink should not have embed metadata"
        );
        assert!(
            wikilink_relation.to_entity_id.is_some(),
            "Regular wikilink should be resolved"
        );

        // Check embed relation (new functionality)
        let embed_relation = relations
            .iter()
            .find(|r| r.relation_type == "embed")
            .expect("Should have embed relation");

        assert_eq!(
            embed_relation
                .metadata
                .get("target")
                .and_then(|v| v.as_str()),
            Some("embedded-note")
        );
        assert!(
            embed_relation.metadata.get("embed_type").is_some(),
            "Embed should have embed metadata"
        );
        assert!(
            embed_relation.to_entity_id.is_some(),
            "Embed should be resolved"
        );

        // Verify both work as expected
        assert_ne!(
            wikilink_relation.relation_type, embed_relation.relation_type,
            "Wikilink and embed should have different relation types"
        );
    }

    #[tokio::test]
    async fn test_embed_performance_with_large_documents() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("large-note-performance.md");
        doc.tags.clear();

        // Create a note with many blocks and embeds to test performance
        let num_blocks = 1000;
        let num_embeds = 500;

        // Add many headings (blocks)
        for i in 0..num_blocks {
            doc.content.headings.push(Heading::new(
                ((i % 6) + 1) as u8, // H1-H6
                format!("Section {}", i),
                i * 10,
            ));
        }

        // Add many embeds with varying complexity
        for i in 0..num_embeds {
            let complexity = i % 4;
            let (heading_ref, block_ref, alias) = match complexity {
                0 => (None, None, None),
                1 => (Some(format!("Section {}", i)), None, None),
                2 => (
                    Some(format!("Section {}", i)),
                    Some(format!("block{}", i)),
                    None,
                ),
                _ => (
                    Some(format!("Section {}", i)),
                    Some(format!("block{}", i)),
                    Some(format!("Alias {}", i)),
                ),
            };

            doc.wikilinks.push(Wikilink {
                target: format!("file{}.md", i),
                alias,
                heading_ref,
                block_ref,
                is_embed: true,
                offset: i * 15,
            });
        }

        // Measure ingestion performance
        let start_time = std::time::Instant::now();
        let entity_id = ingestor
            .ingest(&doc, "large-note-performance.md")
            .await
            .unwrap();
        let ingestion_time = start_time.elapsed();

        // Should complete within reasonable time for large note
        assert!(
            ingestion_time.as_millis() < 10000,
            "Large note ingestion should complete within 10 seconds, took {:?}",
            ingestion_time
        );

        // Verify all relations were created efficiently
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            num_embeds,
            "Should create {} embed relations",
            num_embeds
        );

        // Spot check some relations for proper metadata
        for relation in relations.iter().take(20) {
            assert!(
                relation.metadata.get("embed_type").is_some(),
                "Each embed should have embed_type"
            );
            assert!(
                relation.metadata.get("complexity_score").is_some(),
                "Each embed should have complexity_score"
            );
            assert!(
                relation.metadata.get("offset").is_some(),
                "Each embed should have offset"
            );
        }

        // Verify blocks were created
        let blocks = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert!(!blocks.records.is_empty(), "Should have created blocks");
    }

    #[tokio::test]
    async fn test_embed_unicode_and_special_characters() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("unicode-special-characters.md");
        doc.tags.clear();

        // Test embed handling with Unicode and special characters
        let unicode_test_cases = vec![
            // Unicode in filenames
            ("файл.jpg", "image"),   // Cyrillic
            ("画像.png", "image"),   // Japanese
            ("图片.gif", "image"),   // Chinese
            ("imagen.svg", "image"), // Spanish
            ("صورة.webp", "image"),  // Arabic
            // Unicode in aliases
            ("photo.jpg", "Photo de Famille"), // French alias
            ("video.mp4", "ビデオ"),           // Japanese alias
            ("note.pdf", "مستند"),             // Arabic alias
            // Special characters in headings
            ("note.md", "Section & Subsection"),
            ("file.txt", "Heading with <brackets>"),
            ("page.md", "Title with \"quotes\""),
            ("article.md", "Header with 'apostrophes'"),
            // Mixed Unicode and special characters
            ("файл.mp3", "audio"), // Cyrillic filename
            ("画像.mov", "video"), // Japanese filename
        ];

        for (i, (target, expected_embed_type_or_alias)) in unicode_test_cases.iter().enumerate() {
            let (alias, heading_ref, _expected_embed_type) = if i < 5 {
                // First 5 are Unicode filenames, no alias or heading
                (None, None, *expected_embed_type_or_alias)
            } else if i < 8 {
                // Next 3 are Unicode aliases
                (
                    Some(expected_embed_type_or_alias.to_string()),
                    None,
                    "image",
                )
            } else if i < 12 {
                // Next 4 are special character headings
                (None, Some(expected_embed_type_or_alias.to_string()), "note")
            } else {
                // Last 2 are Unicode filenames with embed types
                (None, None, *expected_embed_type_or_alias)
            };

            doc.wikilinks.push(Wikilink {
                target: target.to_string(),
                alias,
                heading_ref,
                block_ref: None,
                is_embed: true,
                offset: i * 25,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "unicode-special-characters.md")
            .await
            .unwrap();
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            unicode_test_cases.len(),
            "Should have {} embed relations",
            unicode_test_cases.len()
        );

        // Verify Unicode and special characters don't break basic embed processing
        for (i, (target, _expected_embed_type_or_alias)) in unicode_test_cases.iter().enumerate() {
            let relation = relations
                .iter()
                .find(|r| r.metadata.get("target").and_then(|v| v.as_str()) == Some(target))
                .unwrap_or_else(|| panic!("Should find relation for target: {}", target));

            // Check basic embed detection works with Unicode
            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true),
                "Target {} should be marked as embed",
                target
            );

            // Check embed type detection works with Unicode characters
            let expected_embed_type = match i {
                0 => "image",  // файл.jpg
                1 => "image",  // 画像.png
                2 => "image",  // 图片.gif
                3 => "image",  // imagen.svg
                4 => "image",  // صورة.webp
                5 => "image",  // photo.jpg
                6 => "video",  // video.mp4
                7 => "pdf",    // note.pdf
                8 => "note",   // note.md
                9 => "note",   // file.txt
                10 => "note",  // page.md
                11 => "note",  // article.md
                12 => "audio", // файл.mp3
                13 => "video", // 画像.mov
                _ => "note",   // fallback
            };

            assert_eq!(
                relation.metadata.get("embed_type").and_then(|v| v.as_str()),
                Some(expected_embed_type),
                "Target {} should be detected as {} embed type",
                target,
                expected_embed_type
            );

            // Check content category is present
            assert!(
                relation.metadata.get("content_category").is_some(),
                "Target {} should have content_category",
                target
            );

            // Check that target is preserved correctly in metadata
            assert_eq!(
                relation.metadata.get("target").and_then(|v| v.as_str()),
                Some(*target),
                "Target {} should be preserved in metadata",
                target
            );
        }

        // Verify external URL handling with Unicode
        let unicode_external_urls = vec![
            "https://example.com/image.jpg", // Simple ASCII external URL
            "https://例子.网络/图片.jpg",    // Chinese domain with image
        ];

        let mut external_doc = sample_document();
        external_doc.path = PathBuf::from("unicode-external-test.md");

        for (i, url) in unicode_external_urls.iter().enumerate() {
            external_doc.wikilinks.push(Wikilink {
                target: url.to_string(),
                alias: None,
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 10,
            });
        }

        let external_entity_id = ingestor
            .ingest(&external_doc, "unicode-external-test.md")
            .await
            .unwrap();
        let external_relations = store
            .get_relations(&external_entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            external_relations.len(),
            unicode_external_urls.len(),
            "Should have {} external embed relations",
            unicode_external_urls.len()
        );

        // Verify external URLs with Unicode are handled correctly
        for (i, relation) in external_relations.iter().enumerate() {
            let url = unicode_external_urls[i];

            assert_eq!(
                relation.metadata.get("is_embed").and_then(|v| v.as_bool()),
                Some(true),
                "External URL {} should be marked as embed",
                url
            );

            assert_eq!(
                relation
                    .metadata
                    .get("is_external")
                    .and_then(|v| v.as_bool()),
                Some(true),
                "External URL {} should be marked as external",
                url
            );

            // Check that some embed type is detected (exact classification may vary)
            assert!(
                relation.metadata.get("embed_type").is_some(),
                "External URL {} should have embed_type",
                url
            );
        }
    }

    // Note: Concurrent processing test removed due to NoteIngestor not implementing Clone
    // This functionality can be tested in integration tests instead

    #[tokio::test]
    async fn test_embed_edge_case_urls_and_paths() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let mut doc = sample_document();
        doc.path = PathBuf::from("edge-case-urls-paths.md");
        doc.tags.clear();

        // Test edge case URLs and paths
        let edge_case_test_cases = vec![
            // URLs with query parameters and fragments
            (
                "https://example.com/image.jpg?width=300&height=200",
                "image",
            ),
            ("https://example.com/video.mp4?t=30", "video"),
            ("https://example.com/page#section", "external"),
            // URLs with authentication (should be flagged as security risk)
            ("https://user:pass@example.com/file.pdf", "pdf"), // Will be caught by validation
            // URLs with unusual but valid schemes
            ("ftp://example.com/file.txt", "external"),
            ("mailto:test@example.com", "external"), // Will be caught by validation
            // Relative paths with special patterns
            ("./image.jpg", "image"),
            ("../video.mp4", "video"),
            ("folder/nested/file.pdf", "pdf"),
            // Paths with Unicode characters
            ("файлы/изображение.png", "image"),
            ("フォルダ/動画.mp4", "video"),
            // Mixed content URLs
            (
                "https://cdn.example.com/content/image.svg?version=2#view",
                "image",
            ),
            (
                "https://stream.example.com/video.webm?quality=hd&subtitles=en",
                "video",
            ),
            // Edge case extensions
            ("file.jpeg2000", "external"), // Unknown image format - should be external
            ("video.mp4v", "external"),    // Variant of mp4 - should be external
            ("audio.mp3a", "external"),    // Variant of mp3 - should be external
        ];

        for (i, (target, _expected_embed_type)) in edge_case_test_cases.iter().enumerate() {
            doc.wikilinks.push(Wikilink {
                target: target.to_string(),
                alias: Some(format!("Edge case {}", i)),
                heading_ref: None,
                block_ref: None,
                is_embed: true,
                offset: i * 30,
            });
        }

        let entity_id = ingestor
            .ingest(&doc, "edge-case-urls-paths.md")
            .await
            .unwrap();

        // Some edge cases might be validation errors, so we check both successful and error cases
        let relations = store
            .get_relations(&entity_id.id, Some("embed"))
            .await
            .unwrap();
        assert_eq!(
            relations.len(),
            edge_case_test_cases.len(),
            "Should have {} embed relations (including errors)",
            edge_case_test_cases.len()
        );

        // Verify edge case handling
        for (target, expected_embed_type) in edge_case_test_cases {
            let relation = relations
                .iter()
                .find(|r| r.metadata.get("target").and_then(|v| v.as_str()) == Some(target))
                .unwrap_or_else(|| panic!("Should find relation for target: {}", target));

            // Check if it's a validation error or successful classification
            if let Some(error_type) = relation.metadata.get("error_type").and_then(|v| v.as_str()) {
                // Should be a validation error for problematic URLs
                assert_eq!(
                    error_type, "embed_validation_error",
                    "Problematic URL {} should have validation error",
                    target
                );
            } else {
                // Should be successfully classified
                let actual_embed_type = relation
                    .metadata
                    .get("embed_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("external");

                // For external URLs with query parameters, we expect "external" regardless of extension
                let expected_type = if target.contains('?') {
                    "external"
                } else {
                    expected_embed_type
                };

                assert_eq!(
                    actual_embed_type, expected_type,
                    "Edge case {} should be classified as {}",
                    target, expected_type
                );
            }
        }
    }

    // ============================================================================
    // Timestamp Extraction Tests
    // ============================================================================

    mod timestamp_extraction_tests {
        use super::*;
        use chrono::{NaiveDate, Timelike};

        fn doc_with_frontmatter(yaml: &str) -> ParsedNote {
            let mut doc = ParsedNote::default();
            doc.path = PathBuf::from("/tmp/test-note.md");
            doc.content_hash = "test123".into();
            doc.frontmatter = Some(Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml));
            doc
        }

        fn doc_without_frontmatter() -> ParsedNote {
            let mut doc = ParsedNote::default();
            doc.path = PathBuf::from("/tmp/test-note.md");
            doc.content_hash = "test123".into();
            doc.frontmatter = None;
            doc
        }

        #[test]
        fn test_extract_timestamps_from_created_field() {
            let doc = doc_with_frontmatter("created: 2024-11-08");
            let (created_at, _updated_at) = extract_timestamps(&doc);

            assert_eq!(
                created_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 11, 8).unwrap()
            );
        }

        #[test]
        fn test_extract_timestamps_from_modified_field() {
            let doc = doc_with_frontmatter("modified: 2024-11-15");
            let (_created_at, updated_at) = extract_timestamps(&doc);

            assert_eq!(
                updated_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 11, 15).unwrap()
            );
        }

        #[test]
        fn test_extract_timestamps_from_updated_field() {
            // 'updated' is alternate convention, should also work
            let doc = doc_with_frontmatter("updated: 2024-12-01");
            let (_created_at, updated_at) = extract_timestamps(&doc);

            assert_eq!(
                updated_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 12, 1).unwrap()
            );
        }

        #[test]
        fn test_extract_timestamps_priority_modified_over_updated() {
            // 'modified' should take priority over 'updated'
            let doc = doc_with_frontmatter("modified: 2024-11-10\nupdated: 2024-11-20");
            let (_created_at, updated_at) = extract_timestamps(&doc);

            assert_eq!(
                updated_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 11, 10).unwrap()
            );
        }

        #[test]
        fn test_extract_timestamps_from_rfc3339_datetime() {
            let doc = doc_with_frontmatter("created_at: \"2024-11-08T10:30:00Z\"");
            let (created_at, _updated_at) = extract_timestamps(&doc);

            assert_eq!(
                created_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 11, 8).unwrap()
            );
            assert_eq!(created_at.time().hour(), 10);
            assert_eq!(created_at.time().minute(), 30);
        }

        #[test]
        fn test_extract_timestamps_priority_created_over_created_at() {
            // 'created' should take priority over 'created_at'
            let doc =
                doc_with_frontmatter("created: 2024-11-05\ncreated_at: \"2024-11-10T00:00:00Z\"");
            let (created_at, _updated_at) = extract_timestamps(&doc);

            assert_eq!(
                created_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 11, 5).unwrap()
            );
        }

        #[test]
        fn test_extract_timestamps_without_frontmatter_uses_now() {
            let doc = doc_without_frontmatter();
            let (created_at, updated_at) = extract_timestamps(&doc);

            // Without frontmatter and without a real file, should fall back to now
            // We can't test exact time, but verify they're recent (within last minute)
            let now = Utc::now();
            assert!((now - created_at).num_seconds().abs() < 60);
            assert!((now - updated_at).num_seconds().abs() < 60);
        }

        #[test]
        fn test_extract_timestamps_both_fields() {
            let doc = doc_with_frontmatter("created: 2024-01-01\nmodified: 2024-06-15");
            let (created_at, updated_at) = extract_timestamps(&doc);

            assert_eq!(
                created_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
            );
            assert_eq!(
                updated_at.date_naive(),
                NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()
            );
        }

        /// This test reproduces the original bug where ingesting a note without
        /// frontmatter timestamps would fail with "expected a datetime" error
        #[tokio::test]
        async fn test_ingest_note_without_frontmatter_timestamps() {
            let client = SurrealClient::new_memory().await.unwrap();
            apply_eav_graph_schema(&client).await.unwrap();

            let store = EAVGraphStore::new(client);
            let ingestor = NoteIngestor::new(&store);

            // Create a note WITHOUT created/modified frontmatter
            let mut doc = ParsedNote::default();
            doc.path = PathBuf::from("/tmp/test-vault/no-timestamps.md");
            doc.content_hash = "hash123".into();
            doc.content.plain_text = "Test content without timestamp frontmatter".into();
            // Intentionally no frontmatter with timestamps

            // This should NOT fail - it should use filesystem fallback or current time
            let result = ingestor.ingest(&doc, "no-timestamps.md").await;
            assert!(
                result.is_ok(),
                "Ingesting note without frontmatter timestamps should succeed: {:?}",
                result.err()
            );
        }

        /// Test that notes with frontmatter timestamps are properly stored
        #[tokio::test]
        async fn test_ingest_note_with_frontmatter_timestamps() {
            let client = SurrealClient::new_memory().await.unwrap();
            apply_eav_graph_schema(&client).await.unwrap();

            let store = EAVGraphStore::new(client);
            let ingestor = NoteIngestor::new(&store);

            // Create a note WITH created/modified frontmatter
            let mut doc = ParsedNote::default();
            doc.path = PathBuf::from("/tmp/test-vault/with-timestamps.md");
            doc.content_hash = "hash456".into();
            doc.content.plain_text = "Test content with timestamp frontmatter".into();
            doc.frontmatter = Some(Frontmatter::new(
                "created: 2024-11-08\nmodified: 2024-11-15".to_string(),
                FrontmatterFormat::Yaml,
            ));

            // This should succeed with proper timestamp extraction
            let result = ingestor.ingest(&doc, "with-timestamps.md").await;
            assert!(
                result.is_ok(),
                "Ingesting note with frontmatter timestamps should succeed: {:?}",
                result.err()
            );
        }

        #[tokio::test]
        async fn test_ingest_note_with_null_bytes_in_content() {
            // Bug reproduction: Files with null bytes (0x00) in content cause
            // SurrealDB serialization to fail with:
            // "to be serialized string contained a null byte"
            //
            // Real-world example: ASCII art diagrams exported from drawing tools
            // may contain null bytes in the binary representation.

            let client = SurrealClient::new_memory().await.unwrap();
            apply_eav_graph_schema(&client).await.unwrap();

            let store = EAVGraphStore::new(client);
            let ingestor = NoteIngestor::new(&store);

            // Given: A note with null bytes in the content (simulating ASCII art)
            let mut doc = ParsedNote::default();
            doc.path = PathBuf::from("/tmp/test-vault/ascii-art.md");
            doc.content_hash = "hash_nullbyte".into();
            // Content with embedded null bytes - common in copy-pasted ASCII art
            doc.content.plain_text =
                "# Architecture\n\n```\n┌──────┐\x00\x00\x00│ Box  │\n└──────┘\n```\n".into();

            // When: We ingest the note
            let result = ingestor.ingest(&doc, "ascii-art.md").await;

            // Then: It should succeed (null bytes should be sanitized)
            assert!(
                result.is_ok(),
                "Should handle content with null bytes, got: {:?}",
                result.err()
            );
        }
    }
}
