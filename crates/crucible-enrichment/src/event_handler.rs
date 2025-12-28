//! Event handler for embedding generation.
//!
//! The `EmbeddingHandler` subscribes to `NoteParsed` and `BlocksUpdated` events
//! to trigger embedding generation via the enrichment service. It acts as a thin
//! wrapper that connects the event system to the enrichment pipeline.
//!
//! # Event Subscriptions
//!
//! | Event | Action |
//! |-------|--------|
//! | `NoteParsed` | Trigger enrichment for parsed note |
//! | `BlocksUpdated` | Trigger re-embedding for updated blocks |
//!
//! # Design
//!
//! The handler delegates all work to the `EnrichmentService` trait. The service
//! is responsible for:
//! - Generating embeddings for content blocks
//! - Emitting `EmbeddingBatchComplete` events when done
//!
//! The handler simply extracts relevant information from events and calls the
//! service with appropriate parameters.

use crucible_core::enrichment::EnrichmentService;
use crucible_core::events::SessionEvent;
use crucible_core::ParsedNote;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Handler for triggering embedding generation in response to events.
///
/// Subscribes to note parsing and block update events to trigger the
/// enrichment pipeline. The actual embedding generation and event emission
/// is delegated to the wrapped `EnrichmentService`.
///
/// # Example
///
/// ```ignore
/// use crucible_enrichment::EmbeddingHandler;
/// use crucible_enrichment::create_default_enrichment_service;
///
/// // Create enrichment service
/// let service = create_default_enrichment_service(Some(embedding_provider))?;
///
/// // Create handler
/// let handler = EmbeddingHandler::new(service);
///
/// // Handle events
/// handler.handle_event(&event).await;
/// ```
pub struct EmbeddingHandler {
    /// The enrichment service that performs actual embedding generation
    service: Arc<dyn EnrichmentService>,
}

impl EmbeddingHandler {
    /// Create a new embedding handler.
    ///
    /// # Arguments
    ///
    /// * `service` - The enrichment service for embedding generation
    pub fn new(service: Arc<dyn EnrichmentService>) -> Self {
        Self { service }
    }

    /// Get a reference to the underlying enrichment service.
    pub fn service(&self) -> &Arc<dyn EnrichmentService> {
        &self.service
    }

    /// Handle a NoteParsed event.
    ///
    /// Triggers enrichment for the parsed note. The enrichment service will:
    /// 1. Generate embeddings for content blocks
    /// 2. Emit `EmbeddingBatchComplete` when done (if configured with an emitter)
    ///
    /// Note: This method requires the full ParsedNote to be available. If only
    /// the path is known, the note must be re-parsed or fetched from storage.
    pub async fn handle_note_parsed(&self, parsed_note: ParsedNote, changed_blocks: Vec<String>) {
        let path = parsed_note.path.clone();
        info!(
            path = %path.display(),
            changed_blocks = changed_blocks.len(),
            "Handling NoteParsed event for embedding generation"
        );

        match self.service.enrich(parsed_note, changed_blocks).await {
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
    /// This event indicates blocks have changed in the database. If we have
    /// access to the full parsed note, we can re-generate embeddings for the
    /// changed blocks.
    ///
    /// Note: In the current architecture, this handler may need access to a
    /// parser or storage layer to retrieve the full note content. For now,
    /// this is a placeholder that logs the event.
    pub async fn handle_blocks_updated(&self, entity_id: &str, block_count: usize) {
        debug!(
            entity_id = %entity_id,
            block_count = block_count,
            "BlocksUpdated event received - re-parsing may be needed for embedding update"
        );

        // TODO: In Phase 7 (runtime wiring), this handler will be connected to
        // a parser or storage layer that can retrieve the full ParsedNote.
        // For now, we just log the event.
        //
        // The actual re-embedding flow should be:
        // 1. Look up the entity in storage to get the file path
        // 2. Parse the file to get the ParsedNote
        // 3. Call service.enrich() with the changed block IDs
        //
        // Alternatively, the BlocksUpdated event could carry the ParsedNote
        // payload directly (at the cost of larger event payloads).
        warn!(
            entity_id = %entity_id,
            "BlocksUpdated handler not yet implemented - embeddings may be stale"
        );
    }

    /// Handle a SessionEvent by dispatching to the appropriate handler method.
    ///
    /// Currently handles:
    /// - `NoteParsed` -> logs only (requires ParsedNote payload)
    /// - `BlocksUpdated` -> `handle_blocks_updated`
    ///
    /// Note: The `NoteParsed` event as received here contains only path and
    /// block_count. For full enrichment, the handler needs access to the
    /// full `ParsedNote` which requires coordination with the parser or storage.
    ///
    /// For direct enrichment with a `ParsedNote`, use `handle_note_parsed` directly.
    pub async fn handle_event(&self, event: &SessionEvent) {
        match event {
            SessionEvent::NoteParsed {
                path,
                block_count,
                payload,
            } => {
                // Log the event - full enrichment requires the ParsedNote
                debug!(
                    path = %path.display(),
                    block_count = block_count,
                    has_payload = payload.is_some(),
                    "NoteParsed event received"
                );

                // If payload is present, we have metadata but not the full AST
                // For full enrichment, the caller should provide ParsedNote directly
                // via handle_note_parsed()
                if payload.is_some() {
                    info!(
                        path = %path.display(),
                        "NoteParsed has payload - enrichment requires full ParsedNote"
                    );
                }
            }
            SessionEvent::BlocksUpdated {
                entity_id,
                block_count,
            } => {
                self.handle_blocks_updated(entity_id, *block_count).await;
            }
            _ => {
                // Ignore other event types
            }
        }
    }

    /// Get the list of event types this handler processes.
    ///
    /// Useful for registering with an event system that supports filtering.
    pub fn handled_event_types() -> &'static [&'static str] {
        &["note_parsed", "blocks_updated"]
    }

    /// Get the recommended handler priority.
    ///
    /// Embedding handlers should run after storage handlers (which have priority 100)
    /// to ensure entities and blocks are persisted before embedding generation.
    pub const PRIORITY: i64 = 200;
}

/// Adapter wrapping EmbeddingHandler to implement the core Handler trait.
///
/// This allows EmbeddingHandler to be registered with the Reactor.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_enrichment::{EmbeddingHandler, EmbeddingHandlerAdapter};
/// use crucible_core::events::Reactor;
///
/// let handler = EmbeddingHandler::new(service);
/// let mut reactor = Reactor::new();
/// reactor.register(Box::new(EmbeddingHandlerAdapter::new(handler)))?;
/// ```
pub struct EmbeddingHandlerAdapter {
    inner: Arc<EmbeddingHandler>,
}

impl EmbeddingHandlerAdapter {
    /// Create a new adapter wrapping an EmbeddingHandler.
    pub fn new(handler: EmbeddingHandler) -> Self {
        Self {
            inner: Arc::new(handler),
        }
    }

