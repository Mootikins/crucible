//! Event handlers for integrating with existing Crucible systems.

pub mod composite;
mod external_change;
mod indexing;

pub use composite::{CompositeHandler, CoordinationStrategy, HandlerState};
pub use external_change::ExternalChangeHandler;
pub use indexing::IndexingHandler;

use crate::watch::{error::Result, events::FileEvent, traits::EventHandler};
use crucible_core::events::{EventEmitter, SessionEvent};
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
///
/// `emitter` is where the handlers publish what they saw; it must be the same
/// emitter the manager was built with, or the events go to a bus nobody reads.
/// Registration here is unconditional. It used to sit behind
/// `#[cfg(feature = "indexing")]`, which named a feature that exists in no
/// `Cargo.toml` in the workspace — so it was not a switched-off option, it was
/// dead code, and the watcher shipped for months delivering events to an empty
/// registry. Indexing an open kiln is the daemon's job, not a build-time
/// choice.
pub fn create_default_handlers(
    emitter: Arc<dyn EventEmitter<Event = SessionEvent>>,
) -> Result<HandlerRegistry> {
    let mut registry = HandlerRegistry::new();

    registry.register(Arc::new(IndexingHandler::with_emitter(emitter)?));

    Ok(registry)
}
