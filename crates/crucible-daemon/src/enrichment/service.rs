//! Note enrichment.
//!
//! Turns a `ParsedNote` + list of changed block IDs into an `EnrichedNote`
//! ready for storage. Runs embedding generation and metadata extraction in
//! parallel.

use super::types::{BlockEmbedding, EnrichmentMetadata};
use anyhow::Result;
use crucible_core::enrichment::{EmbeddingProvider, EnrichedNote};
use crucible_core::parser::types::BlockKind;
use crucible_core::parser::BlockHash;
use crucible_core::ParsedNote;

/// The id a block carries through enrichment: its rank in the note.
///
/// One definition, because the pipeline pairs an embedding back to its block
/// by this string when it writes the rows.
pub fn block_id(index: usize) -> String {
    format!("block_{index}")
}

/// One block queued for embedding.
struct EmbedCandidate {
    /// The block's rank in the note, as `block_<n>`.
    block_id: String,
    /// The text that reaches the model, heading trail included.
    text: String,
    /// BLAKE3 of the block's source bytes. The reuse key: an identical block
    /// anywhere yields this same hash, so one vector can serve them all.
    content_hash: BlockHash,
}
use std::sync::Arc;
use tracing::{debug, info};

/// The name a stored vector records: the backend and the model, as
/// `<provider_kind>/<model_name>`.
///
/// A model name alone is ambiguous across backends. Ollama's
/// `nomic-embed-text` is not fastembed's `nomic-embed-text-v1.5`, and two
/// backends may serve one name with different weights. The pair is the key
/// `cached_vectors` matches on, so a vector is never reused for a backend
/// that did not produce it.
fn storage_key(provider: &dyn EmbeddingProvider) -> String {
    format!("{}/{}", provider.provider_kind(), provider.model_name())
}

/// Enriches parsed notes with embeddings and metadata.
pub struct Enricher {
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    /// Where already-paid-for vectors are looked up by content hash.
    /// `None` disables reuse; every block is then embedded afresh.
    block_cache: Option<Arc<dyn crucible_core::storage::BlockStore>>,
    min_words_for_embedding: usize,
    max_batch_size: usize,
}

