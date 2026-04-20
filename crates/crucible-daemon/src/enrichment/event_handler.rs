//! Event handler for embedding generation.
//!
//! Subscribes to `NoteParsed` and `BlocksUpdated` events to trigger embedding
//! generation via the `Enricher`. Thin wrapper connecting the event system to
//! the enrichment pipeline.

use super::Enricher;
use crucible_core::events::{InternalSessionEvent, SessionEvent};
use crucible_core::ParsedNote;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Handler for triggering embedding generation in response to events.
pub struct EmbeddingHandler {
    enricher: Arc<Enricher>,
}

impl EmbeddingHandler {
    pub fn new(enricher: Arc<Enricher>) -> Self {
        Self { enricher }
    }

    pub fn enricher(&self) -> &Arc<Enricher> {
        &self.enricher
    }

    /// Handle a NoteParsed event.
    ///
    /// Triggers enrichment for the parsed note. The enricher will:
    /// 1. Generate embeddings for content blocks
    /// 2. Emit `EmbeddingBatchComplete` when done (if configured with an emitter)
    pub async fn handle_note_parsed(&self, parsed_note: ParsedNote, changed_blocks: Vec<String>) {
        let path = parsed_note.path.clone();
        info!(
            path = %path.display(),
            changed_blocks = changed_blocks.len(),
            "Handling NoteParsed event for embedding generation"
        );

        match self.enricher.enrich(parsed_note, changed_blocks).await {
            Ok(enriched) => {
                let embedding_count = enriched.embeddings.len();
                debug!(
                    path = %path.display(),
                    embeddings = embedding_count,
                    "Enrichment completed successfully"
                );
            }
            Err(e) => {
                error!(
                    path = %path.display(),
                    error = %e,
                    "Failed to enrich note"
                );
            }
        }
    }

    /// Handle a BlocksUpdated event.
    ///
    /// Placeholder: full re-embedding needs access to the parsed note, which
    /// this handler doesn't have. See the comment in the body.
    pub async fn handle_blocks_updated(&self, entity_id: &str, block_count: usize) {
        debug!(
            entity_id = %entity_id,
            block_count = block_count,
            "BlocksUpdated event received - re-parsing may be needed for embedding update"
        );

        // The BlocksUpdated flow needs the full ParsedNote. Two options when we
        // get around to it: look up the entity in storage and re-parse, or
        // carry the ParsedNote on the event payload.
        warn!(
            entity_id = %entity_id,
            "BlocksUpdated handler not yet implemented - embeddings may be stale"
        );
    }

    /// Handle a SessionEvent by dispatching to the appropriate method.
    pub async fn handle_event(&self, event: &SessionEvent) {
        let SessionEvent::Internal(inner) = event else {
            return;
        };
        match inner.as_ref() {
            InternalSessionEvent::NoteParsed {
                path,
                block_count,
                payload,
            } => {
                debug!(
                    path = %path.display(),
                    block_count = block_count,
                    has_payload = payload.is_some(),
                    "NoteParsed event received"
                );

                // NoteParsed carries path + block_count but not the AST;
                // callers with a ParsedNote in hand should invoke
                // handle_note_parsed directly.
                if payload.is_some() {
                    info!(
                        path = %path.display(),
                        "NoteParsed has payload - enrichment requires full ParsedNote"
                    );
                }
            }
            InternalSessionEvent::BlocksUpdated {
                entity_id,
                block_count,
            } => {
                self.handle_blocks_updated(entity_id, *block_count).await;
            }
            _ => {}
        }
    }

    pub fn handled_event_types() -> &'static [&'static str] {
        &["note_parsed", "blocks_updated"]
    }

    /// Handler priority: runs after storage handlers (100) so entities and
    /// blocks are persisted before embedding kicks off.
    pub const PRIORITY: i64 = 200;
}

/// Adapter wrapping EmbeddingHandler to implement the core Handler trait.
pub struct EmbeddingHandlerAdapter {
    inner: Arc<EmbeddingHandler>,
}

impl EmbeddingHandlerAdapter {
    pub fn new(handler: EmbeddingHandler) -> Self {
        Self {
            inner: Arc::new(handler),
        }
    }

    pub fn from_arc(handler: Arc<EmbeddingHandler>) -> Self {
        Self { inner: handler }
    }
}

#[async_trait::async_trait]
impl crucible_core::events::Handler for EmbeddingHandlerAdapter {
    fn name(&self) -> &str {
        "embedding_handler"
    }

    fn priority(&self) -> i32 {
        EmbeddingHandler::PRIORITY as i32
    }

    fn dependencies(&self) -> &[&str] {
        &["storage_handler", "tag_handler"]
    }

    fn event_pattern(&self) -> &str {
        "*"
    }

    async fn handle(
        &self,
        _ctx: &mut crucible_core::events::HandlerContext,
        event: SessionEvent,
    ) -> crucible_core::events::HandlerResult<SessionEvent> {
        self.inner.handle_event(&event).await;
        crucible_core::events::HandlerResult::ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::parser::ParsedNoteBuilder;
    use crucible_core::ParsedNote;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Embedding provider that counts calls; used to observe that the enricher
    /// is actually being invoked.
    struct CountingProvider {
        calls: AtomicUsize,
        fail: bool,
    }

    impl CountingProvider {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail: true,
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for CountingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![0.1; 3])
        }

