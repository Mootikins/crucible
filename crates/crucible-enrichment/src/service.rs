//! Enrichment Service
//!
//! Orchestrates all enrichment operations including embedding generation,
//! metadata extraction, and relation inference. Follows clean architecture
//! principles with dependency injection.

use crate::types::{
    BlockEmbedding, EnrichedNote, InferredRelation, NoteMetadata,
};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::merkle::HybridMerkleTree;
use crucible_parser::ParsedNote;
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};
use async_trait::async_trait;

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
}

impl Default for DefaultEnrichmentService {
    fn default() -> Self {
        Self {
            embedding_provider: None,
            min_words_for_embedding: DEFAULT_MIN_WORDS_FOR_EMBEDDING,
            max_batch_size: DEFAULT_MAX_BATCH_SIZE,
        }
    }
}

impl DefaultEnrichmentService {
    /// Create a new enrichment service with an embedding provider
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use std::sync::Arc;
    /// use crucible_core::enrichment::EnrichmentService;
    ///
    /// let provider = Arc::new(my_provider);
    /// let service = EnrichmentService::new(provider);
    /// ```
    pub fn new(embedding_provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            embedding_provider: Some(embedding_provider),
            ..Default::default()
        }
    }

    /// Create an enrichment service without embeddings (metadata/relations only)
    ///
    /// This is equivalent to `EnrichmentService::default()`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_enrichment::EnrichmentService;
    ///
    /// let service = EnrichmentService::without_embeddings();
    /// ```
    pub fn without_embeddings() -> Self {
        Self::default()
    }

    /// Set the minimum word count for embedding generation (builder pattern)
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_enrichment::EnrichmentService;
    ///
    /// let service = EnrichmentService::without_embeddings()
    ///     .with_min_words(10);
    /// ```
    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.min_words_for_embedding = min_words;
        self
    }

    /// Set the maximum batch size for embedding generation (builder pattern)
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_enrichment::EnrichmentService;
    ///
    /// let service = EnrichmentService::without_embeddings()
    ///     .with_max_batch_size(20);
    /// ```
    pub fn with_max_batch_size(mut self, max_batch_size: usize) -> Self {
        self.max_batch_size = max_batch_size;
        self
    }

    /// Enrich a parsed note with all available enrichments
    ///
    /// # Arguments
    /// * `parsed` - The parsed note with AST
    /// * `merkle_tree` - The Merkle tree (already computed from AST)
    /// * `changed_blocks` - List of block IDs that changed (from Merkle diff)
    ///
    /// # Returns
    /// An EnrichedNote with embeddings, metadata, and inferred relations
    pub async fn enrich_internal(
        &self,
        parsed: ParsedNote,
        merkle_tree: HybridMerkleTree,
        changed_blocks: Vec<String>,
    ) -> Result<EnrichedNote> {
        info!(
            "Enriching note: {} ({} changed blocks)",
            parsed.path.display(),
            changed_blocks.len()
        );

        // Run enrichment operations in parallel
        let (embeddings, metadata, relations) = tokio::join!(
            self.generate_embeddings(&parsed, &changed_blocks),
            self.extract_metadata(&parsed),
            self.infer_relations(&parsed),
        );

        Ok(EnrichedNote::new(
            parsed,
            merkle_tree,
            embeddings?,
            metadata?,
            relations?,
        ))
    }

    /// Generate embeddings for changed blocks only
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

        // For now, we'll extract text from changed blocks
        // TODO: Integrate with actual block structure from ParsedNote
        let block_texts = self.extract_block_texts(parsed, changed_blocks);

        if block_texts.is_empty() {
            debug!("No blocks meet embedding criteria (min {} words)", self.min_words_for_embedding);
            return Ok(Vec::new());
        }

        info!("Generating embeddings for {} blocks (batches of {})",
            block_texts.len(), self.max_batch_size);

        let mut all_embeddings = Vec::new();

        // Process in chunks to limit memory usage
        for (batch_idx, chunk) in block_texts.chunks(self.max_batch_size).enumerate() {
            debug!("Processing batch {} ({} blocks)", batch_idx + 1, chunk.len());

            // Prepare texts for this batch
            let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();

            // Batch embed
            let vectors = provider.embed_batch(&texts).await?;

            // Package results as BlockEmbedding
            let batch_embeddings: Vec<BlockEmbedding> = chunk
                .iter()
                .zip(vectors)
                .map(|((block_id, _text), vector)| {
                    BlockEmbedding::new(
                        block_id.clone(),
                        vector,
                        provider.model_name().to_string(),
                    )
                })
                .collect();

            all_embeddings.extend(batch_embeddings);
        }

        info!("Generated {} embeddings using {}", all_embeddings.len(), provider.model_name());

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

        // Extract from headings
        for (idx, heading) in parsed.content.headings.iter().enumerate() {
            let block_id = format!("heading_{}", idx);

            // Check if this block changed (if we have change tracking)
            if !changed_blocks.is_empty() && !changed_blocks.contains(&block_id) {
                continue;
            }

            let word_count = heading.text.split_whitespace().count();
            if word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs.get(&heading.offset).cloned().unwrap_or_default();
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
            if !changed_blocks.is_empty() && !changed_blocks.contains(&block_id) {
                continue;
            }

            if paragraph.word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs.get(&paragraph.offset).cloned().unwrap_or_default();
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
            if !changed_blocks.is_empty() && !changed_blocks.contains(&block_id) {
                continue;
            }

            let word_count = code_block.content.split_whitespace().count();
            if word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs.get(&code_block.offset).cloned().unwrap_or_default();
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
            if !changed_blocks.is_empty() && !changed_blocks.contains(&block_id) {
                continue;
            }

            // Concatenate all list items
            let list_text: String = list.items.iter()
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
            if !changed_blocks.is_empty() && !changed_blocks.contains(&block_id) {
                continue;
            }

            let word_count = blockquote.content.split_whitespace().count();
            if word_count >= self.min_words_for_embedding {
                // Add breadcrumbs for context
                let context = breadcrumbs.get(&blockquote.offset).cloned().unwrap_or_default();
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
            parsed.content.headings.len() + parsed.content.paragraphs.len() +
            parsed.content.code_blocks.len() + parsed.content.lists.len() +
            parsed.content.blockquotes.len()
        );

        blocks
    }

    /// Extract metadata from the parsed note
    async fn extract_metadata(&self, parsed: &ParsedNote) -> Result<NoteMetadata> {
        debug!("Computing enrichment metadata for {}", parsed.path.display());

        let mut metadata = NoteMetadata::new();

        // Use structural metadata from parser
        let parser_meta = &parsed.metadata;

        // Compute reading time from word count (parser provides this)
        metadata.reading_time_minutes = NoteMetadata::compute_reading_time(parser_meta.word_count);

        // Compute complexity score from element counts (parser provides these)
        metadata.complexity_score = NoteMetadata::compute_complexity(
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
            metadata.reading_time_minutes,
            metadata.complexity_score
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
/// include their hierarchical context in embeddings.
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
/// # Example
///
/// ```rust,ignore
/// use crucible_core::enrichment::build_breadcrumbs;
///
/// let breadcrumbs = build_breadcrumbs(&parsed_note);
/// // For a paragraph under "# Introduction" > "## Background":
/// // breadcrumbs[offset] == "Introduction > Background"
/// ```
fn build_breadcrumbs(parsed: &ParsedNote) -> std::collections::HashMap<usize, String> {
    use std::collections::HashMap;

    let mut breadcrumbs = HashMap::new();
    let mut heading_stack: Vec<(u8, &str, usize)> = Vec::new(); // (level, text, start_offset)

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

        // Build breadcrumb path for this position
        let path: Vec<&str> = heading_stack.iter().map(|(_, text, _)| *text).collect();
        let breadcrumb = path.join(" > ");

        // Associate this breadcrumb with the heading's offset
        breadcrumbs.insert(heading.offset, breadcrumb.clone());

        // Find the next heading offset or use file end
        let next_offset = parsed.content.headings
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
        // Build Merkle tree from parsed note
        let merkle_tree = HybridMerkleTree::from_document(&parsed);

        // Delegate to enrich_with_tree
        self.enrich_with_tree(parsed, merkle_tree, changed_block_ids).await
    }

    async fn enrich_with_tree(
        &self,
        parsed: ParsedNote,
        merkle_tree: HybridMerkleTree,
        changed_block_ids: Vec<String>,
    ) -> Result<EnrichedNote> {
        // Delegate to existing enrich method (which takes merkle_tree)
        self.enrich_internal(parsed, merkle_tree, changed_block_ids).await
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
            .generate_embeddings(&parsed, &vec!["block_1".to_string()])
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
        use crucible_parser::ParsedNoteBuilder;

        ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build()
    }
}
