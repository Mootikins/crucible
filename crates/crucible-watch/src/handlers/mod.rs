//! Event handlers for integrating with existing Crucible systems.

// TODO: Re-enable when crucible_mcp is available
mod indexing;
mod rune_reload;
mod obsidian_sync;
pub mod composite;

pub use indexing::*;
pub use rune_reload::*;
pub use obsidian_sync::*;
pub use composite::*;

use crate::{events::FileEvent, traits::EventHandler, error::Result};
use std::sync::Arc;

/// Registry for managing event handlers.
pub struct HandlerRegistry {
    handlers: Vec<Arc<dyn EventHandler>>,
}

impl std::fmt::Debug for HandlerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerRegistry")
            .field("handlers", &format!("{} registered handlers", self.handlers.len()))
            .finish()
    }
}

impl HandlerRegistry {
    /// Create a new handler registry.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Add a handler to the registry.
    pub fn register(&mut self, handler: Arc<dyn EventHandler>) {
        self.handlers.push(handler);
        // Sort by priority (highest first)
        self.handlers.sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// Remove a handler by name.
    pub fn unregister(&mut self, name: &str) -> bool {
        let initial_len = self.handlers.len();
        self.handlers.retain(|h| h.name() != name);
        initial_len != self.handlers.len()
    }

    /// Get all handlers that can process the given event.
    pub fn get_handlers_for_event(&self, event: &FileEvent) -> Vec<&Arc<dyn EventHandler>> {
        self.handlers
            .iter()
            .filter(|h| h.can_handle(event))
            .collect()
    }

    /// Get all registered handlers.
    pub fn handlers(&self) -> &[Arc<dyn EventHandler>] {
        &self.handlers
    }

    /// Get handler count.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a default set of handlers for a typical Crucible installation.
pub fn create_default_handlers() -> Result<HandlerRegistry> {
    let registry = HandlerRegistry::new();

    // Register default handlers
    // TODO: Re-enable when crucible_mcp is available
    // #[cfg(feature = "indexing")]
    // {
    //     registry.register(Arc::new(IndexingHandler::new()?));
    // }

    #[cfg(feature = "rune")]
    {
        registry.register(Arc::new(RuneReloadHandler::new()?));
    }

    #[cfg(feature = "obsidian")]
    {
        registry.register(Arc::new(ObsidianSyncHandler::new()?));
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crate::events::{FileEvent, FileEventKind};
    use std::path::PathBuf;

    struct MockHandler {
        name: &'static str,
        priority: u32,
    }

    #[async_trait]
    impl EventHandler for MockHandler {
        async fn handle(&self, _event: FileEvent) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> &'static str {
            self.name
        }

        fn priority(&self) -> u32 {
            self.priority
        }
    }

    #[tokio::test]
    async fn test_handler_registry() {
        let mut registry = HandlerRegistry::new();

        let handler1 = Arc::new(MockHandler { name: "test1", priority: 100 });
        let handler2 = Arc::new(MockHandler { name: "test2", priority: 200 });

        registry.register(handler1.clone());
        registry.register(handler2.clone());

        // Should be sorted by priority (handler2 first)
        assert_eq!(registry.handlers()[0].name(), "test2");
        assert_eq!(registry.handlers()[1].name(), "test1");

        // Test removal
        assert!(registry.unregister("test1"));
        assert!(!registry.unregister("nonexistent"));
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn test_event_filtering() {
        let mut registry = HandlerRegistry::new();

        let handler = Arc::new(MockHandler { name: "test", priority: 100 });
        registry.register(handler);

        let event = FileEvent::new(FileEventKind::Created, PathBuf::from("test.md"));
        let handlers = registry.get_handlers_for_event(&event);

        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].name(), "test");
    }
}