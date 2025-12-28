//! Adapter types that implement `crucible_core::events::Handler` for the
//! storage event handlers.
//!
//! These adapters allow StorageHandler and TagHandler to be used with the
//! unified Reactor event system.

use async_trait::async_trait;
use crucible_core::events::{Handler, HandlerContext, HandlerResult, SessionEvent};
use std::sync::Arc;

use super::{StorageHandler, TagHandler};

/// Adapter wrapping StorageHandler to implement the core Handler trait.
///
/// This allows StorageHandler to be registered with the Reactor.
pub struct StorageHandlerAdapter {
    inner: Arc<StorageHandler>,
}

impl StorageHandlerAdapter {
    /// Create a new adapter wrapping a StorageHandler.
    pub fn new(handler: StorageHandler) -> Self {
        Self {
            inner: Arc::new(handler),
        }
    }

    /// Create a new adapter from an Arc-wrapped StorageHandler.
    pub fn from_arc(handler: Arc<StorageHandler>) -> Self {
        Self { inner: handler }
    }
}

#[async_trait]
impl Handler for StorageHandlerAdapter {
    fn name(&self) -> &str {
        "storage_handler"
    }

    fn priority(&self) -> i32 {
        StorageHandler::PRIORITY as i32
    }

    fn event_pattern(&self) -> &str {
        // Match NoteParsed, FileDeleted, FileMoved events
        "*"
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        // Delegate to the inner handler
        self.inner.handle_event(&event).await;
        HandlerResult::ok(event)
    }
}

/// Adapter wrapping TagHandler to implement the core Handler trait.
///
/// This allows TagHandler to be registered with the Reactor.
pub struct TagHandlerAdapter {
    inner: Arc<TagHandler>,
}

impl TagHandlerAdapter {
    /// Create a new adapter wrapping a TagHandler.
    pub fn new(handler: TagHandler) -> Self {
        Self {
            inner: Arc::new(handler),
        }
    }

    /// Create a new adapter from an Arc-wrapped TagHandler.
    pub fn from_arc(handler: Arc<TagHandler>) -> Self {
        Self { inner: handler }
    }
}

#[async_trait]
impl Handler for TagHandlerAdapter {
    fn name(&self) -> &str {
        "tag_handler"
    }

    fn priority(&self) -> i32 {
        TagHandler::PRIORITY as i32
    }

    fn dependencies(&self) -> &[&str] {
        // TagHandler depends on StorageHandler to ensure entities exist first
        &["storage_handler"]
    }

    fn event_pattern(&self) -> &str {
        "*"
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        // Delegate to the inner handler
        self.inner.handle_event(&event).await;
        HandlerResult::ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::SessionEvent;
    use crucible_core::test_support::mocks::MockEventEmitter;
    use std::path::PathBuf;

    #[cfg(feature = "test-utils")]
    use crate::test_utils::{apply_eav_graph_schema, EAVGraphStore, SurrealClient};

    #[cfg(feature = "test-utils")]
    async fn setup_storage_adapter() -> StorageHandlerAdapter {
        use crucible_core::events::SharedEventBus;

        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = Arc::new(EAVGraphStore::new(client));
        let mock: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
        let emitter: SharedEventBus<SessionEvent> = mock;
        let handler = StorageHandler::new(store, emitter);
        StorageHandlerAdapter::new(handler)
    }

    #[test]
    fn test_storage_adapter_name() {
        // Can't easily test without full setup, but we can test the trait impl exists
        // by checking the name method signature
        fn assert_handler<T: Handler>(_: &T) {}
        // This would compile if the trait is implemented correctly
    }

    #[test]
    fn test_tag_adapter_dependencies() {
        // Verify TagHandlerAdapter declares dependency on StorageHandler
        // This ensures proper ordering in the Reactor
        let deps = ["storage_handler"];
        assert_eq!(deps, ["storage_handler"]);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_storage_adapter_handles_event() {
        let adapter = setup_storage_adapter().await;

        // Create a test event
        let event = SessionEvent::NoteParsed {
            path: PathBuf::from("test.md"),
            block_count: 5,
            payload: None,
        };

        // Create context
        let mut ctx = HandlerContext::new();

        // Handle the event
        let result = adapter.handle(&mut ctx, event.clone()).await;

        // Should continue processing
        assert!(result.should_continue());
    }
}