impl Enricher {
    /// Create an enricher with an embedding provider.
    pub fn new(embedding_provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            embedding_provider: Some(embedding_provider),
            block_cache: None,
            min_words_for_embedding: 5,
            max_batch_size: 10,
        }
    }

    /// Create an enricher without embeddings (metadata only).
    pub fn without_embeddings() -> Self {
        Self {
            embedding_provider: None,
            block_cache: None,
            min_words_for_embedding: 5,
            max_batch_size: 10,
        }
    }

    /// Reuse vectors already stored for the same text under the same model.
    ///
    /// An edit at the top of a file shifts every span below it, so every row
    /// is rewritten — but the text is unchanged, so no vector is recomputed.
    pub fn with_block_cache(mut self, blocks: Arc<dyn crucible_core::storage::BlockStore>) -> Self {
        self.block_cache = Some(blocks);
        self
    }

    /// Create an enricher, using embeddings if a provider is supplied.
    pub fn from_optional_provider(provider: Option<Arc<dyn EmbeddingProvider>>) -> Self {
        match provider {
            Some(p) => Self::new(p),
            None => Self::without_embeddings(),
        }
    }

    /// Enrich a parsed note.
    ///
    /// `changed_blocks` is the list of block IDs known to have changed. Pass
    /// an empty slice to embed every block. Section-style IDs (e.g.
    /// `modified_section_0`) also trigger embed-all since they don't map to
    /// concrete block IDs.
    pub async fn enrich(
        &self,
        parsed: ParsedNote,
        changed_blocks: Vec<String>,
    ) -> Result<EnrichedNote> {
        info!(
            "Enriching note: {} ({} changed blocks)",
            parsed.path.display(),
            changed_blocks.len()
        );

        let (embeddings, note_embedding, metadata) = tokio::join!(
            self.generate_embeddings(&parsed, &changed_blocks),
            self.generate_note_embedding(&parsed),
            self.extract_metadata(&parsed),
        );

        Ok(EnrichedNote::new(
            parsed,
            embeddings?,
            note_embedding?,
            metadata?,
        ))
    }

    /// Embed the note body once, for `notes.embedding`.
    ///
    /// The note gets its own forward pass rather than a mean of its block
    /// vectors: a mean sits at the centroid of a note's topics, which is a
    /// point the note may never make, and it dilutes as the note grows.
    async fn generate_note_embedding(&self, parsed: &ParsedNote) -> Result<Option<BlockEmbedding>> {
        let Some(provider) = &self.embedding_provider else {
            return Ok(None);
        };

        let body = parsed.content.plain_text.trim();
        if body.is_empty() {
            return Ok(None);
        }

        // The title leads the text. It is often the most specific statement of
        // what the note is about, and a query frequently names it.
        let text = format!("{}\n\n{}", parsed.title(), body);
        let vector = provider.embed(&text).await?;
        Ok(Some(BlockEmbedding::new(
            "note".to_string(),
            vector,
            storage_key(provider.as_ref()),
        )))
    }

    /// Generate embeddings for changed blocks only.
    async fn generate_embeddings(
        &self,
        parsed: &ParsedNote,
        changed_blocks: &[String],
    ) -> Result<Vec<BlockEmbedding>> {
        let Some(provider) = &self.embedding_provider else {
            debug!("No embedding provider configured, skipping embeddings");
            return Ok(Vec::new());
        };

        let block_texts = self.extract_block_texts(parsed, changed_blocks);

        if block_texts.is_empty() {
            debug!(
                "No blocks meet embedding criteria (min {} words)",
                self.min_words_for_embedding
            );
            return Ok(Vec::new());
        }

        // The stored name is the backend and the model, so a vector is only
        // ever reused for the backend that produced it.
        let model_name = storage_key(provider.as_ref());
        let model_name = model_name.as_str();

        info!(
            "Generating embeddings for {} blocks (batches of {})",
            block_texts.len(),
            self.max_batch_size
        );

        // Ask the store what it already holds for this text under this
        // model. Anything it answers costs no forward pass.
        let reused = self.reuse_cached(&block_texts, model_name).await;
        let mut all_embeddings: Vec<BlockEmbedding> = Vec::new();
        let mut to_embed: Vec<&EmbedCandidate> = Vec::new();
        for candidate in &block_texts {
            match reused.get(&candidate.content_hash) {
                Some(cached) => all_embeddings.push(BlockEmbedding::with_content_hash(
                    candidate.block_id.clone(),
                    cached.embedding.clone(),
                    model_name.to_string(),
                    None,
                    candidate.content_hash.to_hex(),
                )),
                None => to_embed.push(candidate),
            }
        }

        if !all_embeddings.is_empty() {
            debug!(
                "Reused {} of {} block vectors from the store",
                all_embeddings.len(),
                block_texts.len()
            );
        }

        for (batch_idx, chunk) in to_embed.chunks(self.max_batch_size).enumerate() {
            debug!(
                "Processing batch {} ({} blocks)",
                batch_idx + 1,
                chunk.len()
            );

            let texts: Vec<&str> = chunk.iter().map(|c| c.text.as_str()).collect();

            let vectors = provider.embed_batch(&texts).await?;

            let batch_embeddings: Vec<BlockEmbedding> = chunk
                .iter()
                .zip(vectors)
                .map(|(candidate, vector)| {
                    BlockEmbedding::with_content_hash(
                        candidate.block_id.clone(),
                        vector,
                        model_name.to_string(),
                        None,
                        candidate.content_hash.to_hex(),
                    )
                })
                .collect();

            all_embeddings.extend(batch_embeddings);
        }

        info!(
            "Generated {} embeddings using {}",
            all_embeddings.len(),
            model_name
        );

        // Back into document order: the reused ones were collected first.
        all_embeddings.sort_by(|a, b| a.block_id.cmp(&b.block_id));
        Ok(all_embeddings)
    }

    /// Build `(block_id, embedded_text)` for every block worth embedding.
    ///
    /// One pass in document order. The id is the block's rank in the note, so
    /// it names a position in the document rather than a per-kind counter, and
    /// the heading trail comes from the walk rather than from an offset map.
    /// Vectors the store already holds for these blocks under `model`.
    ///
    /// A miss is not an error: an unreachable store means every block is
    /// embedded afresh, which is correct, only slower.
    async fn reuse_cached(
        &self,
        candidates: &[EmbedCandidate],
        model: &str,
    ) -> std::collections::HashMap<BlockHash, crucible_core::storage::CachedVector> {
        let Some(cache) = &self.block_cache else {
            return std::collections::HashMap::new();
        };

        let mut hashes: Vec<BlockHash> = candidates.iter().map(|c| c.content_hash).collect();
        hashes.sort_by_key(|h| h.to_hex());
        hashes.dedup();

        match cache.cached_vectors(&hashes, model).await {
            Ok(found) => found.into_iter().collect(),
            Err(e) => {
                debug!(error = %e, "block vector cache unavailable; embedding every block");
                std::collections::HashMap::new()
            }
        }
    }

    fn extract_block_texts(
        &self,
        parsed: &ParsedNote,
        changed_blocks: &[String],
    ) -> Vec<EmbedCandidate> {
        // Embed everything when the caller names no blocks, or when the ids
        // are section-style (from the pipeline's diff layer) rather than
        // concrete block ids.
        let embed_all = changed_blocks.is_empty()
            || changed_blocks.iter().any(|id| {
                id.starts_with("modified_section")
                    || id.starts_with("added_section")
                    || id.starts_with("removed_section")
            });

        let filename = parsed
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown");

        // The open heading path, as (level, text). A heading closes every
        // entry at its own level or deeper.
        let mut trail: Vec<(u8, &str)> = Vec::new();
        let mut blocks = Vec::new();

        for (index, block) in parsed.content.blocks.iter().enumerate() {
            if let BlockKind::Heading { level } = block.kind {
                while trail.last().is_some_and(|(open, _)| *open >= level) {
                    trail.pop();
                }
                trail.push((level, block.text.as_str()));
            }

            let block_id = block_id(index);
            if !embed_all && !changed_blocks.contains(&block_id) {
                continue;
            }
            if block.word_count() < self.min_words_for_embedding {
                continue;
            }

            let mut breadcrumb = String::from(filename);
            for (_, heading) in &trail {
                breadcrumb.push_str(" > ");
                breadcrumb.push_str(heading);
            }
            blocks.push(EmbedCandidate {
                block_id,
                text: format!("[{}] {}", breadcrumb, block.text),
                content_hash: block.content_hash,
            });
        }

        blocks
    }

    async fn extract_metadata(&self, parsed: &ParsedNote) -> Result<EnrichmentMetadata> {
        debug!(
            "Computing enrichment metadata for {}",
            parsed.path.display()
        );

        let mut metadata = EnrichmentMetadata::new();

        let parser_meta = &parsed.metadata;

        metadata.reading_time_minutes =
            EnrichmentMetadata::compute_reading_time(parser_meta.word_count);

        metadata.complexity_score = EnrichmentMetadata::compute_complexity(
            parser_meta.heading_count,
            parser_meta.code_block_count,
            parser_meta.list_count,
            parser_meta.latex_count,
        );

        metadata.language = Some("en".to_string());

        debug!(
            "Enrichment metadata computed: {:.1} min read, complexity {:.2}",
            metadata.reading_time_minutes, metadata.complexity_score
        );

        Ok(metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockEmbeddingProvider;
    use std::path::PathBuf;

    #[tokio::test]
    async fn new_with_provider_keeps_provider() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = Enricher::new(provider);

        assert!(service.embedding_provider.is_some());
        assert_eq!(service.min_words_for_embedding, 5);
    }

    #[tokio::test]
    async fn without_embeddings_has_no_provider() {
        let service = Enricher::without_embeddings();

        assert!(service.embedding_provider.is_none());
    }

    #[tokio::test]
    async fn generate_embeddings_returns_empty_without_provider() {
        let service = Enricher::without_embeddings();

        let parsed = create_test_parsed_note();

        let embeddings = service
            .generate_embeddings(&parsed, &["block_1".to_string()])
            .await
            .unwrap();

        assert_eq!(embeddings.len(), 0);
    }

    #[tokio::test]
    async fn extract_metadata_computes_defaults_for_empty_note() {
        let service = Enricher::without_embeddings();
        let parsed = create_test_parsed_note();

        let metadata = service.extract_metadata(&parsed).await.unwrap();

        assert_eq!(metadata.language, Some("en".to_string()));
        assert_eq!(metadata.reading_time_minutes, 0.0);
        assert_eq!(metadata.complexity_score, 0.0);

        assert_eq!(parsed.metadata.word_count, 0);
    }

    fn create_test_parsed_note() -> ParsedNote {
        use crucible_core::parser::ParsedNoteBuilder;

        ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build()
    }

    /// Build a block that spans real bytes: `start_offset` filler, then the
    /// text. The hash is then BLAKE3 of the text, as it is in production.
    fn block(
        kind: crucible_core::parser::types::BlockKind,
        text: &str,
        start_offset: usize,
    ) -> crucible_core::parser::types::Block {
        use crucible_core::parser::types::Block;
        let source = " ".repeat(start_offset) + text;
        let end_offset = source.len();
        Block::new(kind, text.to_string(), start_offset, end_offset, &source)
    }

    fn para(text: &str, start_offset: usize) -> crucible_core::parser::types::Block {
        block(
            crucible_core::parser::types::BlockKind::Paragraph,
            text,
            start_offset,
        )
    }

    fn heading(level: u8, text: &str, start_offset: usize) -> crucible_core::parser::types::Block {
        block(
            crucible_core::parser::types::BlockKind::Heading { level },
            text,
            start_offset,
        )
    }

    fn create_test_parsed_note_with_content() -> ParsedNote {
        use crucible_core::parser::ParsedNoteBuilder;

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content.blocks = vec![
            para(
                "This is the first paragraph with more than five words for embedding.",
                0,
            ),
            para(
                "This is the second paragraph also containing enough words.",
                100,
            ),
        ];
        note
    }

    fn create_test_parsed_note_with_three_paragraphs() -> ParsedNote {
        use crucible_core::parser::ParsedNoteBuilder;

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content.blocks = vec![
            para("Paragraph one has more than five words for embedding", 0),
            para("Paragraph two also has enough words for embedding", 100),
            para("Paragraph three has enough words too for embedding", 200),
        ];
        note
    }

    #[tokio::test]
    async fn generate_embeddings_with_empty_changed_blocks_embeds_all() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = Enricher::new(provider);

        let parsed = create_test_parsed_note_with_content();

        let embeddings = service.generate_embeddings(&parsed, &[]).await.unwrap();

        assert_eq!(
            embeddings.len(),
            2,
            "Expected 2 embeddings for 2 paragraphs"
        );
        assert_eq!(embeddings[0].block_id, "block_0");
        assert_eq!(embeddings[1].block_id, "block_1");
    }

    #[tokio::test]
    async fn section_style_changed_blocks_embed_all() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = Enricher::new(provider);

        let parsed = create_test_parsed_note_with_content();

        let section_style_ids = vec![
            "modified_section_0".to_string(),
            "added_section_1".to_string(),
        ];

        let embeddings = service
            .generate_embeddings(&parsed, &section_style_ids)
            .await
            .unwrap();

        assert_eq!(
            embeddings.len(),
            2,
            "Expected 2 embeddings - section-style IDs should embed all blocks"
        );
    }

    #[tokio::test]
    async fn enrich_full_flow_without_embeddings() {
        let service = Enricher::without_embeddings();
        let parsed = create_test_parsed_note_with_content();

        let enriched = service.enrich(parsed, vec![]).await.unwrap();

        assert!(enriched.embeddings.is_empty());
        assert_eq!(enriched.metadata.language, Some("en".to_string()));
    }

    #[tokio::test]
    async fn enrich_full_flow_with_embeddings() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = Enricher::new(provider);
        let parsed = create_test_parsed_note_with_content();

        let enriched = service.enrich(parsed, vec![]).await.unwrap();

        assert_eq!(enriched.embeddings.len(), 2);
        // The stored name carries the backend, so a vector is never reused
        // for a provider that did not produce it.
        assert_eq!(enriched.embeddings[0].model, "mock/mock-model");
    }

    #[tokio::test]
    async fn generate_embeddings_batches_by_max_batch_size() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let mut service = Enricher::new(provider.clone());
        service.max_batch_size = 1;
        let parsed = create_test_parsed_note_with_content();

        let embeddings = service.generate_embeddings(&parsed, &[]).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(provider.batch_calls(), 2);
    }

    #[tokio::test]
    async fn generate_embeddings_propagates_mid_batch_failure() {
        let provider = Arc::new(MockEmbeddingProvider::with_failure_on_batch_call(2));
        let mut service = Enricher::new(provider);
        service.max_batch_size = 1;
        let parsed = create_test_parsed_note_with_three_paragraphs();

        let result = service.generate_embeddings(&parsed, &[]).await;
        assert!(result.is_err());
    }

    #[test]
    fn a_block_under_the_word_floor_is_not_embedded() {
        use crucible_core::parser::ParsedNoteBuilder;

        let service = Enricher::without_embeddings();
        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content.blocks = vec![para("Hi", 0), para("One two three four five six", 10)];

        let blocks = service.extract_block_texts(&note, &[]);

        assert_eq!(blocks.len(), 1, "the two-word block is skipped");
        assert_eq!(
            blocks[0].block_id, "block_1",
            "the id stays the block's rank in the document, so the skip leaves a hole"
        );
    }

    fn create_test_note_with_all_extractable_block_types() -> ParsedNote {
        use crucible_core::parser::types::BlockKind;
        use crucible_core::parser::ParsedNoteBuilder;

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/enrichment.md")).build();
        note.content.blocks = vec![
            heading(1, "Primary architecture heading context words", 0),
            heading(2, "Secondary execution heading context words", 40),
            heading(3, "Tertiary extraction heading context words", 80),
            para(
                "Paragraph content carries enough words for extraction checks",
                120,
            ),
            block(
                BlockKind::Code {
                    language: Some("rust".to_string()),
                },
                "fn demo_example() { let answer = 42; println!(\"{}\", answer); }",
                160,
            ),
            block(
                BlockKind::List { ordered: false },
                "First list item carries context Second list item keeps meaning",
                230,
            ),
            block(
                BlockKind::Blockquote,
                "Blockquote words stay visible with context",
                300,
            ),
        ];
        note
    }

    fn pairs(candidates: &[EmbedCandidate]) -> Vec<(String, String)> {
        candidates
            .iter()
            .map(|c| (c.block_id.clone(), c.text.clone()))
            .collect()
    }

    /// The seven ids and texts every block-level test below expects, in
    /// document order.
    fn expected_all_block_types() -> Vec<(String, String)> {
        let h1 = "Primary architecture heading context words";
        let h2 = "Secondary execution heading context words";
        let h3 = "Tertiary extraction heading context words";
        let deep = format!("enrichment > {h1} > {h2} > {h3}");

        vec![
            ("block_0".to_string(), format!("[enrichment > {h1}] {h1}")),
            (
                "block_1".to_string(),
                format!("[enrichment > {h1} > {h2}] {h2}"),
            ),
            ("block_2".to_string(), format!("[{deep}] {h3}")),
            (
                "block_3".to_string(),
                format!("[{deep}] Paragraph content carries enough words for extraction checks"),
            ),
            (
                "block_4".to_string(),
                format!(
                    "[{deep}] fn demo_example() {{ let answer = 42; println!(\"{{}}\", answer); }}"
                ),
            ),
            (
                "block_5".to_string(),
                format!("[{deep}] First list item carries context Second list item keeps meaning"),
            ),
            (
                "block_6".to_string(),
                format!("[{deep}] Blockquote words stay visible with context"),
            ),
        ]
    }

    #[test]
    fn every_block_kind_is_embedded_with_its_heading_trail() {
        let service = Enricher::without_embeddings();
        let note = create_test_note_with_all_extractable_block_types();

        let blocks = service.extract_block_texts(&note, &[]);

        assert_eq!(pairs(&blocks), expected_all_block_types());
    }

    #[test]
    fn naming_changed_blocks_embeds_only_those() {
        let service = Enricher::without_embeddings();
        let note = create_test_note_with_all_extractable_block_types();
        let changed: Vec<String> = (2..=6).map(|i| format!("block_{i}")).collect();

        let blocks = service.extract_block_texts(&note, &changed);

        assert_eq!(pairs(&blocks), expected_all_block_types()[2..].to_vec());
    }

    #[test]
    fn a_block_carries_the_breadcrumb_of_the_section_it_sits_in() {
        use crucible_core::parser::types::BlockKind;
        use crucible_core::parser::ParsedNoteBuilder;

        let enricher = Enricher::without_embeddings();
        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/guide.md")).build();
        note.content.blocks = vec![
            block(BlockKind::Heading { level: 1 }, "Guide", 0),
            block(BlockKind::Heading { level: 2 }, "Setup", 8),
            block(
                BlockKind::Paragraph,
                "install the toolchain before anything else",
                17,
            ),
            block(BlockKind::Heading { level: 2 }, "Usage", 60),
            block(
                BlockKind::Paragraph,
                "run the command with the flag you need",
                69,
            ),
        ];

        let blocks = enricher.extract_block_texts(&note, &[]);
        let by_id: std::collections::HashMap<&str, &str> = blocks
            .iter()
            .map(|c| (c.block_id.as_str(), c.text.as_str()))
            .collect();

        assert_eq!(
            by_id.get("block_2").copied(),
            Some("[guide > Guide > Setup] install the toolchain before anything else")
        );
        assert_eq!(
            by_id.get("block_4").copied(),
            Some("[guide > Guide > Usage] run the command with the flag you need"),
            "a block under the second H2 must not inherit the first H2"
        );
    }

    #[tokio::test]
    async fn each_block_embedding_carries_its_blocks_content_hash() {
        use crucible_core::parser::ParsedNoteBuilder;

        let provider = Arc::new(MockEmbeddingProvider::new());
        let enricher = Enricher::new(provider);
        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content.blocks = vec![
            para("Alpha has enough words here for embedding", 0),
            para("Beta has enough words here for embedding", 100),
        ];

        let embeddings = enricher.generate_embeddings(&note, &[]).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        for (embedding, block) in embeddings.iter().zip(&note.content.blocks) {
            assert_eq!(
                embedding.content_hash.as_deref(),
                Some(block.content_hash.to_hex().as_str()),
                "the reuse key must reach the embedding"
            );
        }
    }
}
