//! Event system initialization.
//!
//! This module contains the `initialize_event_system` function that wires together
//! all event-driven components.
//!
//! # Handler Wiring Architecture
//!
//! The EventBus uses sync handler closures, but the actual handlers (StorageHandler,
//! TagHandler, EmbeddingHandler) are async. We bridge this gap by:
//!
//! 1. Extracting event data from the serialized payload in the closure
//! 2. Using `tokio::spawn` to invoke async handler methods
//! 3. Keeping handler Arc references alive in the EventSystemHandle
//!
//! This "fire-and-forget" pattern allows the sync EventBus pipeline to continue
//! while handlers process events asynchronously. Handlers emit follow-up events
//! through the shared EventEmitter.

use anyhow::{Context, Result};
use crucible_core::events::SessionEvent;
use crucible_enrichment::EmbeddingHandler;
use crucible_rune::{EventBus, EventType, Handler};
use crucible_surrealdb::adapters;
use crucible_surrealdb::event_handlers::{StorageHandler, TagHandler};
use crucible_watch::{WatchManager, WatchManagerConfig};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::CliConfig;
use crate::factories;

use super::handle::EventSystemHandle;

/// Initialize the complete event system.
///
/// This function:
/// 1. Creates the EventBus for event dispatch
/// 2. Initializes database storage with event emission
/// 3. Registers StorageHandler (priority 100)
/// 4. Registers TagHandler (priority 110)
/// 5. Initializes embedding provider and EmbeddingHandler (priority 200)
/// 6. Loads and registers Rune handlers from kiln
/// 7. Creates and starts WatchManager for file system events
///
/// # Arguments
///
/// * `config` - CLI configuration
///
/// # Returns
///
/// An `EventSystemHandle` that provides access to the event bus and watch manager,
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
    info!("Initializing event system...");

    // Collect handler references to keep them alive
    let mut handlers: Vec<Arc<dyn Any + Send + Sync>> = Vec::new();

    // 7.2.1: Create EventBus
    debug!("Creating EventBus");
    let bus = Arc::new(RwLock::new(EventBus::new()));

    // 7.2.2: Initialize database
    debug!("Initializing database storage");
    let storage_client = factories::create_surrealdb_storage(config).await?;
    factories::initialize_surrealdb_schema(&storage_client).await?;

    // Create a SessionEvent emitter adapter for the handlers
    let handler_emitter = create_bus_emitter(bus.clone());

    // 7.2.3: Register StorageHandler
    debug!("Registering StorageHandler (priority 100)");
    let storage_handler =
        adapters::create_storage_handler(storage_client.clone(), handler_emitter.clone());
    let storage_handler_ref = register_storage_handler(&bus, storage_handler).await;
    handlers.push(storage_handler_ref);

    // 7.2.4: Register TagHandler
    debug!("Registering TagHandler (priority 110)");
    let tag_handler = adapters::create_tag_handler(storage_client.clone(), handler_emitter.clone());
    let tag_handler_ref = register_tag_handler(&bus, tag_handler).await;
    handlers.push(tag_handler_ref);

    // 7.2.5 & 7.2.6: Initialize embedding provider and register EmbeddingHandler
    debug!("Initializing embedding provider");
    match factories::get_or_create_embedding_provider(config).await {
        Ok(embedding_provider) => {
            debug!("Registering EmbeddingHandler (priority 200)");
            let enrichment_service =
                crucible_enrichment::create_default_enrichment_service(Some(embedding_provider))?;
            let embedding_handler = EmbeddingHandler::new(enrichment_service);
            let embedding_handler_ref = register_embedding_handler(&bus, embedding_handler).await;
            handlers.push(embedding_handler_ref);
        }
        Err(e) => {
            warn!(
                "Failed to initialize embedding provider, embeddings disabled: {}",
                e
            );
        }
    }

    // 7.2.7: Load Rune handlers from kiln
    debug!("Loading Rune handlers from kiln");
    load_rune_handlers(&bus, &config.kiln_path).await;

    // 7.2.8 & 7.2.9: Initialize and start WatchManager
    debug!("Initializing WatchManager");
    let watch_config = WatchManagerConfig::default();
    let watch_manager = WatchManager::with_emitter(watch_config, handler_emitter)
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
        "Event system initialized with {} handlers",
        bus.read().await.list_handlers().count()
    );

    Ok(EventSystemHandle::with_handlers(
        bus,
        watch_manager,
        storage_client,
        handlers,
    ))
}

