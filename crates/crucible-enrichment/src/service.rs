//! Enrichment Service
//!
//! Orchestrates all enrichment operations including embedding generation,
//! metadata extraction, and relation inference. Follows clean architecture
//! principles with dependency injection.

use crate::types::{BlockEmbedding, EnrichmentMetadata, InferredRelation};
use anyhow::Result;
use async_trait::async_trait;
use crucible_core::enrichment::{EmbeddingProvider, EnrichedNote};
use crucible_core::events::{SessionEvent, SharedEventBus};
use crucible_core::ParsedNote;
use std::sync::Arc;
use tracing::{debug, info};

/// Default minimum word count for generating embeddings
pub const DEFAULT_MIN_WORDS_FOR_EMBEDDING: usize = 5;

/// Default maximum batch size for embedding generation
pub const DEFAULT_MAX_BATCH_SIZE: usize = 10;

/// Service that orchestrates all enrichment operations
///
/// Receives a ParsedNote and list of changed blocks, then coordinates:
/// - Embedding generation (only for changed blocks)
/// - Metadata extraction (word counts, language, etc.)
/// - Relation inference (semantic similarity, clustering)
///
/// Returns an EnrichedNote ready for storage.
pub struct DefaultEnrichmentService {
    /// Embedding provider (dependency injected)
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,

    /// Minimum word count for embedding generation
    min_words_for_embedding: usize,

    /// Maximum blocks to embed in a single batch (prevents memory issues)
    max_batch_size: usize,

    /// Event emitter for SessionEvent emission (EmbeddingBatchComplete, etc.)
    emitter: Option<SharedEventBus<SessionEvent>>,
}

