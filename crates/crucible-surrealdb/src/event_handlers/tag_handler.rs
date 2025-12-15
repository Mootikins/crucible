//! Tag handler for processing entity tags.
//!
//! This handler subscribes to `NoteParsed` events to extract and associate tags
//! with entities in the database. It also handles cleanup when entities are deleted.
//!
//! # Event Subscriptions
//!
//! | Event | Action | Emits |
//! |-------|--------|-------|
//! | `NoteParsed` | Extract + upsert tags | `TagAssociated` (one per tag) |
//! | `EntityDeleted` | Remove tag associations | (none) |
//!
//! # Priority
//!
//! TagHandler runs at priority 110 (after StorageHandler at 100) to ensure
//! the entity exists before associating tags with it.

use crucible_core::events::{EntityType as EventEntityType, EventEmitter, SessionEvent, SharedEventBus};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error};

use crate::eav_graph::types::RecordId;
use crate::eav_graph::EAVGraphStore;

/// Handler for managing tag associations with entities.
///
/// Subscribes to parse events to extract and store tags, and cleanup events
/// to remove tag associations when entities are deleted.
pub struct TagHandler {
    /// Reference to the EAV graph store
    store: Arc<EAVGraphStore>,
    /// Event emitter for emitting tag events
    emitter: SharedEventBus<SessionEvent>,
}

impl TagHandler {
    /// Create a new tag handler.
    ///
    /// # Arguments
    ///
    /// * `store` - The EAV graph store for database operations
    /// * `emitter` - Event emitter for emitting tag events
    pub fn new(store: Arc<EAVGraphStore>, emitter: SharedEventBus<SessionEvent>) -> Self {
        Self { store, emitter }
    }

    /// Get reference to the store.
    pub fn store(&self) -> &Arc<EAVGraphStore> {
        &self.store
    }

    /// Get reference to the emitter.
    pub fn emitter(&self) -> &SharedEventBus<SessionEvent> {
        &self.emitter
    }

    /// Handle a NoteParsed event to extract and associate tags.
    ///
    /// Note: In a full implementation, this would receive the parsed note
    /// with tags. For now, this is a placeholder that demonstrates the pattern.
    /// The actual tag extraction would happen in the parser, and tags would
    /// be passed in the event payload.
    pub async fn handle_note_parsed(&self, path: &Path, _block_count: usize) {
        let entity_id = self.path_to_entity_id(path);
        debug!(entity_id = %entity_id, path = %path.display(), "TagHandler: Processing NoteParsed event");

        // In a full implementation:
        // 1. Tags would be extracted from the parsed note content
        // 2. For each tag, upsert the tag entity
        // 3. Associate the tag with the note entity
        //
        // For now, we just log that we received the event.
        // The actual implementation requires the parsed note data
        // which would be added to the NoteParsed event payload.

        debug!(entity_id = %entity_id, "TagHandler: Tag processing would happen here");
    }

    /// Associate tags with an entity and emit TagAssociated events.
    ///
    /// This method is called when tags are extracted from a parsed note.
    /// For each tag, it emits a `TagAssociated` event.
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The entity identifier to associate tags with
    /// * `tags` - List of tag names (without # prefix)
    ///
    /// # Example
    ///
    /// ```ignore
    /// handler.associate_tags("note_example_md", &["rust", "programming"]).await;
    /// ```
    pub async fn associate_tags(&self, entity_id: &str, tags: &[String]) {
        for tag in tags {
            debug!(entity_id = %entity_id, tag = %tag, "Associating tag with entity");

            // Emit TagAssociated event
            let event = SessionEvent::TagAssociated {
                entity_id: entity_id.to_string(),
                tag: tag.clone(),
            };

            if let Err(e) = self.emitter.emit(event).await {
                error!(error = %e, entity_id = %entity_id, tag = %tag, "Failed to emit TagAssociated event");
            }
        }
    }

    /// Handle an EntityDeleted event to clean up tag associations.
    ///
    /// Removes all tag associations for the deleted entity.
    /// Does not delete the tags themselves (they may be used by other entities).
    pub async fn handle_entity_deleted(&self, entity_id: &str, entity_type: &EventEntityType) {
        debug!(entity_id = %entity_id, entity_type = ?entity_type, "TagHandler: Processing EntityDeleted event");

        // Only process Note deletions
        if !matches!(entity_type, EventEntityType::Note) {
            return;
        }

        let record_id = RecordId::new("entities", entity_id);

        match self.store.delete_entity_tags(&record_id).await {
            Ok(_) => {
                debug!(entity_id = %entity_id, "Tag associations removed successfully");
            }
            Err(e) => {
                error!(error = %e, entity_id = %entity_id, "Failed to remove tag associations");
            }
        }
    }

