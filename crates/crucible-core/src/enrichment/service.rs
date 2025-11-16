//! Enrichment Service
//!
//! Orchestrates all enrichment operations including embedding generation,
//! metadata extraction, and relation inference. Follows clean architecture
//! principles with dependency injection.

use crate::enrichment::{
    BlockEmbedding, EmbeddingProvider, EnrichedNote, InferredRelation, NoteMetadata,
};
use crate::merkle::HybridMerkleTree;
use crate::types::ParsedNote;
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};

/// Service that orchestrates all enrichment operations
///
/// Receives a ParsedNote and list of changed blocks, then coordinates:
/// - Embedding generation (only for changed blocks)
/// - Metadata extraction (word counts, language, etc.)
/// - Relation inference (semantic similarity, clustering)
///
/// Returns an EnrichedNote ready for storage.
pub struct EnrichmentService {
    /// Embedding provider (dependency injected)
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,

    /// Minimum word count for embedding generation
    min_words_for_embedding: usize,

    /// Maximum blocks to embed in a single batch (prevents memory issues)
    max_batch_size: usize,
}

impl EnrichmentService {
    /// Create a new enrichment service with an embedding provider
    pub fn new(embedding_provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            embedding_provider: Some(embedding_provider),
            min_words_for_embedding: 5,
            max_batch_size: 10,  // Process 10 blocks at a time
        }
    }

    /// Create an enrichment service without embeddings (metadata/relations only)
    pub fn without_embeddings() -> Self {
        Self {
            embedding_provider: None,
            min_words_for_embedding: 5,
            max_batch_size: 10,
        }
    }

    /// Set the minimum word count for embedding generation
    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.min_words_for_embedding = min_words;
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
    pub async fn enrich(
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
        let breadcrumbs = self.build_breadcrumbs(parsed);

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
        debug!("Extracting metadata from {}", parsed.path.display());

        let mut metadata = NoteMetadata::new();
        let mut block_word_counts = Vec::new();
        let mut total_words = 0;

        // Count words in headings
        for (idx, heading) in parsed.content.headings.iter().enumerate() {
            let block_id = format!("heading_{}", idx);
            let word_count = heading.text.split_whitespace().count();
            total_words += word_count;
            block_word_counts.push((block_id, word_count));
        }

        // Count words in paragraphs
        for (idx, paragraph) in parsed.content.paragraphs.iter().enumerate() {
            let block_id = format!("paragraph_{}", idx);
            total_words += paragraph.word_count;
            block_word_counts.push((block_id, paragraph.word_count));
        }

        // Count words in code blocks
        for (idx, code_block) in parsed.content.code_blocks.iter().enumerate() {
            let block_id = format!("code_{}", idx);
            let word_count = code_block.content.split_whitespace().count();
            total_words += word_count;
            block_word_counts.push((block_id, word_count));
        }

        // Count words in lists
        for (idx, list) in parsed.content.lists.iter().enumerate() {
            let block_id = format!("list_{}", idx);
            let list_words: usize = list.items.iter()
                .map(|item| item.content.split_whitespace().count())
                .sum();
            total_words += list_words;
            block_word_counts.push((block_id, list_words));
        }

        // Count words in blockquotes
        for (idx, blockquote) in parsed.content.blockquotes.iter().enumerate() {
            let block_id = format!("blockquote_{}", idx);
            let word_count = blockquote.content.split_whitespace().count();
            total_words += word_count;
            block_word_counts.push((block_id, word_count));
        }

        metadata.total_word_count = total_words;
        metadata.block_word_counts = block_word_counts;

        // Estimate reading time (assuming 200 words per minute)
        metadata.reading_time_minutes = (total_words as f32) / 200.0;

        // Language detection: default to English
        // TODO: Could use actual language detection library if needed
        metadata.language = Some("en".to_string());

        // Calculate complexity score based on:
        // - Number of headings (structure)
        // - Number of code blocks (technical content)
        // - Average words per block
        // - Number of links (connectivity)
        let heading_count = parsed.content.headings.len() as f32;
        let code_count = parsed.content.code_blocks.len() as f32;
        let link_count = (parsed.wikilinks.len() + parsed.inline_links.len()) as f32;
        let avg_words_per_block = if !metadata.block_word_counts.is_empty() {
            total_words as f32 / metadata.block_word_counts.len() as f32
        } else {
            0.0
        };

        // Normalize and combine factors (0.0-1.0 scale)
        let structure_score = (heading_count / 10.0).min(1.0);
        let technical_score = (code_count / 5.0).min(1.0);
        let connectivity_score = (link_count / 10.0).min(1.0);
        let verbosity_score = (avg_words_per_block / 100.0).min(1.0);

        metadata.complexity_score =
            (structure_score * 0.3 + technical_score * 0.2 +
             connectivity_score * 0.2 + verbosity_score * 0.3)
            .clamp(0.0, 1.0);

        debug!(
            "Metadata extracted: {} words, {:.1} min read, complexity {:.2}",
            metadata.total_word_count,
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

    /// Build breadcrumbs (heading hierarchy) for all content positions
    ///
    /// Creates a map from byte offsets to heading paths, allowing blocks to
    /// include their hierarchical context in embeddings.
    ///
    /// Example: A paragraph under "# Introduction" > "## Background" would have
    /// breadcrumbs "Introduction > Background"
    fn build_breadcrumbs(&self, parsed: &ParsedNote) -> std::collections::HashMap<usize, String> {
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
        let service = EnrichmentService::new(provider);

        assert!(service.embedding_provider.is_some());
        assert_eq!(service.min_words_for_embedding, 5);
    }

    #[tokio::test]
    async fn test_enrichment_service_without_provider() {
        let service = EnrichmentService::without_embeddings();

        assert!(service.embedding_provider.is_none());
    }

    #[tokio::test]
    async fn test_enrichment_service_with_custom_min_words() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = EnrichmentService::new(provider).with_min_words(10);

        assert_eq!(service.min_words_for_embedding, 10);
    }

    #[tokio::test]
    async fn test_generate_embeddings_without_provider() {
        let service = EnrichmentService::without_embeddings();

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
        let service = EnrichmentService::without_embeddings();
        let parsed = create_test_parsed_note();

        let metadata = service.extract_metadata(&parsed).await.unwrap();

        // Empty note should have zero word count and complexity
        assert_eq!(metadata.language, Some("en".to_string()));
        assert_eq!(metadata.total_word_count, 0);
        assert_eq!(metadata.reading_time_minutes, 0.0);
        assert_eq!(metadata.complexity_score, 0.0);
        assert_eq!(metadata.block_word_counts.len(), 0);
    }

    #[tokio::test]
    async fn test_infer_relations() {
        let service = EnrichmentService::without_embeddings();
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