impl DefaultEnrichmentService {
    /// Create a new enrichment service with an embedding provider
    pub fn new(embedding_provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            embedding_provider: Some(embedding_provider),
            min_words_for_embedding: DEFAULT_MIN_WORDS_FOR_EMBEDDING,
            max_batch_size: DEFAULT_MAX_BATCH_SIZE,
            emitter: None,
        }
    }

    /// Create an enrichment service without embeddings (metadata/relations only)
    pub fn without_embeddings() -> Self {
        Self {
            embedding_provider: None,
            min_words_for_embedding: DEFAULT_MIN_WORDS_FOR_EMBEDDING,
            max_batch_size: DEFAULT_MAX_BATCH_SIZE,
            emitter: None,
        }
    }

    /// Set the minimum word count for embedding generation (builder pattern)
    #[allow(dead_code)] // TODO: Add issue - Configurable embedding thresholds
    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.min_words_for_embedding = min_words;
        self
    }

    /// Set the maximum batch size for embedding generation (builder pattern)
    #[allow(dead_code)] // TODO: Add issue - Configurable batch size for performance tuning
    pub fn with_max_batch_size(mut self, max_batch_size: usize) -> Self {
        self.max_batch_size = max_batch_size;
        self
    }

    /// Set the event emitter for SessionEvent emission (builder pattern)
    ///
    /// The emitter is used to emit `SessionEvent` variants (e.g., `EmbeddingBatchComplete`)
    /// when enrichment operations complete.
    #[allow(dead_code)] // TODO: Add issue - Event emission for enrichment pipeline monitoring
    pub fn with_emitter(mut self, emitter: SharedEventBus<SessionEvent>) -> Self {
        self.emitter = Some(emitter);
        self
    }

    /// Get a reference to the event emitter (if configured)
    #[allow(dead_code)] // TODO: Add issue - Event emission for enrichment pipeline monitoring
    pub fn emitter(&self) -> Option<&SharedEventBus<SessionEvent>> {
        self.emitter.as_ref()
    }

    /// Enrich a parsed note with all available enrichments
    ///
    /// # Arguments
    /// * `parsed` - The parsed note with AST
    /// * `changed_blocks` - List of block IDs that changed
    ///
    /// # Returns
    /// An EnrichedNote with embeddings, metadata, and inferred relations
    pub async fn enrich_internal(
        &self,
        parsed: ParsedNote,
        changed_blocks: Vec<String>,
    ) -> Result<EnrichedNote> {
        use std::time::Instant;

        info!(
            "Enriching note: {} ({} changed blocks)",
            parsed.path.display(),
            changed_blocks.len()
        );

        // Track embedding generation time
        let embed_start = Instant::now();

        // Run enrichment operations in parallel
        let (embeddings, metadata, relations) = tokio::join!(
            self.generate_embeddings(&parsed, &changed_blocks),
            self.extract_metadata(&parsed),
            self.infer_relations(&parsed),
        );

        let embeddings = embeddings?;
        let embed_duration = embed_start.elapsed();

        // Emit EmbeddingBatchComplete if we have an emitter and generated embeddings
        if !embeddings.is_empty() {
            if let Some(ref emitter) = self.emitter {
                let entity_id = format!("note:{}", parsed.path.display());
                drop(emitter.emit(SessionEvent::EmbeddingBatchComplete {
                    entity_id,
                    count: embeddings.len(),
                    duration_ms: embed_duration.as_millis() as u64,
                }));
                debug!(
                    "Emitted EmbeddingBatchComplete for {} embeddings in {}ms",
                    embeddings.len(),
                    embed_duration.as_millis()
                );
            }
        }

        Ok(EnrichedNote::new(parsed, embeddings, metadata?, relations?))
    }

    /// Generate embeddings for changed blocks only
    ///
    /// Uses Merkle tree diffs to identify changed blocks and only generates
    /// embeddings for those blocks, avoiding redundant API calls.
    async fn generate_embeddings(
        &self,
        parsed: &ParsedNote,
        changed_blocks: &[String],
    ) -> Result<Vec<BlockEmbedding>> {
        // If no embedding provider, return empty
        let Some(provider) = &self.embedding_provider else {
            debug!("No embedding provider configured, skipping embeddings");
            return Ok(Vec::new());
        };

        // Extract text from changed blocks
        let block_texts = self.extract_block_texts(parsed, changed_blocks);

        if block_texts.is_empty() {
            debug!(
                "No blocks meet embedding criteria (min {} words)",
                self.min_words_for_embedding
            );
            return Ok(Vec::new());
        }

        let model_name = provider.model_name();

        info!(
            "Generating embeddings for {} blocks (batches of {})",
            block_texts.len(),
            self.max_batch_size
        );

        let mut all_embeddings = Vec::new();

        // Process in batches to limit memory usage
        for (batch_idx, chunk) in block_texts.chunks(self.max_batch_size).enumerate() {
            debug!(
                "Processing batch {} ({} blocks)",
                batch_idx + 1,
                chunk.len()
            );

            let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();
            let block_ids: Vec<&String> = chunk.iter().map(|(id, _)| id).collect();

            // Batch embed
            let vectors = provider.embed_batch(&texts).await?;

            // Package results as BlockEmbedding
            let batch_embeddings: Vec<BlockEmbedding> = block_ids
                .iter()
                .zip(vectors)
                .map(|(block_id, vector)| {
                    BlockEmbedding::new((*block_id).clone(), vector, model_name.to_string())
                })
                .collect();

            all_embeddings.extend(batch_embeddings);
        }

        info!(
            "Generated {} embeddings using {}",
            all_embeddings.len(),
            model_name
        );

        Ok(all_embeddings)
    }

    /// Extract block texts that meet embedding criteria
    ///
    /// Traverses the ParsedNote's content structure and extracts text from blocks
    /// that meet the minimum word count threshold. Adds breadcrumbs (heading hierarchy)
    /// to provide context without causing cascade re-embeddings.
    fn extract_block_texts(
        &self,
        parsed: &ParsedNote,
        changed_blocks: &[String],
    ) -> Vec<(String, String)> {
        let mut blocks = Vec::new();

        // Build heading hierarchy for breadcrumbs
        let breadcrumbs = build_breadcrumbs(parsed);

        // Check if we should embed all blocks:
        // 1. Empty changed_blocks means embed everything
        // 2. Section-style IDs (from pipeline) don't map to block IDs, so embed all
        let embed_all = changed_blocks.is_empty()
            || changed_blocks.iter().any(|id| {
                id.starts_with("modified_section")
                    || id.starts_with("added_section")
                    || id.starts_with("removed_section")
            });

        // Extract from headings
        for (idx, heading) in parsed.content.headings.iter().enumerate() {
            let block_id = format!("heading_{}", idx);

            // Check if this block changed (if we have change tracking and block-level IDs)
            if !embed_all && !changed_blocks.contains(&block_id) {
                continue;
            }

            let word_count = heading.text.split_whitespace().count();
            if word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs
                    .get(&heading.offset)
                    .cloned()
                    .unwrap_or_default();
                let text_with_context = if context.is_empty() {
                    heading.text.clone()
                } else {
                    format!("[{}] {}", context, heading.text)
                };
                blocks.push((block_id, text_with_context));
            }
        }

        // Extract from paragraphs
        for (idx, paragraph) in parsed.content.paragraphs.iter().enumerate() {
            let block_id = format!("paragraph_{}", idx);

            // Check if this block changed
            if !embed_all && !changed_blocks.contains(&block_id) {
                continue;
            }

            if paragraph.word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs
                    .get(&paragraph.offset)
                    .cloned()
                    .unwrap_or_default();
                let text_with_context = if context.is_empty() {
                    paragraph.content.clone()
                } else {
                    format!("[{}] {}", context, paragraph.content)
                };
                blocks.push((block_id, text_with_context));
            }
        }

        // Extract from code blocks
        for (idx, code_block) in parsed.content.code_blocks.iter().enumerate() {
            let block_id = format!("code_{}", idx);

            // Check if this block changed
            if !embed_all && !changed_blocks.contains(&block_id) {
                continue;
            }

            let word_count = code_block.content.split_whitespace().count();
            if word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs
                    .get(&code_block.offset)
                    .cloned()
                    .unwrap_or_default();
                let text_with_context = if context.is_empty() {
                    code_block.content.clone()
                } else {
                    format!("[{}] {}", context, code_block.content)
                };
                blocks.push((block_id, text_with_context));
            }
        }

        // Extract from lists
        for (idx, list) in parsed.content.lists.iter().enumerate() {
            let block_id = format!("list_{}", idx);

            // Check if this block changed
            if !embed_all && !changed_blocks.contains(&block_id) {
                continue;
            }

            // Concatenate all list items
            let list_text: String = list
                .items
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            let word_count = list_text.split_whitespace().count();
            if word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs.get(&list.offset).cloned().unwrap_or_default();
                let text_with_context = if context.is_empty() {
                    list_text
                } else {
                    format!("[{}] {}", context, list_text)
                };
                blocks.push((block_id, text_with_context));
            }
        }

        // Extract from blockquotes
        for (idx, blockquote) in parsed.content.blockquotes.iter().enumerate() {
            let block_id = format!("blockquote_{}", idx);

            // Check if this block changed
            if !embed_all && !changed_blocks.contains(&block_id) {
                continue;
            }

            let word_count = blockquote.content.split_whitespace().count();
            if word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs
                    .get(&blockquote.offset)
                    .cloned()
                    .unwrap_or_default();
                let text_with_context = if context.is_empty() {
                    blockquote.content.clone()
                } else {
                    format!("[{}] {}", context, blockquote.content)
                };
                blocks.push((block_id, text_with_context));
            }
        }

        debug!(
            "Extracted {} blocks from {} ({} total blocks in note)",
            blocks.len(),
            parsed.path.display(),
            parsed.content.headings.len()
                + parsed.content.paragraphs.len()
                + parsed.content.code_blocks.len()
                + parsed.content.lists.len()
                + parsed.content.blockquotes.len()
        );

        blocks
    }

    /// Extract metadata from the parsed note
    async fn extract_metadata(&self, parsed: &ParsedNote) -> Result<EnrichmentMetadata> {
        debug!(
            "Computing enrichment metadata for {}",
            parsed.path.display()
        );

        let mut metadata = EnrichmentMetadata::new();

        // Use structural metadata from parser
        let parser_meta = &parsed.metadata;

        // Compute reading time from word count (parser provides this)
        metadata.reading_time_minutes =
            EnrichmentMetadata::compute_reading_time(parser_meta.word_count);

        // Compute complexity score from element counts (parser provides these)
        metadata.complexity_score = EnrichmentMetadata::compute_complexity(
            parser_meta.heading_count,
            parser_meta.code_block_count,
            parser_meta.list_count,
            parser_meta.latex_count,
        );

        // Language detection: default to English
        // TODO: Could use actual language detection library if needed
        metadata.language = Some("en".to_string());

        debug!(
            "Enrichment metadata computed: {:.1} min read, complexity {:.2}",
            metadata.reading_time_minutes, metadata.complexity_score
        );

        Ok(metadata)
    }

    /// Infer relations based on content analysis
    async fn infer_relations(&self, parsed: &ParsedNote) -> Result<Vec<InferredRelation>> {
        debug!("Inferring relations for {}", parsed.path.display());

        // TODO: Implement relation inference
        // This could include:
        // - Semantic similarity to other notes (if we have embeddings stored)
        // - Topic clustering
        // - Temporal proximity
        // - Structural similarity

        // For now, return empty - will be implemented in later phases
        let relations = Vec::new();

        debug!("Inferred {} relations", relations.len());

        Ok(relations)
    }
}

