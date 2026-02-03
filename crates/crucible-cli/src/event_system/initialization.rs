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
//! # Phase 4 Cleanup
//!
//! The EAV graph handlers (StorageHandler, TagHandler) have been removed.
//! Storage is now handled by NoteStore. Event handlers will be updated to use
//! the new storage system in a future update.

use anyhow::{Context, Result};
use crucible_core::events::{Reactor, ReactorEventEmitter, SessionEvent};
use crucible_enrichment::{EmbeddingHandler, EmbeddingHandlerAdapter};
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

    // Initialize database (SurrealDB only)
    #[cfg(feature = "storage-surrealdb")]
    let storage_client = {
        debug!("Initializing database storage");
        let client = factories::create_surrealdb_storage(config).await?;
        factories::initialize_surrealdb_schema(&client).await?;
        client
    };

    // Create shared reactor for the ReactorEventEmitter
    // We need to wrap it now so handlers can get a reference to the emitter
    let reactor_arc = Arc::new(RwLock::new(Reactor::new()));

    // NOTE: Phase 4 cleanup - StorageHandler and TagHandler have been removed.
    // Storage is now handled by NoteStore. The event system will be updated
    // to use the new storage system in a future update.

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

    // Load Lua handlers from kiln
    debug!("Loading Lua handlers from kiln");
    load_lua_handlers(&mut reactor, &config.kiln_path);

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

    #[cfg(feature = "storage-surrealdb")]
    {
        Ok(EventSystemHandle::new(
            reactor_arc,
            watch_manager,
            storage_client,
        ))
    }
    #[cfg(not(feature = "storage-surrealdb"))]
    {
        Ok(EventSystemHandle::new_without_storage(
            reactor_arc,
            watch_manager,
        ))
    }
}

fn load_lua_handlers(reactor: &mut Reactor, kiln_path: &Path) {
    use crucible_core::discovery::DiscoveryPaths;
    use crucible_lua::LuaScriptHandlerRegistry;

    let paths = DiscoveryPaths::new("handlers", Some(kiln_path));
    let existing = paths.existing_paths();
    if existing.is_empty() {
        debug!("No handler directories found, skipping Lua handlers");
        return;
    }

    let registry = match LuaScriptHandlerRegistry::discover(&existing) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to discover Lua handlers: {}", e);
            return;
        }
    };

    let handlers = match registry.to_core_handlers() {
        Ok(h) => h,
        Err(e) => {
            warn!("Failed to create core handlers from Lua: {}", e);
            return;
        }
    };

    let mut loaded_count = 0;
    for handler in handlers {
        let name = handler.name().to_string();
        if let Err(e) = reactor.register(handler) {
            warn!("Failed to register Lua handler {}: {}", name, e);
        } else {
            loaded_count += 1;
            debug!("Loaded Lua handler: {}", name);
        }
    }

    if loaded_count > 0 {
        info!("Loaded {} Lua handlers", loaded_count);
    }
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