    /// Handle a SessionEvent by dispatching to the appropriate handler method.
    ///
    /// This method can be called by any event system (EventBus, reactor, etc.)
    /// to process events. It handles:
    /// - `NoteParsed` -> `handle_note_parsed`
    /// - `EntityDeleted` -> `handle_entity_deleted`
    ///
    /// Other events are ignored.
    ///
    /// # Priority
    ///
    /// This handler should be registered at priority 110 (after StorageHandler)
    /// to ensure entities exist before associating tags.
    pub async fn handle_event(&self, event: &SessionEvent) {
        match event {
            SessionEvent::NoteParsed { path, block_count, .. } => {
                self.handle_note_parsed(path, *block_count).await;
            }
            SessionEvent::EntityDeleted { entity_id, entity_type } => {
                self.handle_entity_deleted(entity_id, entity_type).await;
            }
            _ => {
                // Ignore other event types
            }
        }
    }

    /// Convert a file path to an entity ID.
    fn path_to_entity_id(&self, path: &Path) -> String {
        path.display().to_string().replace(['/', '\\'], "_")
    }

    /// Get the list of event types this handler processes.
    pub fn handled_event_types() -> &'static [&'static str] {
        &["note_parsed", "entity_deleted"]
    }

    /// Get the recommended handler priority.
    ///
    /// Tag handlers should run after storage handlers (priority 100) to ensure
    /// entities exist before associating tags.
    pub const PRIORITY: i64 = 110;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::SessionEvent;
    use crucible_core::test_support::mocks::MockEventEmitter;
    use std::sync::Arc;

    #[cfg(feature = "test-utils")]
    use crate::test_utils::{apply_eav_graph_schema, EAVGraphStore, SurrealClient};

    /// Helper to create a mock event emitter as a SharedEventBus
    fn create_mock_emitter() -> (Arc<MockEventEmitter<SessionEvent>>, SharedEventBus<SessionEvent>)
    {
        let mock = Arc::new(MockEventEmitter::new());
        let shared: SharedEventBus<SessionEvent> = mock.clone();
        (mock, shared)
    }

    #[cfg(feature = "test-utils")]
    async fn setup_handler() -> (
        TagHandler,
        Arc<MockEventEmitter<SessionEvent>>,
    ) {
        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = Arc::new(EAVGraphStore::new(client));
        let (mock, emitter) = create_mock_emitter();
        let handler = TagHandler::new(store, emitter);
        (handler, mock)
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_associate_tags_emits_tag_associated() {
        let (handler, mock) = setup_handler().await;

        // Associate tags with an entity
        let tags = vec!["rust".to_string(), "programming".to_string(), "tutorial".to_string()];
        handler.associate_tags("note_example_md", &tags).await;

        // Verify TagAssociated events were emitted
        let events = mock.emitted_events();
        assert_eq!(
            events.len(),
            3,
            "Expected 3 TagAssociated events, got {}",
            events.len()
        );

        // Verify each event
        let mut found_tags = vec![];
        for event in &events {
            match event {
                SessionEvent::TagAssociated { entity_id, tag } => {
                    assert_eq!(entity_id, "note_example_md");
                    found_tags.push(tag.clone());
                }
                _ => panic!("Expected TagAssociated event, got {:?}", event),
            }
        }

        // Verify all tags were emitted
        assert!(found_tags.contains(&"rust".to_string()));
        assert!(found_tags.contains(&"programming".to_string()));
        assert!(found_tags.contains(&"tutorial".to_string()));
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_associate_tags_empty_list() {
        let (handler, mock) = setup_handler().await;

        // Associate empty tag list
        handler.associate_tags("note_empty_md", &[]).await;

        // Verify no events were emitted
        let events = mock.emitted_events();
        assert!(events.is_empty(), "Expected no events for empty tag list");
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_associate_tags_single_tag() {
        let (handler, mock) = setup_handler().await;

        // Associate a single tag
        let tags = vec!["important".to_string()];
        handler.associate_tags("note_single_md", &tags).await;

        // Verify single TagAssociated event
        let events = mock.emitted_events();
        assert_eq!(events.len(), 1, "Expected 1 TagAssociated event");

        match &events[0] {
            SessionEvent::TagAssociated { entity_id, tag } => {
                assert_eq!(entity_id, "note_single_md");
                assert_eq!(tag, "important");
            }
            _ => panic!("Expected TagAssociated event"),
        }
    }

    #[test]
    fn test_handled_event_types() {
        let types = TagHandler::handled_event_types();
        assert!(types.contains(&"note_parsed"));
        assert!(types.contains(&"entity_deleted"));
    }

    #[test]
    fn test_priority_constant() {
        assert_eq!(TagHandler::PRIORITY, 110);
    }
}