/// Build breadcrumbs (heading hierarchy) for all content positions
///
/// Creates a map from byte offsets to heading paths, allowing blocks to
/// include their hierarchical context in embeddings. The breadcrumb format is:
/// `Filename > H1 > H2 > ...` to provide full document context.
///
/// This is a pure function that operates on the AST structure of a parsed note.
/// It doesn't depend on any service state, making it easier to test and reuse.
///
/// # Arguments
///
/// * `parsed` - The parsed note with heading structure
///
/// # Returns
///
/// A HashMap mapping byte offsets to breadcrumb strings
///
fn build_breadcrumbs(parsed: &ParsedNote) -> std::collections::HashMap<usize, String> {
    use std::collections::HashMap;

    let mut breadcrumbs = HashMap::new();
    let mut heading_stack: Vec<(u8, &str, usize)> = Vec::new(); // (level, text, start_offset)

    // Extract filename (without extension) for breadcrumb prefix
    let filename = parsed
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown");

    // Process all headings and build hierarchical paths
    for heading in &parsed.content.headings {
        // Pop headings at same or higher level (lower number)
        while let Some(&(stack_level, _, _)) = heading_stack.last() {
            if stack_level >= heading.level {
                heading_stack.pop();
            } else {
                break;
            }
        }

        // Add current heading to stack
        heading_stack.push((heading.level, &heading.text, heading.offset));

        // Build breadcrumb path: Filename > H1 > H2 > ...
        let mut path_parts: Vec<&str> = vec![filename];
        path_parts.extend(heading_stack.iter().map(|(_, text, _)| *text));
        let breadcrumb = path_parts.join(" > ");

        // Associate this breadcrumb with the heading's offset
        breadcrumbs.insert(heading.offset, breadcrumb.clone());

        // Find the next heading offset or use file end
        let next_offset = parsed
            .content
            .headings
            .iter()
            .find(|h| h.offset > heading.offset)
            .map(|h| h.offset)
            .unwrap_or(usize::MAX);

        // Apply this breadcrumb to all content in this section
        // (paragraphs, code blocks, etc. between this heading and the next)
        for para in &parsed.content.paragraphs {
            if para.offset > heading.offset && para.offset < next_offset {
                breadcrumbs.insert(para.offset, breadcrumb.clone());
            }
        }

        for code in &parsed.content.code_blocks {
            if code.offset > heading.offset && code.offset < next_offset {
                breadcrumbs.insert(code.offset, breadcrumb.clone());
            }
        }

        for list in &parsed.content.lists {
            if list.offset > heading.offset && list.offset < next_offset {
                breadcrumbs.insert(list.offset, breadcrumb.clone());
            }
        }

        for quote in &parsed.content.blockquotes {
            if quote.offset > heading.offset && quote.offset < next_offset {
                breadcrumbs.insert(quote.offset, breadcrumb.clone());
            }
        }
    }

    breadcrumbs
}

