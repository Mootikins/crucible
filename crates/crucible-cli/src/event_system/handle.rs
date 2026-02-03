//! Event system handle for lifecycle management.
//!
//! The `EventSystemHandle` provides:
//! - Access to the shared Reactor for handler registration
//! - Access to WatchManager for adding/removing watches
//! - Graceful shutdown coordination

use anyhow::Result;
use crucible_core::events::Reactor;
#[cfg(feature = "storage-surrealdb")]
use crucible_surrealdb::adapters::SurrealClientHandle;
use crucible_watch::WatchManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Handle for the initialized event system.
///
/// Provides access to the reactor, watch manager, and shutdown coordination.
/// Drop this handle to trigger graceful shutdown of all components.
pub struct EventSystemHandle {
    /// The shared reactor for event dispatch
    pub reactor: Arc<RwLock<Reactor>>,
    /// The file system watch manager
    pub watch_manager: Arc<RwLock<WatchManager>>,
    /// Storage handle for database access
    #[cfg(feature = "storage-surrealdb")]
    storage_client: SurrealClientHandle,
}

impl EventSystemHandle {
    /// Create a new event system handle.
    #[cfg(feature = "storage-surrealdb")]
    pub(crate) fn new(
        reactor: Arc<RwLock<Reactor>>,
        watch_manager: Arc<RwLock<WatchManager>>,
        storage_client: SurrealClientHandle,
    ) -> Self {
        Self {
            reactor,
            watch_manager,
            storage_client,
        }
    }

    /// Create a new event system handle without storage (non-SurrealDB modes).
    #[cfg(not(feature = "storage-surrealdb"))]
    pub(crate) fn new_without_storage(
        reactor: Arc<RwLock<Reactor>>,
        watch_manager: Arc<RwLock<WatchManager>>,
    ) -> Self {
        Self {
            reactor,
            watch_manager,
        }
    }

    /// Get access to the reactor for handler registration.
    pub fn reactor(&self) -> &Arc<RwLock<Reactor>> {
        &self.reactor
    }

    /// Get access to the watch manager.
    pub fn watch_manager(&self) -> &Arc<RwLock<WatchManager>> {
        &self.watch_manager
    }

    /// Get access to the storage client.
    #[cfg(feature = "storage-surrealdb")]
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

        // Reactor doesn't need explicit shutdown - just drop the reference
        // All handlers will be dropped when reactor is dropped

        info!("Event system shutdown complete");
        Ok(())
    }

    /// Get the number of registered handlers.
    pub async fn handler_count(&self) -> usize {
        let reactor = self.reactor.read().await;
        reactor.handler_count()
    }

    /// Check if the watch manager is running.
    pub async fn is_watching(&self) -> bool {
        let watch = self.watch_manager.read().await;
        watch.get_status().await.is_running
    }
}
