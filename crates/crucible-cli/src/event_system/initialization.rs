//! Event system initialization.
//!
//! This module contains the `initialize_event_system` function that wires together
//! all event-driven components using the unified Reactor pattern.
//!
//! # Architecture
//!
//! The event system uses the Reactor pattern from `crucible_core::events`:
//! - Handlers implement the `Handler` trait (async, with dependencies and priorities)
//! - The `Reactor` dispatches events through handlers in dependency+priority order
//! - `ReactorEventEmitter` provides the `EventEmitter` trait for integration
//!
//! # Handler Chain
//!
//! Handlers are registered with explicit dependencies:
//!
//! ```text
//! StorageHandler (priority 100) → TagHandler (priority 110) → EmbeddingHandler (priority 200)
//!                                      ↑ depends on storage_handler ↑ depends on both
//! ```

use anyhow::{Context, Result};
use crucible_core::events::{Reactor, ReactorEventEmitter, SessionEvent};
use crucible_enrichment::{EmbeddingHandler, EmbeddingHandlerAdapter};
use crucible_surrealdb::adapters;
use crucible_surrealdb::event_handlers::{
    StorageHandler, StorageHandlerAdapter, TagHandler, TagHandlerAdapter,
};
use crucible_watch::{WatchManager, WatchManagerConfig};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::CliConfig;
use crate::factories;

use super::handle::EventSystemHandle;

/// Initialize the complete event system using the Reactor pattern.
///
/// This function:
/// 1. Creates the Reactor for unified event dispatch
/// 2. Initializes database storage
/// 3. Registers StorageHandlerAdapter (priority 100)
/// 4. Registers TagHandlerAdapter (priority 110, depends on storage)
/// 5. Initializes embedding provider and registers EmbeddingHandlerAdapter (priority 200)
/// 6. Loads Rune handlers from kiln (using RuneHandler adapter)
/// 7. Creates and starts WatchManager for file system events
///
/// # Arguments
///
/// * `config` - CLI configuration
///
/// # Returns
///
/// An `EventSystemHandle` that provides access to the reactor and watch manager,
/// and coordinates graceful shutdown.
///
/// # Example
///
/// ```rust,ignore
/// let handle = initialize_event_system(&config).await?;
///
/// // Event system is now running - file changes trigger the cascade:
/// // FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingGenerated
///
/// // Shutdown when done
/// handle.shutdown().await?;
/// ```
pub async fn initialize_event_system(config: &CliConfig) -> Result<EventSystemHandle> {
    info!("Initializing event system with Reactor...");

    // Create Reactor for unified event dispatch
    debug!("Creating Reactor");
    let mut reactor = Reactor::new();

    // Initialize database
    debug!("Initializing database storage");
    let storage_client = factories::create_surrealdb_storage(config).await?;
    factories::initialize_surrealdb_schema(&storage_client).await?;

    // Create shared reactor for the ReactorEventEmitter
    // We need to wrap it now so handlers can get a reference to the emitter
    let reactor_arc = Arc::new(RwLock::new(Reactor::new()));
    let emitter = ReactorEventEmitter::new(reactor_arc.clone());
    let shared_emitter: Arc<dyn crucible_core::events::EventEmitter<Event = SessionEvent>> =
        Arc::new(emitter);

    // Register StorageHandler
    debug!("Registering StorageHandler (priority 100)");
    let storage_handler = adapters::create_storage_handler(storage_client.clone(), shared_emitter.clone());
    reactor
        .register(Box::new(StorageHandlerAdapter::new(storage_handler)))
        .context("Failed to register StorageHandler")?;

    // Register TagHandler (depends on storage_handler)
    debug!("Registering TagHandler (priority 110)");
    let tag_handler = adapters::create_tag_handler(storage_client.clone(), shared_emitter.clone());
    reactor
        .register(Box::new(TagHandlerAdapter::new(tag_handler)))
        .context("Failed to register TagHandler")?;

    // Initialize embedding provider and register EmbeddingHandler
    debug!("Initializing embedding provider");
    match factories::get_or_create_embedding_provider(config).await {
        Ok(embedding_provider) => {
            debug!("Registering EmbeddingHandler (priority 200)");
            let enrichment_service =
                crucible_enrichment::create_default_enrichment_service(Some(embedding_provider))?;
            let embedding_handler = EmbeddingHandler::new(enrichment_service);
            reactor
                .register(Box::new(EmbeddingHandlerAdapter::new(embedding_handler)))
                .context("Failed to register EmbeddingHandler")?;
        }
        Err(e) => {
            warn!(
                "Failed to initialize embedding provider, embeddings disabled: {}",
                e
            );
        }
    }

    // Load Rune handlers from kiln
    debug!("Loading Rune handlers from kiln");
    load_rune_handlers(&mut reactor, &config.kiln_path).await;

    // Get final handler count
    let handler_count = reactor.handler_count();

    // Now move the fully-configured reactor into the Arc
    *reactor_arc.write().await = reactor;

    // Create WatchManager with ReactorEventEmitter
    debug!("Initializing WatchManager");
    let watch_config = WatchManagerConfig::default();
    let watch_emitter = ReactorEventEmitter::new(reactor_arc.clone());
    let watch_manager = WatchManager::with_emitter(watch_config, Arc::new(watch_emitter))
        .await
        .context("Failed to create WatchManager")?;

    let watch_manager = Arc::new(RwLock::new(watch_manager));

    // Start the watch manager
    {
        let mut watch = watch_manager.write().await;
        watch
            .start()
            .await
            .context("Failed to start WatchManager")?;
    }

    info!(
        "Event system initialized with {} handlers (Reactor pattern)",
        handler_count
    );

    Ok(EventSystemHandle::new(
        reactor_arc,
        watch_manager,
        storage_client,
    ))
}

