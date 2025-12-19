//! Backend implementations for file watching.

mod editor_backend;
mod factory;
mod notify_backend;
mod polling_backend;

pub use editor_backend::*;
pub use factory::*;
pub use notify_backend::*;
pub use polling_backend::*;

use crate::error::{Error, Result};
use crate::traits::{BackendCapabilities, FileWatcher};
use crate::WatchBackend;
use async_trait::async_trait;

/// Factory trait for creating file watcher backends.
#[async_trait]
pub trait WatcherFactory: Send + Sync {
    /// Create a new watcher instance.
    async fn create_watcher(&self) -> Result<Box<dyn FileWatcher>>;

    /// Get the backend type this factory creates.
    fn backend_type(&self) -> WatchBackend;

    /// Check if this backend is available on the current platform.
    fn is_available(&self) -> bool;

    /// Get the backend capabilities.
    fn capabilities(&self) -> BackendCapabilities;
}

/// Registry for managing watcher factories.
pub struct BackendRegistry {
    factories: std::collections::HashMap<WatchBackend, Box<dyn WatcherFactory>>,
}

impl std::fmt::Debug for BackendRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendRegistry")
            .field(
                "factories",
                &format!("{} registered backends", self.factories.len()),
            )
            .finish()
    }
}

impl BackendRegistry {
    /// Create a new backend registry.
    pub fn new() -> Self {
        let mut registry = Self {
            factories: std::collections::HashMap::new(),
        };

        // Register built-in backends
        registry.register_factory(Box::new(NotifyFactory::new()));
        registry.register_factory(Box::new(PollingFactory::new()));
        registry.register_factory(Box::new(EditorFactory::new()));

        registry
    }

    /// Register a new watcher factory.
    pub fn register_factory(&mut self, factory: Box<dyn WatcherFactory>) {
        let backend_type = factory.backend_type();
        self.factories.insert(backend_type, factory);
    }

    /// Create a watcher for the specified backend type.
    pub async fn create_watcher(&self, backend_type: WatchBackend) -> Result<Box<dyn FileWatcher>> {
        let factory = self
            .factories
            .get(&backend_type)
            .ok_or_else(|| Error::BackendUnavailable(format!("{:?}", backend_type)))?;

        if !factory.is_available() {
            return Err(Error::BackendUnavailable(format!(
                "{:?} not available on this platform",
                backend_type
            )));
        }

        factory.create_watcher().await
    }

    /// Get all available backends.
    pub fn available_backends(&self) -> Vec<WatchBackend> {
        self.factories
            .iter()
            .filter(|(_, factory)| factory.is_available())
            .map(|(backend_type, _)| *backend_type)
            .collect()
    }

    /// Get capabilities for a backend type.
    pub fn get_capabilities(&self, backend_type: WatchBackend) -> Option<BackendCapabilities> {
        self.factories.get(&backend_type).map(|f| f.capabilities())
    }

    /// Check if a backend is available.
    pub fn is_available(&self, backend_type: WatchBackend) -> bool {
        self.factories
            .get(&backend_type)
            .map(|f| f.is_available())
            .unwrap_or(false)
    }

    /// Get the default backend for this platform.
    pub fn default_backend(&self) -> Option<WatchBackend> {
        // Priority order: Notify -> Polling -> Editor
        let priorities = [
            WatchBackend::Notify,
            WatchBackend::Polling,
            WatchBackend::Editor,
        ];

        priorities.into_iter().find(|&backend| self.is_available(backend))
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}
