//! Note enrichment.
//!
//! Turns a `ParsedNote` + list of changed block IDs into an `EnrichedNote`
//! ready for storage. Runs embedding generation and metadata extraction in
//! parallel.

use super::types::{BlockEmbedding, EnrichmentMetadata};
use anyhow::Result;
use crucible_core::enrichment::{EmbeddingProvider, EnrichedNote};
use crucible_core::events::{InternalSessionEvent, SessionEvent, SharedEventBus};
use crucible_core::ParsedNote;
use std::sync::Arc;
use tracing::{debug, info};

/// Default minimum word count for generating embeddings
pub const DEFAULT_MIN_WORDS_FOR_EMBEDDING: usize = 5;

/// Default maximum batch size for embedding generation
pub const DEFAULT_MAX_BATCH_SIZE: usize = 10;

/// Enriches parsed notes with embeddings and metadata.
pub struct Enricher {
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    min_words_for_embedding: usize,
    max_batch_size: usize,
    emitter: Option<SharedEventBus<SessionEvent>>,
}

struct BlockCandidate {
    block_id: String,
    word_count: usize,
    offset: usize,
    text: String,
}

impl Enricher {
    /// Create an enricher with an embedding provider.
    pub fn new(embedding_provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            embedding_provider: Some(embedding_provider),
            min_words_for_embedding: DEFAULT_MIN_WORDS_FOR_EMBEDDING,
            max_batch_size: DEFAULT_MAX_BATCH_SIZE,
            emitter: None,
        }
    }

    /// Create an enricher without embeddings (metadata only).
    pub fn without_embeddings() -> Self {
        Self {
            embedding_provider: None,
            min_words_for_embedding: DEFAULT_MIN_WORDS_FOR_EMBEDDING,
            max_batch_size: DEFAULT_MAX_BATCH_SIZE,
            emitter: None,
        }
    }

    /// Create an enricher, using embeddings if a provider is supplied.
    pub fn from_optional_provider(provider: Option<Arc<dyn EmbeddingProvider>>) -> Self {
        match provider {
            Some(p) => Self::new(p),
            None => Self::without_embeddings(),
        }
    }

    #[allow(dead_code)] // builder API, exercised by tests
    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.min_words_for_embedding = min_words;
        self
    }

    #[allow(dead_code)] // builder API, exercised by tests
    pub fn with_max_batch_size(mut self, max_batch_size: usize) -> Self {
        self.max_batch_size = max_batch_size;
        self
    }

    /// Attach an event bus so batch-complete events reach subscribers.
    #[allow(dead_code)] // builder API, completes configuration surface
    pub fn with_emitter(mut self, emitter: SharedEventBus<SessionEvent>) -> Self {
        self.emitter = Some(emitter);
        self
    }

    pub fn min_words_for_embedding(&self) -> usize {
        self.min_words_for_embedding
    }

    pub fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }

    pub fn has_embedding_provider(&self) -> bool {
        self.embedding_provider.is_some()
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
        use std::time::Instant;

        info!(
            "Enriching note: {} ({} changed blocks)",
            parsed.path.display(),
            changed_blocks.len()
        );

        let embed_start = Instant::now();

        let (embeddings, metadata) = tokio::join!(
            self.generate_embeddings(&parsed, &changed_blocks),
            self.extract_metadata(&parsed),
        );

        let embeddings = embeddings?;
        let embed_duration = embed_start.elapsed();

        if !embeddings.is_empty() {
            if let Some(ref emitter) = self.emitter {
                let entity_id = format!("note:{}", parsed.path.display());
                drop(emitter.emit(SessionEvent::internal(
                    InternalSessionEvent::EmbeddingBatchComplete {
                        entity_id,
                        count: embeddings.len(),
                        duration_ms: embed_duration.as_millis() as u64,
                    },
                )));
                debug!(
                    "Emitted EmbeddingBatchComplete for {} embeddings in {}ms",
                    embeddings.len(),
                    embed_duration.as_millis()
                );
            }
        }

        Ok(EnrichedNote::new(parsed, embeddings, metadata?))
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

        let model_name = provider.model_name();

        info!(
            "Generating embeddings for {} blocks (batches of {})",
            block_texts.len(),
            self.max_batch_size
        );

        let mut all_embeddings = Vec::new();

        for (batch_idx, chunk) in block_texts.chunks(self.max_batch_size).enumerate() {
            debug!(
                "Processing batch {} ({} blocks)",
                batch_idx + 1,
                chunk.len()
            );

            let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();
            let block_ids: Vec<&String> = chunk.iter().map(|(id, _)| id).collect();

            let vectors = provider.embed_batch(&texts).await?;

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

    fn extract_block_texts(
        &self,
        parsed: &ParsedNote,
        changed_blocks: &[String],
    ) -> Vec<(String, String)> {
        let mut blocks = Vec::new();

        let breadcrumbs = build_breadcrumbs(parsed);

        // Embed everything when caller passes no block IDs, or when the IDs
        // are section-style (from the pipeline's diff layer) rather than
        // concrete block IDs.
        let embed_all = changed_blocks.is_empty()
            || changed_blocks.iter().any(|id| {
                id.starts_with("modified_section")
                    || id.starts_with("added_section")
                    || id.starts_with("removed_section")
            });

        self.process_block_candidates(
            &mut blocks,
            parsed
                .content
                .headings
                .iter()
                .enumerate()
                .map(|(idx, heading)| BlockCandidate {
                    block_id: format!("heading_{}", idx),
                    word_count: heading.text.split_whitespace().count(),
                    offset: heading.offset,
                    text: heading.text.clone(),
                })
                .collect(),
            changed_blocks,
            embed_all,
            &breadcrumbs,
        );

        self.process_block_candidates(
            &mut blocks,
            parsed
                .content
                .paragraphs
                .iter()
                .enumerate()
                .map(|(idx, paragraph)| BlockCandidate {
                    block_id: format!("paragraph_{}", idx),
                    word_count: paragraph.word_count,
                    offset: paragraph.offset,
                    text: paragraph.content.clone(),
                })
                .collect(),
            changed_blocks,
            embed_all,
            &breadcrumbs,
        );

        self.process_block_candidates(
            &mut blocks,
            parsed
                .content
                .code_blocks
                .iter()
                .enumerate()
                .map(|(idx, code_block)| BlockCandidate {
                    block_id: format!("code_{}", idx),
                    word_count: code_block.content.split_whitespace().count(),
                    offset: code_block.offset,
                    text: code_block.content.clone(),
                })
                .collect(),
            changed_blocks,
            embed_all,
            &breadcrumbs,
        );

        self.process_block_candidates(
            &mut blocks,
            parsed
                .content
                .lists
                .iter()
                .enumerate()
                .map(|(idx, list)| {
                    let text = list
                        .items
                        .iter()
                        .map(|item| item.content.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    BlockCandidate {
                        block_id: format!("list_{}", idx),
                        word_count: text.split_whitespace().count(),
                        offset: list.offset,
                        text,
                    }
                })
                .collect(),
            changed_blocks,
            embed_all,
            &breadcrumbs,
        );

        self.process_block_candidates(
            &mut blocks,
            parsed
                .content
                .blockquotes
                .iter()
                .enumerate()
                .map(|(idx, blockquote)| BlockCandidate {
                    block_id: format!("blockquote_{}", idx),
                    word_count: blockquote.content.split_whitespace().count(),
                    offset: blockquote.offset,
                    text: blockquote.content.clone(),
                })
                .collect(),
            changed_blocks,
            embed_all,
            &breadcrumbs,
        );

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

    fn process_block_candidates(
        &self,
        blocks: &mut Vec<(String, String)>,
        candidates: Vec<BlockCandidate>,
        changed_blocks: &[String],
        embed_all: bool,
        breadcrumbs: &std::collections::HashMap<usize, String>,
    ) {
        for candidate in candidates {
            if !embed_all && !changed_blocks.contains(&candidate.block_id) {
                continue;
            }

            if candidate.word_count < self.min_words_for_embedding {
                continue;
            }

            let context = breadcrumbs
                .get(&candidate.offset)
                .cloned()
                .unwrap_or_default();
            let text_with_context = if context.is_empty() {
                candidate.text
            } else {
                format!("[{}] {}", context, candidate.text)
            };
            blocks.push((candidate.block_id, text_with_context));
        }
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

/// Build breadcrumbs (heading hierarchy) for all content positions.
///
/// Maps byte offsets to paths like `Filename > H1 > H2 > ...`, letting blocks
/// include their hierarchical context in embeddings without triggering cascade
/// re-embeddings when a heading changes.
fn build_breadcrumbs(parsed: &ParsedNote) -> std::collections::HashMap<usize, String> {
    use std::collections::HashMap;

    let mut breadcrumbs = HashMap::new();
    let mut heading_stack: Vec<(u8, &str, usize)> = Vec::new(); // (level, text, start_offset)

    let filename = parsed
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown");

    for heading in &parsed.content.headings {
        while let Some(&(stack_level, _, _)) = heading_stack.last() {
            if stack_level >= heading.level {
                heading_stack.pop();
            } else {
                break;
            }
        }

        heading_stack.push((heading.level, &heading.text, heading.offset));

        let mut path_parts: Vec<&str> = vec![filename];
        path_parts.extend(heading_stack.iter().map(|(_, text, _)| *text));
        let breadcrumb = path_parts.join(" > ");

        breadcrumbs.insert(heading.offset, breadcrumb.clone());

        let next_offset = parsed
            .content
            .headings
            .iter()
            .find(|h| h.offset > heading.offset)
            .map(|h| h.offset)
            .unwrap_or(usize::MAX);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::{EmitOutcome, EventEmitter};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockEmbeddingProvider {
        model: String,
        dimensions: usize,
        embed_batch_calls: AtomicUsize,
        fail_on_batch_call: Option<usize>,
    }

    impl MockEmbeddingProvider {
        fn new() -> Self {
            Self {
                model: "mock-model".to_string(),
                dimensions: 3,
                embed_batch_calls: AtomicUsize::new(0),
                fail_on_batch_call: None,
            }
        }

        fn with_failure_on_batch_call(fail_on_batch_call: usize) -> Self {
            Self {
                model: "mock-model".to_string(),
                dimensions: 3,
                embed_batch_calls: AtomicUsize::new(0),
                fail_on_batch_call: Some(fail_on_batch_call),
            }
        }

        fn batch_calls(&self) -> usize {
            self.embed_batch_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            let call_idx = self.embed_batch_calls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.fail_on_batch_call == Some(call_idx) {
                anyhow::bail!("forced embed_batch failure on call {call_idx}");
            }
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn model_name(&self) -> &str {
            &self.model
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec![self.model.clone()])
        }
    }

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
    async fn with_min_words_overrides_default() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = Enricher::new(provider).with_min_words(10);

        assert_eq!(service.min_words_for_embedding, 10);
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

    fn create_test_parsed_note_with_content() -> ParsedNote {
        use crucible_core::parser::{Paragraph, ParsedNoteBuilder};

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();

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

    fn create_test_parsed_note_with_three_paragraphs() -> ParsedNote {
        use crucible_core::parser::{Paragraph, ParsedNoteBuilder};

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content.paragraphs.push(Paragraph::new(
            "Paragraph one has more than five words for embedding".to_string(),
            0,
        ));
        note.content.paragraphs.push(Paragraph::new(
            "Paragraph two also has enough words for embedding".to_string(),
            10,
        ));
        note.content.paragraphs.push(Paragraph::new(
            "Paragraph three has enough words too for embedding".to_string(),
            20,
        ));
        note
    }

    #[derive(Default)]
    struct RecordingEmitter {
        emitted: std::sync::Mutex<Vec<SessionEvent>>,
    }

    #[async_trait::async_trait]
    impl EventEmitter for RecordingEmitter {
        type Event = SessionEvent;

        async fn emit(
            &self,
            event: Self::Event,
        ) -> crucible_core::events::EmitResult<EmitOutcome<Self::Event>> {
            self.emitted.lock().unwrap().push(event.clone());
            Ok(EmitOutcome::new(event))
        }
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
        assert_eq!(embeddings[0].block_id, "paragraph_0");
        assert_eq!(embeddings[1].block_id, "paragraph_1");
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
        assert_eq!(enriched.embeddings[0].model, "mock-model");
    }

    #[tokio::test]
    async fn generate_embeddings_batches_by_max_batch_size() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = Enricher::new(provider.clone()).with_max_batch_size(1);
        let parsed = create_test_parsed_note_with_content();

        let embeddings = service.generate_embeddings(&parsed, &[]).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(provider.batch_calls(), 2);
    }

    #[tokio::test]
    async fn generate_embeddings_propagates_mid_batch_failure() {
        let provider = Arc::new(MockEmbeddingProvider::with_failure_on_batch_call(2));
        let service = Enricher::new(provider).with_max_batch_size(1);
        let parsed = create_test_parsed_note_with_three_paragraphs();

        let result = service.generate_embeddings(&parsed, &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn enrich_with_emitter_keeps_enrichment_successful() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let recording_emitter = Arc::new(RecordingEmitter::default());
        let service = Enricher::new(provider)
            .with_emitter(recording_emitter.clone() as Arc<dyn EventEmitter<Event = SessionEvent>>);
        let parsed = create_test_parsed_note_with_content();

        let enriched = service.enrich(parsed, vec![]).await.unwrap();
        assert_eq!(enriched.embeddings.len(), 2);
        assert!(recording_emitter.emitted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn enrich_skips_emitter_when_no_embeddings() {
        let recording_emitter = Arc::new(RecordingEmitter::default());
        let service = Enricher::without_embeddings()
            .with_emitter(recording_emitter.clone() as Arc<dyn EventEmitter<Event = SessionEvent>>);
        let parsed = create_test_parsed_note_with_content();

        let enriched = service.enrich(parsed, vec![]).await.unwrap();
        assert!(enriched.embeddings.is_empty());
        assert!(recording_emitter.emitted.lock().unwrap().is_empty());
    }

    #[test]
    fn build_breadcrumbs_with_headings() {
        use crucible_core::parser::{Heading, Paragraph, ParsedNoteBuilder};

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content
            .headings
            .push(Heading::new(1, "Introduction", 0));
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
    fn build_breadcrumbs_empty_note() {
        let note = create_test_parsed_note();
        let crumbs = build_breadcrumbs(&note);
        assert!(crumbs.is_empty());
    }

    #[test]
    fn extract_block_texts_skips_short_blocks() {
        use crucible_core::parser::{Paragraph, ParsedNoteBuilder};

        let service = Enricher::without_embeddings();
        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content
            .paragraphs
            .push(Paragraph::new("Hi".to_string(), 0));
        note.content.paragraphs.push(Paragraph::new(
            "One two three four five six".to_string(),
            10,
        ));

        let blocks = service.extract_block_texts(&note, &[]);
        assert_eq!(blocks.len(), 1, "short paragraph should be skipped");
        assert_eq!(blocks[0].0, "paragraph_1");
    }

    fn create_test_note_with_all_extractable_block_types() -> ParsedNote {
        use crucible_core::parser::{
            Blockquote, CodeBlock, Heading, ListBlock, ListItem, ListType, Paragraph,
            ParsedNoteBuilder,
        };

        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/enrichment.md")).build();

        note.content.headings.push(Heading::new(
            1,
            "Primary architecture heading context words",
            0,
        ));
        note.content.headings.push(Heading::new(
            2,
            "Secondary execution heading context words",
            40,
        ));
        note.content.headings.push(Heading::new(
            3,
            "Tertiary extraction heading context words",
            80,
        ));

        note.content.paragraphs.push(Paragraph::new(
            "Paragraph content carries enough words for extraction checks".to_string(),
            120,
        ));

        note.content.code_blocks.push(CodeBlock::new(
            Some("rust".to_string()),
            "fn demo_example() { let answer = 42; println!(\"{}\", answer); }".to_string(),
            160,
        ));

        let mut list = ListBlock::new(ListType::Unordered, 220);
        list.add_item(ListItem::new(
            "First list item carries context".to_string(),
            0,
        ));
        list.add_item(ListItem::new(
            "Second list item keeps meaning".to_string(),
            0,
        ));
        note.content.lists.push(list);

        note.content.blockquotes.push(Blockquote::new(
            "Blockquote words stay visible with context".to_string(),
            280,
        ));

        note
    }

    #[test]
    fn extract_block_texts_all_block_types_with_context() {
        let service = Enricher::without_embeddings();
        let note = create_test_note_with_all_extractable_block_types();

        let blocks = service.extract_block_texts(&note, &[]);

        let h1 = "Primary architecture heading context words";
        let h2 = "Secondary execution heading context words";
        let h3 = "Tertiary extraction heading context words";
        let deep_context = format!("enrichment > {h1} > {h2} > {h3}");

        let expected = vec![
            (
                "heading_0".to_string(),
                format!("[enrichment > {h1}] {h1}"),
            ),
            (
                "heading_1".to_string(),
                format!("[enrichment > {h1} > {h2}] {h2}"),
            ),
            (
                "heading_2".to_string(),
                format!("[{deep_context}] {h3}"),
            ),
            (
                "paragraph_0".to_string(),
                format!("[{deep_context}] Paragraph content carries enough words for extraction checks"),
            ),
            (
                "code_0".to_string(),
                format!("[{deep_context}] fn demo_example() {{ let answer = 42; println!(\"{{}}\", answer); }}"),
            ),
            (
                "list_0".to_string(),
                format!("[{deep_context}] First list item carries context Second list item keeps meaning"),
            ),
            (
                "blockquote_0".to_string(),
                format!("[{deep_context}] Blockquote words stay visible with context"),
            ),
        ];

        assert_eq!(blocks, expected);
    }

    #[test]
    fn extract_block_texts_respects_changed_block_ids() {
        let service = Enricher::without_embeddings();
        let note = create_test_note_with_all_extractable_block_types();
        let changed_blocks = vec![
            "heading_2".to_string(),
            "paragraph_0".to_string(),
            "code_0".to_string(),
            "list_0".to_string(),
            "blockquote_0".to_string(),
        ];

        let blocks = service.extract_block_texts(&note, &changed_blocks);

        let h1 = "Primary architecture heading context words";
        let h2 = "Secondary execution heading context words";
        let h3 = "Tertiary extraction heading context words";
        let deep_context = format!("enrichment > {h1} > {h2} > {h3}");

        let expected = vec![
            (
                "heading_2".to_string(),
                format!("[{deep_context}] {h3}"),
            ),
            (
                "paragraph_0".to_string(),
                format!("[{deep_context}] Paragraph content carries enough words for extraction checks"),
            ),
            (
                "code_0".to_string(),
                format!("[{deep_context}] fn demo_example() {{ let answer = 42; println!(\"{{}}\", answer); }}"),
            ),
            (
                "list_0".to_string(),
                format!("[{deep_context}] First list item carries context Second list item keeps meaning"),
            ),
            (
                "blockquote_0".to_string(),
                format!("[{deep_context}] Blockquote words stay visible with context"),
            ),
        ];

        assert_eq!(blocks, expected);
    }

    #[test]
    fn builder_with_max_batch_size_stores_value() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = Enricher::new(provider).with_max_batch_size(5);
        assert_eq!(service.max_batch_size, 5);
    }
}