/// Load Rune handlers from the kiln's `.crucible/handlers/` directory.
///
/// Uses the `RuneHandler` adapter from `crucible_rune::core_handler` to
/// integrate Rune scripts with the Reactor.
async fn load_rune_handlers(reactor: &mut Reactor, kiln_path: &Path) {
    let handlers_dir = kiln_path.join(".crucible").join("handlers");

    if !handlers_dir.exists() {
        debug!(
            "No handlers directory at {}, skipping Rune handlers",
            handlers_dir.display()
        );
        return;
    }

    // Scan for .rn files
    let entries = match std::fs::read_dir(&handlers_dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!("Failed to read handlers directory: {}", e);
            return;
        }
    };

    let mut loaded_count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rn") {
            match load_single_rune_handler(&path).await {
                Ok(handler) => {
                    if let Err(e) = reactor.register(handler) {
                        warn!("Failed to register Rune handler {}: {}", path.display(), e);
                    } else {
                        loaded_count += 1;
                        debug!("Loaded Rune handler from {}", path.display());
                    }
                }
                Err(e) => {
                    warn!("Failed to load Rune handler from {}: {}", path.display(), e);
                }
            }
        }
    }

    if loaded_count > 0 {
        info!(
            "Loaded {} Rune handlers from {}",
            loaded_count,
            handlers_dir.display()
        );
    }
}

/// Load a single Rune handler from a file.
///
/// Uses `crucible_rune::core_handler::RuneHandler` to create a Handler
/// that can be registered with the Reactor.
async fn load_single_rune_handler(
    path: &Path,
) -> Result<Box<dyn crucible_core::events::Handler>> {
    use crucible_rune::core_handler::{RuneHandler, RuneHandlerMeta};
    use crucible_rune::RuneExecutor;

    // Create executor for this handler
    let executor = Arc::new(
        RuneExecutor::new()
            .with_context(|| "Failed to create Rune executor")?
    );

    // Create handler metadata
    // Priority 500+ for user scripts (after built-in handlers)
    let meta = RuneHandlerMeta::new(path.to_path_buf(), "handle")
        .with_priority(500)
        .with_event_pattern("*");

    // Create RuneHandler - this will compile the script
    let handler = RuneHandler::new(meta, executor)
        .with_context(|| format!("Failed to create Rune handler from {}", path.display()))?;

    Ok(Box::new(handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::EventEmitter;

    #[tokio::test]
    async fn test_reactor_event_emitter_is_available() {
        let reactor = Arc::new(RwLock::new(Reactor::new()));
        let emitter = ReactorEventEmitter::new(reactor);
        assert!(emitter.is_available());
    }
}