// Trait implementation for SOLID principles (Dependency Inversion)
#[async_trait]
impl crucible_core::enrichment::EnrichmentService for DefaultEnrichmentService {
    async fn enrich(
        &self,
        parsed: ParsedNote,
        changed_block_ids: Vec<String>,
    ) -> Result<EnrichedNote> {
        self.enrich_internal(parsed, changed_block_ids).await
    }

    async fn enrich_with_tree(
        &self,
        parsed: ParsedNote,
        changed_block_ids: Vec<String>,
    ) -> Result<EnrichedNote> {
        // Same as enrich - tree building is now handled elsewhere if needed
        self.enrich_internal(parsed, changed_block_ids).await
    }

    async fn infer_relations(
        &self,
        _enriched: &EnrichedNote,
        _threshold: f64,
    ) -> Result<Vec<InferredRelation>> {
        // Delegate to existing infer_relations method
        // Current implementation returns empty for now (placeholder)
        Ok(Vec::new())
    }

    fn min_words_for_embedding(&self) -> usize {
        self.min_words_for_embedding
    }

    fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }

    fn has_embedding_provider(&self) -> bool {
        self.embedding_provider.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Mock embedding provider for testing
    struct MockEmbeddingProvider {
        model: String,
        dimensions: usize,
    }

    impl MockEmbeddingProvider {
        fn new() -> Self {
            Self {
                model: "mock-model".to_string(),
                dimensions: 3,
            }
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn model_name(&self) -> &str {
            &self.model
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }
    }

    #[tokio::test]
    async fn test_enrichment_service_with_provider() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = DefaultEnrichmentService::new(provider);

        assert!(service.embedding_provider.is_some());
        assert_eq!(service.min_words_for_embedding, 5);
    }

    #[tokio::test]
    async fn test_enrichment_service_without_provider() {
        let service = DefaultEnrichmentService::without_embeddings();

        assert!(service.embedding_provider.is_none());
    }

    #[tokio::test]
    async fn test_enrichment_service_with_custom_min_words() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = DefaultEnrichmentService::new(provider).with_min_words(10);

        assert_eq!(service.min_words_for_embedding, 10);
    }

    #[tokio::test]
    async fn test_generate_embeddings_without_provider() {
        let service = DefaultEnrichmentService::without_embeddings();

        // Create a minimal ParsedNote for testing
        let parsed = create_test_parsed_note();

        let embeddings = service
            .generate_embeddings(&parsed, &["block_1".to_string()])
            .await
            .unwrap();

        // Should return empty when no provider
        assert_eq!(embeddings.len(), 0);
    }

    #[tokio::test]
    async fn test_extract_metadata() {
        let service = DefaultEnrichmentService::without_embeddings();
        let parsed = create_test_parsed_note();

        let metadata = service.extract_metadata(&parsed).await.unwrap();

        // Empty note should have zero reading time and complexity (computed from parser metadata)
        assert_eq!(metadata.language, Some("en".to_string()));
        assert_eq!(metadata.reading_time_minutes, 0.0);
        assert_eq!(metadata.complexity_score, 0.0);

        // Verify parser provided structural metadata
        assert_eq!(parsed.metadata.word_count, 0);
    }

    #[tokio::test]
    async fn test_infer_relations() {
        let service = DefaultEnrichmentService::without_embeddings();
        let parsed = create_test_parsed_note();

        let relations = service.infer_relations(&parsed).await.unwrap();

        // Should return empty for now (placeholder implementation)
        assert_eq!(relations.len(), 0);
    }

    /// Helper to create a minimal ParsedNote for testing
    fn create_test_parsed_note() -> ParsedNote {
        use crucible_core::parser::ParsedNoteBuilder;

        ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build()
    }

    /// Helper to create a ParsedNote with content for testing embeddings
    fn create_test_parsed_note_with_content() -> ParsedNote {
        use crucible_core::parser::{Paragraph, ParsedNoteBuilder};

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();

        // Add paragraphs with sufficient words (>= 5 to meet embedding threshold)
        note.content.paragraphs.push(Paragraph::new(
            "This is the first paragraph with more than five words for embedding.".to_string(),
            0,
        ));
        note.content.paragraphs.push(Paragraph::new(
            "This is the second paragraph also containing enough words.".to_string(),
            100,
        ));

        note
    }

    /// Test that embeddings are generated when changed_blocks is empty (embed all)
    #[tokio::test]
    async fn test_generate_embeddings_with_empty_changed_blocks() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = DefaultEnrichmentService::new(provider);

        let parsed = create_test_parsed_note_with_content();

        // Empty changed_blocks should embed ALL blocks
        let embeddings = service.generate_embeddings(&parsed, &[]).await.unwrap();

        // Should have embeddings for both paragraphs
        assert_eq!(
            embeddings.len(),
            2,
            "Expected 2 embeddings for 2 paragraphs"
        );
        assert_eq!(embeddings[0].block_id, "paragraph_0");
        assert_eq!(embeddings[1].block_id, "paragraph_1");
    }

    /// Test that section-style changed_blocks (from pipeline) trigger embedding all blocks
    /// This is the bug reproduction test - with section-style IDs, no blocks should be skipped
    #[tokio::test]
    async fn test_generate_embeddings_with_section_style_changed_blocks() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = DefaultEnrichmentService::new(provider);

        let parsed = create_test_parsed_note_with_content();

        // Section-style IDs from pipeline (modified_section_0, added_section_1, etc.)
        // These don't match block IDs (paragraph_0, heading_0, etc.)
        let section_style_ids = vec![
            "modified_section_0".to_string(),
            "added_section_1".to_string(),
        ];

        let embeddings = service
            .generate_embeddings(&parsed, &section_style_ids)
            .await
            .unwrap();

        // BUG: Currently returns 0 because section IDs don't match block IDs
        // After fix: Should return 2 (all blocks embedded when IDs are section-style)
        assert_eq!(
            embeddings.len(),
            2,
            "Expected 2 embeddings - section-style IDs should embed all blocks"
        );
    }

    #[tokio::test]
    async fn test_enrich_internal_full_flow_without_embeddings() {
        let service = DefaultEnrichmentService::without_embeddings();
        let parsed = create_test_parsed_note_with_content();

        let enriched = service.enrich_internal(parsed, vec![]).await.unwrap();

        assert!(enriched.embeddings.is_empty());
        assert_eq!(enriched.metadata.language, Some("en".to_string()));
        assert!(enriched.inferred_relations.is_empty());
    }

    #[tokio::test]
    async fn test_enrich_internal_full_flow_with_embeddings() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = DefaultEnrichmentService::new(provider);
        let parsed = create_test_parsed_note_with_content();

        let enriched = service.enrich_internal(parsed, vec![]).await.unwrap();

        assert_eq!(enriched.embeddings.len(), 2);
        assert_eq!(enriched.embeddings[0].model, "mock-model");
    }

    #[tokio::test]
    async fn test_trait_impl_enrich() {
        use crucible_core::enrichment::EnrichmentService;

        let service = DefaultEnrichmentService::without_embeddings();
        let parsed = create_test_parsed_note();

        let enriched = service.enrich(parsed, vec![]).await.unwrap();
        assert!(enriched.embeddings.is_empty());
    }

    #[tokio::test]
    async fn test_trait_impl_has_embedding_provider() {
        use crucible_core::enrichment::EnrichmentService;

        let without = DefaultEnrichmentService::without_embeddings();
        assert!(!without.has_embedding_provider());

        let with = DefaultEnrichmentService::new(Arc::new(MockEmbeddingProvider::new()));
        assert!(with.has_embedding_provider());
    }

    #[test]
    fn test_build_breadcrumbs_with_headings() {
        use crucible_core::parser::{Heading, Paragraph, ParsedNoteBuilder};

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content.headings.push(Heading::new(1, "Introduction", 0));
        note.content.headings.push(Heading::new(2, "Details", 50));
        note.content.paragraphs.push(Paragraph::new(
            "A paragraph under Details heading with enough words.".to_string(),
            60,
        ));

        let crumbs = build_breadcrumbs(&note);

        assert!(crumbs.contains_key(&0));
        assert!(crumbs.contains_key(&50));
        let para_crumb = crumbs.get(&60).unwrap();
        assert!(
            para_crumb.contains("Details"),
            "paragraph breadcrumb should contain parent heading: {para_crumb}"
        );
    }

    #[test]
    fn test_build_breadcrumbs_empty_note() {
        let note = create_test_parsed_note();
        let crumbs = build_breadcrumbs(&note);
        assert!(crumbs.is_empty());
    }

    #[test]
    fn test_extract_block_texts_skips_short_blocks() {
        use crucible_core::parser::{Paragraph, ParsedNoteBuilder};

        let service = DefaultEnrichmentService::without_embeddings();
        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content
            .paragraphs
            .push(Paragraph::new("Hi".to_string(), 0));
        note.content
            .paragraphs
            .push(Paragraph::new("One two three four five six".to_string(), 10));

        let blocks = service.extract_block_texts(&note, &[]);
        assert_eq!(blocks.len(), 1, "short paragraph should be skipped");
        assert_eq!(blocks[0].0, "paragraph_1");
    }

    #[test]
    fn test_builder_with_max_batch_size() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = DefaultEnrichmentService::new(provider).with_max_batch_size(5);
        assert_eq!(service.max_batch_size, 5);
    }
}
