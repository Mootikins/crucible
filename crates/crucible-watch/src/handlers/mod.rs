//! Event handlers for integrating with existing Crucible systems.

// TODO: Re-enable when crucible_mcp is available
pub mod composite;
mod indexing;
mod obsidian_sync;
mod parser_handler;

pub use composite::*;
pub use indexing::*;
pub use obsidian_sync::*;
pub use parser_handler::ParserHandler;

use crate::{error::Result, events::FileEvent, traits::EventHandler};
use std::sync::Arc;

/// Registry for managing event handlers.
pub struct HandlerRegistry {
    handlers: Vec<Arc<dyn EventHandler>>,
}

impl std::fmt::Debug for HandlerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerRegistry")
            .field(
                "handlers",
                &format!("{} registered handlers", self.handlers.len()),
            )
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
        self.handlers
            .sort_by(|a, b| b.priority().cmp(&a.priority()));
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

    #[cfg(feature = "obsidian")]
    {
        registry.register(Arc::new(ObsidianSyncHandler::new()?));
    }

    Ok(registry)
}