/// Create a SessionEvent emitter that dispatches to the EventBus.
///
/// This bridges the core `EventEmitter` trait to the Rune `EventBus`.
fn create_bus_emitter(
    bus: Arc<RwLock<EventBus>>,
) -> Arc<dyn crucible_core::events::EventEmitter<Event = SessionEvent>> {
    Arc::new(EventBusEmitter { bus })
}

/// Adapter that implements `EventEmitter<SessionEvent>` by dispatching to the EventBus.
struct EventBusEmitter {
    bus: Arc<RwLock<EventBus>>,
}

#[async_trait::async_trait]
impl crucible_core::events::EventEmitter for EventBusEmitter {
    type Event = SessionEvent;

    async fn emit(
        &self,
        event: Self::Event,
    ) -> crucible_core::events::EmitResult<crucible_core::events::EmitOutcome<Self::Event>> {
        let bus = self.bus.read().await;
        let (result, _ctx, errors) = bus.emit_session(event);

        // Convert handler errors to EmitOutcome errors
        let error_infos: Vec<crucible_core::events::HandlerErrorInfo> = errors
            .into_iter()
            .map(|e| crucible_core::events::HandlerErrorInfo::new(&e.handler_name, &e.message))
            .collect();

        Ok(crucible_core::events::EmitOutcome {
            event: result,
            cancelled: false, // EventBus tracks this differently
            errors: error_infos,
        })
    }

