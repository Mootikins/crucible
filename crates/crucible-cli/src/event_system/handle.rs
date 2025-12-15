//! Event system handle for lifecycle management.
//!
//! The `EventSystemHandle` provides:
//! - Access to the shared EventBus for handler registration
//! - Access to WatchManager for adding/removing watches
//! - Graceful shutdown coordination

use anyhow::Result;
use crucible_rune::EventBus;
use crucible_surrealdb::adapters::SurrealClientHandle;
use crucible_watch::WatchManager;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Handle for the initialized event system.
///
/// Provides access to the event bus, watch manager, and shutdown coordination.
/// Drop this handle to trigger graceful shutdown of all components.
pub struct EventSystemHandle {
    /// The shared event bus for event dispatch
    pub bus: Arc<RwLock<EventBus>>,
    /// The file system watch manager
    pub watch_manager: Arc<RwLock<WatchManager>>,
    /// Storage handle for database access
    storage_client: SurrealClientHandle,
    /// Handler references kept alive for the lifetime of the event system
    /// Without this, handlers would be dropped after registration
    _handlers: Vec<Arc<dyn Any + Send + Sync>>,
}

impl EventSystemHandle {
    /// Create a new event system handle.
    pub(crate) fn new(
        bus: Arc<RwLock<EventBus>>,
        watch_manager: Arc<RwLock<WatchManager>>,
        storage_client: SurrealClientHandle,
    ) -> Self {
        Self {
            bus,
            watch_manager,
            storage_client,
            _handlers: Vec::new(),
        }
    }

    /// Create a new event system handle with handler references.
    pub(crate) fn with_handlers(
        bus: Arc<RwLock<EventBus>>,
        watch_manager: Arc<RwLock<WatchManager>>,
        storage_client: SurrealClientHandle,
        handlers: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Self {
        Self {
            bus,
            watch_manager,
            storage_client,
            _handlers: handlers,
        }
    }

    /// Get access to the event bus for handler registration.
    pub fn bus(&self) -> &Arc<RwLock<EventBus>> {
        &self.bus
    }

    /// Get access to the watch manager.
    pub fn watch_manager(&self) -> &Arc<RwLock<WatchManager>> {
        &self.watch_manager
    }

    /// Get access to the storage client.
    pub fn storage_client(&self) -> &SurrealClientHandle {
        &self.storage_client
    }

    /// Gracefully shutdown the event system.
    ///
    /// This will:
    /// 1. Stop the watch manager
    /// 2. Wait for pending events to drain
    /// 3. Clean up resources
    pub async fn shutdown(self) -> Result<()> {
        info!("Shutting down event system...");

        // Stop watch manager first to prevent new events
        {
            let mut watch = self.watch_manager.write().await;
            watch.shutdown().await?;
        }

        // Event bus doesn't need explicit shutdown - just drop the reference
        // All handlers will be dropped when bus is dropped

        info!("Event system shutdown complete");
        Ok(())
    }

    /// Get the number of registered handlers.
    pub async fn handler_count(&self) -> usize {
        let bus = self.bus.read().await;
        bus.list_handlers().count()
    }

    /// Check if the watch manager is running.
    pub async fn is_watching(&self) -> bool {
        let watch = self.watch_manager.read().await;
        watch.get_status().await.is_running
    }
}