        async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                anyhow::bail!("intentional embed failure");
            }
            Ok(texts.iter().map(|_| vec![0.1; 3]).collect())
        }

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        async fn list_models(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["mock-model".to_string()])
        }
    }

    fn create_test_parsed_note() -> ParsedNote {
        ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build()
    }

    fn create_parsed_note_with_paragraph() -> ParsedNote {
        use crucible_core::parser::Paragraph;
        let mut note = ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build();
        note.content.paragraphs.push(Paragraph::new(
            "Paragraph with enough words to embed correctly".to_string(),
            0,
        ));
        note
    }

    #[tokio::test]
    async fn embedding_handler_exposes_inner_enricher() {
        let enricher = Arc::new(Enricher::without_embeddings());
        let handler = EmbeddingHandler::new(enricher.clone());

        assert!(Arc::ptr_eq(handler.enricher(), &enricher));
    }

    #[tokio::test]
    async fn handle_note_parsed_invokes_enricher() {
        let provider = Arc::new(CountingProvider::new());
        let enricher = Arc::new(Enricher::new(provider.clone()));
        let handler = EmbeddingHandler::new(enricher);

        // Empty changed_blocks means embed-all, so the paragraph is picked up.
        let parsed = create_parsed_note_with_paragraph();
        handler.handle_note_parsed(parsed, vec![]).await;

        assert_eq!(provider.call_count(), 1, "provider should be called once");
    }

    #[tokio::test]
    async fn handle_event_dispatches_blocks_updated() {
        let enricher = Arc::new(Enricher::without_embeddings());
        let handler = EmbeddingHandler::new(enricher);

        let event = SessionEvent::Internal(Box::new(InternalSessionEvent::BlocksUpdated {
            entity_id: "test_entity".to_string(),
            block_count: 5,
        }));

        handler.handle_event(&event).await;
    }

    #[tokio::test]
    async fn handle_event_ignores_other_events() {
        let provider = Arc::new(CountingProvider::new());
        let enricher = Arc::new(Enricher::new(provider.clone()));
        let handler = EmbeddingHandler::new(enricher);

        let event = SessionEvent::SessionEnded {
            reason: "test".to_string(),
        };

        handler.handle_event(&event).await;

        assert_eq!(provider.call_count(), 0);
    }

    #[test]
    fn handled_event_types_lists_both() {
        let types = EmbeddingHandler::handled_event_types();
        assert!(types.contains(&"note_parsed"));
        assert!(types.contains(&"blocks_updated"));
    }

    #[test]
    fn priority_constant() {
        assert_eq!(EmbeddingHandler::PRIORITY, 200);
    }

    #[test]
    fn adapter_name() {
        let handler = EmbeddingHandler::new(Arc::new(Enricher::without_embeddings()));
        let adapter = EmbeddingHandlerAdapter::new(handler);
        assert_eq!(
            crucible_core::events::Handler::name(&adapter),
            "embedding_handler"
        );
    }

    #[test]
    fn adapter_priority() {
        let handler = EmbeddingHandler::new(Arc::new(Enricher::without_embeddings()));
        let adapter = EmbeddingHandlerAdapter::new(handler);
        assert_eq!(crucible_core::events::Handler::priority(&adapter), 200);
    }

    #[test]
    fn adapter_dependencies() {
        let handler = EmbeddingHandler::new(Arc::new(Enricher::without_embeddings()));
        let adapter = EmbeddingHandlerAdapter::new(handler);
        let deps = crucible_core::events::Handler::dependencies(&adapter);
        assert!(deps.contains(&"storage_handler"));
        assert!(deps.contains(&"tag_handler"));
    }

    #[test]
    fn adapter_event_pattern() {
        let handler = EmbeddingHandler::new(Arc::new(Enricher::without_embeddings()));
        let adapter = EmbeddingHandlerAdapter::new(handler);
        assert_eq!(crucible_core::events::Handler::event_pattern(&adapter), "*");
    }

    #[tokio::test]
    async fn handle_note_parsed_failure_no_panic() {
        let provider = Arc::new(CountingProvider::failing());
        let enricher = Arc::new(Enricher::new(provider));
        let handler = EmbeddingHandler::new(enricher);

        let parsed = create_parsed_note_with_paragraph();
        handler
            .handle_note_parsed(parsed, vec!["block_0".to_string()])
            .await;
    }

    #[tokio::test]
    async fn handle_note_parsed_skips_when_no_blocks() {
        // Empty parsed note has no blocks to embed; provider should not be invoked.
        let provider = Arc::new(CountingProvider::new());
        let enricher = Arc::new(Enricher::new(provider.clone()));
        let handler = EmbeddingHandler::new(enricher);

        handler
            .handle_note_parsed(create_test_parsed_note(), vec![])
            .await;

        assert_eq!(provider.call_count(), 0);
    }
}