    async fn emit_recursive(
        &self,
        event: Self::Event,
    ) -> crucible_core::events::EmitResult<Vec<crucible_core::events::EmitOutcome<Self::Event>>>
    {
        // Recursive emission in this architecture works differently:
        // - Handlers are invoked via tokio::spawn from sync closures
        // - Each spawned handler may call self.emitter.emit() for follow-up events
        // - Those emissions are independent emit() calls, not collected here
        //
        // This is intentional: the "fire-and-forget" async pattern means secondary
        // events are emitted asynchronously as handlers complete their work.
        // The EventBus processes each event independently.
        let outcome = self.emit(event).await?;
        Ok(vec![outcome])
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Register the StorageHandler with the EventBus.
///
/// The handler closure extracts event data from the payload and spawns
/// an async task to invoke the actual handler methods.
///
/// Returns the handler reference to keep it alive in EventSystemHandle.
async fn register_storage_handler(
    bus: &Arc<RwLock<EventBus>>,
    handler: StorageHandler,
) -> Arc<dyn Any + Send + Sync> {
    let handler = Arc::new(handler);

    // Register for NoteParsed events
    let handler_clone = handler.clone();
    let bus_handler = Handler::new(
        "storage_handler_note_parsed",
        EventType::NoteParsed,
        "*",
        move |_ctx, event| {
            // Extract path and block_count from the serialized payload
            if let Some(path_str) = event.payload.get("path").and_then(|v| v.as_str()) {
                let path = PathBuf::from(path_str);
                let block_count = event
                    .payload
                    .get("block_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;

                // Spawn async handler invocation
                let h = handler_clone.clone();
                tokio::spawn(async move {
                    h.handle_note_parsed(&path, block_count).await;
                });
            } else {
                error!("NoteParsed event missing path in payload");
            }
            Ok(event)
        },
    )
    .with_priority(StorageHandler::PRIORITY);

    bus.write().await.register(bus_handler);

    // Register for FileDeleted events
    let handler_clone = handler.clone();
    let bus_handler = Handler::new(
        "storage_handler_file_deleted",
        EventType::FileDeleted,
        "*",
        move |_ctx, event| {
            // Extract path from the serialized payload
            if let Some(path_str) = event.payload.get("path").and_then(|v| v.as_str()) {
                let path = PathBuf::from(path_str);

                // Spawn async handler invocation
                let h = handler_clone.clone();
                tokio::spawn(async move {
                    h.handle_file_deleted(&path).await;
                });
            } else {
                error!("FileDeleted event missing path in payload");
            }
            Ok(event)
        },
    )
    .with_priority(StorageHandler::PRIORITY);

    bus.write().await.register(bus_handler);

    debug!("Registered StorageHandler for note_parsed and file_deleted events");

    // Return the handler reference to keep it alive
    handler as Arc<dyn Any + Send + Sync>
}

/// Register the TagHandler with the EventBus.
///
/// The handler closure extracts event data from the payload and spawns
/// an async task to invoke the actual handler methods.
///
/// Returns the handler reference to keep it alive in EventSystemHandle.
async fn register_tag_handler(
    bus: &Arc<RwLock<EventBus>>,
    handler: TagHandler,
) -> Arc<dyn Any + Send + Sync> {
    let handler = Arc::new(handler);

    // Register for NoteParsed events
    let handler_clone = handler.clone();
    let bus_handler = Handler::new(
        "tag_handler_note_parsed",
        EventType::NoteParsed,
        "*",
        move |_ctx, event| {
            // Extract path and block_count from the serialized payload
            if let Some(path_str) = event.payload.get("path").and_then(|v| v.as_str()) {
                let path = PathBuf::from(path_str);
                let block_count = event
                    .payload
                    .get("block_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;

                // Spawn async handler invocation
                let h = handler_clone.clone();
                tokio::spawn(async move {
                    h.handle_note_parsed(&path, block_count).await;
                });
            } else {
                error!("NoteParsed event missing path in payload for TagHandler");
            }
            Ok(event)
        },
    )
    .with_priority(TagHandler::PRIORITY);

    bus.write().await.register(bus_handler);

    debug!("Registered TagHandler for note_parsed events");

    // Return the handler reference to keep it alive
    handler as Arc<dyn Any + Send + Sync>
}

/// Register the EmbeddingHandler with the EventBus.
///
/// The handler closure extracts event data from the payload and spawns
/// an async task to invoke the actual handler methods.
///
/// Note: The EmbeddingHandler requires a full ParsedNote for enrichment,
/// which is not available in the NoteParsed event payload. The handler
/// logs the event and requires coordination with parser/storage for
/// full enrichment.
///
/// Returns the handler reference to keep it alive in EventSystemHandle.
async fn register_embedding_handler(
    bus: &Arc<RwLock<EventBus>>,
    handler: EmbeddingHandler,
) -> Arc<dyn Any + Send + Sync> {
    let handler = Arc::new(handler);

    // Register for NoteParsed events (to trigger embedding generation)
    let handler_clone = handler.clone();
    let bus_handler = Handler::new(
        "embedding_handler_note_parsed",
        EventType::NoteParsed,
        "*",
        move |_ctx, event| {
            // Reconstruct SessionEvent from payload for handler
            if let Some(path_str) = event.payload.get("path").and_then(|v| v.as_str()) {
                let path = PathBuf::from(path_str);
                let block_count = event
                    .payload
                    .get("block_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;

                // Spawn async handler invocation
                let h = handler_clone.clone();
                tokio::spawn(async move {
                    let session_event = SessionEvent::NoteParsed {
                        path,
                        block_count,
                        payload: None, // Full payload requires re-parsing
                    };
                    h.handle_event(&session_event).await;
                });
            } else {
                error!("NoteParsed event missing path in payload for EmbeddingHandler");
            }
            Ok(event)
        },
    )
    .with_priority(EmbeddingHandler::PRIORITY);

    bus.write().await.register(bus_handler);

    debug!("Registered EmbeddingHandler for note_parsed events");

    // Return the handler reference to keep it alive
    handler as Arc<dyn Any + Send + Sync>
}

/// Load Rune handlers from the kiln's `.crucible/handlers/` directory.
async fn load_rune_handlers(bus: &Arc<RwLock<EventBus>>, kiln_path: &Path) {
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
        if path.extension().map_or(false, |ext| ext == "rn") {
            match load_single_rune_handler(&path).await {
                Ok(handler) => {
                    bus.write().await.register(handler);
                    loaded_count += 1;
                    debug!("Loaded Rune handler from {}", path.display());
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
async fn load_single_rune_handler(path: &Path) -> Result<Handler> {
    let _content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read handler file: {}", path.display()))?;

    let handler_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // For now, create a placeholder handler that logs execution
    // Full Rune compilation would require the Rune VM setup
    let handler = Handler::new(
        format!("rune_{}", handler_name),
        EventType::Custom, // Rune handlers typically handle custom events
        "*",
        move |_ctx, event| {
            tracing::debug!("Rune handler '{}' received event", handler_name);
            Ok(event)
        },
    )
    .with_priority(500); // Rune handlers run after built-in handlers

    Ok(handler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_emitter_is_available() {
        let bus = Arc::new(RwLock::new(EventBus::new()));
        let emitter = create_bus_emitter(bus);
        assert!(emitter.is_available());
    }
}