    /// Create a new adapter from an Arc-wrapped EmbeddingHandler.
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
        // EmbeddingHandler depends on storage and tag handlers
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
        // Delegate to the inner handler
        self.inner.handle_event(&event).await;
        crucible_core::events::HandlerResult::ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::enrichment::{EnrichedNote, EnrichmentMetadata, InferredRelation};
    use crucible_core::parser::ParsedNoteBuilder;
    use crucible_core::ParsedNote;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock enrichment service for testing
    struct MockEnrichmentService {
        call_count: AtomicUsize,
    }

    impl MockEnrichmentService {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl EnrichmentService for MockEnrichmentService {
        async fn enrich(
            &self,
            parsed: ParsedNote,
            _changed_block_ids: Vec<String>,
        ) -> anyhow::Result<EnrichedNote> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(EnrichedNote {
                parsed,
                embeddings: Vec::new(),
                metadata: EnrichmentMetadata::default(),
                inferred_relations: Vec::new(),
            })
        }

        async fn enrich_with_tree(
            &self,
            parsed: ParsedNote,
            changed_block_ids: Vec<String>,
        ) -> anyhow::Result<EnrichedNote> {
            self.enrich(parsed, changed_block_ids).await
        }

        async fn infer_relations(
            &self,
            _enriched: &EnrichedNote,
            _threshold: f64,
        ) -> anyhow::Result<Vec<InferredRelation>> {
            Ok(Vec::new())
        }

        fn min_words_for_embedding(&self) -> usize {
            5
        }

        fn max_batch_size(&self) -> usize {
            10
        }

        fn has_embedding_provider(&self) -> bool {
            false
        }
    }

    fn create_test_parsed_note() -> ParsedNote {
        ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build()
    }

    #[tokio::test]
    async fn test_embedding_handler_creation() {
        let service = Arc::new(MockEnrichmentService::new());
        let handler = EmbeddingHandler::new(service.clone());

        assert!(Arc::ptr_eq(
            &handler.service,
            &(service as Arc<dyn EnrichmentService>)
        ));
    }

    #[tokio::test]
    async fn test_handle_note_parsed_calls_service() {
        let service = Arc::new(MockEnrichmentService::new());
        let handler = EmbeddingHandler::new(service.clone());

        let parsed = create_test_parsed_note();
        handler
            .handle_note_parsed(parsed, vec!["block_0".to_string()])
            .await;

        assert_eq!(service.call_count(), 1, "Service should be called once");
    }

    #[tokio::test]
    async fn test_handle_event_dispatches_blocks_updated() {
        let service = Arc::new(MockEnrichmentService::new());
        let handler = EmbeddingHandler::new(service.clone());

        let event = SessionEvent::BlocksUpdated {
            entity_id: "test_entity".to_string(),
            block_count: 5,
        };

        // This should not panic and should log appropriately
        handler.handle_event(&event).await;
    }

    #[tokio::test]
    async fn test_handle_event_ignores_other_events() {
        let service = Arc::new(MockEnrichmentService::new());
        let handler = EmbeddingHandler::new(service.clone());

        // Create an unrelated event
        let event = SessionEvent::SessionEnded {
            reason: "test".to_string(),
        };

        handler.handle_event(&event).await;

        // Service should not be called
        assert_eq!(service.call_count(), 0);
    }

    #[test]
    fn test_handled_event_types() {
        let types = EmbeddingHandler::handled_event_types();
        assert!(types.contains(&"note_parsed"));
        assert!(types.contains(&"blocks_updated"));
    }

    #[test]
    fn test_priority_constant() {
        assert_eq!(EmbeddingHandler::PRIORITY, 200);
    }
}
